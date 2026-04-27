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
        <GripHorizontal :size="16" :stroke-width="2" />
      </div>
      <template v-if="!isEditing">
        <button class="viewer-action-btn" @mousedown.left.stop.prevent @click.stop="startEdit">编辑</button>
        <button class="viewer-action-btn" @mousedown.left.stop.prevent @click.stop="requestClose(true)">关闭</button>
      </template>
      <template v-else>
        <button class="viewer-action-btn primary" @mousedown.left.stop.prevent @click.stop="saveEdit">保存</button>
        <button class="viewer-action-btn" @mousedown.left.stop.prevent @click.stop="cancelEdit">取消</button>
      </template>
    </div>
    <div
        :class="['viewer-card', animationState]"
        @click.stop
    >
      <div class="preview-content">
        <textarea
            v-if="isEditing"
            v-model="editableText"
            class="edit-textarea"
            ref="textareaRef"
            spellcheck="false"
        ></textarea>
        <FormattedContent v-else :content="textContent" />
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, ref} from 'vue'
import { GripHorizontal } from 'lucide-vue-next'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {ImageClipboardService, ClipboardService} from '../../services/ipc'
import FormattedContent from '../../components/FormattedContent.vue'
import {ElMessage} from 'element-plus'

const currentWindow = getCurrentWebviewWindow()
const animationState = ref('entering')
const textContent = ref('')
const itemId = ref(null)
const editableText = ref('')
const isEditing = ref(false)
const textareaRef = ref(null)

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

const startEdit = async () => {
  const container = document.querySelector('.preview-content')
  const currentScrollTop = container ? container.scrollTop : 0

  editableText.value = textContent.value
  isEditing.value = true
  await nextTick()
  if (textareaRef.value) {
    textareaRef.value.focus({ preventScroll: true })
    textareaRef.value.setSelectionRange(0, 0)
    textareaRef.value.scrollTop = currentScrollTop
  }
}

const cancelEdit = async () => {
  const currentScrollTop = textareaRef.value ? textareaRef.value.scrollTop : 0

  isEditing.value = false
  editableText.value = ''

  await nextTick()
  const container = document.querySelector('.preview-content')
  if (container) {
    container.scrollTop = currentScrollTop
  }
}

const saveEdit = async () => {
  if (editableText.value.trim() === '') {
    ElMessage.warning('内容不能为空')
    return
  }
  if (editableText.value === textContent.value) {
    cancelEdit()
    return
  }
  
  const currentScrollTop = textareaRef.value ? textareaRef.value.scrollTop : 0

  try {
    if (itemId.value) {
      await ImageClipboardService.updateTextItem(itemId.value, editableText.value)
    } else {
      await ClipboardService.copyText(editableText.value)
    }
    
    textContent.value = editableText.value
    isEditing.value = false
    ElMessage.success('保存成功')

    await nextTick()
    const container = document.querySelector('.preview-content')
    if (container) {
      container.scrollTop = currentScrollTop
    }
  } catch (error) {
    console.error('保存修改失败:', error)
    ElMessage.error('保存失败')
  }
}

const handleKeydown = (e) => {
  if (e.key === 'Escape') {
    if (isEditing.value) {
      cancelEdit()
    } else {
      requestClose()
    }
  } else if ((e.ctrlKey || e.metaKey) && e.key === 's' && isEditing.value) {
    e.preventDefault()
    saveEdit()
  }
}

onMounted(async () => {
  window.addEventListener('keydown', handleKeydown)

  unlisten = await listen('show-text-preview', (event) => {
    const payload = event.payload || {}
    if (payload.text) {
      textContent.value = payload.text
    }
    if (payload.item_id) {
      itemId.value = payload.item_id
    } else {
      itemId.value = null
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
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 4px 6px;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.4);
  color: rgba(255, 255, 255, 0.8);
  cursor: grab;
  transition: background-color 0.2s, color 0.2s;
}

.viewer-drag-icon:active {
  cursor: grabbing;
}

.viewer-drag-icon:hover {
  background: rgba(0, 0, 0, 0.6);
  color: rgba(255, 255, 255, 1);
}

.viewer-action-btn {
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

.viewer-action-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  border-color: rgba(255, 255, 255, 0.2);
}

.viewer-action-btn.primary {
  background: rgba(64, 158, 255, 0.8);
  border-color: rgba(64, 158, 255, 0.5);
}

.viewer-action-btn.primary:hover {
  background: rgba(64, 158, 255, 1);
  border-color: rgba(64, 158, 255, 0.8);
}

.edit-textarea {
  width: 100%;
  height: 100%;
  background: transparent;
  border: none;
  outline: none;
  color: #dcdfe6;
  font-size: 15px;
  line-height: 1.6;
  font-family: inherit;
  resize: none;
}

.viewer-card {
  width: calc(100vw - 32px);
  height: calc(100vh - 32px);
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
  min-height: 0;
  padding: 40px;
  padding-top: 60px;
  overflow-y: auto;
  color: #dcdfe6;
  font-size: 15px;
  line-height: 1.6;
}

.viewer-card ::-webkit-scrollbar {
  display: block !important;
  width: 8px !important;
  height: 8px !important;
}

.viewer-card ::-webkit-scrollbar-track {
  background: transparent !important;
}

.viewer-card ::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2) !important;
  border-radius: 4px !important;
}

.viewer-card ::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.3) !important;
}
</style>
