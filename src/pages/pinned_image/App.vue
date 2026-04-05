<template>
  <div ref="rootRef" class="pinned-image-root"
       @mousedown.left="handleRootMouseDown"
       @dblclick.left.stop.prevent="closeWindow"
       @contextmenu.prevent="handleContextMenu">
    <img v-if="imageSrc" ref="imageRef" :src="imageSrc" alt="" class="pinned-image" draggable="false"
         @load="handleImageLoaded"/>
    <div v-if="ocrEnabled && isRecognizing" class="ocr-status">识别中...</div>
    <div v-else-if="ocrStatusMessage" class="ocr-status ocr-status-error">{{ ocrStatusMessage }}</div>
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
const ocrStatusMessage = ref('')
const currentPngBase64 = ref('')
const currentImageWidth = ref(0)
const currentImageHeight = ref(0)
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
  ocrStatusMessage.value = '右键开始识别文字'
  currentPngBase64.value = detail.png_base64
  currentImageWidth.value = width
  currentImageHeight.value = height
  imageSrc.value = `data:image/png;base64,${detail.png_base64}`
}

function handlePinnedImageData(event) {
  applyPinnedPayload(event?.detail)
}

async function closeWindow() {
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
  startDrag()
}

async function handleContextMenu() {
  if (isRecognizing.value) return
  if (!currentPngBase64.value) return
  const lines = await runOcr(currentPngBase64.value, currentImageWidth.value, currentImageHeight.value)
  if (!lines.length) return
  const text = lines.map(line => (line?.text || '').trim()).filter(Boolean).join('\n').trim()
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

async function runOcr(base64, width, height) {
  if (!ocrEnabled.value || !base64) return []
  const taskId = ++ocrTaskId
  isRecognizing.value = true
  ocrStatusMessage.value = ''
  try {
    const result = await invoke('recognize_image_ocr', {pngBase64: base64})
    if (taskId !== ocrTaskId) return []
    if (!result?.success) {
      ocrStatusMessage.value = result?.error || '本地OCR识别失败'
      return []
    }
    const lines = normalizeNativeOcrParagraphs(result?.paragraphs, taskId)
    if (!lines.length) {
      ocrStatusMessage.value = '未识别到文字'
    }
    if (width > 0 && height > 0) {
      sourceWidth.value = width
      sourceHeight.value = height
    }
    if (lines.length) {
      ocrStatusMessage.value = '识别完成'
    }
    return lines
  } catch (error) {
    if (taskId !== ocrTaskId) return []
    ocrStatusMessage.value = 'OCR 初始化失败'
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

.ocr-status {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 3;
  background: rgba(0, 0, 0, 0.6);
  color: #fff;
  font-size: 12px;
  padding: 3px 8px;
  border-radius: 6px;
  pointer-events: none;
}

.ocr-status.ocr-status-error {
  background: rgba(120, 0, 0, 0.75);
}
</style>
