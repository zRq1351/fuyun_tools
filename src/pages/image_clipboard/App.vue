<template>
  <div ref="containerRef" class="container" tabindex="-1" @click="closeContextMenu" @keydown="handleKeydown"
       @mousedown="handleContainerMouseDown">
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
        create-category-text="新增分类"
        search-placeholder="搜索分类/标签"
        :start-create-category="startCreateCategory"
        :start-window-offset-drag="startWindowOffsetDrag"
        :show-ai-toggle="false"
    />
    <div v-if="filteredHistory.length === 0" class="empty-state">
      <el-empty :image-size="100" description="暂无图片剪切板记录"/>
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
        :handle-item-hover="handleItemHover"
        :has-more="hasMore"
        :is-ctrl-key-pressed="isCtrlKeyPressed"
        :is-loading-page="isLoadingPage"
        :open-fullscreen="openFullscreen"
        :promote-item="promoteImageItem"
        :select-by-index="selectByIndex"
        :selected-index="selectedIndex"
        :show-context-menu="showContextMenu"
        :visible-history="filteredHistory"
        @content-scroll="tryLoadMoreByScroll"
        @load-more-intent="handleLoadMoreIntent"
    />

    <div class="status-footer" @click.stop @mousedown.stop>
      <div class="status-text">
        <span class="status-label">{{ selectedStatusText }}</span>
        <span class="status-meta">{{ loadStatusText }}</span>
        <div class="status-actions">
          <button
              :title="`切换分页大小（当前每页 ${pageSize} 条）`"
              class="nav-action-btn"
              type="button"
              @click="cyclePageSize"
          >
            每页{{ pageSize }}
          </button>
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
        ref="contextMenuRef"
        class="context-menu"
        @click.stop
    >
      <div class="context-menu-item" @click="editItemTags">
        编辑标签
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
        <el-icon v-if="getItemCategory(contextMenuItemId) === category" class="check-icon">
          <Check/>
        </el-icon>
      </div>
    </div>

  </div>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch} from 'vue'
import {ArrowLeftBold, ArrowRightBold, Check} from '@element-plus/icons-vue'
import {ElMessage} from 'element-plus'
import {listen} from '@tauri-apps/api/event'
import {convertFileSrc} from '@tauri-apps/api/core'
import {open as openDialog} from '@tauri-apps/plugin-dialog'
import {ImageCategoryService, ImageClipboardService, WindowService} from '../../services/ipc'
import ClipboardToolbar from '../clipboard/components/ClipboardToolbar.vue'
import ImageClipboardList from './components/ImageClipboardList.vue'
import {useWindowOffset} from '../clipboard/composables/useWindowOffset'
import {useContextMenuState} from '../shared/useContextMenuState'
import {runCategoryAssignment} from '../shared/categoryActions'

const containerRef = ref(null)
const imageListRef = ref(null)
const contentRef = computed(() => imageListRef.value?.contentRef?.value || imageListRef.value?.contentRef || null)
const contextMenuRef = ref(null)
const history = ref([])
const categoryMap = ref({})
const tagMap = ref({})
const pinnedItems = ref([])
const categories = ref(['未分类'])
const categoryFilter = ref('全部')
const selectedIndex = ref(0)
const searchKeyword = ref('')
const totalCount = ref(0)
const pageOffset = ref(0)
const pageSize = ref(10)
const hasMore = ref(false)
const isLoadingPage = ref(false)
const sortBy = ref('pinnedFirst')
const sortOrder = ref('asc')
const isVisible = ref(false)
const isAddingCategory = ref(false)
const newCategoryName = ref('')
const newCategoryInputRef = ref(null)
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

const dragItemId = ref('')
const isFilling = ref(false)
const categoryInputOpenedAt = ref(0)
const previewCache = new Map()
const asyncPreviewCache = new Map()
const warmedIndices = new Set()
const warmingIndices = new Set()
const pendingWarmupItemIds = new Set()
let unlistenShowWindow = null
let unlistenItemPromoted = null
let unlistenHistoryPayloadUpdated = null
let unlistenHistoryItemAdded = null
let unlistenPreviewReady = null
let unlistenWritebackResult = null
let pendingHistorySync = false
let historyUpdateTimer = null
let initialPageRetryTimer = null
let warmupBatchTimer = null
let warmupBatchInFlight = null
const isCtrlKeyPressed = ref(false)
const loadMoreIntent = ref(false)
const prefetchedPage = ref(null)
const isPrefetchingPage = ref(false)
let prefetchRequestSeq = 0
let prefetchPromise = null

const IMAGE_ITEM_UNIT = 258
const IMAGE_PREVIEW_CACHE_MARGIN = 24
const IMAGE_PREVIEW_CACHE_MAX_ITEMS = 300
const ASYNC_PREVIEW_CACHE_MAX_ITEMS = 180
const filterDataRevision = ref(0)
const filterEntriesCache = new Map()
const keywordTagMatchCache = new Map()
const keywordCategoryMatchCache = new Map()
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

