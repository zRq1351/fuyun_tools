<template>
  <div class="toolbar">
    <div :title="$t('clipboard.dragHint')" class="window-offset-handle"
         @mousedown.stop.prevent="startWindowOffsetDrag">
      <el-icon>
        <Rank/>
      </el-icon>
      <span class="window-offset-label">{{ $t('clipboard.offsetUp') }}</span>
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
        :placeholder="searchPlaceholder || $t('clipboard.searchHistory')"
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
        {{ $t('common.all') }}
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
        <span>{{ createCategoryText || $t('clipboard.createCategory') }}</span>
      </div>
      <el-input
          v-else
          ref="newCategoryInputRef"
          v-model="newCategoryName"
          class="category-input"
          :placeholder="$t('clipboard.inputCategoryName')"
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
import {useI18n} from 'vue-i18n'

const {t} = useI18n()

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
  ,
  searchPlaceholder: {
    type: String,
    default: ''
  },
  createCategoryText: {
    type: String,
    default: ''
  }
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
  color: var(--fy-text-primary);
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
  border-color: var(--fy-danger);
  color: #fff;
  background: var(--fy-danger-bg);
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
  border: 1px solid var(--fy-border-light);
  background: transparent;
  color: var(--fy-text-accent);
  cursor: pointer;
  transition: background 0.2s ease, border-color 0.2s ease, box-shadow 0.2s ease;
}

.ai-toggle-btn:hover {
  background: var(--fy-bg-surface);
  border-color: var(--fy-accent);
  box-shadow: 0 0 0 1px var(--fy-accent-bg);
}

.ai-toggle-arrow {
  font-size: 14px;
  color: var(--fy-text-accent);
}

.search-input {
  width: 240px;
  flex: 0 0 auto;
}

.search-input :deep(.el-input__wrapper) {
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border-light);
  box-shadow: 0 0 0 1px var(--fy-accent-bg);
  border-radius: 10px;
  padding: 2px 10px;
  backdrop-filter: blur(12px);
  transition: all 0.2s ease;
}

.search-input :deep(.el-input__wrapper.is-focus) {
  border-color: var(--fy-accent);
  box-shadow: 0 0 0 2px var(--fy-accent-bg-hover);
}

.search-input :deep(.el-input__inner) {
  color: var(--fy-text-primary);
  font-size: 13px;
  letter-spacing: 0.2px;
}

.search-input :deep(.el-input__prefix) {
  color: var(--fy-text-muted);
}

.search-input :deep(.el-input__suffix) {
  color: var(--fy-text-muted);
}

.search-input :deep(.el-input__inner::placeholder) {
  color: var(--fy-text-muted);
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
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border-light);
  color: var(--fy-text-secondary);
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
  border-color: var(--fy-accent);
  color: #fff;
}

.category-pill.active {
  background: var(--fy-accent-bg);
  border-color: var(--fy-accent);
  color: #fff;
  box-shadow: 0 0 0 1px var(--fy-accent-bg-hover);
}

.category-pill.drag-over {
  background: var(--fy-success-bg);
  border-color: var(--fy-success);
  color: #fff;
  box-shadow: 0 0 0 1px var(--fy-success);
}

.category-pill.add-category {
  border-style: dashed;
  color: var(--fy-text-muted);
}

.category-pill.add-category:hover {
  color: #fff;
  border-color: var(--fy-accent);
}

.category-input {
  width: 160px;
}

.category-input :deep(.el-input__wrapper) {
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border-light);
  box-shadow: 0 0 0 1px var(--fy-accent-bg);
  border-radius: 999px;
  padding: 2px 10px;
  transition: all 0.2s ease;
}

.category-input :deep(.el-input__wrapper.is-focus) {
  border-color: var(--fy-accent);
  box-shadow: 0 0 0 2px var(--fy-accent-bg-hover);
}

.category-input :deep(.el-input__inner) {
  color: var(--fy-text-primary);
  font-size: 12px;
}

.category-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--fy-bg-hover);
  color: var(--fy-text-muted);
  transition: all 0.2s ease;
}

.category-pill:hover .category-remove {
  background: var(--fy-danger-bg);
  color: var(--fy-danger);
}
</style>
