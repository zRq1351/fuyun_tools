<template>
  <div class="app-grid-container" @contextmenu.prevent>
    <!-- Categories Grid with Sortable -->
    <div ref="categoriesContainer" class="categories-grid">
      <div
          v-for="(category, catIndex) in categories"
          :key="category.name"
          :data-index="catIndex"
          class="category-box"
          @click="expandCategory(category)"
      >
        <div class="category-header">
          <span class="category-name">{{ category.name }}</span>
          <span class="category-count">{{ category.apps.length }}</span>
        </div>
        <div class="category-apps">
          <div
              v-for="app in category.apps.slice(0, 4)"
              :key="app.id"
              class="app-item"
              @dblclick.stop="$emit('select', app)"
              @contextmenu.prevent.stop="showContextMenu($event, app)"
          >
            <div class="app-icon">
              <img v-if="app.icon_base64" :src="app.icon_base64" class="icon-img"/>
              <el-icon v-else :size="24">
                <Monitor/>
              </el-icon>
            </div>
            <div class="app-name">{{ app.title }}</div>
          </div>
        </div>
      </div>
    </div>

    <!-- Expanded Category Popup -->
    <div v-if="expandedCategory" class="expand-overlay" @click.self="closeExpanded">
      <div class="expand-popup">
        <div class="expand-header">
          <span class="expand-title">{{ expandedCategory.name }}</span>
          <button class="expand-close" @click="closeExpanded">
            <el-icon :size="14">
              <Close/>
            </el-icon>
          </button>
        </div>
        <div ref="appsContainer" class="expand-apps">
          <div
              v-for="app in expandedCategory.apps"
              :key="app.id"
              :data-app-id="app.id"
              class="app-item sortable-app"
              @dblclick.stop="$emit('select', app)"
              @contextmenu.prevent.stop="showContextMenu($event, app)"
          >
            <div class="app-icon">
              <img v-if="app.icon_base64" :src="app.icon_base64" class="icon-img"/>
              <el-icon v-else :size="24">
                <Monitor/>
              </el-icon>
            </div>
            <div class="app-name">{{ app.title }}</div>
          </div>
        </div>
      </div>
    </div>

    <!-- Context Menu -->
    <div
        v-if="contextMenu.visible"
        :style="{ top: contextMenu.y + 'px', left: contextMenu.x + 'px' }"
        class="context-menu"
        @click.stop
    >
      <div class="menu-item" @click="openApp(contextMenu.app)">
        <el-icon :size="14">
          <Monitor/>
        </el-icon>
        <span>打开</span>
      </div>
      <div class="menu-divider"></div>
      <div class="menu-category-list">
        <div
            v-for="cat in customCategories"
            :key="cat.id"
            class="menu-item"
            @click="assignToCategory(contextMenu.app, cat.id)"
        >
          <el-icon :size="14">
            <component :is="getIcon(cat.icon)"/>
          </el-icon>
          <span>{{ cat.name }}</span>
        </div>
      </div>
      <div v-if="customCategories.length > 5" class="menu-divider"></div>
      <div class="menu-item" @click="removeFromCategory(contextMenu.app)">
        <el-icon :size="14">
          <Close/>
        </el-icon>
        <span>移出分类</span>
      </div>
      <div class="menu-divider"></div>
      <div class="menu-item" @click="showAddCommandDialog">
        <el-icon :size="14">
          <Star/>
        </el-icon>
        <span>添加启动命令</span>
      </div>
    </div>

    <!-- 添加命令对话框 -->
    <div v-if="showCommandDialog" class="dialog-overlay">
      <div class="command-dialog">
        <div class="dialog-title">为应用添加启动命令</div>
        <div class="app-info-preview">
          <img v-if="contextMenu.app?.icon_base64" :src="contextMenu.app.icon_base64" class="preview-icon"/>
          <span class="preview-name">{{ contextMenu.app?.title }}</span>
        </div>

        <div class="form-group">
          <label>命令前缀</label>
          <div class="prefix-input-wrapper">
            <span class="prefix-symbol">:</span>
            <input
                v-model="commandForm.prefix"
                class="prefix-input"
            />
          </div>
          <span class="form-hint">输入前缀，用于快速搜索（自动添加 : 前缀）</span>
        </div>

        <div class="dialog-actions">
          <button class="dialog-btn cancel" @click="closeCommandDialog">取消</button>
          <button class="dialog-btn confirm" @click="confirmAddCommand">确定</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {nextTick, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {ElMessage} from 'element-plus'
import {Close, Grid, Monitor, Star} from '@element-plus/icons-vue'
import Sortable from 'sortablejs'
import {invoke} from '@tauri-apps/api/core'

const props = defineProps({
  categories: {type: Array, required: true}
})

const emit = defineEmits(['select', 'reorder-apps', 'reorder-categories', 'category-changed'])

const customCategories = ref([])
const contextMenu = ref({visible: false, x: 0, y: 0, app: null})
const expandedCategory = ref(null)
const appsContainer = ref(null)
const categoriesContainer = ref(null)
const showCommandDialog = ref(false)
const commandForm = ref({
  prefix: ''
})

const iconMap = {Monitor, Grid}

const getIcon = (iconName) => {
  return iconMap[iconName] || Grid
}

// 加载分类列表
const loadCategories = async () => {
  try {
    const config = await invoke('get_launcher_config')
    customCategories.value = config.categories || []
  } catch (error) {
    console.error('Load categories error:', error)
  }
}

let categoriesSortable = null
let appsSortable = null
let appFallbackElement = null  // 跟踪fallback元素
let catFallbackElement = null  // 跟踪分类fallback元素

// Category functions
const expandCategory = (category) => {
  if (category.apps.length > 4) {
    expandedCategory.value = category
    // Initialize sortable for expanded apps after DOM update
    nextTick(() => {
      initAppsSortable()
    })
  }
}

const closeExpanded = () => {
  expandedCategory.value = null
  destroyAppsSortable()
}

// Initialize Sortable for categories
const initCategoriesSortable = () => {
  if (!categoriesContainer.value) return

  categoriesSortable = Sortable.create(categoriesContainer.value, {
    animation: 200,
    ghostClass: 'category-ghost',
    dragClass: 'category-drag',
    chosenClass: 'category-chosen',
    delay: 600,
    delayOnTouchOnly: true,
    forceFallback: true,
    fallbackClass: 'category-fallback',
    fallbackTolerance: 3,
    fallbackOnBody: true,
    swapThreshold: 0.65,
    onStart: (evt) => {
      // 拖动开始时关闭展开的分类
      if (expandedCategory.value) {
        closeExpanded()
      }
      // 获取fallback元素并添加鼠标跟踪
      setTimeout(() => {
        catFallbackElement = document.querySelector('.category-fallback')
        if (catFallbackElement) {
          const moveHandler = (e) => {
            if (catFallbackElement) {
              catFallbackElement.style.left = (e.clientX - catFallbackElement.offsetWidth / 2) + 'px'
              catFallbackElement.style.top = (e.clientY - catFallbackElement.offsetHeight / 2) + 'px'
            }
          }
          document.addEventListener('mousemove', moveHandler)

          // 拖动结束时移除监听
          const removeHandler = () => {
            document.removeEventListener('mousemove', moveHandler)
            document.removeEventListener('mouseup', removeHandler)
            catFallbackElement = null
          }
          document.addEventListener('mouseup', removeHandler)
        }
      }, 50)
    },
    onEnd: (evt) => {
      const {oldIndex, newIndex} = evt
      if (oldIndex !== newIndex && oldIndex !== undefined && newIndex !== undefined) {
        emit('reorder-categories', oldIndex, newIndex)
      }
    }
  })
}

// Initialize Sortable for expanded apps
const initAppsSortable = () => {
  if (!appsContainer.value) return

  appsSortable = Sortable.create(appsContainer.value, {
    animation: 200,
    ghostClass: 'app-ghost',
    dragClass: 'app-drag',
    chosenClass: 'app-chosen',
    delay: 300,
    delayOnTouchOnly: false,
    forceFallback: true,
    fallbackClass: 'app-fallback',
    fallbackTolerance: 3,
    fallbackOnBody: true,
    swapThreshold: 0.65,
    invertSwap: false,
    direction: 'vertical',
    scroll: false,
    onMove: (evt) => {
      return true
    },
    onStart: (evt) => {
      // 获取fallback元素并添加鼠标跟踪
      setTimeout(() => {
        appFallbackElement = document.querySelector('.app-fallback')
        if (appFallbackElement) {
          const moveHandler = (e) => {
            if (appFallbackElement) {
              appFallbackElement.style.left = (e.clientX - appFallbackElement.offsetWidth / 2) + 'px'
              appFallbackElement.style.top = (e.clientY - appFallbackElement.offsetHeight / 2) + 'px'
            }
          }
          document.addEventListener('mousemove', moveHandler)

          // 拖动结束时移除监听
          const removeHandler = () => {
            document.removeEventListener('mousemove', moveHandler)
            document.removeEventListener('mouseup', removeHandler)
            appFallbackElement = null
          }
          document.addEventListener('mouseup', removeHandler)
        }
      }, 50)
    },
    onEnd: (evt) => {
      const {oldIndex, newIndex} = evt
      if (oldIndex !== newIndex && oldIndex !== undefined && newIndex !== undefined && expandedCategory.value) {
        const apps = [...expandedCategory.value.apps]
        const [moved] = apps.splice(oldIndex, 1)
        apps.splice(newIndex, 0, moved)
        expandedCategory.value.apps = apps
        emit('reorder-apps', apps)
      }
    }
  })
}

const destroyAppsSortable = () => {
  if (appsSortable) {
    appsSortable.destroy()
    appsSortable = null
  }
}

// Context menu
const openApp = (app) => {
  emit('select', app)
  contextMenu.value.visible = false
}

const showContextMenu = (event, app) => {
  // 使用固定定位，相对于视口
  let x = event.clientX
  let y = event.clientY

  // 获取菜单的大致尺寸（预设值）
  const menuWidth = 120
  const menuHeight = 150

  // 边界检测，确保菜单不超出视口
  if (x + menuWidth > window.innerWidth) {
    x = window.innerWidth - menuWidth - 10
  }
  if (y + menuHeight > window.innerHeight) {
    y = window.innerHeight - menuHeight - 10
  }

  contextMenu.value = {visible: true, x: Math.max(10, x), y: Math.max(10, y), app}
}

const hideContextMenu = () => {
  contextMenu.value.visible = false
}

const removeFromCategory = async (app) => {
  if (!app || !app.id) return
  try {
    const {invoke} = await import('@tauri-apps/api/core')
    await invoke('set_app_category', {appId: app.id, categoryId: ''})
    hideContextMenu()
    // 通知父组件重新加载配置
    emit('category-changed')
  } catch (error) {
    console.error('Remove category error:', error)
  }
}

const assignToCategory = async (app, categoryId) => {
  if (!app || !app.id) return
  try {
    await invoke('set_app_category', {appId: app.id, categoryId})
    hideContextMenu()
    // 通知父组件重新加载配置
    emit('category-changed')
  } catch (error) {
    console.error('Assign category error:', error)
  }
}

// 显示添加命令对话框
const showAddCommandDialog = () => {
  const app = contextMenu.value.app
  if (!app) return

  // 自动生成前缀：使用应用名称的小写形式（不包含 :）
  const prefix = app.title.toLowerCase().replace(/[^a-z0-9]/g, '').substring(0, 10)

  commandForm.value = {
    prefix
  }
  showCommandDialog.value = true
}

// 关闭命令对话框
const closeCommandDialog = () => {
  showCommandDialog.value = false
  hideContextMenu()
}

// 确认添加命令
const confirmAddCommand = async () => {
  const app = contextMenu.value.app
  if (!app || !commandForm.value.prefix.trim()) return

  try {
    // 加载配置检查该应用是否已有命令
    const config = await invoke('get_launcher_config')
    const existingCommands = config.custom_commands || []

    // 检查该应用是否已有命令（通过 path 判断）
    const existingCommand = existingCommands.find(cmd => {
      if (cmd.command_type.RunProgram) {
        return cmd.command_type.RunProgram.path === app.path
      }
      return false
    })

    if (existingCommand) {
      ElMessage({
        message: `该应用已有命令 "${existingCommand.prefix}"，请勿重复添加`,
        type: 'warning',
        duration: 3000,
        offset: 60
      })
      return
    }

    // 检查前缀是否已存在
    const finalPrefix = ':' + commandForm.value.prefix.trim()
    const prefixExists = existingCommands.some(cmd => cmd.prefix === finalPrefix)
    if (prefixExists) {
      ElMessage({
        message: `命令前缀 "${finalPrefix}" 已被使用，请使用其他前缀`,
        type: 'warning',
        duration: 3000,
        offset: 60
      })
      return
    }

    // 构建命令类型 - 运行程序
    const commandType = {
      RunProgram: {
        path: app.path,
        args: null
      }
    }

    await invoke('add_custom_command', {
      prefix: finalPrefix,
      title: app.title,
      description: `启动 ${app.title}`,
      icon: 'Monitor',  // 使用默认图标，实际显示时会使用应用图标
      commandType: commandType
    })

    closeCommandDialog()
    // 通知父组件重新加载自定义命令
    emit('category-changed')
  } catch (error) {
    console.error('添加命令失败:', error)
    ElMessage({
      message: error,
      type: 'error',
      duration: 3000,
      offset: 60
    })
  }
}

onMounted(() => {
  document.addEventListener('click', hideContextMenu)
  initCategoriesSortable()
  loadCategories()
})

// 监听分类变化，重新加载
watch(() => props.categories, () => {
  loadCategories()
}, {deep: true})

onBeforeUnmount(() => {
  document.removeEventListener('click', hideContextMenu)
  if (categoriesSortable) {
    categoriesSortable.destroy()
  }
  destroyAppsSortable()
})
</script>

<style scoped>
.app-grid-container {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
  position: relative;
}

.categories-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 12px;
  align-items: start;
}

