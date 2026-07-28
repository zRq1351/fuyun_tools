<template>
  <div ref="contentRef" class="content">
    <div ref="stageRef" class="carousel-stage" @mousedown="onStageMouseDown" @click="onStageClick">
      <button
          v-if="selectedIndex > 0"
          class="nav-arrow nav-prev"
          @click.stop="navigateTo(selectedIndex - 1)"
      >‹</button>
      <button
          v-if="selectedIndex < stackItems.length - 1"
          class="nav-arrow nav-next"
          @click.stop="navigateTo(selectedIndex + 1)"
      >›</button>
      <div
          v-for="entry in visibleCards"
          :id="'clipboard-item-' + entry.id"
          :key="entry.id"
          :class="{ selected: entry.id === selectedItemId, pinned: entry.pinned }"
          :draggable="isCtrlKeyPressed"
          :style="cardStyle(entry._index)"
          class="clipboard-item"
          @dblclick="handleDoubleClick(entry.id)"
          @dragend="handleDragEnd"
          @dragstart="handleItemDragStart($event, entry.id)"
          @contextmenu.prevent="showContextMenu($event, entry.id, entry._index)"
      >
        <div class="item-header">
          <span class="item-index">{{ entry._index + 1 }}/{{ totalCount || stackItems.length }}</span>
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

    <div v-if="stackItems.length > 1" class="timeline-bar">
      <div
          v-for="(_, idx) in stackItems"
          :key="idx"
          :class="{ active: idx === selectedIndex }"
          class="timeline-dot"
          :title="(idx + 1) + ' / ' + stackItems.length"
          @click.stop="navigateTo(idx)"
      />
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
import {computed, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {Close, Link, Loading, Star, View} from '@element-plus/icons-vue'
import {openUrl as openExternalUrl} from '@tauri-apps/plugin-opener'
import {useI18n} from 'vue-i18n'
import FormattedContent from '../../../components/FormattedContent.vue'

const {t} = useI18n()

const CARD_WIDTH = 300
const CARD_HEIGHT = 280

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
  isLoadingPage: {type: Boolean, default: false},
  totalCount: {type: Number, default: 0}
})
const emit = defineEmits(['load-more-intent', 'preview'])

const contentRef = ref(null)
const stageRef = ref(null)
const containerWidth = ref(800)

let resizeObserver = null

// Calculate visual properties — continuous scroll strip with center-focused scale
const CARD_STEP = 76
const VISIBLE_PAD = 6  // extra cards rendered on each side of viewport

const stackItems = computed(() =>
  props.visibleHistory.map((entry, index) => ({
    ...entry,
    snippet: entry.snippet || '',
    pinned: props.isPinned(entry.id),
  }))
)

// Only render cards within viewport range — massive perf win for large lists
const visibleCards = computed(() => {
  const total = props.visibleHistory.length
  if (total === 0) return []
  const centerIdx = scrollPos.value / CARD_STEP
  const half = (containerWidth.value / CARD_STEP) / 2 + VISIBLE_PAD
  const start = Math.max(0, Math.floor(centerIdx - half))
  const end = Math.min(total, Math.ceil(centerIdx + half) + 1)
  // Cache: only rebuild when the visible range actually shifts
  if (start === _lastStart && end === _lastEnd) return _lastVisible
  _lastStart = start; _lastEnd = end
  _lastVisible = props.visibleHistory.slice(start, end).map((entry, i) => ({
    ...entry,
    _index: start + i,
    snippet: entry.snippet || '',
    pinned: props.isPinned(entry.id),
  }))
  return _lastVisible
})

let _lastStart = -1
let _lastEnd = -1
let _lastVisible = []

