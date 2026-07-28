<template>
  <div v-if="visible" class="category-manager-overlay" @click.self="$emit('close')">
    <div class="category-manager">
      <div class="category-header">
        <span class="title">{{ t('launcher.manageCategories') }}</span>
        <button class="add-btn" @click="showAddDialog">
          <el-icon :size="14">
            <Plus/>
          </el-icon>
        </button>
        <button class="close-btn" @click="$emit('close')">
          <el-icon :size="14">
            <Close/>
          </el-icon>
        </button>
      </div>
      <div ref="categoryListRef" class="category-list">
        <div
            v-for="category in categories"
            :key="category.id"
            :data-category-id="category.id"
            class="category-item sortable-category"
        >
          <div class="icon-selector" @click="showIconPicker(category)">
            <el-icon :size="16">
              <component :is="getIcon(category.icon)"/>
            </el-icon>
          </div>
          <span class="category-name">{{ category.name }}</span>
          <span class="category-count">{{ getCategoryCount(category.id) }}</span>
          <button :title="t('common.rename')" class="edit-btn" @click="startEdit(category)">
            <el-icon :size="12">
              <Edit/>
            </el-icon>
          </button>
          <button :title="t('common.delete')" class="delete-btn" @click="handleDelete(category)">
            <el-icon :size="12">
              <Delete/>
            </el-icon>
          </button>
        </div>
      </div>

      <!-- 图标选择器 -->
      <div v-if="showIconPickerDialog" class="dialog-overlay" @click.self="closeIconPicker">
        <div class="icon-picker-dialog">
          <div class="dialog-title">选择图标</div>
          <div class="icon-grid">
            <div
                v-for="iconName in availableIcons"
                :key="iconName"
                :class="{ selected: currentEditingCategory?.icon === iconName }"
                class="icon-option"
                @click="selectIcon(iconName)"
            >
              <el-icon :size="20">
                <component :is="getIcon(iconName)"/>
              </el-icon>
            </div>
          </div>
          <div class="dialog-actions">
            <button class="dialog-btn cancel" @click="closeIconPicker">{{ t('common.cancel') }}</button>
          </div>
        </div>
      </div>

      <!-- 添加/重命名对话框 -->
      <div v-if="showDialog" class="dialog-overlay" @click.self="cancelDialog">
        <div class="dialog">
          <div class="dialog-title">{{ editingCategory ? t('common.rename') + '分类' : t('common.add') + '分类' }}</div>
          <input
              ref="dialogInput"
              v-model="dialogName"
              class="dialog-input"
              placeholder="输入分类名称"
              @keydown.enter="confirmDialog"
          />
          <div class="dialog-actions">
            <button class="dialog-btn cancel" @click="cancelDialog">{{ t('common.cancel') }}</button>
            <button class="dialog-btn confirm" @click="confirmDialog">{{ t('common.ok') }}</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {ref, nextTick, watch, onMounted, onBeforeUnmount} from 'vue'
import {useI18n} from 'vue-i18n'
import {
  Plus, Close, Edit, Delete,
  Monitor, Document, Setting, VideoCamera, Grid
} from '@element-plus/icons-vue'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'
import Sortable from 'sortablejs'

const {t} = useI18n()

const props = defineProps({
  visible: Boolean,
  categories: Array,
  appCategoryMap: Object
})

const emit = defineEmits(['close', 'updated'])

const showDialog = ref(false)
const editingCategory = ref(null)
const dialogName = ref('')
const dialogInput = ref(null)
const categoryListRef = ref(null)
const showIconPickerDialog = ref(false)
const currentEditingCategory = ref(null)
let categoriesSortable = null

// 可用的图标列表（从 Element Plus 动态获取）
const availableIcons = Object.keys(ElementPlusIconsVue).filter(name => {
  const excluded = ['Loading', 'Loading2']
  return !excluded.includes(name)
}).sort()

const getIcon = (iconName) => {
  return ElementPlusIconsVue[iconName] || Grid
}

const getCategoryCount = (categoryId) => {
  if (!props.appCategoryMap) return 0
  return Object.values(props.appCategoryMap).filter(v => v === categoryId).length
}

const showAddDialog = () => {
  editingCategory.value = null
  dialogName.value = ''
  showDialog.value = true
  nextTick(() => dialogInput.value?.focus())
}

const startEdit = (category) => {
  editingCategory.value = category
  dialogName.value = category.name
  showDialog.value = true
  nextTick(() => dialogInput.value?.focus())
}

const cancelDialog = () => {
  showDialog.value = false
  editingCategory.value = null
  dialogName.value = ''
}

