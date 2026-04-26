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
        <span></span><span></span><span></span>
        <span></span><span></span><span></span>
      </div>
      <button class="viewer-close" @mousedown.left.stop.prevent @click.stop="requestClose(true)">关闭</button>
    </div>
    <div
        :class="['viewer-card', animationState]"
        @click.stop
    >
      <div class="preview-content" v-html="renderedHtml"></div>
    </div>
  </div>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, ref} from 'vue'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {ImageClipboardService} from '../../services/ipc'
import {marked} from 'marked'

const currentWindow = getCurrentWebviewWindow()
const animationState = ref('entering')
const textContent = ref('')

const renderer = new marked.Renderer()
const renderedHtml = computed(() => {
  return marked.parse(textContent.value || '', {
    renderer,
    gfm: true,
    breaks: true
  })
})

let unlisten = null

const startWindowDrag = async () => {
  try {
    await currentWindow.startDragging()
  } catch (e) {
    console.warn('tauri drag failed, falling back to ipc:', e)
    ImageClipboardService.startTextPreviewWindowDrag()
  }
}

const requestClose = (fromButton = false) => {
  if (animationState.value === 'leaving') return
  animationState.value = 'leaving'
  setTimeout(() => {
    ImageClipboardService.closeTextPreviewWindow()
  }, 200)
}

const handleKeydown = (e) => {
  if (e.key === 'Escape') {
    requestClose()
  }
}

onMounted(async () => {
  window.addEventListener('keydown', handleKeydown)

  unlisten = await listen('show-text-preview', (event) => {
    const payload = event.payload || {}
    if (payload.text) {
      textContent.value = payload.text
    }
    setTimeout(() => {
      animationState.value = 'entered'
    }, 50)
  })
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleKeydown)
  if (unlisten) unlisten()
})
</script>

<style>
@import "../shared/windowBase.css";

.viewer-root {
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
}

.viewer-drag-strip {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 40px;
  z-index: 10;
  cursor: grab;
  background: transparent;
}

.viewer-drag-strip:active {
  cursor: grabbing;
}

.viewer-topbar {
  position: absolute;
  top: 20px;
  right: 30px;
  display: flex;
  align-items: center;
  gap: 16px;
  z-index: 20;
}

.viewer-drag-icon {
  width: 24px;
  height: 16px;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 2px;
  padding: 4px;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.4);
  cursor: grab;
  transition: background-color 0.2s;
}

.viewer-drag-icon:active {
  cursor: grabbing;
}

.viewer-drag-icon:hover {
  background: rgba(0, 0, 0, 0.6);
}

.viewer-drag-icon span {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.6);
}

.viewer-close {
  background: rgba(0, 0, 0, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: white;
  padding: 6px 16px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  transition: all 0.2s;
  user-select: none;
}

.viewer-close:hover {
  background: rgba(255, 69, 58, 0.8);
  border-color: rgba(255, 69, 58, 0.5);
}

.viewer-card {
  width: 100%;
  height: 100%;
  background: linear-gradient(160deg, rgba(20, 24, 32, 0.95), rgba(12, 14, 20, 0.95));
  border-radius: 12px;
  box-shadow: 0 20px 40px rgba(0, 0, 0, 0.4), 0 0 0 1px rgba(255, 255, 255, 0.1);
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
  transition: transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1), opacity 0.2s ease-out, border-radius 0.3s ease;
  margin: 16px;
}

.viewer-card.entering {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}

.viewer-card.entered {
  transform: scale(1) translateY(0);
  opacity: 1;
}

.viewer-card.leaving {
  transform: scale(0.95) translateY(-10px);
  opacity: 0;
}

.preview-content {
  flex: 1;
  padding: 40px;
  padding-top: 60px;
  overflow-y: auto;
  color: #dcdfe6;
  font-size: 15px;
  line-height: 1.6;
}

.preview-content pre {
  background: rgba(0, 0, 0, 0.3);
  padding: 12px;
  border-radius: 6px;
  overflow-x: auto;
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.preview-content code {
  font-family: 'Courier New', Courier, monospace;
  background: rgba(0, 0, 0, 0.2);
  padding: 2px 4px;
  border-radius: 4px;
}

.preview-content blockquote {
  border-left: 4px solid rgba(255, 255, 255, 0.2);
  margin: 0;
  padding-left: 16px;
  color: rgba(255, 255, 255, 0.6);
}

.preview-content table {
  border-collapse: collapse;
  width: 100%;
  margin-bottom: 16px;
}

.preview-content th,
.preview-content td {
  border: 1px solid rgba(255, 255, 255, 0.2);
  padding: 8px;
}

.preview-content a {
  color: #409eff;
  text-decoration: none;
}

.preview-content a:hover {
  text-decoration: underline;
}

::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.3);
}
</style>