const cardStyle = (index) => {
  const w = containerWidth.value
  const viewCenter = w / 2 - CARD_WIDTH / 2
  const moving = dragActive || inertiaId
  const searching = (props.highlightKeyword || '').trim().length > 0

  // Spread out during drag so cards don't block each other
  const spread = moving ? 1.35 : 1
  const step = CARD_STEP * spread
  const cardX = index * step - scrollPos.value * spread + viewCenter
  const cardCenter = cardX + CARD_WIDTH / 2
  const distFromCenter = cardCenter - w / 2
  const absDist = Math.abs(distFromCenter)
  const far = absDist > w * 1.2

  const scaleMin = searching ? 0.88 : (moving ? 0.8 : 0.78)
  const scaleDecay = searching ? 0.70 : 0.55
  const opacityMin = searching ? 0.4 : 0.3
  const opacityDecay = searching ? 1.0 : 0.85

  const scale = Math.max(scaleMin, 1 - absDist / (w * scaleDecay))
  const opacity = Math.max(opacityMin, 1 - absDist / (w * opacityDecay))
  const zIndex = 200 - Math.floor(absDist / CARD_STEP) * 5

  return {
    left: '0px',
    top: '50%',
    width: CARD_WIDTH + 'px',
    height: CARD_HEIGHT + 'px',
    transform: `translateX(${cardX}px) translateY(-50%) scale(${scale})`,
    opacity,
    zIndex,
    transition: dragActive ? 'none' : undefined,
    // Solid bg during motion, glass only when stationary — massive perf win
    background: (moving || far) ? 'var(--fy-bg-surface)' : undefined,
    backdropFilter: (moving || far) ? 'none' : undefined,
    WebkitBackdropFilter: (moving || far) ? 'none' : undefined,
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

// --- Scroll / drag state ---
const scrollPos = ref(0)  // current scroll offset in px
let dragStartX = 0
let dragStartScroll = 0
let dragActive = false
let dragMoved = false

// Derived: which card is closest to center
const selectedIndex = computed(() => {
  if (props.visibleHistory.length === 0) return 0
  return Math.max(0, Math.min(props.visibleHistory.length - 1,
    Math.round(scrollPos.value / CARD_STEP)))
})

// Keep scrollPos synced with external selection changes
const syncScrollToSelection = () => {
  const idx = props.visibleHistory.findIndex(e => e.id === props.selectedItemId)
  if (idx >= 0) {
    scrollPos.value = idx * CARD_STEP
  }
}

const onStageMouseDown = (e) => {
  if (e.button !== 0) return
  if (e.target.closest('.action-btn')) return
  stopInertia()
  dragStartX = e.clientX
  dragStartScroll = scrollPos.value
  dragActive = true
  dragMoved = false
  velocitySamples = []
  document.body.style.userSelect = 'none'
  document.addEventListener('mousemove', onStageMouseMove)
  document.addEventListener('mouseup', onStageMouseUp)
}

const onStageMouseMove = (e) => {
  if (!dragActive) return
  const delta = e.clientX - dragStartX
  if (!dragMoved && Math.abs(delta) > 3) dragMoved = true
  if (!dragMoved) return
  // RAF-throttle: batch all mousemove events into at most one update per frame
  pendingScroll = dragStartScroll - delta
  if (!rafPending) {
    rafPending = true
    requestAnimationFrame(() => {
      rafPending = false
      scrollPos.value = pendingScroll
      if (dragActive) {
        velocitySamples.push({ time: performance.now(), pos: scrollPos.value })
        if (velocitySamples.length > 5) velocitySamples.shift()
      }
    })
  }
}

let rafPending = false
let pendingScroll = 0

const onStageMouseUp = () => {
  const wasDragged = dragMoved
  dragActive = false
  document.body.style.removeProperty('user-select')
  document.removeEventListener('mousemove', onStageMouseMove)
  document.removeEventListener('mouseup', onStageMouseUp)

  if (!wasDragged) return
  skipNextClick = true

  // Calculate velocity from recent samples (px/ms)
  let vx = 0
  if (velocitySamples.length >= 2) {
    const first = velocitySamples[0]
    const last = velocitySamples[velocitySamples.length - 1]
    const dt = last.time - first.time
    if (dt > 0) vx = (last.pos - first.pos) / dt
  }
  velocitySamples = []

  const absV = Math.abs(vx)
  if (absV < 0.02) {
    snapToNearest()
    return
  }
  startInertia(vx)
}

// --- Inertia animation ---
const FRICTION = 0.88
const MIN_SPEED = 0.02

const startInertia = (velocity) => {
  stopInertia()
  let v = Math.max(-1.5, Math.min(1.5, velocity))
  const maxPos = Math.max(0, (props.visibleHistory.length - 1) * CARD_STEP)
  const animate = () => {
    scrollPos.value += v * 16
    v *= FRICTION

    // Bounce at edges
    if (scrollPos.value < 0) { scrollPos.value = 0; v = Math.abs(v) * 0.3 }
    if (scrollPos.value > maxPos) { scrollPos.value = maxPos; v = -Math.abs(v) * 0.3 }

    if (Math.abs(v) > MIN_SPEED) {
      inertiaId = requestAnimationFrame(animate)
    } else {
      inertiaId = null
      snapToNearest()
    }
  }
  inertiaId = requestAnimationFrame(animate)
}

const stopInertia = () => {
  if (inertiaId) { cancelAnimationFrame(inertiaId); inertiaId = null }
}

const snapToNearest = () => {
  const nearestIdx = Math.max(0, Math.min(props.visibleHistory.length - 1,
    Math.round(scrollPos.value / CARD_STEP)))
  scrollPos.value = nearestIdx * CARD_STEP
  navigateTo(nearestIdx)
}

let velocitySamples = []
let inertiaId = null

let skipNextClick = false

const handleClick = (entryId) => {
  if (skipNextClick) { skipNextClick = false; return }
  const idx = props.visibleHistory.findIndex(e => e.id === entryId)
  if (idx >= 0) navigateTo(idx)
}

// Stage-level click: use elementFromPoint to hit the correct card
// (per-card @click fails when cards overlap — the topmost card captures all clicks)
const onStageClick = (e) => {
  if (skipNextClick) { skipNextClick = false; return }
  const el = document.elementFromPoint(e.clientX, e.clientY)
  const card = el?.closest('.clipboard-item')
  if (!card) return
  const id = card.id?.replace('clipboard-item-', '')
  if (id) {
    const idx = props.visibleHistory.findIndex(entry => entry.id === id)
    if (idx >= 0) navigateTo(idx)
  }
}

const navigateTo = (idx) => {
  const id = props.visibleHistory[idx]?.id
  if (id) {
    scrollPos.value = idx * CARD_STEP
    props.updateSelection(id, false, null, null)
  }
}

const jumpToStart = () => navigateTo(0)
const jumpToEnd = () => navigateTo(props.visibleHistory.length - 1)

// Sync scroll position when selection changes externally (keyboard, etc.)
watch(() => props.selectedItemId, () => {
  if (dragActive) return
  const idx = props.visibleHistory.findIndex(e => e.id === props.selectedItemId)
  if (idx >= 0) {
    scrollPos.value = idx * CARD_STEP
  }
})

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
  syncScrollToSelection()
  resizeObserver = new ResizeObserver(() => {
    containerWidth.value = contentRef.value?.clientWidth || 800
  })
  if (contentRef.value) resizeObserver.observe(contentRef.value)
  stageRef.value?.addEventListener('wheel', onWheel, {passive: false})
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  stageRef.value?.removeEventListener('wheel', onWheel)
  stopInertia()
})

