<template>
  <div
    ref="containerRef"
    class="container"
    tabindex="-1"
    @click="closeContextMenu"
    @keydown="handleKeydown"
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
      :create-category-text="$t('imageClipboard.createCategory')"
      :search-placeholder="$t('imageClipboard.search')"
      :start-create-category="startCreateCategory"
      :start-window-offset-drag="startWindowOffsetDrag"
      :show-ai-toggle="false"
      :is-reorder-mode="isReorderMode"
      @toggle-reorder="toggleReorderMode"
    />
    <div
      v-if="isLoadingPage && filteredHistory.length === 0"
      class="loading-state"
    >
      <el-icon
        :size="24"
        class="is-loading"
      >
        <Loading />
      </el-icon>
      <span>{{ $t('clipboard.loading') }}</span>
    </div>
    <div
      v-else-if="filteredHistory.length === 0"
      class="empty-state"
    >
      <el-empty :image-size="100">
        <template #description>
          <p>{{ $t('imageClipboard.noRecords') }}</p>
          <p class="hint">
            {{ $t('imageClipboard.emptyHint') }}
          </p>
        </template>
      </el-empty>
    </div>

    <ImageClipboardList
      v-else
      ref="imageListRef"
      :delete-item="deleteItem"
      :download-item="downloadItem"
      :fill-by-id="fillById"
      :get-preview-data-url="getPreviewDataUrl"
      :handle-drag-end="handleDragEnd"
      :handle-drag-start="handleDragStart"
      :has-more="hasMore"
      :is-ctrl-key-pressed="isCtrlKeyPressed"
      :is-loading-page="isLoadingPage"
      :open-fullscreen="openFullscreen"
      :promote-item="promoteImageItem"
      :select-by-index="selectByIndex"
      :selected-index="selectedDisplayIndex"
      :show-context-menu="showContextMenu"
      :visible-history="filteredHistory"
      :total-count="totalCount"
      :is-reorder-mode="isReorderMode"
      :on-reorder="handleReorder"
      @load-more-intent="handleLoadMoreIntent"
    />

    <div
      class="status-footer"
      @click.stop
      @mousedown.stop
    >
      <div class="status-text">
        <span class="status-label">{{ selectedStatusText }}</span>
        <span class="status-meta">{{ loadStatusText }}</span>
        <div class="status-actions">
          <button
            :title="$t('imageClipboard.cyclePageSize', {size: pageSize})"
            class="nav-action-btn"
            type="button"
            @click="cyclePageSize"
          >
            {{ $t('imageClipboard.perPage', {size: pageSize}) }}
          </button>
          <button
            :aria-label="$t('imageClipboard.backToStart')"
            :title="$t('imageClipboard.backToStart')"
            class="nav-action-btn icon-btn"
            type="button"
            @click="scrollToStart"
          >
            <el-icon>
              <ArrowLeftBold />
            </el-icon>
          </button>
          <button
            :aria-label="$t('imageClipboard.scrollToEnd')"
            :title="$t('imageClipboard.scrollToEnd')"
            class="nav-action-btn icon-btn"
            type="button"
            @click="scrollToEnd"
          >
            <el-icon>
              <ArrowRightBold />
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
    >
      <div
        class="context-menu-item"
        @click="editItemTags"
      >
        {{ $t('imageClipboard.editTags') }}
      </div>
      <div class="context-menu-divider" />
      <div class="context-menu-header">
        {{ $t('imageClipboard.addToCategory') }}
      </div>
      <div
        v-for="category in categories"
        :key="category"
        class="context-menu-item"
        @click="assignToCategory(category)"
      >
        {{ category }}
        <el-icon
          v-if="getItemCategory(contextMenuItemId) === category"
          class="check-icon"
        >
          <Check />
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
import {open as openDialog} from '@tauri-apps/plugin-dialog'
import {ImageClipboardService, WindowService} from '../../services/ipc'
import ClipboardToolbar from '../clipboard/components/ClipboardToolbar.vue'
import ImageClipboardList from './components/ImageClipboardList.vue'
import {useWindowOffset} from '../clipboard/composables/useWindowOffset'
import {useContextMenuState} from '../shared/useContextMenuState'
import {useImageHistory} from './composables/useImageHistory'
import {useImageCategories} from './composables/useImageCategories'
import {useImagePreview} from './composables/useImagePreview'

