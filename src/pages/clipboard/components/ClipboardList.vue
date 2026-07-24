<template>
  <div
      ref="contentRef"
      class="content"
      @mousedown="onMouseDown"
      @scroll="handleScroll"
  >
    <div class="scroll-track" :style="{ paddingRight: rightPadding + 'px' }">
      <div
          v-for="(entry, index) in visibleHistory"
          :id="'clipboard-item-' + entry.id"
          :key="entry.id"
          v-memo="[entry.content, index, selectedItemId, getItemCategory(entry.id), isPinned(entry.id), entry.snippet]"
          :class="{ selected: selectedItemId === entry.id, pinned: isPinned(entry.id) }"
          class="clipboard-item"
          :draggable="isCtrlKeyPressed"
          @click="handleClick(entry.id)"
          @dblclick="handleDoubleClick(entry.id)"
          @contextmenu.prevent="showContextMenu($event, entry.id, index)"
          @dragstart="handleItemDragStart($event, entry.id)"
          @dragend="handleDragEnd"
      >
        <div class="item-header">
          <span class="item-index">{{ index + 1 }}</span>
          <span class="item-category" @click.stop>{{ getItemCategory(entry.id) }}</span>
          <div v-if="isPinned(entry.id)" class="item-pinned-dot"></div>
          <div class="item-actions">
            <div v-if="isWebUrl(entry.content)" class="action-btn" @click.stop="openWebUrl(entry.content)">
              <Link :size="9"/>
            </div>
            <div class="action-btn" @click.stop="emit('preview', entry.content, entry.id)">
              <View :size="9"/>
            </div>
            <div :class="{ active: isPinned(entry.id) }" class="action-btn"
                 @click.stop="promoteItem(entry.id)">
              <Star :size="9"/>
            </div>
            <div class="action-btn action-delete" @click.stop="deleteItem(entry.id)">
              <Close :size="9"/>
            </div>
          </div>
        </div>
        <div class="item-body">
          <FormattedContent :content="entry.content" />
        </div>
        <div v-if="entry.snippet" class="item-snippet">
          <template v-for="(part, partIndex) in renderHighlightParts(entry.snippet)" :key="partIndex">
            <mark v-if="part.hit" class="snippet-hit">{{ part.text }}</mark>
            <span v-else>{{ part.text }}</span>
          </template>
        </div>
      </div>

      <div v-if="showTailLoadMoreHint" class="load-more">
        <el-icon v-if="isLoadingMore" :size="14" class="is-loading">
          <Loading/>
        </el-icon>
        <span class="load-more-text">
          {{ isLoadingMore ? $t('clipboard.loading') : $t('clipboard.loadMore') }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onMounted, onUnmounted, ref, watch} from 'vue'
import {Close, Link, Loading, Star, View} from '@element-plus/icons-vue'
import {openUrl as openExternalUrl} from '@tauri-apps/plugin-opener'
import FormattedContent from '../../../components/FormattedContent.vue'

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
const rightPadding = ref(0)

const ensureLastCardVisible = () => {
  const el = contentRef.value
  if (!el) return
  const lastCard = el.querySelector('.clipboard-item:last-of-type')
  if (!lastCard) return
  const cardRight = lastCard.offsetLeft + lastCard.offsetWidth
  const visibleRight = el.scrollLeft + el.clientWidth
  if (cardRight > visibleRight) {
    rightPadding.value = cardRight - visibleRight + 20
  }
}
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
  if (contentRef.value) contentRef.value.style.cursor = 'grab'
  document.body.style.removeProperty('user-select')
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onMouseUp)
}

const onMouseDown = (e) => {
  if (e.target.closest('.action-btn')) return
  isDown = true
  isDragging = false
  startX = e.pageX
  scrollLeftStart = contentRef.value?.scrollLeft || 0
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
}

const onMouseMove = (e) => {
  if (!isDown || !contentRef.value) return
  const walk = e.pageX - startX
  if (!isDragging && Math.abs(walk) > 4) {
    isDragging = true
    document.body.style.userSelect = 'none'
    if (window.getSelection) window.getSelection().removeAllRanges()
  }
  if (!isDragging) return
  e.preventDefault()
  if (!dragRafId) {
    dragRafId = requestAnimationFrame(() => {
      dragRafId = 0
      if (contentRef.value) contentRef.value.scrollLeft = scrollLeftStart - walk
    })
  }
}

const onMouseUp = () => stopDragging()

onMounted(() => {
  window.addEventListener('blur', stopDragging)
  setTimeout(ensureLastCardVisible, 300)
})
onUnmounted(() => {
  stopDragging()
  window.removeEventListener('blur', stopDragging)
})

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)

watch(() => props.visibleHistory.length, () => {
  nextTick(ensureLastCardVisible)
})
const showTailLoadMoreHint = computed(() => (props.hasMore || isLoadingMore.value) && props.visibleHistory.length > 0)

const handleScroll = () => {
  emit('content-scroll')
  if (!contentRef.value || !props.hasMore || props.isLoadingPage) return
  const {scrollWidth, scrollLeft, clientWidth} = contentRef.value
  if (scrollWidth - scrollLeft - clientWidth < 260) {
    emit('load-more-intent')
  }
}

const renderHighlightParts = (text) => {
  const value = typeof text === 'string' ? text : ''
  const keyword = (props.highlightKeyword || '').trim()
  const tokens = Array.from(new Set(keyword.split(/\s+/).map((v) => v.trim()).filter(Boolean)))
      .sort((a, b) => b.length - a.length)
  if (!value || tokens.length === 0) return [{text: value, hit: false}]
  const sourceLower = value.toLowerCase()
  const tokenLowers = tokens.map((t) => t.toLowerCase())
  const out = []
  let start = 0
  while (start < value.length) {
    let bestIndex = -1
    let bestToken = ''
    for (let i = 0; i < tokenLowers.length; i++) {
      const idx = sourceLower.indexOf(tokenLowers[i], start)
      if (idx === -1) continue
      if (bestIndex === -1 || idx < bestIndex || (idx === bestIndex && tokenLowers[i].length > bestToken.length)) {
        bestIndex = idx
        bestToken = tokenLowers[i]
      }
    }
    if (bestIndex === -1) {
      out.push({text: value.slice(start), hit: false});
      break
    }
    if (bestIndex > start) out.push({text: value.slice(start, bestIndex), hit: false})
    out.push({text: value.slice(bestIndex, bestIndex + bestToken.length), hit: true})
    start = bestIndex + bestToken.length
  }
  return out.length > 0 ? out : [{text: value, hit: false}]
}

const handleItemDragStart = (e, id) => {
  if (typeof props.handleDragStart === 'function') props.handleDragStart(e, id)
}
const handleClick = (entryId) => props.updateSelection(entryId, false, contentRef.value, null)
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

defineExpose({contentRef})
</script>

<style scoped>
.content {
  flex: 1;
  min-height: 0;
  overflow-x: auto;
  white-space: nowrap;
  scrollbar-width: none;
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
  padding: 0 14px;
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

.clipboard-item:last-child {
  margin-right: 0;
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
  display: inline-block;
  vertical-align: top;
  width: 48px;
  height: 100%;
  white-space: normal;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  color: var(--fy-text-muted);
  user-select: none;
}

.load-more-text {
  font-size: 10px;
  color: var(--fy-text-muted);
}
</style>
