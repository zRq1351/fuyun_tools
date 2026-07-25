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
          <button v-if="hasCategorizedApps && !searchQuery"
                  :title="viewMode === 'category' ? t('launcher.listView') : t('launcher.categoryView')"
                  class="mode-button"
                  @click="toggleViewMode"
                  @mousedown.stop>
            <el-icon :size="14">
              <Grid v-if="viewMode === 'list'"/>
              <List v-else/>
            </el-icon>
          </button>
          <button :class="{ spinning: isRefreshing }" :title="t('launcher.refreshApps')" class="mode-button"
                  @click="handleRefresh"
                  @mousedown.stop>
            <el-icon :size="14">
              <Refresh/>
            </el-icon>
          </button>
          <button :title="t('launcher.manageCategories')" class="mode-button" @click="showCategoryManager = true"
                  @mousedown.stop>
            <el-icon :size="14">
              <Setting/>
            </el-icon>
          </button>
          <button :title="t('launcher.manageCommands')" class="mode-button" @click="showCommandManager = true"
                  @mousedown.stop>
            <el-icon :size="14">
              <Tools/>
            </el-icon>
          </button>
          <button :title="t('launcher.addApp')" class="mode-button" @click="showAddManualDialog = true" @mousedown.stop>
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
        <span>{{ t('launcher.scanning') }}</span>
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
          <div class="command-header">{{ t('launcher.commands') }}</div>
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
            <div class="dialog-title">{{ t('launcher.addAppTitle') }}</div>
            <div class="form-group">
              <label>{{ t('launcher.appName') }}</label>
              <input v-model="manualForm.name" :placeholder="t('launcher.appNamePlaceholder')" class="form-input"/>
            </div>
            <div class="form-group">
              <label>{{ t('launcher.appPath') }}</label>
              <div class="file-input-row">
                <input v-model="manualForm.path" :placeholder="t('launcher.appPathPlaceholder')" class="form-input"
                       readonly/>
                <button class="dialog-btn browse" @click="browseManualFile">{{ t('common.browse') }}</button>
              </div>
            </div>
            <div class="dialog-actions">
              <button class="dialog-btn cancel" @click="cancelAddManual">{{ t('common.cancel') }}</button>
              <button :disabled="!manualForm.name || !manualForm.path" class="dialog-btn confirm"
                      @click="confirmAddManual">{{ t('common.ok') }}
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
import {useI18n} from 'vue-i18n'
import SearchBox from './components/SearchBox.vue'
import AppGrid from './components/AppGrid.vue'
import AppList from './components/AppList.vue'
import CategoryManager from './components/CategoryManager.vue'
import CommandManager from './components/CommandManager.vue'
import {useLauncherSearch} from './composables/useLauncherSearch'

const {t} = useI18n()

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
let searchDebounceTimer = null
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
    ElMessage.error(t('launcher.scanFailed'))
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
    ElMessage.success(t('launcher.refreshed'))
    commandResults.value = []
    activeIndex.value = 0
  } catch (error) {
    console.error('Refresh error:', error)
    ElMessage.error(t('launcher.refreshFailed'))
  } finally {
    isRefreshing.value = false
  }
}