const {t} = useI18n()

const containerRef = ref(null)
const imageListRef = ref(null)
const contentRef = computed(() => imageListRef.value?.getScrollEl?.() || imageListRef.value?.contentRef?.value || imageListRef.value?.contentRef || null)
const contextMenuRef = ref(null)
const categoryMap = ref({})
const tagMap = ref({})
const categories = ref(['未分类'])
const isAddingCategory = ref(false)
const {
  contextMenuVisible,
  contextMenuX,
  contextMenuY,
  contextMenuItem: contextMenuItemId,
  openContextMenu,
  closeContextMenu
} = useContextMenuState('', {menuWidth: 160, maxHeightPx: 300, maxHeightRatio: 0.6})

const syncContextMenuPosition = () => {
  const el = contextMenuRef.value
  if (!el) return
  el.style.top = `${contextMenuY.value}px`
  el.style.left = `${contextMenuX.value}px`
}

const isFilling = ref(false)
const warmedIndices = new Set()
const warmingIndices = new Set()
let unlistenShowWindow = null
let unlistenHistoryPayloadUpdated = null
let unlistenHistoryItemAdded = null
let unlistenWritebackResult = null
let unlistenImageQueueFull = null
let unlistenImagePersistDrop = null
let writebackErrorMsg = null
const isCtrlKeyPressed = ref(false)

const IMAGE_ITEM_UNIT = 76 // 与 ImageClipboardList 的 CARD_STEP 保持一致
const IMAGE_PREVIEW_CACHE_MARGIN = 24
const IMAGE_PREVIEW_CACHE_MAX_ITEMS = 300
const ASYNC_PREVIEW_CACHE_MAX_ITEMS = 180
const FILTER_CACHE_MAX_SIZE = 500
const filterDataRevision = ref(0)

const createBoundedMap = (maxSize) => {
  const map = new Map()
  const originalSet = map.set.bind(map)
  const originalGet = map.get.bind(map)
  map.set = (key, value) => {
    if (map.size >= maxSize && !map.has(key)) {
      const firstKey = map.keys().next().value
      map.delete(firstKey)
    }
    return originalSet(key, value)
  }
  map.get = (key) => {
    if (!map.has(key)) return undefined
    const value = originalGet(key)
    // 刷新 key 顺序，实现 LRU 淘汰
    map.delete(key)
    originalSet(key, value)
    return value
  }
  return map
}

const previewCache = createBoundedMap(IMAGE_PREVIEW_CACHE_MAX_ITEMS)
const asyncPreviewCache = createBoundedMap(ASYNC_PREVIEW_CACHE_MAX_ITEMS)
const filterEntriesCache = createBoundedMap(FILTER_CACHE_MAX_SIZE)
const keywordTagMatchCache = createBoundedMap(FILTER_CACHE_MAX_SIZE)
const keywordCategoryMatchCache = createBoundedMap(FILTER_CACHE_MAX_SIZE)
const tagSearchIndex = new Map()
const itemTagSnapshot = new Map()
const categorySearchIndex = new Map()
const itemCategorySnapshot = new Map()
const clearFilterCaches = () => {
  filterEntriesCache.clear()
  keywordTagMatchCache.clear()
  keywordCategoryMatchCache.clear()
}

const bumpFilterDataRevision = () => {
  filterDataRevision.value += 1
  clearFilterCaches()
}

const getLoadedHistoryCount = () => {
  let count = 0
  for (const item of history.value) {
    if (item) count += 1
  }
  return count
}

