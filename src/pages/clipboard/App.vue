<template>
  <div ref="containerRef" class="container" tabindex="-1" @click="closeContextMenu" @keydown="handleKeydown">
    <div class="toolbar">
      <el-input
          v-model="searchKeyword"
          class="search-input"
          clearable
          placeholder="搜索剪切板历史"
          size="small"
      >
        <template #prefix>
          <el-icon>
            <Search/>
          </el-icon>
        </template>
      </el-input>
      <div class="category-nav">
        <div
            :class="{ active: categoryFilter === '全部' }"
            class="category-pill"
            @click="categoryFilter = '全部'"
        >
          全部
        </div>
        <div
            v-for="category in categories"
            :key="category"
            :class="{ active: categoryFilter === category }"
            class="category-pill"
            @click="categoryFilter = category"
            @dragenter="handleDragEnter"
            @dragleave="handleDragLeave"
            @drop="handleDrop($event, category)"
            @dragover.prevent="handleDragOver"
        >
          <span class="category-label">{{ category }}</span>
          <span
              v-if="canDeleteCategory(category)"
              class="category-remove"
              @click.stop="removeCategory(category)"
          >
            <el-icon>
              <Close/>
            </el-icon>
          </span>
        </div>
        <div v-if="!isAddingCategory" class="category-pill add-category" @click="startCreateCategory">
          <el-icon>
            <Plus/>
          </el-icon>
          <span>新增分类</span>
        </div>
        <el-input
            v-else
            ref="newCategoryInputRef"
            v-model="newCategoryName"
            class="category-input"
            placeholder="输入分类名"
            size="small"
            @blur="confirmCreateCategory"
            @keydown.enter.prevent="confirmCreateCategory"
            @keydown.esc.prevent="cancelCreateCategory"
        />
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

    <div
        v-else
        ref="contentRef"
        class="content"
        @mousedown="handleMouseDown"
        @mouseleave="handleMouseLeave"
        @mousemove="handleMouseMove"
        @mouseup="handleMouseUp"
    >
      <div
          v-for="(entry, visibleIndex) in visibleHistory"
          :key="entry.index"
          :class="{ selected: selectedIndex === entry.index }"
          class="clipboard-item"
          draggable="true"
          @click="handleClick(entry.index, visibleIndex)"
          @dblclick="handleDoubleClick(entry.index)"
          @dragend="handleDragEnd"
          @dragstart="handleDragStart($event, entry.item)"
          @contextmenu.prevent="showContextMenu($event, entry.item)"
      >
        <div v-if="isWebUrl(entry.item)" class="open-btn" @click.stop="openWebUrl(entry.item)">
          <el-icon>
            <Link/>
          </el-icon>
        </div>
        <div class="delete-btn" @click.stop="deleteItem(entry.index)">
          <el-icon>
            <Close/>
          </el-icon>
        </div>
        <div class="index">{{ entry.index + 1 }}</div>
        <div class="category-wrap" @click.stop>
          <div class="category-chip">{{ getItemCategory(entry.item) }}</div>
        </div>
        <div class="item-content">{{ entry.item }}</div>
      </div>
      <!-- Spacer for alignment/scrolling if needed, original had it -->
      <div class="spacer"></div>
    </div>

    <!-- Context Menu -->
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
        <el-icon v-if="getItemCategory(contextMenuItem) === category" class="check-icon">
          <Check/>
        </el-icon>
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onMounted, ref} from 'vue'
import {Check, Close, Link, Plus, Search} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'
import {listen} from '@tauri-apps/api/event'
import {openUrl as openExternalUrl} from '@tauri-apps/plugin-opener'

const history = ref([])
const selectedIndex = ref(-1)
const isVisible = ref(false)
const containerRef = ref(null)
const contentRef = ref(null)
const searchKeyword = ref('')
const categoryFilter = ref('全部')
const categoryMap = ref({})
const categories = ref(['未分类'])
const isAddingCategory = ref(false)
const newCategoryName = ref('')
const newCategoryInputRef = ref(null)

