<template>
  <div :class="['ocr-text-root', `theme-${themeMode}`]" @dblclick.left.stop.prevent="closeWindow">
    <div class="drag-handle-wrap">
      <div class="drag-handle" @mousedown.left.stop.prevent="startDrag"></div>
    </div>
    <textarea
        v-model="text"
        class="ocr-editor"
        spellcheck="false"
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
let lastDragStartAt = 0

function getCurrentTheme() {
  const saved = localStorage.getItem('settings-theme')
  if (saved === 'dark' || saved === 'light') {
    return saved
  }
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
  text.value = String(payload?.text || '').trim()
}

async function closeWindow() {
  try {
    await getCurrentWebviewWindow().close()
  } catch (error) {
    console.error('关闭OCR结果窗口失败:', error)
  }
}

async function startDrag() {
  const now = Date.now()
  if (now - lastDragStartAt < 220) return
  lastDragStartAt = now
  try {
    await getCurrentWebviewWindow().startDragging()
  } catch (error) {
    console.error('系统拖动OCR结果窗口失败:', error)
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
  if (window.__OCR_TEXT_PAYLOAD__) {
    applyPayload(window.__OCR_TEXT_PAYLOAD__)
  }
})

onUnmounted(() => {
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
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: rgba(13, 20, 30, 0.96);
  color: #dce8ff;
  position: relative;
}

.drag-handle-wrap {
  position: absolute;
  top: 6px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 2;
  pointer-events: none;
}

.drag-handle {
  width: 96px;
  height: 5px;
  border-radius: 999px;
  background: rgba(220, 232, 255, 0.35);
  cursor: move;
  pointer-events: auto;
}

.ocr-editor {
  box-sizing: border-box;
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: #dce8ff;
  padding: 20px 12px 12px;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 13px;
  line-height: 1.5;
  user-select: text;
  font-family: 'Consolas', 'Microsoft YaHei', sans-serif;
  overflow: auto;
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
