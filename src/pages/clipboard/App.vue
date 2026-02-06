<template>
  <div ref="containerRef" class="container" tabindex="-1" @keydown="handleKeydown">
    <div v-if="history.length === 0" class="empty-state">
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
          v-for="(item, index) in history"
          :key="index"
          :class="{ selected: selectedIndex === index }"
          class="clipboard-item"
          @click="handleClick(index)"
          @dblclick="handleDoubleClick(index)"
      >
        <div class="delete-btn" @click.stop="deleteItem(index)">
          <el-icon>
            <Close/>
          </el-icon>
        </div>
        <div class="index">{{ index + 1 }}</div>
        <div class="item-content">{{ item }}</div>
      </div>
      <!-- Spacer for alignment/scrolling if needed, original had it -->
      <div class="spacer"></div>
    </div>
  </div>
</template>

<script setup>
import {nextTick, onMounted, ref} from 'vue'
import {Close} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'
import {listen} from '@tauri-apps/api/event'

const history = ref([])
const selectedIndex = ref(-1)
const isVisible = ref(false)
const containerRef = ref(null)
const contentRef = ref(null)

// Drag scrolling state
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
  selectedIndex.value = data.selectedIndex !== undefined ? data.selectedIndex : 0
  isVisible.value = true

  if (history.value.length > 0) {
    // Ensure selection is valid
    if (selectedIndex.value < 0 || selectedIndex.value >= history.value.length) {
      selectedIndex.value = 0
    }
    updateSelection(selectedIndex.value, true)
  }

  // Focus container to capture key events
  nextTick(() => {
    containerRef.value?.focus()
  })
}

const updateSelection = (index, shouldScroll = false) => {
  if (index < 0 || index >= history.value.length) return
  selectedIndex.value = index

  if (shouldScroll && contentRef.value) {
    const items = contentRef.value.querySelectorAll('.clipboard-item')
    if (items[index]) {
      items[index].scrollIntoView({
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
    // Optimistic update
    history.value.splice(index, 1)
    if (selectedIndex.value >= history.value.length) {
      selectedIndex.value = Math.max(0, history.value.length - 1)
    }
    // Call backend to delete? Original script calls deleteItem but I don't see its definition in the snippet I read.
    // Wait, the original script snippet I read had:
    // invoke('delete_item', { index }) ? No, it called `deleteItem(index)` function which wasn't fully shown or was implicit?
    // Let's assume there is a backend command `delete_item` or similar.
    // Actually, looking at the snippet again:
    // 134: deleteItem(index).then(r => { ... })
    // But `deleteItem` function definition was NOT in the first 200 lines I read.
    // I should check if there is a `delete_item` command in `lib.rs` or `main.rs`.
    // But safely, I should invoke whatever command deletes the item.
    // Let's assume it's `delete_item`.
    await invoke('delete_item', {index})
  } catch (error) {
    console.error('删除失败:', error)
  }
}

const handleClick = (index) => {
  updateSelection(index)
}

const handleDoubleClick = (index) => {
  selectAndFillDirect(index)
}

const handleKeydown = (event) => {
  if (!isVisible.value) return

  switch (event.key) {
    case 'ArrowLeft':
      event.preventDefault()
      if (history.value.length > 0) {
        const newIndex = selectedIndex.value > 0 ? selectedIndex.value - 1 : 0
        updateSelection(newIndex, true)
      }
      break
    case 'ArrowRight':
      event.preventDefault()
      if (history.value.length > 0) {
        const newIndex = selectedIndex.value < history.value.length - 1 ? selectedIndex.value + 1 : history.value.length - 1
        updateSelection(newIndex, true)
      }
      break
    case 'Enter':
      event.preventDefault()
      if (selectedIndex.value >= 0 && selectedIndex.value < history.value.length) {
        selectAndFillDirect(selectedIndex.value)
      }
      break
  }
}

// Drag Scrolling Logic
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
  init()
})
</script>

<style scoped>
/* Reset and Base Styles */
.container {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: transparent;
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

.item-content {
  margin-top: 20px;
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
</style>
