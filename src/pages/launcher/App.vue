<template>
  <div class="launcher-container">
    <div ref="launcherBoxRef" class="launcher-box">
      <div class="search-wrapper" @mousedown="startDrag">
        <SearchBox
            v-model="searchQuery"
            @blur="isFocused = false"
            @focus="isFocused = true"
            @input="handleSearch"
            @keydown="handleKeydown"
        />
        <button class="close-button" @click="hideLauncher" @mousedown.stop>
          <el-icon :size="16">
            <Close/>
          </el-icon>
        </button>
      </div>
      <div class="content-area">
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
        <AppGrid
            v-else-if="!searchQuery && appCategories.length > 0"
            :categories="appCategories"
            @select="handleSelect"
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
import {onBeforeUnmount, onMounted, ref} from 'vue'
import {Close, Search} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {invoke} from '@tauri-apps/api/core'
import SearchBox from './components/SearchBox.vue'
import ResultList from './components/ResultList.vue'
import AppGrid from './components/AppGrid.vue'
import {useLauncherSearch} from './composables/useLauncherSearch'

const searchQuery = ref('')
const results = ref([])
const appCategories = ref([])
const activeIndex = ref(0)
const isFocused = ref(false)
const isSearching = ref(false)
const launcherBoxRef = ref(null)

const {search, executeAction} = useLauncherSearch()

let unlistenShow = null

const loadAllApps = async () => {
  try {
    appCategories.value = await invoke('get_all_apps')
    loadIcons()
  } catch (error) {
    console.error('Load apps error:', error)
  }
}

const loadIcons = async () => {
  const allPaths = []
  for (const category of appCategories.value) {
    for (const app of category.apps) {
      if (app.path && !app.icon_base64) {
        allPaths.push(app.path)
      }
    }
  }
  if (allPaths.length === 0) return

  try {
    const icons = await invoke('batch_extract_icons', {paths: allPaths})
    for (const category of appCategories.value) {
      for (const app of category.apps) {
        if (app.path && icons[app.path]) {
          app.icon_base64 = icons[app.path]
        }
      }
    }
  } catch (error) {
    console.error('Load icons error:', error)
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
    console.error('Execute error:', error)
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
      event.target.closest('.clear-button') ||
      event.target.closest('.close-button')) {
    return
  }
  try {
    const window = getCurrentWebviewWindow()
    await window.startDragging()
  } catch (error) {
    console.error('Start drag error:', error)
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
  if (unlistenShow) {
    unlistenShow()
  }
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

.content-area {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.close-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  margin-right: 8px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.2s;
  flex-shrink: 0;
}

.close-button:hover {
  background: var(--fy-danger-bg);
  color: var(--fy-danger);
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

.no-results .el-icon {
  font-size: 18px;
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
