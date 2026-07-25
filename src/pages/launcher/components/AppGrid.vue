<template>
  <div class="app-grid-container" @contextmenu.prevent>
    <div v-if="totalApps === 0" class="empty-category-hint">
      <el-icon :size="40" class="hint-icon">
        <FolderAdd/>
      </el-icon>
      <p>还没有为应用设置分类</p>
      <p class="hint-sub">切换回列表视图，右键应用选择「添加到分类」即可归类</p>
    </div>
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
          <div class="category-actions">
            <button
                class="launch-all-btn"
                title="启动所有应用"
                @click.stop="launchAllApps(category)"
            >
              <el-icon :size="14">
                <Monitor/>
              </el-icon>
            </button>
            <span class="category-count">{{ category.apps.length }}</span>
          </div>
        </div>
        <div class="category-apps">
          <div
              v-for="app in category.apps.slice(0, 4)"
              :key="app.id"
              :class="{ 'ctx-anchor': ctxAnchorId === app.id }"
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
            <span :class="app.source === 'manual' ? 'manual' : 'scan'" class="app-source-badge">{{
                app.source === 'manual' ? '手动' : '扫描'
              }}</span>
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
              :class="{ 'ctx-anchor': ctxAnchorId === app.id }"
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
            <span :class="app.source === 'manual' ? 'manual' : 'scan'" class="app-source-badge">{{
                app.source === 'manual' ? '手动' : '扫描'
              }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Context Menu -->
    <ContextMenu :show="ctxVisible" :x="ctxX" :y="ctxY" @close="closeCtxMenu">
      <div class="context-menu-item" @click="openApp(ctxApp)">
        <el-icon :size="14">
          <Monitor/>
        </el-icon>
        <span>{{ t('common.open') }}</span>
      </div>
      <div class="context-menu-item" @click="openAppDirectory(ctxApp)">
        <el-icon :size="14">
          <FolderOpened/>
        </el-icon>
        <span>打开应用目录</span>
      </div>
      <div class="context-menu-divider"></div>
      <div v-if="ctxApp?.source === 'manual'" class="context-menu-item" @click="removeApp(ctxApp)">
        <el-icon :size="14">
          <Delete/>
        </el-icon>
        <span>{{ t('common.remove') }}应用</span>
      </div>
      <div v-else class="context-menu-item" @click="removeFromCategory(ctxApp)">
        <el-icon :size="14">
          <Close/>
        </el-icon>
        <span>移出分类</span>
      </div>
      <div class="context-menu-divider"></div>
      <div class="context-menu-item" @click="showAddCommandDialogFn">
        <el-icon :size="14">
          <Star/>
        </el-icon>
        <span>添加启动命令</span>
      </div>
    </ContextMenu>

    <!-- 添加命令对话框 -->
    <div v-if="showCommandDialog" class="dialog-overlay">
      <div class="command-dialog">
        <div class="dialog-title">为应用添加启动命令</div>
        <div class="app-info-preview">
          <img v-if="ctxApp?.icon_base64" :src="ctxApp.icon_base64" class="preview-icon"/>
          <span class="preview-name">{{ ctxApp?.title }}</span>
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
          <button class="dialog-btn cancel" @click="closeCommandDialog">{{ t('common.cancel') }}</button>
          <button class="dialog-btn confirm" @click="confirmAddCommand">{{ t('common.ok') }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {nextTick, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage, ElMessageBox} from 'element-plus'
import {Close, Delete, FolderAdd, FolderOpened, Monitor, Star} from '@element-plus/icons-vue'
import Sortable from 'sortablejs'
import {invoke} from '@tauri-apps/api/core'
import ContextMenu from '../../../components/ContextMenu.vue'
import {useAppActions} from '../composables/useAppActions'

const {t} = useI18n()

const props = defineProps({
  categories: {type: Array, required: true},
  totalApps: {type: Number, default: 0}
})

const emit = defineEmits(['select', 'reorder-apps', 'reorder-categories', 'category-changed'])

// Use shared composable for app actions
const {
  showCommandDialog,
  commandForm,
  ctxApp: sharedCtxApp,
  openAppDirectory: sharedOpenAppDirectory,
  removeApp: sharedRemoveApp,
  removeFromCategory: sharedRemoveFromCategory,
  showAddCommandDialog: sharedShowAddCommandDialog,
  closeCommandDialog: sharedCloseCommandDialog,
  confirmAddCommand: sharedConfirmAddCommand
} = useAppActions(emit)

const customCategories = ref([])
const ctxVisible = ref(false)
const ctxX = ref(0)
const ctxY = ref(0)
const ctxApp = ref(null)
const ctxAnchorId = ref(null)
const expandedCategory = ref(null)
const appsContainer = ref(null)
const categoriesContainer = ref(null)

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
  if (category.apps.length >= 2) {
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
      if (__DEV_PANEL__) console.log('Categories sortable onEnd:', {oldIndex, newIndex})
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
  closeCtxMenu()
}

const openAppDirectory = async (app) => {
  await sharedOpenAppDirectory(app)
  closeCtxMenu()
}

const showContextMenu = (event, app) => {
  ctxApp.value = app
  ctxX.value = event.clientX
  ctxY.value = event.clientY
  ctxVisible.value = true
  ctxAnchorId.value = app.id
}

const closeCtxMenu = () => {
  ctxVisible.value = false
  ctxAnchorId.value = null
}

// 启动分类下的所有应用
const launchAllApps = async (category) => {
  if (!category || !category.apps || category.apps.length === 0) return

  try {
    // 二次确认对话框
    await ElMessageBox.confirm(
        `确定要启动「${category.name}」分类下的 ${category.apps.length} 个应用吗？`,
        '批量启动应用',
        {
          confirmButtonText: '确定启动',
          cancelButtonText: t('common.cancel'),
          type: 'warning',
          distinguishCancelAndClose: true
        }
    )

    // 依次启动所有应用
    for (const app of category.apps) {
      emit('select', app)
      // 添加短暂延迟，避免同时启动过多应用
      await new Promise(resolve => setTimeout(resolve, 100))
    }
    ElMessage.success(`已启动 ${category.apps.length} 个应用`)
  } catch (error) {
    // 用户取消或关闭对话框
    if (error === 'cancel' || error === 'close') {
      return
    }
    console.error('Launch all apps error:', error)
    ElMessage.error('启动应用失败')
  }
}

const removeFromCategory = async (app) => {
  await sharedRemoveFromCategory(app)
  closeCtxMenu()
}

const removeApp = async (app) => {
  await sharedRemoveApp(app)
  closeCtxMenu()
}

const assignToCategory = async (app, categoryId) => {
  if (!app || !app.id) return
  try {
    await invoke('set_app_category', {appId: app.id, categoryId})
    closeCtxMenu()
    emit('category-changed')
  } catch (error) {
    console.error('Assign category error:', error)
  }
}

const showAddCommandDialogFn = () => {
  sharedShowAddCommandDialog(ctxApp.value)
}

const closeCommandDialog = () => {
  sharedCloseCommandDialog()
  closeCtxMenu()
}

const confirmAddCommand = async () => {
  // Sync ctxApp to shared composable
  sharedCtxApp.value = ctxApp.value
  await sharedConfirmAddCommand()
}

onMounted(() => {
  initCategoriesSortable()
  loadCategories()
})

// 监听分类变化，重新加载
watch(() => props.categories, () => {
  loadCategories()
}, {deep: true})

onBeforeUnmount(() => {
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

.empty-category-hint {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px 24px;
  text-align: center;
  color: var(--fy-text-muted);
  font-size: 14px;
  gap: 8px;
}

.empty-category-hint p {
  margin: 0;
}

.hint-icon {
  opacity: 0.4;
  margin-bottom: 8px;
}

.hint-sub {
  font-size: 12px;
  color: var(--fy-text-secondary);
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
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.category-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.launch-all-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  border-radius: 4px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.2s;
  padding: 0;
}

.launch-all-btn:hover {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

.launch-all-btn:active {
  transform: scale(0.95);
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

.app-item.ctx-anchor {
  background: var(--fy-accent-bg);
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

.app-source-badge {
  font-size: 9px;
  padding: 1px 5px;
  border-radius: 6px;
  flex-shrink: 0;
  margin-top: 2px;
}

.app-source-badge.scan {
  background: var(--fy-bg-hover);
  color: var(--fy-text-muted);
}

.app-source-badge.manual {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
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

/* 响应式布局 */
@media (max-width: 900px) {
  .categories-grid {
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  }

  .expand-popup {
    width: min(340px, calc(100vw - 32px));
  }

  .expand-apps {
    grid-template-columns: repeat(3, 1fr);
  }
}

@media (max-width: 480px) {
  .categories-grid {
    grid-template-columns: 1fr;
  }

  .expand-popup {
    width: calc(100vw - 24px);
    max-height: calc(100vh - 48px);
  }

  .expand-apps {
    grid-template-columns: repeat(2, 1fr);
  }

  .category-apps {
    grid-template-columns: repeat(2, 1fr);
  }
}
</style>
