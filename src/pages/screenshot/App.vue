<template>
  <div
      :class="['pixpin-editor', editorCursorClass]"
       @mousedown="onMouseDown"
       @mousemove="onMouseMove"
       @mouseup="onMouseUp"
       @contextmenu.prevent="onContextMenu">

    <!-- 底层截图 -->
    <img v-if="screenshotSrc" :src="screenshotSrc" class="bg-image" draggable="false"/>

    <!-- 绘制层 (全屏尺寸，缩放适配高DPI) -->
    <canvas ref="canvas" class="draw-canvas"></canvas>

    <!-- 遮罩与选区层 -->
    <div :class="{ 'pointer-none': state === 'drawing' }" class="mask-layer">
      <!-- 智能窗口高亮 -->
      <div v-if="highlightedWindow && state === 'idle'" ref="windowHighlightRef" class="window-highlight">
        <div class="window-border"></div>
      </div>

      <!-- 选区镂空及控制点 -->
      <div v-if="hasSelection || state === 'selecting'"
           ref="cutoutRef"
           :class="{ 'is-active': state === 'selected' }"
           class="cutout">

        <div :class="{ 'is-active': state !== 'drawing' }" class="cutout-border"></div>

        <div v-if="state === 'selecting' || state === 'resizing' || state === 'moving'" class="size-info">
          {{ Math.round(rect.width) }} × {{ Math.round(rect.height) }}
        </div>

        <!-- 8个调整控制点 -->
        <template v-if="hasSelection && state !== 'drawing' && currentTool === 'select'">
          <div class="handle tl" @mousedown.stop="startResize('tl', $event)"></div>
          <div class="handle tm" @mousedown.stop="startResize('tm', $event)"></div>
          <div class="handle tr" @mousedown.stop="startResize('tr', $event)"></div>
          <div class="handle ml" @mousedown.stop="startResize('ml', $event)"></div>
          <div class="handle mr" @mousedown.stop="startResize('mr', $event)"></div>
          <div class="handle bl" @mousedown.stop="startResize('bl', $event)"></div>
          <div class="handle bm" @mousedown.stop="startResize('bm', $event)"></div>
          <div class="handle br" @mousedown.stop="startResize('br', $event)"></div>
        </template>
      </div>
    </div>

    <!-- 浮动工具栏 -->
    <div v-if="hasSelection && (state === 'selected' || state === 'drawing')"
         ref="floatingToolbarRef"
         class="floating-toolbar"
         @mousedown.stop>

      <div class="tools-row primary-tools">
        <button v-for="tool in drawingTools" :key="tool.id"
                :class="{ active: currentTool === tool.id }" :title="tool.name"
                class="tool-btn" @click="setTool(tool.id)">
          <component :is="tool.icon" class="tool-icon-wrap"/>
        </button>

        <div class="divider"></div>

        <button :disabled="historyIndex <= 0" class="tool-btn" title="撤销 (Ctrl+Z)" @click="undo">
          <RefreshLeft class="tool-icon-wrap"/>
        </button>
        <button :disabled="historyIndex >= history.length - 1" class="tool-btn" title="重做 (Ctrl+Y)" @click="redo">
          <RefreshRight class="tool-icon-wrap"/>
        </button>

        <div class="divider"></div>

        <button :disabled="!canExport" class="tool-btn" title="复制到剪贴板 (Ctrl+C)" @click="copyToClipboardLinked">
          <DocumentCopy class="tool-icon-wrap"/>
        </button>
        <button :disabled="!canExport" class="tool-btn" title="保存文件 (Ctrl+S)" @click="saveAndClose">
          <Download class="tool-icon-wrap"/>
        </button>
        <button :disabled="!canExport" class="tool-btn" title="固定到屏幕" @click="pinToScreenAndClose">
          📌
        </button>
        <button class="tool-btn cancel" title="取消选区 (Esc)" @click="cancelSelection">
          <CloseBold class="tool-icon-wrap"/>
        </button>
        <button :disabled="!canExport" class="tool-btn confirm" title="完成并复制" @click="completeAndCopyUnlinked">
          <Check class="tool-icon-wrap"/>
        </button>
      </div>

      <!-- 二级属性栏 -->
      <div v-if="currentTool !== 'select' && currentTool !== 'picker'" class="tools-row secondary-tools">
        <input v-model="currentColor" class="color-picker" title="文字颜色" type="color" @input="syncEditingTextStyle"/>
        <input v-model="lineWidth" class="line-slider" max="20" min="1" title="线宽/字号" type="range"
               @input="syncEditingTextStyle"/>
        <template v-if="currentTool === 'text'">
          <select v-model="textStyle.fontFamily" class="text-style-select" title="字体" @change="syncEditingTextStyle">
            <option v-for="font in fontFamilies" :key="font" :value="font">{{ font }}</option>
          </select>
          <button :class="{ active: textStyle.bold }" class="tool-btn mini" title="加粗" @click="toggleTextBold">B
          </button>
          <button :class="{ active: textStyle.stroke }" class="tool-btn mini" title="描边" @click="toggleTextStroke">
            描
          </button>
          <input v-if="textStyle.stroke" v-model="textStyle.strokeColor" class="color-picker mini-picker" title="描边颜色"
                 type="color" @input="syncEditingTextStyle"/>
          <button :class="{ active: textStyle.shadow }" class="tool-btn mini" title="阴影" @click="toggleTextShadow">
            影
          </button>
        </template>
      </div>
    </div>

    <!-- 取色器放大镜 (暂留位) -->
    <div v-if="currentTool === 'picker' && pickColor" ref="pickerInfoRef" class="color-picker-info">
      {{ pickColor }}
    </div>

    <div
        v-for="item in textItems"
        :key="item.id"
        :class="{ editing: editingTextId === item.id, selected: selectedTextId === item.id }"
        :ref="(el) => setTextOverlayRef(el, item.id)"
        class="text-overlay-item"
        @mousedown.stop="selectTextItem(item.id)"
        @dblclick.stop="startEditTextItem(item)"
    >
      <div
          v-if="editingTextId === item.id"
          :ref="setEditingElementRef"
          class="text-inline-editor"
          contenteditable="plaintext-only"
          @blur="finishInlineEdit"
          @input="onInlineTextInput(item, $event)"
          @keydown.stop="handleInlineEditorKeydown($event)"
      ></div>
      <template v-else>
        <div v-for="(line, index) in item.text.split('\n')" :key="`${item.id}-${index}`">{{ line }}</div>
      </template>
    </div>

  </div>