const normalizeTagList = (tags) =>
    (Array.isArray(tags) ? tags : [])
        .map((tag) => String(tag ?? '').trim())
        .filter((tag) => tag.length > 0)

const removeTagIndexForItem = (itemId) => {
  const oldTags = itemTagSnapshot.get(itemId)
  if (!oldTags || oldTags.length === 0) {
    itemTagSnapshot.delete(itemId)
    return
  }
  for (const tag of oldTags) {
    const idSet = tagSearchIndex.get(tag)
    if (!idSet) continue
    idSet.delete(itemId)
    if (idSet.size === 0) {
      tagSearchIndex.delete(tag)
    }
  }
  itemTagSnapshot.delete(itemId)
}

const applyTagIndexForItem = (itemId, tags) => {
  removeTagIndexForItem(itemId)
  const normalized = normalizeTagList(tags).map((tag) => tag.toLowerCase())
  if (normalized.length === 0) {
    return
  }
  itemTagSnapshot.set(itemId, normalized)
  for (const tag of normalized) {
    let idSet = tagSearchIndex.get(tag)
    if (!idSet) {
      idSet = new Set()
      tagSearchIndex.set(tag, idSet)
    }
    idSet.add(itemId)
  }
}

const setItemTagsLocal = (itemId, tags) => {
  if (!itemId) return
  const normalized = normalizeTagList(tags)
  tagMap.value[itemId] = normalized
  applyTagIndexForItem(itemId, normalized)
  keywordTagMatchCache.clear()
}

const removeItemTagsLocal = (itemId) => {
  if (!itemId) return
  delete tagMap.value[itemId]
  removeTagIndexForItem(itemId)
  keywordTagMatchCache.clear()
}

const rebuildTagSearchIndex = () => {
  tagSearchIndex.clear()
  itemTagSnapshot.clear()
  keywordTagMatchCache.clear()
  const currentTagMap = tagMap.value || {}
  for (const itemId of Object.keys(currentTagMap)) {
    applyTagIndexForItem(itemId, currentTagMap[itemId])
  }
}

const removeCategoryIndexForItem = (itemId) => {
  const oldCategory = itemCategorySnapshot.get(itemId)
  if (!oldCategory) {
    itemCategorySnapshot.delete(itemId)
    return
  }
  const idSet = categorySearchIndex.get(oldCategory)
  if (idSet) {
    idSet.delete(itemId)
    if (idSet.size === 0) {
      categorySearchIndex.delete(oldCategory)
    }
  }
  itemCategorySnapshot.delete(itemId)
}

const applyCategoryIndexForItem = (itemId, category) => {
  removeCategoryIndexForItem(itemId)
  const normalized = String(category || '未分类')
  itemCategorySnapshot.set(itemId, normalized)
  let idSet = categorySearchIndex.get(normalized)
  if (!idSet) {
    idSet = new Set()
    categorySearchIndex.set(normalized, idSet)
  }
  idSet.add(itemId)
}

const setItemCategoryLocal = (itemId, category) => {
  if (!itemId) return
  const normalized = String(category || '未分类')
  categoryMap.value[itemId] = normalized
  applyCategoryIndexForItem(itemId, normalized)
  keywordCategoryMatchCache.clear()
}

const removeItemCategoryLocal = (itemId) => {
  if (!itemId) return
  delete categoryMap.value[itemId]
  removeCategoryIndexForItem(itemId)
  keywordCategoryMatchCache.clear()
}

const rebuildCategorySearchIndex = () => {
  categorySearchIndex.clear()
  itemCategorySnapshot.clear()
  keywordCategoryMatchCache.clear()
  const currentCategoryMap = categoryMap.value || {}
  for (const itemId of Object.keys(currentCategoryMap)) {
    applyCategoryIndexForItem(itemId, currentCategoryMap[itemId] || '未分类')
  }
}

const rebuildFilterIndexes = () => {
  rebuildTagSearchIndex()
  rebuildCategorySearchIndex()
}

