<template>
  <div ref="contentRef" class="content">
    <div ref="stageRef" class="carousel-stage" @mousedown="onStageMouseDown">
      <div
          v-for="(entry, index) in stackItems"
          :id="'clipboard-item-' + entry.id"
          :key="entry.id"
          :class="{ selected: entry.id === selectedItemId, pinned: entry.pinned }"
          :draggable="isCtrlKeyPressed"
          :style="cardStyle(index)"
          class="clipboard-item"
          @click="handleClick(entry.id)"
          @dblclick="handleDoubleClick(entry.id)"
          @dragend="handleDragEnd"
          @dragstart="handleItemDragStart($event, entry.id)"
          @contextmenu.prevent="showContextMenu($event, entry.id, index)"
      >
        <div class="item-header">
          <span class="item-index">{{ index + 1 }}</span>
          <span class="item-category" @click.stop>{{ translateCategory(getItemCategory(entry.id)) }}</span>
          <div v-if="entry.pinned" class="item-pinned-dot"></div>
          <div class="item-actions">
            <div v-if="isWebUrl(entry.content)" class="action-btn" @click.stop="openWebUrl(entry.content)">
              <Link :size="9"/>
            </div>
            <div class="action-btn" @click.stop="emit('preview', entry.content, entry.id)">
              <View :size="9"/>
            </div>
            <div :class="{ active: entry.pinned }" class="action-btn" @click.stop="promoteItem(entry.id)">
              <Star :size="9"/>
            </div>
            <div class="action-btn action-delete" @click.stop="deleteItem(entry.id)">
              <Close :size="9"/>
            </div>
          </div>
        </div>
        <div class="item-body">
          <FormattedContent :content="entry.content"/>
        </div>
        <div v-if="entry.snippet" class="item-snippet">
          <template v-for="(part, partIndex) in renderHighlightParts(entry.snippet)" :key="partIndex">
            <mark v-if="part.hit" class="snippet-hit">{{ part.text }}</mark>
            <span v-else>{{ part.text }}</span>
          </template>
        </div>
      </div>
    </div>

    <div v-if="showLoadMoreHint" class="load-more-bar">
      <el-icon v-if="isLoadingMore" :size="14" class="is-loading"><Loading/></el-icon>
      <span class="load-more-text" @click="emit('load-more-intent')">
        {{ isLoadingMore ? $t('clipboard.loading') : $t('clipboard.loadMore') }}
      </span>
    </div>
  </div>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, ref} from 'vue'
import {Close, Link, Loading, Star, View} from '@element-plus/icons-vue'
import {openUrl as openExternalUrl} from '@tauri-apps/plugin-opener'
import {useI18n} from 'vue-i18n'
import FormattedContent from '../../../components/FormattedContent.vue'

const {t} = useI18n()

const CARD_WIDTH = 260
const CARD_HEIGHT = 250

const CATEGORY_TRANSLATIONS = {
  '未分类': () => t('common.uncategorized'),
  '全部': () => t('common.all'),
}

const translateCategory = (category) => {
  const translator = CATEGORY_TRANSLATIONS[category]
  return translator ? translator() : category
}

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
const emit = defineEmits(['load-more-intent', 'preview'])

const contentRef = ref(null)
const stageRef = ref(null)
const containerWidth = ref(800)

let resizeObserver = null

const selectedIndex = computed(() => {
  const idx = props.visibleHistory.findIndex(e => e.id === props.selectedItemId)
  return idx >= 0 ? idx : 0
})

const stackItems = computed(() =>
  props.visibleHistory.map((entry, index) => ({
    ...entry,
    snippet: entry.snippet || '',
    pinned: props.isPinned(entry.id),
  }))
)

// Calculate visual properties for each card — stacked deck with swipe
const STACK_OFFSET_X = 10
const STACK_OFFSET_Y = 14
const STACK_SCALE_STEP = 0.07
const SWIPE_THRESHOLD = 0.3  // fraction of stage width to trigger navigation

