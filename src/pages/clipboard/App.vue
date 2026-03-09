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
    />

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
import {nextTick, onMounted, ref} from 'vue'
import {Check} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {ClipboardService, WindowService} from '../../services/ipc'
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

const init = async () => {
  try {
    await listen('show-window', (event) => {
      showWindow(event.payload)
    })

    window.addEventListener('blur', async () => {
      try {
        await WindowService.blur()
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
    isVisible.value = false
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

const handleKeydown = async (event) => {
  if (!isVisible.value) return

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
</style>
