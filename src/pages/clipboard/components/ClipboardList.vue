<template>
  <div
      ref="contentRef"
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
        @scroll.passive="onScroll"
    >
      <template #default="{ item: scrollerItem }">
        <div
            :id="'clipboard-item-' + scrollerItem.id"
            :class="{ selected: selectedItemId === scrollerItem.id, pinned: isPinned(scrollerItem.id) }"
            :draggable="isCtrlKeyPressed"
            class="clipboard-item"
            @click="handleClick(scrollerItem.id)"
            @dblclick="handleDoubleClick(scrollerItem.id)"
            @dragend="handleDragEnd"
            @dragstart="handleItemDragStart($event, scrollerItem.id)"
            @contextmenu.prevent="showContextMenu($event, scrollerItem.id, scrollerItem.entryIndex)"
        >
          <div class="item-header">
            <span class="item-index">{{ scrollerItem.displayIndex + 1 }}</span>
            <span class="item-category" @click.stop>{{ translateCategory(getItemCategory(scrollerItem.id)) }}</span>
            <div v-if="isPinned(scrollerItem.id)" class="item-pinned-dot"></div>
            <div class="item-actions">
              <div v-if="isWebUrl(scrollerItem.content)" class="action-btn"
                   @click.stop="openWebUrl(scrollerItem.content)">
                <Link :size="9"/>
              </div>
              <div class="action-btn" @click.stop="emit('preview', scrollerItem.content, scrollerItem.id)">
                <View :size="9"/>
              </div>
              <div :class="{ active: isPinned(scrollerItem.id) }" class="action-btn"
                   @click.stop="promoteItem(scrollerItem.id)">
                <Star :size="9"/>
              </div>
              <div class="action-btn action-delete" @click.stop="deleteItem(scrollerItem.id)">
                <Close :size="9"/>
              </div>
            </div>
          </div>
          <div class="item-body">
            <FormattedContent :content="scrollerItem.content"/>
          </div>
          <div v-if="scrollerItem.snippet" class="item-snippet">
            <template v-for="(part, partIndex) in renderHighlightParts(scrollerItem.snippet)" :key="partIndex">
              <mark v-if="part.hit" class="snippet-hit">{{ part.text }}</mark>
              <span v-else>{{ part.text }}</span>
            </template>
          </div>
        </div>
      </template>
    </RecycleScroller>

    <div v-if="showTailLoadMoreHint" class="load-more">
      <el-icon v-if="isLoadingMore" :size="14" class="is-loading">
        <Loading/>
      </el-icon>
      <span class="load-more-text">
        {{ isLoadingMore ? $t('clipboard.loading') : $t('clipboard.loadMore') }}
      </span>
    </div>
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {Close, Link, Loading, Star, View} from '@element-plus/icons-vue'
import {openUrl as openExternalUrl} from '@tauri-apps/plugin-opener'
import {useI18n} from 'vue-i18n'
import {RecycleScroller} from 'vue-virtual-scroller'
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css'
import FormattedContent from '../../../components/FormattedContent.vue'

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
  selectedItemId: {type: String, required: true},
  getItemCategory: {type: Function, required: true},
  deleteItem: {type: Function, required: true},
  updateSelection: {type: Function, required: true},
  selectAndFillDirect: {type: Function, required: true},
  showContextMenu: {type: Function, required: true},
  handleDragStart: {type: Function, required: true},
  handleDragEnd: {type: Function, required: true},
  promoteItem: {type: Function, required: true},
  isPinned: {type: Function, required: true},
  isCtrlKeyPressed: {type: Boolean, default: false},
  highlightKeyword: {type: String, default: ''},
  hasMore: {type: Boolean, default: false},
  isLoadingPage: {type: Boolean, default: false}
})
const emit = defineEmits(['content-scroll', 'load-more-intent', 'preview'])

const contentRef = ref(null)
const scrollerRef = ref(null)

const getScrollEl = () => {
  if (!scrollerRef.value?.$el) return null
  return scrollerRef.value.$el.querySelector('.vue-recycle-scroller') || scrollerRef.value.$el
}

const scrollerItems = computed(() => {
  return props.visibleHistory.map((entry, index) => ({
    id: entry.id,
    content: entry.content,
    snippet: entry.snippet || '',
    entryIndex: index,
    displayIndex: index,
  }))
})

let isDown = false
let isDragging = false
let startX = 0
let scrollLeftStart = 0
let dragRafId = 0

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

const onMouseDown = (e) => {
  // Ignore clicks on action buttons
  if (e.target.closest('.action-btn')) return

  isDown = true
  isDragging = false
  startX = e.pageX
  scrollLeftStart = getScrollEl()?.scrollLeft || 0

  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
}

