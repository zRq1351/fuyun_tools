<template>
  <div :class="['ocr-text-root', `theme-${currentTheme}`]" @dblclick.left.stop.prevent="closeWindow">
    <div class="drag-handle-wrap">
      <div class="drag-handle" @mousedown.left.stop.prevent="startDrag"></div>
    </div>
    <textarea
        v-model="text"
        class="ocr-editor"
        placeholder="暂无识别结果"
        spellcheck="false"
        @dblclick.left.stop.prevent="closeWindow"
    />
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {useTheme} from '../../composables/useTheme'

const text = ref('')
const {currentTheme} = useTheme()

let onOcrTextData = null
let initialPayloadTimer = null
let initialPayloadTryCount = 0
let lastDragStartAt = 0

function applyPayload(payload) {
  const value = String(payload?.text || '').trim()
  if (!value) return
  text.value = value
}

function applyInitialPayload() {
  if (window.__OCR_TEXT_PAYLOAD__) {
    applyPayload(window.__OCR_TEXT_PAYLOAD__)
  }
}

function stopInitialPayloadTimer() {
  if (initialPayloadTimer) {
    clearInterval(initialPayloadTimer)
    initialPayloadTimer = null
  }
}

function startInitialPayloadTimer() {
  stopInitialPayloadTimer()
  initialPayloadTryCount = 0
  initialPayloadTimer = setInterval(() => {
    initialPayloadTryCount += 1
    applyInitialPayload()
    if (text.value || initialPayloadTryCount >= 25) {
      stopInitialPayloadTimer()
    }
  }, 80)
}

async function closeWindow() {
  try {
    await getCurrentWebviewWindow().close()
  } catch (_) {
    try {
      window.close()
    } catch (_) {
    }
  }
}

async function startDrag() {
  const now = Date.now()
  if (now - lastDragStartAt < 220) return
  lastDragStartAt = now
  try {
    await getCurrentWebviewWindow().startDragging()
  } catch (_) {
  }
}

onMounted(() => {
  onOcrTextData = (event) => {
    applyPayload(event?.detail)
  }
  window.addEventListener('ocr-text-data', onOcrTextData)

  applyInitialPayload()
  startInitialPayloadTimer()
})

onUnmounted(() => {
  stopInitialPayloadTimer()
  if (onOcrTextData) {
    window.removeEventListener('ocr-text-data', onOcrTextData)
    onOcrTextData = null
  }
})
</script>

<style scoped>
.ocr-text-root {
  box-sizing: border-box;
  position: fixed;
  inset: 0;
  overflow: hidden;
  background: var(--fy-bg-primary);
  color: var(--fy-text-primary);
  display: flex;
  flex-direction: column;
}

.drag-handle-wrap {
  height: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-top: 6px;
  flex: 0 0 auto;
}

.drag-handle {
  width: 96px;
  height: 5px;
  border-radius: 999px;
  background: var(--fy-border);
  cursor: move;
}

.ocr-editor {
  box-sizing: border-box;
  width: 100%;
  flex: 1;
  min-height: 0;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: var(--fy-text-primary);
  padding: 8px 12px 12px;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 13px;
  line-height: 1.5;
  user-select: text;
  font-family: 'Consolas', 'Microsoft YaHei', sans-serif;
  overflow: auto;
  overflow-x: hidden;
  overflow-wrap: anywhere;
  scrollbar-width: none;
  -ms-overflow-style: none;
}

.ocr-editor::-webkit-scrollbar {
  width: 0;
  height: 0;
  display: none;
}
</style>