const loadIcons = async () => {
  const paths = allApps.value.filter(a => a.path && !a.icon_base64).map(a => a.path)
  if (paths.length === 0) return

  try {
    const icons = await invoke('batch_extract_icons', {paths})
    allApps.value = allApps.value.map(app => {
      if (app.path && icons[app.path]) {
        return {...app, icon_base64: icons[app.path]}
      }
      return app
    })
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

const handleSearch = () => {
  const query = searchQuery.value.trim()
  if (!query) {
    commandResults.value = []
    activeIndex.value = 0
    return
  }
  
  // Clear previous debounce timer
  if (searchDebounceTimer) {
    clearTimeout(searchDebounceTimer)
  }
  
  // Debounce search by 100ms to reduce IPC calls
  searchDebounceTimer = setTimeout(async () => {
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
  }, 100)
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
      if (items.length > 0) {
        const idx = Math.min(activeIndex.value, items.length - 1)
        handleSelect(items[idx])
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
            t('launcher.appNotFound'),
            t('launcher.appNotFoundTitle'),
            {confirmButtonText: t('common.remove'), cancelButtonText: t('common.cancel'), type: 'warning'}
        )
        await invoke('remove_app_record', {appId: item.id})
        allApps.value = allApps.value.filter(a => a.id !== item.id)
        ElMessage.success(t('launcher.appRemoved'))
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
    allApps.value = [...allApps.value].sort((a, b) => (a.sort_order || 0) - (b.sort_order || 0))
  } catch (error) {
    console.error('Reorder apps error:', error)
  }
}

const browseManualFile = async () => {
  try {
    const {open} = await import('@tauri-apps/plugin-dialog')
    const selected = await open({
      filters: [{name: t('launcher.executableFile'), extensions: ['exe', 'lnk']}],
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
    ElMessage.success(t('launcher.appAdded'))
  } catch (error) {
    console.error('添加应用失败:', error)
    ElMessage.error(t('launcher.addFailed'))
  }
}

const cancelAddManual = () => {
  manualForm.value = {name: '', path: ''}
  showAddManualDialog.value = false
}

const handleReorderCategories = async (fromIndex, toIndex) => {
  if (__DEV_PANEL__) {
    console.log('handleReorderCategories called:', {fromIndex, toIndex})
  }
  if (!launcherConfig.value) {
    console.error('launcherConfig is null')
    return
  }
  const categories = [...launcherConfig.value.categories]
  if (__DEV_PANEL__) {
    console.log('Categories before reorder:', categories.map(c => ({id: c.id, name: c.name})))
  }
  if (fromIndex < 0 || fromIndex >= categories.length || toIndex < 0 || toIndex > categories.length) {
    console.error('Invalid indices')
    return
  }
  const [moved] = categories.splice(fromIndex, 1)
  categories.splice(toIndex, 0, moved)
  if (__DEV_PANEL__) {
    console.log('Categories after reorder:', categories.map(c => ({id: c.id, name: c.name})))
  }
  launcherConfig.value.categories = categories
  try {
    const categoryIds = categories.map(cat => cat.id)
    if (__DEV_PANEL__) {
      console.log('Calling reorder_categories with IDs:', categoryIds)
    }
    const result = await invoke('reorder_categories', {categoryIds})
    if (__DEV_PANEL__) {
      console.log('reorder_categories result:', result)
    }
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
  clearTimeout(searchDebounceTimer)
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
  background: var(--fy-glass-bg);
  border: 1px solid var(--fy-glass-border);
  border-radius: var(--fy-radius-xl);
  box-shadow: var(--fy-glass-shadow);
  backdrop-filter: var(--fy-glass-blur);
  -webkit-backdrop-filter: var(--fy-glass-blur);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  pointer-events: auto;
  clip-path: inset(0 round var(--fy-radius-xl));
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
  gap: var(--fy-space-1);
  margin-right: var(--fy-space-2);
}

.mode-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  border-radius: var(--fy-radius-sm);
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
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
  border-radius: var(--fy-radius-sm);
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
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
  gap: var(--fy-space-3);
  color: var(--fy-text-muted);
  font-size: var(--fy-text-md);
}

.loading-icon {
  animation: spin 1s linear infinite;
}

.content-area {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  position: relative;
  padding-bottom: var(--fy-space-2);
}

.command-section {
  border-top: 1px solid var(--fy-border-light);
  padding: var(--fy-space-1) 0;
}

.command-header {
  padding: var(--fy-space-1) var(--fy-space-4);
  font-size: var(--fy-text-xs);
  color: var(--fy-text-muted);
}

.command-item {
  display: flex;
  align-items: center;
  gap: var(--fy-space-3);
  padding: var(--fy-space-2) var(--fy-space-4);
  cursor: pointer;
  transition: background-color var(--fy-duration-fast) var(--fy-ease-out);
}

.command-item:hover,
.command-item.is-active {
  background: var(--fy-bg-hover);
}

.command-prefix {
  font-family: var(--fy-font-mono);
  font-size: var(--fy-text-sm);
  color: var(--fy-accent);
  min-width: 80px;
}

.command-title {
  font-size: var(--fy-text-base);
  color: var(--fy-text-primary);
}

.dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: var(--fy-bg-overlay);
  opacity: 0.85;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10001;
}

.manual-dialog {
  width: 420px;
  background: var(--fy-glass-bg);
  border: 1px solid var(--fy-glass-border);
  border-radius: var(--fy-radius-xl);
  padding: var(--fy-space-6);
  box-shadow: var(--fy-glass-shadow);
  backdrop-filter: var(--fy-glass-blur-light);
  -webkit-backdrop-filter: var(--fy-glass-blur-light);
}

.dialog-title {
  font-size: var(--fy-text-lg);
  font-weight: var(--fy-weight-semibold);
  color: var(--fy-text-primary);
  margin-bottom: var(--fy-space-5);
}

.form-group {
  margin-bottom: var(--fy-space-4);
}

.form-group label {
  display: block;
  font-size: var(--fy-text-base);
  color: var(--fy-text-secondary);
  margin-bottom: var(--fy-space-1);
}

.form-input {
  width: 100%;
  height: 36px;
  padding: 0 var(--fy-space-3);
  border: 1px solid var(--fy-border);
  border-radius: var(--fy-radius-sm);
  background: var(--fy-bg-card);
  color: var(--fy-text-primary);
  font-size: var(--fy-text-md);
  outline: none;
  box-sizing: border-box;
}

.form-input:focus {
  border-color: var(--fy-accent);
}

.file-input-row {
  display: flex;
  gap: var(--fy-space-2);
}

.file-input-row .form-input {
  flex: 1;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--fy-space-2);
  margin-top: var(--fy-space-5);
}

.dialog-btn {
  padding: var(--fy-space-1) var(--fy-space-4);
  border: none;
  border-radius: var(--fy-radius-sm);
  font-size: var(--fy-text-base);
  cursor: pointer;
  transition: all var(--fy-duration-normal) var(--fy-ease-out);
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
  color: var(--fy-text-primary);
}

.dialog-btn.confirm:hover {
  background: var(--fy-accent-hover);
}

.dialog-btn.confirm:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.launcher-hints {
  display: flex;
  justify-content: center;
  gap: var(--fy-space-4);
  padding: var(--fy-space-3) var(--fy-space-4);
  border-top: 1px solid var(--fy-border-light);
  cursor: move;
  flex-shrink: 0;
}

.hint-item {
  display: flex;
  align-items: center;
  gap: var(--fy-space-1);
  font-size: var(--fy-text-sm);
  color: var(--fy-text-muted);
}

.hint-key {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 24px;
  height: 20px;
  padding: 0 var(--fy-space-1);
  background: var(--fy-bg-hover);
  border: 1px solid var(--fy-border-light);
  border-radius: var(--fy-radius-xs);
  font-size: var(--fy-text-xs);
  font-family: var(--fy-font-mono);
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
