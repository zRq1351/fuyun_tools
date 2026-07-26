<template>
  <div ref="rootRef" class="pinned-image-root"
       @mousedown.left="handleRootMouseDown"
       @dblclick.left.stop.prevent="closeWindow"
       @contextmenu.prevent="handleContextMenu"
       @click="closeCtxMenu">
    <img v-if="imageSrc" ref="imageRef" :src="imageSrc" alt="" class="pinned-image" draggable="false"
         @load="handleImageLoaded"/>

    <!-- 透明实况文本层 -->
    <div v-if="!isRecognizing && ocrLines.length > 0 && sourceWidth > 0 && sourceHeight > 0" class="ocr-text-overlay">
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

    <ContextMenu :show="ctxMenuShow" :x="ctxMenuX" :y="ctxMenuY" @close="closeCtxMenu">
      <div class="context-menu-item" @click="copyAllText">{{ t('pinnedImage.copyAllText') }}</div>
      <div class="context-menu-item" @click="openTextWindow">{{ t('pinnedImage.viewInWindow') }}</div>
      <div class="context-menu-divider"></div>
      <div class="context-menu-item" @click="closeWindow">{{ t('pinnedImage.closePinned') }}</div>
    </ContextMenu>
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {useI18n} from 'vue-i18n'
import {useWindowDrag} from '../../composables/useWindowDrag'
import ContextMenu from '../../components/ContextMenu.vue'

const {t} = useI18n()
const {startDrag} = useWindowDrag()

const imageSrc = ref('')
const windowLabel = ref('')
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
const ctxMenuShow = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)

function handleContextMenu(event) {
  ctxMenuX.value = event.clientX
  ctxMenuY.value = event.clientY
  ctxMenuShow.value = true
}

function closeCtxMenu() {
  ctxMenuShow.value = false
}

let ocrTaskId = 0
let lastPayloadBase64 = ''

function applyPinnedPayload(detail) {
  if (!detail?.png_base64) return
  // 防止重复处理相同载荷（初始化脚本和 eval 都会触发）
  if (detail.png_base64 === lastPayloadBase64) return
  lastPayloadBase64 = detail.png_base64
  if (detail?.label) {
    windowLabel.value = detail.label
  }
  const width = Number(detail?.width) || 0
  const height = Number(detail?.height) || 0
  currentPngBase64.value = detail.png_base64
  currentImageWidth.value = width
  currentImageHeight.value = height
  imageSrc.value = `data:image/png;base64,${detail.png_base64}`

  // 注意：sourceWidth/sourceHeight 由 handleImageLoaded 设置（自然图片尺寸），
  // 不从 payload 设置（payload 是窗口尺寸，与 OCR 坐标空间不一致）
  // OCR 在图片加载完成后触发
}

function handlePinnedImageData(event) {
  applyPinnedPayload(event?.detail)
}

async function closeWindow() {
  closeCtxMenu()
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

function handleRootMouseDown(event) {
  closeCtxMenu()
  startDrag()
}

async function copyAllText() {
  closeCtxMenu()
  if (isRecognizing.value) {
    showToast(t('pinnedImage.recognizing'), false)
    return
  }
  if (!ocrLines.value.length) {
    showToast(t('pinnedImage.noTextRecognized'), true)
    return
  }
  const text = ocrLines.value.map(line => (line?.text || '').trim()).filter(Boolean).join('\n').trim()
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    showToast(t('pinnedImage.textCopied'), false)
  } catch (e) {
    console.error('复制失败', e)
    showToast(t('pinnedImage.copyFailed'), true)
  }
}

async function openTextWindow() {
  closeCtxMenu()
  if (isRecognizing.value) {
    showToast(t('pinnedImage.recognizing'), false)
    return
  }
  if (!ocrLines.value.length) {
    showToast(t('pinnedImage.noTextRecognized'), true)
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

function handleImageLoaded(event) {
  const naturalWidth = Number(event?.target?.naturalWidth) || 0
  const naturalHeight = Number(event?.target?.naturalHeight) || 0
  if (naturalWidth > 0 && naturalHeight > 0) {
    sourceWidth.value = naturalWidth
    sourceHeight.value = naturalHeight
    // 图片加载完成后触发 OCR（使用自然尺寸作为坐标空间）
    if (currentPngBase64.value) {
      ocrLines.value = []
      runOcr(currentPngBase64.value, naturalWidth, naturalHeight)
    }
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
    const binaryString = atob(base64)
    const bytes = new Uint8Array(binaryString.length)
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i)
    }
    const payload = { pngBytes: Array.from(bytes) }
    if (engine) {
      payload.engine = engine
    }
    const result = await invoke('recognize_image_ocr', payload)
    if (taskId !== ocrTaskId) return []
    if (!result?.success) {
      showToast(result?.error || t('pinnedImage.ocrFailed'), true)
      return []
    }
    const lines = normalizeNativeOcrParagraphs(result?.paragraphs, taskId)
    if (!lines.length) {
      showToast(t('pinnedImage.noTextRecognized'), true)
    }
    if (width > 0 && height > 0) {
      sourceWidth.value = width
      sourceHeight.value = height
    }
    ocrLines.value = lines
    return lines
  } catch (error) {
    if (taskId !== ocrTaskId) return []
    showToast(t('pinnedImage.ocrInitFailed'), true)
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
})

onUnmounted(() => {
  if (toastTimer) clearTimeout(toastTimer)
  window.removeEventListener('pinned-image-data', handlePinnedImageData)
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
  background: var(--fy-accent);
  box-shadow: 0 0 10px var(--fy-accent), 0 0 20px var(--fy-accent);
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
  background: rgba(30, 34, 48, 0.72);
  color: var(--fy-text-primary);
  font-size: var(--fy-text-base);
  padding: 6px 12px;
  border-radius: var(--fy-radius-md);
  pointer-events: none;
  backdrop-filter: blur(20px) saturate(150%);
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.2);
  transition: opacity 0.3s;
}
.ocr-toast-error {
  background: var(--fy-danger);
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
  background: var(--fy-accent-bg);
  color: transparent;
}


/* 全局交互增强：焦点环 + 过渡 */
button:focus-visible,
[role="button"]:focus-visible,
[tabindex]:focus-visible {
  outline: 2px solid var(--fy-accent);
  outline-offset: 2px;
}

button, [role="button"] {
  transition: transform 0.12s var(--fy-ease-out), filter 0.12s var(--fy-ease-out), opacity 0.15s var(--fy-ease-out);
}

button:active:not(:disabled),
[role="button"]:active:not([aria-disabled="true"]) {
  transform: scale(0.96);
}
</style>