const getLoadedHistorySnapshot = () => {
  const loadedItems = []
  const existingById = new Map()
  for (const item of history.value) {
    if (!item?.id) continue
    loadedItems.push(item)
    existingById.set(item.id, item)
  }
  return {loadedItems, existingById}
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

const currentPageQuerySignature = () =>
    JSON.stringify({
      pageSize: pageSize.value,
      category: categoryFilter.value === '全部' ? null : categoryFilter.value,
      keyword: searchKeyword.value.trim() || null,
      sortBy: sortBy.value,
      sortOrder: sortOrder.value
    })

const clearPrefetchedPage = () => {
  prefetchRequestSeq += 1
  prefetchedPage.value = null
  isPrefetchingPage.value = false
  prefetchPromise = null
}

const getHistoryRetentionLimit = (...candidateCounts) => {
  const normalizedPageSize = normalizeImagePageSize(pageSize.value)
  const loadedCount = Math.max(0, Number(pageOffset.value) || 0)
  const normalizedCandidates = candidateCounts
      .map((value) => Number(value))
      .filter((value) => Number.isFinite(value))
      .map((value) => Math.max(0, value))
  return Math.max(100, normalizedPageSize * 4, loadedCount, ...normalizedCandidates)
}

const prefetchNextPage = async () => {
  if (isLoadingPage.value || isPrefetchingPage.value || !hasMore.value) return
  const offset = pageOffset.value
  const signature = currentPageQuerySignature()
  if (
      prefetchedPage.value
      && prefetchedPage.value.offset === offset
      && prefetchedPage.value.signature === signature
  ) {
    return
  }
  const requestSeq = ++prefetchRequestSeq
  isPrefetchingPage.value = true
  prefetchPromise = (async () => {
    try {
      const data = await ImageClipboardService.getHistoryPage({
        offset,
        limit: pageSize.value,
        category: categoryFilter.value === '全部' ? null : categoryFilter.value,
        keyword: searchKeyword.value.trim() || null,
        pinnedOnly: false,
        sortBy: sortBy.value,
        sortOrder: sortOrder.value
      })
      if (requestSeq !== prefetchRequestSeq) return
      prefetchedPage.value = {
        offset,
        signature,
        data
      }
    } catch (_) {
      if (requestSeq === prefetchRequestSeq) {
        prefetchedPage.value = null
      }
    } finally {
      if (requestSeq === prefetchRequestSeq) {
        isPrefetchingPage.value = false
        prefetchPromise = null
      }
    }
  })()
  await prefetchPromise
}

const flushPendingHistorySync = () => {
  if (pendingHistorySync) {
    pendingHistorySync = false
    scheduleHistorySync(0)
  }
}

const handleContainerMouseDown = (event) => {
  if (event.button !== 0) return
  const target = event.target
  if (isAddingCategory.value && target instanceof Element && !target.closest('.category-input')) {
    cancelCreateCategory()
  }
}

const handleLoadMoreIntent = () => {
  if (!hasMore.value || isLoadingPage.value) return
  loadMoreIntent.value = true
  void tryLoadMoreByScroll()
}

const tryLoadMoreByScroll = async () => {
  if (!hasMore.value || isLoadingPage.value) return false
  const container = contentRef.value
  if (!container) return false
  const remaining = container.scrollWidth - container.clientWidth - container.scrollLeft
  if (remaining <= 240 && loadMoreIntent.value) {
    loadMoreIntent.value = false
    const beforeLoaded = getLoadedHistoryCount()
    await loadHistoryPage({reset: false})
    return getLoadedHistoryCount() > beforeLoaded
  }
  return false
}

const loadTailPage = async () => {
  if (!hasMore.value || isLoadingPage.value) return false
  const loadedCount = getLoadedHistoryCount()
  const exactTotal = Math.max(Number(totalCount.value) || 0, loadedCount)
  const targetOffset = Math.max(0, exactTotal - (Number(pageSize.value) || 10))
  if (targetOffset <= 0 && loadedCount >= exactTotal) {
    return false
  }
  clearPrefetchedPage()
  const data = await ImageClipboardService.getHistoryPage({
    offset: targetOffset,
    limit: pageSize.value,
    category: categoryFilter.value === '全部' ? null : categoryFilter.value,
    keyword: searchKeyword.value.trim() || null,
    pinnedOnly: false,
    sortBy: sortBy.value,
    sortOrder: sortOrder.value
  })
  mergeImagePageIntoState(data, false)
  return true
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

const canDeleteCategory = (category) => {
  return category !== '未分类'
}

const getItemCategory = (itemId) => {
  return categoryMap.value[itemId] || '未分类'
}

const getItemTags = (itemId) => {
  const tags = tagMap.value[itemId]
  return Array.isArray(tags) ? tags : []
}

const pinnedItemSet = computed(() => new Set(pinnedItems.value))
const isPinned = (itemId) => pinnedItemSet.value.has(itemId)

const filteredHistoryState = computed(() => {
  const revision = filterDataRevision.value
  const category = categoryFilter.value
  const keyword = searchKeyword.value.trim().toLowerCase()
  const cacheKey = `${revision}|${category}|${keyword}`
  let cached = filterEntriesCache.get(cacheKey)
  if (!cached) {
    const out = []
    const displayIndexMap = new Map()
    const categoryFilteredIds = category === '全部' ? null : (categorySearchIndex.get(category) || new Set())
    const tagMatchedIds = keyword ? getKeywordTagMatchedIds(keyword) : null
    const categoryMatchedIds = keyword ? getKeywordCategoryMatchedIds(keyword) : null
    const currentPinnedSet = pinnedItemSet.value
    for (let index = 0; index < history.value.length; index++) {
      const item = history.value[index]
      if (!item) continue
      const itemId = item.id
      if (categoryFilteredIds && !categoryFilteredIds.has(itemId)) {
        continue
      }
      if (keyword) {
        const categoryMatched = categoryMatchedIds && categoryMatchedIds.has(itemId)
        const tagMatched = tagMatchedIds && tagMatchedIds.has(itemId)
        if (!categoryMatched && !tagMatched) {
          continue
        }
      }
      const nextDisplay = out.length + 1
      out.push({
        item,
        index,
        pinned: currentPinnedSet.has(itemId),
        category: getItemCategory(itemId),
        tags: getItemTags(itemId)
      })
      displayIndexMap.set(index, nextDisplay)
    }
    cached = {
      entries: out,
      total: out.length,
      displayIndexMap
    }
    filterEntriesCache.set(cacheKey, cached)
  }
  const selectedDisplay = cached.displayIndexMap.get(selectedIndex.value) || 1
  return {
    entries: cached.entries,
    total: cached.total,
    selectedDisplay
  }
})

const filteredHistory = computed(() => filteredHistoryState.value.entries)

const previewCacheKeepIds = computed(() => {
  if (filteredHistoryState.value.total === 0) return []
  const ids = filteredHistory.value
      .map((entry) => entry.item?.id)
      .filter(Boolean)
  const selectedId = history.value[selectedIndex.value]?.id
  if (selectedId) {
    ids.push(selectedId)
  }
  return Array.from(new Set(ids))
})

const selectedStatusText = computed(() => {
  const total = totalCount.value || filteredHistoryState.value.total
  if (total === 0) return '当前无选中项'
  const display = filteredHistoryState.value.selectedDisplay
  return `当前选中：第 ${display} / ${total} 条`
})

const loadStatusText = computed(() => {
  if (isLoadingPage.value) return '正在加载...'
  if (hasMore.value) return `已加载 ${filteredHistoryState.value.total} / ${totalCount.value || filteredHistoryState.value.total}`
  return `已全部加载 ${filteredHistoryState.value.total} 条`
})

const IMAGE_PAGE_SIZE_OPTIONS = [10, 30, 50]
const normalizeImagePageSize = (value) => {
  const parsed = Number(value)
  return IMAGE_PAGE_SIZE_OPTIONS.includes(parsed) ? parsed : 10
}

const cyclePageSize = async () => {
  const current = normalizeImagePageSize(pageSize.value)
  const index = IMAGE_PAGE_SIZE_OPTIONS.indexOf(current)
  const next = IMAGE_PAGE_SIZE_OPTIONS[(index + 1) % IMAGE_PAGE_SIZE_OPTIONS.length]
  pageSize.value = next
  localStorage.setItem('image_history_page_size', String(next))
  clearPrefetchedPage()
  await syncHistory()
}

const buildFileUrlFromPath = (imagePath) => {
  if (!imagePath) return ''
  try {
    return convertFileSrc(imagePath)
  } catch (_) {
    return ''
  }
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

const mergeIncrementalImageItem = (rawItem) => {
  if (!rawItem?.id) return
  const selectedId = history.value[selectedIndex.value]?.id
  const pinnedSet = new Set(pinnedItems.value)
  let existingIndex = -1
  let firstUnpinnedIndex = -1
  for (let i = 0; i < history.value.length; i++) {
    const item = history.value[i]
    if (!item) continue
    if (existingIndex < 0 && item.id === rawItem.id) {
      existingIndex = i
    }
    if (firstUnpinnedIndex < 0 && !pinnedSet.has(item.id)) {
      firstUnpinnedIndex = i
    }
    if (existingIndex >= 0 && firstUnpinnedIndex >= 0) {
      break
    }
  }
  const existing = existingIndex >= 0 ? history.value[existingIndex] : null
  if (existingIndex >= 0) {
    history.value.splice(existingIndex, 1)
    if (firstUnpinnedIndex > existingIndex) {
      firstUnpinnedIndex -= 1
    }
  }
  const normalized = {
    id: rawItem.id,
    width: rawItem.width ?? existing?.width ?? 0,
    height: rawItem.height ?? existing?.height ?? 0,
    preview_png_base64: rawItem.preview_png_base64 ?? rawItem.previewPngBase64 ?? existing?.preview_png_base64 ?? '',
    image_path: rawItem.image_path ?? rawItem.imagePath ?? existing?.image_path ?? ''
  }
  const isPinnedItem = pinnedSet.has(normalized.id)
  const insertIndex = isPinnedItem ? 0 : (firstUnpinnedIndex >= 0 ? firstUnpinnedIndex : history.value.length)
  history.value.splice(insertIndex, 0, normalized)
  const keepCount = getHistoryRetentionLimit(
      (Number(pageOffset.value) || 0) + (existingIndex >= 0 ? 0 : 1)
  )
  if (history.value.length > keepCount) {
    history.value = history.value.slice(0, keepCount)
  }
  let loadedCount = 0
  let nextSelectedIndex = -1
  for (let i = 0; i < history.value.length; i++) {
    const item = history.value[i]
    if (!item) continue
    loadedCount += 1
    if (selectedId && nextSelectedIndex < 0 && item.id === selectedId) {
      nextSelectedIndex = i
    }
  }
  totalCount.value = Math.max(totalCount.value || 0, loadedCount)
  pageOffset.value = Math.max(pageOffset.value || 0, loadedCount)
  if (selectedId) {
    selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : 0
  } else if (selectedIndex.value < 0) {
    selectedIndex.value = 0
  }
  const wasHistoryMutated = existingIndex >= 0 || !!rawItem?.id
  if (wasHistoryMutated) {
    bumpFilterDataRevision()
  }
}

const prunePreviewCache = (keepIds) => {
  if (!Array.isArray(keepIds)) return
  const keepSet = new Set(keepIds)
  for (const key of previewCache.keys()) {
    if (!keepSet.has(key)) {
      previewCache.delete(key)
    }
  }
  enforcePreviewCacheSize()
}

const pruneAsyncPreviewCache = (keepIds) => {
  if (!Array.isArray(keepIds)) return
  const keepSet = new Set(keepIds)
  for (const key of asyncPreviewCache.keys()) {
    if (!keepSet.has(key)) {
      asyncPreviewCache.delete(key)
    }
  }
  enforceAsyncPreviewCacheSize()
}

const rgbaBase64ToPngDataUrl = (rgbaBase64, width, height) => {
  if (!rgbaBase64 || width <= 0 || height <= 0) return ''
  const binary = window.atob(rgbaBase64)
  const bytes = new Uint8ClampedArray(binary.length)
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i)
  }
  if (bytes.length !== width * height * 4) {
    return ''
  }
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext('2d')
  if (!ctx) return ''
  const imageData = new ImageData(bytes, width, height)
  ctx.putImageData(imageData, 0, 0)
  return canvas.toDataURL('image/png')
}

// 缓存命中率统计
const cacheStats = {
  hits: 0,
  misses: 0,
  get hitRate() {
    const total = this.hits + this.misses
    return total > 0 ? (this.hits / total * 100).toFixed(2) : 0
  },
  recordHit() {
    this.hits++
  },
  recordMiss() {
    this.misses++
  },
  reset() {
    this.hits = 0
    this.misses = 0
  },
  getReport() {
    return `缓存命中率: ${this.hitRate}% (${this.hits}/${this.hits + this.misses})`
  }
}

const getPreviewDataUrl = (item) => {
  if (previewCache.has(item.id)) {
    return previewCache.get(item.id)
  }
  try {
    // 统一使用异步生成的预览（preview_png_base64）
    const previewBase64 = typeof item.preview_png_base64 === 'string' ? item.preview_png_base64.trim() : ''
    if (previewBase64) {
      const previewUrl = `data:image/png;base64,${previewBase64}`
      previewCache.set(item.id, previewUrl)
      enforcePreviewCacheSize()
      return previewUrl
    }

    // 检查是否有异步生成的预览（通过事件更新）
    if (asyncPreviewCache.has(item.id)) {
      const previewUrl = asyncPreviewCache.get(item.id)
      asyncPreviewCache.delete(item.id)
      asyncPreviewCache.set(item.id, previewUrl)
      enforceAsyncPreviewCacheSize()
      previewCache.set(item.id, previewUrl)
      enforcePreviewCacheSize()
      return previewUrl
    }

    // 预览还未生成，使用文件路径作为占位
    const previewUrl = buildFileUrlFromPath(item.image_path)
    previewCache.set(item.id, previewUrl)
    enforcePreviewCacheSize()
    return previewUrl
  } catch (error) {
    console.error('图片预览生成失败:', error)
    return ''
  }
}

const selectByIndex = (index) => {
  selectedIndex.value = index
  warmupAround(index)
}

const enqueueWarmupByIndex = (index) => {
  if (index < 0 || index >= history.value.length) return
  const itemId = history.value[index]?.id
  if (!itemId || warmedIndices.has(itemId) || warmingIndices.has(itemId)) return
  pendingWarmupItemIds.add(itemId)
}

const flushWarmupBatch = async () => {
  if (warmupBatchInFlight) return warmupBatchInFlight
  const itemIds = Array.from(pendingWarmupItemIds).filter(
      (itemId) => itemId && !warmedIndices.has(itemId) && !warmingIndices.has(itemId)
  )
  pendingWarmupItemIds.clear()
  if (itemIds.length === 0) return null
  itemIds.forEach((itemId) => warmingIndices.add(itemId))
  warmupBatchInFlight = ImageClipboardService.warmupMultipleItems(itemIds)
      .then(() => {
        itemIds.forEach((itemId) => warmedIndices.add(itemId))
      })
      .catch((error) => {
        console.error('批量预热失败:', error)
      })
      .finally(() => {
        itemIds.forEach((itemId) => warmingIndices.delete(itemId))
        warmupBatchInFlight = null
        if (pendingWarmupItemIds.size > 0) {
          void flushWarmupBatch()
        }
      })
  return warmupBatchInFlight
}

const scheduleWarmupBatch = (delay = 80) => {
  if (warmupBatchTimer) {
    clearTimeout(warmupBatchTimer)
  }
  warmupBatchTimer = setTimeout(() => {
    warmupBatchTimer = null
    void flushWarmupBatch()
  }, delay)
}

const warmupOne = (index) => {
  enqueueWarmupByIndex(index)
  scheduleWarmupBatch(60)
}

const warmupBatch = (startIndex, count = 6) => {
  for (let i = startIndex; i < Math.min(startIndex + count, history.value.length); i++) {
    enqueueWarmupByIndex(i)
  }
  scheduleWarmupBatch(80)
}

const warmupAround = (index) => {
  warmupBatch(Math.max(0, index - 1), 3)
}

const handleItemHover = (index) => {
  warmupAround(index)
}

const scrollToStart = async () => {
  if (contentRef.value) {
    contentRef.value.scrollLeft = 0
  }
  if (filteredHistory.value.length > 0) {
    const firstIndex = filteredHistory.value[0].index
    selectedIndex.value = firstIndex
    await ensureKeyboardSelectionVisible()
  }
}

const scrollToEnd = async () => {
  if (!contentRef.value) return
  contentRef.value.scrollLeft = Math.max(0, contentRef.value.scrollWidth - contentRef.value.clientWidth)
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
  if (filteredHistory.value.length > 0) {
    const lastIndex = filteredHistory.value[filteredHistory.value.length - 1].index
    selectedIndex.value = lastIndex
    await ensureKeyboardSelectionVisible()
  }
}

const fillById = async (itemId) => {
  if (isFilling.value) return
  isFilling.value = true
  isVisible.value = false
  try {
    if (!itemId) return
    await ImageClipboardService.selectAndFillById(itemId)
  } catch (error) {
    console.error('回填图片失败:', error)
    ElMessage.error(`回填图片失败: ${String(error)}`)
  } finally {
    window.setTimeout(() => {
      isFilling.value = false
    }, 300)
  }
}

const openFullscreen = async (itemId) => {
  try {
    if (!itemId) return
    await ImageClipboardService.openPreviewWindowById(itemId)
  } catch (error) {
    console.error('打开预览窗口失败:', error)
    ElMessage.error(`打开预览窗口失败: ${String(error)}`)
  }
}

const downloadItem = async (itemId) => {
  if (!itemId) return
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: '选择图片下载目录'
    })
    const targetDirectory = Array.isArray(selected) ? selected[0] : selected
    if (!targetDirectory) return
    const result = await ImageClipboardService.copyItemToDirectory(itemId, targetDirectory)
    const savedPath = String(result?.savedPath || '')
    if (savedPath) {
      ElMessage.success(`下载成功：${savedPath}`)
    } else {
      ElMessage.success('下载成功')
    }
  } catch (error) {
    ElMessage.error(`下载失败: ${String(error)}`)
  }
}

