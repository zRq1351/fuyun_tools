<template>
  <div
      ref="containerRef"
      class="container"
      tabindex="-1"
      @mousedown="handleContainerMouseDown"
      @keydown="handleKeydown"
  >
    <ClipboardToolbar
        v-model:category-filter="categoryFilter"
        v-model:new-category-name="newCategoryName"
        v-model:search-keyword="searchKeyword"
        :can-delete-category="canDeleteCategory"
        :cancel-create-category="cancelCreateCategory"
        :categories="categories"
        :confirm-create-category="confirmCreateCategory"
        :handle-drop="handleDrop"
        :is-adding-category="isAddingCategory"
        :new-category-input-ref="newCategoryInputRef"
        :remove-category="removeCategory"
        :start-create-category="startCreateCategory"
        :start-window-offset-drag="startWindowOffsetDrag"
        :is-ai-settings-collapsed="isAiSettingsCollapsed"
        :toggle-ai-settings="toggleAiSettings"
        :translation-target-language="translationTargetLanguage"
        :explanation-target-language="explanationTargetLanguage"
    />
    <div v-show="!isAiSettingsCollapsed" class="ai-quick-panel-wrap" @click.stop @mousedown.stop>
      <div class="ai-quick-panel">
        <div class="ai-quick-top">
        <div class="ai-control-item ai-select-item">
          <span class="ai-control-label">翻译目标</span>
          <el-select
              v-model="translationTargetLanguage"
              class="ai-select"
              size="small"
              popper-class="clipboard-ai-select-popper"
          >
            <el-option label="简体中文" value="简体中文"/>
            <el-option label="繁体中文" value="繁体中文"/>
            <el-option label="英语" value="英语"/>
            <el-option label="日语" value="日语"/>
            <el-option label="韩语" value="韩语"/>
            <el-option label="法语" value="法语"/>
            <el-option label="德语" value="德语"/>
          </el-select>
        </div>
        <div class="ai-control-item ai-select-item">
          <span class="ai-control-label">解释语言</span>
          <el-select
              v-model="explanationTargetLanguage"
              class="ai-select"
              size="small"
              popper-class="clipboard-ai-select-popper"
          >
            <el-option label="中文" value="中文"/>
            <el-option label="英文" value="英文"/>
            <el-option label="日文" value="日文"/>
            <el-option label="韩文" value="韩文"/>
          </el-select>
        </div>
        <div class="ai-shortcut-tip">选中记录后按：T 翻译 / E 解释</div>
      </div>
      </div>
    </div>

    <div v-if="visibleHistory.length === 0" class="empty-state">
      <el-empty :image-size="100" description="暂无剪切板记录">
        <template #description>
          <p>暂无剪切板记录</p>
          <p class="hint">复制内容后会自动添加</p>
        </template>
      </el-empty>
    </div>

    <ClipboardList
        v-else
        ref="clipboardListRef"
        class="history-list"
        :delete-item="deleteItem"
        :get-item-category="getItemCategory"
        :handle-drag-end="handleDragEnd"
        :handle-drag-start="handleDragStart"
        :select-and-fill-direct="selectAndFillDirect"
        :selected-index="selectedIndex"
        :show-context-menu="showContextMenu"
        :update-selection="updateSelection"
        :visible-history="visibleHistory"
    />

    <div class="status-footer" @click.stop @mousedown.stop>
      <div class="status-text">
        <span class="status-label">{{ selectedStatusText }}</span>
        <div class="status-actions">
          <button aria-label="回到开头" class="nav-action-btn icon-btn" title="回到开头" type="button"
                  @click="scrollToStart">
            <el-icon>
              <ArrowLeftBold/>
            </el-icon>
          </button>
          <button aria-label="滑动到最后" class="nav-action-btn icon-btn" title="滑动到最后" type="button"
                  @click="scrollToEnd">
            <el-icon>
              <ArrowRightBold/>
            </el-icon>
          </button>
        </div>
      </div>
    </div>

    <div
        v-if="contextMenuVisible"
        :style="{ top: contextMenuY + 'px', left: contextMenuX + 'px' }"
        class="context-menu"
        @click.stop
        @mousedown.stop
    >
      <div class="context-menu-header">AI 快捷处理</div>
      <div class="context-menu-item" @click="triggerAiFromContextMenu('translate')">
        翻译
        <span class="shortcut-hint">T</span>
      </div>
      <div class="context-menu-item" @click="triggerAiFromContextMenu('explain')">
        解释
        <span class="shortcut-hint">E</span>
      </div>
      <div class="context-menu-divider"></div>
      <div class="context-menu-header">添加到分类</div>
      <div
          v-for="category in categories"
          :key="category"
          class="context-menu-item"
          @click="assignToCategory(category)"
      >
        {{ category }}
        <el-icon v-if="getItemCategory(contextMenuItem) === category" class="check-icon">
          <Check/>
        </el-icon>
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onMounted, ref} from 'vue'
import {ArrowLeftBold, ArrowRightBold, Check} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {AIService, ClipboardService, WindowService} from '../../services/ipc'
import {handleAppError} from '../../utils/errorHandler'
import ClipboardToolbar from './components/ClipboardToolbar.vue'
import ClipboardList from './components/ClipboardList.vue'
import {useClipboardHistory} from './composables/useClipboardHistory'
import {useCategoryManager} from './composables/useCategoryManager'
import {useWindowOffset} from './composables/useWindowOffset'

