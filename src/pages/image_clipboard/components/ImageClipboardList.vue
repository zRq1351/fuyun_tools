<template>
  <div
      ref="contentRef"
      class="content"
      @mousedown="onMouseDown"
      @scroll="onScroll"
  >
    <div ref="trackRef" class="scroll-track" :style="{ paddingRight: rightPadding + 'px' }">
      <div
          v-for="entry in visibleHistory"
          :id="`image-item-${entry.index}`"
          :key="entry.item.id"
          v-memo="[
            entry.item.id,
            entry.index,
            selectedIndex === entry.index,
            entry.pinned,
            entry.category,
            entry.tags,
            entry.item.preview_png_base64,
            entry.item.image_path
          ]"
          :class="{ selected: selectedIndex === entry.index, pinned: entry.pinned }"
          :draggable="isCtrlKeyPressed"
          class="clipboard-item"
          @click="handleClick(entry.index)"
          @dblclick="handleDoubleClick(entry.item.id)"
          @dragend="handleDragEnd"
          @dragstart="handleDragStart($event, entry.item.id)"
          @mouseenter="handleItemHover(entry.index)"
          @contextmenu.prevent="showContextMenu($event, entry.item.id)"
      >
        <div class="item-header">
          <span class="item-index">{{ entry.index + 1 }}</span>
          <span class="item-category" @click.stop>{{ entry.category }}</span>
          <div v-if="entry.pinned" class="item-pinned-dot"></div>
          <div class="item-actions">
            <div class="action-btn" @click.stop="openFullscreen(entry.item.id)">
              <el-icon :size="9"><FullScreen/></el-icon>
            </div>
            <div class="action-btn" @click.stop="downloadItem(entry.item.id)">
              <el-icon :size="9"><Download/></el-icon>
            </div>
            <div :class="{ active: entry.pinned }" class="action-btn" @click.stop="promoteItem(entry.item.id)">
              <Star :size="9"/>
            </div>
            <div class="action-btn action-delete" @click.stop="deleteItem(entry.item.id, entry.index)">
              <el-icon :size="9"><Close/></el-icon>
            </div>
          </div>
        </div>
        <div class="item-content">
          <img :src="getPreviewDataUrl(entry.item)" alt="" class="image-preview" decoding="async" draggable="false" @dragstart.prevent/>
        </div>
        <div v-if="entry.tags.length" class="tag-wrap">
          <div class="tag-chip-list">
            <span v-for="tag in entry.tags" :key="`${entry.item.id}-${tag}`" class="tag-chip">#{{ tag }}</span>
          </div>
        </div>
        <div class="image-meta">{{ entry.item.width }} × {{ entry.item.height }}</div>
      </div>

      <div v-if="showTailLoadMoreHint" class="load-more">
        <el-icon v-if="isLoadingMore" :size="14" class="is-loading"><Loading/></el-icon>
        <span class="load-more-text">{{ isLoadingMore ? $t('imageClipboard.loading') : $t('imageClipboard.loadMore') }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onMounted, onUnmounted, ref, watch} from 'vue'
import {Close, Download, FullScreen, Loading, Star} from '@element-plus/icons-vue'

const props = defineProps({
  visibleHistory: {type: Array, required: true},
  selectedIndex: {type: Number, required: true},
  isCtrlKeyPressed: {type: Boolean, default: false},
  deleteItem: {type: Function, required: true},
  selectByIndex: {type: Function, required: true},
  fillById: {type: Function, required: true},
  handleDragStart: {type: Function, required: true},
  handleDragEnd: {type: Function, required: true},
  handleItemHover: {type: Function, required: true},
  showContextMenu: {type: Function, required: true},
  promoteItem: {type: Function, required: true},
  downloadItem: {type: Function, required: true},
  openFullscreen: {type: Function, required: true},
  getPreviewDataUrl: {type: Function, required: true},
  hasMore: {type: Boolean, default: false},
  isLoadingPage: {type: Boolean, default: false}
})

const emit = defineEmits(['content-scroll', 'load-more-intent'])

const contentRef = ref(null)
const trackRef = ref(null)
const rightPadding = ref(0)

const ensureLastCardVisible = () => {
  const el = contentRef.value
  if (!el) return
  const lastCard = el.querySelector('.clipboard-item:last-of-type')
  if (!lastCard) return
  const lastCardRight = lastCard.offsetLeft + lastCard.offsetWidth
  const needed = lastCardRight - el.scrollWidth + el.clientWidth
  if (needed > 0) rightPadding.value = needed
}

let isDown = false
let isDragging = false
let startX = 0
let scrollLeftStart = 0
let dragRafId = 0

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)
const showTailLoadMoreHint = computed(() => (props.hasMore || isLoadingMore.value) && props.visibleHistory.length > 0)

const onScroll = () => {
  emit('content-scroll')
  ensureLastCardVisible()
}

