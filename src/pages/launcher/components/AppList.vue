<template>
  <div class="app-list-container">
    <div v-if="thirdPartyApps.length > 0" :class="{ collapsed: thirdPartyCollapsed }" class="app-group">
      <div class="group-header sticky-header" @click="thirdPartyCollapsed = !thirdPartyCollapsed">
        <span class="group-title">
          <el-icon :class="{ collapsed: thirdPartyCollapsed }" :size="14" class="collapse-icon">
            <ArrowDown/>
          </el-icon>
          第三方应用
        </span>
        <span class="group-count">{{ thirdPartyApps.length }}</span>
      </div>
      <div ref="thirdPartySectionRef" :style="thirdPartyContentStyle" class="section-content">
        <div
            v-for="app in thirdPartyApps"
            :key="app.id"
            :class="{ 'ctx-anchor': ctxAnchorId === app.id }"
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
          <span v-if="getAppCategoryName(app.id)" class="app-tag category-tag">{{ getAppCategoryName(app.id) }}</span>
          <span v-if="getAppCommandPrefix(app.path)" class="app-tag command-tag">{{
              getAppCommandPrefix(app.path)
            }}</span>
          <span :class="app.source === 'manual' ? 'manual' : 'scan'"
                class="app-source-badge">{{ app.source === 'manual' ? '手动' : '扫描' }}</span>
        </div>
      </div>
    </div>

    <div v-if="systemApps.length > 0" :class="{ collapsed: systemCollapsed }" class="app-group">
      <div class="group-header sticky-header" @click="systemCollapsed = !systemCollapsed">
        <span class="group-title">
          <el-icon :class="{ collapsed: systemCollapsed }" :size="14" class="collapse-icon">
            <ArrowDown/>
          </el-icon>
          系统应用
        </span>
        <span class="group-count">{{ systemApps.length }}</span>
      </div>
      <div ref="systemSectionRef" :style="systemContentStyle" class="section-content">
        <div
            v-for="app in systemApps"
            :key="app.id"
            :class="{ 'ctx-anchor': ctxAnchorId === app.id }"
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
          <span v-if="getAppCategoryName(app.id)" class="app-tag category-tag">{{ getAppCategoryName(app.id) }}</span>
          <span v-if="getAppCommandPrefix(app.path)" class="app-tag command-tag">{{
              getAppCommandPrefix(app.path)
            }}</span>
          <span :class="app.source === 'manual' ? 'manual' : 'scan'"
                class="app-source-badge">{{ app.source === 'manual' ? '手动' : '扫描' }}</span>
        </div>
      </div>
    </div>

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
      <ContextSubMenu label="添加到分类">
        <div class="menu-category-list">
          <div
              v-for="cat in getCategories()"
              :key="cat.id"
              class="context-menu-item"
              @click="assignToCategory(ctxApp, cat.id)"
          >
            <el-icon :size="14">
              <component :is="getIcon(cat.icon)"/>
            </el-icon>
            <span>{{ cat.name }}</span>
          </div>
        </div>
      </ContextSubMenu>
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
      <div class="context-menu-item" @click="showAddCommandDialog">
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
import {computed, nextTick, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage} from 'element-plus'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import {ArrowDown, Close, Delete, FolderOpened, Monitor, Star} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'
import ContextMenu from '../../../components/ContextMenu.vue'
import ContextSubMenu from '../../../components/ContextSubMenu.vue'

const {t} = useI18n()

const props = defineProps({
  apps: {
    type: Array,
    required: true
  },
  categories: {
    type: Array,
    default: () => []
  },
  appCategoryMap: {
    type: Object,
    default: () => ({})
  },
  customCommands: {
    type: Array,
    default: () => []
  }
})

const emit = defineEmits(['select', 'category-changed'])

const getAppCategoryName = (appId) => {
  const catId = props.appCategoryMap[appId]
  if (!catId) return null
  const cat = props.categories.find(c => c.id === catId)
  return cat ? cat.name : null
}

const getAppCommandPrefix = (appPath) => {
  if (!appPath) return null
  const cmd = props.customCommands.find(c => {
    return c.enabled && c.command_type?.RunProgram?.path === appPath
  })
  return cmd ? cmd.prefix : null
}

const customCategories = ref([])
const thirdPartyCollapsed = ref(false)
const systemCollapsed = ref(false)
const ctxVisible = ref(false)
const ctxX = ref(0)
const ctxY = ref(0)
const ctxApp = ref(null)
const ctxAnchorId = ref(null)
const showCommandDialog = ref(false)
const commandForm = ref({
  prefix: ''
})

// 使用传入的 categories prop，如果没有则从后端加载
const getCategories = () => {
  if (props.categories && props.categories.length > 0) {
    return props.categories
  }
  return customCategories.value
}

const getIcon = (iconName) => {
  return ElementPlusIconsVue[iconName] || Monitor
}

const thirdPartySectionRef = ref(null)
const systemSectionRef = ref(null)
const thirdPartyActualHeight = ref('0px')
const systemActualHeight = ref('0px')

