//! useSelectionRect — 选区状态机 composable。
//! 职责：选区交互状态（idle/selecting/selected/moving/resizing/drawing）、
//! 选区矩形与起止点、8 向缩放手柄、选区移动、智能窗口探测与高亮，
//! 以及选区相关 DOM（镂空/窗口高亮/浮动工具栏）样式计算。
//! 跨域依赖（场景坐标换算、当前工具、捕获原点）通过 deps 注入。

import {computed, reactive, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {IPC_COMMANDS} from '../../../services/ipc.js'

export function useSelectionRect(deps) {
    const {
        toScenePoint,
        dpr,
        regionConfirmAnchor,
        currentTool,
        longshotResultActive,
        captureOriginX,
        captureOriginY,
        onMouseDown
    } = deps

    const state = ref('idle') // idle, selecting, selected, moving, resizing, drawing

    const windowHighlightRef = ref(null)
    const cutoutRef = ref(null)
    const floatingToolbarRef = ref(null)

    const isDrawing = ref(false)

    const rect = reactive({x: 0, y: 0, width: 0, height: 0})
    const startRect = reactive({x: 0, y: 0, width: 0, height: 0})
    const startPoint = reactive({x: 0, y: 0})
    const drawStart = reactive({x: 0, y: 0})
    let resizeHandleType = ''

    const windows = ref([])
    const highlightedWindow = ref(null)
    const windowCoordScale = ref(1)

    const hasSelection = computed(() => rect.width > 0 && rect.height > 0)

    const selectionInfoText = computed(() => {
      const width = Math.max(0, Math.round(rect.width * dpr))
      const height = Math.max(0, Math.round(rect.height * dpr))
      if (width <= 0 || height <= 0) {
        return `${width} × ${height} px`
      }
      return `${width} × ${height} px (${getAspectRatioText(width, height)})`
    })

    const recordingConfirmStyle = computed(() => {
      const margin = 8
      const panelWidth = 168
      const panelHeight = 32
      const defaultX = regionConfirmAnchor.ready
          ? Math.round(regionConfirmAnchor.x)
          : Math.round(rect.x + rect.width - panelWidth)
      const defaultY = regionConfirmAnchor.ready
          ? Math.round(regionConfirmAnchor.y)
          : (rect.y > panelHeight + 12 ? Math.round(rect.y - panelHeight - margin) : Math.round(rect.y + rect.height + margin))
      const x = Math.max(margin, Math.min(window.innerWidth - panelWidth - margin, defaultX))
      const y = defaultY
      const top = Math.max(margin, Math.min(window.innerHeight - panelHeight - margin, y))
      return {
        left: `${x}px`,
        top: `${top}px`
      }
    })

    const sizeInfoStyle = computed(() => {
      if (rect.y < 24) {
        return { top: '4px', left: '4px' }
      }
      return { top: '-24px', left: '0' }
    })

    function getAspectRatioText(width, height) {
      const divisor = getGreatestCommonDivisor(width, height)
      const ratioWidth = Math.round(width / divisor)
      const ratioHeight = Math.round(height / divisor)
      return `${ratioWidth}:${ratioHeight}`
    }

    function getGreatestCommonDivisor(a, b) {
      let x = Math.abs(Math.trunc(a))
      let y = Math.abs(Math.trunc(b))
      while (y !== 0) {
        const temp = y
        y = x % y
        x = temp
      }
      return x || 1
    }

    // 样式计算
    const editorCursorClass = computed(() => {
      if (longshotResultActive.value && currentTool.value === 'select') return 'grab'
      if (state.value === 'idle' || state.value === 'selecting') return 'crosshair'
      if (currentTool.value !== 'select') return 'crosshair'
      return 'default'
    })

    const cutoutStyle = computed(() => {
      return {
        left: `${rect.x}px`,
        top: `${rect.y}px`,
        width: `${rect.width}px`,
        height: `${rect.height}px`
      }
    })

    const windowHighlightStyle = computed(() => {
      if (!highlightedWindow.value) return {}
      return {
        left: `${highlightedWindow.value.x}px`,
        top: `${highlightedWindow.value.y}px`,
        width: `${highlightedWindow.value.width}px`,
        height: `${highlightedWindow.value.height}px`
      }
    })

    const toolbarStyle = computed(() => {
      let top = rect.y + rect.height + 10
      let right = window.innerWidth - (rect.x + rect.width)


      if (top + 100 > window.innerHeight) {
        top = rect.y - 60
        if (currentTool.value !== 'select' && currentTool.value !== 'picker') {
          top -= 40
        }
      }

      if (top < 0) {
        top = Math.max(10, rect.y + rect.height - 60 - 10)
      }

      const estimatedToolbarHeight = 80
      if (top + estimatedToolbarHeight > window.innerHeight) {
        top = Math.max(10, window.innerHeight - estimatedToolbarHeight)
      }

      if (right < 10) right = 10

      return {
        top: `${top}px`,
        right: `${right}px`
      }
    })

    function adjustToolbarPosition() {
      const toolbar = floatingToolbarRef.value
      if (!toolbar) return

      const toolbarRect = toolbar.getBoundingClientRect()
      const toolbarWidth = toolbarRect.width
      const toolbarLeft = toolbarRect.left

      // 如果工具栏超出屏幕左侧
      if (toolbarLeft < 0) {
        const newRight = window.innerWidth - toolbarWidth - 10
        toolbar.style.right = `${Math.max(10, newRight)}px`
      }

      // 如果工具栏超出屏幕右侧
      if (toolbarLeft + toolbarWidth > window.innerWidth) {
        toolbar.style.right = '10px'
      }
    }

    async function fetchWindows() {
      try {
        const result = await invoke(IPC_COMMANDS.GET_WINDOW_LIST)
        if (result.success) {
          const list = result.windows || []
          const maxWidth = list.reduce((max, item) => Math.max(max, Number(item?.width) || 0), 0)
          windowCoordScale.value = maxWidth > window.innerWidth * 1.15 ? dpr : 1
          windows.value = (result.windows || []).map(normalizeWindowRectToViewport)
        }
      } catch (error) {
        console.error('获取窗口失败:', error)
      }
    }

    function normalizeWindowRectToViewport(w) {
      const scale = windowCoordScale.value || 1
      const rawX = Number(w?.x) || 0
      const rawY = Number(w?.y) || 0
      const rawWidth = Number(w?.width) || 0
      const rawHeight = Number(w?.height) || 0
      return {
        ...w,
        x: (rawX - captureOriginX.value) / scale,
        y: (rawY - captureOriginY.value) / scale,
        width: rawWidth / scale,
        height: rawHeight / scale
      }
    }

    // 鼠标交互逻辑
    function onBorderMouseDown(e) {
      onMouseDown(e)
    }

    function isInside(x, y, r) {
      return x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
    }

    function isInsideBorder(x, y, r, borderThreshold = 4) {
      if (!isInside(x, y, r)) return false
      if (!r.width || !r.height) return false
      const nearLeft = x - r.x < borderThreshold
      const nearRight = (r.x + r.width) - x < borderThreshold
      const nearTop = y - r.y < borderThreshold
      const nearBottom = (r.y + r.height) - y < borderThreshold
      return nearLeft || nearRight || nearTop || nearBottom
    }

    function detectWindowAt(x, y) {
      for (const w of windows.value) {
        if (x >= w.x && x <= w.x + w.width && y >= w.y && y <= w.y + w.height) {
          return w
        }
      }
      return null
    }

    function startResize(handle, event) {
      state.value = 'resizing'
      resizeHandleType = handle
      const p = toScenePoint(event)
      startPoint.x = p.x
      startPoint.y = p.y
      Object.assign(startRect, rect)
    }

    function handleResize(e) {
      const p = toScenePoint(e)
      const dx = p.x - startPoint.x
      const dy = p.y - startPoint.y
      let {x, y, width, height} = startRect

      if (resizeHandleType.includes('l')) {
        x += dx;
        width -= dx;
      }
      if (resizeHandleType.includes('r')) {
        width += dx;
      }
      if (resizeHandleType.includes('t')) {
        y += dy;
        height -= dy;
      }
      if (resizeHandleType.includes('b')) {
        height += dy;
      }


      if (width < 0) {
        x += width;
        width = -width;
        resizeHandleType = resizeHandleType.replace('l', 'L').replace('r', 'l').replace('L', 'r');
        startPoint.x = p.x;
        startRect.x = x;
        startRect.width = width;
      }
      if (height < 0) {
        y += height;
        height = -height;
        resizeHandleType = resizeHandleType.replace('t', 'T').replace('b', 't').replace('T', 'b');
        startPoint.y = p.y;
        startRect.y = y;
        startRect.height = height;
      }

      rect.x = x;
      rect.y = y;
      rect.width = width;
      rect.height = height;
    }

    return {
        state,
        rect,
        startRect,
        startPoint,
        drawStart,
        isDrawing,
        windowHighlightRef,
        cutoutRef,
        floatingToolbarRef,
        highlightedWindow,
        hasSelection,
        cutoutStyle,
        sizeInfoStyle,
        selectionInfoText,
        recordingConfirmStyle,
        windowHighlightStyle,
        toolbarStyle,
        adjustToolbarPosition,
        editorCursorClass,
        isInside,
        isInsideBorder,
        detectWindowAt,
        fetchWindows,
        startResize,
        handleResize
    }
}
