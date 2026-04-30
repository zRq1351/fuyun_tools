<template>
  <div class="viewer-root" @click="requestClose">
    <div
        class="viewer-drag-strip"
        data-tauri-drag-region
        @click.stop
        @mousedown.left="startWindowDrag"
    ></div>
    <div class="viewer-topbar" @click.stop>
      <div
          class="viewer-drag-icon"
          data-tauri-drag-region
          title="拖动窗口"
          @mousedown.left.stop.prevent="startWindowDrag"
      >
        <GripHorizontal :size="16" :stroke-width="2" />
      </div>
      <div class="viewer-zoom">{{ zoomPercent }}</div>
      <button class="viewer-close" @mousedown.left.stop.prevent @click.stop="requestClose(true)">关闭</button>
    </div>
    <div
        :class="['viewer-card', animationState, {'is-dragging': isDragging}]"
        @click.stop
        @mousedown.left.stop.prevent="startDrag"
        @wheel.prevent.stop="handleWheel"
    >
      <img
          v-if="imageUrl"
          :class="{ 'viewer-image-hidden': !isImageReady }"
          :src="imageUrl"
          alt=""
          class="viewer-image"
          :style="imageTransformStyle"
          @dblclick.stop.prevent="resetViewTransform"
          @error="onImageError"
          @load="onImageLoaded"
      />
    </div>
    <div v-if="!isImageReady && !loadErrorMessage" class="viewer-loading viewer-loading-overlay">
      <div class="viewer-loading-spinner"></div>
      <div class="viewer-loading-text">正在加载图片...</div>
    </div>
    <div v-if="isImageReady && loadErrorMessage" class="viewer-loading viewer-loading-overlay viewer-error">
      <div class="viewer-loading-text">{{ loadErrorMessage }}</div>
    </div>
  </div>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, ref} from 'vue'
import {GripHorizontal} from 'lucide-vue-next'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {convertFileSrc} from '@tauri-apps/api/core'
import {ImageClipboardService} from '../../services/ipc'

const currentWindow = getCurrentWebviewWindow()
const imageUrl = ref('')
const isImageReady = ref(false)
const loadErrorMessage = ref('')
const animationState = ref('closed')
const loadingStartedAt = ref(0)
const activeRequestId = ref('')
const zoomScale = ref(1)
const offsetX = ref(0)
const offsetY = ref(0)
const isDragging = ref(false)
const MIN_LOADING_MS = 180
let unlistenShowPreview = null
let unlistenCloseRequested = null
let closeTimer = null
let revealTimer = null
let keydownHandler = null
let payloadWatchdogTimer = null
let dragStartX = 0
let dragStartY = 0
let dragBaseOffsetX = 0
let dragBaseOffsetY = 0
let isMouseDown = false

const imageTransformStyle = computed(() => ({
  transform: `translate3d(${offsetX.value}px, ${offsetY.value}px, 0) scale(${zoomScale.value})`
}))
const zoomPercent = computed(() => `${Math.round(zoomScale.value * 100)}%`)

const resetViewTransform = () => {
  zoomScale.value = 1
  offsetX.value = 0
  offsetY.value = 0
  isDragging.value = false
  isMouseDown = false
}

const handleGlobalMouseMove = (event) => {
  if (!isMouseDown || !isImageReady.value) return
  isDragging.value = true
  offsetX.value = dragBaseOffsetX + (event.clientX - dragStartX)
  offsetY.value = dragBaseOffsetY + (event.clientY - dragStartY)
}

const stopDrag = () => {
  isMouseDown = false
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', stopDrag, true)
}

const startDrag = (event) => {
  if (!isImageReady.value || !imageUrl.value) return
  isMouseDown = true
  dragStartX = event.clientX
  dragStartY = event.clientY
  dragBaseOffsetX = offsetX.value
  dragBaseOffsetY = offsetY.value
  window.addEventListener('mousemove', handleGlobalMouseMove)
  window.addEventListener('mouseup', stopDrag, true)
}

const handleWheel = (event) => {
  if (!isImageReady.value || !imageUrl.value) return
  const factor = event.deltaY > 0 ? 0.92 : 1.08
  const next = Math.min(6, Math.max(0.2, zoomScale.value * factor))
  zoomScale.value = Number(next.toFixed(3))
}

const startWindowDrag = () => {
  currentWindow.startDragging().catch((error) => {
    ImageClipboardService.startPreviewWindowDrag().catch((fallbackError) => {
      console.error('拖动预览窗口失败:', fallbackError || error)
    })
  })
}