.category-box {
  background: var(--fy-bg-card);
  border: 1px solid var(--fy-border-light);
  border-radius: 8px;
  overflow: hidden;
  cursor: pointer;
  user-select: none;
  transition: opacity 0.2s, border-color 0.2s, transform 0.2s;
}

.category-box:hover {
  border-color: var(--fy-accent);
}

/* Sortable chosen 状态 - 长按后准备拖动 */
.category-box.category-chosen {
  transform: scale(1.05);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  z-index: 10;
  border-color: var(--fy-accent);
}

.category-box.category-ghost {
  opacity: 0.4;
  background: var(--fy-accent-bg);
  border: 2px dashed var(--fy-accent);
}

.category-box.category-drag {
  opacity: 0.3;
  transform: scale(0.95);
}

.category-fallback {
  opacity: 0.95;
  background: var(--fy-bg-card);
  border: 2px solid var(--fy-accent);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  overflow: hidden;
  z-index: 9999;
  cursor: grabbing !important;
  pointer-events: none;
  position: fixed !important;
}

.category-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--fy-border-light);
  transition: background 0.15s;
}

.category-header:hover {
  background: var(--fy-bg-hover);
}

.category-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--fy-text-primary);
}

.category-count {
  font-size: 11px;
  color: var(--fy-text-muted);
  background: var(--fy-bg-hover);
  padding: 1px 6px;
  border-radius: 10px;
}

