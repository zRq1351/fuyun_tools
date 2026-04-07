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

        <div v-if="state === 'selecting' || state === 'resizing' || state === 'moving' || state === 'selected'"
             class="size-info">
          {{ selectionInfoText }}
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

    <div
        v-if="regionSelectMode === 'recording_region' && hasSelection && state === 'selected'"
        :style="recordingConfirmStyle"
        class="recording-region-confirm"
        @mousedown.stop
    >
      <button class="region-icon-btn primary" title="确定区域 (Enter)" @click.stop="commitRecordingRegionSelection">
        <Check class="tool-icon-wrap"/>
      </button>
      <button class="region-icon-btn" title="重选区域" @click.stop="cancelSelection">
        <RefreshLeft class="tool-icon-wrap"/>
      </button>
      <button class="region-icon-btn danger" title="取消 (Esc)" @click.stop="close">
        <X class="tool-icon-wrap"/>
      </button>
    </div>

    <!-- 浮动工具栏 -->
    <div v-if="regionSelectMode !== 'recording_region' && hasSelection && (state === 'selected' || state === 'drawing')"
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
          <Pin class="tool-icon-wrap"/>
        </button>
        <button class="tool-btn" title="取消选区 (Esc)" @click="cancelSelection">
          <X class="tool-icon-wrap"/>
        </button>
        <button :disabled="!canExport" class="tool-btn" title="完成并复制" @click="completeAndCopyUnlinked">
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

    <!-- 取色器放大镜 -->
    <div v-if="currentTool === 'picker' && pickColor" ref="pickerInfoRef" class="color-picker-info">
      <canvas ref="pickerMagnifierCanvasRef" class="picker-magnifier"></canvas>
      <div class="picker-meta">
        <div :style="{ backgroundColor: pickColor }" class="picker-swatch"></div>
        <div class="picker-value">{{ pickerDisplayValue }}</div>
        <div class="picker-hint">{{ pickerCopyHint }}</div>
      </div>
    </div>

    <div
        v-for="shape in shapeItems"
        :key="`shape-${shape.id}`"
        :class="{ selected: selectedShapeId === shape.id }"
        :style="getShapeItemStyle(shape)"
        class="shape-overlay-item"
        @mousedown.stop="startDragShapeItem(shape.id, $event)"
    >
      <template v-if="shape.type === 'rect'">
        <div :style="getShapeStrokeStyle(shape)" class="shape-rect"></div>
      </template>
      <template v-else-if="shape.type === 'circle'">
        <div :style="getShapeStrokeStyle(shape)" class="shape-circle"></div>
      </template>
      <template v-else>
        <svg :height="shape.height" :width="shape.width" class="shape-line-svg">
          <line
              :stroke="shape.color"
              :stroke-width="shape.lineWidth"
              :x1="shape.x1"
              :x2="shape.x2"
              :y1="shape.y1"
              :y2="shape.y2"
              stroke-linecap="round"
          />
          <template v-if="shape.type === 'arrow'">
            <line
                :stroke="shape.color"
                :stroke-width="shape.lineWidth"
                :x1="shape.x2"
                :x2="shape.arrowLeft.x"
                :y1="shape.y2"
                :y2="shape.arrowLeft.y"
                stroke-linecap="round"
            />
            <line
                :stroke="shape.color"
                :stroke-width="shape.lineWidth"
                :x1="shape.x2"
                :x2="shape.arrowRight.x"
                :y1="shape.y2"
                :y2="shape.arrowRight.y"
                stroke-linecap="round"
            />
          </template>
        </svg>
      </template>
      <template v-if="selectedShapeId === shape.id && (shape.type === 'rect' || shape.type === 'circle')">
        <div class="shape-resize-handle tl" @mousedown.stop="startResizeShapeItem(shape.id, 'tl', $event)"></div>
        <div class="shape-resize-handle tm" @mousedown.stop="startResizeShapeItem(shape.id, 'tm', $event)"></div>
        <div class="shape-resize-handle tr" @mousedown.stop="startResizeShapeItem(shape.id, 'tr', $event)"></div>
        <div class="shape-resize-handle ml" @mousedown.stop="startResizeShapeItem(shape.id, 'ml', $event)"></div>
        <div class="shape-resize-handle mr" @mousedown.stop="startResizeShapeItem(shape.id, 'mr', $event)"></div>
        <div class="shape-resize-handle bl" @mousedown.stop="startResizeShapeItem(shape.id, 'bl', $event)"></div>
        <div class="shape-resize-handle bm" @mousedown.stop="startResizeShapeItem(shape.id, 'bm', $event)"></div>
        <div class="shape-resize-handle br" @mousedown.stop="startResizeShapeItem(shape.id, 'br', $event)"></div>
      </template>
      <template v-if="selectedShapeId === shape.id && (shape.type === 'line' || shape.type === 'arrow')">
        <div
            :style="{ left: `${shape.x1}px`, top: `${shape.y1}px` }"
            class="shape-point-handle"
            @mousedown.stop="startAdjustLineEndpoint(shape.id, 'start', $event)"
        ></div>
        <div
            :style="{ left: `${shape.x2}px`, top: `${shape.y2}px` }"
            class="shape-point-handle"
            @mousedown.stop="startAdjustLineEndpoint(shape.id, 'end', $event)"
        ></div>
      </template>
    </div>

    <div
        v-for="item in textItems"
        :key="item.id"
        :class="{ editing: editingTextId === item.id, selected: selectedTextId === item.id }"
        :ref="(el) => setTextOverlayRef(el, item.id)"
        class="text-overlay-item"
        @mousedown.stop="startDragTextItem(item.id, $event)"
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
import {emit} from '@tauri-apps/api/event'
import {Check, Circle, Pin, Square, X} from 'lucide-vue-next'
import {
  Brush,
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
const pickerMagnifierCanvasRef = ref(null)
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
const pickColorRgb = ref('')
const pickerDisplayMode = ref('hex')
const pickerCopyHint = ref('Shift切换 RGB/# · Ctrl复制')
const textItems = ref([])
const shapeItems = ref([])
let textItemIdSeed = 1
let shapeItemIdSeed = 1
const editingTextId = ref(null)
const selectedTextId = ref(null)
const selectedShapeId = ref(null)
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
const regionConfirmAnchor = reactive({x: 0, y: 0, ready: false})

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
const regionSelectMode = ref('screenshot')
const screenshotRequestInFlight = ref(false)
const fallbackRequestedSessionIds = new Set()
let fallbackRequestedWithoutSession = false
let screenshotFallbackTimer = null

// 物理像素比例
const dpr = window.devicePixelRatio || 1

// 定义工具
const drawingTools = [
  {id: 'select', name: '框选/移动', icon: Pointer},
  {id: 'pen', name: '画笔', icon: EditPen},
  {id: 'line', name: '直线', icon: Minus},
  {id: 'arrow', name: '箭头', icon: TopRight},
  {id: 'rect', name: '矩形', icon: Square},
  {id: 'circle', name: '圆形', icon: Circle},
  {id: 'text', name: '文字', icon: Edit},
  {id: 'mosaic', name: '马赛克', icon: Grid},
  {id: 'picker', name: '取色', icon: Brush}
]

const hasSelection = computed(() => rect.width > 0 && rect.height > 0)
const canExport = computed(() => hasSelection.value)
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
  const panelWidth = 112
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
const pickerDisplayValue = computed(() => {
  return pickerDisplayMode.value === 'rgb' ? pickColorRgb.value : pickColor.value
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

let screenshotPixelCanvas = null
let screenshotPixelCtx = null
const movingTextStart = reactive({x: 0, y: 0, itemX: 0, itemY: 0, id: 0})
const movingShapeStart = reactive({x: 0, y: 0, itemX: 0, itemY: 0, id: 0})
const resizingShapeStart = reactive({x: 0, y: 0, itemX: 0, itemY: 0, itemWidth: 0, itemHeight: 0, handle: '', id: 0})
const adjustingLinePointStart = reactive({id: 0, point: 'start'})

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
  textItems.value = []
  shapeItems.value = []
  selectedTextId.value = null
  selectedShapeId.value = null
  editingTextId.value = null
  rect.x = 0
  rect.y = 0
  rect.width = 0
  rect.height = 0
  if (canvas.value) {
    canvas.value.width = 0
    canvas.value.height = 0
  }
  pickColor.value = ''
  pickColorRgb.value = ''
  pickerCopyHint.value = 'Shift切换 RGB/# · Ctrl复制'
  screenshotPixelCanvas = null
  screenshotPixelCtx = null
  screenshotRequestInFlight.value = false
  fallbackRequestedSessionIds.clear()
  fallbackRequestedWithoutSession = false
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
    const bootMode = String(boot.pendingMode || 'screenshot')
    handleStartRegionSelect({detail: {session_id: bootStartSessionId, mode: bootMode}})
    boot.pendingStartSessionId = 0
    boot.pendingMode = null
  }
}

function scheduleScreenshotFallback() {
  if (!screenshotSessionRequested.value || hasScreenshotPayload.value) return
  if (screenshotFallbackTimer) {
    window.clearTimeout(screenshotFallbackTimer)
  }
  const scheduledSessionId = activeSessionId.value
  screenshotFallbackTimer = window.setTimeout(async () => {
    if (scheduledSessionId > 0 && activeSessionId.value !== scheduledSessionId) {
      return
    }
    if (screenshotSessionRequested.value && !hasScreenshotPayload.value) {
      if (scheduledSessionId > 0) {
        if (fallbackRequestedSessionIds.has(scheduledSessionId)) {
          return
        }
        fallbackRequestedSessionIds.add(scheduledSessionId)
      } else {
        if (fallbackRequestedWithoutSession) {
          return
        }
        fallbackRequestedWithoutSession = true
      }
      const success = await requestScreenshot()
      if (!success) {
        if (scheduledSessionId > 0) {
          fallbackRequestedSessionIds.delete(scheduledSessionId)
        } else {
          fallbackRequestedWithoutSession = false
        }
      }
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
  if (screenshotRequestInFlight.value) {
    return false
  }
  screenshotRequestInFlight.value = true
  try {
    const result = await invoke('start_screenshot')
    if (result.success && result.png_base64) {
      hasScreenshotPayload.value = true
      captureOriginX.value = Number(result.origin_x) || 0
      captureOriginY.value = Number(result.origin_y) || 0
      await fetchWindows()
      loadImageFromBase64(result.png_base64)
      return true
    }
    return false
  } catch (error) {
    console.error('请求截图失败:', error)
    return false
  } finally {
    screenshotRequestInFlight.value = false
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
  regionSelectMode.value = String(event?.detail?.mode || 'screenshot')
  if (sessionId > 0) {
    fallbackRequestedSessionIds.delete(sessionId)
    activeSessionId.value = sessionId
    screenshotSessionRequested.value = true
    hasScreenshotPayload.value = payloadSessionId.value === sessionId
  } else {
    fallbackRequestedWithoutSession = false
    screenshotSessionRequested.value = true
  }
  scheduleScreenshotFallback()
  state.value = 'idle'
  currentTool.value = 'select'
  rect.width = 0
  rect.height = 0
  regionConfirmAnchor.ready = false
}

async function commitRecordingRegionSelection() {
  const payload = getGlobalSelectionRect()
  try {
    await invoke('notify_recording_region_selected', {payload})
  } catch (_e) {
    await emit('recording-region-selected', payload)
  }
  await close()
}

function getGlobalSelectionRect() {
  const viewportW = Math.max(1, Number(window.innerWidth) || 1)
  const viewportH = Math.max(1, Number(window.innerHeight) || 1)
  const imageW = Math.max(1, Number(screenshotImg.value?.width) || viewportW)
  const imageH = Math.max(1, Number(screenshotImg.value?.height) || viewportH)
  const scaleX = imageW / viewportW
  const scaleY = imageH / viewportH
  const rawX = Math.round(captureOriginX.value + rect.x * scaleX)
  const rawY = Math.round(captureOriginY.value + rect.y * scaleY)
  const rawW = Math.max(1, Math.round(rect.width * scaleX))
  const rawH = Math.max(1, Math.round(rect.height * scaleY))
  const minX = Math.round(captureOriginX.value)
  const minY = Math.round(captureOriginY.value)
  const maxX = minX + imageW
  const maxY = minY + imageH
  const x = Math.max(minX, Math.min(maxX - 1, rawX))
  const y = Math.max(minY, Math.min(maxY - 1, rawY))
  const width = Math.max(1, Math.min(maxX - x, rawW))
  const height = Math.max(1, Math.min(maxY - y, rawH))
  return {
    x,
    y,
    width,
    height,
  }
}

function loadImageFromBase64(base64Data) {
  isCaptureReady.value = false
  screenshotSrc.value = `data:image/png;base64,${base64Data}`
  const img = new Image()
  img.onload = () => {
    screenshotImg.value = img
    screenshotPixelCanvas = document.createElement('canvas')
    screenshotPixelCanvas.width = img.width
    screenshotPixelCanvas.height = img.height
    screenshotPixelCtx = screenshotPixelCanvas.getContext('2d')
    if (screenshotPixelCtx) {
      screenshotPixelCtx.drawImage(img, 0, 0)
    }
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
  if (state.value !== 'text-moving') {
    selectedTextId.value = null
  }
  if (state.value !== 'shape-moving') {
    selectedShapeId.value = null
  }
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
  } else if (state.value === 'text-moving') {
    const item = textItems.value.find((entry) => entry.id === movingTextStart.id)
    if (!item) return
    const dx = e.clientX - movingTextStart.x
    const dy = e.clientY - movingTextStart.y
    item.x = movingTextStart.itemX + dx
    item.y = movingTextStart.itemY + dy
  } else if (state.value === 'shape-moving') {
    const item = shapeItems.value.find((entry) => entry.id === movingShapeStart.id)
    if (!item) return
    const dx = e.clientX - movingShapeStart.x
    const dy = e.clientY - movingShapeStart.y
    item.x = movingShapeStart.itemX + dx
    item.y = movingShapeStart.itemY + dy
  } else if (state.value === 'shape-resizing') {
    handleResizeShapeItem(e)
  } else if (state.value === 'shape-point-moving') {
    handleAdjustLineEndpoint(e)
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
    if (regionSelectMode.value === 'recording_region' && state.value === 'selected' && rect.width > 0 && rect.height > 0) {
      regionConfirmAnchor.x = e.clientX + 8
      regionConfirmAnchor.y = e.clientY + 8
      regionConfirmAnchor.ready = true
    }
  } else if (state.value === 'moving' || state.value === 'resizing') {
    state.value = 'selected'
  } else if (state.value === 'text-moving') {
    state.value = 'selected'
    saveToHistory()
  } else if (state.value === 'shape-moving') {
    state.value = 'selected'
    saveToHistory()
  } else if (state.value === 'shape-resizing') {
    state.value = 'selected'
    saveToHistory()
  } else if (state.value === 'shape-point-moving') {
    state.value = 'selected'
    saveToHistory()
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
  selectedShapeId.value = null
  state.value = 'idle'
  rect.width = 0
  rect.height = 0
  regionConfirmAnchor.ready = false
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
  selectedShapeId.value = null
  if (toolId !== 'picker') {
    pickerCopyHint.value = 'Shift切换 RGB/# · Ctrl复制'
  }
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

  if (['line', 'arrow', 'rect', 'circle'].includes(currentTool.value)) {
    if (currentDrawingSnapshot && ctx) {
      const oldTransform = ctx.getTransform()
      ctx.resetTransform()
      ctx.putImageData(currentDrawingSnapshot, 0, 0)
      ctx.setTransform(oldTransform)
      currentDrawingSnapshot = null
    }
    createShapeItem(currentTool.value, drawStart.x, drawStart.y, x, y)
    currentTool.value = 'select'
    saveToHistory()
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

function startDragTextItem(id, event) {
  if (editingTextId.value !== null) return
  selectedTextId.value = id
  selectedShapeId.value = null
  const item = textItems.value.find((entry) => entry.id === id)
  if (!item) return
  movingTextStart.x = event.clientX
  movingTextStart.y = event.clientY
  movingTextStart.itemX = item.x
  movingTextStart.itemY = item.y
  movingTextStart.id = id
  state.value = 'text-moving'
}

function startDragShapeItem(id, event) {
  if (editingTextId.value !== null) return
  currentTool.value = 'select'
  selectedShapeId.value = id
  selectedTextId.value = null
  const item = shapeItems.value.find((entry) => entry.id === id)
  if (!item) return
  movingShapeStart.x = event.clientX
  movingShapeStart.y = event.clientY
  movingShapeStart.itemX = item.x
  movingShapeStart.itemY = item.y
  movingShapeStart.id = id
  state.value = 'shape-moving'
}

function startResizeShapeItem(id, handle, event) {
  if (editingTextId.value !== null) return
  const item = shapeItems.value.find((entry) => entry.id === id)
  if (!item || (item.type !== 'rect' && item.type !== 'circle')) return
  selectedShapeId.value = id
  selectedTextId.value = null
  resizingShapeStart.x = event.clientX
  resizingShapeStart.y = event.clientY
  resizingShapeStart.itemX = item.x
  resizingShapeStart.itemY = item.y
  resizingShapeStart.itemWidth = item.width
  resizingShapeStart.itemHeight = item.height
  resizingShapeStart.handle = handle
  resizingShapeStart.id = id
  state.value = 'shape-resizing'
}

function handleResizeShapeItem(event) {
  const item = shapeItems.value.find((entry) => entry.id === resizingShapeStart.id)
  if (!item) return
  const dx = event.clientX - resizingShapeStart.x
  const dy = event.clientY - resizingShapeStart.y
  let x = resizingShapeStart.itemX
  let y = resizingShapeStart.itemY
  let width = resizingShapeStart.itemWidth
  let height = resizingShapeStart.itemHeight
  const handle = resizingShapeStart.handle
  if (handle.includes('l')) {
    x += dx
    width -= dx
  }
  if (handle.includes('r')) {
    width += dx
  }
  if (handle.includes('t')) {
    y += dy
    height -= dy
  }
  if (handle.includes('b')) {
    height += dy
  }
  const minSize = 4
  if (width < minSize) {
    if (handle.includes('l')) {
      x = resizingShapeStart.itemX + resizingShapeStart.itemWidth - minSize
    }
    width = minSize
  }
  if (height < minSize) {
    if (handle.includes('t')) {
      y = resizingShapeStart.itemY + resizingShapeStart.itemHeight - minSize
    }
    height = minSize
  }
  item.x = x
  item.y = y
  item.width = width
  item.height = height
}

function startAdjustLineEndpoint(id, point, event) {
  if (editingTextId.value !== null) return
  const item = shapeItems.value.find((entry) => entry.id === id)
  if (!item || (item.type !== 'line' && item.type !== 'arrow')) return
  selectedShapeId.value = id
  selectedTextId.value = null
  adjustingLinePointStart.id = id
  adjustingLinePointStart.point = point
  state.value = 'shape-point-moving'
}

function handleAdjustLineEndpoint(event) {
  const item = shapeItems.value.find((entry) => entry.id === adjustingLinePointStart.id)
  if (!item || (item.type !== 'line' && item.type !== 'arrow')) return
  const startAbs = {
    x: item.x + item.x1,
    y: item.y + item.y1
  }
  const endAbs = {
    x: item.x + item.x2,
    y: item.y + item.y2
  }
  if (adjustingLinePointStart.point === 'start') {
    startAbs.x = event.clientX
    startAbs.y = event.clientY
  } else {
    endAbs.x = event.clientX
    endAbs.y = event.clientY
  }
  updateLineLikeShapeFromAbsolutePoints(item, startAbs, endAbs)
}

function updateLineLikeShapeFromAbsolutePoints(item, startAbs, endAbs) {
  const minX = Math.min(startAbs.x, endAbs.x)
  const minY = Math.min(startAbs.y, endAbs.y)
  item.x = minX
  item.y = minY
  item.width = Math.max(1, Math.abs(endAbs.x - startAbs.x))
  item.height = Math.max(1, Math.abs(endAbs.y - startAbs.y))
  item.x1 = startAbs.x - minX
  item.y1 = startAbs.y - minY
  item.x2 = endAbs.x - minX
  item.y2 = endAbs.y - minY
  if (item.type === 'arrow') {
    const arrowHead = getArrowHeadPoints(item.x1, item.y1, item.x2, item.y2)
    item.arrowLeft = arrowHead.left
    item.arrowRight = arrowHead.right
  }
}

function createShapeItem(type, fromX, fromY, toX, toY) {
  const dx = toX - fromX
  const dy = toY - fromY
  if (Math.abs(dx) < 2 && Math.abs(dy) < 2) return
  const stroke = Math.max(1, Number(lineWidth.value) || 1)
  if (type === 'rect') {
    shapeItems.value.push({
      id: shapeItemIdSeed++,
      type,
      x: Math.min(fromX, toX),
      y: Math.min(fromY, toY),
      width: Math.max(2, Math.abs(dx)),
      height: Math.max(2, Math.abs(dy)),
      color: currentColor.value,
      lineWidth: stroke
    })
    return
  }
  if (type === 'circle') {
    const radius = Math.sqrt(dx * dx + dy * dy)
    shapeItems.value.push({
      id: shapeItemIdSeed++,
      type,
      x: fromX - radius,
      y: fromY - radius,
      width: Math.max(2, radius * 2),
      height: Math.max(2, radius * 2),
      color: currentColor.value,
      lineWidth: stroke
    })
    return
  }
  const minX = Math.min(fromX, toX)
  const minY = Math.min(fromY, toY)
  const width = Math.max(2, Math.abs(dx))
  const height = Math.max(2, Math.abs(dy))
  const x1 = fromX - minX
  const y1 = fromY - minY
  const x2 = toX - minX
  const y2 = toY - minY
  const arrowHead = getArrowHeadPoints(x1, y1, x2, y2)
  shapeItems.value.push({
    id: shapeItemIdSeed++,
    type,
    x: minX,
    y: minY,
    width,
    height,
    x1,
    y1,
    x2,
    y2,
    color: currentColor.value,
    lineWidth: stroke,
    arrowLeft: arrowHead.left,
    arrowRight: arrowHead.right
  })
}

function getArrowHeadPoints(fromX, fromY, toX, toY) {
  const angle = Math.atan2(toY - fromY, toX - fromX)
  const headLength = 12
  return {
    left: {
      x: toX - headLength * Math.cos(angle - Math.PI / 6),
      y: toY - headLength * Math.sin(angle - Math.PI / 6)
    },
    right: {
      x: toX - headLength * Math.cos(angle + Math.PI / 6),
      y: toY - headLength * Math.sin(angle + Math.PI / 6)
    }
  }
}

function getShapeItemStyle(shape) {
  return {
    left: `${shape.x}px`,
    top: `${shape.y}px`,
    width: `${shape.width}px`,
    height: `${shape.height}px`
  }
}

function getShapeStrokeStyle(shape) {
  return {
    borderColor: shape.color,
    borderWidth: `${shape.lineWidth}px`
  }
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
  const px = Math.round(event.clientX * dpr)
  const py = Math.round(event.clientY * dpr)
  const color = getMergedPixelColorAt(px, py)
  if (!color) return
  const hex = '#' + [color.r, color.g, color.b].map(v => v.toString(16).padStart(2, '0')).join('')
  pickColor.value = hex.toUpperCase()
  pickColorRgb.value = `RGB(${color.r}, ${color.g}, ${color.b})`
  if (isDrawing.value) {
    currentColor.value = pickColor.value
  }
  renderPickerMagnifier(px, py)
}

function getMergedPixelColorAt(px, py) {
  const drawCanvas = canvas.value
  if (!drawCanvas || !screenshotPixelCtx || !drawCanvas.width || !drawCanvas.height) return null
  if (px < 0 || py < 0 || px >= drawCanvas.width || py >= drawCanvas.height) return null
  const baseData = screenshotPixelCtx.getImageData(px, py, 1, 1).data
  const drawCtx = drawCanvas.getContext('2d')
  if (!drawCtx) return {r: baseData[0], g: baseData[1], b: baseData[2]}
  const oldTransform = drawCtx.getTransform()
  drawCtx.resetTransform()
  const overlayData = drawCtx.getImageData(px, py, 1, 1).data
  drawCtx.setTransform(oldTransform)
  if (overlayData[3] > 0) {
    return {r: overlayData[0], g: overlayData[1], b: overlayData[2]}
  }
  return {r: baseData[0], g: baseData[1], b: baseData[2]}
}

function renderPickerMagnifier(px, py) {
  if (!pickerMagnifierCanvasRef.value || !canvas.value || !screenshotPixelCtx) return
  const magnifierCanvas = pickerMagnifierCanvasRef.value
  const magnifierCtx = magnifierCanvas.getContext('2d')
  if (!magnifierCtx) return
  const sampleSize = 11
  const zoom = 12
  const half = Math.floor(sampleSize / 2)
  magnifierCanvas.width = sampleSize * zoom
  magnifierCanvas.height = sampleSize * zoom
  magnifierCtx.clearRect(0, 0, magnifierCanvas.width, magnifierCanvas.height)
  for (let row = 0; row < sampleSize; row++) {
    for (let col = 0; col < sampleSize; col++) {
      const sx = px + col - half
      const sy = py + row - half
      const color = getMergedPixelColorAt(sx, sy)
      const fill = color ? `rgb(${color.r}, ${color.g}, ${color.b})` : 'rgba(0,0,0,0.2)'
      magnifierCtx.fillStyle = fill
      magnifierCtx.fillRect(col * zoom, row * zoom, zoom, zoom)
    }
  }
  magnifierCtx.strokeStyle = 'rgba(255,255,255,0.35)'
  magnifierCtx.lineWidth = 1
  for (let i = 0; i <= sampleSize; i++) {
    const p = i * zoom
    magnifierCtx.beginPath()
    magnifierCtx.moveTo(p, 0)
    magnifierCtx.lineTo(p, magnifierCanvas.height)
    magnifierCtx.stroke()
    magnifierCtx.beginPath()
    magnifierCtx.moveTo(0, p)
    magnifierCtx.lineTo(magnifierCanvas.width, p)
    magnifierCtx.stroke()
  }
  magnifierCtx.strokeStyle = '#00aaff'
  magnifierCtx.lineWidth = 2
  magnifierCtx.strokeRect(half * zoom + 1, half * zoom + 1, zoom - 2, zoom - 2)
}

async function copyPickedColor() {
  if (!pickColor.value) return
  const value = pickerDisplayValue.value || pickColor.value
  try {
    await invoke('copy_text', {text: value})
    pickerCopyHint.value = `已复制：${value}`
    window.setTimeout(() => {
      if (currentTool.value === 'picker') {
        pickerCopyHint.value = 'Shift切换 RGB/# · Ctrl复制'
      }
    }, 1000)
  } catch (error) {
    pickerCopyHint.value = '复制失败，请重试'
  }
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
  const shapeSnapshot = shapeItems.value.map(item => ({...item}))

  history.value = history.value.slice(0, historyIndex.value + 1)
  history.value.push({
    imageData,
    textItems: textSnapshot,
    shapeItems: shapeSnapshot
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
  shapeItems.value = (snapshot.shapeItems || []).map(item => ({...item}))
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
  drawShapeItemsOnCroppedCanvas(ctx, sourceX, sourceY, sourceWidth, sourceHeight)
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
  ctx.scale(dpr, dpr)
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

function drawShapeItemsOnCroppedCanvas(ctx, sourceX, sourceY, sourceWidth, sourceHeight) {
  const cropLeft = sourceX / dpr
  const cropTop = sourceY / dpr
  const cropRight = cropLeft + sourceWidth / dpr
  const cropBottom = cropTop + sourceHeight / dpr
  for (const item of shapeItems.value) {
    const itemRight = item.x + item.width
    const itemBottom = item.y + item.height
    if (itemRight < cropLeft || itemBottom < cropTop || item.x > cropRight || item.y > cropBottom) continue
    drawShapeItemToContext(ctx, item, cropLeft, cropTop)
  }
}

function drawShapeItemToContext(ctx, item, cropLeft, cropTop) {
  const x = item.x - cropLeft
  const y = item.y - cropTop
  ctx.save()
  ctx.scale(dpr, dpr)
  ctx.strokeStyle = item.color
  ctx.lineWidth = item.lineWidth
  ctx.lineCap = 'round'
  if (item.type === 'rect') {
    ctx.strokeRect(x, y, item.width, item.height)
    ctx.restore()
    return
  }
  if (item.type === 'circle') {
    ctx.beginPath()
    ctx.ellipse(
        x + item.width / 2,
        y + item.height / 2,
        item.width / 2,
        item.height / 2,
        0,
        0,
        Math.PI * 2
    )
    ctx.stroke()
    ctx.restore()
    return
  }
  ctx.beginPath()
  ctx.moveTo(x + item.x1, y + item.y1)
  ctx.lineTo(x + item.x2, y + item.y2)
  ctx.stroke()
  if (item.type === 'arrow') {
    ctx.beginPath()
    ctx.moveTo(x + item.x2, y + item.y2)
    ctx.lineTo(x + item.arrowLeft.x, y + item.arrowLeft.y)
    ctx.moveTo(x + item.x2, y + item.y2)
    ctx.lineTo(x + item.arrowRight.x, y + item.arrowRight.y)
    ctx.stroke()
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
  if (regionSelectMode.value === 'recording_region' && event.key === 'Enter') {
    event.preventDefault()
    if (hasSelection.value && state.value === 'selected') {
      commitRecordingRegionSelection()
    }
    return
  }
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
  } else if (currentTool.value === 'picker' && event.key === 'Shift' && !event.repeat) {
    event.preventDefault()
    pickerDisplayMode.value = pickerDisplayMode.value === 'hex' ? 'rgb' : 'hex'
  } else if (currentTool.value === 'picker' && event.key === 'Control' && !event.repeat) {
    event.preventDefault()
    copyPickedColor()
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

.recording-region-confirm {
  position: fixed;
  display: flex;
  gap: 6px;
  z-index: 2100;
}

.region-icon-btn {
  width: 32px;
  height: 32px;
  border: 1px solid rgba(255, 255, 255, 0.24);
  background: rgba(22, 26, 36, 0.76);
  color: #edf2ff;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  cursor: pointer;
  backdrop-filter: blur(2px);
}

.region-icon-btn:hover {
  background: rgba(46, 54, 72, 0.88);
}

.region-icon-btn.primary {
  background: rgba(64, 158, 255, 0.22);
  border-color: rgba(64, 158, 255, 0.72);
  color: #fff;
}

.region-icon-btn.danger {
  background: rgba(245, 108, 108, 0.15);
  border-color: rgba(245, 108, 108, 0.52);
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
  background: rgba(17, 24, 39, 0.95);
  color: #e5e7eb;
  padding: 8px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.15);
  font-size: 12px;
  z-index: 1000;
  pointer-events: none;
  display: flex;
  gap: 8px;
  align-items: center;
}

.picker-magnifier {
  width: 132px;
  height: 132px;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  image-rendering: pixelated;
}

.picker-meta {
  min-width: 130px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.picker-swatch {
  width: 100%;
  height: 22px;
  border-radius: 4px;
  border: 1px solid rgba(255, 255, 255, 0.25);
}

.picker-value {
  font-size: 14px;
  font-weight: 700;
  letter-spacing: 0.2px;
}

.picker-hint {
  font-size: 11px;
  color: #9ca3af;
}

.shape-overlay-item {
  position: absolute;
  z-index: 1080;
  cursor: move;
  user-select: none;
}

.shape-overlay-item.selected {
  box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.45);
}

.shape-rect,
.shape-circle {
  width: 100%;
  height: 100%;
  border-style: solid;
  box-sizing: border-box;
}

.shape-circle {
  border-radius: 50%;
}

.shape-line-svg {
  display: block;
  overflow: visible;
}

.shape-resize-handle {
  position: absolute;
  width: 10px;
  height: 10px;
  background: #00aaff;
  border: 1px solid #ffffff;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  z-index: 5;
}

.shape-resize-handle.tl {
  top: 0;
  left: 0;
  cursor: nwse-resize;
}

.shape-resize-handle.tm {
  top: 0;
  left: 50%;
  cursor: ns-resize;
}

.shape-resize-handle.tr {
  top: 0;
  left: 100%;
  cursor: nesw-resize;
}

.shape-resize-handle.ml {
  top: 50%;
  left: 0;
  cursor: ew-resize;
}

.shape-resize-handle.mr {
  top: 50%;
  left: 100%;
  cursor: ew-resize;
}

.shape-resize-handle.bl {
  top: 100%;
  left: 0;
  cursor: nesw-resize;
}

.shape-resize-handle.bm {
  top: 100%;
  left: 50%;
  cursor: ns-resize;
}

.shape-resize-handle.br {
  top: 100%;
  left: 100%;
  cursor: nwse-resize;
}

.shape-point-handle {
  position: absolute;
  width: 12px;
  height: 12px;
  background: #00aaff;
  border: 1px solid #ffffff;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  z-index: 6;
  cursor: move;
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
