<template>
  <div
      class="content"
      @mousedown="onMouseDown"
      @scroll="onScroll"
  >
    <RecycleScroller
        ref="scrollerRef"
        :buffer="SCROLL_BUFFER"
        :item-size="SCROLL_ITEM_SIZE"
        :items="scrollerItems"
        class="scroll-track"
        direction="horizontal"
        key-field="id"
        @scroll.native="onScroll"
    >
      <template #default="{ item: scrollerItem }">
        <div
            :id="`image-item-${scrollerItem.entryIndex}`"
            :class="{ selected: selectedIndex === scrollerItem.entryIndex, pinned: scrollerItem.pinned }"
            :draggable="isCtrlKeyPressed"
            class="clipboard-item"
            @click="handleClick(scrollerItem.entryIndex)"
            @dblclick="handleDoubleClick(scrollerItem.id)"
            @dragend="handleDragEnd"
            @dragstart="handleDragStart($event, scrollerItem.id)"
            @mouseenter="handleItemHover(scrollerItem.entryIndex)"
            @contextmenu.prevent="showContextMenu($event, scrollerItem.id)"
        >
          <div class="item-header">
            <span class="item-index">{{ scrollerItem.entryIndex + 1 }}</span>
            <span class="item-category" @click.stop>{{ translateCategory(scrollerItem.category) }}</span>
            <div v-if="scrollerItem.pinned" class="item-pinned-dot"></div>
            <div class="item-actions">
              <div class="action-btn" @click.stop="openFullscreen(scrollerItem.id)">
                <el-icon :size="9">
                  <FullScreen/>
                </el-icon>
              </div>
              <div class="action-btn" @click.stop="downloadItem(scrollerItem.id)">
                <el-icon :size="9">
                  <Download/>
                </el-icon>
              </div>
              <div :class="{ active: scrollerItem.pinned }" class="action-btn"
                   @click.stop="promoteItem(scrollerItem.id)">
                <Star :size="9"/>
              </div>
              <div class="action-btn action-delete" @click.stop="deleteItem(scrollerItem.id, scrollerItem.entryIndex)">
                <el-icon :size="9">
                  <Close/>
                </el-icon>
              </div>
            </div>
          </div>
          <div class="item-content">
            <img :src="getPreviewDataUrl(scrollerItem.rawItem)" alt="" class="image-preview" decoding="async"
                 draggable="false" @dragstart.prevent/>
          </div>
          <div v-if="scrollerItem.tags.length" class="tag-wrap">
            <div class="tag-chip-list">
              <span v-for="tag in scrollerItem.tags" :key="`${scrollerItem.id}-${tag}`"
                    class="tag-chip">#{{ tag }}</span>
            </div>
          </div>
          <div class="image-meta">{{ scrollerItem.rawItem.width }} × {{ scrollerItem.rawItem.height }}</div>
        </div>
      </template>
    </RecycleScroller>

    <div v-if="showTailLoadMoreHint" class="load-more">
      <el-icon v-if="isLoadingMore" :size="14" class="is-loading">
        <Loading/>
      </el-icon>
      <span
          class="load-more-text">{{ isLoadingMore ? $t('imageClipboard.loading') : $t('imageClipboard.loadMore') }}</span>
    </div>
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {Close, Download, FullScreen, Loading, Star} from '@element-plus/icons-vue'
import {useI18n} from 'vue-i18n'
import {RecycleScroller} from 'vue-virtual-scroller'
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css'

const {t} = useI18n()

const CATEGORY_TRANSLATIONS = {
  '未分类': () => t('common.uncategorized'),
  '全部': () => t('common.all'),
}

const translateCategory = (category) => {
  const translator = CATEGORY_TRANSLATIONS[category]
  return translator ? translator() : category
}

const SCROLL_ITEM_SIZE = 270
const SCROLL_BUFFER = 540

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
const scrollerRef = ref(null)

const getScrollEl = () => {
  if (!scrollerRef.value?.$el) return null
  return scrollerRef.value.$el.querySelector('.vue-recycle-scroller') || scrollerRef.value.$el
}

const scrollerItems = computed(() => {
  return props.visibleHistory.map((entry) => ({
    id: entry.item.id,
    rawItem: entry.item,
    entryIndex: entry.index,
    pinned: entry.pinned,
    category: entry.category,
    tags: entry.tags,
  }))
})

let isDown = false
let isDragging = false
let startX = 0
let scrollLeftStart = 0
let dragRafId = 0

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)
const showTailLoadMoreHint = computed(() => (props.hasMore || isLoadingMore.value) && props.visibleHistory.length > 0)

const onScroll = () => {
  emit('content-scroll')
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
  scrollLeftStart = getScrollEl()?.scrollLeft || 0
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
}

const onMouseMove = (event) => {
  if (!isDown) return
  const el = getScrollEl()
  if (!el) return
  const walk = event.pageX - startX
  if (!isDragging && Math.abs(walk) > 4) {
    isDragging = true
    document.body.style.userSelect = 'none'
    if (window.getSelection) window.getSelection().removeAllRanges()
  }
  if (!isDragging) return
  event.preventDefault()
  const newScroll = scrollLeftStart - walk
  const max = Math.max(0, el.scrollWidth - el.clientWidth)
  if (newScroll >= max - 260 && props.hasMore && !props.isLoadingPage) {
    emit('load-more-intent')
  }
  if (!dragRafId) {
    dragRafId = requestAnimationFrame(() => {
      dragRafId = 0
      el.scrollLeft = newScroll
    })
  }
}

const onMouseUp = () => stopDragging()

const handleClick = (entryIndex) => props.selectByIndex(entryIndex)
const handleDoubleClick = (itemId) => props.fillById(itemId)

onMounted(() => {
  window.addEventListener('blur', stopDragging)
})
onUnmounted(() => {
  stopDragging()
  window.removeEventListener('blur', stopDragging)
})

defineExpose({contentRef, scrollerRef, getScrollEl})
</script>

<style scoped>
.content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  cursor: grab;
}

.scroll-track {
  width: 100%;
  height: 100%;
}

.scroll-track :deep(.vue-recycle-scroller) {
  height: 100%;
}

.scroll-track :deep(.vue-recycle-scroller__item-wrapper) {
  display: inline-flex;
  align-items: center;
  padding: 0 0 0 14px;
  gap: 10px;
  height: 100%;
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
