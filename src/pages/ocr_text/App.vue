<template>
  <div class="ocr-text-root" @dblclick.left.stop.prevent="closeWindow">
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
let onOcrTextData = null
let lastDragStartAt = 0

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
  onOcrTextData = (event) => {
    applyPayload(event?.detail)
  }
  window.addEventListener('ocr-text-data', onOcrTextData)
  if (window.__OCR_TEXT_PAYLOAD__) {
    applyPayload(window.__OCR_TEXT_PAYLOAD__)
  }
})

onUnmounted(() => {
  if (onOcrTextData) {
    window.removeEventListener('ocr-text-data', onOcrTextData)
    onOcrTextData = null
  }
})
</script>

<style scoped>
.ocr-text-root {
  box-sizing: border-box;
  padding: 6px 6px 8px 6px;
  width: 100%;
  height: 100%;
  background: rgba(13, 20, 30, 0.96);
  color: #dce8ff;
  display: flex;
  flex-direction: column;
}

.drag-handle-wrap {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 14px;
  margin-bottom: 4px;
}

.drag-handle {
  width: 96px;
  height: 5px;
  border-radius: 999px;
  background: rgba(220, 232, 255, 0.35);
  cursor: move;
}

.ocr-editor {
  width: 100%;
  height: calc(100% - 18px);
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: #dce8ff;
  padding: 12px;
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
</style>
