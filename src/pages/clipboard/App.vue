<template>
  <div
      ref="containerRef"
      class="container"
      tabindex="-1"
      @mousedown="handleContainerMouseDown"
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
        :create-category-text="$t('clipboard.createCategory')"
        :search-placeholder="$t('clipboard.search')"
    />
    <div v-show="!isAiSettingsCollapsed" class="ai-quick-panel-wrap" @click.stop @mousedown.stop>
      <div class="ai-quick-panel">
        <div class="ai-quick-top">
        <div class="ai-control-item ai-select-item">
          <span class="ai-control-label">{{ $t('clipboard.translationTarget') }}</span>
          <el-select
              v-model="translationTargetLanguage"
              class="ai-select"
              size="small"
              popper-class="clipboard-ai-select-popper"
          >
            <el-option :label="$t('language.zhCN')" value="简体中文"/>
            <el-option :label="$t('language.traditionalChinese')" value="繁体中文"/>
            <el-option :label="$t('language.english')" value="英语"/>
            <el-option :label="$t('language.japanese')" value="日语"/>
            <el-option :label="$t('language.korean')" value="韩语"/>
            <el-option :label="$t('language.french')" value="法语"/>
            <el-option :label="$t('language.german')" value="德语"/>
          </el-select>
        </div>
        <div class="ai-control-item ai-select-item">
          <span class="ai-control-label">{{ $t('clipboard.explanationLang') }}</span>
          <el-select
              v-model="explanationTargetLanguage"
              class="ai-select"
              size="small"
              popper-class="clipboard-ai-select-popper"
          >
            <el-option :label="$t('language.chinese')" value="中文"/>
            <el-option :label="$t('language.englishDesc')" value="英文"/>
            <el-option :label="$t('language.japaneseDesc')" value="日文"/>
            <el-option :label="$t('language.koreanDesc')" value="韩文"/>
          </el-select>
        </div>
          <div class="ai-shortcut-tip">{{ $t('clipboard.shortcutTip') }}</div>
      </div>
      </div>
    </div>

    <div v-if="visibleHistory.length === 0" class="empty-state">
      <el-empty v-if="!isLoadingPage" :image-size="100">
        <template #description>
          <p>{{ $t('clipboard.noRecords') }}</p>
          <p class="hint">{{ $t('clipboard.emptyHint') }}</p>
        </template>
      </el-empty>
      <div v-else class="loading-state">
        <el-icon class="is-loading" :size="40"><Loading /></el-icon>
        <p>{{ $t('common.loading') }}</p>
      </div>
    </div>

    <ClipboardList
        v-else
        ref="clipboardListRef"
        class="history-list"
        :highlight-keyword="searchKeyword"
        :delete-item="originalDeleteItem"
        :get-item-category="getItemCategory"
        :handle-drag-end="handleDragEnd"
        :handle-drag-start="handleDragStart"
        :select-and-fill-direct="selectAndFillDirect"
        :selected-item-id="selectedItemId"
        :show-context-menu="showContextMenu"
        :is-ctrl-key-pressed="isCtrlKeyPressed"
        :is-pinned="isItemPinned"
        :promote-item="promoteItem"
        :update-selection="updateSelection"
        :has-more="hasMore"
        :is-loading-page="isLoadingPage"
        :total-count="totalCount"
        @load-more-intent="handleLoadMoreIntent"
        @preview="handlePreview"
        :visible-history="visibleHistory"
    />

    <div class="status-footer" @click.stop @mousedown.stop>
      <div class="status-text">
        <span class="status-label">{{ selectedStatusText }}</span>
        <span class="status-meta">{{ loadStatusText }}</span>
        <span v-if="searchKeyword.trim()" class="status-meta">{{
            $t('clipboard.hitCount', {count: keywordHitCount})
          }}</span>
        <div class="status-actions">
          <button
              :title="$t('clipboard.cyclePageSize', {size: pageSize})"
              class="nav-action-btn"
              type="button"
              @click="cyclePageSize"
          >
            {{ $t('clipboard.perPage', {size: pageSize}) }}
          </button>

          <button :aria-label="$t('clipboard.backToStart')" :title="$t('clipboard.backToStart')"
                  class="nav-action-btn icon-btn" type="button"
                  @click="scrollToStart">
            <el-icon>
              <ArrowLeftBold/>
            </el-icon>
          </button>
          <button :aria-label="$t('clipboard.scrollToEnd')" :title="$t('clipboard.scrollToEnd')"
                  class="nav-action-btn icon-btn" type="button"
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
        ref="contextMenuRef"
        class="context-menu"
        @click.stop
        @mousedown.stop
    >
      <div class="context-menu-header">{{ $t('clipboard.aiShortcut') }}</div>
      <div class="context-menu-item" @click="triggerAiFromContextMenu('translate')">
        {{ $t('selectionToolbar.translate') }}
        <span class="shortcut-hint">T</span>
      </div>
      <div class="context-menu-item" @click="triggerAiFromContextMenu('explain')">
        {{ $t('selectionToolbar.explain') }}
        <span class="shortcut-hint">E</span>
      </div>
      <div class="context-menu-divider"></div>
      <div class="context-menu-header">{{ $t('clipboard.addToCategory') }}</div>
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
import {computed, nextTick, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {useI18n} from 'vue-i18n'
import {ArrowLeftBold, ArrowRightBold, Check, Loading} from '@element-plus/icons-vue'
import {ElMessage} from 'element-plus'
import {listen} from '@tauri-apps/api/event'
import {AIService, ClipboardService, ImageClipboardService, WindowService} from '../../services/ipc'
import {handleAppError} from '../../utils/errorHandler'
import ClipboardToolbar from './components/ClipboardToolbar.vue'
import ClipboardList from './components/ClipboardList.vue'
import {useClipboardHistory} from './composables/useClipboardHistory'
import {useCategoryManager} from './composables/useCategoryManager'
import {useWindowOffset} from './composables/useWindowOffset'
import {useContextMenuState} from '../shared/useContextMenuState'
import {runCategoryAssignment} from '../shared/categoryActions'

const {t} = useI18n()

const containerRef = ref(null)
const clipboardListRef = ref(null)
const contextMenuRef = ref(null)
const isVisible = ref(false)
const isCtrlKeyPressed = ref(false)
const categories = ref(['未分类'])
const pinnedItems = ref([])

const {
  contextMenuVisible,
  contextMenuX,
  contextMenuY,
  contextMenuItem,
  openContextMenu,
  closeContextMenu
} = useContextMenuState(null, {menuWidth: 160, maxHeightPx: 300, maxHeightRatio: 0.6})

const syncContextMenuPosition = () => {
  const el = contextMenuRef.value
  if (!el) return
  el.style.top = `${contextMenuY.value}px`
  el.style.left = `${contextMenuX.value}px`
}
const dragItem = ref(null)
const aiActionLoading = ref(false)
const isAiSettingsCollapsed = ref(true)
const translationTargetLanguage = ref(localStorage.getItem('clipboard_ai_target_language') || '简体中文')
const explanationTargetLanguage = ref(localStorage.getItem('clipboard_ai_explain_language') || '中文')
  const loadMoreIntent = ref(false)

const handlePreview = async (content, id) => {
  try {
    await ImageClipboardService.openTextPreviewWindow(content, id)
  } catch (error) {
    console.error('打开文本预览失败:', error)
  }
  }

  const isUpdatingCategory = ref(false)
let unlistenShowWindow = null
let unlistenHistoryPayloadUpdated = null
let unlistenHistoryItemUpdated = null
let unlistenTextItemPromoted = null
let unlistenTextItemReplaced = null
let unlistenWritebackResult = null
let writebackErrorMsg = null
let windowBlurHandler = null
let isPageReloading = false
let beforeUnloadHandler = null
let pageHideHandler = null

const {
  selectedItemId,
  searchKeyword,
  categoryFilter,
  categoryMap,
  visibleHistory,
  pageSize,
  totalCount,
  hasMore,
  isLoadingPage,
  getItemCategory,
  updateSelection,
  deleteItem: originalDeleteItem,
  moveSelection,
  resetAndReloadHistory,
  syncHistoryIncremental,
  loadMoreHistory,
  loadTailPage,
  setPageSize,
  promoteLocalById,
  replaceLocalById,
  setLocalPinnedById,
  insertLocalIncomingContent,
  applyPayloadSnapshot,
  bumpFilterDataRevision,
  setItemCategoryLocal,
  removeItemCategoryLocal,
  rebuildCategorySearchIndex
} = useClipboardHistory(pinnedItems)

const {
  isAddingCategory,
  newCategoryName,
  newCategoryInputRef,
  setItemCategory,
  removeCategory,
  canDeleteCategory,
  startCreateCategory,
  confirmCreateCategory,
  cancelCreateCategory
} = useCategoryManager(categories, categoryMap, categoryFilter, {
  bumpFilterDataRevision,
  setIsUpdatingCategory: (value) => {
    isUpdatingCategory.value = value
  },
  setItemCategoryLocal,
  removeItemCategoryLocal
})

const {
  bottomOffset,
  clampBottomOffset,
  startWindowOffsetDrag
} = useWindowOffset()

const isItemPinned = (id) => pinnedItems.value.includes(id)

const promoteItem = async (id) => {
  const shouldPin = !isItemPinned(id)
  if (shouldPin) {
    pinnedItems.value = [id, ...pinnedItems.value.filter((p) => p !== id)]
  } else {
    pinnedItems.value = pinnedItems.value.filter((p) => p !== id)
  }
  setLocalPinnedById(id, shouldPin)
  try {
    await ClipboardService.setItemPinned(id, shouldPin)
  } catch (error) {
    if (shouldPin) {
      pinnedItems.value = pinnedItems.value.filter((p) => p !== id)
    } else {
      pinnedItems.value = [id, ...pinnedItems.value.filter((p) => p !== id)]
    }
    setLocalPinnedById(id, !shouldPin)
    console.error('置顶失败:', error)
    handleAppError(error, t('clipboard.pinFailed'))
  }
}

const toggleAiSettings = () => {
  isAiSettingsCollapsed.value = !isAiSettingsCollapsed.value
}

const hideClipboardWindow = () => {
  isVisible.value = false
  isAiSettingsCollapsed.value = true
}

const selectedStatusText = computed(() => {
  const total = totalCount.value || visibleHistory.value.length
  if (total === 0) return t('clipboard.noSelection')
  const current = visibleHistory.value.findIndex((entry) => entry.id === selectedItemId.value)
  const display = current >= 0 ? current + 1 : 1
  return t('clipboard.selected', {display, total})
})

const loadStatusText = computed(() => {
  if (isLoadingPage.value) return t('common.loading')
  if (hasMore.value) return t('clipboard.loaded', {
    loaded: visibleHistory.value.length,
    total: totalCount.value || visibleHistory.value.length
  })
  return t('clipboard.allLoaded', {total: visibleHistory.value.length})
})

const keywordHitCount = computed(() => {
  const tokens = searchKeyword.value
      .trim()
      .toLowerCase()
      .split(/\s+/)
      .map((t) => t.trim())
      .filter(Boolean)
  if (tokens.length === 0) return 0
  const tokenCount = tokens.length
  return visibleHistory.value.filter((entry) => {
    const text = `${entry.content || ''}\n${entry.snippet || ''}`.toLowerCase()
    for (let i = 0; i < tokenCount; i++) {
      if (text.includes(tokens[i])) return true
    }
    return false
  }).length
})

const PAGE_SIZE_OPTIONS = [10, 30, 50]
const normalizePageSize = (value) => {
  const parsed = Number(value)
  return PAGE_SIZE_OPTIONS.includes(parsed) ? parsed : 50
}

const cyclePageSize = async () => {
  const current = normalizePageSize(pageSize.value)
  const currentIndex = PAGE_SIZE_OPTIONS.indexOf(current)
  const next = PAGE_SIZE_OPTIONS[(currentIndex + 1) % PAGE_SIZE_OPTIONS.length]
  await setPageSize(next)
  localStorage.setItem('clipboard_history_page_size', String(next))
}

const syncWindowPayload = (payload = {}) => {
  if (payload.categories) {
    categoryMap.value = payload.categories
  }
  if (Array.isArray(payload.pinned_items)) {
    pinnedItems.value = payload.pinned_items
  }
  if (Array.isArray(payload.category_list)) {
    const list = payload.category_list.filter(c => c !== '未分类' && c !== '全部')
    categories.value = ['未分类', ...Array.from(new Set(list))]
  } else if (payload.categories) {
    const extractedCategories = Object.values(payload.categories)
    const uniqueList = Array.from(new Set(extractedCategories)).filter(c => c !== '未分类' && c !== '全部')
    categories.value = ['未分类', ...uniqueList]
  }
  applyPayloadSnapshot(payload)
}

const init = async () => {
  try {
    unlistenShowWindow = await listen('show-window', (event) => {
      void showWindow(event.payload)
    })
    unlistenHistoryPayloadUpdated = await listen('clipboard-history-payload-updated', (event) => {


      syncHistoryIncremental()
    })
    unlistenHistoryItemUpdated = await listen('clipboard-history-item-updated', (event) => {
      const payload = event?.payload || {}
      const latestItem = typeof payload.latest_item === 'string' ? payload.latest_item : ''
      const latestItemId = typeof payload.latest_item_id === 'string' ? payload.latest_item_id : ''
      if (!latestItem || !latestItemId) {
        return
      }
      insertLocalIncomingContent(latestItem, latestItemId, Boolean(payload.is_pinned))
    })
    unlistenTextItemPromoted = await listen('text-item-promoted', (event) => {
      const id = event?.payload?.id
      promoteLocalById(id)
    })
    unlistenTextItemReplaced = await listen('text-item-replaced', (event) => {
      const payload = event?.payload || {}
      if (payload.old_id && payload.new_id) {
        replaceLocalById(payload.old_id, payload.new_id, payload.new_content)
      }
    })
    unlistenWritebackResult = await listen('writeback-result', (event) => {
      const payload = event.payload || {}
      if (payload.source !== '文本') return

      if (writebackErrorMsg) {
        writebackErrorMsg.close()
        writebackErrorMsg = null
      }

      if (!payload.success) {
        writebackErrorMsg = ElMessage.error({
          message: t('clipboard.pasteFailed', {error: payload.detail || t('common.unknownError')}),
          duration: 0,
          showClose: true
        })
      }
    })

    windowBlurHandler = async () => {
      isCtrlKeyPressed.value = false
      if (isPageReloading) {
        return
      }
      try {
        await WindowService.blur()
        hideClipboardWindow()
      } catch (error) {
        console.error('调用 window_blur 失败:', error)
      }
    }
    window.addEventListener('blur', windowBlurHandler)
    beforeUnloadHandler = () => {
      isPageReloading = true
    }
    pageHideHandler = () => {
      isPageReloading = true
    }
    window.addEventListener('beforeunload', beforeUnloadHandler)
    window.addEventListener('pagehide', pageHideHandler)
    window.addEventListener('keydown', handleKeydown)
  window.addEventListener('keyup', handleKeyup)

    isVisible.value = true
    let payload = null
    try {
      payload = await ClipboardService.getHistory()
      syncWindowPayload(payload)
    } catch (error) {
      console.error('初始化拉取历史失败:', error)
    }
  } catch (error) {
    console.error('初始化失败:', error)
  }
}

const showWindow = async (data) => {
  if (typeof data.bottomOffset === 'number') {
    bottomOffset.value = clampBottomOffset(data.bottomOffset)
  }
  syncWindowPayload(data)

  const resolvedSelectedId = (() => {
    if (typeof data.selectedItemId === 'string' && data.selectedItemId) {
      return data.selectedItemId
    }
    if (typeof data.selectedIndex === 'number') {
      const entry = visibleHistory.value.find((e) => e.index === data.selectedIndex)
      if (entry?.id) return entry.id
    }
    return visibleHistory.value.length > 0 ? visibleHistory.value[0].id : ''
  })()
  selectedItemId.value = resolvedSelectedId
  isVisible.value = true
  loadMoreIntent.value = false

  if (visibleHistory.value.length > 0) {
    if (!visibleHistory.value.some((entry) => entry.id === selectedItemId.value)) {
      selectedItemId.value = visibleHistory.value[0].id
    }
    const contentRef = clipboardListRef.value?.contentRef
    updateSelection(selectedItemId.value, true, contentRef)
  }

  nextTick(() => {
    containerRef.value?.focus()
  })
}

const selectAndFillDirect = async (itemId) => {
  if (writebackErrorMsg) {
    writebackErrorMsg.close()
    writebackErrorMsg = null
  }
  try {
    await ClipboardService.selectAndFill(itemId, null)
    hideClipboardWindow()
  } catch (error) {
    console.error('填充内容失败:', error)
    writebackErrorMsg = ElMessage.error({
      message: t('clipboard.fillFailed', {error: String(error)}),
      duration: 0,
      showClose: true
    })
  }
}

const showContextMenu = (event, itemId, index) => {
  openContextMenu(event, itemId)
}

const closeFloatingPanels = () => {
  closeContextMenu()
  isAiSettingsCollapsed.value = true
}

const handleContainerMouseDown = (event) => {
  if (event.button !== 0) return
  const target = event.target
  if (isAddingCategory.value && target instanceof Element && !target.closest('.category-input')) {
    cancelCreateCategory()
  }
  if (target instanceof Element && target.closest('.clipboard-ai-select-popper')) {
    return
  }
  closeFloatingPanels()
}

const assignToCategory = async (category) => {
  const itemKey = contextMenuItem.value
  await runCategoryAssignment({
    itemKey,
    category,
    persist: (itemKey, nextCategory) => setItemCategory(itemKey, nextCategory),
    onFinally: closeContextMenu
  })
}

const handleDragStart = (event, id) => {
  if (!isCtrlKeyPressed.value) {
    event.preventDefault()
    return
  }
  dragItem.value = id
  event.dataTransfer.effectAllowed = 'copy'
  event.dataTransfer.setData('text/plain', id)
}

const handleDragEnd = () => {
  dragItem.value = null
}

const handleDrop = async (event, category) => {
  event.preventDefault()

  const target = event.currentTarget
  if (target && target.classList.contains('category-pill')) {
    target.classList.remove('drag-over')
  }

  await runCategoryAssignment({
    itemKey: dragItem.value,
    category,
    persist: (itemKey, nextCategory) => setItemCategory(itemKey, nextCategory)
  })
}

const buildOpId = () => Date.now() * 10000 + Math.floor(Math.random() * 10000)

const resolveSelectedText = () => {
  if (!selectedItemId.value) {
    return ''
  }
  const entry = visibleHistory.value.find((e) => e.id === selectedItemId.value)
  return entry ? entry.content : ''
}

const triggerAiFlow = async (rawText, mode) => {
    let text = typeof rawText === 'string' ? rawText.trim() : ''
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
          t('clipboard.autoDetect'),
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
    handleAppError(error, mode === 'translate' ? t('clipboard.translateFailed') : t('clipboard.explainFailed'))
  } finally {
    aiActionLoading.value = false
  }
}