defineExpose({contentRef, jumpToStart, jumpToEnd})
</script>

<style scoped>
.content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: visible;
}

.carousel-stage {
  flex: 1;
  min-height: 0;
  position: relative;
  overflow: visible;
  perspective: 1000px;
  cursor: grab;
}

.carousel-stage:active {
  cursor: grabbing;
}

.nav-arrow {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  z-index: 300;
  width: 36px;
  height: 60px;
  border: none;
  background: rgba(0, 0, 0, 0.15);
  color: var(--fy-text-primary);
  font-size: 28px;
  line-height: 1;
  cursor: pointer;
  border-radius: var(--fy-radius-md);
  opacity: 0;
  transition: opacity 0.2s ease, background 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  backdrop-filter: blur(8px);
}

.carousel-stage:hover .nav-arrow {
  opacity: 1;
}

.nav-arrow:hover {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

.nav-prev { left: 8px; }
.nav-next { right: 8px; }

.clipboard-item {
  position: absolute;
  left: 0;
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
  transition: transform 0.4s var(--fy-ease-out),
              opacity 0.4s var(--fy-ease-out),
              box-shadow 0.3s ease;
  overflow: hidden;
  contain: layout style paint;
  will-change: transform, opacity;
}

.clipboard-item.selected {
  background: var(--fy-accent-bg);
  border-color: var(--fy-border-active);
  box-shadow: 0 4px 28px rgba(108, 140, 255, 0.2), 0 8px 32px rgba(0, 0, 0, 0.12);
}

.clipboard-item.pinned {
  border-top: 2px solid var(--fy-warning);
}

.clipboard-item:hover {
  box-shadow: 0 4px 24px rgba(108, 140, 255, 0.15), 0 8px 32px rgba(0, 0, 0, 0.12);
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

.timeline-bar {
  position: relative;
  z-index: 250;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 3px;
  padding: 6px 16px;
  flex-shrink: 0;
  overflow-x: auto;
  scrollbar-width: none;
}

.timeline-bar::-webkit-scrollbar { display: none; }

.timeline-dot {
  width: 7px;
  height: 7px;
  min-width: 7px;
  border-radius: 50%;
  background: var(--fy-border);
  cursor: pointer;
  transition: all 0.2s ease;
}

.timeline-dot:hover {
  background: var(--fy-accent);
  transform: scale(1.6);
}

.timeline-dot.active {
  background: var(--fy-accent);
  width: 18px;
  border-radius: 4px;
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
