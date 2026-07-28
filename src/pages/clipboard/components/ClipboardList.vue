<template>
  <div ref="contentRef" class="content">
    <div ref="stackRef" class="cards-stack">
      <div
          v-for="(entry, index) in stackItems"
          :id="'clipboard-item-' + entry.id"
          :key="entry.id"
          :class="{ selected: selectedItemId === entry.id, pinned: entry.pinned }"
          :draggable="isCtrlKeyPressed"
          :style="{ zIndex: entry.zIndex, marginRight: index < stackItems.length - 1 ? (-overlapPx) + 'px' : '0' }"
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
            <div v-if="isWebUrl(entry.content)" class="action-btn"
                 @click.stop="openWebUrl(entry.content)">
              <Link :size="9"/>
            </div>
            <div class="action-btn" @click.stop="emit('preview', entry.content, entry.id)">
              <View :size="9"/>
            </div>
            <div :class="{ active: entry.pinned }" class="action-btn"
                 @click.stop="promoteItem(entry.id)">
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
      <el-icon v-if="isLoadingMore" :size="14" class="is-loading">
        <Loading/>
      </el-icon>
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
const MIN_PEEK = 24
const MAX_OVERLAP = 220

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
const stackRef = ref(null)
const containerWidth = ref(0)

let resizeObserver = null

const stackItems = computed(() => {
  const total = props.visibleHistory.length
  return props.visibleHistory.map((entry, index) => ({
    ...entry,
    snippet: entry.snippet || '',
    pinned: props.isPinned(entry.id),
    zIndex: total - index,
  }))
})

const overlapPx = computed(() => {
  const count = stackItems.value.length
  if (count <= 1) return 0
  const w = containerWidth.value || 800
  // Calculate overlap so all cards fit in container
  // Total width = CARD_WIDTH + (count-1)*(CARD_WIDTH - overlap)
  // overlap = CARD_WIDTH - (w - CARD_WIDTH) / (count-1)
  const calc = CARD_WIDTH - (w - CARD_WIDTH - 40) / (count - 1)
  return Math.round(Math.min(MAX_OVERLAP, Math.max(MIN_PEEK, calc)))
})

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)
const showLoadMoreHint = computed(() => props.hasMore || isLoadingMore.value)

const handleClick = (entryId) => props.updateSelection(entryId, false, null, null)
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
    let bestIndex = -1
    let bestTokenLength = 0
    for (let i = 0; i < tokenLowers.length; i++) {
      const idx = sourceLower.indexOf(tokenLowers[i], start)
      if (idx === -1) continue
      if (bestIndex === -1 || idx < bestIndex || (idx === bestIndex && tokenLowers[i].length > bestTokenLength)) {
        bestIndex = idx
        bestTokenLength = tokenLowers[i].length
      }
    }
    if (bestIndex === -1) { out.push({text: value.slice(start), hit: false}); break }
    if (bestIndex > start) out.push({text: value.slice(start, bestIndex), hit: false})
    out.push({text: value.slice(bestIndex, bestIndex + bestTokenLength), hit: true})
    start = bestIndex + bestTokenLength
  }
  return out.length > 0 ? out : [{text: value, hit: false}]
}

const handleItemDragStart = (e, id) => {
  if (typeof props.handleDragStart === 'function') props.handleDragStart(e, id)
}

const isWebUrl = (v) => {
  if (!v) return false
  return /^https?:\/\/\S+$/i.test(t) || /^www\.\S+$/i.test(t)
}
const normalizeUrl = (v) => {
  const t = v.trim()
  if (/^https?:\/\//i.test(t)) return t
  if (/^www\./i.test(t)) return `https://${t}`
  return t
}
const openWebUrl = async (v) => {
  try { if (isWebUrl(v)) await openExternalUrl(normalizeUrl(v)) }
  catch (e) { console.error('打开网址失败:', e) }
}

const measureWidth = () => {
  if (contentRef.value) {
    containerWidth.value = contentRef.value.clientWidth
  }
}

onMounted(() => {
  measureWidth()
  resizeObserver = new ResizeObserver(() => measureWidth())
  if (contentRef.value) resizeObserver.observe(contentRef.value)
})

onBeforeUnmount(() => {
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
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
  perspective: 800px;
}

.cards-stack {
  display: flex;
  align-items: center;
  flex: 1;
  min-height: 0;
  padding: 0 20px;
  overflow: hidden;
}

.clipboard-item {
  display: inline-flex;
  flex-direction: column;
  width: 260px;
  min-width: 260px;
  height: 250px;
  white-space: normal;
  flex-shrink: 0;
  background: var(--fy-glass-bg);
  border: 0.5px solid var(--fy-glass-border);
  border-radius: 14px;
  padding: 0;
  cursor: pointer;
  position: relative;
  user-select: none;
  backdrop-filter: var(--fy-glass-blur);
  -webkit-backdrop-filter: var(--fy-glass-blur);
  color: var(--fy-text-primary);
  box-shadow: -4px 0 16px rgba(0, 0, 0, 0.1),
              0 4px 16px rgba(0, 0, 0, 0.06);
  transition: transform 0.3s var(--fy-ease-out),
              box-shadow 0.3s var(--fy-ease-out),
              margin-top 0.3s var(--fy-ease-out);
  overflow: hidden;
}

.clipboard-item:hover {
  background: var(--fy-bg-surface);
  border-color: var(--fy-border-hover);
  transform: translateY(-8px) scale(1.02);
  box-shadow: -4px 0 24px rgba(0, 0, 0, 0.15),
              0 8px 28px rgba(0, 0, 0, 0.12);
  z-index: 1000 !important;
}

.clipboard-item.selected {
  background: var(--fy-accent-bg);
  border-color: var(--fy-border-active);
  transform: translateY(-8px) scale(1.02);
  box-shadow: -4px 0 24px rgba(108, 140, 255, 0.12),
              0 8px 28px rgba(0, 0, 0, 0.12);
  z-index: 1000 !important;
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

.clipboard-item:hover .item-category {
  opacity: 1;
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

.action-btn.active {
  color: var(--fy-warning);
}

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

.item-body::-webkit-scrollbar {
  display: none;
}

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

.load-more-text {
  font-size: 12px;
}

.load-more-text:hover {
  color: var(--fy-accent);
}
</style>
