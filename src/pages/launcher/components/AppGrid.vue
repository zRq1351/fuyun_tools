<template>
  <div class="app-grid-container" @contextmenu.prevent>
    <div ref="categoriesContainer" class="categories-grid">
      <template v-for="(category, catIndex) in categories" :key="category.name">
        <div v-if="catDropIndex === catIndex && catDragging" class="cat-drop-indicator"></div>
        <div
            :class="{
              'cat-dragging': catDragging && catDragIndex === catIndex,
              'cat-drop-target': catDragging && catDropIndex === catIndex && catDragIndex !== catIndex
            }"
            class="category-box"
            @mousedown.prevent="onCatMouseDown($event, catIndex)"
        >
          <div class="category-header" @click="expandCategory(category)">
            <span class="category-name">{{ category.name }}</span>
            <span class="category-count">{{ category.apps.length }}</span>
          </div>
          <div class="category-apps">
            <div
                v-for="app in category.apps.slice(0, 4)"
                :key="app.id"
                class="app-item"
                @dblclick="$emit('select', app)"
                @contextmenu.prevent="showContextMenu($event, app)"
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
      </template>
      <div v-if="catDropIndex >= categories.length && catDragging" class="cat-drop-indicator"></div>
    </div>

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
              v-for="(app, index) in expandedCategory.apps"
              :key="app.id"
              :class="{ 'app-drag-ready': appDragging && appDragIndex === index }"
              class="app-item"
              @dblclick="$emit('select', app)"
              @contextmenu.prevent="showContextMenu($event, app)"
              @mousedown.prevent="onAppMouseDown($event, index, app)"
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

    <div v-if="appGhost.show" :style="{ left: appGhost.x + 'px', top: appGhost.y + 'px' }" class="drag-ghost app-ghost">
      <div class="app-icon">
        <img v-if="appGhost.app?.icon_base64" :src="appGhost.app.icon_base64" class="icon-img"/>
        <el-icon v-else :size="24">
          <Monitor/>
        </el-icon>
      </div>
      <div class="app-name">{{ appGhost.app?.title }}</div>
    </div>

    <div v-if="catGhost.show" :style="{ left: catGhost.x + 'px', top: catGhost.y + 'px' }" class="drag-ghost cat-ghost">
      <div class="ghost-cat-header">
        <span class="ghost-cat-name">{{ catGhost.name }}</span>
        <span class="ghost-cat-count">{{ catGhost.count }}</span>
      </div>
      <div class="ghost-cat-apps">
        <div v-for="app in catGhost.apps" :key="app.id" class="ghost-app-icon">
          <img v-if="app.icon_base64" :src="app.icon_base64" class="icon-img"/>
          <el-icon v-else :size="16">
            <Monitor/>
          </el-icon>
        </div>
      </div>
    </div>

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
import {onBeforeUnmount, onMounted, reactive, ref} from 'vue'
import {Close, Monitor} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'

const props = defineProps({
  categories: {type: Array, required: true}
})

const emit = defineEmits(['select', 'reorder-apps', 'reorder-categories'])

const contextMenu = ref({visible: false, x: 0, y: 0, app: null})
const expandedCategory = ref(null)
const appsContainer = ref(null)
const categoriesContainer = ref(null)

// App drag state
const appDragging = ref(false)
const appDragIndex = ref(-1)
const appGhost = reactive({show: false, x: 0, y: 0, app: null})
let appLongPressTimer = null
let appStartX = 0
let appStartY = 0
let lastAppTargetIndex = -1

// Category drag state
const catDragging = ref(false)
const catDragIndex = ref(-1)
const catDropIndex = ref(-1)
const catGhost = reactive({show: false, x: 0, y: 0, name: '', count: 0, apps: []})
let catLongPressTimer = null
let catStartX = 0
let catStartY = 0
let lastCatTargetIndex = -1

// Category functions
const expandCategory = (category) => {
  if (category.apps.length > 4 && !catDragging.value) {
    expandedCategory.value = category
  }
}

const closeExpanded = () => {
  expandedCategory.value = null
  cancelAppDrag()
}