const confirmDialog = async () => {
  const name = dialogName.value.trim()
  if (!name) return

  try {
    if (editingCategory.value) {
      await invoke('rename_launcher_category', {
        categoryId: editingCategory.value.id,
        newName: name
      })
    } else {
      await invoke('add_launcher_category', {
        name,
        icon: 'Grid'
      })
    }
    cancelDialog()
    emit('updated')
  } catch (error) {
    console.error('Category operation error:', error)
  }
}

const handleDelete = async (category) => {
  try {
    await invoke('remove_launcher_category', {categoryId: category.id})
    emit('updated')
  } catch (error) {
    console.error('Delete category error:', error)
  }
}

// 初始化分类拖拽排序
const initCategoriesSortable = () => {
  if (!categoryListRef.value) return

  categoriesSortable = Sortable.create(categoryListRef.value, {
    animation: 200,
    ghostClass: 'category-ghost',
    dragClass: 'category-drag',
    chosenClass: 'category-chosen',
    delay: 300,
    delayOnTouchOnly: false,
    forceFallback: true,
    fallbackClass: 'category-fallback',
    fallbackTolerance: 3,
    fallbackOnBody: true,
    swapThreshold: 0.65,
    scroll: false,
    filter: '.icon-selector, .edit-btn, .delete-btn',
    preventOnFilter: false,
    onStart: (evt) => {
      // 获取fallback元素并添加鼠标跟踪
      setTimeout(() => {
        const fallbackElement = document.querySelector('.category-fallback')
        if (fallbackElement) {
          const moveHandler = (e) => {
            if (fallbackElement) {
              fallbackElement.style.left = (e.clientX - fallbackElement.offsetWidth / 2) + 'px'
              fallbackElement.style.top = (e.clientY - fallbackElement.offsetHeight / 2) + 'px'
            }
          }
          document.addEventListener('mousemove', moveHandler)

          // 拖动结束时移除监听
          const removeHandler = () => {
            document.removeEventListener('mousemove', moveHandler)
            document.removeEventListener('mouseup', removeHandler)
          }
          document.addEventListener('mouseup', removeHandler)
        }
      }, 50)
    },
    onEnd: async (evt) => {
      const oldIndex = evt.oldIndex
      const newIndex = evt.newIndex

      if (oldIndex !== newIndex && oldIndex !== undefined && newIndex !== undefined) {
        // 重新排列分类数组
        const newCategories = [...props.categories]
        const [movedItem] = newCategories.splice(oldIndex, 1)
        newCategories.splice(newIndex, 0, movedItem)

        // 调用后端API保存新顺序
        try {
          const categoryIds = newCategories.map(c => c.id)
          await invoke('reorder_categories', {categoryIds})
          emit('updated')
        } catch (error) {
          console.error('Reorder categories error:', error)
        }
      }
    }
  })
}

// 显示图标选择器
const showIconPicker = (category) => {
  currentEditingCategory.value = category
  showIconPickerDialog.value = true
}

// 关闭图标选择器
const closeIconPicker = () => {
  showIconPickerDialog.value = false
  currentEditingCategory.value = null
}

// 选择图标
const selectIcon = async (iconName) => {
  if (!currentEditingCategory.value) return

  try {
    await invoke('update_category_icon', {
      categoryId: currentEditingCategory.value.id,
      icon: iconName
    })
    closeIconPicker()
    emit('updated')
  } catch (error) {
    console.error('Update icon error:', error)
  }
}

onMounted(() => {
  // 不在这里初始化，等待 visible 变化时再初始化
})

onBeforeUnmount(() => {
  if (categoriesSortable) {
    categoriesSortable.destroy()
    categoriesSortable = null
  }
})

// 监听弹框显示状态，确保 DOM 渲染完成后再初始化 Sortable
watch(() => props.visible, (val) => {
  if (val) {
    // 弹框显示后，等待下一个 tick 确保 DOM 已渲染
    nextTick(() => {
      initCategoriesSortable()
    })
  } else {
    // 弹框关闭时销毁 Sortable 实例
    if (categoriesSortable) {
      categoriesSortable.destroy()
      categoriesSortable = null
    }
    cancelDialog()
  }
})

// 监听分类数据变化，重新初始化 Sortable
watch(() => props.categories, () => {
  if (props.visible && categoryListRef.value) {
    // 先销毁旧的实例
    if (categoriesSortable) {
      categoriesSortable.destroy()
    }
    // 等待 DOM 更新后重新初始化
    nextTick(() => {
      initCategoriesSortable()
    })
  }
}, {deep: true})
</script>