const onMouseMove = (e) => {
  if (!isDown) return
  const el = getScrollEl()
  if (!el) return

  const walk = e.pageX - startX

  // Start dragging after threshold
  if (!isDragging && Math.abs(walk) > 4) {
    isDragging = true
    document.body.style.userSelect = 'none'
    if (window.getSelection) window.getSelection().removeAllRanges()
  }

  if (!isDragging) return

  e.preventDefault()
  const newScroll = scrollLeftStart - walk
  const max = Math.max(0, el.scrollWidth - el.clientWidth)

  // Trigger load more when near end
  if (newScroll >= max - 260 && props.hasMore && !props.isLoadingPage) {
    emit('load-more-intent')
  }

  // Throttle scroll updates with RAF
  if (!dragRafId) {
    dragRafId = requestAnimationFrame(() => {
      dragRafId = 0
      el.scrollLeft = newScroll
    })
  }
}

const onMouseUp = () => stopDragging()

onMounted(() => {
  window.addEventListener('blur', stopDragging)
})
onUnmounted(() => {
  stopDragging()
  window.removeEventListener('blur', stopDragging)
})

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)

const showTailLoadMoreHint = computed(() => (props.hasMore || isLoadingMore.value) && props.visibleHistory.length > 0)

const onScroll = () => {
  emit('content-scroll')
}

const renderHighlightParts = (text) => {
  const value = typeof text === 'string' ? text : ''
  const keyword = (props.highlightKeyword || '').trim()

  // Fast path: no keyword or empty text
  if (!value || !keyword) return [{text: value, hit: false}]

  // Parse and dedupe tokens, sort by length descending for greedy matching
  const tokens = Array.from(new Set(keyword.split(/\s+/).map(v => v.trim()).filter(Boolean)))
      .sort((a, b) => b.length - a.length)

  if (tokens.length === 0) return [{text: value, hit: false}]

  const sourceLower = value.toLowerCase()
  const tokenLowers = tokens.map(t => t.toLowerCase())
  const out = []
  let start = 0

  while (start < value.length) {
    let bestIndex = -1
    let bestTokenLength = 0

    // Find earliest and longest matching token
    for (let i = 0; i < tokenLowers.length; i++) {
      const idx = sourceLower.indexOf(tokenLowers[i], start)
      if (idx === -1) continue
      if (bestIndex === -1 || idx < bestIndex || (idx === bestIndex && tokenLowers[i].length > bestTokenLength)) {
        bestIndex = idx
        bestTokenLength = tokenLowers[i].length
      }
    }

    if (bestIndex === -1) {
      out.push({text: value.slice(start), hit: false})
      break
    }

    if (bestIndex > start) {
      out.push({text: value.slice(start, bestIndex), hit: false})
    }

    out.push({text: value.slice(bestIndex, bestIndex + bestTokenLength), hit: true})
    start = bestIndex + bestTokenLength
  }

  return out.length > 0 ? out : [{text: value, hit: false}]
}

const handleItemDragStart = (e, id) => {
  if (typeof props.handleDragStart === 'function') props.handleDragStart(e, id)
}
const handleClick = (entryId) => props.updateSelection(entryId, false, getScrollEl(), null)
const handleDoubleClick = (entryId) => props.selectAndFillDirect(entryId)

const isWebUrl = (v) => {
  if (!v) return false
  const t = v.trim()
  return /^https?:\/\/\S+$/i.test(t) || /^www\.\S+$/i.test(t)
}
const normalizeUrl = (v) => {
  const t = v.trim()
  if (/^https?:\/\//i.test(t)) return t
  if (/^www\./i.test(t)) return `https://${t}`
  return t
}
const openWebUrl = async (v) => {
  try {
    if (isWebUrl(v)) await openExternalUrl(normalizeUrl(v))
  } catch (e) {
    console.error('打开网址失败:', e)
  }
}

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

.action-btn.active {
  color: var(--fy-warning);
}

.action-delete:hover {
  color: var(--fy-danger);
  background: rgba(248, 113, 113, 0.1);
}

.item-body {
  flex: 1;
  min-height: 0;
  padding: 6px 12px;
  font-size: var(--fy-text-sm);
  line-height: 1.55;
  color: var(--fy-text-secondary);
  white-space: pre-wrap;
  word-break: break-all;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: none;
}

.item-body::-webkit-scrollbar {
  display: none
}

.item-snippet {
  margin: 0 12px 8px;
  padding: 6px 0 0;
  border-top: 0.5px solid rgba(255, 255, 255, 0.05);
  font-size: 10px;
  line-height: 1.4;
  color: var(--fy-text-accent);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 40px;
  overflow: hidden;
}
.snippet-hit {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
  border-radius: 2px;
  padding: 0 2px;
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
