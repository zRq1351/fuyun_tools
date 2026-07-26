<template>
  <div class="panel">
    <div class="header" data-tauri-drag-region>
      <div class="title" data-tauri-drag-region>{{ titleText }}</div>
      <div class="actions no-drag">
        <button :disabled="phase === 'finishing' || phase === 'canceling' || phase === 'failed'" :title="paused ? t('longshot.continue') : t('longshot.pause')"
                class="region-icon-btn"
                @click="togglePause">
          <span v-if="paused" class="btn-icon">▶</span>
          <span v-else class="btn-icon">||</span>
        </button>
        <button :disabled="phase === 'finishing' || phase === 'canceling' || phase === 'failed'"
                :title="t('longshot.finish')"
                class="region-icon-btn primary"
                @click="finish">
          <Check class="tool-icon-wrap"/>
        </button>
        <button :disabled="phase === 'canceling'" :title="t('longshot.cancel')"
                class="region-icon-btn danger"
                @click="cancel">
          <X class="tool-icon-wrap"/>
        </button>
      </div>
    </div>

    <div class="preview-wrap" data-tauri-drag-region>
      <img v-if="previewSrc" :src="previewSrc" alt="longshot preview" class="preview" data-tauri-drag-region
           draggable="false"/>
      <div v-else class="preview-empty" data-tauri-drag-region>{{ t('longshot.waitPreview') }}</div>
      <div v-if="previewSrc && viewportStyle" class="viewport-marker" :style="viewportStyle"></div>
    </div>

    <div class="meta" data-tauri-drag-region>
      {{ t('longshot.status') }} {{ phaseText }} ·
      {{ t('longshot.height') }} {{ stitchedHeight }} px · {{ t('longshot.frame') }} {{ frameCount }} ·
      {{ t('longshot.dropped') }} {{ droppedFrames }} · {{ t('longshot.confidence') }}
      {{ Number(lastConfidence || 0).toFixed(2) }}
    </div>
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {listen} from '@tauri-apps/api/event'
import {useI18n} from 'vue-i18n'
import {Check, X} from 'lucide-vue-next'

const {t} = useI18n()
const paused = ref(false)
const phase = ref('starting')
const previewSrc = ref('')
const stitchedHeight = ref(0)
const captureHeight = ref(0)
const frameCount = ref(0)
const droppedFrames = ref(0)
const lastConfidence = ref(0)

let unlistenProgress = null
let unlistenPreview = null
let unlistenLifecycle = null
let unlistenReset = null
let snapTimer = null

const togglePause = async () => {
  try {
    await invoke('longshot_toolbar_action', {action: paused.value ? 'resume' : 'pause'})
  } catch (e) {
    console.error('longshot togglePause failed:', e)
  }
}

const finish = async () => {
  try {
    await invoke('longshot_toolbar_action', {action: 'finish'})
  } catch (e) {
    console.error('longshot finish failed:', e)
  }
}

const cancel = async () => {
  try {
    await invoke('longshot_toolbar_action', {action: 'cancel'})
  } catch (e) {
    console.error('longshot cancel failed:', e)
  }
}

const phaseText = computed(() => {
  if (phase.value === 'starting') return t('longshot.phasePreparing')
  if (phase.value === 'running') return t('longshot.phaseInProgress')
  if (phase.value === 'paused') return t('longshot.phasePaused')
  if (phase.value === 'finishing') return t('longshot.phaseFinishing')
  if (phase.value === 'canceling') return t('longshot.phaseCancelling')
  if (phase.value === 'failed') return t('longshot.phaseFailed')
  if (phase.value === 'done') return t('longshot.phaseCompleted')
  return t('longshot.phaseUnknown')
})

const titleText = computed(() => {
  if (phase.value === 'paused') return t('longshot.titlePaused')
  if (phase.value === 'finishing') return t('longshot.titleFinishing')
  if (phase.value === 'canceling') return t('longshot.titleCancelling')
  if (phase.value === 'failed') return t('longshot.titleFailed')
  if (phase.value === 'done') return t('longshot.titleCompleted')
  if (phase.value === 'starting') return t('longshot.titlePreparing')
  return t('longshot.titleInProgress')
})

