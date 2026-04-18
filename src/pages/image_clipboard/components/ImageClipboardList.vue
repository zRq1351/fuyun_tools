<template>
  <div
      v-bind="containerProps"
      class="content"
      @mousedown="handleMouseDown"
      @scroll="handleScroll"
      @wheel.prevent="handleWheel"
  >
    <div
        class="virtual-wrapper"
        :style="{
          width: (parseFloat(wrapperProps.style?.width || 0) + parseFloat(wrapperProps.style?.marginLeft || 0)) + 'px',
          paddingLeft: wrapperProps.style?.marginLeft || '0px',
          boxSizing: 'border-box'
        }"
    >
      <div
          v-for="virtualRow in list"
          :id="`image-item-${virtualRow.data.index}`"
          :key="virtualRow.data.item.id"
          v-memo="[
            virtualRow.data.item.id,
            selectedIndex === virtualRow.data.index,
            virtualRow.data.pinned,
            virtualRow.data.category,
            virtualRow.data.tags.join('|'),
            virtualRow.data.item.preview_png_base64,
            virtualRow.data.item.image_path
          ]"
          :class="{ selected: selectedIndex === virtualRow.data.index }"
          :draggable="isCtrlKeyPressed"
          class="clipboard-item"
          @click="handleClick(virtualRow.data.index)"
          @dblclick="handleDoubleClick(virtualRow.data.item.id)"
          @dragend="handleDragEnd"
          @dragstart="handleDragStart($event, virtualRow.data.item.id)"
          @mouseenter="handleItemHover(virtualRow.data.index)"
          @contextmenu.prevent="showContextMenu($event, virtualRow.data.item.id)"
      >
        <div class="delete-btn" @click.stop="deleteItem(virtualRow.data.item.id, virtualRow.data.index)">
          <el-icon>
            <Close/>
          </el-icon>
        </div>
        <button class="download-btn" title="下载到目录" @click.stop="downloadItem(virtualRow.data.item.id)">
          <el-icon>
            <Download/>
          </el-icon>
        </button>
        <button class="fullscreen-btn" title="全屏预览" @click.stop="openFullscreen(virtualRow.data.item.id)">
          <el-icon>
            <FullScreen/>
          </el-icon>
        </button>
        <button :class="{ active: virtualRow.data.pinned }" class="pin-btn" title="置顶"
                @click.stop="promoteItem(virtualRow.data.item.id)">
          <Pin class="pin-lucide"/>
        </button>
        <div class="index-tools">
          <div class="index">{{ virtualRow.data.index + 1 }}</div>
        </div>
        <div class="category-wrap">
          <div class="category-chip">{{ virtualRow.data.category }}</div>
        </div>
        <div class="tag-wrap">
          <div v-if="virtualRow.data.tags.length" class="tag-chip-list">
            <span v-for="tag in virtualRow.data.tags" :key="`${virtualRow.data.item.id}-${tag}`" class="tag-chip">#{{
                tag
              }}</span>
          </div>
          <div v-else class="tag-chip-empty">无标签</div>
        </div>
        <div class="item-content">
          <img :src="getPreviewDataUrl(virtualRow.data.item)" alt="" class="image-preview" decoding="async" draggable="false"
               @dragstart.prevent/>
          <div class="image-meta">{{ virtualRow.data.item.width }} × {{ virtualRow.data.item.height }}</div>
        </div>
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
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {Close, Download, FullScreen, Loading} from '@element-plus/icons-vue'
import {Pin} from 'lucide-vue-next'
import {useVirtualList} from '@vueuse/core'

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

const visibleHistoryComputed = computed(() => props.visibleHistory)
const { list, containerProps, wrapperProps } = useVirtualList(visibleHistoryComputed, {
  itemWidth: 258,
  overscan: 10
})

const contentRef = containerProps.ref
let isDown = false
let isDragging = false
let startX = 0
let scrollLeftVal = 0
let dragTargetScrollLeft = 0
let dragScrollRafId = 0

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)
const showTailLoadMoreHint = computed(() => (props.hasMore || isLoadingMore.value) && props.visibleHistory.length > 0)

let scrollRafId = 0
const handleScroll = (e) => {
  containerProps.onScroll?.(e)
  if (!scrollRafId) {
    scrollRafId = requestAnimationFrame(() => {
      emit('content-scroll')
      scrollRafId = 0
    })
  }
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

onMounted(() => {
  window.addEventListener('blur', stopDragging)
  document.addEventListener('visibilitychange', handleVisibilityChange)
})

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
  flex-direction: row;
  padding: 8px 8px 8px 8px;
  overflow-x: auto;
  overflow-y: hidden;
  margin-top: 10px;
  scrollbar-width: none;
}

.virtual-wrapper {
  display: flex;
  flex-direction: row;
  height: 100%;
  flex-shrink: 0;
}

.content::after {
  content: '';
  flex-shrink: 0;
  width: 1px;
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

.load-more-tail-indicator {
  flex-shrink: 0;
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
  margin-right: 8px;
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
  margin-right: 8px;
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