const promoteImageItem = async (itemId) => {
  try {
    const shouldPin = !isPinned(itemId)
    await ImageClipboardService.setItemPinned(itemId, shouldPin)

    // 优化：只更新本地状态，不重新加载所有数据
    if (shouldPin) {
      // 置顶：将图片移到列表顶部
      pinnedItems.value = [itemId, ...pinnedItems.value.filter(id => id !== itemId)]
      promoteLocalItemToTop(itemId)
    } else {
      // 取消置顶：从置顶列表中移除，并将图片移到非置顶区域的开头
      pinnedItems.value = pinnedItems.value.filter(id => id !== itemId)
      demoteLocalItemFromTop(itemId)
    }

    // 触发置顶事件，通知其他窗口
    const {emit} = await import('@tauri-apps/api/event')
    await emit('image-item-pinned', {itemId, pinned: shouldPin})

  } catch (error) {
    console.error('置顶图片失败:', error)
    ElMessage.error(`置顶图片失败: ${String(error)}`)
    // 如果失败，回退到重新加载
    await syncHistory()
  }
}

// 取消置顶：将图片从顶部移动到非置顶区域的开头
const demoteLocalItemFromTop = (itemId) => {
  if (!itemId || !Array.isArray(history.value) || history.value.length < 2) return

  const currentIndex = history.value.findIndex((item) => item?.id === itemId)
  if (currentIndex < 0) return

  const selectedId = history.value[selectedIndex.value]?.id
  const [moved] = history.value.splice(currentIndex, 1)
  if (!moved) return

  // 找到第一个非置顶图片的位置
  const pinnedSet = new Set(pinnedItems.value)
  let insertIndex = history.value.findIndex((item) => !pinnedSet.has(item.id))

  // 如果没有找到非置顶图片，插入到末尾
  if (insertIndex < 0) {
    insertIndex = history.value.length
  }

  history.value.splice(insertIndex, 0, moved)

  // 更新选中索引
  if (selectedId) {
    const nextSelectedIndex = history.value.findIndex((item) => item?.id === selectedId)
    selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : 0
  } else {
    selectedIndex.value = 0
  }
  bumpFilterDataRevision()
}