const triggerAiFromSelection = async (mode) => {
  const text = resolveSelectedText()
  await triggerAiFlow(text, mode)
}

const triggerAiFromContextMenu = async (mode) => {
  const id = contextMenuItem.value
  const entry = visibleHistory.value.find((e) => e.id === id)
  const text = entry ? entry.content : ''
  closeContextMenu()
  await triggerAiFlow(text, mode)
}

const isInputLikeTarget = (target) => {
  const tagName = target?.tagName?.toLowerCase?.()
  return tagName === 'input' || tagName === 'textarea' || target?.isContentEditable
}

const ensureKeyboardSelectionVisible = async () => {
  await nextTick()
  const selected = selectedItemId.value
  if (!selected) return
  const element = document.getElementById(`clipboard-item-${selected}`)
  const container = clipboardListRef.value?.getScrollEl() || document.getElementById(`clipboard-item-${selected}`)?.closest('.content')
  if (!element || !container) return
  const EDGE_PADDING = 8
  const maxScrollLeft = Math.max(0, container.scrollWidth - container.clientWidth)
  const targetLeft = Math.max(0, element.offsetLeft - EDGE_PADDING)
  container.scrollLeft = Math.min(maxScrollLeft, targetLeft)
}

const getContentContainer = () => {
  return clipboardListRef.value?.getScrollEl() || null
}

