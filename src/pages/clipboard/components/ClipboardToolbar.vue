<template>
  <div class="toolbar">
    <div :title="$t('clipboard.dragHint')" class="window-offset-handle"
         @mousedown.stop.prevent="startWindowOffsetDrag">
      <el-icon :size="14">
        <Rank/>
      </el-icon>
    </div>
    <button v-if="showAiToggle" class="ai-toggle-btn" type="button" @click.stop
            @mousedown.stop="handleToggleAiSettings">
      <el-icon :size="14" class="ai-toggle-arrow">
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
        <el-icon :size="14">
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
        <span class="category-label">{{ translateCategory(category) }}</span>
        <span
            v-if="canDeleteCategory(category)"
            class="category-remove"
            @click.stop="removeCategory(category)"
        >
          <el-icon :size="10"><Close/></el-icon>
        </span>
      </div>
      <div v-if="!isAddingCategory" class="category-pill add-category" @click="startCreateCategory">
        <el-icon :size="12">
          <Plus/>
        </el-icon>
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

const CATEGORY_TRANSLATIONS = {
  '未分类': () => t('common.uncategorized'),
  '全部': () => t('common.all'),
}

const translateCategory = (category) => {
  const translator = CATEGORY_TRANSLATIONS[category]
  return translator ? translator() : category
}

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
  handleDrop: Function,
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
  gap: 6px;
  padding: 10px 14px;
  align-items: center;
}

.window-offset-handle {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--fy-radius-sm);
  background: transparent;
  border: none;
  color: var(--fy-text-muted);
  cursor: ns-resize;
  flex: 0 0 auto;
  user-select: none;
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
}

.window-offset-handle:hover {
  color: var(--fy-danger);
  background: var(--fy-danger-bg);
}

.ai-toggle-btn {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: var(--fy-radius-sm);
  border: none;
  background: transparent;
  color: var(--fy-text-accent);
  cursor: pointer;
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
}

.ai-toggle-btn:hover {
  background: var(--fy-accent-bg);
}

.search-input {
  width: 200px;
  flex: 0 0 auto;
}

.search-input :deep(.el-input__wrapper) {
  background: rgba(255, 255, 255, 0.05);
  border: 0.5px solid var(--fy-border-light);
  box-shadow: none;
  border-radius: var(--fy-radius-full);
  padding: 1px 10px;
  height: 28px;
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
}

.search-input :deep(.el-input__wrapper:hover) {
  border-color: var(--fy-border);
  background: rgba(255, 255, 255, 0.08);
}

.search-input :deep(.el-input__wrapper.is-focus) {
  border-color: var(--fy-accent);
  background: rgba(255, 255, 255, 0.08);
}

.search-input :deep(.el-input__inner) {
  color: var(--fy-text-primary);
  font-size: var(--fy-text-sm);
}

.search-input :deep(.el-input__prefix),
.search-input :deep(.el-input__suffix) {
  color: var(--fy-text-muted);
}

.search-input :deep(.el-input__inner::placeholder) {
  color: var(--fy-text-muted);
}

.category-nav {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 1;
  min-width: 0;
}

.category-pill {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 26px;
  padding: 0 10px;
  border-radius: var(--fy-radius-full);
  background: rgba(255, 255, 255, 0.12);
  border: 0.5px solid rgba(255, 255, 255, 0.15);
  color: var(--fy-text-primary);
  font-size: var(--fy-text-sm);
  cursor: pointer;
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
  white-space: nowrap;
}

.category-pill * {
  pointer-events: none;
}

.category-pill .category-remove {
  pointer-events: auto;
}

.category-pill:hover {
  background: rgba(255, 255, 255, 0.18);
  border-color: rgba(255, 255, 255, 0.25);
  color: var(--fy-text-primary);
}

.category-pill.active {
  background: var(--fy-accent);
  border-color: transparent;
  color: var(--fy-text-primary);
}

.category-pill.drag-over {
  background: var(--fy-success-bg);
  border-color: var(--fy-success);
  color: var(--fy-success);
}

.category-pill.add-category {
  border-style: dashed;
  color: var(--fy-text-muted);
  padding: 0 8px;
}

.category-pill.add-category:hover {
  color: var(--fy-accent);
  border-color: var(--fy-accent);
  border-style: solid;
}

.category-input {
  width: 140px;
}

.category-input :deep(.el-input__wrapper) {
  background: rgba(255, 255, 255, 0.05);
  border: 0.5px solid var(--fy-border-light);
  box-shadow: none;
  border-radius: var(--fy-radius-full);
  padding: 1px 10px;
  height: 26px;
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
}

.category-input :deep(.el-input__wrapper.is-focus) {
  border-color: var(--fy-accent);
}

.category-input :deep(.el-input__inner) {
  color: var(--fy-text-primary);
  font-size: var(--fy-text-sm);
}

.category-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: rgba(0, 0, 0, 0.15);
  color: inherit;
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
}

.category-pill:hover .category-remove {
  background: var(--fy-danger);
  color: var(--fy-text-primary);
}
</style>
