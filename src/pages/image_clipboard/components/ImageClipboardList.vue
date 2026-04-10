<template>
  <div
      ref="contentRef"
      class="content"
      @mousedown="handleMouseDown"
      @scroll="handleScroll"
      @wheel.prevent="handleWheel"
  >
    <div
        v-for="entry in visibleHistory"
        :id="`image-item-${entry.index}`"
        :key="entry.item.id"
        :class="{ selected: selectedIndex === entry.index }"
        :draggable="isCtrlKeyPressed"
        class="clipboard-item"
        @click="handleClick(entry.index)"
        @dblclick="handleDoubleClick(entry.item.id)"
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
      <button class="download-btn" title="下载到目录" @click.stop="downloadItem(entry.item.id)">
        <el-icon>
          <Download/>
        </el-icon>
      </button>
      <button class="fullscreen-btn" title="全屏预览" @click.stop="openFullscreen(entry.item.id)">
        <el-icon>
          <FullScreen/>
        </el-icon>
      </button>
      <button :class="{ active: isPinned(entry.item.id) }" class="pin-btn" title="置顶"
              @click.stop="promoteItem(entry.item.id)">
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
             @dragstart.prevent/>
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
    <div aria-hidden="true" class="spacer"></div>
  </div>
</template>

<script setup>
import {computed, onUnmounted, ref} from 'vue'
import {Close, Download, FullScreen, Loading} from '@element-plus/icons-vue'
import {Pin} from 'lucide-vue-next'

const props = defineProps({
  visibleHistory: {
    type: Array,
    required: true
  },
  selectedIndex: {
    type: Number,
    required: true
  },
  isCtrlKeyPressed: {
    type: Boolean,
    default: false
  },
  deleteItem: {
    type: Function,
    required: true
  },
  selectByIndex: {
    type: Function,
    required: true
  },
  fillById: {
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
  },
  handleItemHover: {
    type: Function,
    required: true
  },
  showContextMenu: {
    type: Function,
    required: true
  },
  isPinned: {
    type: Function,
    required: true
  },
  promoteItem: {
    type: Function,
    required: true
  },
  downloadItem: {
    type: Function,
    required: true
  },
  openFullscreen: {
    type: Function,
    required: true
  },
  getItemCategory: {
    type: Function,
    required: true
  },
  getItemTags: {
    type: Function,
    required: true
  },
  getPreviewDataUrl: {
    type: Function,
    required: true
  },
  hasMore: {
    type: Boolean,
    default: false
  },
  isLoadingPage: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(['content-scroll', 'load-more-intent'])

const contentRef = ref(null)
let isDown = false
let isDragging = false
let startX = 0
let scrollLeftVal = 0
let dragTargetScrollLeft = 0
let dragScrollRafId = 0

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)
const showTailLoadMoreHint = computed(() => (props.hasMore || isLoadingMore.value) && props.visibleHistory.length > 0)

const handleScroll = () => {
  emit('content-scroll')
}

const stopDragging = () => {
  if (!isDown) return
  isDown = false
  isDragging = false
  if (dragScrollRafId) {
    cancelAnimationFrame(dragScrollRafId)
    dragScrollRafId = 0
  }
  if (contentRef.value) {
    contentRef.value.classList.remove('is-dragging')
    contentRef.value.style.cursor = 'default'
  }
  document.body.style.removeProperty('user-select')
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
  window.removeEventListener('dragend', handleGlobalDragEnd)
}

const handleClick = (entryIndex) => {
  props.selectByIndex(entryIndex)
}

const handleDoubleClick = (itemId) => {
  props.fillById(itemId)
}

const handleMouseDown = (event) => {
  if (event.button !== 0) return
  if (
      event.target.closest('.delete-btn')
      || event.target.closest('.download-btn')
      || event.target.closest('.fullscreen-btn')
      || event.target.closest('.pin-btn')
  ) {
    return
  }
  isDown = true
  isDragging = false
  startX = event.pageX
  if (contentRef.value) {
    scrollLeftVal = contentRef.value.scrollLeft
    dragTargetScrollLeft = scrollLeftVal
  }
  window.addEventListener('mousemove', handleGlobalMouseMove)
  window.addEventListener('mouseup', handleGlobalMouseUp, true)
  window.addEventListener('dragend', handleGlobalDragEnd)
}

const handleGlobalMouseUp = () => {
  stopDragging()
}

const handleGlobalDragEnd = () => {
  stopDragging()
}

const handleGlobalMouseMove = (event) => {
  if (!isDown || !contentRef.value) return
  const walk = event.pageX - startX
  if (!isDragging && Math.abs(walk) > 4) {
    isDragging = true
    contentRef.value.style.cursor = 'grabbing'
    contentRef.value.classList.add('is-dragging')
    document.body.style.userSelect = 'none'
  }
  if (!isDragging) return
  dragTargetScrollLeft = scrollLeftVal - walk
  const maxScrollLeft = Math.max(0, contentRef.value.scrollWidth - contentRef.value.clientWidth)
  if (dragTargetScrollLeft > maxScrollLeft + 36) {
    emit('load-more-intent')
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

const handleWheel = (event) => {
  if (!contentRef.value) return
  const delta = Math.abs(event.deltaY) >= Math.abs(event.deltaX) ? event.deltaY : event.deltaX
  const maxScrollLeft = Math.max(0, contentRef.value.scrollWidth - contentRef.value.clientWidth)
  const nearEnd = contentRef.value.scrollLeft >= maxScrollLeft - 8
  if (delta > 0 && nearEnd) {
    emit('load-more-intent')
  }
  contentRef.value.scrollLeft += delta
}

const handleVisibilityChange = () => {
  if (document.hidden) {
    stopDragging()
  }
}

window.addEventListener('blur', stopDragging)
document.addEventListener('visibilitychange', handleVisibilityChange)

onUnmounted(() => {
  stopDragging()
  window.removeEventListener('blur', stopDragging)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
  window.removeEventListener('dragend', handleGlobalDragEnd)
})

defineExpose({
  contentRef
})
</script>

<style scoped>
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
.content.is-dragging .download-btn,
.content.is-dragging .fullscreen-btn,
.content.is-dragging .pin-btn {
  opacity: 0 !important;
}

.content.is-dragging .clipboard-item {
  pointer-events: none;
}

.spacer {
  flex: 0 0 742px;
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

.pin-btn {
  position: absolute;
  top: 5px;
  right: 77px;
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

.download-btn {
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

.download-btn:hover {
  border-color: #67c23a;
  color: #fff;
  background: #67c23a;
}

.clipboard-item:hover .download-btn {
  opacity: 1;
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
  right: 86px;
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
</style>
