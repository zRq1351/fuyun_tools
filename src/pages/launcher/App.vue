<template>
  <div class="launcher-container">
    <!-- Element Plus 消息容器 -->
    <el-config-provider>
      <div ref="launcherBoxRef" class="launcher-box">
      <div class="search-wrapper" @mousedown="startDrag">
        <SearchBox
            ref="searchBoxRef"
            v-model="searchQuery"
            @blur="isFocused = false"
            @clear="handleClear"
            @focus="isFocused = true"
            @input="handleSearch"
            @keydown="handleKeydown"
        />
        <div class="header-actions">
          <button v-if="hasCategorizedApps && !searchQuery" :title="viewMode === 'category' ? '列表视图' : '分类视图'"
                  class="mode-button"
                  @click="toggleViewMode"
                  @mousedown.stop>
            <el-icon :size="14">
              <Grid v-if="viewMode === 'list'"/>
              <List v-else/>
            </el-icon>
          </button>
          <button :class="{ spinning: isRefreshing }" class="mode-button" title="刷新应用列表" @click="handleRefresh"
                  @mousedown.stop>
            <el-icon :size="14">
              <Refresh/>
            </el-icon>
          </button>
          <button class="mode-button" title="管理分类" @click="showCategoryManager = true" @mousedown.stop>
            <el-icon :size="14">
              <Setting/>
            </el-icon>
          </button>
          <button class="mode-button" title="管理命令" @click="showCommandManager = true" @mousedown.stop>
            <el-icon :size="14">
              <Tools/>
            </el-icon>
          </button>
          <button class="mode-button" title="手动添加应用" @click="showAddManualDialog = true" @mousedown.stop>
            <el-icon :size="14">
              <Plus/>
            </el-icon>
          </button>
          <button class="close-button" @click="hideLauncher" @mousedown.stop>
            <el-icon :size="16">
              <Close/>
            </el-icon>
          </button>
        </div>
      </div>

      <div v-if="isLoading" class="loading-state">
        <el-icon :size="24" class="loading-icon">
          <Loading/>
        </el-icon>
        <span>正在扫描应用，请稍候...</span>
      </div>

      <div v-else class="content-area">
        <AppGrid
            v-if="viewMode === 'category' && hasCategorizedApps"
            :categories="categorizedApps"
            :total-apps="totalCategorizedApps"
            @select="handleSelect"
            @reorder-apps="handleReorderApps"
            @reorder-categories="handleReorderCategories"
            @category-changed="handleCategoryChanged"
        />
        <AppList
            v-else
            :apps="displayApps"
            :categories="launcherConfig?.categories || []"
            :app-category-map="launcherConfig?.app_category_map || {}"
            :custom-commands="launcherConfig?.custom_commands || []"
            @reorder="handleReorder"
            @select="handleSelect"
            @category-changed="handleCategoryChanged"
        />
        <div v-if="commandResults.length > 0" class="command-section">
          <div class="command-header">命令</div>
          <div
              v-for="(item, index) in commandResults"
              :key="item.id"
              :class="{ 'is-active': index === activeIndex }"
              class="command-item"
              @click="handleSelect(item)"
              @mouseenter="activeIndex = index"
          >
            <div class="command-prefix">{{ item.shortcut }}</div>
            <div class="command-title">{{ item.title }}</div>
          </div>
        </div>
        <CategoryManager
            :app-category-map="launcherConfig?.app_category_map || {}"
            :categories="launcherConfig?.categories || []"
            :visible="showCategoryManager"
            @close="showCategoryManager = false"
            @updated="handleCategoryUpdated"
        />
        <CommandManager
            :visible="showCommandManager"
            @close="showCommandManager = false"
            @updated="handleCommandUpdated"
        />

        <!-- 手动添加应用对话框 -->
        <div v-if="showAddManualDialog" class="dialog-overlay" @click.self="cancelAddManual">
          <div class="manual-dialog">
            <div class="dialog-title">手动添加应用</div>
            <div class="form-group">
              <label>应用名称</label>
              <input v-model="manualForm.name" class="form-input" placeholder="输入应用名称"/>
            </div>
            <div class="form-group">
              <label>应用程序</label>
              <div class="file-input-row">
                <input v-model="manualForm.path" class="form-input" placeholder="选择 .exe 或 .lnk 文件" readonly/>
                <button class="dialog-btn browse" @click="browseManualFile">浏览</button>
              </div>
            </div>
            <div class="dialog-actions">
              <button class="dialog-btn cancel" @click="cancelAddManual">取消</button>
              <button :disabled="!manualForm.name || !manualForm.path" class="dialog-btn confirm"
                      @click="confirmAddManual">确定
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
    </el-config-provider>
  </div>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, ref} from 'vue'