const thirdPartyApps = computed(() => props.apps.filter(a => a.app_type !== 'system'))
const systemApps = computed(() => props.apps.filter(a => a.app_type === 'system'))

const thirdPartyContentStyle = computed(() => ({
  maxHeight: thirdPartyCollapsed.value ? '0' : thirdPartyActualHeight.value,
}))

const systemContentStyle = computed(() => ({
  maxHeight: systemCollapsed.value ? '0' : systemActualHeight.value,
}))

const updateSectionHeights = async () => {
  await nextTick()
  if (thirdPartySectionRef.value && !thirdPartyCollapsed.value) {
    thirdPartyActualHeight.value = thirdPartySectionRef.value.scrollHeight + 'px'
  }
  if (systemSectionRef.value && !systemCollapsed.value) {
    systemActualHeight.value = systemSectionRef.value.scrollHeight + 'px'
  }
}

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
  closeCtxMenu()
}

const openAppDirectory = async (app) => {
  if (!app || !app.path) return
  try {
    await invoke('open_app_directory', {path: app.path})
    closeCtxMenu()
  } catch (error) {
    console.error('打开应用目录失败:', error)
  }
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

const assignToCategory = async (app, categoryId) => {
  if (!app || !app.id) return
  try {
    await invoke('set_app_category', {appId: app.id, categoryId})
    closeCtxMenu()
    // 通知父组件重新加载配置
    emit('category-changed')
  } catch (error) {
    console.error('Assign category error:', error)
  }
}

const removeFromCategory = async (app) => {
  if (!app || !app.id) return
  try {
    await invoke('set_app_category', {appId: app.id, categoryId: ''})
    closeCtxMenu()
    // 通知父组件重新加载配置
    emit('category-changed')
  } catch (error) {
    console.error('Remove category error:', error)
  }
}

const removeApp = async (app) => {
  if (!app || !app.id) return
  try {
    await invoke('remove_app_record', {appId: app.id})
    closeCtxMenu()
    emit('category-changed')
  } catch (error) {
    console.error('Remove app error:', error)
  }
}

// 显示添加命令对话框
const showAddCommandDialog = () => {
  const app = ctxApp.value
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
  closeCtxMenu()
}

// 确认添加命令
const confirmAddCommand = async () => {
  const app = ctxApp.value
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
      icon: app.icon_base64 || 'Monitor',
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

onMounted(async () => {
  await loadCategories()
  await nextTick()
  await updateSectionHeights()
})

watch(thirdPartyCollapsed, async (collapsed) => {
  if (!collapsed) {
    await nextTick()
    if (thirdPartySectionRef.value) {
      thirdPartyActualHeight.value = thirdPartySectionRef.value.scrollHeight + 'px'
    }
  }
})

watch(systemCollapsed, async (collapsed) => {
  if (!collapsed) {
    await nextTick()
    if (systemSectionRef.value) {
      systemActualHeight.value = systemSectionRef.value.scrollHeight + 'px'
    }
  }
})

// 监听应用列表变化，重新加载分类
watch(() => props.apps, async () => {
  await loadCategories()
  await nextTick()
  await updateSectionHeights()
}, {deep: true})

// 监听窗口焦点事件，当启动器显示时重新加载分类
const handleFocus = () => {
  loadCategories()
}
window.addEventListener('focus', handleFocus)

onBeforeUnmount(() => {
  window.removeEventListener('focus', handleFocus)
})
</script>

<style scoped>
.app-list-container {
  padding: 4px 0 8px 0;
  min-height: 100%;
}

.app-group {
  margin-bottom: 4px;
  transition: margin-bottom 0.3s ease;
}

.app-group.collapsed {
  margin-bottom: 0;
}

.group-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  background: var(--fy-bg-surface);
  border-bottom: 1px solid var(--fy-border-light);
  cursor: pointer;
  user-select: none;
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
  display: flex;
  align-items: center;
  gap: 4px;
}

.collapse-icon {
  transition: transform 0.2s ease;
}

.collapse-icon.collapsed {
  transform: rotate(-90deg);
}

.section-content {
  overflow: hidden;
  transition: max-height 0.3s ease;
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

.app-item:hover,
.app-item.ctx-anchor {
  background: var(--fy-accent-bg);
  padding-left: 20px;
  border-left: 3px solid var(--fy-accent);
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
  line-height: 1.2;
  color: var(--fy-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.app-category {
  font-size: 11px;
  line-height: 1.2;
  color: var(--fy-text-muted);
  margin-top: 2px;
}

.app-source-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  flex-shrink: 0;
  margin-left: 8px;
}

.app-source-badge.scan {
  background: var(--fy-bg-hover);
  color: var(--fy-text-muted);
}

.app-source-badge.manual {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

.app-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  flex-shrink: 0;
  margin-left: 4px;
  max-width: 80px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.category-tag {
  background: var(--fy-bg-hover);
  color: var(--fy-text-secondary);
}

.command-tag {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
  font-family: monospace;
}

.menu-category-list {
  max-height: calc(32px * 5);
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: thin;
  scrollbar-color: var(--fy-border) transparent;
  touch-action: pan-y;
  flex-shrink: 1;
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
