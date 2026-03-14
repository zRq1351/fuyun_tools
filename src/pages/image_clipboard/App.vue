<template>
  <div ref="containerRef" class="container" tabindex="-1" @click="closeContextMenu" @keydown="handleKeydown">
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
        @mouseleave="handleContentMouseLeave"
        @mousemove="handleContentMouseMove"
        @mouseup="handleContentMouseUp"
        @wheel.prevent="handleContentWheel"
    >
      <div
          v-for="entry in filteredHistory"
          :id="`image-item-${entry.index}`"
          :key="entry.item.id"
          :class="{ selected: selectedIndex === entry.index }"
          class="clipboard-item"
          draggable="true"
          @click="selectByIndex(entry.index)"
          @dblclick="fillByIndex(entry.index)"
          @dragend="handleDragEnd"
          @dragstart="handleDragStart($event, entry.item.id)"
          @mouseenter="handleItemHover(entry.index)"
          @contextmenu.prevent="showContextMenu($event, entry.item.id)"
      >
        <div class="delete-btn" @click.stop="deleteItem(entry.index)">
          <el-icon>
            <Close/>
          </el-icon>
        </div>
        <button class="fullscreen-btn" title="全屏预览" @click.stop="openFullscreen(entry.index)">
          <el-icon>
            <FullScreen/>
          </el-icon>
        </button>
        <div class="index-tools">
          <div class="index">{{ entry.index + 1 }}</div>
        </div>
        <div class="category-wrap">
          <div class="category-chip">{{ getItemCategory(entry.item.id) }}</div>
        </div>
        <div class="item-content">
          <img :src="getPreviewDataUrl(entry.item)" alt="" class="image-preview" draggable="false" @dragstart.prevent/>
          <div class="image-meta">{{ entry.item.width }} × {{ entry.item.height }}</div>
        </div>
      </div>
      <div class="spacer"></div>
    </div>

    <div class="status-footer" @click.stop @mousedown.stop>
      <div class="status-text">
        <span class="status-label">{{ selectedStatusText }}</span>
        <div class="status-actions">
          <button aria-label="回到开头" class="nav-action-btn icon-btn" title="回到开头" type="button"
                  @click="scrollToStart">←
          </button>
          <button aria-label="滑动到最后" class="nav-action-btn icon-btn" title="滑动到最后" type="button"
                  @click="scrollToEnd">→
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
    </div>

  </div>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {Check, Close, FullScreen} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {ImageCategoryService, ImageClipboardService, WindowService} from '../../services/ipc'
import ClipboardToolbar from '../clipboard/components/ClipboardToolbar.vue'
import {useWindowOffset} from '../clipboard/composables/useWindowOffset'

const containerRef = ref(null)
const contentRef = ref(null)
const history = ref([])
const categoryMap = ref({})
const categories = ref(['未分类'])
const categoryFilter = ref('全部')
const selectedIndex = ref(0)
const searchKeyword = ref('')
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
let refreshTimer = null
let unlistenShowWindow = null
let unlistenHistoryUpdated = null
const SHORT_POLL_INTERVAL_MS = 1500
const SHORT_POLL_DURATION_MS = 12000
let shortPollUntil = 0
let isPointerDown = false
let isContentDragging = false
let dragStartX = 0
let dragStartScrollLeft = 0

const stopContentDragging = () => {
  isPointerDown = false
  isContentDragging = false
  if (contentRef.value) {
    contentRef.value.style.cursor = 'default'
  }
  document.body.style.removeProperty('user-select')
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
}

const handleGlobalMouseMove = (event) => {
  if (!isPointerDown || !contentRef.value) return
  const delta = event.pageX - dragStartX
  if (!isContentDragging && Math.abs(delta) > 4) {
    isContentDragging = true
    contentRef.value.style.cursor = 'grabbing'
    document.body.style.userSelect = 'none'
  }
  if (isContentDragging) {
    contentRef.value.scrollLeft = dragStartScrollLeft - delta
  }
}

const handleGlobalMouseUp = () => {
  stopContentDragging()
}

const handleContentMouseDown = (event) => {
  if (event.button !== 0) return
  if (event.target.closest('.delete-btn') || event.target.closest('.fullscreen-btn')) {
    return
  }
  if (!contentRef.value) return
  isPointerDown = true
  isContentDragging = false
  dragStartX = event.pageX
  dragStartScrollLeft = contentRef.value.scrollLeft
  window.addEventListener('mousemove', handleGlobalMouseMove)
  window.addEventListener('mouseup', handleGlobalMouseUp, true)
}

const handleContentMouseMove = () => {
}
const handleContentMouseUp = () => {
}
const handleContentMouseLeave = () => {
}

const handleContentWheel = (event) => {
  if (!contentRef.value) return
  const delta = Math.abs(event.deltaY) >= Math.abs(event.deltaX) ? event.deltaY : event.deltaX
  contentRef.value.scrollLeft += delta
}

