<template>
  <div ref="rootRef" class="pinned-image-root"
       @mousedown.left="handleRootMouseDown"
       @dblclick.stop.prevent="ignoreDoubleClick"
       @contextmenu.prevent="closeWindow">
    <img v-if="imageSrc" ref="imageRef" :src="imageSrc" alt="" class="pinned-image" draggable="false"
         @load="handleImageLoaded"/>
    <div v-if="ocrEnabled && ocrLines.length" class="ocr-text-layer">
      <span
          v-for="line in ocrLines"
          :key="line.id"
          :style="getLineStyle(line)"
          class="ocr-line"
          @mousedown.stop
      >
        {{ line.text }}
      </span>
    </div>
    <div v-if="ocrEnabled && isRecognizing" class="ocr-status">识别中...</div>
    <div v-else-if="ocrStatusMessage" class="ocr-status ocr-status-error">{{ ocrStatusMessage }}</div>
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {PhysicalSize} from '@tauri-apps/api/window'
import {createWorker} from 'tesseract.js'

const imageSrc = ref('')
const windowLabel = ref('')
const aspectRatio = ref(1)
const isResizingProgrammatically = ref(false)
const pendingResize = ref(null)
const rootRef = ref(null)
const imageRef = ref(null)
const ocrLines = ref([])
const isRecognizing = ref(false)
const ocrEnabled = ref(true)
const sourceWidth = ref(0)
const sourceHeight = ref(0)
const renderBox = ref({left: 0, top: 0, width: 0, height: 0, scale: 1})
const ocrStatusMessage = ref('')
let unlistenResized = null
let resizeDebounceTimer = null
let lastDragStartAt = 0
let resizeObserver = null
let ocrTaskId = 0
let ocrWorker = null

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
  ocrLines.value = []
  ocrStatusMessage.value = ''
  imageSrc.value = `data:image/png;base64,${detail.png_base64}`
  runOcr(detail.png_base64, width, height)
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
  if (event?.target?.closest?.('.ocr-line')) {
    return
  }
  startDrag()
}

function ignoreDoubleClick() {
  return
}

function handleImageLoaded(event) {
  const naturalWidth = Number(event?.target?.naturalWidth) || 0
  const naturalHeight = Number(event?.target?.naturalHeight) || 0
  if (naturalWidth > 0 && naturalHeight > 0) {
    sourceWidth.value = naturalWidth
    sourceHeight.value = naturalHeight
  }
  updateRenderBox()
}

function updateRenderBox() {
  const root = rootRef.value
  const srcW = sourceWidth.value
  const srcH = sourceHeight.value
  if (!root || !srcW || !srcH) return
  const containerW = root.clientWidth || 0
  const containerH = root.clientHeight || 0
  if (!containerW || !containerH) return
  const scale = Math.min(containerW / srcW, containerH / srcH)
  const width = srcW * scale
  const height = srcH * scale
  renderBox.value = {
    left: (containerW - width) / 2,
    top: (containerH - height) / 2,
    width,
    height,
    scale
  }
}

function getLineStyle(line) {
  const box = renderBox.value
  const x = Number(line?.x0) || 0
  const y = Number(line?.y0) || 0
  const w = Math.max(1, (Number(line?.x1) || x) - x)
  const h = Math.max(1, (Number(line?.y1) || y) - y)
  const padding = Math.max(2, Math.round(box.scale * 2))
  return {
    left: `${box.left + x * box.scale - padding}px`,
    top: `${box.top + y * box.scale - padding}px`,
    width: `${w * box.scale + padding * 2}px`,
    height: `${h * box.scale + padding * 2}px`,
    fontSize: `${Math.max(8, h * box.scale * 0.9)}px`,
    lineHeight: `${Math.max(1, h * box.scale)}px`
  }
}

function extractWordsFromBlocks(blocks, taskId) {
  const words = []
  for (const block of blocks || []) {
    for (const paragraph of block?.paragraphs || []) {
      for (const line of paragraph?.lines || []) {
        for (const word of line?.words || []) {
          const text = (word?.text || '').trim()
          if (!text) continue
          const x0 = Number(word?.bbox?.x0) || 0
          const y0 = Number(word?.bbox?.y0) || 0
          const rawX1 = Number(word?.bbox?.x1)
          const rawY1 = Number(word?.bbox?.y1)
          const x1 = Number.isFinite(rawX1) && rawX1 > x0 ? rawX1 : x0 + Math.max(8, text.length * 10)
          const y1 = Number.isFinite(rawY1) && rawY1 > y0 ? rawY1 : y0 + 18
          words.push({
            id: `${taskId}-${words.length}`,
            text,
            x0,
            y0,
            x1,
            y1
          })
        }
      }
    }
  }
  return words
}

