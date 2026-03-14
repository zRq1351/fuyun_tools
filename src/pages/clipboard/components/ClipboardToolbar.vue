<template>
  <div class="toolbar">
    <div class="window-offset-handle" title="按住拖动上下调整窗口位置"
         @mousedown.stop.prevent="startWindowOffsetDrag">
      <el-icon>
        <Rank/>
      </el-icon>
      <span class="window-offset-label">调高</span>
    </div>
    <button v-if="showAiToggle" class="ai-toggle-btn" type="button" @click.stop
            @mousedown.stop="handleToggleAiSettings">
      <el-icon class="ai-toggle-arrow">
        <ArrowRight v-if="isAiSettingsCollapsed"/>
        <ArrowDown v-else/>
      </el-icon>
    </button>
    <el-input
        v-model="searchKeyword"
        class="search-input"
        clearable
        placeholder="搜索剪切板历史"
        size="small"
    >
      <template #prefix>
        <el-icon>
          <Search/>
        </el-icon>
      </template>
    </el-input>
    <div class="category-nav">
      <div
          :class="{ active: categoryFilter === '全部' }"
          class="category-pill"
          @click="updateCategoryFilter('全部')"
      >
        全部
      </div>
      <div
          v-for="category in categories"
          :key="category"
          :class="{ active: categoryFilter === category }"
          class="category-pill"
          @click="updateCategoryFilter(category)"
          @dragenter="handleDragEnter"
          @dragleave="handleDragLeave"
          @drop="handleDrop($event, category)"
          @dragover.prevent="handleDragOver"
      >
        <span class="category-label">{{ category }}</span>
        <span
            v-if="canDeleteCategory(category)"
            class="category-remove"
            @click.stop="removeCategory(category)"
        >
          <el-icon>
            <Close/>
          </el-icon>
        </span>
      </div>
      <div v-if="!isAddingCategory" class="category-pill add-category" @click="startCreateCategory">
        <el-icon>
          <Plus/>
        </el-icon>
        <span>新增分类</span>
      </div>
      <el-input
          v-else
          ref="newCategoryInputRef"
          v-model="newCategoryName"
          class="category-input"
          placeholder="输入分类名"
          size="small"
          @blur="confirmCreateCategory"
          @keydown.enter.prevent="confirmCreateCategory"
          @keydown.esc.prevent="cancelCreateCategory"
      />
    </div>
  </div>
</template>

<script setup>
import {ArrowDown, ArrowRight, Close, Plus, Rank, Search} from '@element-plus/icons-vue'
import {computed} from 'vue'

const props = defineProps({
  searchKeyword: String,
  categoryFilter: String,
  categories: Array,
  isAddingCategory: Boolean,
  newCategoryName: String,
  newCategoryInputRef: Object,
  canDeleteCategory: Function,
  startWindowOffsetDrag: Function,
  showAiToggle: {
    type: Boolean,
    default: true
  },
  isAiSettingsCollapsed: Boolean,
  toggleAiSettings: Function,
  translationTargetLanguage: String,
  explanationTargetLanguage: String,
  removeCategory: Function,
  startCreateCategory: Function,
  confirmCreateCategory: Function,
  cancelCreateCategory: Function,
  handleDrop: Function
})

const emit = defineEmits(['update:searchKeyword', 'update:categoryFilter', 'update:newCategoryName'])

const searchKeyword = computed({
  get: () => props.searchKeyword,
  set: (val) => emit('update:searchKeyword', val)
})

const newCategoryName = computed({
  get: () => props.newCategoryName,
  set: (val) => emit('update:newCategoryName', val)
})

const updateCategoryFilter = (val) => {
  emit('update:categoryFilter', val)
}

const handleDragOver = (event) => {
  event.preventDefault()
  event.dataTransfer.dropEffect = 'copy'
}

const handleDragEnter = (event) => {
  event.preventDefault()
  const target = event.currentTarget
  if (target && target.classList.contains('category-pill')) {
    target.classList.add('drag-over')
  }
}

const handleDragLeave = (event) => {
  const target = event.currentTarget
  if (target && target.classList.contains('category-pill')) {
    target.classList.remove('drag-over')
  }
}

const handleToggleAiSettings = () => {
  if (typeof props.toggleAiSettings === 'function') {
    props.toggleAiSettings()
  }
}
</script>