const containerRef = ref(null)
const clipboardListRef = ref(null)
const isVisible = ref(false)
const categories = ref(['未分类'])

const contextMenuVisible = ref(false)
const contextMenuX = ref(0)
const contextMenuY = ref(0)
const contextMenuItem = ref(null)
const dragItem = ref(null)
const aiActionLoading = ref(false)
const isAiSettingsCollapsed = ref(true)
const translationTargetLanguage = ref(localStorage.getItem('clipboard_ai_target_language') || '简体中文')
const explanationTargetLanguage = ref(localStorage.getItem('clipboard_ai_explain_language') || '中文')

const {
  history,
  selectedIndex,
  searchKeyword,
  categoryFilter,
  categoryMap,
  visibleHistory,
  getItemCategory,
  updateSelection,
  deleteItem: originalDeleteItem,
  moveSelection
} = useClipboardHistory()

const {
  isAddingCategory,
  newCategoryName,
  newCategoryInputRef,
  setItemCategory,
  removeItemCategory,
  removeCategory,
  canDeleteCategory,
  startCreateCategory,
  confirmCreateCategory,
  cancelCreateCategory
} = useCategoryManager(categories, categoryMap, categoryFilter)

const {
  bottomOffset,
  clampBottomOffset,
  startWindowOffsetDrag
} = useWindowOffset()

const deleteItem = async (index) => {
  await originalDeleteItem(index)
}

const toggleAiSettings = () => {
  isAiSettingsCollapsed.value = !isAiSettingsCollapsed.value
}

const hideClipboardWindow = () => {
  isVisible.value = false
  isAiSettingsCollapsed.value = true
}

const selectedStatusText = computed(() => {
  const total = visibleHistory.value.length
  if (total === 0) return '当前无选中项'
  const current = visibleHistory.value.findIndex((entry) => entry.index === selectedIndex.value)
  const display = current >= 0 ? current + 1 : 1
  return `当前选中：第 ${display} / ${total} 条`
})

const init = async () => {
  try {
    await listen('show-window', (event) => {
      showWindow(event.payload)
    })

    window.addEventListener('blur', async () => {
      try {
        await WindowService.blur()
        hideClipboardWindow()
      } catch (error) {
        console.error('调用 window_blur 失败:', error)
      }
    })
  } catch (error) {
    console.error('初始化失败:', error)
  }
}

const showWindow = (data) => {
  history.value = Array.isArray(data.history) ? data.history : []
  if (typeof data.bottomOffset === 'number') {
    bottomOffset.value = clampBottomOffset(data.bottomOffset)
  }
  if (data.categories) {
    categoryMap.value = data.categories
  }
  if (Array.isArray(data.category_list)) {
    const list = data.category_list.filter(c => c !== '未分类' && c !== '全部')
    const uniqueList = Array.from(new Set(list))
    categories.value = ['未分类', ...uniqueList]
  } else {
    if (data.categories) {
      const extractedCategories = Object.values(data.categories)
      const uniqueList = Array.from(new Set(extractedCategories)).filter(c => c !== '未分类' && c !== '全部')
      categories.value = ['未分类', ...uniqueList]
    }
  }

  selectedIndex.value = data.selectedIndex !== undefined ? data.selectedIndex : 0
  isVisible.value = true

  if (history.value.length > 0) {
    if (selectedIndex.value < 0 || selectedIndex.value >= history.value.length) {
      selectedIndex.value = 0
    }
    const contentRef = clipboardListRef.value?.contentRef
    updateSelection(selectedIndex.value, true, contentRef)
  }

  nextTick(() => {
    containerRef.value?.focus()
  })
}