</template>

<script setup>
import {computed, nextTick, onMounted, onUnmounted, reactive, ref, watchPostEffect} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {
  Aim,
  Brush,
  Check,
  CloseBold,
  Crop,
  DocumentCopy,
  Download,
  Edit,
  EditPen,
  Grid,
  Minus,
  Pointer,
  RefreshLeft,
  RefreshRight,
  TopRight
} from '@element-plus/icons-vue'

// 核心状态
const state = ref('idle') // idle, selecting, selected, moving, resizing, drawing
const screenshotSrc = ref('')
const screenshotImg = ref(null)
const canvas = ref(null)
const windowHighlightRef = ref(null)
const cutoutRef = ref(null)
const floatingToolbarRef = ref(null)
const pickerInfoRef = ref(null)
const isDrawing = ref(false)
const isCaptureReady = ref(false)

// 选区数据
const rect = reactive({x: 0, y: 0, width: 0, height: 0})
const startRect = reactive({x: 0, y: 0, width: 0, height: 0})
const startPoint = reactive({x: 0, y: 0})
const drawStart = reactive({x: 0, y: 0})
let resizeHandleType = ''
let currentDrawingSnapshot = null

// 工具数据
const currentTool = ref('select')
const currentColor = ref('#ff0000')
const lineWidth = ref(3)
const pickColor = ref('')
const textItems = ref([])
let textItemIdSeed = 1
const editingTextId = ref(null)
const selectedTextId = ref(null)
const editingElementRef = ref(null)
const textOverlayRefMap = new Map()
const editingBeforeText = ref('')
const fontFamilies = ['Arial', 'Microsoft YaHei', 'PingFang SC', 'Consolas', 'Times New Roman']
const textStyle = reactive({
  fontFamily: 'Arial',
  bold: false,
  stroke: false,
  strokeColor: '#000000',
  shadow: false
})

// 历史记录
const history = ref([])
const historyIndex = ref(-1)

// 窗口探测
const windows = ref([])
const highlightedWindow = ref(null)
const windowCoordScale = ref(1)
const captureOriginX = ref(0)
const captureOriginY = ref(0)
const hasScreenshotPayload = ref(false)
const screenshotSessionRequested = ref(false)
const activeSessionId = ref(0)
const payloadSessionId = ref(0)
let screenshotFallbackTimer = null

// 物理像素比例
const dpr = window.devicePixelRatio || 1

// 定义工具
const drawingTools = [
  {id: 'select', name: '框选/移动', icon: Pointer},
  {id: 'pen', name: '画笔', icon: EditPen},
  {id: 'line', name: '直线', icon: Minus},
  {id: 'arrow', name: '箭头', icon: TopRight},
  {id: 'rect', name: '矩形', icon: Crop},
  {id: 'circle', name: '圆形', icon: Aim},
  {id: 'text', name: '文字', icon: Edit},
  {id: 'mosaic', name: '马赛克', icon: Grid},
  {id: 'picker', name: '取色', icon: Brush}
]

const hasSelection = computed(() => rect.width > 0 && rect.height > 0)
const canExport = computed(() => hasSelection.value)

// 样式计算
const editorCursorClass = computed(() => {
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

  // 底部溢出则放上方
  if (top + 100 > window.innerHeight) {
    top = rect.y - 60
    if (currentTool.value !== 'select' && currentTool.value !== 'picker') {
      top -= 40 // 二级菜单空间
    }
  }
  // 上方也溢出则放内部底部
  if (top < 0) {
    top = rect.y + rect.height - 60 - 10
  }

  if (right < 10) right = 10
  if (right + 300 > window.innerWidth) right = window.innerWidth - 300 // 基础防溢出

  return {
    top: `${top}px`,
    right: `${right}px`
  }
})

const pickerStyle = computed(() => {
  return {
    left: `${drawStart.x + 15}px`,
    top: `${drawStart.y + 15}px`
  }
})

function setTextOverlayRef(el, id) {
  if (!id) return
  if (el) {
    textOverlayRefMap.set(id, el)
  } else {
    textOverlayRefMap.delete(id)
  }
}

function applyStyleObject(el, styleObj) {
  if (!el || !styleObj) return
  Object.entries(styleObj).forEach(([key, value]) => {
    el.style[key] = value
  })
}

watchPostEffect(() => {
  applyStyleObject(windowHighlightRef.value, windowHighlightStyle.value)
  applyStyleObject(cutoutRef.value, cutoutStyle.value)
  applyStyleObject(floatingToolbarRef.value, toolbarStyle.value)
  applyStyleObject(pickerInfoRef.value, pickerStyle.value)
  const validIds = new Set(textItems.value.map((item) => item.id))
  for (const [id, el] of textOverlayRefMap.entries()) {
    if (!validIds.has(id)) {
      textOverlayRefMap.delete(id)
      continue
    }
    const item = textItems.value.find((entry) => entry.id === id)
    if (item) {
      applyStyleObject(el, getTextItemStyle(item))
    }
  }
})

// 初始化与通信
onMounted(() => {
  window.addEventListener('screenshot-data', handleScreenshotData)
  window.addEventListener('start-region-select', handleStartRegionSelect)
  window.addEventListener('screenshot-reset', handleScreenshotReset)
  document.addEventListener('keydown', handleKeyDown)
  consumeBootPayload()
})

onUnmounted(() => {
  window.removeEventListener('screenshot-data', handleScreenshotData)
  window.removeEventListener('start-region-select', handleStartRegionSelect)
  window.removeEventListener('screenshot-reset', handleScreenshotReset)
  document.removeEventListener('keydown', handleKeyDown)
  if (screenshotFallbackTimer) {
    window.clearTimeout(screenshotFallbackTimer)
    screenshotFallbackTimer = null
  }
})

