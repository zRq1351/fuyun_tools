<template>
  <div ref="rootRef" class="pinned-image-root"
       @mousedown.left="handleRootMouseDown"
       @dblclick.left.stop.prevent="closeWindow"
       @contextmenu.prevent="handleContextMenu"
       @click="hideContextMenu">
    <img v-if="imageSrc" ref="imageRef" :src="imageSrc" alt="" class="pinned-image" draggable="false"
         @load="handleImageLoaded"/>

    <!-- 透明实况文本层 -->
    <div class="ocr-text-overlay" v-if="!isRecognizing && ocrLines.length > 0">
      <span 
        v-for="item in ocrLines" 
        :key="item.id"
        class="selectable-text"
        :style="{
          left: (item.x0 / sourceWidth * 100) + '%',
          top: (item.y0 / sourceHeight * 100) + '%',
          width: ((item.x1 - item.x0) / sourceWidth * 100) + '%',
          height: ((item.y1 - item.y0) / sourceHeight * 100) + '%',
          fontSize: ((item.y1 - item.y0) / sourceHeight * 100 * 0.8) + 'vh',
          lineHeight: ((item.y1 - item.y0) / sourceHeight * 100) + 'vh'
        }"
        @mousedown.stop
      >
        {{ item.text }}
      </span>
    </div>

    <!-- 扫描线动画 -->
    <div v-if="ocrEnabled && isRecognizing" class="ocr-scanner"></div>

    <!-- 轻量级错误提示 -->
    <div v-if="toastMessage" class="ocr-toast" :class="{'ocr-toast-error': toastIsError}">
      {{ toastMessage }}
    </div>

    <!-- 自定义右键菜单 -->
    <div v-if="contextMenu.show" class="context-menu" :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }" @mousedown.stop>
      <div class="menu-item" @click="copyAllText">复制全部文字</div>
      <div class="menu-item" @click="openTextWindow">在独立窗口查看</div>
      <div class="menu-divider"></div>
      <div class="menu-item" @click="retryWithOcrRs">高精度重新识别</div>
      <div class="menu-divider"></div>
      <div class="menu-item" @click="closeWindow">关闭贴图</div>
    </div>
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {PhysicalSize} from '@tauri-apps/api/window'

const imageSrc = ref('')
const windowLabel = ref('')
const aspectRatio = ref(1)
const isResizingProgrammatically = ref(false)
const pendingResize = ref(null)
const rootRef = ref(null)
const imageRef = ref(null)
const isRecognizing = ref(false)
const ocrEnabled = ref(true)
const sourceWidth = ref(0)
const sourceHeight = ref(0)

const currentPngBase64 = ref('')
const currentImageWidth = ref(0)
const currentImageHeight = ref(0)

const ocrLines = ref([])

// Toast
const toastMessage = ref('')
const toastIsError = ref(false)
let toastTimer = null

function showToast(msg, isError = false) {
  toastMessage.value = msg
  toastIsError.value = isError
  if (toastTimer) clearTimeout(toastTimer)
  toastTimer = setTimeout(() => {
    toastMessage.value = ''
  }, 3000)
}

// Context Menu
const contextMenu = ref({ show: false, x: 0, y: 0 })

function handleContextMenu(event) {
  let x = event.clientX
  let y = event.clientY
  
  if (x + 140 > window.innerWidth) x = window.innerWidth - 140
  if (y + 120 > window.innerHeight) y = window.innerHeight - 120
  
  contextMenu.value = {
    show: true,
    x,
    y
  }
}

function hideContextMenu() {
  contextMenu.value.show = false
}

let unlistenResized = null
let resizeDebounceTimer = null
let lastDragStartAt = 0
let ocrTaskId = 0

function applyPinnedPayload(detail) {
  if (!detail?.png_base64) return
  if (detail?.label) {
    windowLabel.value = detail.label
  }
  const width = Number(detail?.width) || 0
  const height = Number(detail?.height) || 0
  if (width > 0 && height > 0) {
    aspectRatio.value = width / height
    sourceWidth.value = width
    sourceHeight.value = height
  }
  currentPngBase64.value = detail.png_base64
  currentImageWidth.value = width
  currentImageHeight.value = height
  imageSrc.value = `data:image/png;base64,${detail.png_base64}`

  // 静默触发OCR
  ocrLines.value = []
  setTimeout(() => {
    runOcr(detail.png_base64, width, height)
  }, 100)
}

function handlePinnedImageData(event) {
  applyPinnedPayload(event?.detail)
}

async function closeWindow() {
  hideContextMenu()
  try {
    if (windowLabel.value) {
      await invoke('close_pinned_image_window', {label: windowLabel.value})
      return
    }
    window.close()
  } catch (error) {
    console.error('关闭固定窗口失败:', error)
  }
}