const selectAndFillDirect = async (index) => {
  try {
    await ClipboardService.selectAndFill(index)
    hideClipboardWindow()
  } catch (error) {
    console.error('填充内容失败:', error)
  }
}

const showContextMenu = (event, item) => {
  contextMenuVisible.value = true
  contextMenuItem.value = item

  const menuWidth = 160
  const maxMenuHeight = Math.min(300, window.innerHeight * 0.6)

  let x = event.clientX
  let y = event.clientY

  if (x + menuWidth > window.innerWidth) {
    x -= menuWidth
  }

  if (y + maxMenuHeight > window.innerHeight) {
    y -= maxMenuHeight
  }

  contextMenuX.value = x
  contextMenuY.value = y
}

const closeContextMenu = () => {
  contextMenuVisible.value = false
  contextMenuItem.value = null
}

const closeFloatingPanels = () => {
  closeContextMenu()
  isAiSettingsCollapsed.value = true
}

const handleContainerMouseDown = (event) => {
  if (event.button !== 0) return
  const target = event.target
  if (target instanceof Element && target.closest('.clipboard-ai-select-popper')) {
    return
  }
  closeFloatingPanels()
}

const assignToCategory = (category) => {
  if (contextMenuItem.value && category !== '全部') {
    setItemCategory(contextMenuItem.value, category)
  }
  closeContextMenu()
}

const handleDragStart = (event, item) => {
  dragItem.value = item
  event.dataTransfer.effectAllowed = 'copy'
  event.dataTransfer.setData('text/plain', item)
}

const handleDragEnd = () => {
  dragItem.value = null
}

const handleDrop = (event, category) => {
  event.preventDefault()

  const target = event.currentTarget
  if (target && target.classList.contains('category-pill')) {
    target.classList.remove('drag-over')
  }

  if (dragItem.value && category !== '全部') {
    setItemCategory(dragItem.value, category)
  }
}

const buildOpId = () => Date.now() * 1000 + Math.floor(Math.random() * 1000)

const resolveSelectedText = () => {
  if (selectedIndex.value < 0 || selectedIndex.value >= history.value.length) {
    return ''
  }
  return history.value[selectedIndex.value] || ''
}

const triggerAiFlow = async (rawText, mode) => {
  const text = typeof rawText === 'string' ? rawText.trim() : ''
  if (!text || aiActionLoading.value) return
  aiActionLoading.value = true
  try {
    await WindowService.blur()
    hideClipboardWindow()
    const opId = buildOpId()
    localStorage.setItem('clipboard_ai_target_language', translationTargetLanguage.value)
    localStorage.setItem('clipboard_ai_explain_language', explanationTargetLanguage.value)
    if (mode === 'translate') {
      await AIService.streamTranslate(
          text,
          '自动识别',
          translationTargetLanguage.value,
          opId
      )
    } else {
      await AIService.streamExplain(
          text,
          explanationTargetLanguage.value,
          opId
      )
    }
  } catch (error) {
    handleAppError(error, mode === 'translate' ? '剪贴板翻译失败' : '剪贴板解释失败')
  } finally {
    aiActionLoading.value = false
  }
}

const triggerAiFromSelection = async (mode) => {
  const text = resolveSelectedText()
  await triggerAiFlow(text, mode)
}

const triggerAiFromContextMenu = async (mode) => {
  const text = contextMenuItem.value || ''
  closeContextMenu()
  await triggerAiFlow(text, mode)
}

const isInputLikeTarget = (target) => {
  const tagName = target?.tagName?.toLowerCase?.()
  return tagName === 'input' || tagName === 'textarea' || target?.isContentEditable
}

const ensureKeyboardSelectionVisible = async () => {
  await nextTick()
  const selected = selectedIndex.value
  if (selected < 0) return
  const element = document.getElementById(`clipboard-item-${selected}`)
  const containerRefOrEl = clipboardListRef.value?.contentRef
  const container = containerRefOrEl?.value || containerRefOrEl || element?.closest('.content')
  if (!element || !container) return
  const EDGE_PADDING = 8
  const maxScrollLeft = Math.max(0, container.scrollWidth - container.clientWidth)
  const targetLeft = Math.max(0, element.offsetLeft - EDGE_PADDING)
  container.scrollLeft = Math.min(maxScrollLeft, targetLeft)
}