const deleteItem = async (itemId, index) => {
  if (!itemId) return
  const snapshot = {
    history: history.value.slice(),
    categoryMap: {...categoryMap.value},
    tagMap: Object.fromEntries(
        Object.entries(tagMap.value || {}).map(([key, value]) => [key, Array.isArray(value) ? [...value] : []])
    ),
    pinnedItems: pinnedItems.value.slice(),
    selectedIndex: selectedIndex.value,
    totalCount: totalCount.value,
    pageOffset: pageOffset.value,
    hasMore: hasMore.value
  }
  try {
    previewCache.delete(itemId)
    asyncPreviewCache.delete(itemId)
    removeItemCategoryLocal(itemId)
    removeItemTagsLocal(itemId)
    pinnedItems.value = pinnedItems.value.filter((id) => id !== itemId)
    if (Number.isInteger(index) && index >= 0 && index < history.value.length) {
      history.value.splice(index, 1)
      if (selectedIndex.value >= history.value.length) {
        selectedIndex.value = Math.max(0, history.value.length - 1)
      }
      bumpFilterDataRevision()
    }
    await ImageClipboardService.removeItemById(itemId)
    await syncHistory()
  } catch (error) {
    console.error('删除图片记录失败:', error)
    history.value = snapshot.history
    categoryMap.value = snapshot.categoryMap
    tagMap.value = snapshot.tagMap
    pinnedItems.value = snapshot.pinnedItems
    selectedIndex.value = snapshot.selectedIndex
    totalCount.value = snapshot.totalCount
    pageOffset.value = snapshot.pageOffset
    hasMore.value = snapshot.hasMore
    rebuildFilterIndexes()
    bumpFilterDataRevision()
    try {
      await syncHistory()
    } catch (syncError) {
      console.error('删除失败后重同步图片历史失败:', syncError)
    }
  }
}