async function startDrag() {
  const now = Date.now()
  if (now - lastDragStartAt < 220) return
  lastDragStartAt = now
  try {
    await getCurrentWebviewWindow().startDragging()
  } catch (error) {
    console.error('系统拖动固定窗口失败:', error)
  }
}

function handleRootMouseDown(event) {
  hideContextMenu()
  startDrag()
}

async function copyAllText() {
  hideContextMenu()
  if (isRecognizing.value) {
    showToast('正在识别中，请稍候...', false)
    return
  }
  if (!ocrLines.value.length) {
    showToast('未识别到文字', true)
    return
  }
  const text = ocrLines.value.map(line => (line?.text || '').trim()).filter(Boolean).join('\n').trim()
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    showToast('文字已复制', false)
  } catch (e) {
    console.error('复制失败', e)
    showToast('复制失败', true)
  }
}

async function openTextWindow() {
  hideContextMenu()
  if (isRecognizing.value) {
    showToast('正在识别中，请稍候...', false)
    return
  }
  if (!ocrLines.value.length) {
    showToast('未识别到文字', true)
    return
  }
  const text = ocrLines.value.map(line => (line?.text || '').trim()).filter(Boolean).join('\n').trim()
  if (!text) return
  try {
    await invoke('show_ocr_text_window', {
      sourceLabel: windowLabel.value || 'pinned_image_window',
      text
    })
  } catch (error) {
    console.error('显示OCR文本窗口失败:', error)
  }
}

async function retryWithOcrRs() {
  hideContextMenu()
  if (isRecognizing.value) {
    showToast('正在识别中，请稍候...', false)
    return
  }
  if (!currentPngBase64.value) return
  showToast('正在使用高精度引擎重新识别...', false)
  await runOcr(currentPngBase64.value, currentImageWidth.value, currentImageHeight.value, 'ocr-rs')
}

function handleImageLoaded(event) {
  const naturalWidth = Number(event?.target?.naturalWidth) || 0
  const naturalHeight = Number(event?.target?.naturalHeight) || 0
  if (naturalWidth > 0 && naturalHeight > 0) {
    sourceWidth.value = naturalWidth
    sourceHeight.value = naturalHeight
  }
}

function normalizeNativeOcrParagraphs(paragraphs, taskId) {
  return (paragraphs || [])
      .map((paragraph, index) => {
        const text = String(paragraph?.text || '').trim()
        if (!text) return null
        const x0 = Number(paragraph?.x0) || 0
        const y0 = Number(paragraph?.y0) || 0
        const x1 = Number(paragraph?.x1) || x0 + Math.max(8, text.length * 10)
        const y1 = Number(paragraph?.y1) || y0 + 20
        return {
          id: `${taskId}-p-${index}`,
          text,
          x0,
          y0,
          x1: Math.max(x0 + 1, x1),
          y1: Math.max(y0 + 1, y1)
        }
      })
      .filter(Boolean)
}

async function runOcr(base64, width, height, engine = null) {
  if (!ocrEnabled.value || !base64) return []
  const taskId = ++ocrTaskId
  isRecognizing.value = true
  toastMessage.value = ''
  try {
    const payload = { pngBase64: base64 }
    if (engine) {
      payload.engine = engine
    }
    const result = await invoke('recognize_image_ocr', payload)
    if (taskId !== ocrTaskId) return []
    if (!result?.success) {
      showToast(result?.error || '本地OCR识别失败', true)
      return []
    }
    const lines = normalizeNativeOcrParagraphs(result?.paragraphs, taskId)
    if (!lines.length) {
      showToast('未识别到文字', true)
    }
    if (width > 0 && height > 0) {
      sourceWidth.value = width
      sourceHeight.value = height
    }
    ocrLines.value = lines
    return lines
  } catch (error) {
    if (taskId !== ocrTaskId) return []
    showToast('OCR 初始化失败', true)
    console.error('固定窗口OCR识别失败:', error)
    return []
  } finally {
    if (taskId === ocrTaskId) {
      isRecognizing.value = false
    }
  }
}