const getContentContainer = () => {
  const containerRefOrEl = clipboardListRef.value?.contentRef
  return containerRefOrEl?.value || containerRefOrEl || null
}

const scrollToStart = async () => {
  const container = getContentContainer()
  if (container) {
    container.scrollLeft = 0
  }
  if (visibleHistory.value.length > 0) {
    selectedIndex.value = visibleHistory.value[0].index
    await ensureKeyboardSelectionVisible()
  }
}

const scrollToEnd = async () => {
  const container = getContentContainer()
  if (container) {
    container.scrollLeft = Math.max(0, container.scrollWidth - container.clientWidth)
  }
  if (visibleHistory.value.length > 0) {
    selectedIndex.value = visibleHistory.value[visibleHistory.value.length - 1].index
    await ensureKeyboardSelectionVisible()
  }
}

const handleKeydown = async (event) => {
  if (!isVisible.value) return
  if (isInputLikeTarget(event.target)) return

  if (contextMenuVisible.value && event.key === 'Escape') {
    closeContextMenu()
    return
  }

  switch (event.key) {
    case 'ArrowLeft':
      event.preventDefault()
      moveSelection(-1, clipboardListRef.value?.contentRef)
      await ensureKeyboardSelectionVisible()
      break
    case 'ArrowRight':
      event.preventDefault()
      moveSelection(1, clipboardListRef.value?.contentRef)
      await ensureKeyboardSelectionVisible()
      break
    case 'Enter':
      event.preventDefault()
      if (selectedIndex.value >= 0 && selectedIndex.value < history.value.length) {
        const visibleIndex = visibleHistory.value.findIndex((entry) => entry.index === selectedIndex.value)
        if (visibleIndex >= 0) {
          selectAndFillDirect(selectedIndex.value)
        }
      }
      break
    case 't':
    case 'T':
      event.preventDefault()
      await triggerAiFromSelection('translate')
      break
    case 'e':
    case 'E':
      event.preventDefault()
      await triggerAiFromSelection('explain')
      break
  }
}

onMounted(() => {
  init()
})
</script>

<style>
::-webkit-scrollbar {
  display: none !important;
  width: 0 !important;
  height: 0 !important;
}

html, body {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  scrollbar-width: none;
}

#app {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
}

.clipboard-ai-select-popper {
  border: 1px solid rgba(255, 255, 255, 0.14) !important;
  border-radius: 10px !important;
  background: linear-gradient(150deg, rgba(30, 36, 50, 0.98), rgba(21, 27, 40, 0.96)) !important;
  backdrop-filter: blur(10px);
}

.clipboard-ai-select-popper .el-select-dropdown__item {
  color: #d5e0f4 !important;
}

.clipboard-ai-select-popper .el-select-dropdown__item.hover,
.clipboard-ai-select-popper .el-select-dropdown__item:hover {
  background: rgba(64, 158, 255, 0.2) !important;
  color: #ffffff !important;
}

.clipboard-ai-select-popper .el-select-dropdown__item.selected,
.clipboard-ai-select-popper .el-select-dropdown__item.is-selected {
  color: #a9d7ff !important;
  font-weight: 700;
  background: rgba(64, 158, 255, 0.26) !important;
}

</style>

<style scoped>
.container {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: linear-gradient(160deg, rgba(20, 24, 32, 0.72), rgba(12, 14, 20, 0.66));
  backdrop-filter: blur(22px) saturate(140%);
  -webkit-backdrop-filter: blur(22px) saturate(140%);
  border: 1px solid rgba(255, 255, 255, 0.14);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.16), 0 10px 28px rgba(0, 0, 0, 0.26);
  overflow: hidden;
  outline: none;
}

.container > * {
  min-width: 0;
}

.empty-state {
  flex: 1;
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 0;
  color: #fff;
}

.ai-quick-panel-wrap {
  position: relative;
  height: 0;
  margin: 0 8px;
  z-index: 50;
}

