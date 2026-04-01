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

    <div
        v-else
        ref="contentRef"
        class="content"
        @mousedown="handleContentMouseDown"
        @scroll="handleContentScroll"
        @wheel.prevent="handleContentWheel"
    >
      <div ref="leadingSpacerRef" class="virtual-spacer"></div>
      <div
          v-for="entry in renderedHistory"
          :id="`image-item-${entry.index}`"
          :key="entry.item.id"
          :class="{ selected: selectedIndex === entry.index }"
          class="clipboard-item"
          :draggable="isCtrlKeyPressed"
          @click="selectByIndex(entry.index)"
          @dblclick="fillById(entry.item.id)"
          @dragend="handleDragEnd"
          @dragstart="handleDragStart($event, entry.item.id)"
          @mouseenter="handleItemHover(entry.index)"
          @contextmenu.prevent="showContextMenu($event, entry.item.id)"
      >
        <div class="delete-btn" @click.stop="deleteItem(entry.item.id, entry.index)">
          <el-icon>
            <Close/>
          </el-icon>
        </div>
        <button class="fullscreen-btn" title="全屏预览" @click.stop="openFullscreen(entry.item.id)">
          <el-icon>
            <FullScreen/>
          </el-icon>
        </button>
        <button :class="{ active: isPinned(entry.item.id) }" class="pin-btn" title="置顶"
                @click.stop="promoteImageItem(entry.item.id)">
          <Pin class="pin-lucide"/>
        </button>
        <div class="index-tools">
          <div class="index">{{ entry.index + 1 }}</div>
        </div>
        <div class="category-wrap">
          <div class="category-chip">{{ getItemCategory(entry.item.id) }}</div>
        </div>
        <div class="tag-wrap">
          <div v-if="getItemTags(entry.item.id).length" class="tag-chip-list">
            <span v-for="tag in getItemTags(entry.item.id)" :key="`${entry.item.id}-${tag}`" class="tag-chip">#{{
                tag
              }}</span>
          </div>
          <div v-else class="tag-chip-empty">无标签</div>
        </div>
        <div class="item-content">
          <img :src="getPreviewDataUrl(entry.item)" alt="" class="image-preview" decoding="async" draggable="false"
               loading="lazy" @dragstart.prevent/>
          <div class="image-meta">{{ entry.item.width }} × {{ entry.item.height }}</div>
        </div>
      </div>
      <div v-if="showTailLoadMoreHint" class="load-more-tail-indicator">
        <el-icon v-if="isLoadingMore" class="load-more-tail-spinner is-loading">
          <Loading/>
        </el-icon>
        <div class="load-more-tail-text">
          <span>左滑</span>
          <span>{{ isLoadingMore ? '加载中' : '加载更多' }}</span>
        </div>
      </div>
      <div ref="trailingSpacerRef" class="virtual-spacer"></div>
    </div>

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
import {computed, nextTick, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {ArrowLeftBold, ArrowRightBold, Check, Close, FullScreen, Loading} from '@element-plus/icons-vue'
import {Pin} from 'lucide-vue-next'
import {listen} from '@tauri-apps/api/event'
import {convertFileSrc} from '@tauri-apps/api/core'
import {ElMessage, ElMessageBox} from 'element-plus'
import {ImageCategoryService, ImageClipboardService, WindowService} from '../../services/ipc'
import ClipboardToolbar from '../clipboard/components/ClipboardToolbar.vue'
import {useWindowOffset} from '../clipboard/composables/useWindowOffset'
import {useContextMenuState} from '../shared/useContextMenuState'
import {runCategoryAssignment} from '../shared/categoryActions'

const containerRef = ref(null)
const contentRef = ref(null)
const contextMenuRef = ref(null)
const leadingSpacerRef = ref(null)
const trailingSpacerRef = ref(null)
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

const syncVirtualSpacerWidths = () => {
  if (leadingSpacerRef.value) {
    leadingSpacerRef.value.style.width = `${leadingSpacerWidth.value}px`
  }
  if (trailingSpacerRef.value) {
    trailingSpacerRef.value.style.width = `${trailingSpacerWidth.value}px`
  }
}
const dragItemId = ref('')
const isFilling = ref(false)
const categoryInputOpenedAt = ref(0)
const previewCache = new Map()
const asyncPreviewCache = new Map()
const warmedIndices = new Set()
const warmingIndices = new Set()
let unlistenShowWindow = null
let unlistenItemPromoted = null
let unlistenHistoryPayloadUpdated = null
let unlistenHistoryItemAdded = null
let unlistenPreviewReady = null
let isPointerDown = false
let isContentDragging = false
let pendingHistorySync = false
let dragStartX = 0
let dragStartScrollLeft = 0
let dragTargetScrollLeft = 0
let dragScrollRafId = 0
let contentMetricsRafId = 0
let loadMorePending = false
let historyUpdateTimer = null
let initialPageRetryTimer = null
const isCtrlKeyPressed = ref(false)
const contentScrollLeft = ref(0)
const contentViewportWidth = ref(0)
const loadMoreIntent = ref(false)
const prefetchedPage = ref(null)
const isPrefetchingPage = ref(false)
let prefetchRequestSeq = 0
let prefetchPromise = null

const IMAGE_ITEM_WIDTH = 250
const IMAGE_ITEM_GAP = 8
const IMAGE_ITEM_UNIT = IMAGE_ITEM_WIDTH + IMAGE_ITEM_GAP
const IMAGE_TAIL_SPACER = 742
const IMAGE_VIRTUAL_OVERSCAN = 4
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

const stopContentDragging = () => {
  isPointerDown = false
  isContentDragging = false
  if (dragScrollRafId) {
    cancelAnimationFrame(dragScrollRafId)
    dragScrollRafId = 0
  }
  if (contentMetricsRafId) {
    cancelAnimationFrame(contentMetricsRafId)
    contentMetricsRafId = 0
  }
  loadMorePending = false
  if (contentRef.value) {
    contentRef.value.classList.remove('is-dragging')
  }
  if (contentRef.value) {
    contentRef.value.style.cursor = 'default'
  }
  document.body.style.removeProperty('user-select')
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
  if (pendingHistorySync) {
    pendingHistorySync = false
    scheduleHistorySync(0)
  }
}

const handleGlobalMouseMove = (event) => {
  if (!isPointerDown || !contentRef.value) return
  const delta = event.pageX - dragStartX
  if (!isContentDragging && Math.abs(delta) > 4) {
    isContentDragging = true
    contentRef.value.style.cursor = 'grabbing'
    contentRef.value.classList.add('is-dragging')
    document.body.style.userSelect = 'none'
  }
  if (isContentDragging) {
    dragTargetScrollLeft = dragStartScrollLeft - delta
    const maxScrollLeft = Math.max(0, contentRef.value.scrollWidth - contentRef.value.clientWidth)
    if (dragTargetScrollLeft > maxScrollLeft + 36 && hasMore.value && !isLoadingPage.value) {
      loadMoreIntent.value = true
    }
    if (!dragScrollRafId) {
      dragScrollRafId = requestAnimationFrame(() => {
        dragScrollRafId = 0
        if (contentRef.value) {
          contentRef.value.scrollLeft = dragTargetScrollLeft
        }
      })
    }
  }
}

const handleGlobalMouseUp = () => {
  stopContentDragging()
}

const handleContentMouseDown = (event) => {
  if (event.button !== 0) return
  if (event.target.closest('.delete-btn') || event.target.closest('.fullscreen-btn') || event.target.closest('.pin-btn')) {
    return
  }
  if (!contentRef.value) return
  isPointerDown = true
  isContentDragging = false
  dragStartX = event.pageX
  dragStartScrollLeft = contentRef.value.scrollLeft
  dragTargetScrollLeft = dragStartScrollLeft
  window.addEventListener('mousemove', handleGlobalMouseMove)
  window.addEventListener('mouseup', handleGlobalMouseUp, true)
}

const handleContainerMouseDown = (event) => {
  if (event.button !== 0) return
  const target = event.target
  if (isAddingCategory.value && target instanceof Element && !target.closest('.category-input')) {
    cancelCreateCategory()
  }
}

const syncContentMetrics = () => {
  if (!contentRef.value) return
  contentScrollLeft.value = contentRef.value.scrollLeft
  contentViewportWidth.value = contentRef.value.clientWidth
}

const handleContentWheel = (event) => {
  if (!contentRef.value) return
  const delta = Math.abs(event.deltaY) >= Math.abs(event.deltaX) ? event.deltaY : event.deltaX
  const maxScrollLeft = Math.max(0, contentRef.value.scrollWidth - contentRef.value.clientWidth)
  const nearEnd = contentRef.value.scrollLeft >= maxScrollLeft - 8
  if (delta > 0 && nearEnd && hasMore.value && !isLoadingPage.value) {
    loadMoreIntent.value = true
  }
  contentRef.value.scrollLeft += delta
  void tryLoadMoreByScroll()
}

const handleContentScroll = () => {
  if (contentMetricsRafId) return
  contentMetricsRafId = requestAnimationFrame(() => {
    contentMetricsRafId = 0
    syncContentMetrics()
    if (!loadMorePending) {
      loadMorePending = true
      Promise.resolve(tryLoadMoreByScroll()).finally(() => {
        loadMorePending = false
      })
    }
  })

  // 优化方案 5：滚动时批量预热前方可见区域
  const scrollLeft = contentRef.value?.scrollLeft || 0
  const visibleStartIndex = Math.floor(scrollLeft / IMAGE_ITEM_UNIT)
  warmupBatch(visibleStartIndex + 3, 6) // 预热前方第 3-8 个图片
}

const tryLoadMoreByScroll = async () => {
  if (!hasMore.value || isLoadingPage.value || !contentRef.value) return
  const remaining = contentRef.value.scrollWidth - contentRef.value.clientWidth - contentRef.value.scrollLeft
  if (remaining <= 240 && loadMoreIntent.value) {
    loadMoreIntent.value = false
    await loadHistoryPage({reset: false})
  }
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

const isPinned = (itemId) => pinnedItems.value.includes(itemId)

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
      out.push({item, index})
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

const virtualRange = computed(() => {
  const total = filteredHistoryState.value.total
  if (total === 0) {
    return {start: 0, end: 0}
  }
  const viewport = Math.max(contentViewportWidth.value, IMAGE_ITEM_UNIT)
  const scroll = Math.max(0, contentScrollLeft.value)
  const start = Math.max(0, Math.floor(scroll / IMAGE_ITEM_UNIT) - IMAGE_VIRTUAL_OVERSCAN)
  const visibleCount = Math.ceil(viewport / IMAGE_ITEM_UNIT) + IMAGE_VIRTUAL_OVERSCAN * 2
  const end = Math.min(total, start + visibleCount)
  return {start, end}
})

const renderedHistory = computed(() => {
  const {start, end} = virtualRange.value
  return filteredHistory.value.slice(start, end)
})

const previewCacheKeepIds = computed(() => {
  const total = filteredHistoryState.value.total
  if (total === 0) return []
  const start = Math.max(0, virtualRange.value.start - IMAGE_PREVIEW_CACHE_MARGIN)
  const end = Math.min(total, virtualRange.value.end + IMAGE_PREVIEW_CACHE_MARGIN)
  const ids = filteredHistory.value
      .slice(start, end)
      .map((entry) => entry.item?.id)
      .filter(Boolean)
  const selectedId = history.value[selectedIndex.value]?.id
  if (selectedId) {
    ids.push(selectedId)
  }
  return Array.from(new Set(ids))
})

const leadingSpacerWidth = computed(() => virtualRange.value.start * IMAGE_ITEM_UNIT)

const trailingSpacerWidth = computed(() => {
  const total = filteredHistoryState.value.total
  const trailing = Math.max(0, total - virtualRange.value.end) * IMAGE_ITEM_UNIT
  return trailing + IMAGE_TAIL_SPACER
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

const isLoadingMore = computed(() => isLoadingPage.value && filteredHistoryState.value.total > 0)

const showTailLoadMoreHint = computed(() => {
  if (!(hasMore.value || isLoadingMore.value) || filteredHistoryState.value.total === 0) return false
  return virtualRange.value.end >= filteredHistoryState.value.total
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
  const keepCount = Math.max(100, Number(pageOffset.value) || 100, Number(pageSize.value) || 100, history.value.length)
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

const warmupOne = (index) => {
  if (index < 0 || index >= history.value.length) return
  if (warmedIndices.has(index) || warmingIndices.has(index)) return
  warmingIndices.add(index)
  const itemId = history.value[index]?.id
  if (!itemId) {
    warmingIndices.delete(index)
    return
  }
  const warmupTask = ImageClipboardService.warmupItemById(itemId)
  warmupTask
      .then(() => {
        warmedIndices.add(index)
      })
      .catch(() => {
      })
      .finally(() => {
        warmingIndices.delete(index)
      })
}

// 优化方案 5：批量预热前方多个图片
let warmupBatchTimer = null
const warmupBatch = (startIndex, count = 6) => {
  if (warmupBatchTimer) {
    clearTimeout(warmupBatchTimer)
  }
  warmupBatchTimer = setTimeout(async () => {
    const itemIds = []
    for (let i = startIndex; i < Math.min(startIndex + count, history.value.length); i++) {
      const itemId = history.value[i]?.id
      if (itemId && !warmedIndices.has(i) && !warmingIndices.has(i)) {
        itemIds.push(itemId)
      }
    }
    if (itemIds.length > 0) {
      try {
        await ImageClipboardService.warmupMultipleItems(itemIds)
        for (let i = startIndex; i < Math.min(startIndex + count, history.value.length); i++) {
          warmedIndices.add(i)
        }
      } catch (error) {
        console.error('批量预热失败:', error)
      }
    }
  }, 100) // 100ms 防抖
}

const warmupAround = (index) => {
  warmupOne(index - 1)
  warmupOne(index)
  warmupOne(index + 1)
}

const handleItemHover = (index) => {
  if (isPointerDown || isContentDragging) return
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
  if (contentRef.value) {
    contentRef.value.scrollLeft = Math.max(0, contentRef.value.scrollWidth - contentRef.value.clientWidth)
  }
  loadMoreIntent.value = true
  await tryLoadMoreByScroll()
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
  try {
    if (itemId) {
      previewCache.delete(itemId)
      asyncPreviewCache.delete(itemId)
      removeItemCategoryLocal(itemId)
      removeItemTagsLocal(itemId)
    }
    if (Number.isInteger(index) && index >= 0 && index < history.value.length) {
      history.value.splice(index, 1)
      if (selectedIndex.value >= history.value.length) {
        selectedIndex.value = Math.max(0, history.value.length - 1)
      }
      bumpFilterDataRevision()
    }
    if (!itemId) return
    await ImageClipboardService.removeItemById(itemId)
    await syncHistory()
  } catch (error) {
    console.error('删除图片记录失败:', error)
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
  if (!isCtrlKeyPressed.value || isContentDragging) {
    event.preventDefault()
    return
  }
  stopContentDragging()
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
  totalCount.value = history.value.filter(Boolean).length
  pageOffset.value = totalCount.value
  hasMore.value = false
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
  if (incoming.length > 0) {
    const existingById = new Map(history.value.filter(Boolean).map((item) => [item.id, item]))
    const incomingIds = new Set()

    // 修复：使用更大的保留数量，确保所有图片都被保留
    const keepCount = Math.max(100, Number(pageOffset.value) || 100, Number(pageSize.value) || 100, history.value.length + incoming.length)

    // 修复：正确处理所有传入的图片，而不是只处理第一张
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

    // 保留现有历史记录中不在传入数据中的项目
    const rest = history.value.filter((item) => item && !incomingIds.has(item.id))

    // 合并并限制总数
    history.value = [...front, ...rest].slice(0, keepCount)
    const loadedCount = history.value.filter(Boolean).length
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
  const pinnedSet = new Set(pinnedItems.value)
  items.forEach((row) => {
    if (row?.pinned && row.id) {
      pinnedSet.add(row.id)
    }
  })
  const loadedItems = history.value.filter(Boolean)
  pinnedItems.value = loadedItems
      .filter((item) => pinnedSet.has(item.id))
      .map((item) => item.id)
  totalCount.value = Number.isFinite(data?.total) ? data.total : loadedItems.length
  const nextOffset = (reset ? 0 : pageOffset.value) + items.length
  pageOffset.value = nextOffset
  hasMore.value = nextOffset < totalCount.value
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
      totalCount.value = data.total
      const loadedCount = history.value.filter(Boolean).length
      if (loadedCount > totalCount.value) {
        history.value = history.value.slice(0, Math.max(0, totalCount.value))
      }
      pageOffset.value = history.value.filter(Boolean).length
      hasMore.value = pageOffset.value < totalCount.value
    }
    bumpFilterDataRevision()
    return
  }
  const selectedId = history.value[selectedIndex.value]?.id
  const incomingIds = new Set()
  const existingById = new Map(history.value.filter(Boolean).map((item) => [item.id, item]))
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
  const rest = history.value.filter((item) => item && !incomingIds.has(item.id))
  const loadedCountBefore = history.value.filter(Boolean).length
  const keepCount = Math.max(loadedCountBefore, Number(pageSize.value) || 10, front.length)
  const expectedTotal = Number.isFinite(data?.total) ? Math.max(0, Number(data.total)) : loadedCountBefore
  const maxCount = expectedTotal > 0 ? Math.min(keepCount, expectedTotal) : keepCount
  history.value = [...front, ...rest].slice(0, maxCount)

  const pinnedSet = new Set(pinnedItems.value)
  for (const row of items) {
    if (!row?.id) continue
    if (row.pinned) {
      pinnedSet.add(row.id)
    } else {
      pinnedSet.delete(row.id)
    }
  }
  pinnedItems.value = history.value
      .filter(Boolean)
      .map((item) => item.id)
      .filter((id) => pinnedSet.has(id))

  const loadedCount = history.value.filter(Boolean).length
  totalCount.value = Number.isFinite(data?.total) ? data.total : Math.max(totalCount.value || 0, loadedCount)
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
    syncContentMetrics()
    if (hasMore.value) {
      void prefetchNextPage()
    }
  } catch (error) {
    console.error('同步图片历史失败:', error)
    if (reset && history.value.filter(Boolean).length === 0 && !initialPageRetryTimer) {
      initialPageRetryTimer = setTimeout(() => {
        initialPageRetryTimer = null
        loadHistoryPage({reset: true}).catch(() => {
        })
      }, 800)
    }
  } finally {
    isLoadingPage.value = false
    if (pendingHistorySync && !isPointerDown && !isContentDragging) {
      pendingHistorySync = false
      scheduleHistorySync(0)
    }
  }
}

const ensureInitialPageLoaded = (force = false) => {
  if (isLoadingPage.value && !force) return
  if (history.value.filter(Boolean).length > 0) return
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
  syncContentMetrics()
}

const scheduleHistorySync = (delay = 220) => {
  if (historyUpdateTimer) return
  historyUpdateTimer = window.setTimeout(async () => {
    historyUpdateTimer = null
    if (isPointerDown || isContentDragging || isLoadingPage.value) {
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
    loadMoreIntent.value = true
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

onMounted(async () => {
  window.addEventListener('keydown', handleWindowKeydown)
  window.addEventListener('keyup', handleWindowKeyup)
  window.addEventListener('blur', handleWindowBlur)
  window.addEventListener('resize', syncContentMetrics)
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
      applyPayload(payload, {refocus: true})
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
    if (shouldDeferHeavyPayloadApply(payload)) {
      scheduleHistorySync(0)
      return
    }
    applyPayload(payload)
  })
  unlistenHistoryItemAdded = await listen('image-history-item-added', (event) => {
    if (isAddingCategory.value) return
    mergeIncrementalImageItem(event?.payload?.item)
  })
  unlistenItemPromoted = await listen('image-item-promoted', (event) => {
    const itemId = event?.payload?.itemId
    promoteLocalItemToTop(itemId)
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
        // 使用展开运算符创建新对象，确保 Vue 检测到变化
        history.value[itemIndex] = {
          ...history.value[itemIndex],
          preview_png_base64: base64
        }
      }
      
    }
  })
})

onBeforeUnmount(() => {
  stopContentDragging()
  if (contentMetricsRafId) {
    cancelAnimationFrame(contentMetricsRafId)
    contentMetricsRafId = 0
  }
  loadMorePending = false
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
  window.removeEventListener('keydown', handleWindowKeydown)
  window.removeEventListener('keyup', handleWindowKeyup)
  window.removeEventListener('blur', handleWindowBlur)
  window.removeEventListener('resize', syncContentMetrics)
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
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

watch([leadingSpacerWidth, trailingSpacerWidth], async () => {
  await nextTick()
  syncVirtualSpacerWidths()
}, {immediate: true})

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

.content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  gap: 8px;
  padding: 8px;
  flex-direction: row;
  overflow-x: auto;
  overflow-y: hidden;
  margin-top: 10px;
  scrollbar-width: none;
}

.content::-webkit-scrollbar {
  display: none;
}

.content.is-dragging .clipboard-item {
  transition: none !important;
  backdrop-filter: none !important;
  -webkit-backdrop-filter: none !important;
}

.content.is-dragging .clipboard-item:hover,
.content.is-dragging .clipboard-item.selected {
  box-shadow: none !important;
}

.content.is-dragging .clipboard-item.selected {
  transform: none !important;
}

.content.is-dragging .delete-btn,
.content.is-dragging .fullscreen-btn,
.content.is-dragging .pin-btn {
  opacity: 0 !important;
}

.content.is-dragging .clipboard-item {
  pointer-events: none;
}

.virtual-spacer {
  flex: 0 0 auto;
  height: 1px;
}

.load-more-tail-indicator {
  width: 56px;
  flex: 0 0 56px;
  min-height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: rgba(166, 213, 255, 0.9);
  user-select: none;
  pointer-events: none;
}

.load-more-tail-text {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  letter-spacing: 0.5px;
  line-height: 1;
}

.load-more-tail-spinner {
  font-size: 16px;
  color: rgba(220, 240, 255, 0.95);
}

.clipboard-item {
  background: rgba(0, 0, 0, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  padding: 12px;
  cursor: pointer;
  position: relative;
  user-select: none;
  width: 250px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  backdrop-filter: blur(10px);
  color: white;
  transition: all 0.3s ease;
  box-sizing: border-box;
}

.clipboard-item:hover, .clipboard-item.selected {
  background: rgba(0, 0, 0, 0.8);
  border-color: var(--el-color-primary, #409eff);
  box-shadow: 0 0 15px rgba(64, 158, 255, 0.5);
}

.clipboard-item.selected {
  transform: scale(1.02);
}

.delete-btn {
  position: absolute;
  top: 5px;
  right: 5px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.2);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.2s;
  z-index: 10;
}

.delete-btn .el-icon {
  font-size: 12px;
}

.clipboard-item:hover .delete-btn {
  opacity: 1;
}

.delete-btn:hover {
  background: #f56c6c;
}

.fullscreen-btn {
  position: absolute;
  top: 5px;
  right: 29px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 1px solid rgba(255, 255, 255, 0.22);
  background: rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.75);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.2s, border-color 0.2s, color 0.2s, background-color 0.2s;
  z-index: 10;
  padding: 0;
}

.pin-btn {
  position: absolute;
  top: 5px;
  right: 53px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 1px solid rgba(255, 255, 255, 0.22);
  background: rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.75);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.2s, border-color 0.2s, color 0.2s, background-color 0.2s;
  z-index: 10;
  padding: 0;
}

.pin-lucide {
  width: 12px;
  height: 12px;
  stroke-width: 2;
}

.pin-btn:hover {
  border-color: var(--el-color-primary, #409eff);
  color: #fff;
  background: var(--el-color-primary, #409eff);
}

.pin-btn.active {
  opacity: 1;
  border-color: #f7b955;
  color: #fff;
  background: rgba(247, 185, 85, 0.75);
}

.fullscreen-btn:hover {
  border-color: var(--el-color-primary, #409eff);
  color: #fff;
  background: var(--el-color-primary, #409eff);
}

.clipboard-item:hover .fullscreen-btn {
  opacity: 1;
}

.clipboard-item:hover .pin-btn {
  opacity: 1;
}

.index-tools {
  position: absolute;
  top: 5px;
  left: 5px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  z-index: 10;
}

.index {
  background: rgba(255, 255, 255, 0.1);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
  color: #909399;
}

.clipboard-item:hover .index, .clipboard-item.selected .index {
  background: var(--el-color-primary, #409eff);
  color: #fff;
}

.category-wrap {
  position: absolute;
  left: 36px;
  right: 62px;
  top: 5px;
  display: flex;
  justify-content: center;
  z-index: 10;
  pointer-events: none;
}

.category-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  max-width: 100%;
  padding: 4px 10px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.85);
  font-size: 12px;
  text-align: center;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tag-wrap {
  position: absolute;
  left: 10px;
  right: 10px;
  top: 32px;
  min-height: 20px;
  display: flex;
  align-items: center;
  z-index: 8;
}

.tag-chip-list {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
}

.tag-chip {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 999px;
  font-size: 11px;
  color: #d9ecff;
  background: rgba(64, 158, 255, 0.2);
  border: 1px solid rgba(64, 158, 255, 0.45);
}

.tag-chip-empty {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.45);
}

.item-content {
  margin-top: 56px;
  flex: 1;
  min-height: 0;
  position: relative;
  z-index: 1;
}

.image-preview {
  width: 100%;
  height: 100%;
  object-fit: contain;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.45);
}

.image-meta {
  position: absolute;
  right: 8px;
  bottom: 6px;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
  color: #dcdfe6;
  background: rgba(0, 0, 0, 0.45);
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
