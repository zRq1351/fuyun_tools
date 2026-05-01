<template>
  <div class="category-manager" v-if="visible">
    <div class="category-header">
      <span class="title">管理分类</span>
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
    <div class="category-list">
      <div
          v-for="category in categories"
          :key="category.id"
          class="category-item"
      >
        <el-icon :size="16">
          <component :is="getIcon(category.icon)"/>
        </el-icon>
        <span class="category-name">{{ category.name }}</span>
        <span class="category-count">{{ getCategoryCount(category.id) }}</span>
        <button class="edit-btn" @click="startEdit(category)" title="重命名">
          <el-icon :size="12">
            <Edit/>
          </el-icon>
        </button>
        <button class="delete-btn" @click="handleDelete(category)" title="删除">
          <el-icon :size="12">
            <Delete/>
          </el-icon>
        </button>
      </div>
    </div>

    <div v-if="showDialog" class="dialog-overlay" @click.self="cancelDialog">
      <div class="dialog">
        <div class="dialog-title">{{ editingCategory ? '重命名分类' : '添加分类' }}</div>
        <input
            v-model="dialogName"
            class="dialog-input"
            placeholder="输入分类名称"
            @keydown.enter="confirmDialog"
            ref="dialogInput"
        />
        <div class="dialog-actions">
          <button class="dialog-btn cancel" @click="cancelDialog">取消</button>
          <button class="dialog-btn confirm" @click="confirmDialog">确定</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {ref, nextTick, watch} from 'vue'
import {Plus, Close, Edit, Delete, Monitor, Document, Setting, VideoCamera, Grid} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'

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

const iconMap = {
  Monitor, Document, Setting, VideoCamera, Grid
}

const getIcon = (iconName) => {
  return iconMap[iconName] || Grid
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

watch(() => props.visible, (val) => {
  if (!val) cancelDialog()
})
</script>

<style scoped>
.category-manager {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: var(--fy-bg-surface);
  z-index: 10;
  display: flex;
  flex-direction: column;
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
  position: absolute;
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