.ai-quick-panel {
  position: absolute;
  top: 4px;
  left: 0;
  width: min(560px, calc(100vw - 36px));
  padding: 8px;
  border-radius: 10px;
  background: linear-gradient(150deg, rgba(32, 39, 55, 0.96), rgba(20, 25, 36, 0.94));
  border: 1px solid rgba(255, 255, 255, 0.12);
  box-shadow: 0 10px 24px rgba(0, 0, 0, 0.35);
  backdrop-filter: blur(10px);
}

.history-list {
  flex: 1;
  min-height: 0;
}

.status-footer {
  flex: 0 0 auto;
  min-height: 44px;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 8px;
  padding: 8px 10px;
  position: sticky;
  bottom: 0;
  left: 0;
  right: 0;
  z-index: 120;
}

.status-text {
  flex: 1 1 0;
  min-width: 0;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: rgba(233, 244, 255, 0.92);
}

.status-label {
  flex: 0 1 auto;
  min-width: 0;
  width: 150px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-variant-numeric: tabular-nums;
}

.status-actions {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-left: 12px;
}

.nav-action-btn {
  appearance: none;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: transparent;
  color: #f1f7ff;
  border-radius: 7px;
  font-size: 12px;
  line-height: 1;
  font-weight: 700;
  padding: 9px 14px;
  min-height: 32px;
  cursor: pointer;
  transition: background 0.2s ease, border-color 0.2s ease, box-shadow 0.2s ease;
  box-shadow: none;
}

.icon-btn {
  flex: 0 0 auto;
  width: 36px;
  height: 34px;
  min-width: 36px;
  padding: 0;
  border-radius: 8px;
  font-size: 16px;
  line-height: 1;
  justify-content: center;
  display: inline-flex;
  align-items: center;
  font-weight: 800;
}

.nav-action-btn:hover {
  border-color: rgba(127, 194, 255, 0.5);
  background: linear-gradient(135deg, rgba(28, 36, 52, 0.9), rgba(35, 45, 63, 0.84));
  color: #ffffff;
  box-shadow: 0 0 0 1px rgba(127, 194, 255, 0.18);
}

.nav-action-btn:focus-visible {
  outline: 2px solid rgba(180, 226, 255, 0.95);
  outline-offset: 2px;
}

.ai-quick-top {
  display: grid;
  grid-template-columns: max-content max-content minmax(0, 1fr);
  align-items: center;
  gap: 10px;
  width: 100%;
  min-width: 0;
}

.ai-control-item {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #cfd8ea;
  font-size: 12px;
  min-width: 0;
}

.ai-select-item {
  padding: 4px 8px;
  border-radius: 8px;
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.08);
  flex: 0 0 auto;
}

.ai-control-label {
  color: #b8c6de;
  white-space: nowrap;
  font-weight: 600;
  letter-spacing: 0.2px;
}

.ai-shortcut-tip {
  margin-top: 0;
  justify-self: end;
  white-space: nowrap;
  font-size: 11px;
  color: rgba(208, 220, 241, 0.72);
}

:deep(.ai-select) {
  width: 112px;
}

:deep(.ai-select .el-select__wrapper) {
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.16);
  border-radius: 8px;
  box-shadow: none;
}

:deep(.ai-select .el-select__wrapper:hover),
:deep(.ai-select .el-select__wrapper.is-focused) {
  background: rgba(17, 23, 34, 0.78);
  border-color: rgba(255, 255, 255, 0.16);
}

:deep(.ai-select .el-select__selected-item) {
  color: #e7efff;
  font-size: 12px;
}

:deep(.ai-select .el-select__placeholder) {
  color: rgba(215, 226, 244, 0.62);
}

.hint {
  color: #909399;
  font-size: 12px;
}

.context-menu {
  position: fixed;
  z-index: 2000;
  background: rgba(30, 30, 35, 0.95);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
  padding: 4px 0;
  min-width: 120px;
  backdrop-filter: blur(10px);
  color: #e5e7eb;
}

.context-menu-header {
  padding: 4px 12px;
  font-size: 12px;
  color: #909399;
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  margin-bottom: 4px;
}

.context-menu-item {
  padding: 6px 12px;
  font-size: 13px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: space-between;
  transition: background 0.2s;
}

.context-menu-item:hover {
  background: var(--el-color-primary, #409eff);
  color: #fff;
}

.context-menu-divider {
  height: 1px;
  margin: 4px 0;
  background: rgba(255, 255, 255, 0.1);
}

.shortcut-hint {
  font-size: 11px;
  opacity: 0.75;
}

.check-icon {
  font-size: 12px;
}
</style>
