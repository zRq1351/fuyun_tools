<template>
  <div
      ref="contentRef"
      class="content"
      @mousedown="handleMouseDown"
      @scroll="handleScroll"
      @wheel.prevent="handleWheel"
  >
    <div
        v-for="(entry, index) in visibleHistory"
        :id="'clipboard-item-' + index"
        :key="index"
        v-memo="[entry.content, index, selectedIndex, getItemCategory(entry.content), isPinned(entry.content), entry.snippet]"
        class="clipboard-item"
        :class="{ selected: selectedIndex === index }"
        @click="handleClick(entry.index)"
        @dblclick="handleDoubleClick(entry.index)"
        @contextmenu.prevent="showContextMenu($event, entry.content, index)"
    >
      <div v-if="isWebUrl(entry.content)" class="open-btn" @click.stop="openWebUrl(entry.content)">
        <el-icon>
          <Link/>
        </el-icon>
      </div>
      <div :class="{ active: isPinned(entry.content) }" class="pin-btn"
           @click.stop="promoteItem(entry.index, entry.content)">
        <el-icon>
          <Pin/>
        </el-icon>
      </div>
      <div class="delete-btn" @click.stop="deleteItem(entry.index, entry.content)">
        <el-icon>
          <Close/>
        </el-icon>
      </div>
      <div class="index">{{ index + 1 }}</div>
      <div class="category-wrap" @click.stop>
        <div class="category-chip">{{ getItemCategory(entry.content) }}</div>
      </div>
      <div class="item-content">{{ entry.content }}</div>
      <div v-if="entry.snippet" class="item-snippet">
        <template v-for="(part, partIndex) in renderHighlightParts(entry.snippet)" :key="partIndex">
          <mark v-if="part.hit" class="snippet-hit">{{ part.text }}</mark>
          <span v-else>{{ part.text }}</span>
        </template>
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
    <div class="spacer"></div>
  </div>
</template>

<script setup>
import {computed, onUnmounted, ref} from 'vue'
import {Close, Link, Loading} from '@element-plus/icons-vue'
import {Pin} from 'lucide-vue-next'
import {openUrl as openExternalUrl} from '@tauri-apps/plugin-opener'