import {ElConfigProvider, ElMessage, ElMessageBox} from 'element-plus'
import {Close, Grid, List, Loading, Plus, Refresh, Setting, Tools} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {invoke} from '@tauri-apps/api/core'
import SearchBox from './components/SearchBox.vue'
import AppGrid from './components/AppGrid.vue'
import AppList from './components/AppList.vue'
import CategoryManager from './components/CategoryManager.vue'
import CommandManager from './components/CommandManager.vue'
import {useLauncherSearch} from './composables/useLauncherSearch'

const searchQuery = ref('')
const commandResults = ref([])
const allApps = ref([])
const activeIndex = ref(0)
const isFocused = ref(false)
const isSearching = ref(false)
const isLoading = ref(false)
const isRefreshing = ref(false)
const isDragging = ref(false)
const isResizing = ref(false)
let resizeTimer = null
const launcherBoxRef = ref(null)
const searchBoxRef = ref(null)
const viewMode = ref('list')
const showCategoryManager = ref(false)
const showCommandManager = ref(false)
const showAddManualDialog = ref(false)
const manualForm = ref({name: '', path: ''})
const launcherConfig = ref(null)

const {search, executeAction, loadCustomCommands} = useLauncherSearch()

let unlistenShow = null
let unlistenBlur = null
let unlistenResize = null

const loadAllApps = async () => {
  try {
    const apps = await invoke('get_all_apps')
    allApps.value = apps.sort((a, b) => (a.sort_order || 0) - (b.sort_order || 0))
    launcherConfig.value = await invoke('get_launcher_config')
    viewMode.value = launcherConfig.value.view_mode || 'list'
    loadIcons()
    // 加载自定义命令
    await loadCustomCommands()
  } catch (error) {
    if (error === 'NEED_SCAN') {
      await handleFirstScan()
    } else {
      console.error('Load apps error:', error)
    }
  }
}

const handleFirstScan = async () => {
  isLoading.value = true
  try {
    const apps = await invoke('scan_and_save_apps')
    allApps.value = apps
    launcherConfig.value = await invoke('get_launcher_config')
    viewMode.value = launcherConfig.value.view_mode || 'list'
    await loadIcons()
    // 加载自定义命令
    await loadCustomCommands()
  } catch (error) {
    console.error('Scan error:', error)
    ElMessage.error('扫描应用失败')
  } finally {
    isLoading.value = false
  }
}

const handleRefresh = async () => {
  if (isRefreshing.value) return
  isRefreshing.value = true
  try {
    const apps = await invoke('scan_and_save_apps')
    allApps.value = apps
    launcherConfig.value = await invoke('get_launcher_config')
    await loadIcons()
    ElMessage.success('应用列表已刷新')
    commandResults.value = []
    activeIndex.value = 0
  } catch (error) {
    console.error('Refresh error:', error)
    ElMessage.error('刷新失败')
  } finally {
    isRefreshing.value = false
  }
}

const loadIcons = async () => {
  const paths = allApps.value.filter(a => a.path && !a.icon_base64).map(a => a.path)
  if (paths.length === 0) return

  try {
    const icons = await invoke('batch_extract_icons', {paths})
    for (const app of allApps.value) {
      if (app.path && icons[app.path]) {
        app.icon_base64 = icons[app.path]
      }
    }
  } catch (error) {
    console.error('Load icons error:', error)
  }
}

const hasCategorizedApps = computed(() => {
  if (!launcherConfig.value || !launcherConfig.value.categories) return false
  return launcherConfig.value.categories.length > 0
})

