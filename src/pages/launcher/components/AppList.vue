<template>
  <div class="app-list-container">
    <div v-if="thirdPartyApps.length > 0" class="app-group">
      <div class="group-header sticky-header">
        <span class="group-title">第三方应用</span>
        <span class="group-count">{{ thirdPartyApps.length }}</span>
      </div>
      <div
          v-for="app in thirdPartyApps"
          :key="app.id"
          class="app-item"
          @dblclick="$emit('select', app)"
          @contextmenu.prevent="showContextMenu($event, app)"
      >
        <div class="app-icon">
          <img v-if="app.icon_base64" :src="app.icon_base64" class="icon-img"/>
          <el-icon v-else :size="20">
            <Monitor/>
          </el-icon>
        </div>
        <div class="app-info">
          <div class="app-name">{{ app.title }}</div>
          <div v-if="app.category" class="app-category">{{ app.category }}</div>
        </div>
      </div>
    </div>

    <div v-if="systemApps.length > 0" class="app-group">
      <div class="group-header sticky-header">
        <span class="group-title">系统应用</span>
        <span class="group-count">{{ systemApps.length }}</span>
      </div>
      <div
          v-for="app in systemApps"
          :key="app.id"
          class="app-item"
          @dblclick="$emit('select', app)"
          @contextmenu.prevent="showContextMenu($event, app)"
      >
        <div class="app-icon">
          <img v-if="app.icon_base64" :src="app.icon_base64" class="icon-img"/>
          <el-icon v-else :size="20">
            <Monitor/>
          </el-icon>
        </div>
        <div class="app-info">
          <div class="app-name">{{ app.title }}</div>
          <div v-if="app.category" class="app-category">{{ app.category }}</div>
        </div>
      </div>
    </div>

    <div
        v-if="contextMenu.visible"
        class="context-menu"
        :style="{ top: contextMenu.y + 'px', left: contextMenu.x + 'px' }"
        @click.stop
    >
      <div class="menu-item" @click="openApp(contextMenu.app)">
        <el-icon :size="14">
          <Monitor/>
        </el-icon>
        <span>打开</span>
      </div>
      <div class="menu-divider"></div>
      <div class="menu-title">添加到分类</div>
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
      <div v-if="customCategories.length > 0" class="menu-divider"></div>
      <div class="menu-item" @click="removeFromCategory(contextMenu.app)">
        <el-icon :size="14">
          <Close/>
        </el-icon>
        <span>移出分类</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import {ref, computed, onMounted, onBeforeUnmount} from 'vue'
import {Monitor, Close, Grid} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'

const props = defineProps({
  apps: {
    type: Array,
    required: true
  }
})

defineEmits(['select'])

const customCategories = ref([])
const contextMenu = ref({visible: false, x: 0, y: 0, app: null})

const iconMap = {Monitor, Grid}

const getIcon = (iconName) => {
  return iconMap[iconName] || Grid
}

const thirdPartyApps = computed(() => props.apps.filter(a => a.app_type !== 'system'))
const systemApps = computed(() => props.apps.filter(a => a.app_type === 'system'))

const loadCategories = async () => {
  try {
    const config = await invoke('get_launcher_config')
    customCategories.value = config.categories || []
  } catch (error) {
    console.error('Load categories error:', error)
  }
}

const openApp = (app) => {
  emit('select', app)
  contextMenu.value.visible = false
}

const showContextMenu = (event, app) => {
  const container = event.currentTarget.closest('.app-list-container')
  const rect = container.getBoundingClientRect()
  const scrollTop = container.scrollTop

  let x = event.clientX - rect.left
  let y = event.clientY - rect.top + scrollTop

  contextMenu.value = {visible: true, x: Math.max(0, x), y: Math.max(0, y), app}
}

const hideContextMenu = () => {
  contextMenu.value.visible = false
}

const assignToCategory = async (app, categoryId) => {
  if (!app || !app.id) return
  try {
    await invoke('set_app_category', {appId: app.id, categoryId})
    hideContextMenu()
  } catch (error) {
    console.error('Assign category error:', error)
  }
}

const removeFromCategory = async (app) => {
  if (!app || !app.id) return
  try {
    await invoke('set_app_category', {appId: app.id, categoryId: ''})
    hideContextMenu()
  } catch (error) {
    console.error('Remove category error:', error)
  }
}

onMounted(() => {
  loadCategories()
  document.addEventListener('click', hideContextMenu)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', hideContextMenu)
})
</script>

<style scoped>
.app-list-container {
  padding: 4px 0;
  position: relative;
  height: 100%;
}

.app-group {
  margin-bottom: 4px;
}

.group-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  background: var(--fy-bg-surface);
  border-bottom: 1px solid var(--fy-border-light);
}

.sticky-header {
  position: sticky;
  top: 0;
  z-index: 10;
}

.group-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--fy-text-muted);
}

.group-count {
  font-size: 11px;
  color: var(--fy-text-muted);
  background: var(--fy-bg-hover);
  padding: 1px 6px;
  border-radius: 10px;
}

.app-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  cursor: pointer;
  transition: all 0.15s;
  user-select: none;
}

.app-item:hover {
  background: var(--fy-bg-hover);
}

.app-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 8px;
  overflow: hidden;
  flex-shrink: 0;
}

.icon-img {
  width: 32px;
  height: 32px;
  object-fit: contain;
}

.app-info {
  flex: 1;
  min-width: 0;
}

.app-name {
  font-size: 13px;
  color: var(--fy-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.app-category {
  font-size: 11px;
  color: var(--fy-text-muted);
  margin-top: 2px;
}

.context-menu {
  position: absolute;
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 8px;
  padding: 4px 0;
  min-width: 160px;
  box-shadow: var(--fy-shadow);
  z-index: 100;
}

.menu-title {
  padding: 6px 12px;
  font-size: 11px;
  color: var(--fy-text-muted);
  border-bottom: 1px solid var(--fy-border-light);
  margin-bottom: 4px;
}

.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  font-size: 12px;
  color: var(--fy-text-primary);
  cursor: pointer;
  transition: background 0.1s;
}

.menu-item:hover {
  background: var(--fy-bg-hover);
}

.menu-divider {
  height: 1px;
  background: var(--fy-border-light);
  margin: 4px 0;
}
</style>