const ensureKeyboardSelectionVisible = async () => {
  await nextTick()
  const selected = selectedIndex.value
  if (selected < 0) return
  const element = document.getElementById(`image-item-${selected}`)
  const container = contentRef.value || element?.closest('.content')
  if (!element || !container) return
  const EDGE_PADDING = 8
  const maxScrollLeft = Math.max(0, container.scrollWidth - container.clientWidth)
  const targetLeft = Math.max(0, element.offsetLeft - EDGE_PADDING)
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

const filteredHistory = computed(() => {
  const keyword = searchKeyword.value.trim().toLowerCase()
  return history.value
      .map((item, index) => ({item, index}))
      .filter((entry) => {
        const category = getItemCategory(entry.item.id)
        if (categoryFilter.value !== '全部' && category !== categoryFilter.value) {
          return false
        }
        if (!keyword) {
          return true
        }
        return category.toLowerCase().includes(keyword)
      })
})

const selectedStatusText = computed(() => {
  const total = filteredHistory.value.length
  if (total === 0) return '当前无选中项'
  const current = filteredHistory.value.findIndex((entry) => entry.index === selectedIndex.value)
  const display = current >= 0 ? current + 1 : 1
  return `当前选中：第 ${display} / ${total} 条`
})

const decodeBase64ToRgba = (base64) => {
  const binary = atob(base64)
  const rgba = new Uint8ClampedArray(binary.length)
  for (let i = 0; i < binary.length; i++) {
    rgba[i] = binary.charCodeAt(i)
  }
  return rgba
}

const buildDataUrlFromRgba = (rgbaBase64, width, height) => {
  if (!rgbaBase64 || !width || !height) {
    return ''
  }
  const rgba = decodeBase64ToRgba(rgbaBase64)
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext('2d')
  if (!ctx) {
    return ''
  }
  const imageData = new ImageData(rgba, width, height)
  ctx.putImageData(imageData, 0, 0)
  return canvas.toDataURL('image/png')
}

const getPreviewDataUrl = (item) => {
  if (previewCache.has(item.id)) {
    return previewCache.get(item.id)
  }
  try {
    const hasPreview = item.preview_rgba_base64 && item.preview_width && item.preview_height
    const rgbaBase64 = hasPreview ? item.preview_rgba_base64 : item.rgba_base64
    const drawWidth = hasPreview ? item.preview_width : item.width
    const drawHeight = hasPreview ? item.preview_height : item.height
    const dataUrl = buildDataUrlFromRgba(rgbaBase64, drawWidth, drawHeight)
    previewCache.set(item.id, dataUrl)
    return dataUrl
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
  ImageClipboardService.warmupItem(index)
      .then(() => {
        warmedIndices.add(index)
      })
      .catch(() => {
      })
      .finally(() => {
        warmingIndices.delete(index)
      })
}

const warmupAround = (index) => {
  warmupOne(index - 1)
  warmupOne(index)
  warmupOne(index + 1)
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
  if (contentRef.value) {
    contentRef.value.scrollLeft = Math.max(0, contentRef.value.scrollWidth - contentRef.value.clientWidth)
  }
  if (filteredHistory.value.length > 0) {
    const lastIndex = filteredHistory.value[filteredHistory.value.length - 1].index
    selectedIndex.value = lastIndex
    await ensureKeyboardSelectionVisible()
  }
}

const fillByIndex = async (index) => {
  if (isFilling.value) return
  isFilling.value = true
  isVisible.value = false
  try {
    await ImageClipboardService.selectAndFill(index)
  } catch (error) {
    console.error('回填图片失败:', error)
  } finally {
    window.setTimeout(() => {
      isFilling.value = false
    }, 300)
  }
}

const openFullscreen = async (index) => {
  try {
    await ImageClipboardService.openPreviewWindow(index)
  } catch (error) {
    console.error('打开预览窗口失败:', error)
  }
}

const deleteItem = async (index) => {
  try {
    const removed = history.value[index]
    if (removed) {
      previewCache.delete(removed.id)
      delete categoryMap.value[removed.id]
    }
    history.value.splice(index, 1)
    if (selectedIndex.value >= history.value.length) {
      selectedIndex.value = Math.max(0, history.value.length - 1)
    }
    await ImageClipboardService.removeItem(index)
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
  } catch (error) {
    console.error('设置图片分类失败:', error)
  }
  closeContextMenu()
}

const handleDragStart = (event, itemId) => {
  if (!event.ctrlKey || isContentDragging) {
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

const confirmCreateCategory = async (event) => {
  const isBlurEvent = event?.type === 'blur'
  if (isBlurEvent && Date.now() - categoryInputOpenedAt.value < 250) {
    nextTick(() => {
      newCategoryInputRef.value?.focus()
    })
    return
  }
  const category = newCategoryName.value.trim()
  if (category && category !== '未分类' && category !== '全部' && !categories.value.includes(category)) {
    categories.value.push(category)
    try {
      await ImageCategoryService.addCategory(category)
    } catch (error) {
      console.error('添加图片分类失败:', error)
    }
  }
  isAddingCategory.value = false
  newCategoryName.value = ''
  categoryInputOpenedAt.value = 0
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
  history.value = Array.isArray(data.history) ? data.history : []
  warmedIndices.clear()
  warmingIndices.clear()
  if (typeof data.bottomOffset === 'number') {
    bottomOffset.value = clampBottomOffset(data.bottomOffset)
  }
  if (!isAddingCategory.value) {
    categoryMap.value = data.categories || {}
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

const syncHistory = async () => {
  try {
    const data = await ImageClipboardService.getHistory()
    applyPayload({
      ...data,
      selectedIndex: typeof selectedIndex.value === 'number' ? selectedIndex.value : 0
    }, {refocus: false})
  } catch (error) {
    console.error('同步图片历史失败:', error)
  }
}

const startShortPolling = () => {
  shortPollUntil = Date.now() + SHORT_POLL_DURATION_MS
  if (refreshTimer) return
  refreshTimer = window.setInterval(async () => {
    if (!isVisible.value || isAddingCategory.value) return
    if (Date.now() > shortPollUntil) {
      if (refreshTimer) {
        window.clearInterval(refreshTimer)
        refreshTimer = null
      }
      return
    }
    await syncHistory()
  }, SHORT_POLL_INTERVAL_MS)
}

const handleKeydown = async (event) => {
  if (!isVisible.value) return
  if (isInputLikeTarget(event.target)) return

  if (contextMenuVisible.value && event.key === 'Escape') {
    closeContextMenu()
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
    await ensureKeyboardSelectionVisible()
  } else if (event.key === 'ArrowRight') {
    event.preventDefault()
    currentVisibleIndex = Math.min(visible.length - 1, currentVisibleIndex + 1)
    selectedIndex.value = visible[currentVisibleIndex].index
    await ensureKeyboardSelectionVisible()
  } else if (event.key === 'Enter') {
    event.preventDefault()
    if (selectedIndex.value >= 0 && selectedIndex.value < history.value.length) {
      await fillByIndex(selectedIndex.value)
    }
  } else if (event.key === 'Escape') {
    event.preventDefault()
    await WindowService.imageBlur()
  }
}

const isInputLikeTarget = (target) => {
  const tagName = target?.tagName?.toLowerCase?.()
  return tagName === 'input' || tagName === 'textarea' || target?.isContentEditable
}

onMounted(async () => {
  await syncHistory()
  unlistenShowWindow = await listen('show-image-window', (event) => {
    applyPayload(event.payload, {refocus: true})
    startShortPolling()
  })
  unlistenHistoryUpdated = await listen('image-history-updated', async () => {
    if (isVisible.value && !isAddingCategory.value) {
      await syncHistory()
    }
  })

  window.addEventListener('blur', async () => {
    try {
      await WindowService.imageBlur()
      isVisible.value = false
    } catch (error) {
      console.error('调用 image_window_blur 失败:', error)
    }
  })
})

onBeforeUnmount(() => {
  stopContentDragging()
  if (refreshTimer) {
    window.clearInterval(refreshTimer)
    refreshTimer = null
  }
  if (unlistenShowWindow) {
    unlistenShowWindow()
    unlistenShowWindow = null
  }
  if (unlistenHistoryUpdated) {
    unlistenHistoryUpdated()
    unlistenHistoryUpdated = null
  }
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
})

watch(selectedIndex, (value) => {
  warmupOne(value)
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

.spacer {
  flex: 0 0 742px;
  height: 1px;
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

.fullscreen-btn:hover {
  border-color: var(--el-color-primary, #409eff);
  color: #fff;
  background: var(--el-color-primary, #409eff);
}

.clipboard-item:hover .fullscreen-btn {
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
  right: 36px;
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

.item-content {
  margin-top: 38px;
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
  background: linear-gradient(180deg, rgba(13, 20, 33, 0.96), rgba(10, 16, 26, 0.94));
  border-top: 1px solid rgba(167, 214, 255, 0.36);
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

.status-actions {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-left: 12px;
}

.nav-action-btn {
  appearance: none;
  border: 1px solid rgba(178, 223, 255, 0.95);
  background: rgba(51, 112, 201, 0.18);
  color: #f1f7ff;
  border-radius: 7px;
  font-size: 12px;
  line-height: 1;
  font-weight: 700;
  padding: 9px 14px;
  min-height: 32px;
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.2s ease, background 0.2s ease;
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
  border-color: rgba(178, 223, 255, 0.95);
  background: linear-gradient(160deg, rgba(66, 146, 238, 0.98), rgba(51, 112, 201, 0.95));
  color: #ffffff;
  box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.14) inset, 0 6px 14px rgba(20, 56, 105, 0.52);
  transform: translateY(-1px);
}

.nav-action-btn:active {
  transform: translateY(0);
}

.nav-action-btn:focus-visible {
  outline: 2px solid rgba(180, 226, 255, 0.95);
  outline-offset: 2px;
}

</style>