const getKeywordTagMatchedIds = (keyword) => {
  if (!keyword) return null
  const cacheKey = `${filterDataRevision.value}|${keyword}`
  const cached = keywordTagMatchCache.get(cacheKey)
  if (cached) {
    return cached
  }
  const matchedIds = new Set()
  for (const [tag, idSet] of tagSearchIndex.entries()) {
    if (!tag.includes(keyword)) continue
    for (const itemId of idSet) {
      matchedIds.add(itemId)
    }
  }
  keywordTagMatchCache.set(cacheKey, matchedIds)
  return matchedIds
}

const getKeywordCategoryMatchedIds = (keyword) => {
  if (!keyword) return null
  const cacheKey = `${filterDataRevision.value}|${keyword}`
  const cached = keywordCategoryMatchCache.get(cacheKey)
  if (cached) {
    return cached
  }
  const matchedIds = new Set()
  for (const [category, idSet] of categorySearchIndex.entries()) {
    if (!String(category).toLowerCase().includes(keyword)) continue
    for (const itemId of idSet) {
      matchedIds.add(itemId)
    }
  }
  keywordCategoryMatchCache.set(cacheKey, matchedIds)
  return matchedIds
}

const ensureKeyboardSelectionVisible = async () => {
  await nextTick()
  const container = contentRef.value
  if (!container) return
  const selected = selectedIndex.value
  if (selected < 0) return
  const orderIndex = filteredHistory.value.findIndex((entry) => entry.index === selected)
  if (orderIndex < 0) return
  const EDGE_PADDING = 8
  const targetLeft = Math.max(0, orderIndex * IMAGE_ITEM_UNIT - EDGE_PADDING)
  const maxScrollLeft = Math.max(0, container.scrollWidth - container.clientWidth)
  container.scrollLeft = Math.min(maxScrollLeft, targetLeft)
}

const {
  bottomOffset,
  clampBottomOffset,
  startWindowOffsetDrag
} = useWindowOffset()

const getItemCategory = (itemId) => {
  return categoryMap.value[itemId] || '未分类'
}

const getItemTags = (itemId) => {
  const tags = tagMap.value[itemId]
  return Array.isArray(tags) ? tags : []
}

const enforcePreviewCacheSize = () => {
  while (previewCache.size > IMAGE_PREVIEW_CACHE_MAX_ITEMS) {
    const oldestKey = previewCache.keys().next().value
    if (!oldestKey) break
    previewCache.delete(oldestKey)
  }
}

const enforceAsyncPreviewCacheSize = () => {
  while (asyncPreviewCache.size > ASYNC_PREVIEW_CACHE_MAX_ITEMS) {
    const oldestKey = asyncPreviewCache.keys().next().value
    if (!oldestKey) break
    asyncPreviewCache.delete(oldestKey)
  }
}

const {
  history,
  categoryFilter,
  searchKeyword,
  selectedIndex,
  totalCount,
  pageOffset,
  pageSize,
  hasMore,
  isLoadingPage,
  isVisible,
  loadMoreIntent,
  isReorderMode,
  pinnedItems,
  filteredHistory,
  filteredHistoryState,
  selectedDisplayIndex,
  selectedStatusText,
  loadStatusText,
  normalizeImagePageSize,
  cyclePageSize,
  handleLoadMoreIntent,
  toggleReorderMode,
  handleReorder,
  promoteImageItem,
  deleteItem,
  clearPrefetchedPage,
  syncHistory,
  loadHistoryPage,
  loadTailPage,
  flushPendingHistorySync,
  mergeShowWindowPayload,
  mergeIncrementalImageItem,
  ensureInitialPageLoaded,
  scheduleHistorySync
} = useImageHistory({
  categoryMap,
  tagMap,
  categories,
  isAddingCategory,
  warmedIndices,
  warmingIndices,
  previewCache,
  asyncPreviewCache,
  tagSearchIndex,
  itemTagSnapshot,
  categorySearchIndex,
  itemCategorySnapshot,
  keywordTagMatchCache,
  keywordCategoryMatchCache,
  filterDataRevision,
  filterEntriesCache,
  getKeywordTagMatchedIds,
  getKeywordCategoryMatchedIds,
  getItemCategory,
  getItemTags,
  getLoadedHistoryCount,
  bumpFilterDataRevision,
  rebuildFilterIndexes,
  setItemCategoryLocal,
  setItemTagsLocal,
  removeItemCategoryLocal,
  removeItemTagsLocal
})