// Context Menu State
const contextMenuVisible = ref(false)
const contextMenuX = ref(0)
const contextMenuY = ref(0)
const contextMenuItem = ref(null)
const dragItem = ref(null)

let isDown = false
let startX = 0
let scrollLeft = 0

const init = async () => {
  try {
    await listen('show-window', (event) => {
      showWindow(event.payload)
    })

    window.addEventListener('blur', async () => {
      try {
        await invoke('window_blur')
        isVisible.value = false
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
  // 从后端数据恢复分类状态
  if (data.categories) {
    categoryMap.value = data.categories
  }
  if (Array.isArray(data.category_list)) {
    // 过滤并去重
    const list = data.category_list.filter(c => c !== '未分类' && c !== '全部')
    const uniqueList = Array.from(new Set(list))
    categories.value = ['未分类', ...uniqueList]
  } else {
    // 如果后端未返回分类列表（旧数据），尝试从 categories map 中恢复
    if (data.categories) {
      const extractedCategories = Object.values(data.categories)
      const uniqueList = Array.from(new Set(extractedCategories)).filter(c => c !== '未分类' && c !== '全部')
      categories.value = ['未分类', ...uniqueList]
    }
  }
  
  selectedIndex.value = data.selectedIndex !== undefined ? data.selectedIndex : 0
  isVisible.value = true
  // 不再需要前端同步逻辑，完全信任后端数据
  // syncCategoryMap()

  if (history.value.length > 0) {
    if (selectedIndex.value < 0 || selectedIndex.value >= history.value.length) {
      selectedIndex.value = 0
    }
    updateSelection(selectedIndex.value, true)
  }

  nextTick(() => {
    containerRef.value?.focus()
  })
}

const updateSelection = (index, shouldScroll = false, visibleIndex = null) => {
  if (index < 0 || index >= history.value.length) return
  selectedIndex.value = index

  if (shouldScroll && contentRef.value) {
    const items = contentRef.value.querySelectorAll('.clipboard-item')
    const targetIndex =
        visibleIndex !== null ? visibleIndex : visibleHistory.value.findIndex((entry) => entry.index === index)
    if (targetIndex >= 0 && items[targetIndex]) {
      items[targetIndex].scrollIntoView({
        behavior: 'smooth',
        block: 'nearest',
        inline: 'center'
      })
    }
  }
}

const selectAndFillDirect = async (index) => {
  try {
    await invoke('select_and_fill', {index})
    isVisible.value = false
  } catch (error) {
    console.error('填充内容失败:', error)
  }
}

const deleteItem = async (index) => {
  try {
    const removedItem = history.value[index]
    history.value.splice(index, 1)
    if (selectedIndex.value >= history.value.length) {
      selectedIndex.value = Math.max(0, history.value.length - 1)
    }
    removeItemCategory(removedItem)
    await invoke('remove_clipboard_item', {index})
  } catch (error) {
    console.error('删除失败:', error)
  }
}

const handleClick = (index, visibleIndex) => {
  updateSelection(index, false, visibleIndex)
  closeContextMenu()
}

const handleDoubleClick = (index) => {
  selectAndFillDirect(index)
}

const showContextMenu = (event, item) => {
  contextMenuVisible.value = true
  contextMenuItem.value = item

  // Calculate position to keep menu within viewport
  const menuWidth = 160
  // Max height limit for context menu (e.g. 50% of window height or fixed value)
  const maxMenuHeight = Math.min(300, window.innerHeight * 0.6)

  let x = event.clientX
  let y = event.clientY

  if (x + menuWidth > window.innerWidth) {
    x -= menuWidth
  }

  // If menu would go off bottom, open upwards
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

const assignToCategory = (category) => {
  if (contextMenuItem.value && category !== '全部') {
    setItemCategory(contextMenuItem.value, category)
  }
  closeContextMenu()
}

const handleDragStart = (event, item) => {
  isDown = false // 防止与自定义滚动冲突
  dragItem.value = item
  event.dataTransfer.effectAllowed = 'copy'
  event.dataTransfer.setData('text/plain', item)
}

const handleDragEnd = () => {
  dragItem.value = null
}

const handleDragOver = (event) => {
  event.preventDefault()
  event.dataTransfer.dropEffect = 'copy'
}

const handleDragEnter = (event) => {
  event.preventDefault()
  const target = event.currentTarget
  if (target && target.classList.contains('category-pill')) {
    target.classList.add('drag-over')
  }
}

const handleDragLeave = (event) => {
  const target = event.currentTarget
  if (target && target.classList.contains('category-pill')) {
    target.classList.remove('drag-over')
  }
}

const handleDrop = (event, category) => {
  event.preventDefault()

  // Remove visual feedback
  const target = event.currentTarget
  if (target && target.classList.contains('category-pill')) {
    target.classList.remove('drag-over')
  }

  if (dragItem.value && category !== '全部') {
    setItemCategory(dragItem.value, category)
  }
}

const handleKeydown = (event) => {
  if (!isVisible.value) return

  // Close context menu on Esc
  if (contextMenuVisible.value && event.key === 'Escape') {
    closeContextMenu()
    return
  }

  switch (event.key) {
    case 'ArrowLeft':
      event.preventDefault()
      moveSelection(-1)
      break
    case 'ArrowRight':
      event.preventDefault()
      moveSelection(1)
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
  }
}

const handleMouseDown = (e) => {
  isDown = true
  startX = e.pageX - contentRef.value.offsetLeft
  scrollLeft = contentRef.value.scrollLeft
  contentRef.value.style.cursor = 'grabbing'
}

const handleMouseLeave = () => {
  isDown = false
  if (contentRef.value) contentRef.value.style.cursor = 'default'
}

const handleMouseUp = () => {
  isDown = false
  if (contentRef.value) contentRef.value.style.cursor = 'default'
}

const handleMouseMove = (e) => {
  if (!isDown) return
  e.preventDefault()
  const x = e.pageX - contentRef.value.offsetLeft
  const walk = (x - startX) * 2
  contentRef.value.scrollLeft = scrollLeft - walk
}

onMounted(() => {
  loadCategoryStore()
  init()
})

const visibleHistory = computed(() => {
  const keyword = searchKeyword.value.trim().toLowerCase()
  const filter = categoryFilter.value
  return history.value
      .map((item, index) => ({item, index}))
      .filter((entry) => {
        const itemCategory = getItemCategory(entry.item)
        if (filter !== '全部' && itemCategory !== filter) {
          return false
        }
        if (!keyword) return true
        return entry.item.toLowerCase().includes(keyword)
      })
})

const moveSelection = (direction) => {
  const visible = visibleHistory.value
  if (visible.length === 0) return
  let visibleIndex = visible.findIndex((entry) => entry.index === selectedIndex.value)
  if (visibleIndex < 0) visibleIndex = 0
  const nextVisibleIndex = Math.max(0, Math.min(visible.length - 1, visibleIndex + direction))
  updateSelection(visible[nextVisibleIndex].index, true, nextVisibleIndex)
}

const loadCategoryStore = () => {
  // 不再从 localStorage 加载，数据由 showWindow 从后端传入
}

const saveCategoryStore = () => {
  // 不再保存到 localStorage，改用后端接口
}

const syncCategoryMap = () => {
  // 不再需要前端同步逻辑
}

const getItemCategory = (item) => {
  return categoryMap.value[item] || '未分类'
}

const setItemCategory = async (item, value) => {
  const category = (value || '').trim()
  if (!category) {
    await removeItemCategory(item)
    return
  }

  // 乐观更新
  categoryMap.value[item] = category
  if (!categories.value.includes(category)) {
    categories.value.push(category)
  }

  try {
    await invoke('set_item_category', {item, category})
  } catch (error) {
    console.error('保存分类失败:', error)
  }
}

const removeItemCategory = async (item) => {
  if (!item) return
  if (categoryMap.value[item]) {
    delete categoryMap.value[item]
    try {
      await invoke('set_item_category', {item, category: ""})
    } catch (error) {
      console.error('移除分类失败:', error)
    }
  }
}

const removeCategory = async (category) => {
  if (!canDeleteCategory(category)) return

  // 乐观更新
  categories.value = categories.value.filter((item) => item !== category)
  Object.keys(categoryMap.value).forEach((item) => {
    if (categoryMap.value[item] === category) {
      delete categoryMap.value[item]
    }
  })

  if (categoryFilter.value === category) {
    categoryFilter.value = '全部'
  }

  try {
    await invoke('remove_category', {category})
  } catch (error) {
    console.error('删除分类失败:', error)
  }
}

const canDeleteCategory = (category) => {
  return category !== '未分类'
}

const startCreateCategory = () => {
  isAddingCategory.value = true
  newCategoryName.value = ''
  nextTick(() => {
    newCategoryInputRef.value?.focus()
  })
}

const confirmCreateCategory = async () => {
  const category = newCategoryName.value.trim()
  if (category && category !== '未分类' && category !== '全部') {
    if (!categories.value.includes(category)) {
      categories.value.push(category)
      try {
        await invoke('add_category', {category})
      } catch (error) {
        console.error('添加分类失败:', error)
      }
    }
  }
  isAddingCategory.value = false
  newCategoryName.value = ''
}

const cancelCreateCategory = () => {
  isAddingCategory.value = false
  newCategoryName.value = ''
}

const isWebUrl = (value) => {
  if (!value) return false
  const text = value.trim()
  return /^https?:\/\/\S+$/i.test(text) || /^www\.\S+$/i.test(text)
}

const normalizeUrl = (value) => {
  const text = value.trim()
  if (/^https?:\/\//i.test(text)) return text
  if (/^www\./i.test(text)) return `https://${text}`
  return text
}

const openWebUrl = async (value) => {
  try {
    const url = normalizeUrl(value)
    if (isWebUrl(url)) {
      await openExternalUrl(url)
    }
  } catch (error) {
    console.error('打开网址失败:', error)
  }
}
</script>

<style>
::-webkit-scrollbar {
  display: none !important;
  width: 0 !important;
  height: 0 !important;
}

html, body {
  overflow: hidden;
  scrollbar-width: none;
}
</style>

<style scoped>
/* Reset and Base Styles */
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

.toolbar {
  display: flex;
  gap: 8px;
  padding: 8px;
  align-items: center;
}

.search-input {
  width: 240px;
  flex: 0 0 auto;
}

.search-input :deep(.el-input__wrapper) {
  background: rgba(15, 15, 20, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.12);
  box-shadow: 0 0 0 1px rgba(64, 158, 255, 0.15);
  border-radius: 10px;
  padding: 2px 10px;
  backdrop-filter: blur(12px);
  transition: all 0.2s ease;
}

.search-input :deep(.el-input__wrapper.is-focus) {
  border-color: var(--el-color-primary, #409eff);
  box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.25);
}

.search-input :deep(.el-input__inner) {
  color: #e5e7eb;
  font-size: 13px;
  letter-spacing: 0.2px;
}

.search-input :deep(.el-input__prefix) {
  color: rgba(255, 255, 255, 0.55);
}

.search-input :deep(.el-input__suffix) {
  color: rgba(255, 255, 255, 0.45);
}

.search-input :deep(.el-input__inner::placeholder) {
  color: rgba(255, 255, 255, 0.45);
}

.category-nav {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
}

.category-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 999px;
  background: rgba(15, 15, 20, 0.55);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.75);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s ease;
  white-space: nowrap;
}

/* 关键修复：确保子元素不拦截鼠标事件，保证 dragover/drop 能触发在父元素上 */
.category-pill * {
  pointer-events: none;
}

/* 但移除按钮需要响应点击 */
.category-pill .category-remove {
  pointer-events: auto;
}

.category-pill:hover {
  border-color: rgba(64, 158, 255, 0.5);
  color: #fff;
}

.category-pill.active {
  background: rgba(64, 158, 255, 0.2);
  border-color: var(--el-color-primary, #409eff);
  color: #fff;
  box-shadow: 0 0 0 1px rgba(64, 158, 255, 0.3);
}

.category-pill.drag-over {
  background: rgba(103, 194, 58, 0.18);
  border-color: rgba(103, 194, 58, 0.8);
  color: #fff;
  box-shadow: 0 0 0 1px rgba(103, 194, 58, 0.35);
}

.category-pill.add-category {
  border-style: dashed;
  color: rgba(255, 255, 255, 0.7);
}

.category-pill.add-category:hover {
  color: #fff;
  border-color: rgba(64, 158, 255, 0.6);
}

.category-input {
  width: 160px;
}

.category-input :deep(.el-input__wrapper) {
  background: rgba(15, 15, 20, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.12);
  box-shadow: 0 0 0 1px rgba(64, 158, 255, 0.12);
  border-radius: 999px;
  padding: 2px 10px;
  transition: all 0.2s ease;
}

.category-input :deep(.el-input__wrapper.is-focus) {
  border-color: var(--el-color-primary, #409eff);
  box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.2);
}

.category-input :deep(.el-input__inner) {
  color: #e5e7eb;
  font-size: 12px;
}

.category-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.7);
  transition: all 0.2s ease;
}

.category-pill:hover .category-remove {
  background: rgba(245, 108, 108, 0.2);
  color: #f56c6c;
}

.empty-state {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 100%;
  color: #fff;
}

.hint {
  color: #909399;
  font-size: 12px;
}

.content {
  flex: 1;
  display: flex;
  gap: 8px;
  padding: 8px;
  flex-direction: row;
  overflow-x: auto;
  overflow-y: hidden;
  scroll-behavior: smooth;
  margin-top: 10px;
  scrollbar-width: none; /* Firefox */
}

.content::-webkit-scrollbar {
  display: none; /* Chrome/Safari */
}

.spacer {
  flex: 0 0 742px; /* Original design had this spacer */
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
  height: 180px;
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

.open-btn {
  position: absolute;
  top: 5px;
  right: 30px;
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

.open-btn .el-icon {
  font-size: 12px;
}

.clipboard-item:hover .open-btn {
  opacity: 1;
}

.open-btn:hover {
  background: var(--el-color-primary, #409eff);
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

.index {
  position: absolute;
  top: 5px;
  left: 5px;
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
  left: 8px;
  right: 8px;
  bottom: 8px;
}

.category-chip {
  width: 100%;
  padding: 4px 10px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.85);
  font-size: 12px;
  text-align: center;
}

.item-content {
  margin-top: 24px;
  padding-bottom: 32px;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 7; /* Show about 7 lines */
  -webkit-box-orient: vertical;
  font-size: 13px;
  line-height: 1.5;
  color: #dcdfe6;
  white-space: pre-wrap;
  word-break: break-all;
}

.context-menu {
  position: fixed;
  z-index: 9999;
  background: rgba(18, 18, 24, 0.95);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(12px);
  min-width: 160px;
  max-height: 60vh;
  overflow-y: auto;
  padding: 4px 0;
  color: #e5e7eb;
  font-size: 13px;
}

.context-menu-header {
  padding: 8px 12px;
  color: rgba(255, 255, 255, 0.5);
  font-size: 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  margin-bottom: 4px;
}

.context-menu-item {
  padding: 8px 12px;
  cursor: pointer;
  display: flex;
  justify-content: space-between;
  align-items: center;
  transition: background 0.2s;
}

.context-menu-item:hover {
  background: rgba(64, 158, 255, 0.2);
  color: #fff;
}

.check-icon {
  color: var(--el-color-primary, #409eff);
  font-size: 14px;
}
</style>