function normalizeOcrBlocks(result, taskId) {
  const blockWords = extractWordsFromBlocks(result?.data?.blocks || [], taskId)
  if (blockWords.length > 0) return blockWords
  const words = (result?.data?.words || [])
      .filter(word => (word?.text || '').trim().length > 0)
      .map((word, index) => {
        const text = (word.text || '').trim()
        const x0 = Number(word?.bbox?.x0) || 0
        const y0 = Number(word?.bbox?.y0) || 0
        const rawX1 = Number(word?.bbox?.x1)
        const rawY1 = Number(word?.bbox?.y1)
        const x1 = Number.isFinite(rawX1) && rawX1 > x0 ? rawX1 : x0 + Math.max(8, text.length * 10)
        const y1 = Number.isFinite(rawY1) && rawY1 > y0 ? rawY1 : y0 + 18
        return {
          id: `${taskId}-${index}`,
          text,
          x0,
          y0,
          x1,
          y1
        }
      })
  if (words.length > 0) return words
  return (result?.data?.lines || [])
      .filter(line => (line?.text || '').trim().length > 0)
      .map((line, index) => ({
        id: `${taskId}-line-${index}`,
        text: (line.text || '').trim(),
        x0: Number(line?.bbox?.x0) || 0,
        y0: Number(line?.bbox?.y0) || 0,
        x1: Number(line?.bbox?.x1) || 0,
        y1: Number(line?.bbox?.y1) || 0
      }))
}

async function ensureOcrWorker() {
  if (ocrWorker) return ocrWorker
  ocrWorker = await createWorker('chi_sim+eng', 1, {
    workerPath: '/ocr/worker.min.js',
    corePath: '/ocr/core/tesseract-core.wasm.js',
    langPath: '/ocr/lang'
  })
  await ocrWorker.setParameters({
    tessedit_pageseg_mode: '6',
    preserve_interword_spaces: '1',
    user_defined_dpi: '300'
  })
  return ocrWorker
}

async function buildEnhancedDataUrl(base64) {
  const img = new Image()
  const src = `data:image/png;base64,${base64}`
  await new Promise((resolve, reject) => {
    img.onload = resolve
    img.onerror = reject
    img.src = src
  })
  const srcW = img.naturalWidth || img.width
  const srcH = img.naturalHeight || img.height
  const targetScale = 2
  const canvas = document.createElement('canvas')
  canvas.width = Math.max(1, Math.round(srcW * targetScale))
  canvas.height = Math.max(1, Math.round(srcH * targetScale))
  const ctx = canvas.getContext('2d', {willReadFrequently: true})
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, canvas.width, canvas.height)
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height)
  const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height)
  const data = imageData.data
  for (let i = 0; i < data.length; i += 4) {
    const gray = Math.round(data[i] * 0.299 + data[i + 1] * 0.587 + data[i + 2] * 0.114)
    const contrast = gray < 160 ? 0 : 255
    data[i] = contrast
    data[i + 1] = contrast
    data[i + 2] = contrast
    data[i + 3] = 255
  }
  ctx.putImageData(imageData, 0, 0)
  return canvas.toDataURL('image/png')
}

async function runOcr(base64, width, height) {
  if (!ocrEnabled.value || !base64) return
  const taskId = ++ocrTaskId
  isRecognizing.value = true
  ocrStatusMessage.value = ''
  try {
    const worker = await ensureOcrWorker()
    const dataUrl = `data:image/png;base64,${base64}`
    const result = await worker.recognize(dataUrl, {}, {blocks: true})
    if (taskId !== ocrTaskId) return
    let lines = normalizeOcrBlocks(result, taskId)
    if (!lines.length) {
      const enhancedDataUrl = await buildEnhancedDataUrl(base64)
      const enhancedResult = await worker.recognize(enhancedDataUrl, {}, {blocks: true})
      if (taskId !== ocrTaskId) return
      lines = normalizeOcrBlocks(enhancedResult, taskId)
    }
    ocrLines.value = lines
    if (!lines.length) {
      ocrStatusMessage.value = '未识别到文字'
    }
    if (width > 0 && height > 0) {
      sourceWidth.value = width
      sourceHeight.value = height
      updateRenderBox()
    }
  } catch (error) {
    if (taskId !== ocrTaskId) return
    ocrLines.value = []
    ocrStatusMessage.value = 'OCR 初始化失败'
    console.error('固定窗口OCR识别失败:', error)
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
  updateRenderBox()
  if (rootRef.value && typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(() => {
      updateRenderBox()
    })
    resizeObserver.observe(rootRef.value)
  }
  window.addEventListener('resize', updateRenderBox)
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
  window.removeEventListener('resize', updateRenderBox)
  if (resizeDebounceTimer) {
    clearTimeout(resizeDebounceTimer)
    resizeDebounceTimer = null
  }
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  if (unlistenResized) {
    unlistenResized()
    unlistenResized = null
  }
  if (ocrWorker) {
    ocrWorker.terminate().catch(() => {
    })
    ocrWorker = null
  }
})
</script>

<style scoped>
.pinned-image-root {
  width: 100%;
  height: 100%;
  overflow: hidden;
  position: relative;
  cursor: crosshair;
}

.pinned-image {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: contain;
  user-select: none;
}

.ocr-text-layer {
  position: absolute;
  inset: 0;
  z-index: 2;
  pointer-events: none;
}

.ocr-line {
  position: absolute;
  display: inline-block;
  user-select: text;
  white-space: pre;
  color: rgba(0, 0, 0, 0.01);
  cursor: text;
  pointer-events: auto;
  box-sizing: border-box;
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