const {
  newCategoryName,
  newCategoryInputRef,
  canDeleteCategory,
  startCreateCategory,
  confirmCreateCategory,
  cancelCreateCategory,
  removeCategory,
  assignToCategory,
  editItemTags,
  handleDragStart,
  handleDragEnd,
  handleDrop
} = useImageCategories({
  categories,
  categoryMap,
  categoryFilter,
  isAddingCategory,
  getItemTags,
  setItemCategoryLocal,
  setItemTagsLocal,
  removeItemCategoryLocal,
  bumpFilterDataRevision,
  isCtrlKeyPressed,
  flushPendingHistorySync,
  contextMenuItemId,
  closeContextMenu
})

const {
  getPreviewDataUrl,
  selectByIndex,
  openFullscreen,
  warmupOne
} = useImagePreview({
  previewCache,
  asyncPreviewCache,
  enforcePreviewCacheSize,
  enforceAsyncPreviewCacheSize,
  warmedIndices,
  warmingIndices,
  history,
  selectedIndex,
  filteredHistory,
  filteredHistoryState,
  hasMore,
  isLoadingPage,
  totalCount,
  getLoadedHistoryCount,
  loadHistoryPage
})

const handleContainerMouseDown = (event) => {
  if (event.button !== 0) return
  const target = event.target
  if (isAddingCategory.value && target instanceof Element && !target.closest('.category-input')) {
    cancelCreateCategory()
  }
}

const rgbaBase64ToPngDataUrl = (rgbaBase64, width, height) => {
  if (!rgbaBase64 || width <= 0 || height <= 0) return ''
  const expectedLen = width * height * 4
  try {
    const binaryString = window.atob(rgbaBase64)
    if (binaryString.length !== expectedLen) return ''
    const bytes = new Uint8ClampedArray(expectedLen)
    for (let i = 0; i < expectedLen; i++) {
      bytes[i] = binaryString.charCodeAt(i)
    }
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    const ctx = canvas.getContext('2d')
    if (!ctx) return ''
    const imageData = new ImageData(bytes, width, height)
    ctx.putImageData(imageData, 0, 0)
    return canvas.toDataURL('image/png')
  } catch {
    return ''
  }
}

const scrollToStart = () => {
  imageListRef.value?.jumpToStart()
}

const scrollToEnd = async () => {
  if (hasMore.value) {
    try {
      await loadTailPage()
      await nextTick()
    } catch (error) {
      console.error('加载图片尾页失败:', error)
      await syncHistory()
      await nextTick()
    }
  }
  imageListRef.value?.jumpToEnd()
}

const fillById = async (itemId) => {
  if (writebackErrorMsg) {
    writebackErrorMsg.close()
    writebackErrorMsg = null
  }
  if (isFilling.value) return
  isFilling.value = true
  isVisible.value = false
  try {
    if (!itemId) return
    await ImageClipboardService.selectAndFillById(itemId)
  } catch (error) {
    console.error('回填图片失败:', error)
    writebackErrorMsg = ElMessage.error({
      message: t('imageClipboard.pasteFailed'),
      duration: 0,
      showClose: true
    })
  } finally {
    window.setTimeout(() => {
      isFilling.value = false
    }, 300)
  }
}