const stopDragging = () => {
  if (!isDown) return
  isDown = false
  isDragging = false
  if (dragRafId) {
    cancelAnimationFrame(dragRafId)
    dragRafId = 0
  }
  document.body.style.removeProperty('user-select')
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onMouseUp)
}

const onMouseDown = (event) => {
  if (event.button !== 0) return
  if (event.target.closest('.action-btn')) return
  isDown = true
  isDragging = false
  startX = event.pageX
  scrollLeftStart = contentRef.value?.scrollLeft || 0
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
}

const onMouseMove = (event) => {
  if (!isDown || !contentRef.value) return
  const walk = event.pageX - startX
  if (!isDragging && Math.abs(walk) > 4) {
    isDragging = true
    document.body.style.userSelect = 'none'
    if (window.getSelection) window.getSelection().removeAllRanges()
  }
  if (!isDragging) return
  event.preventDefault()
  if (!dragRafId) {
    dragRafId = requestAnimationFrame(() => {
      dragRafId = 0
      if (contentRef.value) contentRef.value.scrollLeft = scrollLeftStart - walk
    })
  }
}

const onMouseUp = () => stopDragging()

const handleClick = (entryIndex) => props.selectByIndex(entryIndex)
const handleDoubleClick = (itemId) => props.fillById(itemId)

onMounted(() => {
  window.addEventListener('blur', stopDragging)
  nextTick(ensureLastCardVisible)
})
onUnmounted(() => {
  stopDragging()
  window.removeEventListener('blur', stopDragging)
})

watch(() => props.visibleHistory.length, () => nextTick(ensureLastCardVisible))

defineExpose({contentRef})
</script>

<style scoped>
.content {
  flex: 1;
  min-height: 0;
  overflow-x: auto;
  cursor: grab;
}

.content::-webkit-scrollbar {
  display: none
}

.scroll-track {
  display: inline-flex;
  flex-direction: row;
  align-items: center;
  white-space: nowrap;
  padding: 0 0 0 14px;
  height: 100%;
  gap: 10px;
}

.clipboard-item {
  display: inline-flex;
  flex-direction: column;
  width: 260px;
  height: 250px;
  white-space: normal;
  flex-shrink: 0;
  background: rgba(255, 255, 255, 0.04);
  border: 0.5px solid rgba(255, 255, 255, 0.07);
  border-radius: 14px;
  padding: 0;
  cursor: pointer;
  position: relative;
  user-select: none;
  backdrop-filter: blur(40px) saturate(180%);
  -webkit-backdrop-filter: blur(40px) saturate(180%);
  color: var(--fy-text-primary);
  transition: all 0.2s ease;
  overflow: hidden;
}

.clipboard-item:hover {
  background: rgba(255, 255, 255, 0.07);
  border-color: rgba(255, 255, 255, 0.12);
}

.clipboard-item.selected {
  background: rgba(108, 140, 255, 0.1);
  border-color: rgba(108, 140, 255, 0.4);
}

.item-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px 0;
}

.item-index {
  font-size: 10px;
  font-family: var(--fy-font-mono);
  color: var(--fy-text-muted);
  opacity: 0.5;
  flex: 0 0 auto;
  transition: opacity 0.2s;
}

.clipboard-item:hover .item-index {
  opacity: 1;
  color: var(--fy-accent)
}

.item-category {
  font-size: 10px;
  color: var(--fy-text-muted);
  opacity: 0.6;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
  transition: opacity 0.2s;
}

.clipboard-item:hover .item-category {
  opacity: 1
}

.item-pinned-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--fy-warning);
  flex: 0 0 auto;
}

.item-actions {
  display: flex;
  align-items: center;
  gap: 1px;
  flex: 0 0 auto;
  opacity: 0;
  transition: opacity 0.15s;
}

.clipboard-item:hover .item-actions,
.clipboard-item.selected .item-actions {
  opacity: 1
}

.action-btn {
  width: 16px;
  height: 16px;
  border-radius: 3px;
  background: transparent;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.15s ease;
}

.action-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--fy-text-primary);
}

.action-delete:hover {
  color: var(--fy-danger);
  background: rgba(248, 113, 113, 0.1);
}

.item-content {
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 4px;
  overflow: hidden;
}

.image-preview {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  border-radius: 4px;
}

.tag-wrap {
  padding: 0 8px 4px;
}

.tag-chip-list {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.tag-chip {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.08);
  color: var(--fy-text-accent);
}

.image-meta {
  padding: 0 10px 6px;
  font-size: 10px;
  color: var(--fy-text-muted);
  opacity: 0.6;
}

.load-more {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 250px;
  gap: 4px;
  color: var(--fy-text-muted);
  user-select: none;
}

.load-more-text {
  font-size: 10px;
  color: var(--fy-text-muted);
}
</style>