<style scoped>
.category-manager-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10001;
}

.category-manager {
  width: 400px;
  max-height: 500px;
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 12px;
  box-shadow: var(--fy-shadow-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.category-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--fy-border-light);
}

.title {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  color: var(--fy-text-primary);
}

.add-btn, .close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  border-radius: 4px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.15s;
}

.add-btn:hover {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

.close-btn:hover {
  background: var(--fy-danger-bg);
  color: var(--fy-danger);
}

.category-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
}

.category-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  color: var(--fy-text-secondary);
  cursor: grab;
  transition: all 0.15s;
  user-select: none;
  border-radius: 6px;
  margin: 2px 8px;
  border: 1px solid transparent;
}

.category-item.sortable-category {
  cursor: grab;
  transition: all 0.2s ease;
}

.category-item.sortable-category:active {
  cursor: grabbing;
}

.category-item:hover {
  background: var(--fy-accent-bg);
  padding-left: 20px;
  border-left: 3px solid var(--fy-accent);
  transform: translateX(2px);
}

.icon-selector {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  cursor: pointer;
  color: var(--fy-text-secondary);
  transition: all 0.15s;
}

.icon-selector:hover {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

.category-ghost {
  opacity: 0.4;
  background: var(--fy-accent-bg);
}

.category-chosen {
  transform: scale(1.08);
  box-shadow: var(--fy-shadow-lg), 0 0 0 3px var(--fy-accent);
  z-index: 10;
  background: var(--fy-accent-bg);
  animation: chosen-pulse 0.6s ease-in-out infinite alternate;
}

@keyframes chosen-pulse {
  from {
    box-shadow: var(--fy-shadow-lg), 0 0 0 3px var(--fy-accent);
  }
  to {
    box-shadow: var(--fy-shadow-lg), 0 0 0 5px var(--fy-accent);
  }
}

.category-drag {
  opacity: 0.3;
  transform: scale(0.95);
}

.category-fallback {
  position: fixed !important;
  z-index: 9999 !important;
  pointer-events: none;
  opacity: 0.95;
  transform: scale(1.05);
  box-shadow: var(--fy-shadow-lg);
  background: var(--fy-bg-surface);
  border: 2px solid var(--fy-accent);
  border-radius: 8px;
  cursor: grabbing !important;
}

.category-name {
  flex: 1;
  font-size: 13px;
}

.category-count {
  font-size: 11px;
  color: var(--fy-text-muted);
  background: var(--fy-bg-hover);
  padding: 1px 6px;
  border-radius: 10px;
}

.edit-btn, .delete-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  border-radius: 4px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.15s;
  opacity: 0;
}

.category-item:hover .edit-btn,
.category-item:hover .delete-btn {
  opacity: 1;
}

.edit-btn:hover {
  color: var(--fy-accent);
}

.delete-btn:hover {
  color: var(--fy-danger);
}

.dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 20;
}

.icon-picker-dialog {
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 12px;
  padding: 16px;
  width: 320px;
  max-height: 400px;
  display: flex;
  flex-direction: column;
}

.icon-grid {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 8px;
  padding: 8px 0;
  overflow-y: auto;
  max-height: 280px;
}

.icon-option {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: 8px;
  cursor: pointer;
  color: var(--fy-text-secondary);
  transition: all 0.15s;
  border: 2px solid transparent;
}

.icon-option:hover {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
  transform: scale(1.1);
}

.icon-option.selected {
  border-color: var(--fy-accent);
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

.dialog {
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 8px;
  padding: 16px;
  width: 280px;
}

.dialog-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--fy-text-primary);
  margin-bottom: 12px;
}

.dialog-input {
  width: 100%;
  height: 32px;
  padding: 0 8px;
  border: 1px solid var(--fy-border);
  border-radius: 6px;
  background: var(--fy-bg-input);
  color: var(--fy-text-primary);
  font-size: 13px;
  outline: none;
  box-sizing: border-box;
}

.dialog-input:focus {
  border-color: var(--fy-accent);
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}

.dialog-btn {
  height: 28px;
  padding: 0 12px;
  border: none;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-btn.cancel {
  background: var(--fy-bg-hover);
  color: var(--fy-text-secondary);
}

.dialog-btn.cancel:hover {
  background: var(--fy-bg-active);
}

.dialog-btn.confirm {
  background: var(--fy-accent);
  color: white;
}

.dialog-btn.confirm:hover {
  opacity: 0.9;
}
</style>