const downloadItem = async (itemId) => {
  if (!itemId) return
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t('imageClipboard.downloadDir')
    })
    const targetDirectory = Array.isArray(selected) ? selected[0] : selected
    if (!targetDirectory) return
    const result = await ImageClipboardService.copyItemToDirectory(itemId, targetDirectory)
    const savedPath = String(result?.savedPath || '')
    if (savedPath) {
      ElMessage.success(t('imageClipboard.downloadSuccess', {path: savedPath}))
    } else {
      ElMessage.success(t('imageClipboard.downloadSucceeded'))
    }
  } catch (error) {
    ElMessage.error(t('imageClipboard.downloadFailed', {error: String(error)}))
  }
}

const showContextMenu = openContextMenu

const applyPayload = (data, options = {}) => {
  const {refocus = false} = options
  clearPrefetchedPage()
  history.value = Array.isArray(data.history) ? data.history : []
  totalCount.value = getLoadedHistoryCount()
  pageOffset.value = totalCount.value

  hasMore.value = totalCount.value > 0
  warmedIndices.clear()
  warmingIndices.clear()
  if (typeof data.bottomOffset === 'number') {
    bottomOffset.value = clampBottomOffset(data.bottomOffset)
  }
  if (!isAddingCategory.value) {
    categoryMap.value = data.categories || {}
    tagMap.value = data.image_tags || {}
    pinnedItems.value = Array.isArray(data.pinned_items) ? data.pinned_items : []
    if (Array.isArray(data.category_list)) {
      const list = data.category_list.filter((c) => c !== '未分类' && c !== '全部')
      categories.value = ['未分类', ...Array.from(new Set(list))]
    } else {
      categories.value = ['未分类']
    }
  }
  rebuildFilterIndexes()
  selectedIndex.value = typeof data.selectedIndex === 'number' ? data.selectedIndex : 0
  if (selectedIndex.value < 0 || selectedIndex.value >= history.value.length) {
    selectedIndex.value = history.value.length > 0 ? 0 : -1
  }
  warmupOne(selectedIndex.value)
  bumpFilterDataRevision()
  isVisible.value = true
  if (refocus && !isAddingCategory.value) {
    nextTick(() => {
      containerRef.value?.focus()
    })
  }
}

const waitForFirstPaint = () =>
    new Promise((resolve) => {
      requestAnimationFrame(() => resolve())
    })

const handleKeydown = async (event) => {
  if (event.ctrlKey) {
    isCtrlKeyPressed.value = true
  }
  if (!isVisible.value) return
  if (isInputLikeTarget(event.target)) return

  if (event.ctrlKey && event.key >= '1' && event.key <= '9') {
    event.preventDefault()
    const index = parseInt(event.key, 10) - 1
    if (index >= 0 && index < filteredHistory.value.length) {
      await fillById(filteredHistory.value[index].item.id)
    }
    return
  }

  const visible = filteredHistory.value
  if (visible.length === 0) return
  let currentVisibleIndex = visible.findIndex((entry) => entry.index === selectedIndex.value)
  if (currentVisibleIndex < 0) currentVisibleIndex = 0
  if (event.key === 'ArrowLeft') {
    event.preventDefault()
    currentVisibleIndex = Math.max(0, currentVisibleIndex - 1)
    selectedIndex.value = visible[currentVisibleIndex].index
  } else if (event.key === 'ArrowRight') {
    event.preventDefault()
    currentVisibleIndex = Math.min(visible.length - 1, currentVisibleIndex + 1)
    selectedIndex.value = visible[currentVisibleIndex].index
    handleLoadMoreIntent()
  } else if (event.key === 'Home') {
    event.preventDefault()
    imageListRef.value?.jumpToStart()
  } else if (event.key === 'Enter') {
    event.preventDefault()
    if (selectedIndex.value >= 0 && selectedIndex.value < history.value.length) {
      await fillById(history.value[selectedIndex.value]?.id)
    }
  }
}