const buildFileUrlFromPath = (imagePath) => {
  if (!imagePath) return ''
  try {
    return convertFileSrc(imagePath)
  } catch (_) {
    return ''
  }
}

const playOpenAnimation = () => {
  animationState.value = 'opening'
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      animationState.value = 'opened'
    })
  })
}

const onImageLoaded = () => {
  loadErrorMessage.value = ''
  const elapsed = performance.now() - loadingStartedAt.value
  const remain = Math.max(0, MIN_LOADING_MS - elapsed)
  if (revealTimer) {
    window.clearTimeout(revealTimer)
  }
  revealTimer = window.setTimeout(() => {
    isImageReady.value = true
    revealTimer = null
  }, remain)
}

const onImageError = () => {
  if (!loadErrorMessage.value) {
    loadErrorMessage.value = '图片加载失败，路径不可访问或文件已失效'
  }
  const elapsed = performance.now() - loadingStartedAt.value
  const remain = Math.max(0, MIN_LOADING_MS - elapsed)
  if (revealTimer) {
    window.clearTimeout(revealTimer)
  }
  revealTimer = window.setTimeout(() => {
    isImageReady.value = true
    revealTimer = null
  }, remain)
}

const closeWindowNow = async () => {
  imageUrl.value = ''
  isImageReady.value = false
  loadErrorMessage.value = ''
  activeRequestId.value = ''
  resetViewTransform()
  await new Promise((resolve) => {
    requestAnimationFrame(() => resolve())
  })
  try {
    await ImageClipboardService.closePreviewWindow()
  } catch (error) {
    await currentWindow.hide()
  }
  animationState.value = 'closed'
}

const requestClose = (immediate = false) => {
  if (immediate) {
    closeWindowNow()
    return
  }
  if (animationState.value === 'closing' || animationState.value === 'closed') return
  if (closeTimer) {
    window.clearTimeout(closeTimer)
    closeTimer = null
  }
  animationState.value = 'closing'
  closeTimer = window.setTimeout(async () => {
    closeTimer = null
    await closeWindowNow()
  }, 220)
}

const schedulePayloadWatchdog = () => {
  if (payloadWatchdogTimer) {
    window.clearTimeout(payloadWatchdogTimer)
  }
  payloadWatchdogTimer = window.setTimeout(async () => {
    payloadWatchdogTimer = null
    if (!imageUrl.value && animationState.value === 'closed') {
      await closeWindowNow()
    }
  }, 1200)
}

onMounted(async () => {
  unlistenShowPreview = await listen('show-image-preview', (event) => {
    const payload = event.payload || {}
    const payloadRequestId = String(payload.request_id || '')
    if (payload.loading) {
      activeRequestId.value = payloadRequestId
    } else if (payloadRequestId && payloadRequestId !== activeRequestId.value) {
      return
    }
    if (revealTimer) {
      window.clearTimeout(revealTimer)
      revealTimer = null
    }
    if (payload.loading) {
      loadingStartedAt.value = performance.now()
      isImageReady.value = false
      loadErrorMessage.value = ''
      imageUrl.value = ''
      resetViewTransform()
      playOpenAnimation()
      return
    }
    const keepVisible = !!imageUrl.value && payload.is_final === true && isImageReady.value
    if (!keepVisible) {
      loadingStartedAt.value = performance.now()
      isImageReady.value = false
      loadErrorMessage.value = ''
    }
    const payloadError = typeof payload.error_message === 'string'
        ? payload.error_message.trim()
        : ''
    if (payloadError) {
      imageUrl.value = ''
      resetViewTransform()
      loadErrorMessage.value = payloadError
      onImageError()
      playOpenAnimation()
      return
    }
    imageUrl.value = ''
    resetViewTransform()
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const nextUrl = buildFileUrlFromPath(payload.image_path)
        imageUrl.value = nextUrl
        if (!nextUrl) {
          loadErrorMessage.value = '图片路径无效或不在允许目录内'
          onImageError()
        }
      })
    })
    playOpenAnimation()
  })
  schedulePayloadWatchdog()

  keydownHandler = (event) => {
    if (event.key === 'F5' || ((event.ctrlKey || event.metaKey) && String(event.key).toLowerCase() === 'r')) {
      event.preventDefault()
      return
    }
    if (event.key === 'Escape') {
      event.preventDefault()
      requestClose()
    }
  }
  window.addEventListener('keydown', keydownHandler)

  unlistenCloseRequested = await currentWindow.onCloseRequested(async (event) => {
    if (animationState.value !== 'closing' && animationState.value !== 'closed') {
      event.preventDefault()
      requestClose()
    }
  })
})

