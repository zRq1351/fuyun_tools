<template>
  <div class="panel" data-tauri-drag-region>
    <div class="header">
      <div class="title">{{ paused ? '长截图已暂停' : '长截图进行中' }}</div>
      <div class="actions no-drag">
        <button class="btn" @click="togglePause">{{ paused ? '继续' : '暂停' }}</button>
        <button class="btn primary" @click="finish">完成</button>
        <button class="btn danger" @click="cancel">取消</button>
      </div>
    </div>

    <div class="preview-wrap">
      <img v-if="previewSrc" :src="previewSrc" alt="longshot preview" class="preview"/>
      <div v-else class="preview-empty">等待预览...</div>
      <div v-if="previewSrc && viewportStyle" class="viewport-marker" :style="viewportStyle"></div>
    </div>

    <div class="meta">
      高度 {{ stitchedHeight }} px · 帧 {{ frameCount }} · 丢帧 {{ droppedFrames }} · 置信度
      {{ Number(lastConfidence || 0).toFixed(2) }}
    </div>
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {listen} from '@tauri-apps/api/event'

const paused = ref(false)
const previewSrc = ref('')
const stitchedHeight = ref(0)
const captureHeight = ref(0)
const frameCount = ref(0)
const droppedFrames = ref(0)
const lastConfidence = ref(0)

let unlistenProgress = null
let unlistenPreview = null
let unlistenLifecycle = null
let snapTimer = null

const togglePause = async () => {
  await invoke('longshot_toolbar_action', {action: paused.value ? 'resume' : 'pause'})
}

const finish = async () => {
  await invoke('longshot_toolbar_action', {action: 'finish'})
}

const cancel = async () => {
  await invoke('longshot_toolbar_action', {action: 'cancel'})
}

onMounted(async () => {
  unlistenProgress = await listen('manual-longshot-progress', (event) => {
    const payload = event.payload || {}
    stitchedHeight.value = Number(payload.stitchedHeight || 0)
    captureHeight.value = Number(payload.captureHeight || 0)
    frameCount.value = Number(payload.frameCount || 0)
    droppedFrames.value = Number(payload.droppedFrames || 0)
    lastConfidence.value = Number(payload.lastConfidence || 0)
  })
  unlistenPreview = await listen('manual-longshot-preview-updated', (event) => {
    const payload = event.payload || {}
    const b64 = String(payload.previewBase64 || '')
    if (b64) {
      previewSrc.value = `data:image/png;base64,${b64}`
    }
  })
  unlistenLifecycle = await listen('manual-longshot-lifecycle', (event) => {
    const payload = event.payload || {}
    const state = String(payload.state || '')
    if (state === 'paused') paused.value = true
    if (state === 'resumed' || state === 'started' || state === 'running') paused.value = false
  })
  window.addEventListener('mouseup', scheduleSnap)
})

onUnmounted(() => {
  if (typeof unlistenProgress === 'function') unlistenProgress()
  if (typeof unlistenPreview === 'function') unlistenPreview()
  if (typeof unlistenLifecycle === 'function') unlistenLifecycle()
  window.removeEventListener('mouseup', scheduleSnap)
  if (snapTimer) {
    clearTimeout(snapTimer)
    snapTimer = null
  }
})

function scheduleSnap() {
  if (snapTimer) clearTimeout(snapTimer)
  snapTimer = setTimeout(() => {
    invoke('snap_longshot_toolbar_window').catch(() => {
    })
  }, 80)
}

const viewportStyle = computed(() => {
  const stitched = stitchedHeight.value
  const cap = captureHeight.value
  if (!stitched || !cap || stitched <= 0 || cap <= 0) return null
  const ratio = Math.min(1, cap / stitched)
  const markerHeightPercent = Math.max(8, ratio * 100)
  const topPercent = Math.max(0, 100 - markerHeightPercent)
  return {
    top: `${topPercent}%`,
    height: `${markerHeightPercent}%`
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
  overflow: hidden;
  background: transparent;
}

:global(*),
:global(*::before),
:global(*::after) {
  box-sizing: border-box;
}

.panel {
  width: 100%;
  height: 100%;
  background: rgba(17, 22, 32, 0.94);
  border: none;
  border-radius: 10px;
  box-sizing: border-box;
  padding: 8px;
  color: #e9eefc;
  backdrop-filter: blur(4px);
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow: hidden;
}
.header {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.title {
  font-size: 13px;
  font-weight: 700;
  line-height: 1.1;
}
.actions {
  display: flex;
  gap: 6px;
}
.btn {
  flex: 1;
  height: 24px;
  border-radius: 6px;
  border: 1px solid rgba(255,255,255,0.24);
  background: rgba(255,255,255,0.08);
  color: #ecf2ff;
  cursor: pointer;
  padding: 0 4px;
  font-size: 12px;
}
.btn.primary { background: rgba(73, 151, 255, 0.35); border-color: rgba(114,183,255,0.8); }
.btn.danger { background: rgba(245,108,108,0.18); border-color: rgba(245,108,108,0.55); }
.preview-wrap {
  position: relative;
  flex: 1;
  width: 100%;
  min-height: 0;
  border-radius: 6px;
  overflow: hidden;
  background: rgba(7, 10, 16, 0.95);
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
}
.preview {
  width: 100%;
  height: 100%;
  object-fit: contain;
  image-rendering: auto;
}
.viewport-marker {
  position: absolute;
  left: 2px;
  right: 2px;
  min-height: 10px;
  border: 2px solid rgba(92, 201, 255, 0.95);
  border-radius: 4px;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.45);
  background: rgba(92, 201, 255, 0.08);
  pointer-events: none;
}
.preview-empty { font-size: 12px; opacity: 0.75; }
.meta { font-size: 11px; line-height: 1.35; opacity: 0.92; }
.no-drag { -webkit-app-region: no-drag; }

/* PixPin 风格：预览窗不显示滚动条 */
.panel,
.preview-wrap {
  scrollbar-width: none;
  -ms-overflow-style: none;
}
.panel::-webkit-scrollbar,
.preview-wrap::-webkit-scrollbar {
  width: 0;
  height: 0;
  display: none;
}
</style>