const isInputLikeTarget = (target) => {
  const tagName = target?.tagName?.toLowerCase?.()
  return tagName === 'input' || tagName === 'textarea' || target?.isContentEditable
}

const shouldDeferHeavyPayloadApply = (data) => {
  const historyList = Array.isArray(data?.history) ? data.history : null
  if (!historyList) return false
  const threshold = Math.max((Number(pageSize.value) || 10) * 6, 180)
  return historyList.length > threshold
}

const handleWindowKeydown = (event) => {
  if (event.ctrlKey) {
    isCtrlKeyPressed.value = true
  }
}

const handleWindowKeyup = (event) => {
  if (!event.ctrlKey) {
    isCtrlKeyPressed.value = false
  }
}

const handleWindowBlur = () => {
  isCtrlKeyPressed.value = false
  isVisible.value = false
  WindowService.imageBlur().catch(() => {
  })
}

const applyImagePayloadMeta = (payload) => {
  if (!payload || typeof payload !== 'object') return
  const snapshotCount = Array.isArray(payload.history)
      ? payload.history.filter((item) => item && item.id).length
      : 0
  if (snapshotCount > 0) {
    totalCount.value = Math.max(Number(totalCount.value) || 0, snapshotCount)
    const loadedCount = getLoadedHistoryCount()
    pageOffset.value = Math.max(Number(pageOffset.value) || 0, loadedCount)
    hasMore.value = pageOffset.value < totalCount.value
  }
  if (!isAddingCategory.value) {
    if (payload.categories) {
      categoryMap.value = payload.categories
    }
    if (payload.image_tags) {
      tagMap.value = payload.image_tags
    }
    if (Array.isArray(payload.pinned_items)) {
      pinnedItems.value = payload.pinned_items
    }
    if (Array.isArray(payload.category_list)) {
      const list = payload.category_list.filter((c) => c !== '未分类' && c !== '全部')
      categories.value = ['未分类', ...Array.from(new Set(list))]
    }
  }
  rebuildFilterIndexes()
  bumpFilterDataRevision()
}

onMounted(async () => {
  window.addEventListener('keydown', handleWindowKeydown)
  window.addEventListener('keyup', handleWindowKeyup)
  window.addEventListener('blur', handleWindowBlur)
  pageSize.value = normalizeImagePageSize(localStorage.getItem('image_history_page_size'))
  await nextTick()
  await waitForFirstPaint()
  ensureInitialPageLoaded()
  unlistenShowWindow = await listen('show-image-window', (event) => {
    const payload = event.payload || {}
    ensureInitialPageLoaded(true)
    if (shouldDeferHeavyPayloadApply(payload)) {
      if (typeof payload.bottomOffset === 'number') {
        bottomOffset.value = clampBottomOffset(payload.bottomOffset)
      }
      if (typeof payload.selectedIndex === 'number') {
        selectedIndex.value = payload.selectedIndex
      }
      isVisible.value = true
      scheduleHistorySync(0)
      return
    }
    if (Object.prototype.hasOwnProperty.call(payload, 'history') && Array.isArray(payload.history)) {

      mergeShowWindowPayload(payload)
      loadMoreIntent.value = false
      ensureInitialPageLoaded(true)
      isVisible.value = true
      if (!isAddingCategory.value) {
        nextTick(() => {
          containerRef.value?.focus()
        })
      }
      return
    }
    if (typeof payload.bottomOffset === 'number') {
      bottomOffset.value = clampBottomOffset(payload.bottomOffset)
    }
    if (typeof payload.selectedIndex === 'number') {
      selectedIndex.value = payload.selectedIndex
    }
    if (selectedIndex.value < 0 || selectedIndex.value >= history.value.length) {
      selectedIndex.value = history.value.length > 0 ? 0 : -1
    }
    warmupOne(selectedIndex.value)
    isVisible.value = true
    if (!isAddingCategory.value) {
      nextTick(() => {
        containerRef.value?.focus()
      })
    }
  })
  unlistenHistoryPayloadUpdated = await listen('image-history-payload-updated', (event) => {
    if (isAddingCategory.value) return
    const payload = event.payload || {}


    const hasHistoryArray = Array.isArray(payload.history)


    if (hasHistoryArray) {
      applyImagePayloadMeta(payload)
      clearPrefetchedPage()
      pageOffset.value = 0
      totalCount.value = 0
      hasMore.value = false
      loadMoreIntent.value = false
      void syncHistory()
      return
    }


    if (shouldDeferHeavyPayloadApply(payload)) {
      scheduleHistorySync(0)
      return
    }

    applyImagePayloadMeta(payload)
  })
  unlistenHistoryItemAdded = await listen('image-history-item-added', (event) => {
    if (isAddingCategory.value) return
    mergeIncrementalImageItem(event?.payload?.item)
  })
  unlistenWritebackResult = await listen('writeback-result', (event) => {
    const payload = event.payload || {}
    if (payload.source !== '图片') return

    if (writebackErrorMsg) {
      writebackErrorMsg.close()
      writebackErrorMsg = null
    }

    if (!payload.success) {
      console.warn('图片回填失败详情:', payload.detail)
      writebackErrorMsg = ElMessage.error({
        message: t('imageClipboard.pasteFailed'),
        duration: 0,
        showClose: true
      })
    }
  })

  unlistenImageQueueFull = await listen('image-queue-full', (event) => {
    const msg = event?.payload?.message || '图片处理队列已满'
    ElMessage.warning({message: msg, duration: 3000})
  })

  unlistenImagePersistDrop = await listen('image-persist-drop', (event) => {
    // 优先 i18n，payload.message 仅作兜底
    const msg = t('imageClipboard.persistDrop')
      || event?.payload?.message
      || '图片持久化队列超时'
    ElMessage.warning({message: msg, duration: 4000})
  })
})