.category-apps {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 4px;
  padding: 8px;
}

.app-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 8px 4px;
  border-radius: 6px;
  transition: all 0.15s;
  user-select: none;
}

.app-item:not(.sortable-app) {
  cursor: pointer;
}

.app-item:hover {
  background: var(--fy-accent-bg);
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.app-item.sortable-app {
  cursor: grab;
  transition: transform 0.2s ease;
}

.app-item.sortable-app:active {
  cursor: grabbing;
}

/* Sortable chosen 状态 - 长按后准备拖动 */
.app-item.app-chosen {
  transform: scale(1.1);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  z-index: 10;
}

.app-item.app-ghost {
  opacity: 0.4;
  background: var(--fy-accent-bg);
  border: 2px dashed var(--fy-accent);
  transform: scale(1.05);
}

.app-item.app-drag {
  opacity: 0.3;
}

.app-fallback {
  opacity: 0.95;
  background: var(--fy-bg-surface);
  border: 2px solid var(--fy-accent);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  padding: 8px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  z-index: 9999;
  cursor: grabbing !important;
  pointer-events: none;
  transform-origin: center center;
  position: fixed !important; /* 强制使用fixed定位 */
}

.app-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 8px;
  overflow: hidden;
}

.icon-img {
  width: 32px;
  height: 32px;
  object-fit: contain;
}