onBeforeUnmount(() => {
  if (unlistenShowPreview) {
    unlistenShowPreview()
    unlistenShowPreview = null
  }
  if (unlistenCloseRequested) {
    unlistenCloseRequested()
    unlistenCloseRequested = null
  }
  if (keydownHandler) {
    window.removeEventListener('keydown', keydownHandler)
    keydownHandler = null
  }
  if (closeTimer) {
    window.clearTimeout(closeTimer)
    closeTimer = null
  }
  if (revealTimer) {
    window.clearTimeout(revealTimer)
    revealTimer = null
  }
  if (payloadWatchdogTimer) {
    window.clearTimeout(payloadWatchdogTimer)
    payloadWatchdogTimer = null
  }
  stopDrag()
})
</script>

<style>
html, body, #app {
  width: 100%;
  height: 100%;
  margin: 0;
  padding: 0;
  overflow: hidden;
}
</style>

<style scoped>
.viewer-root {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: radial-gradient(circle at top, rgba(35, 35, 40, 0.92), rgba(10, 10, 12, 0.96));
  backdrop-filter: blur(10px);
}

.viewer-topbar {
  position: absolute;
  top: 12px;
  right: 12px;
  z-index: 30;
  pointer-events: auto;
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.viewer-drag-strip {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 44px;
  z-index: 25;
  cursor: move;
}

.viewer-drag-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid rgba(255, 255, 255, 0.22);
  background: rgba(0, 0, 0, 0.6);
  color: rgba(255, 255, 255, 0.86);
  border-radius: 8px;
  box-sizing: border-box;
  cursor: move;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  transition: background-color 0.2s, color 0.2s;
}

.viewer-drag-icon:hover {
  background: rgba(0, 0, 0, 0.8);
  color: rgba(255, 255, 255, 1);
}

.viewer-zoom {
  min-width: 56px;
  text-align: center;
  border: 1px solid rgba(255, 255, 255, 0.22);
  background: rgba(0, 0, 0, 0.6);
  color: #e7f2ff;
  border-radius: 8px;
  padding: 6px 10px;
  font-size: 12px;
  font-weight: 600;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  cursor: move;
}

.viewer-card {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 22px;
  box-sizing: border-box;
  position: relative;
  z-index: 10;
  opacity: 0;
  transform: scale(0.84);
  transition: transform 220ms cubic-bezier(0.2, 0.8, 0.2, 1), opacity 220ms ease;
}

.viewer-card.opening,
.viewer-card.opened {
  opacity: 1;
  transform: scale(1);
}

.viewer-card.closing {
  opacity: 0;
  transform: scale(0.9);
}

.viewer-image {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  border-radius: 10px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.45);
  transform-origin: center center;
  transition: transform 80ms linear;
  user-select: none;
}

.viewer-image-hidden {
  opacity: 0;
}

.viewer-card {
  cursor: grab;
}

.viewer-card.is-dragging {
  cursor: grabbing;
}

.viewer-close {
  border: 1px solid rgba(255, 255, 255, 0.26);
  background: rgba(0, 0, 0, 0.65);
  color: #fff;
  border-radius: 8px;
  padding: 6px 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

.viewer-loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  color: rgba(255, 255, 255, 0.86);
  font-size: 14px;
}

.viewer-loading-overlay {
  position: absolute;
  inset: 0;
  z-index: 20;
  justify-content: center;
  pointer-events: none;
}

.viewer-loading-spinner {
  width: 30px;
  height: 30px;
  border-radius: 50%;
  border: 3px solid rgba(255, 255, 255, 0.25);
  border-top-color: rgba(255, 255, 255, 0.92);
  animation: viewer-spin 700ms linear infinite;
}

.viewer-loading-text {
  font-size: 13px;
  letter-spacing: 0.2px;
}

.viewer-error .viewer-loading-text {
  max-width: min(720px, 78vw);
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid rgba(255, 120, 120, 0.5);
  background: rgba(60, 10, 10, 0.65);
  color: rgba(255, 230, 230, 0.96);
  text-align: center;
}

@keyframes viewer-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