<style scoped>
.toolbar {
  display: flex;
  gap: 8px;
  padding: 8px;
  align-items: center;
}

.window-offset-handle {
  width: 36px;
  height: 36px;
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1px;
  border-radius: 10px;
  background: transparent;
  border: 1px solid transparent;
  color: rgba(255, 255, 255, 0.78);
  cursor: ns-resize;
  flex: 0 0 auto;
  user-select: none;
  transition: all 0.2s ease;
  box-shadow: none;
}

.window-offset-label {
  font-size: 10px;
  line-height: 1;
  letter-spacing: 0.5px;
}

.window-offset-handle:hover {
  border-color: rgba(255, 120, 120, 0.9);
  color: #fff;
  background: rgba(245, 108, 108, 0.2);
  box-shadow: 0 4px 14px rgba(245, 108, 108, 0.35);
}

.ai-toggle-btn {
  width: 36px;
  height: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 10px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: transparent;
  color: #dce8ff;
  cursor: pointer;
  transition: background 0.2s ease, border-color 0.2s ease, box-shadow 0.2s ease;
}

.ai-toggle-btn:hover {
  background: linear-gradient(135deg, rgba(28, 36, 52, 0.9), rgba(35, 45, 63, 0.84));
  border-color: rgba(127, 194, 255, 0.5);
  box-shadow: 0 0 0 1px rgba(127, 194, 255, 0.18);
}

.ai-toggle-arrow {
  font-size: 14px;
  color: #9cd4ff;
}

.search-input {
  width: 240px;
  flex: 0 0 auto;
}

.search-input :deep(.el-input__wrapper) {
  background: rgba(15, 15, 20, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.12);
  box-shadow: 0 0 0 1px rgba(64, 158, 255, 0.15);
  border-radius: 10px;
  padding: 2px 10px;
  backdrop-filter: blur(12px);
  transition: all 0.2s ease;
}

.search-input :deep(.el-input__wrapper.is-focus) {
  border-color: var(--el-color-primary, #409eff);
  box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.25);
}

.search-input :deep(.el-input__inner) {
  color: #e5e7eb;
  font-size: 13px;
  letter-spacing: 0.2px;
}

.search-input :deep(.el-input__prefix) {
  color: rgba(255, 255, 255, 0.55);
}

.search-input :deep(.el-input__suffix) {
  color: rgba(255, 255, 255, 0.45);
}

.search-input :deep(.el-input__inner::placeholder) {
  color: rgba(255, 255, 255, 0.45);
}

.category-nav {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
}

.category-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 999px;
  background: rgba(15, 15, 20, 0.55);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.75);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s ease;
  white-space: nowrap;
}

.category-pill * {
  pointer-events: none;
}

.category-pill .category-remove {
  pointer-events: auto;
}

.category-pill:hover {
  border-color: rgba(64, 158, 255, 0.5);
  color: #fff;
}

.category-pill.active {
  background: rgba(64, 158, 255, 0.2);
  border-color: var(--el-color-primary, #409eff);
  color: #fff;
  box-shadow: 0 0 0 1px rgba(64, 158, 255, 0.3);
}

.category-pill.drag-over {
  background: rgba(103, 194, 58, 0.18);
  border-color: rgba(103, 194, 58, 0.8);
  color: #fff;
  box-shadow: 0 0 0 1px rgba(103, 194, 58, 0.35);
}

.category-pill.add-category {
  border-style: dashed;
  color: rgba(255, 255, 255, 0.7);
}

.category-pill.add-category:hover {
  color: #fff;
  border-color: rgba(64, 158, 255, 0.6);
}

.category-input {
  width: 160px;
}

.category-input :deep(.el-input__wrapper) {
  background: rgba(15, 15, 20, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.12);
  box-shadow: 0 0 0 1px rgba(64, 158, 255, 0.12);
  border-radius: 999px;
  padding: 2px 10px;
  transition: all 0.2s ease;
}

.category-input :deep(.el-input__wrapper.is-focus) {
  border-color: var(--el-color-primary, #409eff);
  box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.2);
}

.category-input :deep(.el-input__inner) {
  color: #e5e7eb;
  font-size: 12px;
}

.category-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.7);
  transition: all 0.2s ease;
}

.category-pill:hover .category-remove {
  background: rgba(245, 108, 108, 0.2);
  color: #f56c6c;
}
</style>