.app-name {
  font-size: 11px;
  color: var(--fy-text-primary);
  text-align: center;
  width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  line-height: 1.2;
}

.expand-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.expand-popup {
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 12px;
  width: 340px;
  max-height: 360px;
  box-shadow: var(--fy-shadow-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.expand-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--fy-border-light);
  flex-shrink: 0;
}

.expand-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--fy-text-primary);
}

.expand-close {
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

.expand-close:hover {
  background: var(--fy-danger-bg);
  color: var(--fy-danger);
}

.expand-apps {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 8px;
  padding: 12px;
  overflow-y: auto;
  overflow-x: hidden;
}

.expand-apps .app-item {
  min-width: 0;
}

.expand-apps .app-name {
  font-size: 11px;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.context-menu {
  position: fixed;
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 8px;
  padding: 4px 0;
  min-width: 120px;
  box-shadow: var(--fy-shadow);
  z-index: 10000;
}

.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px; /* 缩短上下间距从6px到4px */
  font-size: 12px;
  color: var(--fy-text-primary);
  cursor: pointer;
  transition: all 0.15s;
}

.menu-item:hover {
  background: var(--fy-accent-bg);
  padding-left: 16px;
}

.menu-divider {
  height: 1px;
  background: var(--fy-border-light);
  margin: 4px 0;
}

.menu-category-list {
  max-height: calc(32px * 5); /* 每条菜单项约32px，最多显示5条 */
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: thin;
  scrollbar-color: var(--fy-border) transparent;
  touch-action: pan-y;
  height: auto;
}

.menu-category-list::-webkit-scrollbar {
  width: 4px;
}

.menu-category-list::-webkit-scrollbar-track {
  background: transparent;
}

.menu-category-list::-webkit-scrollbar-thumb {
  background: var(--fy-border);
  border-radius: 2px;
}

.menu-category-list::-webkit-scrollbar-thumb:hover {
  background: var(--fy-text-muted);
}

/* Dialog styles */
.dialog-overlay {
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

.command-dialog {
  width: 400px;
  background: var(--fy-bg-surface);
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
}

.dialog-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--fy-text-primary);
  margin-bottom: 16px;
}