onBeforeUnmount(() => {
  flushPendingHistorySync()
  if (filterDebounceTimer) {
    clearTimeout(filterDebounceTimer)
    filterDebounceTimer = null
  }
  if (unlistenShowWindow) {
    unlistenShowWindow()
    unlistenShowWindow = null
  }
  if (unlistenHistoryPayloadUpdated) {
    unlistenHistoryPayloadUpdated()
    unlistenHistoryPayloadUpdated = null
  }
  if (unlistenHistoryItemAdded) {
    unlistenHistoryItemAdded()
    unlistenHistoryItemAdded = null
  }
  if (unlistenWritebackResult) {
    unlistenWritebackResult()
    unlistenWritebackResult = null
  }
  if (unlistenImageQueueFull) {
    unlistenImageQueueFull()
    unlistenImageQueueFull = null
  }
  if (unlistenImagePersistDrop) {
    unlistenImagePersistDrop()
    unlistenImagePersistDrop = null
  }
  window.removeEventListener('keydown', handleWindowKeydown)
  window.removeEventListener('keyup', handleWindowKeyup)
  window.removeEventListener('blur', handleWindowBlur)
})

watch(selectedIndex, (value) => {
  warmupOne(value)
})

watch(filteredHistory, (list) => {
  if (!Array.isArray(list) || list.length === 0) return
  const exists = list.some((entry) => entry.index === selectedIndex.value)
  if (!exists) {
    selectedIndex.value = list[0].index
  }
})

watch([contextMenuVisible, contextMenuX, contextMenuY], async ([visible]) => {
  if (!visible) return
  await nextTick()
  syncContextMenuPosition()
})

let filterDebounceTimer = null
watch([searchKeyword, categoryFilter], () => {
  if (!isVisible.value) return
  if (filterDebounceTimer) {
    clearTimeout(filterDebounceTimer)
  }
  filterDebounceTimer = setTimeout(() => {
    scheduleHistorySync(0)
  }, 180)
})
</script>

<style src="./image_clipboard.css"></style>

<style src="./image_clipboard.scoped.css" scoped></style>
