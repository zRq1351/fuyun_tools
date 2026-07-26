<template>
  <div :class="['ocr-text-root', `theme-${currentTheme}`]" @dblclick.left.stop.prevent="closeWindow">
    <div class="drag-handle-wrap">
      <div class="drag-handle" @mousedown.left.stop.prevent="startDrag"></div>
    </div>
    <textarea
        v-model="text"
        class="ocr-editor"
        :placeholder="t('ocrText.noResult')"
        spellcheck="false"
        @dblclick.left.stop.prevent="closeWindow"
    />
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {useI18n} from 'vue-i18n'
import {ElMessage} from 'element-plus'
import {useTheme} from '../../composables/useTheme'
import {useWindowDrag} from '../../composables/useWindowDrag'

const {t} = useI18n()
const text = ref('')
const {currentTheme} = useTheme()
const {startDrag} = useWindowDrag()

let onOcrTextData = null
let initialPayloadTimer = null
let initialPayloadTryCount = 0

function applyPayload(payload) {
  const value = String(payload?.text || '').trim()
  if (!value) {
    // OCR 结果为空时提示用户并清除旧文本
    if (payload?.text !== undefined) {
      ElMessage.warning(t('ocrText.noResult'))
      text.value = ''
    }
    return
  }
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
  border-radius: 10px;
  backdrop-filter: var(--fy-backdrop-blur-light);
  transition: background 0.25s var(--fy-ease-out), color 0.25s var(--fy-ease-out);
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
  transition: background 0.15s var(--fy-ease-out), width 0.15s var(--fy-ease-out);
}

.drag-handle:hover {
  background: var(--fy-text-muted);
  width: 120px;
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
  font-size: var(--fy-text-base);
  line-height: 1.5;
  user-select: text;
  font-family: var(--fy-font-mono);
  overflow: auto;
  overflow-x: hidden;
  overflow-wrap: anywhere;
  scrollbar-width: none;
  -ms-overflow-style: none;
  transition: color 0.2s var(--fy-ease-out);
}

.ocr-editor::placeholder {
  color: var(--fy-text-muted);
  opacity: 0.5;
}

.ocr-editor:focus {
  box-shadow: inset 0 0 0 1px var(--fy-accent);
  border-radius: 6px;
  outline: none;
}

textarea:focus-visible {
  outline: 2px solid var(--fy-accent);
  outline-offset: -2px;
  border-radius: 6px;
}

.ocr-editor::-webkit-scrollbar {
  width: 0;
  height: 0;
  display: none;
}
</style>
