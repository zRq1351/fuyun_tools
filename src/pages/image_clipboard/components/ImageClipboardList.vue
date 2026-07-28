<template>
  <div ref="contentRef" class="content">
    <div ref="stageRef" class="carousel-stage" @mousedown="onStageMouseDown" @click="onStageClick">
      <button
          v-if="selectedIndex > 0"
          class="nav-arrow nav-prev"
          @mousedown.stop="startNavRepeat(-1)"
          @mouseup.stop="stopNavRepeat"
          @mouseleave.stop="stopNavRepeat"
      >‹</button>
      <button
          v-if="selectedIndex < visibleHistory.length - 1"
          class="nav-arrow nav-next"
          @mousedown.stop="startNavRepeat(1)"
          @mouseup.stop="stopNavRepeat"
          @mouseleave.stop="stopNavRepeat"
      >›</button>
      <div
          v-for="entry in visibleCards"
          :id="'image-item-' + entry._index"
          :key="entry.id"
          :class="{ selected: entry._index === selectedIndex, pinned: entry.pinned }"
          :draggable="isCtrlKeyPressed"
          :style="cardStyle(entry._index)"
          class="clipboard-item"
          @dblclick="handleDoubleClick(entry.id)"
          @dragend="handleDragEnd"
          @dragstart="handleItemDragStart($event, entry.id)"
          @contextmenu.prevent="showContextMenu($event, entry.id)"
      >
        <div class="item-header">
          <span class="item-index">{{ entry._index + 1 }}/{{ totalCount || visibleHistory.length }}</span>
          <span class="item-category" @click.stop>{{ translateCategory(entry.category) }}</span>
          <div v-if="entry.pinned" class="item-pinned-dot"></div>
          <div class="item-actions">
            <div class="action-btn" @click.stop="openFullscreen(entry.id)">
              <FullScreen :size="9"/>
            </div>
            <div class="action-btn" @click.stop="downloadItem(entry.id)">
              <Download :size="9"/>
            </div>
            <div :class="{ active: entry.pinned }" class="action-btn" @click.stop="promoteItem(entry.id)">
              <Star :size="9"/>
            </div>
            <div class="action-btn action-delete" @click.stop="deleteItem(entry.id, entry._index)">
              <Close :size="9"/>
            </div>
          </div>
        </div>
        <div class="item-content">
          <img :src="getPreviewDataUrl(entry.rawItem)" alt="" class="image-preview" decoding="async"
               draggable="false" @dragstart.prevent/>
        </div>
        <div v-if="entry.tags && entry.tags.length" class="tag-wrap">
          <div class="tag-chip-list">
            <span v-for="tag in entry.tags" :key="`${entry.id}-${tag}`" class="tag-chip">#{{ tag }}</span>
          </div>
        </div>
        <div class="image-meta">{{ entry.rawItem.width }} × {{ entry.rawItem.height }}</div>
      </div>
    </div>

    <div v-if="showLoadMoreHint" class="load-more-bar">
      <el-icon v-if="isLoadingMore" :size="14" class="is-loading"><Loading/></el-icon>
      <span class="load-more-text" @click="emit('load-more-intent')">
        {{ isLoadingMore ? $t('imageClipboard.loading') : $t('imageClipboard.loadMore') }}
      </span>
    </div>
  </div>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {Close, Download, FullScreen, Loading, Star} from '@element-plus/icons-vue'
import {useI18n} from 'vue-i18n'

const {t} = useI18n()

const CARD_WIDTH = 300
const CARD_HEIGHT = 280
const CARD_STEP = 76
const VISIBLE_PAD = 6

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
  selectedIndex: {type: Number, required: true},
  isCtrlKeyPressed: {type: Boolean, default: false},
  deleteItem: {type: Function, required: true},
  selectByIndex: {type: Function, required: true},
  fillById: {type: Function, required: true},
  handleDragStart: {type: Function, required: true},
  handleDragEnd: {type: Function, required: true},
  showContextMenu: {type: Function, required: true},
  promoteItem: {type: Function, required: true},
  downloadItem: {type: Function, required: true},
  openFullscreen: {type: Function, required: true},
  getPreviewDataUrl: {type: Function, required: true},
  hasMore: {type: Boolean, default: false},
  isLoadingPage: {type: Boolean, default: false},
  totalCount: {type: Number, default: 0}
})
const emit = defineEmits(['load-more-intent'])

const contentRef = ref(null)
const stageRef = ref(null)
const containerWidth = ref(800)
let resizeObserver = null

// --- Scroll state ---
const scrollPos = ref(0)
let dragStartX = 0
let dragStartScroll = 0
let dragActive = false
let dragMoved = false

// Keep scrollPos synced with external selectedIndex changes
watch(() => props.selectedIndex, (idx) => {
  if (dragActive) return
  scrollPos.value = idx * CARD_STEP
})