const cardStyle = (index) => {
  const d = index - selectedIndex.value
  const absD = Math.abs(d)

  // Base stack transform (no swipe): cards behind are offset & scaled
  const baseX = d * STACK_OFFSET_X
  const baseY = absD * STACK_OFFSET_Y
  const baseScale = 1 - absD * STACK_SCALE_STEP
  const baseOpacity = absD <= 1 ? 1 - absD * 0.5 : 0.15
  let zIndex = 100 - absD * 15

  const w = containerWidth.value
  const centerX = w / 2 - CARD_WIDTH / 2

  // Apply swipe — focused card follows finger, cards behind shift toward center
  const progress = swipeProgress.value  // -1 to +1
  const swipeAbs = Math.abs(progress)
  const swipeDir = progress > 0 ? 1 : -1  // +1=swipe right (to prev), -1=swipe left (to next)

  const isFocus = d === 0
  const isNext = d === swipeDir  // card being revealed

  let x, y, s, opacity

  if (isFocus) {
    // Focus card follows mouse, fades as it moves away
    x = centerX + progress * w * 0.6
    y = swipeAbs * STACK_OFFSET_Y  // slightly down as it slides
    s = 1 - swipeAbs * 0.08
    opacity = 1 - swipeAbs * 0.5
    zIndex = 100
  } else if (isNext && swipeAbs > 0.05) {
    // Card being revealed: moves from offset toward center, scales up, opacity up
    const t = Math.min(1, swipeAbs / SWIPE_THRESHOLD)
    x = centerX + (1 - t) * swipeDir * STACK_OFFSET_X
    y = (1 - t) * STACK_OFFSET_Y
    s = baseScale + t * (1 - baseScale)
    opacity = baseOpacity + t * (1 - baseOpacity)
    zIndex = 95
  } else {
    // Other cards: stay in stack position
    x = centerX + baseX
    y = baseY
    s = baseScale
    opacity = baseOpacity
  }

  return {
    left: x + 'px',
    top: '50%',
    width: CARD_WIDTH + 'px',
    height: CARD_HEIGHT + 'px',
    transform: `translateY(-50%) translateY(${y}px) scale(${s})`,
    opacity,
    zIndex,
    transition: dragActive ? 'none' : undefined,
  }
}

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)
const showLoadMoreHint = computed(() => props.hasMore || isLoadingMore.value)

const handleDoubleClick = (entryId) => props.selectAndFillDirect(entryId)

const renderHighlightParts = (text) => {
  const value = typeof text === 'string' ? text : ''
  const keyword = (props.highlightKeyword || '').trim()
  if (!value || !keyword) return [{text: value, hit: false}]
  const tokens = Array.from(new Set(keyword.split(/\s+/).map(v => v.trim()).filter(Boolean)))
      .sort((a, b) => b.length - a.length)
  if (tokens.length === 0) return [{text: value, hit: false}]
  const sourceLower = value.toLowerCase()
  const tokenLowers = tokens.map(t => t.toLowerCase())
  const out = []
  let start = 0
  while (start < value.length) {
    let bestIndex = -1, bestLen = 0
    for (let i = 0; i < tokenLowers.length; i++) {
      const idx = sourceLower.indexOf(tokenLowers[i], start)
      if (idx === -1) continue
      if (bestIndex === -1 || idx < bestIndex || (idx === bestIndex && tokenLowers[i].length > bestLen)) {
        bestIndex = idx; bestLen = tokenLowers[i].length
      }
    }
    if (bestIndex === -1) { out.push({text: value.slice(start), hit: false}); break }
    if (bestIndex > start) out.push({text: value.slice(start, bestIndex), hit: false})
    out.push({text: value.slice(bestIndex, bestIndex + bestLen), hit: true})
    start = bestIndex + bestLen
  }
  return out.length > 0 ? out : [{text: value, hit: false}]
}

const handleItemDragStart = (e, id) => {
  if (typeof props.handleDragStart === 'function') props.handleDragStart(e, id)
}

const isWebUrl = (v) => !!v && /^https?:\/\/\S+$/i.test(v.trim()) || /^www\.\S+$/i.test(v.trim())
const openWebUrl = async (v) => {
  try {
    if (!v) return
    const t = v.trim()
    const url = /^https?:\/\//i.test(t) ? t : /^www\./i.test(t) ? `https://${t}` : t
    await openExternalUrl(url)
  } catch (e) { /* ignore */ }
}

// --- Swipe state ---
const swipeProgress = ref(0)
let swipeStartX = 0
let dragActive = false
let dragMoved = false

const onStageMouseDown = (e) => {
  if (e.button !== 0) return
  if (e.target.closest('.action-btn')) return
  swipeStartX = e.clientX
  dragActive = true
  dragMoved = false
  document.body.style.userSelect = 'none'
  document.addEventListener('mousemove', onStageMouseMove)
  document.addEventListener('mouseup', onStageMouseUp)
}

const onStageMouseMove = (e) => {
  if (!dragActive) return
  const delta = e.clientX - swipeStartX
  if (!dragMoved && Math.abs(delta) > 4) dragMoved = true
  if (!dragMoved) return
  const w = containerWidth.value || 800
  swipeProgress.value = Math.max(-1, Math.min(1, delta / (w * 0.5)))
}

const onStageMouseUp = () => {
  const wasDragged = dragMoved
  dragActive = false
  document.body.style.removeProperty('user-select')
  document.removeEventListener('mousemove', onStageMouseMove)
  document.removeEventListener('mouseup', onStageMouseUp)
  if (!wasDragged) return

  const p = swipeProgress.value
  if (p > SWIPE_THRESHOLD && selectedIndex.value > 0) {
    swipeProgress.value = 0
    navigateTo(selectedIndex.value - 1)
  } else if (p < -SWIPE_THRESHOLD && selectedIndex.value < props.visibleHistory.length - 1) {
    swipeProgress.value = 0
    navigateTo(selectedIndex.value + 1)
  } else {
    swipeProgress.value = 0  // snap back
  }
  skipNextClick = true
}