const tryLoadMoreByScroll = async () => {
  if (!hasMore.value || isLoadingPage.value) return
  const container = getContentContainer()
  if (!container) return
  const remaining = container.scrollWidth - container.clientWidth - container.scrollLeft
  if (remaining <= 260 && loadMoreIntent.value) {
    loadMoreIntent.value = false
    await loadMoreHistory()
  }
}

const handleLoadMoreIntent = () => {
  if (!hasMore.value || isLoadingPage.value) return
  loadMoreIntent.value = true
  void tryLoadMoreByScroll()
}

const scrollToStart = async () => {
  clipboardListRef.value?.jumpToStart()
}

const scrollToEnd = async () => {
  if (hasMore.value) {
    try {
      await loadTailPage()
      await nextTick()
    } catch (error) {
      console.error('加载文字尾页失败:', error)
      await syncHistoryIncremental()
      await nextTick()
    }
  }
  clipboardListRef.value?.jumpToEnd()
}

const handleKeyup = (event) => {
  if (!event.ctrlKey) {
    isCtrlKeyPressed.value = false
  }
}

const handleKeydown = async (event) => {
  if (event.ctrlKey) {
    isCtrlKeyPressed.value = true
  }

  if (!isVisible.value) return
  if (isInputLikeTarget(event.target)) return

  // Ctrl+Number: Quick select and fill
  if (event.ctrlKey && event.key >= '1' && event.key <= '9') {
    event.preventDefault()
    const index = parseInt(event.key, 10) - 1
    if (index >= 0 && index < visibleHistory.value.length) {
      selectAndFillDirect(visibleHistory.value[index].id)
    }
    return
  }

  // Escape: Close context menu if open
  if (contextMenuVisible.value && event.key === 'Escape') {
    closeContextMenu()
    return
  }

  // Handle navigation and action keys
  switch (event.key) {
    case 'Home':
      event.preventDefault()
      clipboardListRef.value?.jumpToStart()
      break

    case 'ArrowLeft':
      event.preventDefault()
      moveSelection(-1, clipboardListRef.value?.contentRef)
      await ensureKeyboardSelectionVisible()
      break

    case 'ArrowRight':
      event.preventDefault()
      moveSelection(1, clipboardListRef.value?.contentRef)
      loadMoreIntent.value = true
      await tryLoadMoreByScroll()
      await ensureKeyboardSelectionVisible()
      break

    case 'Enter':
      event.preventDefault()
      if (selectedItemId.value) {
        const entry = visibleHistory.value.find(e => e.id === selectedItemId.value)
        if (entry) {
          selectAndFillDirect(entry.id)
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

// 监听搜索和分类过滤变化，使用增量同步而不是全量重置
// 这样可以保留前端已有的数据（包括刚分类的项）
let filterDebounceTimer = null

watch([searchKeyword, categoryFilter], (newVals, oldVals) => {
  if (!isVisible.value) return

  // Skip sync during category updates to avoid race conditions
  if (isUpdatingCategory.value) return

  const [newSearch, newCategory] = newVals
  const [oldSearch, oldCategory] = oldVals

  // Clear existing timer
  if (filterDebounceTimer) {
    clearTimeout(filterDebounceTimer)
  }

  // Use longer delay for search changes to reduce API calls
  const delay = newSearch !== oldSearch ? 300 : 50

  filterDebounceTimer = setTimeout(() => {
    loadMoreIntent.value = false
    syncHistoryIncremental()
  }, delay)
})

watch(visibleHistory, (list) => {
  if (!Array.isArray(list) || list.length === 0) {
    selectedItemId.value = ''
    return
  }
  // 仅当选中的项不在当前列表中时才重置选择（删除、过滤或首次加载导致），
  // 而非列表顺序变化时也重置，避免覆盖用户操作
  const exists = list.some((entry) => entry.id === selectedItemId.value)
  if (!exists) {
    selectedItemId.value = list[0].id
  }
})

// Auto-jump to first item when category filter changes
watch(categoryFilter, (newVal, oldVal) => {
  if (!isVisible.value || newVal === oldVal) return
  nextTick(() => {
    if (visibleHistory.value.length > 0) {
      clipboardListRef.value?.jumpToStart()
    }
  })
})

watch([contextMenuVisible, contextMenuX, contextMenuY], async ([visible]) => {
  if (!visible) return
  await nextTick()
  syncContextMenuPosition()
})

onMounted(() => {
  const savedPageSize = localStorage.getItem('clipboard_history_page_size')
  pageSize.value = normalizePageSize(savedPageSize)
  init()
})

onBeforeUnmount(() => {
  if (unlistenShowWindow) {
    unlistenShowWindow()
    unlistenShowWindow = null
  }
  if (unlistenHistoryPayloadUpdated) {
    unlistenHistoryPayloadUpdated()
    unlistenHistoryPayloadUpdated = null
  }
  if (unlistenHistoryItemUpdated) {
    unlistenHistoryItemUpdated()
    unlistenHistoryItemUpdated = null
  }
  if (unlistenTextItemPromoted) {
    unlistenTextItemPromoted()
    unlistenTextItemPromoted = null
  }
  if (unlistenTextItemReplaced) {
    unlistenTextItemReplaced()
    unlistenTextItemReplaced = null
  }
  if (unlistenWritebackResult) {
    unlistenWritebackResult()
    unlistenWritebackResult = null
  }
  if (windowBlurHandler) {
    window.removeEventListener('blur', windowBlurHandler)
    windowBlurHandler = null
  }
  if (beforeUnloadHandler) {
    window.removeEventListener('beforeunload', beforeUnloadHandler)
    beforeUnloadHandler = null
  }
  if (pageHideHandler) {
    window.removeEventListener('pagehide', pageHideHandler)
    pageHideHandler = null
  }
  window.removeEventListener('keydown', handleKeydown)
  window.removeEventListener('keyup', handleKeyup)
  if (filterDebounceTimer) {
    clearTimeout(filterDebounceTimer)
    filterDebounceTimer = null
  }
})
</script>

<style>
@import "../shared/contextMenu.css";

.clipboard-ai-select-popper {
  border: 1px solid var(--fy-border) !important;
  border-radius: var(--fy-radius-lg) !important;
  background: var(--fy-glass-bg) !important;
  backdrop-filter: var(--fy-glass-blur-light);
  -webkit-backdrop-filter: var(--fy-glass-blur-light);
  box-shadow: var(--fy-glass-shadow) !important;
}

.clipboard-ai-select-popper .el-select-dropdown__item {
  color: var(--fy-text-primary) !important;
}

.clipboard-ai-select-popper .el-select-dropdown__item.hover,
.clipboard-ai-select-popper .el-select-dropdown__item:hover {
  background: var(--fy-accent-bg-hover) !important;
  color: var(--fy-text-primary) !important;
}

.clipboard-ai-select-popper .el-select-dropdown__item.selected,
.clipboard-ai-select-popper .el-select-dropdown__item.is-selected {
  color: var(--fy-text-accent) !important;
  font-weight: 700;
  background: var(--fy-accent-bg) !important;
}

</style>

<style scoped>
.container {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--fy-container-bg);
  backdrop-filter: var(--fy-backdrop-blur);
  -webkit-backdrop-filter: var(--fy-backdrop-blur);
  border: 0.5px solid var(--fy-container-border);
  box-shadow: var(--fy-shadow-lg);
  overflow: hidden;
  outline: none;
  transition: background 0.3s ease, border-color 0.3s ease;
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
  color: var(--fy-text-primary);
}

.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--fy-space-3);
  color: var(--fy-text-accent);
  font-size: var(--fy-text-md);
}

.ai-quick-panel-wrap {
  position: relative;
  height: 0;
  margin: 0 var(--fy-space-2);
  z-index: 50;
}

.ai-quick-panel {
  position: absolute;
  top: 4px;
  left: 0;
  width: min(560px, calc(100vw - 36px));
  padding: var(--fy-space-3);
  border-radius: var(--fy-radius-lg);
  background: var(--fy-glass-bg);
  border: 0.5px solid var(--fy-glass-border);
  box-shadow: var(--fy-glass-shadow);
  backdrop-filter: var(--fy-backdrop-blur-light);
  -webkit-backdrop-filter: var(--fy-backdrop-blur-light);
}

.history-list {
  flex: 1;
  min-height: 0;
}

.status-footer {
  flex: 0 0 auto;
  min-height: 32px;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 8px;
  padding: 0 14px;
  position: sticky;
  bottom: 0;
  left: 0;
  right: 0;
  z-index: 120;
  border-top: 0.5px solid var(--fy-border-light);
}

.status-text {
  flex: 1 1 0;
  min-width: 0;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--fy-text-muted);
  font-family: var(--fy-font-mono);
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

.status-meta {
  flex: 0 1 auto;
  min-width: 0;
  color: var(--fy-accent);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-weight: var(--fy-weight-medium);
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
  border: 1px solid var(--fy-border);
  background: var(--fy-glass-bg);
  color: var(--fy-text-secondary);
  border-radius: var(--fy-radius-sm);
  font-size: var(--fy-text-xs);
  line-height: 1;
  font-weight: var(--fy-weight-semibold);
  padding: var(--fy-space-2) var(--fy-space-3);
  min-height: 30px;
  cursor: pointer;
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
  box-shadow: none;
  backdrop-filter: var(--fy-glass-blur-light);
  -webkit-backdrop-filter: var(--fy-glass-blur-light);
}

.icon-btn {
  flex: 0 0 auto;
  width: 32px;
  height: 30px;
  min-width: 32px;
  padding: 0;
  border-radius: var(--fy-radius-sm);
  font-size: 14px;
  line-height: 1;
  justify-content: center;
  display: inline-flex;
  align-items: center;
  font-weight: var(--fy-weight-bold);
}

.nav-action-btn:hover {
  border-color: var(--fy-accent);
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

.nav-action-btn:focus-visible {
  outline: 2px solid var(--fy-accent);
  outline-offset: 2px;
}

.ai-quick-top {
  display: grid;
  grid-template-columns: max-content max-content minmax(0, 1fr);
  align-items: center;
  gap: var(--fy-space-3);
  width: 100%;
  min-width: 0;
}

.ai-control-item {
  display: flex;
  align-items: center;
  gap: var(--fy-space-2);
  color: var(--fy-text-secondary);
  font-size: var(--fy-text-sm);
  min-width: 0;
}

.ai-select-item {
  padding: var(--fy-space-1) var(--fy-space-2);
  border-radius: var(--fy-radius-md);
  background: transparent;
  border: 0.5px solid var(--fy-border-light);
  flex: 0 0 auto;
}

.ai-control-label {
  color: var(--fy-text-secondary);
  white-space: nowrap;
  font-weight: var(--fy-weight-semibold);
  letter-spacing: var(--fy-tracking-wide);
}

.ai-shortcut-tip {
  margin-top: 0;
  justify-self: end;
  white-space: nowrap;
  font-size: var(--fy-text-xs);
  color: var(--fy-text-muted);
}

:deep(.ai-select) {
  width: 112px;
}

:deep(.ai-select .el-select__wrapper) {
  background: var(--fy-bg-input);
  border: 1px solid var(--fy-border);
  border-radius: var(--fy-radius-md);
  box-shadow: none;
}

:deep(.ai-select .el-select__wrapper:hover),
:deep(.ai-select .el-select__wrapper.is-focused) {
  background: var(--fy-bg-hover);
  border-color: var(--fy-border-hover);
}

:deep(.ai-select .el-select__selected-item) {
  color: var(--fy-text-primary);
  font-size: var(--fy-text-sm);
}

:deep(.ai-select .el-select__placeholder) {
  color: var(--fy-text-muted);
}

.hint {
  color: var(--fy-text-muted);
  font-size: var(--fy-text-sm);
}

</style>
