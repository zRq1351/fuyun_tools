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
      <div :style="{ width: `${leadingSpacerWidth}px` }" class="virtual-spacer"></div>
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
      <div :style="{ width: `${trailingSpacerWidth}px` }" class="virtual-spacer"></div>
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
        :style="{ top: contextMenuY + 'px', left: contextMenuX + 'px' }"
        class="context-menu"
        @click.stop
    >
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
      <div class="context-menu-divider"></div>
      <div class="context-menu-item" @click="editItemTags">
        编辑标签
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

const containerRef = ref(null)
const contentRef = ref(null)
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
const sortOrder = ref('asc')
const isVisible = ref(false)
const isAddingCategory = ref(false)
const newCategoryName = ref('')
const newCategoryInputRef = ref(null)
const contextMenuVisible = ref(false)
const contextMenuX = ref(0)
const contextMenuY = ref(0)
const contextMenuItemId = ref('')
const dragItemId = ref('')
const isFilling = ref(false)
const categoryInputOpenedAt = ref(0)
const previewCache = new Map()
const warmedIndices = new Set()
const warmingIndices = new Set()
let unlistenShowWindow = null
let unlistenItemPromoted = null
let unlistenHistoryPayloadUpdated = null
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

const currentPageQuerySignature = () =>
    JSON.stringify({
      pageSize: pageSize.value,
      category: categoryFilter.value === '全部' ? null : categoryFilter.value,
      keyword: searchKeyword.value.trim() || null,
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

const filteredHistory = computed(() => {
  return history.value
      .map((item, index) => ({item, index}))
      .filter((entry) => Boolean(entry.item))
})

const virtualRange = computed(() => {
  const total = filteredHistory.value.length
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
  const total = filteredHistory.value.length
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
  const total = filteredHistory.value.length
  const trailing = Math.max(0, total - virtualRange.value.end) * IMAGE_ITEM_UNIT
  return trailing + IMAGE_TAIL_SPACER
})

const selectedStatusText = computed(() => {
  const total = totalCount.value || filteredHistory.value.length
  if (total === 0) return '当前无选中项'
  const current = filteredHistory.value.findIndex((entry) => entry.index === selectedIndex.value)
  const display = current >= 0 ? current + 1 : 1
  return `当前选中：第 ${display} / ${total} 条`
})

const loadStatusText = computed(() => {
  if (isLoadingPage.value) return '正在加载...'
  if (hasMore.value) return `已加载 ${filteredHistory.value.length} / ${totalCount.value || filteredHistory.value.length}`
  return `已全部加载 ${filteredHistory.value.length} 条`
})

const isLoadingMore = computed(() => isLoadingPage.value && filteredHistory.value.length > 0)

const showTailLoadMoreHint = computed(() => {
  if (!(hasMore.value || isLoadingMore.value) || filteredHistory.value.length === 0) return false
  return virtualRange.value.end >= filteredHistory.value.length
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

// 异步预览相关的状态
const asyncPreviewCache = new Map()
const pendingPreviewIds = new Set()
let previewPollInterval = null

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

// 异步预览轮询
const startPreviewPolling = () => {
  if (previewPollInterval) return
  previewPollInterval = setInterval(async () => {
    if (pendingPreviewIds.size === 0) return

    const idsToCheck = Array.from(pendingPreviewIds)
    try {
      const results = await ImageClipboardService.checkPreviewsReady(idsToCheck)

      for (const [itemId, ready] of results) {
        if (ready) {
          pendingPreviewIds.delete(itemId)
          // 获取预览数据
          try {
            const previewData = await ImageClipboardService.getImagePreviewById(itemId)
            if (previewData) {
              const [width, height, base64] = previewData
              const previewUrl = `data:image/png;base64,${base64}`
              asyncPreviewCache.set(itemId, previewUrl)
              previewCache.set(itemId, previewUrl)
              enforcePreviewCacheSize()
            }
          } catch (error) {
            console.error('获取异步预览失败:', error)
          }
        }
      }
    } catch (error) {
      console.error('检查预览状态失败:', error)
    }
  }, 500) // 每500ms检查一次
}

const stopPreviewPolling = () => {
  if (previewPollInterval) {
    clearInterval(previewPollInterval)
    previewPollInterval = null
  }
}

const getPreviewDataUrl = (item) => {
  if (previewCache.has(item.id)) {
    return previewCache.get(item.id)
  }
  try {
    // 优化：对于大图片，直接使用文件路径而不是 Base64
    // 这样可以避免大量的 Base64 解码和编码操作
    const isLargeImage = item.width > 1920 || item.height > 1080

    if (isLargeImage) {
      // 大图片：检查是否有异步生成的预览
      if (asyncPreviewCache.has(item.id)) {
        const previewUrl = asyncPreviewCache.get(item.id)
        previewCache.set(item.id, previewUrl)
        enforcePreviewCacheSize()
        return previewUrl
      }

      // 如果没有异步预览，添加到待检查列表
      if (!pendingPreviewIds.has(item.id)) {
        pendingPreviewIds.add(item.id)
        startPreviewPolling()
      }

      // 暂时使用文件路径作为占位
      const previewUrl = buildFileUrlFromPath(item.image_path)
      previewCache.set(item.id, previewUrl)
      enforcePreviewCacheSize()
      return previewUrl
    }

    // 小图片：使用 Base64 预览
    const lowresBase64 = typeof item.preview_png_base64 === 'string' ? item.preview_png_base64.trim() : ''
    const rgbaBase64 = typeof item.preview_rgba_base64 === 'string' ? item.preview_rgba_base64.trim() : ''
    const previewUrl = lowresBase64
        ? `data:image/png;base64,${lowresBase64}`
        : rgbaBase64ToPngDataUrl(rgbaBase64, Number(item.preview_width) || 0, Number(item.preview_height) || 0)
            || buildFileUrlFromPath(item.image_path)
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
}

const deleteItem = async (itemId, index) => {
  try {
    if (itemId) {
      previewCache.delete(itemId)
      delete categoryMap.value[itemId]
      delete tagMap.value[itemId]
    }
    if (Number.isInteger(index) && index >= 0 && index < history.value.length) {
      history.value.splice(index, 1)
      if (selectedIndex.value >= history.value.length) {
        selectedIndex.value = Math.max(0, history.value.length - 1)
      }
    }
    if (!itemId) return
    await ImageClipboardService.removeItemById(itemId)
    await syncHistory()
  } catch (error) {
    console.error('删除图片记录失败:', error)
  }
}

const showContextMenu = (event, itemId) => {
  contextMenuVisible.value = true
  contextMenuItemId.value = itemId
  contextMenuX.value = event.clientX
  contextMenuY.value = event.clientY
}

const closeContextMenu = () => {
  contextMenuVisible.value = false
  contextMenuItemId.value = ''
}

const assignToCategory = async (category) => {
  if (!contextMenuItemId.value) return
  categoryMap.value[contextMenuItemId.value] = category
  try {
    await ImageCategoryService.setItemCategory(contextMenuItemId.value, category)
    if (category === '未分类') {
      tagMap.value[contextMenuItemId.value] = []
      await ImageClipboardService.setItemTags(contextMenuItemId.value, [])
    }
  } catch (error) {
    console.error('设置图片分类失败:', error)
  }
  closeContextMenu()
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
    tagMap.value[itemId] = tags
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
  if (!droppedItemId || category === '全部') return
  categoryMap.value[droppedItemId] = category
  try {
    await ImageCategoryService.setItemCategory(droppedItemId, category)
  } catch (error) {
    console.error('拖拽设置图片分类失败:', error)
  }
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
      delete categoryMap.value[key]
      tagMap.value[key] = []
    }
  })
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
  selectedIndex.value = typeof data.selectedIndex === 'number' ? data.selectedIndex : 0
  if (selectedIndex.value < 0 || selectedIndex.value >= history.value.length) {
    selectedIndex.value = history.value.length > 0 ? 0 : -1
  }
  warmupOne(selectedIndex.value)
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
        preview_width: item.preview_width ?? item.previewWidth ?? existing.preview_width ?? 0,
        preview_height: item.preview_height ?? item.previewHeight ?? existing.preview_height ?? 0,
        preview_rgba_base64: item.preview_rgba_base64 ?? item.previewRgbaBase64 ?? existing.preview_rgba_base64 ?? '',
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
}

const promoteLocalItemToTop = (itemId) => {
  if (!itemId || !Array.isArray(history.value) || history.value.length < 2) return
  const currentIndex = history.value.findIndex((item) => item?.id === itemId)
  if (currentIndex <= 0) return
  const selectedId = history.value[selectedIndex.value]?.id
  const [moved] = history.value.splice(currentIndex, 1)
  if (!moved) return
  history.value.unshift(moved)
  if (selectedId) {
    const nextSelectedIndex = history.value.findIndex((item) => item?.id === selectedId)
    selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : 0
  } else {
    selectedIndex.value = 0
  }
}

const mergeImagePageIntoState = (data, reset = false) => {
  const items = Array.isArray(data?.items) ? data.items : []
  if (reset) {
    clearPrefetchedPage()
    history.value = []
    categoryMap.value = {}
    tagMap.value = {}
    pinnedItems.value = []
    previewCache.clear()
    warmedIndices.clear()
    warmingIndices.clear()
  }
  for (const item of items) {
    const position = Number(item.position)
    if (!Number.isFinite(position) || position < 0) continue
    history.value[position] = {
      id: item.id,
      width: item.width,
      height: item.height,
      preview_width: item.previewWidth,
      preview_height: item.previewHeight,
      preview_rgba_base64: item.previewRgbaBase64,
      preview_png_base64: item.previewPngBase64,
      image_path: item.imagePath
    }
    categoryMap.value[item.id] = item.category || '未分类'
    tagMap.value[item.id] = Array.isArray(item.tags) ? item.tags : []
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

// 优化方案 2：减少前端同步延迟，从 80ms 降至 40ms
const scheduleHistorySync = (delay = 40) => {
  if (historyUpdateTimer) return
  historyUpdateTimer = window.setTimeout(async () => {
    historyUpdateTimer = null
    if (isPointerDown || isContentDragging || isLoadingPage.value) {
      pendingHistorySync = true
      return
    }
    await syncHistory()
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
    if (Array.isArray(payload.history) && payload.history.length > 0) {
      if (history.value.length === 0) {
        applyPayload(payload, {refocus: true})
        return
      }
      mergeShowWindowPayload(payload)
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
    mergeShowWindowPayload(event.payload || {})
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
      previewCache.set(itemId, previewUrl)
      enforcePreviewCacheSize()
      // 从待检查列表中移除
      pendingPreviewIds.delete(itemId)
      console.log(`预览就绪事件: ${itemId}`)
    }
  })
})

onBeforeUnmount(() => {
  stopContentDragging()
  stopPreviewPolling() // 停止异步预览轮询
  if (contentMetricsRafId) {
    cancelAnimationFrame(contentMetricsRafId)
    contentMetricsRafId = 0
  }
  loadMorePending = false
  previewCache.clear()
  asyncPreviewCache.clear() // 清理异步预览缓存
  pendingPreviewIds.clear() // 清理待检查的预览ID
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
  if (unlistenItemPromoted) {
    unlistenItemPromoted()
    unlistenItemPromoted = null
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

watch(previewCacheKeepIds, (ids) => {
  prunePreviewCache(ids)
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
  margin: 4px 0;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.check-icon {
  font-size: 12px;
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