const navigateTo = (idx) => {
  const id = props.visibleHistory[idx]?.id
  if (id) props.updateSelection(id, false, null, null)
}

let skipNextClick = false

const handleClick = (entryId) => {
  if (skipNextClick) { skipNextClick = false; return }
  navigateTo(props.visibleHistory.findIndex(e => e.id === entryId))
}

let wheelTimer = null
const WHEEL_GAP = 200

const onWheel = (e) => {
  e.preventDefault()
  if (wheelTimer) return
  wheelTimer = setTimeout(() => { wheelTimer = null }, WHEEL_GAP)
  const current = selectedIndex.value
  if (current < 0 || props.visibleHistory.length === 0) return
  if (e.deltaY < 0 && current < props.visibleHistory.length - 1) {
    navigateTo(current + 1)
  } else if (e.deltaY > 0 && current > 0) {
    navigateTo(current - 1)
  }
}

onMounted(() => {
  containerWidth.value = contentRef.value?.clientWidth || 800
  resizeObserver = new ResizeObserver(() => {
    containerWidth.value = contentRef.value?.clientWidth || 800
  })
  if (contentRef.value) resizeObserver.observe(contentRef.value)
  stageRef.value?.addEventListener('wheel', onWheel, {passive: false})
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  stageRef.value?.removeEventListener('wheel', onWheel)
})

defineExpose({contentRef})
</script>

<style scoped>
.content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.carousel-stage {
  flex: 1;
  min-height: 0;
  position: relative;
  overflow: hidden;
  perspective: 1000px;
  cursor: grab;
}

.carousel-stage:active {
  cursor: grabbing;
}

.clipboard-item {
  position: absolute;
  display: flex;
  flex-direction: column;
  width: 260px;
  height: 250px;
  white-space: normal;
  background: var(--fy-glass-bg);
  border: 0.5px solid var(--fy-glass-border);
  border-radius: 14px;
  padding: 0;
  cursor: pointer;
  user-select: none;
  backdrop-filter: var(--fy-glass-blur);
  -webkit-backdrop-filter: var(--fy-glass-blur);
  color: var(--fy-text-primary);
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
  transition: left 0.4s var(--fy-ease-out),
              transform 0.4s var(--fy-ease-out),
              opacity 0.4s var(--fy-ease-out),
              box-shadow 0.3s ease;
  overflow: hidden;
  will-change: transform, opacity, left;
}

.clipboard-item.selected {
  background: var(--fy-accent-bg);
  border-color: var(--fy-border-active);
  box-shadow: 0 4px 28px rgba(108, 140, 255, 0.2), 0 8px 32px rgba(0, 0, 0, 0.12);
}

.clipboard-item:hover {
  box-shadow: 0 4px 24px rgba(108, 140, 255, 0.15), 0 8px 32px rgba(0, 0, 0, 0.12);
  z-index: 1000 !important;
}

.item-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px 0;
  flex-shrink: 0;
}

.item-index {
  font-size: 10px;
  font-family: var(--fy-font-mono);
  color: var(--fy-text-muted);
  opacity: 0.5;
  transition: opacity 0.2s;
}

.clipboard-item:hover .item-index,
.clipboard-item.selected .item-index {
  opacity: 1;
  color: var(--fy-accent);
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

.clipboard-item:hover .item-category,
.clipboard-item.selected .item-category {
  opacity: 1;
}

.item-pinned-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--fy-warning);
  flex-shrink: 0;
}

.item-actions {
  display: flex;
  align-items: center;
  gap: 1px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.15s;
}

.clipboard-item:hover .item-actions,
.clipboard-item.selected .item-actions {
  opacity: 1;
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
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
}

.action-btn.active { color: var(--fy-warning); }

.action-delete:hover {
  color: var(--fy-danger);
  background: var(--fy-danger-bg);
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
.item-body::-webkit-scrollbar { display: none; }

.item-snippet {
  margin: 0 12px 8px;
  padding: 6px 0 0;
  border-top: 0.5px solid var(--fy-border-light);
  font-size: 10px;
  line-height: 1.4;
  color: var(--fy-text-accent);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 40px;
  overflow: hidden;
  flex-shrink: 0;
}

.snippet-hit {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
  border-radius: 2px;
  padding: 0 2px;
}

.load-more-bar {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 0;
  flex-shrink: 0;
  color: var(--fy-text-muted);
  font-size: var(--fy-text-sm);
  border-top: 0.5px solid var(--fy-border-light);
  cursor: pointer;
}
.load-more-text { font-size: 12px; }
.load-more-text:hover { color: var(--fy-accent); }
</style>