const onCatMouseDown = (event, catIndex) => {
  if (event.button !== 0) return

  catStartX = event.clientX
  catStartY = event.clientY
  catDragIndex.value = catIndex

  catLongPressTimer = setTimeout(() => {
    catDragging.value = true
    catGhost.show = true
    catGhost.x = catStartX - 100
    catGhost.y = catStartY - 30
    catGhost.name = props.categories[catIndex].name
    catGhost.count = props.categories[catIndex].apps.length
    catGhost.apps = props.categories[catIndex].apps.slice(0, 4)
    lastCatTargetIndex = catIndex
  }, 600)

  document.addEventListener('mousemove', onCatMouseMove)
  document.addEventListener('mouseup', onCatMouseUp)
}

const onCatMouseMove = (event) => {
  if (catLongPressTimer && !catDragging.value) {
    const dx = Math.abs(event.clientX - catStartX)
    const dy = Math.abs(event.clientY - catStartY)
    if (dx > 5 || dy > 5) {
      clearTimeout(catLongPressTimer)
      catLongPressTimer = null
      document.removeEventListener('mousemove', onCatMouseMove)
      document.removeEventListener('mouseup', onCatMouseUp)
    }
    return
  }

  if (catDragging.value) {
    event.preventDefault()
    catGhost.x = event.clientX - 100
    catGhost.y = event.clientY - 30

    const container = categoriesContainer.value
    if (!container) return

    const items = container.querySelectorAll('.category-box')
    if (!items.length) return

    let targetIndex = items.length
    for (let i = 0; i < items.length; i++) {
      if (i === catDragIndex.value) continue
      const rect = items[i].getBoundingClientRect()
      const midX = rect.left + rect.width / 2
      if (event.clientX < midX) {
        targetIndex = i
        break
      }
    }

    if (targetIndex !== lastCatTargetIndex) {
      lastCatTargetIndex = targetIndex
      catDropIndex.value = targetIndex
    }
  }
}

const onCatMouseUp = () => {
  if (catLongPressTimer) {
    clearTimeout(catLongPressTimer)
    catLongPressTimer = null
  }

  if (catDragging.value && catDropIndex.value >= 0 && catDropIndex.value !== catDragIndex.value) {
    emit('reorder-categories', catDragIndex.value, catDropIndex.value > catDragIndex.value ? catDropIndex.value - 1 : catDropIndex.value)
  }

  catDragging.value = false
  catGhost.show = false
  catDragIndex.value = -1
  catDropIndex.value = -1
  lastCatTargetIndex = -1

  document.removeEventListener('mousemove', onCatMouseMove)
  document.removeEventListener('mouseup', onCatMouseUp)
}

// App functions
const onAppMouseDown = (event, index, app) => {
  if (event.button !== 0) return

  appStartX = event.clientX
  appStartY = event.clientY
  appDragIndex.value = index

  appLongPressTimer = setTimeout(() => {
    appDragging.value = true
    appGhost.show = true
    appGhost.x = appStartX - 30
    appGhost.y = appStartY - 30
    appGhost.app = app
    lastAppTargetIndex = appDragIndex.value
  }, 600)

  document.addEventListener('mousemove', onAppMouseMove)
  document.addEventListener('mouseup', onAppMouseUp)
}

const onAppMouseMove = (event) => {
  if (appLongPressTimer && !appDragging.value) {
    const dx = Math.abs(event.clientX - appStartX)
    const dy = Math.abs(event.clientY - appStartY)
    if (dx > 5 || dy > 5) {
      clearTimeout(appLongPressTimer)
      appLongPressTimer = null
      document.removeEventListener('mousemove', onAppMouseMove)
      document.removeEventListener('mouseup', onAppMouseUp)
    }
    return
  }

  if (appDragging.value) {
    event.preventDefault()
    appGhost.x = event.clientX - 30
    appGhost.y = event.clientY - 30

    const container = appsContainer.value
    if (!container) return

    const items = container.querySelectorAll('.app-item')
    if (!items.length) return

    let targetIndex = -1
    for (let i = 0; i < items.length; i++) {
      const rect = items[i].getBoundingClientRect()
      const midY = rect.top + rect.height / 2
      if (event.clientY < midY) {
        targetIndex = i
        break
      }
    }
    if (targetIndex === -1) targetIndex = items.length

    if (targetIndex !== lastAppTargetIndex) {
      lastAppTargetIndex = targetIndex

      const currentIndex = appDragIndex.value
      if (targetIndex !== currentIndex && targetIndex !== currentIndex + 1) {
        const apps = [...expandedCategory.value.apps]
        const [moved] = apps.splice(currentIndex, 1)
        const insertIndex = targetIndex > currentIndex ? targetIndex - 1 : targetIndex
        apps.splice(insertIndex, 0, moved)
        expandedCategory.value.apps = apps
        appDragIndex.value = insertIndex
      }
    }
  }
}