.app-info-preview {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  background: var(--fy-bg-card);
  border-radius: 8px;
  margin-bottom: 16px;
}

.preview-icon {
  width: 32px;
  height: 32px;
  object-fit: contain;
}

.preview-name {
  font-size: 14px;
  color: var(--fy-text-primary);
  font-weight: 500;
}

.form-group {
  margin-bottom: 16px;
}

.form-group label {
  display: block;
  font-size: 12px;
  color: var(--fy-text-muted);
  margin-bottom: 6px;
}

.prefix-input-wrapper {
  position: relative;
  display: flex;
  align-items: center;
  border: 1px solid var(--fy-border);
  border-radius: 6px;
  background: var(--fy-bg-card);
  transition: border-color 0.2s;
  padding-left: 12px;
}

.prefix-input-wrapper:focus-within {
  border-color: var(--fy-accent);
}

.prefix-symbol {
  font-size: 14px;
  color: var(--fy-text-muted);
  user-select: none;
  pointer-events: none;
  line-height: 1;
  margin-right: 4px;
  transform: translateY(-1px);
}

.prefix-input {
  flex: 1;
  padding: 8px 12px 8px 0;
  border: none;
  outline: none;
  background: transparent;
  color: var(--fy-text-primary);
  font-size: 14px;
}

.prefix-input::placeholder {
  padding-left: 0;
}

.form-input, .form-select {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--fy-border);
  border-radius: 6px;
  background: var(--fy-bg-card);
  color: var(--fy-text-primary);
  font-size: 14px;
  outline: none;
  transition: border-color 0.2s;
  box-sizing: border-box;
}

.form-input:focus, .form-select:focus {
  border-color: var(--fy-accent);
}

.form-hint {
  display: block;
  font-size: 11px;
  color: var(--fy-text-muted);
  margin-top: 4px;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 20px;
}

.dialog-btn {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
  transition: all 0.2s;
}

.dialog-btn.cancel {
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
}

.dialog-btn.cancel:hover {
  background: var(--fy-border);
}

.dialog-btn.confirm {
  background: var(--fy-accent);
  color: white;
}

.dialog-btn.confirm:hover {
  background: var(--fy-accent-hover);
}
</style>