const props = defineProps({
  visibleHistory: {
    type: Array,
    required: true
  },
  selectedIndex: {
    type: Number,
    required: true
  },
  getItemCategory: {
    type: Function,
    required: true
  },
  deleteItem: {
    type: Function,
    required: true
  },
  updateSelection: {
    type: Function,
    required: true
  },
  selectAndFillDirect: {
    type: Function,
    required: true
  },
  showContextMenu: {
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
  promoteItem: {
    type: Function,
    required: true
  },
  isPinned: {
    type: Function,
    required: true
  },
  highlightKeyword: {
    type: String,
    default: ''
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

const handleScroll = () => {
  emit('content-scroll')
}

const renderHighlightParts = (text) => {
  const value = typeof text === 'string' ? text : ''
  const keyword = (props.highlightKeyword || '').trim()
  const tokens = Array.from(new Set(keyword.split(/\s+/).map((v) => v.trim()).filter(Boolean)))
      .sort((a, b) => b.length - a.length)
  if (!value || tokens.length === 0) {
    return [{text: value, hit: false}]
  }

  const sourceLower = value.toLowerCase()
  const tokenLowers = tokens.map((t) => t.toLowerCase())
  const out = []
  let start = 0
  while (start < value.length) {
    let bestIndex = -1
    let bestToken = ''
    for (let i = 0; i < tokenLowers.length; i += 1) {
      const token = tokenLowers[i]
      const idx = sourceLower.indexOf(token, start)
      if (idx === -1) continue
      if (bestIndex === -1 || idx < bestIndex || (idx === bestIndex && token.length > bestToken.length)) {
        bestIndex = idx
        bestToken = token
      }
    }
    if (bestIndex === -1) {
      out.push({text: value.slice(start), hit: false})
      break
    }
    if (bestIndex > start) {
      out.push({text: value.slice(start, bestIndex), hit: false})
    }
    const hitEnd = bestIndex + bestToken.length
    out.push({text: value.slice(bestIndex, hitEnd), hit: true})
    start = hitEnd
  }
  return out.length > 0 ? out : [{text: value, hit: false}]
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

const isLoadingMore = computed(() => props.isLoadingPage && props.visibleHistory.length > 0)
const showTailLoadMoreHint = computed(() => (props.hasMore || isLoadingMore.value) && props.visibleHistory.length > 0)

onUnmounted(() => {
  stopDragging()
  window.removeEventListener('blur', stopDragging)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
  window.removeEventListener('mousemove', handleGlobalMouseMove)
  window.removeEventListener('mouseup', handleGlobalMouseUp, true)
  window.removeEventListener('dragend', handleGlobalDragEnd)
})

const handleClick = (entryIndex) => {
  props.updateSelection(entryIndex, false, contentRef.value, null)
}

const handleDoubleClick = (entryIndex) => {
  props.selectAndFillDirect(entryIndex)
}

const isWebUrl = (value) => {
  if (!value) return false
  const text = value.trim()
  return /^https?:\/\/\S+$/i.test(text) || /^www\.\S+$/i.test(text)
}

const normalizeUrl = (value) => {
  const text = value.trim()
  if (/^https?:\/\//i.test(text)) return text
  if (/^www\./i.test(text)) return `https://${text}`
  return text
}

const openWebUrl = async (value) => {
  try {
    const url = normalizeUrl(value)
    if (isWebUrl(url)) {
      await openExternalUrl(url)
    }
  } catch (error) {
    console.error('打开网址失败:', error)
  }
}

const handleMouseDown = (e) => {
  if (e.target.closest('.delete-btn') || e.target.closest('.open-btn') || e.target.closest('.pin-btn')) {
    return
  }

  isDown = true
  isDragging = false
  startX = e.pageX
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

const handleGlobalMouseMove = (e) => {
  if (!isDown || !contentRef.value) return
  const walk = e.pageX - startX

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

const handleVisibilityChange = () => {
  if (document.hidden) {
    stopDragging()
  }
}

window.addEventListener('blur', stopDragging)
document.addEventListener('visibilitychange', handleVisibilityChange)

const handleWheel = (e) => {
  if (!contentRef.value) return
  const delta = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX
  const maxScrollLeft = Math.max(0, contentRef.value.scrollWidth - contentRef.value.clientWidth)
  const nearEnd = contentRef.value.scrollLeft >= maxScrollLeft - 8
  if (delta > 0 && nearEnd) {
    emit('load-more-intent')
  }
  contentRef.value.scrollLeft += delta
}

defineExpose({
  contentRef
})
</script>

<style scoped>
.content {
  flex: 1;
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

.content.is-dragging .delete-btn,
.content.is-dragging .open-btn,
.content.is-dragging .pin-btn {
  opacity: 0 !important;
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
  /* 优化：限制重排重绘范围 */
  contain: layout style paint;
  will-change: transform;
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

.open-btn {
  position: absolute;
  top: 5px;
  right: 30px;
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

.pin-btn {
  position: absolute;
  top: 5px;
  right: 55px;
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
  z-index: 12;
  padding: 0;
}

.pin-btn:hover {
  border-color: var(--el-color-primary, #409eff);
  color: #fff;
  background: var(--el-color-primary, #409eff);
}

.open-btn .el-icon {
  font-size: 12px;
}

.clipboard-item:hover .open-btn {
  opacity: 1;
}

.clipboard-item:hover .pin-btn {
  opacity: 1;
}

.pin-btn.active {
  opacity: 1;
  background: rgba(247, 185, 85, 0.75);
  color: #fff6d1;
  border: 1px solid rgba(247, 185, 85, 0.9);
}

.open-btn:hover {
  background: var(--el-color-primary, #409eff);
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

.index {
  position: absolute;
  top: 5px;
  left: 5px;
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
  right: 56px;
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

.item-content {
  margin-top: 38px;
  padding-bottom: 10px;
  flex: 1;
  min-height: 0;
  position: relative;
  z-index: 1;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: none;
  -ms-overflow-style: none;
  font-size: 13px;
  line-height: 1.5;
  color: #dcdfe6;
  white-space: pre-wrap;
  word-break: break-all;
}

.item-content::-webkit-scrollbar {
  display: none;
}

.item-snippet {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px dashed rgba(255, 255, 255, 0.14);
  font-size: 12px;
  line-height: 1.4;
  color: rgba(166, 213, 255, 0.9);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 52px;
  overflow: hidden;
}

.snippet-hit {
  background: rgba(247, 185, 85, 0.35);
  color: #fff2c2;
  border-radius: 2px;
  padding: 0 1px;
}
</style>