const showContextMenu = openContextMenu

const persistImageCategory = async (itemId, category, resetTagsWhenUnclassified = false) => {
  await ImageCategoryService.setItemCategory(itemId, category)
  if (resetTagsWhenUnclassified && category === '未分类') {
    setItemTagsLocal(itemId, [])
    await ImageClipboardService.setItemTags(itemId, [])
  }
}

const assignToCategory = async (category) => {
  await runCategoryAssignment({
    itemKey: contextMenuItemId.value,
    category,
    applyLocal: (itemId, nextCategory) => {
      setItemCategoryLocal(itemId, nextCategory)
      bumpFilterDataRevision()
    },
    persist: (itemId, nextCategory) => persistImageCategory(itemId, nextCategory, true),
    onError: (error) => console.error('设置图片分类失败:', error),
    onFinally: closeContextMenu
  })
}

const editItemTags = async () => {
  const itemId = contextMenuItemId.value
  if (!itemId) return
  const current = getItemTags(itemId).join(', ')
  try {
    const {value} = await ElMessageBox.prompt('请输入标签（多个标签用英文逗号分隔）', '编辑标签', {
      inputValue: current,
      confirmButtonText: '保存',
      cancelButtonText: '取消'
    })
    const tags = String(value || '')
        .split(/[,，]/)
        .map((item) => item.trim())
        .filter((item, idx, arr) => item && arr.indexOf(item) === idx)
    setItemTagsLocal(itemId, tags)
    bumpFilterDataRevision()
    await ImageClipboardService.setItemTags(itemId, tags)
    closeContextMenu()
    ElMessage.success('标签已更新')
  } catch (error) {
    if (error !== 'cancel') {
      ElMessage.error(`更新标签失败: ${error}`)
    }
  }
}

const handleDragStart = (event, itemId) => {
  if (!isCtrlKeyPressed.value) {
    event.preventDefault()
    return
  }
  flushPendingHistorySync()
  dragItemId.value = itemId
  event.dataTransfer.effectAllowed = 'copy'
  event.dataTransfer.setData('text/plain', itemId)
}

const handleDragEnd = () => {
  dragItemId.value = ''
}

const handleDrop = async (event, category) => {
  event.preventDefault()
  const target = event.currentTarget
  if (target && target.classList.contains('category-pill')) {
    target.classList.remove('drag-over')
  }
  const droppedItemId = dragItemId.value || event.dataTransfer?.getData('text/plain') || ''
  await runCategoryAssignment({
    itemKey: droppedItemId,
    category,
    applyLocal: (itemId, nextCategory) => {
      setItemCategoryLocal(itemId, nextCategory)
      bumpFilterDataRevision()
    },
    persist: (itemId, nextCategory) => persistImageCategory(itemId, nextCategory),
    onError: (error) => console.error('拖拽设置图片分类失败:', error)
  })
}

const startCreateCategory = () => {
  isAddingCategory.value = true
  categoryInputOpenedAt.value = Date.now()
  newCategoryName.value = ''
  nextTick(() => {
    newCategoryInputRef.value?.focus()
  })
}

const confirmCreateCategory = async () => {
  const category = newCategoryName.value.trim()
  isAddingCategory.value = false
  newCategoryName.value = ''
  categoryInputOpenedAt.value = 0
  if (category && category !== '未分类' && category !== '全部' && !categories.value.includes(category)) {
    categories.value.push(category)
    try {
      await ImageCategoryService.addCategory(category)
    } catch (error) {
      console.error('添加图片分类失败:', error)
    }
  }
}

const cancelCreateCategory = () => {
  isAddingCategory.value = false
  newCategoryName.value = ''
  categoryInputOpenedAt.value = 0
}

const removeCategory = async (category) => {
  if (!canDeleteCategory(category)) return
  categories.value = categories.value.filter((item) => item !== category)
  Object.keys(categoryMap.value).forEach((key) => {
    if (categoryMap.value[key] === category) {
      removeItemCategoryLocal(key)
      setItemTagsLocal(key, [])
    }
  })
  bumpFilterDataRevision()
  if (categoryFilter.value === category) {
    categoryFilter.value = '全部'
  }
  try {
    await ImageCategoryService.removeCategory(category)
  } catch (error) {
    console.error('删除图片分类失败:', error)
  }
}