onMounted(() => {
  window.addEventListener('pinned-image-data', handlePinnedImageData)
  const cached = window.__PINNED_IMAGE_PAYLOAD__
  if (cached) {
    applyPinnedPayload(cached)
  }
  const currentWindow = getCurrentWebviewWindow()
  currentWindow.onResized(async ({payload}) => {
    const width = Number(payload?.width) || 0
    const height = Number(payload?.height) || 0
    if (width <= 0 || height <= 0) return
    if (isResizingProgrammatically.value) {
      isResizingProgrammatically.value = false
      return
    }
    pendingResize.value = {width, height}
    if (resizeDebounceTimer) {
      clearTimeout(resizeDebounceTimer)
      resizeDebounceTimer = null
    }
    resizeDebounceTimer = setTimeout(async () => {
      const next = pendingResize.value
      pendingResize.value = null
      if (!next) return
      const ratio = aspectRatio.value
      if (!ratio || !isFinite(ratio) || ratio <= 0) return
      const fitByWidth = {
        width: next.width,
        height: Math.max(1, Math.round(next.width / ratio))
      }
      const fitByHeight = {
        width: Math.max(1, Math.round(next.height * ratio)),
        height: next.height
      }
      const widthModeError = Math.abs(fitByWidth.height - next.height)
      const heightModeError = Math.abs(fitByHeight.width - next.width)
      const targetWidth = widthModeError <= heightModeError ? fitByWidth.width : fitByHeight.width
      const targetHeight = widthModeError <= heightModeError ? fitByWidth.height : fitByHeight.height
      const needAdjust = Math.abs(targetWidth - next.width) > 1 || Math.abs(targetHeight - next.height) > 1
      if (needAdjust) {
        isResizingProgrammatically.value = true
        try {
          await currentWindow.setSize(new PhysicalSize(targetWidth, targetHeight))
        } catch (error) {
          console.error('固定窗口等比缩放失败:', error)
          isResizingProgrammatically.value = false
        }
      }
    }, 220)
  }).then(unlisten => {
    unlistenResized = unlisten
  }).catch(() => {
  })
})

onUnmounted(() => {
  window.removeEventListener('pinned-image-data', handlePinnedImageData)
  if (resizeDebounceTimer) {
    clearTimeout(resizeDebounceTimer)
    resizeDebounceTimer = null
  }
  if (unlistenResized) {
    unlistenResized()
    unlistenResized = null
  }
})
</script>

<style scoped>
:global(html),
:global(body),
:global(#app) {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden !important;
  overflow-x: hidden !important;
  overflow-y: hidden !important;
  scrollbar-width: none;
  -ms-overflow-style: none;
}

:global(*),
:global(*::before),
:global(*::after) {
  scrollbar-width: none;
}

:global(*::-webkit-scrollbar) {
  width: 0 !important;
  height: 0 !important;
  display: none !important;
}

.pinned-image-root {
  box-sizing: border-box;
  position: fixed;
  inset: 0;
  overflow: clip;
  cursor: move;
}

.pinned-image-root:active {
  cursor: grabbing;
}

.pinned-image {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: fill;
  object-position: left top;
  user-select: none;
  cursor: inherit;
}

/* 扫描线动画 */
.ocr-scanner {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 2px;
  background: #0a84ff;
  box-shadow: 0 0 10px #0a84ff, 0 0 20px #0a84ff;
  animation: scan 1.5s infinite linear;
  pointer-events: none;
  z-index: 2;
}
@keyframes scan {
  0% { top: 0; opacity: 0; }
  10% { opacity: 1; }
  90% { opacity: 1; }
  100% { top: 100%; opacity: 0; }
}

/* 轻量级提示 Toast */
.ocr-toast {
  position: absolute;
  top: 12px;
  right: 12px;
  z-index: 10;
  background: rgba(0, 0, 0, 0.65);
  color: #fff;
  font-size: 13px;
  padding: 6px 12px;
  border-radius: 8px;
  pointer-events: none;
  backdrop-filter: blur(4px);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  transition: opacity 0.3s;
}
.ocr-toast-error {
  background: rgba(220, 38, 38, 0.85);
}

/* 透明实况文本层 */
.ocr-text-overlay {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  z-index: 1;
}

.selectable-text {
  position: absolute;
  color: transparent;
  user-select: text;
  cursor: text;
  white-space: nowrap;
  pointer-events: auto;
  display: flex;
  align-items: center;
  overflow: hidden;
}

.selectable-text::selection {
  background: rgba(10, 132, 255, 0.4);
  color: transparent;
}

/* 自定义右键菜单 */
.context-menu {
  position: absolute;
  z-index: 20;
  background: #2c2c2e;
  border: 1px solid #3a3a3c;
  border-radius: 6px;
  padding: 4px 0;
  min-width: 140px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  color: #fff;
  font-size: 13px;
}

.menu-item {
  padding: 8px 12px;
  cursor: pointer;
  transition: background 0.15s;
}

.menu-item:hover {
  background: #0a84ff;
}

.menu-divider {
  height: 1px;
  background: #3a3a3c;
  margin: 4px 0;
}
</style>