// Derived selectedIndex from scrollPos for display purposes
const displayIndex = computed(() => {
  if (props.visibleHistory.length === 0) return 0
  return Math.max(0, Math.min(props.visibleHistory.length - 1,
    Math.round(scrollPos.value / CARD_STEP)))
})

// Auto-load more when approaching end of loaded items
const LOAD_MORE_GAP = 5
watch(displayIndex, (idx) => {
  if (props.hasMore && !props.isLoadingPage && idx >= props.visibleHistory.length - LOAD_MORE_GAP) {
    emit('load-more-intent')
  }
})

const visibleCards = computed(() => {
  const total = props.visibleHistory.length
  if (total === 0) return []
  const centerIdx = scrollPos.value / CARD_STEP
  const half = (containerWidth.value / CARD_STEP) / 2 + VISIBLE_PAD
  const start = Math.max(0, Math.floor(centerIdx - half))
  const end = Math.min(total, Math.ceil(centerIdx + half) + 1)
  if (start === _lastStart && end === _lastEnd && total === _lastTotal) return _lastVisible
  _lastStart = start; _lastEnd = end; _lastTotal = total
  _lastVisible = props.visibleHistory.slice(start, end).map((entry, i) => ({
    id: entry.item.id,
    rawItem: entry.item,
    _index: start + i,
    pinned: entry.pinned,
    category: entry.category,
    tags: entry.tags,
  }))
  return _lastVisible
})

let _lastStart = -1, _lastEnd = -1, _lastTotal = -1, _lastVisible = []

// Card visual style
const cardStyle = (index) => {
  const w = containerWidth.value
  const viewCenter = w / 2 - CARD_WIDTH / 2
  const moving = dragActive || inertiaId
  const spread = moving ? 1.35 : 1
  const step = CARD_STEP * spread
  const cardX = index * step - scrollPos.value * spread + viewCenter
  const cardCenter = cardX + CARD_WIDTH / 2
  const distFromCenter = cardCenter - w / 2
  const absDist = Math.abs(distFromCenter)
  const far = absDist > w * 1.2

  const scaleMin = moving ? 0.8 : 0.78
  const scale = Math.max(scaleMin, 1 - absDist / (w * 0.55))
  const opacity = Math.max(0.3, 1 - absDist / (w * 0.85))
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
    background: (moving || far) ? 'var(--fy-bg-surface)' : undefined,
    backdropFilter: (moving || far) ? 'none' : undefined,
    WebkitBackdropFilter: (moving || far) ? 'none' : undefined,
  }
}

// --- Drag ---
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

let rafPending = false
let pendingScroll = 0