function handleScreenshotReset() {
  screenshotSrc.value = ''
  screenshotImg.value = null
  isCaptureReady.value = false
  highlightedWindow.value = null
  state.value = 'idle'
  currentTool.value = 'select'
  rect.x = 0
  rect.y = 0
  rect.width = 0
  rect.height = 0
  if (canvas.value) {
    canvas.value.width = 0
    canvas.value.height = 0
  }
}

function consumeBootPayload() {
  const boot = window.__SCREENSHOT_BOOT__
  if (!boot) return
  const bootStartSessionId = Number(boot.pendingStartSessionId) || 0
  if (bootStartSessionId > 0) {
    activeSessionId.value = bootStartSessionId
    screenshotSessionRequested.value = true
    hasScreenshotPayload.value = payloadSessionId.value === bootStartSessionId
  }
  if (boot.pendingData && boot.pendingData.png_base64) {
    handleScreenshotData({detail: boot.pendingData})
    boot.pendingData = null
  }
  if (bootStartSessionId > 0) {
    handleStartRegionSelect({detail: {session_id: bootStartSessionId}})
    boot.pendingStartSessionId = 0
  }
}

function scheduleScreenshotFallback() {
  if (!screenshotSessionRequested.value || hasScreenshotPayload.value) return
  if (screenshotFallbackTimer) {
    window.clearTimeout(screenshotFallbackTimer)
  }
  screenshotFallbackTimer = window.setTimeout(() => {
    if (screenshotSessionRequested.value && !hasScreenshotPayload.value) {
      requestScreenshot()
    }
  }, 120)
}

