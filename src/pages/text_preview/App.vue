<template>
  <div
      class="viewer-root"
      @click="requestClose"
  >
    <div
        class="viewer-drag-strip"
        data-tauri-drag-region
        @click.stop
    />
    <div
        class="viewer-topbar"
        @click.stop
    >
      <div
          class="viewer-drag-icon"
          data-tauri-drag-region
          :title="t('textPreview.dragWindow')"
          @mousedown.left.stop.prevent="startWindowDrag"
      >
        <GripHorizontal
            :size="16"
            :stroke-width="2"
        />
      </div>
      <div
          v-if="displayLabel"
          class="viewer-pos"
          title="序号"
      >
        {{ displayLabel }}
      </div>
      <template v-if="!isEditing">
        <button
            class="viewer-action-btn"
            @mousedown.left.stop.prevent
            @click.stop="startEdit"
        >
          {{
            t('textPreview.edit')
          }}
        </button>
        <button
            class="viewer-action-btn"
            @mousedown.left.stop.prevent
            @click.stop="requestClose"
        >
          {{ t('textPreview.close') }}
        </button>
      </template>
      <template v-else>
        <button
            class="viewer-action-btn primary"
            @mousedown.left.stop.prevent
            @click.stop="saveEdit"
        >
          {{ t('textPreview.save') }}
        </button>
        <button
            class="viewer-action-btn"
            @mousedown.left.stop.prevent
            @click.stop="cancelEdit"
        >
          {{ t('textPreview.cancel') }}
        </button>
      </template>
    </div>
    <div
        :class="['viewer-card', animationState]"
        @click.stop
    >
      <div class="preview-content">
        <textarea
            v-if="isEditing"
            ref="textareaRef"
            v-model="editableText"
            class="edit-textarea"
            spellcheck="false"
        />
        <FormattedContent
            v-else
            :content="textContent"
        />
      </div>
    </div>
    <button
        v-if="hasPrev && !isEditing"
        class="viewer-nav viewer-nav-prev"
        type="button"
        title="上一张"
        @mousedown.left.stop.prevent
        @click.stop.prevent="goBy(-1)"
    >
      <ChevronLeft
          :size="22"
          :stroke-width="2"
      />
    </button>
    <button
        v-if="hasNext && !isEditing"
        class="viewer-nav viewer-nav-next"
        type="button"
        title="下一张"
        @mousedown.left.stop.prevent
        @click.stop.prevent="goBy(1)"
    >
      <ChevronRight
          :size="22"
          :stroke-width="2"
      />
    </button>
  </div>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, ref} from 'vue'
import {ChevronLeft, ChevronRight, GripHorizontal} from 'lucide-vue-next'
import {listen, emit} from '@tauri-apps/api/event'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {useI18n} from 'vue-i18n'
import {ClipboardService, ImageClipboardService} from '../../services/ipc'
import FormattedContent from '../../components/FormattedContent.vue'
import {ElMessage} from 'element-plus'

const {t} = useI18n()
const currentWindow = getCurrentWebviewWindow()
const animationState = ref('entering')
const textContent = ref('')
const itemId = ref(null)
const editableText = ref('')
const isEditing = ref(false)
const textareaRef = ref(null)
const hasPrev = ref(false)
const hasNext = ref(false)
const displayIndex = ref(0)
const displayTotal = ref(0)

// 与卡片序号同源的显示文案，如 "5/30"
const displayLabel = computed(() => {
  if (!displayIndex.value || !displayTotal.value) return ''
  return `${displayIndex.value}/${displayTotal.value}`
})

// 上一张/下一张：邻居由剪贴板主窗口按当前卡片列表现算，保证与卡片展示顺序一致
const goBy = (delta) => {
  if (isEditing.value) return
  const dir = delta === -1 ? -1 : 1
  // 边界防护：第一张没有上一张、最后一张没有下一张
  if (dir === -1 && !hasPrev.value) return
  if (dir === 1 && !hasNext.value) return
  emit('text-preview-navigate', {delta: dir}).catch((error) => {
    console.error('发送预览导航请求失败:', error)
  })
}

let unlisten = null
let unlistenReplaced = null

const startWindowDrag = async () => {
  try {
    await currentWindow.startDragging()
  } catch (e) {
    console.warn('tauri drag failed, falling back to ipc:', e)
    ImageClipboardService.startTextPreviewWindowDrag()
  }
}

const requestClose = () => {
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
    ElMessage.warning(t('textPreview.contentEmpty'))
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
    ElMessage.success(t('textPreview.saveSuccess'))

    await nextTick()
    const container = document.querySelector('.preview-content')
    if (container) {
      container.scrollTop = currentScrollTop
    }
  } catch (error) {
    console.error('保存修改失败:', error)
    ElMessage.error(t('textPreview.saveFailed'))
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
  } else if (!isEditing.value && e.key === 'ArrowLeft') {
    e.preventDefault()
    goBy(-1)
  } else if (!isEditing.value && e.key === 'ArrowRight') {
    e.preventDefault()
    goBy(1)
  }
}