const categorizedApps = computed(() => {
  if (!launcherConfig.value) return []
  const config = launcherConfig.value
  const result = []
  const apps = displayApps.value
  for (const category of config.categories) {
    const catApps = apps
        .filter(app => config.app_category_map[app.id] === category.id)
        .sort((a, b) => (a.sort_order || 0) - (b.sort_order || 0))
    result.push({name: category.name, apps: catApps})
  }
  return result
})

const totalCategorizedApps = computed(() => {
  return categorizedApps.value.reduce((sum, cat) => sum + cat.apps.length, 0)
})

const toggleViewMode = async () => {
  if (!hasCategorizedApps.value) return
  const newMode = viewMode.value === 'category' ? 'list' : 'category'
  try {
    await invoke('set_launcher_view_mode', {mode: newMode})
    viewMode.value = newMode
  } catch (error) {
    console.error('Toggle view mode error:', error)
  }
}

const displayApps = computed(() => {
  const query = searchQuery.value.trim().toLowerCase()
  if (!query || query.startsWith(':')) return allApps.value
  return allApps.value.filter(a => a.title.toLowerCase().includes(query))
})

const handleSearch = async () => {
  const query = searchQuery.value.trim()
  if (!query) {
    commandResults.value = []
    activeIndex.value = 0
    return
  }
  isSearching.value = true
  try {
    commandResults.value = (await search(query, allApps.value))
        .filter(item => item.action !== 'launch_app')
    activeIndex.value = 0
  } catch (error) {
    console.error('Command search error:', error)
    commandResults.value = []
  } finally {
    isSearching.value = false
  }
}

const handleClear = () => {
  searchQuery.value = ''
  commandResults.value = []
  activeIndex.value = 0
}

const handleKeydown = (event) => {
  const items = commandResults.value.length > 0 ? commandResults.value : displayApps.value
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault()
      if (items.length > 0) {
        activeIndex.value = (activeIndex.value + 1) % items.length
      }
      break
    case 'ArrowUp':
      event.preventDefault()
      if (items.length > 0) {
        activeIndex.value = (activeIndex.value - 1 + items.length) % items.length
      }
      break
    case 'Enter':
      event.preventDefault()
      if (items.length > 0 && activeIndex.value < items.length) {
        handleSelect(items[activeIndex.value])
      }
      break
    case 'Escape':
      event.preventDefault()
      hideLauncher()
      break
  }
}

const handleSelect = async (item) => {
  try {
    await executeAction(item)
    hideLauncher()
  } catch (error) {
    if (error === 'APP_NOT_FOUND') {
      try {
        await ElMessageBox.confirm(
            '该应用已被卸载或移动位置，是否从列表中移除？',
            '应用不存在',
            {confirmButtonText: '移除', cancelButtonText: '取消', type: 'warning'}
        )
        await invoke('remove_app_record', {appId: item.id})
        allApps.value = allApps.value.filter(a => a.id !== item.id)
        ElMessage.success('已移除')
      } catch {
      }
    } else {
      console.error('Execute error:', error)
    }
  }
}

const hideLauncher = async () => {
  try {
    const window = getCurrentWebviewWindow()
    await window.hide()
  } catch (error) {
    console.error('Hide window error:', error)
  }
}

const startDrag = async (event) => {
  if (event.target.tagName === 'INPUT' ||
      event.target.closest('.close-button') ||
      event.target.closest('.mode-button') ||
      event.target.closest('.clear-button')) {
    return
  }
  try {
    isDragging.value = true
    const window = getCurrentWebviewWindow()
    await window.startDragging()
    // 拖动结束后延迟重置标志
    setTimeout(() => {
      isDragging.value = false
    }, 200)
  } catch (error) {
    console.error('Start drag error:', error)
    isDragging.value = false
  }
}

const handleCategoryUpdated = async () => {
  launcherConfig.value = await invoke('get_launcher_config')
  // 重新加载自定义命令
  await loadCustomCommands()
}

const handleCategoryChanged = async () => {
  launcherConfig.value = await invoke('get_launcher_config')
  allApps.value = await invoke('get_all_apps')
  await loadCustomCommands()
}

const handleCommandUpdated = async () => {
  // 当命令发生变化时，重新加载配置和自定义命令
  launcherConfig.value = await invoke('get_launcher_config')
  await loadCustomCommands()
}