async function fetchWindows() {
  try {
    const result = await invoke('get_window_list')
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

async function requestScreenshot() {
  try {
    const result = await invoke('start_screenshot')
    if (result.success && result.png_base64) {
      hasScreenshotPayload.value = true
      captureOriginX.value = Number(result.origin_x) || 0
      captureOriginY.value = Number(result.origin_y) || 0
      await fetchWindows()
      loadImageFromBase64(result.png_base64)
    }
  } catch (error) {
    console.error('请求截图失败:', error)
  }
}

function handleScreenshotData(event) {
  if (event.detail && event.detail.png_base64) {
    const sessionId = Number(event.detail.session_id) || 0
    if (sessionId > 0) {
      activeSessionId.value = sessionId
      payloadSessionId.value = sessionId
    }
    isCaptureReady.value = false
    screenshotSessionRequested.value = true
    hasScreenshotPayload.value = true
    if (screenshotFallbackTimer) {
      window.clearTimeout(screenshotFallbackTimer)
      screenshotFallbackTimer = null
    }
    captureOriginX.value = Number(event.detail.origin_x) || 0
    captureOriginY.value = Number(event.detail.origin_y) || 0
    fetchWindows()
    loadImageFromBase64(event.detail.png_base64)
  }
}

function handleStartRegionSelect(event) {
  const sessionId = Number(event?.detail?.session_id) || 0
  if (sessionId > 0) {
    activeSessionId.value = sessionId
    screenshotSessionRequested.value = true
    hasScreenshotPayload.value = payloadSessionId.value === sessionId
  } else {
    screenshotSessionRequested.value = true
  }
  scheduleScreenshotFallback()
  state.value = 'idle'
  currentTool.value = 'select'
  rect.width = 0
  rect.height = 0
}

function loadImageFromBase64(base64Data) {
  isCaptureReady.value = false
  screenshotSrc.value = `data:image/png;base64,${base64Data}`
  const img = new Image()
  img.onload = () => {
    screenshotImg.value = img
    nextTick(() => {
      initCanvas()
      isCaptureReady.value = Boolean(canvas.value && canvas.value.width > 0 && canvas.value.height > 0)
    })
  }
  img.onerror = () => {
    isCaptureReady.value = false
  }
  img.src = `data:image/png;base64,${base64Data}`
}

function initCanvas() {
  if (!canvas.value || !screenshotImg.value) return

  canvas.value.width = window.innerWidth * dpr
  canvas.value.height = window.innerHeight * dpr

  const ctx = canvas.value.getContext('2d')
  if (!ctx) return
  // 仅放大坐标系，物理像素保持不变
  ctx.scale(dpr, dpr)

  // 保存一张纯净版的快照用于重置
  saveToHistory()
  isCaptureReady.value = true
}

async function ensureCaptureReady() {
  if (isCaptureReady.value && screenshotImg.value && canvas.value && canvas.value.width > 0 && canvas.value.height > 0) {
    return true
  }
  if (!screenshotImg.value && !screenshotSrc.value) {
    await requestScreenshot()
  }
  if (!screenshotImg.value && screenshotSrc.value) {
    const img = new Image()
    await new Promise((resolve, reject) => {
      img.onload = resolve
      img.onerror = reject
      img.src = screenshotSrc.value
    }).then(() => {
      screenshotImg.value = img
    }).catch(() => {
    })
  }
  await nextTick()
  if (screenshotImg.value && canvas.value) {
    initCanvas()
  }
  if (isCaptureReady.value && screenshotImg.value && canvas.value && canvas.value.width > 0 && canvas.value.height > 0) {
    return true
  }
  if (!screenshotImg.value || !canvas.value || canvas.value.width <= 0 || canvas.value.height <= 0) {
    await requestScreenshot()
    await nextTick()
    if (screenshotImg.value && canvas.value) {
      initCanvas()
    }
  }
  return Boolean(isCaptureReady.value && screenshotImg.value && canvas.value && canvas.value.width > 0 && canvas.value.height > 0)
}

// 鼠标交互逻辑
function onMouseDown(e) {
  if (editingTextId.value !== null) {
    finishInlineEdit()
    return
  }
  if (e.button !== 0) return
  selectedTextId.value = null
  if (state.value === 'idle') {
    state.value = 'selecting'
    startPoint.x = e.clientX
    startPoint.y = e.clientY
    rect.x = e.clientX
    rect.y = e.clientY
    rect.width = 0
    rect.height = 0
  } else if (state.value === 'selected') {
    if (currentTool.value === 'select') {
      if (isInside(e.clientX, e.clientY, rect)) {
        state.value = 'moving'
        startPoint.x = e.clientX
        startPoint.y = e.clientY
        Object.assign(startRect, rect)
      } else {
        // 点击外部重新选择
        state.value = 'selecting'
        startPoint.x = e.clientX
        startPoint.y = e.clientY
        rect.x = e.clientX
        rect.y = e.clientY
        rect.width = 0
        rect.height = 0
        // 重置画布
        if (history.value.length > 0) {
          historyIndex.value = 0
          restoreFromHistory()
        }
      }
    } else {
      // 绘制模式
      if (!isInside(e.clientX, e.clientY, rect) && currentTool.value !== 'picker') {
        // 外部点击，取消选择并重新选择
        state.value = 'selecting'
        startPoint.x = e.clientX
        startPoint.y = e.clientY
        rect.x = e.clientX
        rect.y = e.clientY
        rect.width = 0
        rect.height = 0
        currentTool.value = 'select'
        // 重置画布
        if (history.value.length > 0) {
          historyIndex.value = 0
          restoreFromHistory()
        }
        return
      }
      state.value = 'drawing'
      handleCanvasMouseDown(e)
    }
  } else if (state.value === 'drawing') {
    handleCanvasMouseDown(e)
  }
}

function onMouseMove(e) {
  if (editingTextId.value !== null) return
  if (state.value === 'idle') {
    highlightedWindow.value = detectWindowAt(e.clientX, e.clientY)
  } else if (state.value === 'selecting') {
    // 鼠标拖动框选区域
    const x = Math.min(startPoint.x, e.clientX)
    const y = Math.min(startPoint.y, e.clientY)
    const width = Math.abs(e.clientX - startPoint.x)
    const height = Math.abs(e.clientY - startPoint.y)
    rect.x = x
    rect.y = y
    rect.width = width
    rect.height = height
  } else if (state.value === 'moving') {
    const dx = e.clientX - startPoint.x
    const dy = e.clientY - startPoint.y
    rect.x = startRect.x + dx
    rect.y = startRect.y + dy
  } else if (state.value === 'resizing') {
    handleResize(e)
  } else if (state.value === 'drawing') {
    handleCanvasMouseMove(e)
  }
}

function onMouseUp(e) {
  if (editingTextId.value !== null) return
  if (e.button !== 0) return
  if (state.value === 'selecting') {
    if (rect.width < 10 || rect.height < 10) {
      if (highlightedWindow.value) {
        const w = highlightedWindow.value
        const almostFullscreen =
            w.width >= window.innerWidth - 16 &&
            w.height >= window.innerHeight - 16
        if (almostFullscreen) {
          rect.x = 0
          rect.y = 0
          rect.width = window.innerWidth
          rect.height = window.innerHeight
        } else {
          Object.assign(rect, highlightedWindow.value)
        }
        state.value = 'selected'
      } else {
        rect.x = 0
        rect.y = 0
        rect.width = window.innerWidth
        rect.height = window.innerHeight
        state.value = 'selected'
      }
    } else {
      state.value = 'selected'
    }
  } else if (state.value === 'moving' || state.value === 'resizing') {
    state.value = 'selected'
  } else if (state.value === 'drawing') {
    handleCanvasMouseUp(e)
    state.value = 'selected'
  }
}

function onContextMenu(e) {
  close()
}

function cancelSelection() {
  finishInlineEdit()
  selectedTextId.value = null
  state.value = 'idle'
  rect.width = 0
  rect.height = 0
  currentTool.value = 'select'
  // 重置画布到最初
  if (history.value.length > 0) {
    historyIndex.value = 0
    restoreFromHistory()
  }
}

// 辅助函数
function isInside(x, y, r) {
  return x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
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
  startPoint.x = event.clientX
  startPoint.y = event.clientY
  Object.assign(startRect, rect)
}

function handleResize(e) {
  const dx = e.clientX - startPoint.x
  const dy = e.clientY - startPoint.y
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

  // 反向处理
  if (width < 0) {
    x += width;
    width = -width;
    resizeHandleType = resizeHandleType.replace('l', 'L').replace('r', 'l').replace('L', 'r');
    startPoint.x = e.clientX;
    startRect.x = x;
    startRect.width = width;
  }
  if (height < 0) {
    y += height;
    height = -height;
    resizeHandleType = resizeHandleType.replace('t', 'T').replace('b', 't').replace('T', 'b');
    startPoint.y = e.clientY;
    startRect.y = y;
    startRect.height = height;
  }

  rect.x = x;
  rect.y = y;
  rect.width = width;
  rect.height = height;
}

function setTool(toolId) {
  currentTool.value = toolId
}

// 绘制相关
function handleCanvasMouseDown(event) {
  if (currentTool.value === 'picker') {
    pickColorAt(event)
    drawStart.x = event.clientX
    drawStart.y = event.clientY
    return
  }

  isDrawing.value = true
  drawStart.x = event.clientX
  drawStart.y = event.clientY

  const ctx = canvas.value.getContext('2d')

  if (currentTool.value === 'pen' || currentTool.value === 'mosaic') {
    ctx.beginPath()
    ctx.moveTo(drawStart.x, drawStart.y)
  } else if (['line', 'arrow', 'rect', 'circle'].includes(currentTool.value)) {
    // 拖拽形状时需要保留之前的快照，以防留痕
    currentDrawingSnapshot = ctx.getImageData(0, 0, canvas.value.width, canvas.value.height)
  }
}

function handleCanvasMouseMove(event) {
  if (currentTool.value === 'picker' && !isDrawing.value) {
    drawStart.x = event.clientX;
    drawStart.y = event.clientY;
    pickColorAt(event);
    return;
  }

  if (!isDrawing.value) return

  const x = event.clientX
  const y = event.clientY
  const ctx = canvas.value.getContext('2d')

  if (currentTool.value === 'pen') {
    ctx.strokeStyle = currentColor.value
    ctx.lineWidth = lineWidth.value
    ctx.lineCap = 'round'
    ctx.lineTo(x, y)
    ctx.stroke()
  } else if (currentTool.value === 'mosaic') {
    // 马赛克处理：直接获取物理像素并打码
    const size = lineWidth.value * 3
    const physSize = Math.round(size * dpr)
    const px = Math.round(x * dpr)
    const py = Math.round(y * dpr)

    // 如果没有背景底图则不处理
    if (!screenshotImg.value) return

    // 直接从原图获取颜色数据
    const tempCanvas = document.createElement('canvas')
    tempCanvas.width = physSize
    tempCanvas.height = physSize
    const tempCtx = tempCanvas.getContext('2d')
    tempCtx.drawImage(
        screenshotImg.value,
        px - physSize / 2, py - physSize / 2, physSize, physSize,
        0, 0, physSize, physSize
    )

    const imageData = tempCtx.getImageData(0, 0, physSize, physSize)
    const data = imageData.data
    const blockSize = Math.round(6 * dpr)

    for (let i = 0; i < physSize; i += blockSize) {
      for (let j = 0; j < physSize; j += blockSize) {
        const pIdx = (j * physSize + i) * 4
        if (pIdx >= data.length) continue
        const r = data[pIdx], g = data[pIdx + 1], b = data[pIdx + 2]

        for (let bi = 0; bi < blockSize && i + bi < physSize; bi++) {
          for (let bj = 0; bj < blockSize && j + bj < physSize; bj++) {
            const idx = ((j + bj) * physSize + (i + bi)) * 4
            if (idx < data.length) {
              data[idx] = r;
              data[idx + 1] = g;
              data[idx + 2] = b
            }
          }
        }
      }
    }

    // 将打码后的像素放回主画布 (注意需要重置scale才能使用putImageData精确对齐)
    const oldTransform = ctx.getTransform()
    ctx.resetTransform()
    ctx.putImageData(imageData, px - physSize / 2, py - physSize / 2)
    ctx.setTransform(oldTransform)

  } else if (['line', 'arrow', 'rect', 'circle'].includes(currentTool.value)) {
    // 恢复快照
    const oldTransform = ctx.getTransform()
    ctx.resetTransform()
    ctx.putImageData(currentDrawingSnapshot, 0, 0)
    ctx.setTransform(oldTransform)

    ctx.strokeStyle = currentColor.value
    ctx.lineWidth = lineWidth.value
    ctx.beginPath()

    if (currentTool.value === 'line' || currentTool.value === 'arrow') {
      ctx.moveTo(drawStart.x, drawStart.y)
      ctx.lineTo(x, y)
      ctx.stroke()
      if (currentTool.value === 'arrow') {
        drawArrowHead(ctx, drawStart.x, drawStart.y, x, y)
      }
    } else if (currentTool.value === 'rect') {
      ctx.strokeRect(drawStart.x, drawStart.y, x - drawStart.x, y - drawStart.y)
    } else if (currentTool.value === 'circle') {
      const radius = Math.sqrt(Math.pow(x - drawStart.x, 2) + Math.pow(y - drawStart.y, 2))
      ctx.arc(drawStart.x, drawStart.y, radius, 0, Math.PI * 2)
      ctx.stroke()
    }
  }
}

function handleCanvasMouseUp(event) {
  if (!isDrawing.value) return
  isDrawing.value = false

  const x = event.clientX
  const y = event.clientY
  const ctx = canvas.value.getContext('2d')

  if (currentTool.value === 'text') {
    startCreateTextItem(x, y)
    state.value = 'selected'
    return
  }

  saveToHistory()
}

function startCreateTextItem(x, y) {
  const safeX = Math.max(10, Math.min(x, window.innerWidth - 240))
  const safeY = Math.max(10, Math.min(y, window.innerHeight - 120))
  const item = {
    id: textItemIdSeed++,
    x: safeX,
    y: safeY,
    text: '',
    color: currentColor.value,
    fontSize: lineWidth.value * 8,
    fontFamily: textStyle.fontFamily,
    bold: textStyle.bold,
    stroke: textStyle.stroke,
    strokeColor: textStyle.strokeColor,
    shadow: textStyle.shadow
  }
  textItems.value.push(item)
  startEditTextItem(item)
}

function startEditTextItem(item) {
  currentTool.value = 'text'
  editingTextId.value = item.id
  selectedTextId.value = item.id
  editingBeforeText.value = item.text
  currentColor.value = item.color
  lineWidth.value = Math.max(1, Math.round(item.fontSize / 8))
  textStyle.fontFamily = item.fontFamily || 'Arial'
  textStyle.bold = !!item.bold
  textStyle.stroke = !!item.stroke
  textStyle.strokeColor = item.strokeColor || '#000000'
  textStyle.shadow = !!item.shadow
  nextTick(() => {
    const el = editingElementRef.value
    if (el) {
      el.focus()
      const selection = window.getSelection()
      const range = document.createRange()
      range.selectNodeContents(el)
      range.collapse(false)
      selection.removeAllRanges()
      selection.addRange(range)
    }
  })
}

function setEditingElementRef(el) {
  if (!el || editingTextId.value === null) return
  editingElementRef.value = el
  const item = textItems.value.find(t => t.id === editingTextId.value)
  if (!item) return
  if (el.innerText !== item.text) {
    el.innerText = item.text
  }
}

function onInlineTextInput(item, event) {
  item.text = event.target.innerText.replace(/\r/g, '')
  selectedTextId.value = item.id
}

function handleInlineEditorKeydown(event) {
  if (event.key === 'Escape') {
    event.preventDefault()
    cancelInlineEdit()
  }
}

function finishInlineEdit() {
  if (editingTextId.value === null) return
  const id = editingTextId.value
  const index = textItems.value.findIndex(item => item.id === id)
  const before = editingBeforeText.value
  if (index === -1) {
    editingTextId.value = null
    editingElementRef.value = null
    editingBeforeText.value = ''
    return
  }
  const text = textItems.value[index].text.trim()
  if (!text) {
    textItems.value.splice(index, 1)
    selectedTextId.value = null
  } else {
    textItems.value[index].text = text
    selectedTextId.value = id
  }
  editingTextId.value = null
  editingElementRef.value = null
  editingBeforeText.value = ''
  if (text !== before) {
    saveToHistory()
  }
}

function cancelInlineEdit() {
  if (editingTextId.value === null) return
  const id = editingTextId.value
  const index = textItems.value.findIndex(item => item.id === id)
  const before = editingBeforeText.value
  if (index !== -1) {
    if (!before.trim()) {
      textItems.value.splice(index, 1)
      selectedTextId.value = null
    } else {
      textItems.value[index].text = before
      selectedTextId.value = id
    }
  }
  editingTextId.value = null
  editingElementRef.value = null
  editingBeforeText.value = ''
}

function selectTextItem(id) {
  selectedTextId.value = id
}

function toggleTextBold() {
  textStyle.bold = !textStyle.bold
  syncEditingTextStyle()
}

function toggleTextStroke() {
  textStyle.stroke = !textStyle.stroke
  syncEditingTextStyle()
}

function toggleTextShadow() {
  textStyle.shadow = !textStyle.shadow
  syncEditingTextStyle()
}

function syncEditingTextStyle() {
  if (editingTextId.value === null) return
  const item = textItems.value.find(t => t.id === editingTextId.value)
  if (!item) return
  item.color = currentColor.value
  item.fontSize = lineWidth.value * 8
  item.fontFamily = textStyle.fontFamily
  item.bold = textStyle.bold
  item.stroke = textStyle.stroke
  item.strokeColor = textStyle.strokeColor
  item.shadow = textStyle.shadow
}

function getTextItemStyle(item) {
  const fontWeight = item.bold ? '700' : '400'
  const textShadow = item.shadow ? '0 2px 8px rgba(0,0,0,0.6)' : 'none'
  return {
    left: `${item.x}px`,
    top: `${item.y}px`,
    color: item.color,
    fontSize: `${item.fontSize}px`,
    fontFamily: item.fontFamily || 'Arial',
    fontWeight,
    textShadow,
    WebkitTextStroke: item.stroke ? `1px ${item.strokeColor || '#000000'}` : '0'
  }
}

function drawArrowHead(ctx, fromX, fromY, toX, toY) {
  const angle = Math.atan2(toY - fromY, toX - fromX)
  const headLength = 15
  ctx.beginPath()
  ctx.moveTo(toX, toY)
  ctx.lineTo(toX - headLength * Math.cos(angle - Math.PI / 6), toY - headLength * Math.sin(angle - Math.PI / 6))
  ctx.moveTo(toX, toY)
  ctx.lineTo(toX - headLength * Math.cos(angle + Math.PI / 6), toY - headLength * Math.sin(angle + Math.PI / 6))
  ctx.stroke()
}

function pickColorAt(event) {
  const ctx = canvas.value.getContext('2d')
  // 获取物理像素
  const px = Math.round(event.clientX * dpr)
  const py = Math.round(event.clientY * dpr)

  // 如果没有绘图层数据，优先从原图取色
  const tempCanvas = document.createElement('canvas')
  tempCanvas.width = 1;
  tempCanvas.height = 1;
  const tempCtx = tempCanvas.getContext('2d')
  tempCtx.drawImage(screenshotImg.value, px, py, 1, 1, 0, 0, 1, 1)

  // 将涂鸦层叠加
  const oldTransform = ctx.getTransform()
  ctx.resetTransform()
  const overlayData = ctx.getImageData(px, py, 1, 1)
  ctx.setTransform(oldTransform)

  // 简单合并 (若涂鸦层不透明则使用涂鸦层)
  let pixel = tempCtx.getImageData(0, 0, 1, 1).data
  if (overlayData.data[3] > 0) {
    pixel = overlayData.data
  }

  const hex = '#' + [pixel[0], pixel[1], pixel[2]].map(v => v.toString(16).padStart(2, '0')).join('')
  pickColor.value = hex.toUpperCase()
  if (isDrawing.value) currentColor.value = hex
}

// 历史记录
function saveToHistory() {
  if (!canvas.value) return
  const ctx = canvas.value.getContext('2d')
  const oldTransform = ctx.getTransform()
  ctx.resetTransform()
  const imageData = ctx.getImageData(0, 0, canvas.value.width, canvas.value.height)
  ctx.setTransform(oldTransform)
  const textSnapshot = textItems.value.map(item => ({...item}))

  history.value = history.value.slice(0, historyIndex.value + 1)
  history.value.push({
    imageData,
    textItems: textSnapshot
  })
  historyIndex.value = history.value.length - 1

  if (history.value.length > 50) {
    history.value.shift()
    historyIndex.value--
  }
}

function undo() {
  if (historyIndex.value > 0) {
    historyIndex.value--
    restoreFromHistory()
  }
}

function redo() {
  if (historyIndex.value < history.value.length - 1) {
    historyIndex.value++
    restoreFromHistory()
  }
}

function restoreFromHistory() {
  if (historyIndex.value < 0 || !history.value[historyIndex.value]) return
  const snapshot = history.value[historyIndex.value]
  const ctx = canvas.value.getContext('2d')
  const oldTransform = ctx.getTransform()
  ctx.resetTransform()
  ctx.putImageData(snapshot.imageData, 0, 0)
  ctx.setTransform(oldTransform)
  textItems.value = snapshot.textItems.map(item => ({...item}))
}

// 最终出图
function getCroppedCanvas() {
  if (!canvas.value || !screenshotImg.value) {
    throw new Error('截图源未就绪')
  }
  const drawCanvasWidth = Number(canvas.value.width) || 0
  const drawCanvasHeight = Number(canvas.value.height) || 0
  if (drawCanvasWidth <= 0 || drawCanvasHeight <= 0) {
    throw new Error('绘制画布尺寸无效')
  }
  const maxX = Math.max(0, drawCanvasWidth - 1)
  const maxY = Math.max(0, drawCanvasHeight - 1)
  const startX = Math.max(0, Math.min(maxX, Math.round(rect.x * dpr)))
  const startY = Math.max(0, Math.min(maxY, Math.round(rect.y * dpr)))
  const rawWidth = Math.max(1, Math.round(rect.width * dpr))
  const rawHeight = Math.max(1, Math.round(rect.height * dpr))
  const sourceWidth = Math.max(1, Math.min(rawWidth, drawCanvasWidth - startX))
  const sourceHeight = Math.max(1, Math.min(rawHeight, drawCanvasHeight - startY))
  const sourceX = Math.max(0, Math.min(startX, drawCanvasWidth - sourceWidth))
  const sourceY = Math.max(0, Math.min(startY, drawCanvasHeight - sourceHeight))
  if (sourceWidth <= 0 || sourceHeight <= 0) {
    throw new Error(
        `裁剪区域无效 rect=(${rect.x},${rect.y},${rect.width},${rect.height}) dpr=${dpr} canvas=${drawCanvasWidth}x${drawCanvasHeight}`
    )
  }

  const cropCanvas = document.createElement('canvas')
  cropCanvas.width = sourceWidth
  cropCanvas.height = sourceHeight
  const ctx = cropCanvas.getContext('2d')
  if (!ctx) {
    throw new Error('裁剪画布上下文创建失败')
  }

  // 绘制底图
  ctx.drawImage(
      screenshotImg.value,
      sourceX, sourceY, sourceWidth, sourceHeight,
      0, 0, sourceWidth, sourceHeight
  )

  // 绘制涂鸦层
  const drawCtx = canvas.value.getContext('2d')
  if (!drawCtx) {
    throw new Error('绘制画布上下文获取失败')
  }
  const oldTransform = drawCtx.getTransform()
  drawCtx.resetTransform()
  const overlayData = drawCtx.getImageData(sourceX, sourceY, sourceWidth, sourceHeight)
  drawCtx.setTransform(oldTransform)

  // 将涂鸦层叠加到最终图像
  const tempCanvas = document.createElement('canvas')
  tempCanvas.width = sourceWidth
  tempCanvas.height = sourceHeight
  const tempCtx = tempCanvas.getContext('2d')
  if (!tempCtx) {
    throw new Error('临时画布上下文创建失败')
  }
  tempCtx.putImageData(overlayData, 0, 0)

  ctx.drawImage(tempCanvas, 0, 0)
  drawTextItemsOnCroppedCanvas(ctx, sourceX, sourceY, sourceWidth, sourceHeight)

  return cropCanvas
}

function drawTextItemsOnCroppedCanvas(ctx, sourceX, sourceY, sourceWidth, sourceHeight) {
  const cropLeft = sourceX / dpr
  const cropTop = sourceY / dpr
  const cropRight = cropLeft + sourceWidth / dpr
  const cropBottom = cropTop + sourceHeight / dpr
  for (const item of textItems.value) {
    if (item.x > cropRight || item.y > cropBottom) continue
    if (item.x < cropLeft - 400 || item.y < cropTop - 200) continue
    drawTextItemToContext(ctx, item, item.x - cropLeft, item.y - cropTop)
  }
}

function drawTextItemToContext(ctx, item, x, y) {
  const lines = item.text.split('\n')
  const fontWeight = item.bold ? '700' : '400'
  ctx.save()
  ctx.fillStyle = item.color
  ctx.font = `${fontWeight} ${item.fontSize}px ${item.fontFamily || 'Arial'}`
  ctx.textBaseline = 'top'
  if (item.shadow) {
    ctx.shadowColor = 'rgba(0, 0, 0, 0.65)'
    ctx.shadowBlur = Math.max(4, Math.round(item.fontSize * 0.35))
    ctx.shadowOffsetX = 0
    ctx.shadowOffsetY = Math.max(1, Math.round(item.fontSize * 0.1))
  } else {
    ctx.shadowColor = 'transparent'
    ctx.shadowBlur = 0
    ctx.shadowOffsetX = 0
    ctx.shadowOffsetY = 0
  }
  const lineHeight = Math.max(item.fontSize * 1.25, item.fontSize + 4)
  for (let i = 0; i < lines.length; i++) {
    const lineY = y + i * lineHeight
    if (item.stroke) {
      ctx.strokeStyle = item.strokeColor || '#000000'
      ctx.lineWidth = Math.max(1, Math.round(item.fontSize / 14))
      ctx.strokeText(lines[i], x, lineY)
    }
    ctx.fillText(lines[i], x, lineY)
  }
  ctx.restore()
}

async function writeClipboardImage(linked, closeAfterCopy) {
  try {
    if (!(await ensureCaptureReady())) {
      alert('截图源尚未就绪，请稍后重试')
      return
    }
    await invoke('set_screenshot_clipboard_link_once', {linked})
    const cropCanvas = getCroppedCanvas()
    cropCanvas.toBlob(async (blob) => {
      await navigator.clipboard.write([new ClipboardItem({'image/png': blob})])
      if (closeAfterCopy) {
        close()
      }
    })
  } catch (error) {
    console.error('复制失败:', error)
    alert('复制失败')
  }
}

async function copyToClipboardLinked() {
  await writeClipboardImage(true, true)
}

async function completeAndCopyUnlinked() {
  await writeClipboardImage(false, true)
}

async function pinToScreenAndClose() {
  try {
    if (!(await ensureCaptureReady())) {
      alert('截图源尚未就绪，请稍后重试')
      return
    }
    const cropCanvas = getCroppedCanvas()
    const dataUrl = cropCanvas.toDataURL('image/png')
    const base64 = dataUrl.split(',')[1]
    const payload = {
      request: {
        pngBase64: base64,
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.max(1, Math.round(rect.width)),
        height: Math.max(1, Math.round(rect.height))
      }
    }
    let result = null
    try {
      result = await invoke('pin_screenshot_on_screen', payload)
    } catch (firstError) {
      await new Promise(resolve => setTimeout(resolve, 80))
      result = await invoke('pin_screenshot_on_screen', payload)
      if (!result?.success) {
        throw firstError
      }
    }
    if (result?.success) {
      close()
    }
  } catch (error) {
    const reason = error?.message
        || (typeof error === 'string' ? error : JSON.stringify(error))
        || '未知错误'
    console.error('固定图片失败:', error)
    alert(`固定图片失败：${reason}`)
  }
}

async function saveAndClose() {
  try {
    if (!(await ensureCaptureReady())) {
      alert('截图源尚未就绪，请稍后重试')
      return
    }
    const cropCanvas = getCroppedCanvas()
    const dataUrl = cropCanvas.toDataURL('image/png')
    const base64 = dataUrl.split(',')[1]

    const result = await invoke('save_screenshot', {pngBase64: base64})
    if (result.success) {
      close()
    }
  } catch (error) {
    console.error('保存失败:', error)
    alert('保存失败')
  }
}

async function close() {
  try {
    await invoke('close_screenshot_window')
  } catch (error) {
    console.error('关闭窗口失败:', error)
  }
}

// 快捷键
function handleKeyDown(event) {
  if (editingTextId.value !== null) {
    if (event.key === 'Escape') {
      event.preventDefault()
      cancelInlineEdit()
    }
    return
  }
  if (event.key === 'Escape') {
    if (state.value === 'selected' || state.value === 'selecting') {
      cancelSelection()
    } else {
      close()
    }
  } else if (event.ctrlKey && event.key === 'z') {
    event.preventDefault()
    undo()
  } else if (event.ctrlKey && event.key === 'y') {
    event.preventDefault()
    redo()
  } else if (event.ctrlKey && event.key === 'c') {
    event.preventDefault()
    if (hasSelection.value) copyToClipboardLinked()
  } else if (event.ctrlKey && event.key === 's') {
    event.preventDefault()
    if (hasSelection.value) saveAndClose()
  }
}

</script>

<style scoped>
:global(html),
:global(body),
:global(#app) {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.pixpin-editor {
  width: 100%;
  height: 100%;
  overflow: hidden;
  user-select: none;
  position: relative;
  background: transparent;
}

.pixpin-editor.cursor-crosshair {
  cursor: crosshair;
}

.pixpin-editor.cursor-default {
  cursor: default;
}

.bg-image {
  position: absolute;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  z-index: 1;
  object-fit: fill;
}

.draw-canvas {
  position: absolute;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  z-index: 2;
  pointer-events: none; /* 画布不接管鼠标，由外层统一接管 */
}

.mask-layer {
  position: absolute;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  z-index: 3;
}

.mask-layer.pointer-none {
  pointer-events: none; /* 绘制模式下，遮罩不阻挡鼠标事件 */
}

.cutout {
  position: absolute;
  /* 使用巨大阴影实现外围遮罩效果 */
  box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.4);
}

.cutout.is-active {
  cursor: move;
}

.cutout-border {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  border: 1px solid rgba(255, 255, 255, 0.5);
}

.cutout-border.is-active {
  border: 2px solid #00aaff;
}

.size-info {
  position: absolute;
  top: -25px;
  left: 0;
  background: rgba(0, 0, 0, 0.8);
  color: white;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
  white-space: nowrap;
}

/* 控制点 */
.handle {
  position: absolute;
  width: 10px;
  height: 10px;
  background: #00aaff;
  border: 1px solid white;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  z-index: 10;
}

.handle.tl {
  top: 0;
  left: 0;
  cursor: nwse-resize;
}

.handle.tm {
  top: 0;
  left: 50%;
  cursor: ns-resize;
}

.handle.tr {
  top: 0;
  left: 100%;
  cursor: nesw-resize;
}

.handle.ml {
  top: 50%;
  left: 0;
  cursor: ew-resize;
}

.handle.mr {
  top: 50%;
  left: 100%;
  cursor: ew-resize;
}

.handle.bl {
  top: 100%;
  left: 0;
  cursor: nesw-resize;
}

.handle.bm {
  top: 100%;
  left: 50%;
  cursor: ns-resize;
}

.handle.br {
  top: 100%;
  left: 100%;
  cursor: nwse-resize;
}

.window-highlight {
  position: absolute;
  z-index: 4;
  pointer-events: none;
}

.window-border {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  border: 2px solid #00aaff;
  background: rgba(0, 170, 255, 0.1);
}

.window-label {
  position: absolute;
  top: -22px;
  left: 0;
  background: rgba(0, 170, 255, 0.9);
  color: white;
  padding: 2px 6px;
  border-radius: 3px;
  font-size: 12px;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 工具栏 */
.floating-toolbar {
  position: absolute;
  background: #2d2d2d;
  border: 1px solid #404040;
  border-radius: 6px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  z-index: 1000;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.tools-row {
  display: flex;
  align-items: center;
  gap: 4px;
}

.tool-btn {
  width: 30px;
  height: 30px;
  border: none;
  background: transparent;
  color: #ccc;
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  justify-content: center;
  align-items: center;
  font-size: 14px;
  transition: all 0.2s;
}

.tool-btn:hover {
  background: #404040;
  color: white;
}

.tool-btn.active {
  background: #0066cc;
  color: white;
}

.tool-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.tool-icon-wrap {
  width: 16px;
  height: 16px;
  color: inherit;
  display: block;
}

.tool-btn.primary {
  color: #00aaff;
}

.tool-btn.cancel {
  color: #ff4444;
}

.tool-btn.confirm {
  color: #00ff00;
  background: #00550033;
}

.tool-btn.confirm:hover {
  background: #00ff0066;
}

.tool-btn.mini {
  width: 26px;
  height: 26px;
  font-size: 12px;
}

.divider {
  width: 1px;
  height: 20px;
  background: #555;
  margin: 0 4px;
}

.secondary-tools {
  padding-top: 4px;
  border-top: 1px solid #404040;
  justify-content: space-between;
  padding-left: 4px;
  padding-right: 4px;
}

.color-picker {
  width: 24px;
  height: 24px;
  border: none;
  padding: 0;
  border-radius: 4px;
  cursor: pointer;
  background: transparent;
}

.line-slider {
  width: 100px;
}

.text-style-select {
  height: 26px;
  border: 1px solid #4b5563;
  border-radius: 4px;
  background: #1f2937;
  color: #f3f4f6;
  padding: 0 6px;
}

.mini-picker {
  width: 20px;
  height: 20px;
}

.color-picker-info {
  position: absolute;
  background: rgba(0, 0, 0, 0.8);
  color: white;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
  z-index: 1000;
  pointer-events: none;
}

.text-overlay-item {
  position: absolute;
  z-index: 1100;
  white-space: pre-wrap;
  line-height: 1.25;
  cursor: text;
  user-select: none;
}

.text-overlay-item.selected {
  border: 1px dashed rgba(255, 255, 255, 0.45);
  border-radius: 3px;
}

.text-overlay-item.editing {
  min-width: 24px;
  min-height: 20px;
  border: 1px dashed rgba(255, 255, 255, 0.78);
  border-radius: 4px;
  user-select: text;
}

.text-inline-editor {
  min-width: 24px;
  min-height: 20px;
  outline: none;
  white-space: pre-wrap;
  caret-color: #ffffff;
  user-select: text;
}
</style>
