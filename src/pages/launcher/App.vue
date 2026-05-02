<template>
  <div class="launcher-container">
    <div ref="launcherBoxRef" class="launcher-box">
      <div class="search-wrapper" @mousedown="startDrag">
        <SearchBox
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
        <ResultList
            v-if="results.length > 0"
            :active-index="activeIndex"
            :results="results"
            @hover="activeIndex = $event"
            @select="handleSelect"
        />
        <div v-else-if="searchQuery && !isSearching" class="no-results">
          <el-icon>
            <Search/>
          </el-icon>
          <span>未找到匹配项</span>
        </div>
        <div v-else-if="!searchQuery" class="app-content">
          <AppGrid
              v-if="viewMode === 'category' && hasCategorizedApps"
              :categories="categorizedApps"
              @select="handleSelect"
              @reorder-apps="handleReorderApps"
              @reorder-categories="handleReorderCategories"
              @category-changed="handleCategoryChanged"
          />
          <AppList
              v-else
              :apps="allApps"
              :categories="launcherConfig?.categories || []"
              @reorder="handleReorder"
              @select="handleSelect"
              @category-changed="handleCategoryChanged"
          />
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
      </div>

      <div class="launcher-hints" @mousedown="startDrag">
        <div class="hint-item">
          <span class="hint-key">↑↓</span>
          <span class="hint-text">导航</span>
        </div>
        <div class="hint-item">
          <span class="hint-key">Enter</span>
          <span class="hint-text">执行</span>
        </div>
        <div class="hint-item">
          <span class="hint-key">Esc</span>
          <span class="hint-text">关闭</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, ref} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {Close, Grid, List, Loading, Refresh, Search, Setting, Tools} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {invoke} from '@tauri-apps/api/core'
import SearchBox from './components/SearchBox.vue'
import ResultList from './components/ResultList.vue'
import AppGrid from './components/AppGrid.vue'
import AppList from './components/AppList.vue'
import CategoryManager from './components/CategoryManager.vue'
import CommandManager from './components/CommandManager.vue'
import {useLauncherSearch} from './composables/useLauncherSearch'

const searchQuery = ref('')
const results = ref([])
const allApps = ref([])
const activeIndex = ref(0)
const isFocused = ref(false)
const isSearching = ref(false)
const isLoading = ref(false)
const isRefreshing = ref(false)
const launcherBoxRef = ref(null)
const viewMode = ref('list')
const showCategoryManager = ref(false)
const showCommandManager = ref(false)
const launcherConfig = ref(null)

const {search, executeAction, loadCustomCommands} = useLauncherSearch()

let unlistenShow = null

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
  return launcherConfig.value.categories.length > 0 &&
      Object.keys(launcherConfig.value.app_category_map || {}).length > 0
})

const categorizedApps = computed(() => {
  if (!launcherConfig.value) return []
  const config = launcherConfig.value
  const result = []
  for (const category of config.categories) {
    const apps = allApps.value
        .filter(app => config.app_category_map[app.id] === category.id)
        .sort((a, b) => (a.sort_order || 0) - (b.sort_order || 0))
    if (apps.length > 0) {
      result.push({name: category.name, apps})
    }
  }
  return result
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

const handleSearch = async () => {
  if (!searchQuery.value.trim()) {
    results.value = []
    return
  }
  isSearching.value = true
  try {
    results.value = await search(searchQuery.value)
    activeIndex.value = 0
  } catch (error) {
    console.error('Search error:', error)
    results.value = []
  } finally {
    isSearching.value = false
  }
}

const handleClear = () => {
  // 直接清空搜索结果
  searchQuery.value = ''
  results.value = []
  activeIndex.value = 0
}

const handleKeydown = (event) => {
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault()
      if (results.value.length > 0) {
        activeIndex.value = (activeIndex.value + 1) % results.value.length
      }
      break
    case 'ArrowUp':
      event.preventDefault()
      if (results.value.length > 0) {
        activeIndex.value = (activeIndex.value - 1 + results.value.length) % results.value.length
      }
      break
    case 'Enter':
      event.preventDefault()
      if (results.value.length > 0 && activeIndex.value < results.value.length) {
        handleSelect(results.value[activeIndex.value])
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
      event.target.closest('.mode-button')) {
    return
  }
  try {
    const window = getCurrentWebviewWindow()
    await window.startDragging()
  } catch (error) {
    console.error('Start drag error:', error)
  }
}

const handleCategoryUpdated = async () => {
  launcherConfig.value = await invoke('get_launcher_config')
  // 重新加载自定义命令
  await loadCustomCommands()
}

const handleCategoryChanged = async () => {
  // 当分类发生变化时，重新加载配置
  launcherConfig.value = await invoke('get_launcher_config')
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

const handleReorderCategories = async (fromIndex, toIndex) => {
  if (!launcherConfig.value) return
  const categories = [...launcherConfig.value.categories]
  if (fromIndex < 0 || fromIndex >= categories.length || toIndex < 0 || toIndex > categories.length) return
  const [moved] = categories.splice(fromIndex, 1)
  categories.splice(toIndex, 0, moved)
  launcherConfig.value.categories = categories
  try {
    await invoke('save_launcher_config', {config: launcherConfig.value})
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
    results.value = []
    activeIndex.value = 0
    await loadAllApps()
  })
})

onBeforeUnmount(() => {
  if (unlistenShow) unlistenShow()
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
  z-index: 9999;
}

.launcher-box {
  width: 620px;
  height: 480px;
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 12px;
  box-shadow: var(--fy-shadow-lg);
  backdrop-filter: var(--fy-backdrop-blur);
  overflow: hidden;
  display: flex;
  flex-direction: column;
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

.app-content {
  height: 100%;
}

.no-results {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 24px;
  color: var(--fy-text-muted);
  font-size: 14px;
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
