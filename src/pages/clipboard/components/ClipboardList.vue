<template>
  <div
      ref="contentRef"
      class="content"
      @mousedown="handleMouseDown"
      @mouseleave="handleMouseLeave"
      @mousemove="handleMouseMove"
      @mouseup="handleMouseUp"
      @scroll="handleScroll"
      @wheel.prevent="handleWheel"
  >
    <div v-if="leftSpacerWidth > 0" :style="{ minWidth: leftSpacerWidth + 'px', height: '1px' }"></div>
    <div
        v-for="(entry) in virtualItems"
        :id="'clipboard-item-' + entry.index"
        :key="entry.index"
        :class="{ selected: selectedIndex === entry.index }"
        class="clipboard-item"
        @click="handleClick(entry.index)"
        @dblclick="handleDoubleClick(entry.index)"
        @contextmenu.prevent="showContextMenu($event, entry.item, entry.index)"
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
    <div v-if="rightSpacerWidth > 0" :style="{ minWidth: rightSpacerWidth + 'px', height: '1px' }"></div>
    <div class="spacer"></div>
  </div>
</template>

<script setup>
import {computed, onUnmounted, ref} from 'vue'
import {Close, Link} from '@element-plus/icons-vue'
import {openUrl as openExternalUrl} from '@tauri-apps/plugin-opener'

const props = defineProps({
  visibleHistory: {
    type: Array,
    required: true
  },
  selectedIndex: {
    type: Number,
    required: true
  },
  getItemCategory: {
    type: Function,
    required: true
  },
  deleteItem: {
    type: Function,
    required: true
  },
  updateSelection: {
    type: Function,
    required: true
  },
  selectAndFillDirect: {
    type: Function,
    required: true
  },
  showContextMenu: {
    type: Function,
    required: true
  },
  handleDragStart: {
    type: Function,
    required: true
  },
  handleDragEnd: {
    type: Function,
    required: true
  }
})

const contentRef = ref(null)
let isDown = false
let startX = 0
let scrollLeftVal = 0
const handleScroll = () => {
}

const stopDragging = () => {
  if (!isDown) return
  isDown = false
  if (contentRef.value) {
    contentRef.value.style.cursor = 'default'
  }
  document.body.style.removeProperty('user-select')
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
  window.removeEventListener('dragend', handleGlobalDragEnd)
}

const virtualItems = computed(() => {
  return props.visibleHistory
})

const leftSpacerWidth = computed(() => 0)
const rightSpacerWidth = computed(() => 0)

onUnmounted(() => {
  stopDragging()
  window.removeEventListener('blur', stopDragging)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
  window.removeEventListener('dragend', handleGlobalDragEnd)
})

const handleClick = (index) => {
  // visibleIndex is not strictly needed for logic but was used for scrolling into view?
  // passing -1 or null as visibleIndex if not needed by updateSelection
  props.updateSelection(index, false, contentRef.value, null)
}

const handleDoubleClick = (index) => {
  props.selectAndFillDirect(index)
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

const handleMouseDown = (e) => {
  // 如果点击的是删除按钮或链接按钮，不触发拖拽
  if (e.target.closest('.delete-btn') || e.target.closest('.open-btn')) {
    return
  }

  isDown = true
  startX = e.pageX
  if (contentRef.value) {
    scrollLeftVal = contentRef.value.scrollLeft
    contentRef.value.style.cursor = 'grabbing'
    // 强制禁用选中，防止滑动时选中文本
    document.body.style.userSelect = 'none'
  }

  // Attach global listeners
  window.addEventListener('mousemove', handleGlobalMouseMove)
  window.addEventListener('mouseup', handleGlobalMouseUp, true)
  // 监听 dragend 以防止原生拖拽导致 mouseup 丢失
  window.addEventListener('dragend', handleGlobalDragEnd)
}

const handleGlobalMouseUp = () => {
  stopDragging()
}

const handleGlobalDragEnd = () => {
  stopDragging()
}

const handleGlobalMouseMove = (e) => {
  if (!isDown || !contentRef.value) return
  const x = e.pageX
  // Calculate delta from initial click position
  const walk = (x - startX)

  // Direct DOM update for maximum responsiveness (1:1 movement)
  contentRef.value.scrollLeft = scrollLeftVal - walk
}

const handleVisibilityChange = () => {
  if (document.hidden) {
    stopDragging()
  }
}

window.addEventListener('blur', stopDragging)
document.addEventListener('visibilitychange', handleVisibilityChange)

const handleWheel = (e) => {
  if (!contentRef.value) return
  const delta = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX
  contentRef.value.scrollLeft += delta
}

// Keep original local handlers for compatibility/safety but they delegate or are replaced
const handleMouseLeave = () => {
  // No-op: dragging should continue even if mouse leaves the element
}

const handleMouseUp = () => {
  // Handled by global listener, but keeping for safety if event bubbling happens
}

const handleMouseMove = (e) => {
  // Handled by global listener
}

defineExpose({
  contentRef
})
</script>

<style scoped>
.content {
  flex: 1;
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
  left: 36px;
  right: 56px;
  top: 5px;
  display: flex;
  justify-content: center;
  z-index: 10;
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
  padding-bottom: 10px;
  flex: 1;
  min-height: 0;
  position: relative;
  z-index: 1;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: none;
  -ms-overflow-style: none;
  font-size: 13px;
  line-height: 1.5;
  color: #dcdfe6;
  white-space: pre-wrap;
  word-break: break-all;
}

.item-content::-webkit-scrollbar {
  display: none;
}
</style>