const handleReorderApps = async (reorderedApps) => {
  const orders = reorderedApps.map((app, index) => [app.id, index])
  try {
    await invoke('update_app_sort_orders', {orders})
    for (const [appId, sortOrder] of orders) {
      const app = allApps.value.find(a => a.id === appId)
      if (app) {
        app.sort_order = sortOrder
      }
    }
  } catch (error) {
    console.error('Reorder apps error:', error)
  }
}

const browseManualFile = async () => {
  try {
    const {open} = await import('@tauri-apps/plugin-dialog')
    const selected = await open({
      filters: [{name: '可执行文件', extensions: ['exe', 'lnk']}],
      multiple: false
    })
    if (selected && typeof selected === 'string') {
      manualForm.value.path = selected
      if (!manualForm.value.name) {
        const fileName = selected.split('\\').pop().split('/').pop().replace(/\.(exe|lnk)$/i, '')
        manualForm.value.name = fileName
      }
    }
  } catch (error) {
    console.error('选择文件失败:', error)
  }
}

const confirmAddManual = async () => {
  if (!manualForm.value.name.trim() || !manualForm.value.path) return
  try {
    await invoke('add_manual_app', {title: manualForm.value.name.trim(), path: manualForm.value.path})
    allApps.value = await invoke('get_all_apps')
    manualForm.value = {name: '', path: ''}
    showAddManualDialog.value = false
    ElMessage.success('应用已添加')
  } catch (error) {
    console.error('添加应用失败:', error)
    ElMessage.error('添加失败')
  }
}

const cancelAddManual = () => {
  manualForm.value = {name: '', path: ''}
  showAddManualDialog.value = false
}

const handleReorderCategories = async (fromIndex, toIndex) => {
  console.log('handleReorderCategories called:', {fromIndex, toIndex})
  if (!launcherConfig.value) {
    console.error('launcherConfig is null')
    return
  }
  const categories = [...launcherConfig.value.categories]
  console.log('Categories before reorder:', categories.map(c => ({id: c.id, name: c.name})))
  if (fromIndex < 0 || fromIndex >= categories.length || toIndex < 0 || toIndex > categories.length) {
    console.error('Invalid indices')
    return
  }
  const [moved] = categories.splice(fromIndex, 1)
  categories.splice(toIndex, 0, moved)
  console.log('Categories after reorder:', categories.map(c => ({id: c.id, name: c.name})))
  launcherConfig.value.categories = categories
  try {
    // 提取分类ID列表并按新顺序排列
    const categoryIds = categories.map(cat => cat.id)
    console.log('Calling reorder_categories with IDs:', categoryIds)
    const result = await invoke('reorder_categories', {categoryIds})
    console.log('reorder_categories result:', result)
  } catch (error) {
    console.error('Reorder categories error:', error)
  }
}

const handleReorder = async (orders) => {
  try {
    await invoke('update_app_sort_orders', {orders})
    for (const [appId, sortOrder] of orders) {
      const app = allApps.value.find(a => a.id === appId)
      if (app) {
        app.sort_order = sortOrder
      }
    }
    allApps.value = [...allApps.value].sort((a, b) => (a.sort_order || 0) - (b.sort_order || 0))
  } catch (error) {
    console.error('Reorder error:', error)
  }
}

onMounted(async () => {
  unlistenShow = await listen('show-launcher', async () => {
    searchQuery.value = ''
    commandResults.value = []
    activeIndex.value = 0
    await loadAllApps()
    // 窗口显示后，让搜索框自动获取焦点
    if (searchBoxRef.value) {
      searchBoxRef.value.focus()
    }
  })

  // 窗口失去焦点时延迟关闭
  const window = getCurrentWebviewWindow()
  unlistenBlur = await window.listen('tauri://blur', async () => {
    if (isDragging.value || isResizing.value) return
    await new Promise(resolve => setTimeout(resolve, 200))
    if (isDragging.value || isResizing.value) return
    if (!showCategoryManager.value && !showCommandManager.value && !showAddManualDialog.value) {
      await hideLauncher()
    }
  })

  // 监听窗口大小变化（拖动边框缩放时抑制失焦关闭）
  unlistenResize = await listen('launcher-resizing', () => {
    isResizing.value = true
    clearTimeout(resizeTimer)
    resizeTimer = setTimeout(() => {
      isResizing.value = false
    }, 1000)
  })
})