onMounted(async () => {
  unlistenProgress = await listen('manual-longshot-progress', (event) => {
    const payload = event.payload || {}
    phase.value = String(payload.phase || phase.value || 'running')
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
    phase.value = String(payload.phase || phase.value)
    if (state === 'paused') paused.value = true
    if (state === 'resumed' || state === 'started' || state === 'running') paused.value = false
    if (phase.value === 'running' || phase.value === 'starting') paused.value = false
    if (phase.value === 'paused') paused.value = true
  })
  unlistenReset = await listen('manual-longshot-toolbar-reset', () => {
    previewSrc.value = ''
    stitchedHeight.value = 0
    captureHeight.value = 0
    frameCount.value = 0
    droppedFrames.value = 0
    lastConfidence.value = 0
    paused.value = false
    phase.value = 'starting'
  })
  window.addEventListener('mouseup', scheduleSnap)
})

onUnmounted(() => {
  if (typeof unlistenProgress === 'function') unlistenProgress()
  if (typeof unlistenPreview === 'function') unlistenPreview()
  if (typeof unlistenLifecycle === 'function') unlistenLifecycle()
  if (typeof unlistenReset === 'function') unlistenReset()
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
  background: var(--fy-bg-primary);
  border: none;
  border-radius: 10px;
  box-sizing: border-box;
  padding: 8px;
  color: var(--fy-text-primary);
  backdrop-filter: var(--fy-backdrop-blur-light);
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

.region-icon-btn {
  width: 32px;
  height: 32px;
  border: 1px solid var(--fy-border);
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  cursor: pointer;
  backdrop-filter: blur(2px);
  transition: all 0.15s var(--fy-ease-out);
}

.region-icon-btn:hover {
  background: var(--fy-bg-surface);
  border-color: var(--fy-text-muted);
  transform: scale(1.05);
}

.region-icon-btn:active {
  transform: scale(0.95);
}

.region-icon-btn:focus-visible {
  outline: 2px solid var(--fy-accent);
  outline-offset: 2px;
}

.region-icon-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  transform: none;
}

.region-icon-btn:disabled:hover {
  background: var(--fy-bg-hover);
  border-color: var(--fy-border);
  transform: none;
}

.region-icon-btn.primary {
  background: var(--fy-accent);
  border-color: var(--fy-accent);
  color: var(--fy-text-inverse);
}

.region-icon-btn.primary:hover {
  background: var(--fy-accent-hover, var(--fy-accent));
  filter: brightness(1.1);
}

.region-icon-btn.danger {
  background: var(--fy-danger);
  border-color: var(--fy-danger);
  color: var(--fy-text-inverse);
}

.region-icon-btn.danger:hover {
  filter: brightness(1.15);
}

.tool-icon-wrap {
  width: 16px;
  height: 16px;
}

.btn-icon {
  font-size: 12px;
  line-height: 1;
}
.preview-wrap {
  position: relative;
  flex: 1;
  width: 100%;
  min-height: 0;
  border-radius: 6px;
  overflow: hidden;
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  display: flex;
  align-items: center;
  justify-content: center;
}
.preview {
  width: 100%;
  height: 100%;
  object-fit: contain;
  image-rendering: auto;
  transition: opacity 0.2s var(--fy-ease-out);
}
.viewport-marker {
  position: absolute;
  left: 2px;
  right: 2px;
  min-height: 10px;
  border: 2px solid var(--fy-accent);
  border-radius: 4px;
  box-shadow: 0 0 0 1px var(--fy-border);
  background: var(--fy-accent-bg);
  pointer-events: none;
  transition: top 0.1s linear, height 0.1s linear;
}

.preview-empty {
  font-size: 12px;
  opacity: 0.6;
}

.meta {
  font-size: 11px;
  line-height: 1.35;
  opacity: 0.75;
  padding: 2px 0;
}
.no-drag { -webkit-app-region: no-drag; }


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
