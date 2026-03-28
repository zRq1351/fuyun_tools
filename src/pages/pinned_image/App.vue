<template>
  <div class="pinned-image-root" @mousedown.left="startDrag" @dblclick.stop.prevent="ignoreDoubleClick"
       @contextmenu.prevent="closeWindow">
    <img v-if="imageSrc" :src="imageSrc" class="pinned-image" draggable="false"/>
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
let unlistenResized = null
let resizeDebounceTimer = null
let lastDragStartAt = 0

function applyPinnedPayload(detail) {
  if (!detail?.png_base64) return
  if (detail?.label) {
    windowLabel.value = detail.label
  }
  const width = Number(detail?.width) || 0
  const height = Number(detail?.height) || 0
  if (width > 0 && height > 0) {
    aspectRatio.value = width / height
  }
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

function ignoreDoubleClick() {
  return
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
.pinned-image-root {
  width: 100%;
  height: 100%;
  cursor: move;
  overflow: hidden;
}

.pinned-image {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: contain;
  user-select: none;
}
</style>