onBeforeUnmount(() => {
  if (unlistenShow) unlistenShow()
  if (unlistenBlur) unlistenBlur()
  if (unlistenResize) unlistenResize()
  clearTimeout(resizeTimer)
})
</script>

<style scoped>
.launcher-container {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  display: flex;
  justify-content: center;
  align-items: center;
  background: transparent;
  z-index: 1000;
  pointer-events: none;
}

.launcher-box {
  width: 100%;
  height: 100%;
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 12px;
  box-shadow: var(--fy-shadow-lg);
  backdrop-filter: var(--fy-backdrop-blur);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  pointer-events: auto;
  /* 使用 clip-path 确保圆角外完全透明 */
  clip-path: inset(0 round 12px);
}

.search-wrapper {
  display: flex;
  align-items: center;
  cursor: move;
  border-bottom: 1px solid var(--fy-border-light);
  flex-shrink: 0;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  margin-right: 8px;
}

.mode-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.2s;
}

.mode-button:hover {
  background: var(--fy-bg-hover);
  color: var(--fy-accent);
}

.mode-button.spinning .el-icon {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.close-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.2s;
}

.close-button:hover {
  background: var(--fy-danger-bg);
  color: var(--fy-danger);
}

.loading-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--fy-text-muted);
  font-size: 14px;
}

.loading-icon {
  animation: spin 1s linear infinite;
}

.content-area {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  position: relative;
}

.command-section {
  border-top: 1px solid var(--fy-border-light);
  padding: 4px 0;
}

.command-header {
  padding: 4px 16px;
  font-size: 11px;
  color: var(--fy-text-muted);
}

.command-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.command-item:hover,
.command-item.is-active {
  background: var(--fy-bg-hover);
}

.command-prefix {
  font-family: monospace;
  font-size: 12px;
  color: var(--fy-accent);
  min-width: 80px;
}

.command-title {
  font-size: 13px;
  color: var(--fy-text-primary);
}

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

.manual-dialog {
  width: 420px;
  background: var(--fy-bg-surface);
  border-radius: 12px;
  padding: 24px;
}

.dialog-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--fy-text-primary);
  margin-bottom: 20px;
}

.form-group {
  margin-bottom: 16px;
}

.form-group label {
  display: block;
  font-size: 13px;
  color: var(--fy-text-secondary);
  margin-bottom: 6px;
}

.form-input {
  width: 100%;
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--fy-border);
  border-radius: 6px;
  background: var(--fy-bg-card);
  color: var(--fy-text-primary);
  font-size: 14px;
  outline: none;
  box-sizing: border-box;
}

.form-input:focus {
  border-color: var(--fy-accent);
}

.file-input-row {
  display: flex;
  gap: 8px;
}

.file-input-row .form-input {
  flex: 1;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 20px;
}

.dialog-btn {
  padding: 6px 16px;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-btn.cancel {
  background: var(--fy-bg-hover);
  color: var(--fy-text-secondary);
}

.dialog-btn.browse {
  background: var(--fy-bg-hover);
  color: var(--fy-accent);
  white-space: nowrap;
}

.dialog-btn.confirm {
  background: var(--fy-accent);
  color: #fff;
}

.dialog-btn.confirm:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.launcher-hints {
  display: flex;
  justify-content: center;
  gap: 16px;
  padding: 10px 16px;
  border-top: 1px solid var(--fy-border-light);
  cursor: move;
  flex-shrink: 0;
}

.hint-item {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--fy-text-muted);
}

.hint-key {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 24px;
  height: 20px;
  padding: 0 6px;
  background: var(--fy-bg-hover);
  border: 1px solid var(--fy-border-light);
  border-radius: 4px;
  font-size: 11px;
  font-family: monospace;
  color: var(--fy-text-secondary);
}
</style>

<style>
/* 全局样式 - Element Plus 消息提示 */
.el-message {
  z-index: 10010 !important;
}

.el-message-box {
  z-index: 10010 !important;
}

.el-overlay {
  z-index: 10009 !important;
}

/* 确保页面背景透明，避免圆角外显示灰色 */
html, body {
  background: transparent !important;
  margin: 0;
  padding: 0;
  overflow: hidden;
}

#app {
  background: transparent !important;
}
</style>
