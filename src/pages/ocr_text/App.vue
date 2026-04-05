<template>
  <div :class="['ocr-text-root', `theme-${themeMode}`]" @dblclick.left.stop.prevent="closeWindow">
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

const text = ref('')
const themeMode = ref('dark')

let onOcrTextData = null
let onStorageThemeChange = null
let initialPayloadTimer = null
let initialPayloadTryCount = 0
let lastDragStartAt = 0

function getCurrentTheme() {
  const saved = localStorage.getItem('settings-theme')
  if (saved === 'dark' || saved === 'light') return saved
  return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function applyTheme(value) {
  const next = value === 'light' ? 'light' : 'dark'
  themeMode.value = next
  document.documentElement.classList.toggle('theme-light', next === 'light')
  document.documentElement.classList.toggle('theme-dark', next === 'dark')
  document.body.classList.toggle('theme-light', next === 'light')
  document.body.classList.toggle('theme-dark', next === 'dark')
}

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
  applyTheme(getCurrentTheme())
  onStorageThemeChange = (event) => {
    if (!event || event.key === 'settings-theme') {
      applyTheme(getCurrentTheme())
    }
  }
  window.addEventListener('storage', onStorageThemeChange)

  onOcrTextData = (event) => {
    applyPayload(event?.detail)
  }
  window.addEventListener('ocr-text-data', onOcrTextData)

  applyInitialPayload()
  startInitialPayloadTimer()
})

onUnmounted(() => {
  stopInitialPayloadTimer()
  if (onStorageThemeChange) {
    window.removeEventListener('storage', onStorageThemeChange)
    onStorageThemeChange = null
  }
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
  background: rgba(13, 20, 30, 0.96);
  color: #dce8ff;
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
  background: rgba(220, 232, 255, 0.35);
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
  color: #dce8ff;
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

.ocr-text-root.theme-light {
  background: rgba(247, 251, 255, 0.98);
  color: #294268;
}

.ocr-text-root.theme-light .drag-handle {
  background: rgba(64, 99, 158, 0.3);
}

.ocr-text-root.theme-light .ocr-editor {
  color: #2d466d;
}
</style>