const applyPayload = (data, options = {}) => {
  const {refocus = false} = options
  clearPrefetchedPage()
  history.value = Array.isArray(data.history) ? data.history : []
  totalCount.value = getLoadedHistoryCount()
  pageOffset.value = totalCount.value
  // 快照仅用于首屏快速展示，不依赖 pageSize 推断 hasMore，后续由分页接口校正。
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

const mergeShowWindowPayload = (data) => {
  clearPrefetchedPage()
  const incoming = Array.isArray(data?.history) ? data.history.filter((item) => item?.id) : []

  // 判断是否为完整快照：包含 history 字段且数据结构完整
  const hasHistoryField = Object.prototype.hasOwnProperty.call(data, 'history') && Array.isArray(data.history)
  const isFullSnapshot = hasHistoryField && (incoming.length === 0 || (incoming.length > 0 && incoming[0]?.image_path !== undefined))

  if (isFullSnapshot) {
    // 完整快照：直接替换历史数据
    if (incoming.length === 0) {
      // 清理全部后，history 为空数组，直接清空
      history.value = []
    } else {
      // 有数据时，完全替换
      const front = []
      for (const item of incoming) {
        if (!item?.id) continue
        front.push({
          id: item.id,
          width: item.width ?? 0,
          height: item.height ?? 0,
          preview_png_base64: item.preview_png_base64 ?? item.previewPngBase64 ?? '',
          image_path: item.image_path ?? item.imagePath ?? ''
        })
      }
      history.value = front
    }

    const loadedCount = history.value.length
    totalCount.value = loadedCount
    pageOffset.value = loadedCount
  } else if (incoming.length > 0) {
    // 增量更新：合并新旧数据
    const {loadedItems, existingById} = getLoadedHistorySnapshot()
    const incomingIds = new Set()

    const keepCount = getHistoryRetentionLimit(incoming.length)

    const front = []
    for (const item of incoming) {
      if (!item?.id) continue
      incomingIds.add(item.id)
      const existing = existingById.get(item.id) || {}
      front.push({
        ...existing,
        id: item.id,
        width: item.width ?? existing.width ?? 0,
        height: item.height ?? existing.height ?? 0,
        preview_png_base64: item.preview_png_base64 ?? item.previewPngBase64 ?? existing.preview_png_base64 ?? '',
        image_path: item.image_path ?? item.imagePath ?? existing.image_path ?? ''
      })
    }

    const rest = []
    for (const item of loadedItems) {
      if (!incomingIds.has(item.id)) {
        rest.push(item)
      }
    }

    const nextHistory = [...front, ...rest].slice(0, keepCount)
    history.value = nextHistory

    const loadedCount = history.value.length
    totalCount.value = Math.max(totalCount.value || 0, loadedCount)
    pageOffset.value = Math.max(pageOffset.value || 0, loadedCount)
  }

  if (!isAddingCategory.value) {
    if (data?.categories) {
      categoryMap.value = data.categories
    }
    if (data?.image_tags) {
      tagMap.value = data.image_tags
    }
    if (Array.isArray(data?.pinned_items)) {
      pinnedItems.value = data.pinned_items
    }
  }
  rebuildFilterIndexes()
  bumpFilterDataRevision()
}

const promoteLocalItemToTop = (itemId) => {
  if (!itemId || !Array.isArray(history.value) || history.value.length < 2) return
  const pinnedSet = new Set(pinnedItems.value)
  const currentIndex = history.value.findIndex((item) => item?.id === itemId)
  if (currentIndex < 0) return
  const selectedId = history.value[selectedIndex.value]?.id
  const [moved] = history.value.splice(currentIndex, 1)
  if (!moved) return
  let insertIndex = 0
  insertIndex = history.value.findIndex((item) => !pinnedSet.has(item?.id))
  if (insertIndex < 0) {
    insertIndex = history.value.length
  }
  if (insertIndex > history.value.length) {
    insertIndex = history.value.length
  }
  history.value.splice(insertIndex, 0, moved)
  if (selectedId) {
    const nextSelectedIndex = history.value.findIndex((item) => item?.id === selectedId)
    selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : 0
  } else {
    selectedIndex.value = 0
  }
  bumpFilterDataRevision()
}

const mergeImagePageIntoState = (data, reset = false) => {
  const items = Array.isArray(data?.items) ? data.items : []
  const baseOffset = Number.isFinite(data?.offset) ? Math.max(0, Number(data.offset)) : (reset ? 0 : pageOffset.value)
  if (reset) {
    clearPrefetchedPage()
    // reset 时保留已接收的窗口快照数据，避免“16条快照被分页首包覆盖成14条”。
    // 仅在确实没有任何历史时才执行清空初始化。
    if (getLoadedHistoryCount() === 0) {
      history.value = []
      categoryMap.value = {}
      tagMap.value = {}
      tagSearchIndex.clear()
      itemTagSnapshot.clear()
      categorySearchIndex.clear()
      itemCategorySnapshot.clear()
      keywordTagMatchCache.clear()
      keywordCategoryMatchCache.clear()
      pinnedItems.value = []
    }
    previewCache.clear()
    asyncPreviewCache.clear()
    warmedIndices.clear()
    warmingIndices.clear()
  }
  for (let i = 0; i < items.length; i++) {
    const item = items[i]
    if (!item) continue
    const position = baseOffset + i
    history.value[position] = {
      id: item.id,
      width: item.width,
      height: item.height,
      preview_png_base64: item.previewPngBase64,
      image_path: item.imagePath
    }
    setItemCategoryLocal(item.id, item.category || '未分类')
    setItemTagsLocal(item.id, item.tags)
  }

  // 去重：将重复 ID 设为 undefined 后，重新压缩数组，去除空洞
  const seenIds = new Set()
  const compactedHistory = []
  for (let i = 0; i < history.value.length; i++) {
    const item = history.value[i]
    if (!item) continue
    if (!seenIds.has(item.id)) {
      seenIds.add(item.id)
      compactedHistory.push(item)
    }
  }
  history.value = compactedHistory

  const pinnedSet = new Set(pinnedItems.value)
  items.forEach((row) => {
    if (row?.pinned && row.id) {
      pinnedSet.add(row.id)
    }
  })
  const loadedItems = history.value // 此时已经没有空洞了
  pinnedItems.value = loadedItems
      .filter((item) => pinnedSet.has(item.id))
      .map((item) => item.id)

  const incomingTotal = Number.isFinite(data?.total) ? Number(data.total) : loadedItems.length
  totalCount.value = Math.max(Number(totalCount.value) || 0, incomingTotal, loadedItems.length)

  const nextOffset = baseOffset + items.length
  pageOffset.value = loadedItems.length // 使用真实的 DOM 节点数量作为偏移量

  // 使用后端准确的 total 边界判断，结合真实节点数量
  if (Number.isFinite(data?.total)) {
    hasMore.value = data.total > loadedItems.length
  } else {
    hasMore.value = loadedItems.length < totalCount.value
  }

  if (Array.isArray(data?.categoryList)) {
    const list = data.categoryList.filter((c) => c !== '未分类' && c !== '全部')
    categories.value = ['未分类', ...Array.from(new Set(list))]
  }
  bumpFilterDataRevision()
}

const mergeIncrementalPageIntoState = (data) => {
  const items = Array.isArray(data?.items) ? data.items : []
  if (items.length === 0) {
    if (Number.isFinite(data?.total)) {
      const loadedCount = getLoadedHistoryCount()
      // 增量同步阶段禁止回退到更小 total，避免短暂时序差异导致已渲染列表被截断。
      totalCount.value = Math.max(Number(data.total), loadedCount, Number(totalCount.value) || 0)
      pageOffset.value = loadedCount
      hasMore.value = pageOffset.value < totalCount.value
    }
    bumpFilterDataRevision()
    return
  }
  const selectedId = history.value[selectedIndex.value]?.id
  const {loadedItems, existingById} = getLoadedHistorySnapshot()
  const incomingIds = new Set()
  const front = []
  for (const row of items) {
    if (!row?.id) continue
    incomingIds.add(row.id)
    const existing = existingById.get(row.id) || {}
    front.push({
      ...existing,
      id: row.id,
      width: row.width ?? existing.width ?? 0,
      height: row.height ?? existing.height ?? 0,
      preview_png_base64: row.previewPngBase64 ?? row.preview_png_base64 ?? existing.preview_png_base64 ?? '',
      image_path: row.imagePath ?? row.image_path ?? existing.image_path ?? ''
    })
    setItemCategoryLocal(row.id, row.category || '未分类')
    if (Array.isArray(row.tags)) {
      setItemTagsLocal(row.id, row.tags)
    } else if (!(row.id in tagMap.value)) {
      setItemTagsLocal(row.id, [])
    }
  }
  if (front.length === 0) return
  const rest = []
  for (const item of loadedItems) {
    if (!incomingIds.has(item.id)) {
      rest.push(item)
    }
  }
  // 增量同步只做“前部更新 + 其余保留”，不按 total 截断，避免出现 9 条被裁成 6 条。
  const nextHistory = [...front, ...rest]
  history.value = nextHistory

  const pinnedSet = new Set(pinnedItems.value)
  for (const row of items) {
    if (!row?.id) continue
    if (row.pinned) {
      pinnedSet.add(row.id)
    } else {
      pinnedSet.delete(row.id)
    }
  }
  pinnedItems.value = nextHistory
      .map((item) => item.id)
      .filter((id) => pinnedSet.has(id))

  const loadedCount = nextHistory.length
  totalCount.value = Number.isFinite(data?.total)
      ? Math.max(Number(data.total), loadedCount, Number(totalCount.value) || 0)
      : Math.max(totalCount.value || 0, loadedCount)
  pageOffset.value = loadedCount
  hasMore.value = pageOffset.value < totalCount.value
  if (selectedId) {
    const nextSelectedIndex = history.value.findIndex((item) => item?.id === selectedId)
    selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : 0
  } else if (selectedIndex.value < 0 && loadedCount > 0) {
    selectedIndex.value = 0
  }
  bumpFilterDataRevision()
}

const loadHistoryPage = async ({reset = false, force = false} = {}) => {
  if (isLoadingPage.value && !force) return
  isLoadingPage.value = true
  try {
    const offset = reset ? 0 : pageOffset.value
    const signature = currentPageQuerySignature()
    let data = null
    if (
        !reset
        && prefetchedPage.value
        && prefetchedPage.value.offset === offset
        && prefetchedPage.value.signature === signature
    ) {
      data = prefetchedPage.value.data
      prefetchedPage.value = null
    } else {
      if (!reset && isPrefetchingPage.value && prefetchPromise) {
        await prefetchPromise
        if (
            prefetchedPage.value
            && prefetchedPage.value.offset === offset
            && prefetchedPage.value.signature === signature
        ) {
          data = prefetchedPage.value.data
          prefetchedPage.value = null
        }
      }
      if (!data) {
        data = await ImageClipboardService.getHistoryPage({
          offset,
          limit: pageSize.value,
          category: categoryFilter.value === '全部' ? null : categoryFilter.value,
          keyword: searchKeyword.value.trim() || null,
          pinnedOnly: false,
          sortBy: sortBy.value,
          sortOrder: sortOrder.value
        })
      }
    }
    mergeImagePageIntoState(data, reset)
    if (selectedIndex.value < 0 || !history.value[selectedIndex.value]) {
      const firstLoaded = filteredHistory.value[0]
      selectedIndex.value = firstLoaded ? firstLoaded.index : -1
    }
    await nextTick()
    if (hasMore.value) {
      void prefetchNextPage()
    }
  } catch (error) {
    console.error('同步图片历史失败:', error)
    if (reset && getLoadedHistoryCount() === 0 && !initialPageRetryTimer) {
      initialPageRetryTimer = setTimeout(() => {
        initialPageRetryTimer = null
        loadHistoryPage({reset: true}).catch(() => {
        })
      }, 800)
    }
  } finally {
    isLoadingPage.value = false
    flushPendingHistorySync()
  }
}

const ensureInitialPageLoaded = (force = false) => {
  if (isLoadingPage.value && !force) return
  if (!force && getLoadedHistoryCount() > 0) return
  loadHistoryPage({reset: true, force}).catch((error) => {
    console.error('初始化图片历史失败:', error)
  })
}

const waitForFirstPaint = () =>
    new Promise((resolve) => {
      requestAnimationFrame(() => resolve())
    })

const syncHistory = async () => {
  clearPrefetchedPage()
  pageOffset.value = 0
  totalCount.value = 0
  hasMore.value = false
  loadMoreIntent.value = false
  await loadHistoryPage({reset: true})
}

const syncHistoryIncremental = async () => {
  const data = await ImageClipboardService.getHistoryPage({
    offset: 0,
    limit: Math.max(Number(pageSize.value) || 10, 30),
    category: categoryFilter.value === '全部' ? null : categoryFilter.value,
    keyword: searchKeyword.value.trim() || null,
    pinnedOnly: false,
    sortBy: sortBy.value,
    sortOrder: sortOrder.value
  })
  mergeIncrementalPageIntoState(data)
  await nextTick()
}

const scheduleHistorySync = (delay = 220) => {
  if (historyUpdateTimer) return
  historyUpdateTimer = window.setTimeout(async () => {
    historyUpdateTimer = null
    if (isLoadingPage.value) {
      pendingHistorySync = true
      return
    }
    try {
      await syncHistoryIncremental()
    } catch (error) {
      console.error('增量同步图片历史失败，回退全量同步:', error)
      await syncHistory()
    }
  }, delay)
}

const handleKeydown = async (event) => {
  if (!isVisible.value) return
  if (isInputLikeTarget(event.target)) return

  const visible = filteredHistory.value
  if (visible.length === 0) return
  let currentVisibleIndex = visible.findIndex((entry) => entry.index === selectedIndex.value)
  if (currentVisibleIndex < 0) currentVisibleIndex = 0
  if (event.key === 'ArrowLeft') {
    event.preventDefault()
    currentVisibleIndex = Math.max(0, currentVisibleIndex - 1)
    selectedIndex.value = visible[currentVisibleIndex].index
    await ensureKeyboardSelectionVisible()
  } else if (event.key === 'ArrowRight') {
    event.preventDefault()
    currentVisibleIndex = Math.min(visible.length - 1, currentVisibleIndex + 1)
    selectedIndex.value = visible[currentVisibleIndex].index
    handleLoadMoreIntent()
    await tryLoadMoreByScroll()
    await ensureKeyboardSelectionVisible()
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
      // 快照先并入列表，保证首屏与当前内存态一致，再由分页接口校正。
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

    // 检测是否为清理操作：如果 payload 包含 history 数组，说明是清理后的状态通知
    const hasHistoryArray = Array.isArray(payload.history)

    // 清理操作必须执行完整同步，不受数据量限制
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

    // 非清理操作的正常流程
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
  unlistenItemPromoted = await listen('image-item-pinned', (event) => {
    const itemId = event?.payload?.itemId
    const pinned = event?.payload?.pinned !== false
    if (pinned) {
      promoteLocalItemToTop(itemId)
    } else {
      demoteLocalItemFromTop(itemId)
    }
  })
  unlistenWritebackResult = await listen('writeback-result', (event) => {
    const payload = event.payload || {}
    if (payload.source !== '图片') return
    if (payload.success) {
      const target = payload.targetWindowTitle ? `：${payload.targetWindowTitle}` : ''
      ElMessage.success(`图片回填成功${target}`)
    } else {
      ElMessage.error(`图片回填失败：${String(payload.detail || '未知错误')}`)
    }
  })

  // 监听预览就绪事件
  unlistenPreviewReady = await listen('preview-ready', (event) => {
    const {itemId, previewUrl} = event.payload || {}
    if (itemId && previewUrl) {
      // 更新缓存
      asyncPreviewCache.set(itemId, previewUrl)
      enforceAsyncPreviewCacheSize()
      previewCache.set(itemId, previewUrl)
      enforcePreviewCacheSize()

      // 关键修复：直接更新 history 数组中的项，触发 Vue 响应式更新
      const itemIndex = history.value.findIndex(item => item?.id === itemId)
      if (itemIndex >= 0) {
        // 从 previewUrl 中提取 base64 部分
        const base64 = previewUrl.replace('data:image/png;base64,', '')
        // 直接在对象上更新，避免完整替换对象导致的不必要重渲染
        history.value[itemIndex].preview_png_base64 = base64
      }

    }
  })
})

onBeforeUnmount(() => {
  flushPendingHistorySync()
  previewCache.clear()
  asyncPreviewCache.clear() // 清理异步预览缓存
  if (filterDebounceTimer) {
    clearTimeout(filterDebounceTimer)
    filterDebounceTimer = null
  }
  if (historyUpdateTimer) {
    clearTimeout(historyUpdateTimer)
    historyUpdateTimer = null
  }
  if (initialPageRetryTimer) {
    clearTimeout(initialPageRetryTimer)
    initialPageRetryTimer = null
  }
  if (warmupBatchTimer) {
    clearTimeout(warmupBatchTimer)
    warmupBatchTimer = null
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
  if (unlistenItemPromoted) {
    unlistenItemPromoted()
    unlistenItemPromoted = null
  }
  if (unlistenPreviewReady) {
    unlistenPreviewReady()
    unlistenPreviewReady = null
  }
  if (unlistenWritebackResult) {
    unlistenWritebackResult()
    unlistenWritebackResult = null
  }
  window.removeEventListener('keydown', handleWindowKeydown)
  window.removeEventListener('keyup', handleWindowKeyup)
  window.removeEventListener('blur', handleWindowBlur)
})

watch(selectedIndex, (value) => {
  warmupOne(value)
})

watch(filteredHistory, (list) => {
  if (!Array.isArray(list) || list.length === 0) {
    selectedIndex.value = -1
    return
  }
  const exists = list.some((entry) => entry.index === selectedIndex.value)
  if (!exists) {
    selectedIndex.value = list[0].index
  }
})

watch(previewCacheKeepIds, (ids) => {
  prunePreviewCache(ids)
  pruneAsyncPreviewCache(ids)
}, {immediate: true})

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

<style>
@import "../shared/windowBase.css";
@import "../shared/contextMenu.css";
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
  display: flex;
  justify-content: center;
  align-items: center;
  height: 100%;
  color: #fff;
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
  max-width: calc(100% - 90px);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-variant-numeric: tabular-nums;
}

.status-meta {
  flex: 0 1 auto;
  min-width: 0;
  color: rgba(166, 213, 255, 0.88);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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

</style>