onMounted(async () => {
  window.addEventListener('keydown', handleKeydown)

  const processPayload = (payload) => {
    if (payload.text) {
      textContent.value = payload.text
    }
    if (payload.item_id) {
      itemId.value = payload.item_id
    } else {
      itemId.value = null
    }
    // 边界导航能力随载荷下发（第一张无上一张、最后一张无下一张）
    if (typeof payload.has_prev === 'boolean') {
      hasPrev.value = payload.has_prev
    }
    if (typeof payload.has_next === 'boolean') {
      hasNext.value = payload.has_next
    }
    // 与卡片一致的序号（1 起位置/总数）
    if (Number.isFinite(payload.display_index) && payload.display_index > 0) {
      displayIndex.value = payload.display_index
    }
    if (Number.isFinite(payload.display_total) && payload.display_total > 0) {
      displayTotal.value = payload.display_total
    }
    setTimeout(() => {
      animationState.value = 'entered'
    }, 50)
  }

  const cachedPayload = window.__TEXT_PREVIEW_PAYLOAD__
  if (cachedPayload) {
    processPayload(cachedPayload)
  }

  unlisten = await listen('show-text-preview', (event) => {
    processPayload(event.payload || {})
  })

  // 监听文本编辑后 ID 变更事件
  unlistenReplaced = await listen('text-item-replaced', (event) => {
    const {old_id, new_id} = event.payload || {}
    if (old_id && new_id && itemId.value === old_id) {
      itemId.value = new_id
    }
  })
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleKeydown)
  if (unlisten) unlisten()
  if (unlistenReplaced) unlistenReplaced()
})
</script>

<style>

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
  border-radius: var(--fy-radius-xs);
  background: var(--fy-bg-overlay);
  color: var(--fy-text-primary);
  cursor: grab;
  transition: background-color 0.15s ease, color 0.15s ease;
}

.viewer-drag-icon:active {
  cursor: grabbing;
}

.viewer-drag-icon:hover {
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
}

.viewer-action-btn {
  background: var(--fy-bg-overlay);
  border: 0.5px solid var(--fy-border-light);
  color: var(--fy-text-primary);
  padding: 6px 16px;
  border-radius: var(--fy-radius-sm);
  cursor: pointer;
  font-size: var(--fy-text-base);
  backdrop-filter: var(--fy-backdrop-blur-light);
  -webkit-backdrop-filter: var(--fy-backdrop-blur-light);
  transition: all 0.15s ease;
  user-select: none;
}

.viewer-action-btn:hover {
  background: var(--fy-bg-hover);
  border-color: var(--fy-border-hover);
}

.viewer-action-btn.primary {
  background: var(--fy-accent);
  border-color: var(--fy-accent-hover);
}

.viewer-action-btn.primary:hover {
  background: var(--fy-accent-hover);
  border-color: var(--fy-accent);
}

.viewer-pos {
  min-width: 56px;
  text-align: center;
  border: 1px solid var(--fy-border);
  background: var(--fy-bg-overlay);
  color: var(--fy-text-primary);
  border-radius: var(--fy-radius-sm);
  padding: 6px 10px;
  font-size: 12px;
  font-weight: 600;
  box-shadow: var(--fy-shadow);
  user-select: none;
}

.viewer-nav {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  z-index: 28;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--fy-border);
  background: var(--fy-bg-overlay);
  color: var(--fy-text-primary);
  border-radius: 50%;
  cursor: pointer;
  box-shadow: var(--fy-shadow);
  opacity: 0.85;
}

.viewer-nav:hover {
  background: var(--fy-bg-hover);
  opacity: 1;
}

.viewer-nav:active:not(:disabled) {
  transform: translateY(-50%) scale(0.96);
}

.viewer-nav-prev {
  left: 16px;
}

.viewer-nav-next {
  right: 16px;
}

.edit-textarea {
  width: 100%;
  height: 100%;
  background: transparent;
  border: none;
  outline: none;
  color: var(--fy-text-primary);
  font-size: var(--fy-text-md);
  line-height: 1.6;
  font-family: inherit;
  resize: none;
}

.viewer-card {
  width: calc(100vw - 32px);
  height: calc(100vh - 32px);
  background: var(--fy-container-bg);
  border-radius: var(--fy-radius-xl);
  box-shadow: var(--fy-shadow-lg), 0 0 0 0.5px var(--fy-container-border);
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
  color: var(--fy-text-primary);
  font-size: var(--fy-text-md);
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
  background: var(--fy-scrollbar-thumb) !important;
  border-radius: 4px !important;
}

.viewer-card ::-webkit-scrollbar-thumb:hover {
  background: var(--fy-scrollbar-thumb-hover) !important;
}

/* 全局交互增强：焦点环 + 过渡 */
button:focus-visible,
[role="button"]:focus-visible,
[tabindex]:focus-visible {
  outline: 2px solid var(--fy-accent);
  outline-offset: 2px;
}

button, [role="button"] {
  transition: transform 0.12s var(--fy-ease-out), filter 0.12s var(--fy-ease-out), opacity 0.15s var(--fy-ease-out);
}

button:active:not(:disabled),
[role="button"]:active:not([aria-disabled="true"]) {
  transform: scale(0.96);
}
</style>