const onStageMouseMove = (e) => {
  if (!dragActive) return
  const delta = e.clientX - dragStartX
  if (!dragMoved && Math.abs(delta) > 3) dragMoved = true
  if (!dragMoved) return
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

let skipNextClick = false

const onStageMouseUp = () => {
  const wasDragged = dragMoved
  dragActive = false
  document.body.style.removeProperty('user-select')
  document.removeEventListener('mousemove', onStageMouseMove)
  document.removeEventListener('mouseup', onStageMouseUp)
  if (!wasDragged) return
  skipNextClick = true

  let vx = 0
  if (velocitySamples.length >= 2) {
    const first = velocitySamples[0]
    const last = velocitySamples[velocitySamples.length - 1]
    const dt = last.time - first.time
    if (dt > 0) vx = (last.pos - first.pos) / dt
  }
  velocitySamples = []
  if (Math.abs(vx) < 0.02) { snapToNearest(); return }
  startInertia(vx)
}

// --- Inertia ---
const FRICTION = 0.88
const MIN_SPEED = 0.02
let velocitySamples = []
let inertiaId = null

const startInertia = (velocity) => {
  stopInertia()
  let v = Math.max(-1.5, Math.min(1.5, velocity))
  const maxPos = Math.max(0, (props.visibleHistory.length - 1) * CARD_STEP)
  const animate = () => {
    scrollPos.value += v * 16
    v *= FRICTION
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

// --- Click / Navigation ---
const onStageClick = (e) => {
  if (skipNextClick) { skipNextClick = false; return }
  const el = document.elementFromPoint(e.clientX, e.clientY)
  const card = el?.closest('.clipboard-item')
  if (!card) return
  const idAttr = card.id?.replace('image-item-', '')
  const idx = parseInt(idAttr, 10)
  if (!isNaN(idx) && idx >= 0 && idx < props.visibleHistory.length) {
    navigateTo(idx)
  }
}

const navigateTo = (idx) => {
  scrollPos.value = idx * CARD_STEP
  props.selectByIndex(idx)
}

const handleDoubleClick = (itemId) => props.fillById(itemId)
const handleItemDragStart = (e, id) => {
  if (typeof props.handleDragStart === 'function') props.handleDragStart(e, id)
}

// --- Nav arrows long-press ---
let navRepeatTimer = null, navRepeatDir = 0

const startNavRepeat = (dir) => {
  navRepeatDir = dir
  navigateTo(displayIndex.value + dir)
  navRepeatTimer = setTimeout(() => {
    navRepeatTimer = setInterval(() => {
      const next = displayIndex.value + navRepeatDir
      if (next < 0 || next >= props.visibleHistory.length) { stopNavRepeat(); return }
      navigateTo(next)
    }, 120)
  }, 400)
}

const stopNavRepeat = () => {
  if (navRepeatTimer) { clearInterval(navRepeatTimer); clearTimeout(navRepeatTimer); navRepeatTimer = null }
}

// --- Wheel ---
let wheelTimer = null
const WHEEL_GAP = 200

const onWheel = (e) => {
  e.preventDefault()
  if (wheelTimer) return
  wheelTimer = setTimeout(() => { wheelTimer = null }, WHEEL_GAP)
  if (props.visibleHistory.length === 0) return
  if (e.deltaY < 0 && displayIndex.value < props.visibleHistory.length - 1) {
    navigateTo(displayIndex.value + 1)
  } else if (e.deltaY > 0 && displayIndex.value > 0) {
    navigateTo(displayIndex.value - 1)
  }
}

// --- Jump helpers ---
const jumpToStart = () => navigateTo(0)
const jumpToEnd = () => navigateTo(props.visibleHistory.length - 1)

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)
const showLoadMoreHint = computed(() => props.hasMore || isLoadingMore.value)

onMounted(() => {
  containerWidth.value = contentRef.value?.clientWidth || 800
  resizeObserver = new ResizeObserver(() => {
    containerWidth.value = contentRef.value?.clientWidth || 800
  })
  if (contentRef.value) resizeObserver.observe(contentRef.value)
  stageRef.value?.addEventListener('wheel', onWheel, {passive: false})
  scrollPos.value = props.selectedIndex * CARD_STEP
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  stageRef.value?.removeEventListener('wheel', onWheel)
  stopInertia()
  stopNavRepeat()
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

.carousel-stage:active { cursor: grabbing; }

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

.carousel-stage:hover .nav-arrow { opacity: 1; }
.nav-arrow:hover { background: var(--fy-accent-bg); color: var(--fy-accent); }
.nav-prev { left: 8px; }
.nav-next { right: 8px; }

.clipboard-item {
  position: absolute;
  left: 0;
  display: flex;
  flex-direction: column;
  width: 300px;
  height: 280px;
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

.clipboard-item.pinned { border-top: 2px solid var(--fy-warning); }

.clipboard-item.selected {
  background: var(--fy-accent-bg);
  border-color: var(--fy-border-active);
  box-shadow: 0 4px 28px rgba(108, 140, 255, 0.2), 0 8px 32px rgba(0, 0, 0, 0.12);
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
.clipboard-item.selected .item-index { opacity: 1; color: var(--fy-accent); }

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
.clipboard-item.selected .item-category { opacity: 1; }

.item-pinned-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--fy-warning); flex-shrink: 0; }

.item-actions {
  display: flex; align-items: center; gap: 1px; flex-shrink: 0;
  opacity: 0; transition: opacity 0.15s;
}

.clipboard-item:hover .item-actions,
.clipboard-item.selected .item-actions { opacity: 1; }

.action-btn {
  width: 16px; height: 16px; border-radius: 3px;
  background: transparent; border: none;
  display: flex; align-items: center; justify-content: center;
  cursor: pointer; color: var(--fy-text-muted);
  transition: all 0.15s ease;
}

.action-btn:hover { background: var(--fy-bg-hover); color: var(--fy-text-primary); }
.action-btn.active { color: var(--fy-warning); }
.action-delete:hover { color: var(--fy-danger); background: var(--fy-danger-bg); }

.item-content {
  flex: 1; min-height: 0;
  display: flex; align-items: center; justify-content: center;
  padding: 4px; overflow: hidden;
}

.image-preview {
  max-width: 100%; max-height: 100%;
  object-fit: contain; border-radius: 4px;
}

.tag-wrap { padding: 0 8px 4px; flex-shrink: 0; }
.tag-chip-list { display: flex; flex-wrap: wrap; gap: 4px; }
.tag-chip {
  font-size: 10px; padding: 1px 6px; border-radius: 8px;
  background: var(--fy-bg-hover); color: var(--fy-text-accent);
}

.image-meta {
  padding: 0 10px 6px; font-size: 10px;
  color: var(--fy-text-muted); opacity: 0.6; flex-shrink: 0;
}

.load-more-bar {
  display: flex; align-items: center; justify-content: center;
  gap: 6px; padding: 8px 0; flex-shrink: 0;
  color: var(--fy-text-muted); font-size: var(--fy-text-sm);
  border-top: 0.5px solid var(--fy-border-light); cursor: pointer;
}
.load-more-text { font-size: 12px; }
.load-more-text:hover { color: var(--fy-accent); }
</style>
