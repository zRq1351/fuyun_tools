<template>
  <div class="viewer-root" @click="requestClose">
    <div class="viewer-topbar">
      <button class="viewer-close" @click.stop="requestClose(true)">关闭</button>
    </div>
    <div :class="['viewer-card', animationState]" @click.stop>
      <img
          v-if="imageUrl"
          :class="{ 'viewer-image-hidden': !isImageReady }"
          :src="imageUrl"
          alt=""
          class="viewer-image"
          @error="onImageLoaded"
          @load="onImageLoaded"
      />
    </div>
    <div v-if="!isImageReady" class="viewer-loading viewer-loading-overlay">
      <div class="viewer-loading-spinner"></div>
      <div class="viewer-loading-text">正在加载图片...</div>
    </div>
  </div>
</template>

<script setup>
import {onBeforeUnmount, onMounted, ref} from 'vue'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {ImageClipboardService} from '../../services/ipc'

const currentWindow = getCurrentWindow()
const imageUrl = ref('')
const isImageReady = ref(false)
const animationState = ref('closed')
const loadingStartedAt = ref(0)
const MIN_LOADING_MS = 180
let unlistenShowPreview = null
let closeTimer = null
let revealTimer = null

const decodeBase64ToRgba = (base64) => {
  const binary = atob(base64)
  const rgba = new Uint8ClampedArray(binary.length)
  for (let i = 0; i < binary.length; i++) {
    rgba[i] = binary.charCodeAt(i)
  }
  return rgba
}

const buildDataUrlFromRgba = (rgbaBase64, width, height) => {
  if (!rgbaBase64 || !width || !height) return ''
  const rgba = decodeBase64ToRgba(rgbaBase64)
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext('2d')
  if (!ctx) return ''
  const imageData = new ImageData(rgba, width, height)
  ctx.putImageData(imageData, 0, 0)
  return canvas.toDataURL('image/png')
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
  try {
    await ImageClipboardService.closePreviewWindow()
  } catch (error) {
    await currentWindow.hide()
  }
  animationState.value = 'closed'
}

const requestClose = (immediate = false) => {
  if (animationState.value === 'closing' || animationState.value === 'closed') return
  if (closeTimer) {
    window.clearTimeout(closeTimer)
    closeTimer = null
  }
  if (immediate) {
    closeWindowNow()
    return
  }
  animationState.value = 'closing'
  closeTimer = window.setTimeout(async () => {
    closeTimer = null
    await closeWindowNow()
  }, 220)
}

onMounted(async () => {
  unlistenShowPreview = await listen('show-image-preview', (event) => {
    const payload = event.payload || {}
    if (revealTimer) {
      window.clearTimeout(revealTimer)
      revealTimer = null
    }
    loadingStartedAt.value = performance.now()
    isImageReady.value = false
    if (payload.loading) {
      imageUrl.value = ''
      playOpenAnimation()
      return
    }
    imageUrl.value = ''
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const nextUrl = buildDataUrlFromRgba(payload.rgba_base64, payload.width, payload.height)
        imageUrl.value = nextUrl
        if (!nextUrl) {
          onImageLoaded()
        }
      })
    })
    playOpenAnimation()
  })

  window.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      requestClose()
    }
  })

  currentWindow.onCloseRequested(async (event) => {
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
  if (closeTimer) {
    window.clearTimeout(closeTimer)
    closeTimer = null
  }
  if (revealTimer) {
    window.clearTimeout(revealTimer)
    revealTimer = null
  }
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
}

.viewer-image-hidden {
  opacity: 0;
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

@keyframes viewer-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