const onAppMouseUp = () => {
  if (appLongPressTimer) {
    clearTimeout(appLongPressTimer)
    appLongPressTimer = null
  }

  if (appDragging.value) {
    appDragging.value = false
    appGhost.show = false
    appDragIndex.value = -1
    appGhost.app = null
    lastAppTargetIndex = -1
    emit('reorder-apps', expandedCategory.value.apps)
  }

  document.removeEventListener('mousemove', onAppMouseMove)
  document.removeEventListener('mouseup', onAppMouseUp)
}

const cancelAppDrag = () => {
  if (appLongPressTimer) {
    clearTimeout(appLongPressTimer)
    appLongPressTimer = null
  }
  appDragging.value = false
  appGhost.show = false
  appDragIndex.value = -1
  appGhost.app = null
}

// Context menu
const openApp = (app) => {
  emit('select', app)
  contextMenu.value.visible = false
}

const showContextMenu = (event, app) => {
  const container = event.currentTarget.closest('.app-grid-container')
  const rect = container.getBoundingClientRect()
  const scrollTop = container.scrollTop
  let x = event.clientX - rect.left
  let y = event.clientY - rect.top + scrollTop
  contextMenu.value = {visible: true, x: Math.max(0, x), y: Math.max(0, y), app}
}

const hideContextMenu = () => {
  contextMenu.value.visible = false
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
  document.addEventListener('click', hideContextMenu)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', hideContextMenu)
  document.removeEventListener('mousemove', onCatMouseMove)
  document.removeEventListener('mouseup', onCatMouseUp)
  document.removeEventListener('mousemove', onAppMouseMove)
  document.removeEventListener('mouseup', onAppMouseUp)
  cancelAppDrag()
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
  transition: opacity 0.2s, border-color 0.2s;
}

.category-box.cat-dragging {
  opacity: 0.4;
  border: 2px dashed var(--fy-accent);
}

.category-box.cat-drop-target {
  border: 2px solid var(--fy-accent);
  background: var(--fy-accent-bg);
}

.cat-drop-indicator {
  width: 3px;
  height: 100%;
  min-height: 100px;
  background: var(--fy-accent);
  border-radius: 2px;
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
  cursor: pointer;
  transition: all 0.15s;
  user-select: none;
}

.app-item:hover {
  background: var(--fy-bg-hover);
}

.app-item.app-drag-ready {
  opacity: 0.4;
  border: 2px dashed var(--fy-accent);
  background: var(--fy-accent-bg);
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

.drag-ghost {
  position: fixed;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 8px;
  background: var(--fy-bg-surface);
  border: 2px solid var(--fy-accent);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  opacity: 0.9;
  pointer-events: none;
  z-index: 9999;
}

.app-ghost .app-icon {
  width: 36px;
  height: 36px;
}

.app-ghost .app-name {
  font-size: 11px;
  max-width: 60px;
}

.cat-ghost {
  min-width: 180px;
  padding: 10px 12px;
  background: var(--fy-bg-card);
}

.ghost-cat-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--fy-border-light);
}

.ghost-cat-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--fy-text-primary);
}

.ghost-cat-count {
  font-size: 11px;
  color: var(--fy-text-muted);
  background: var(--fy-bg-hover);
  padding: 1px 6px;
  border-radius: 10px;
}

.ghost-cat-apps {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 4px;
  padding-top: 8px;
}

.ghost-app-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  overflow: hidden;
  background: var(--fy-bg-hover);
}

.ghost-app-icon .icon-img {
  width: 24px;
  height: 24px;
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
  position: absolute;
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  border-radius: 8px;
  padding: 4px 0;
  min-width: 120px;
  box-shadow: var(--fy-shadow);
  z-index: 100;
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
