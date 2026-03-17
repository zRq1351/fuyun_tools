<template>
  <el-config-provider :locale="zhCn">
    <div :class="{ dark: isDark }" class="settings-container">
      <div class="header">
        <el-radio-group v-model="activeTab" size="large">
          <el-radio-button label="clipboard">
            <el-icon>
              <DocumentCopy/>
            </el-icon>
            剪切板设置
          </el-radio-button>
          <el-radio-button label="ai">
            <el-icon>
              <Cpu/>
            </el-icon>
            AI设置
          </el-radio-button>
          <el-radio-button label="about">
            <el-icon>
              <InfoFilled/>
            </el-icon>
            关于
          </el-radio-button>
        </el-radio-group>
        <div class="header-actions">
          <span :class="['autosave-status', `autosave-${autoSaveState}`]">{{ autoSaveText }}</span>
          <el-button @click="toggleTheme">
            <template #icon>
              <component :is="isDark ? Sunny : Moon"/>
            </template>
            {{ isDark ? '白天' : '黑夜' }}
          </el-button>
        </div>
      </div>

      <div class="content">
        <el-alert
            v-if="shortcutConflictMessage"
            :closable="false"
            :title="shortcutConflictMessage"
            show-icon
            type="error"
        />
        <div v-show="activeTab === 'clipboard'">
          <ClipboardSettings :form="form"/>
        </div>

        <div v-show="activeTab === 'ai'">
          <AISettings ref="aiSettingsRef" :form="form"/>
        </div>

        <div v-show="activeTab === 'about'">
          <AboutSettings
              :current-version="currentVersion"
              :image-toggle-shortcut="form.imageToggleShortcut"
              :toggle-shortcut="form.toggleShortcut"
          />
        </div>
      </div>

      <div class="footer-links">
        <p>
          需要帮助？
          <el-link type="primary" @click="openExternal('https://github.com/zRq1351/fuyun_tools')">查看文档</el-link>
          |
          <el-link type="primary" @click="openExternal('https://github.com/zRq1351/fuyun_tools/issues')">报告问题
          </el-link>
        </p>
        <p>版本 {{ currentVersion }} | © {{ new Date().getFullYear() }} fuyun_tools</p>
      </div>
    </div>
  </el-config-provider>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref, watch} from 'vue'
import {ElMessage} from 'element-plus'
import zhCn from 'element-plus/dist/locale/zh-cn'
import {Cpu, DocumentCopy, InfoFilled, Moon, Sunny} from '@element-plus/icons-vue'
import {openUrl} from '@tauri-apps/plugin-opener'
import {listen} from '@tauri-apps/api/event'
import {AISettingsService} from '../../services/ipc'
import ClipboardSettings from './components/ClipboardSettings.vue'
import AISettings from './components/AISettings.vue'
import AboutSettings from './components/AboutSettings.vue'

const activeTab = ref('clipboard')
const isDark = ref(false)
const currentVersion = ref('0.0.0')
const aiSettingsRef = ref(null)
const shortcutConflictMessage = ref('')
let unlistenShortcutConflict = null
let saveTimer = null
let autoSaveStateResetTimer = null
const isInitializing = ref(true)
const isAutoSaving = ref(false)
const suppressNextAutoSave = ref(false)
const autoSaveState = ref('idle')

const form = reactive({
  textMaxItems: 100,
  imageMaxItems: 100,
  imageDiskLimitMb: 2048,
  groupedItemsProtectedFromLimit: true,
  toggleShortcut: '',
  imageToggleShortcut: '',
  aiProvider: '',
  apiUrl: '',
  modelName: '',
  apiKey: '',
  customProviderName: '',
  selectionEnabled: true,
  translationPromptTemplate: '',
  explanationPromptTemplate: '',
  imageFillVerifyMode: 'fast'
})

const toggleTheme = () => {
  isDark.value = !isDark.value
  const html = document.documentElement
  if (isDark.value) {
    html.classList.add('dark')
    localStorage.setItem('settings-theme', 'dark')
  } else {
    html.classList.remove('dark')
    localStorage.setItem('settings-theme', 'light')
  }
}

const autoSaveText = computed(() => {
  if (autoSaveState.value === 'pending') return '有未保存变更'
  if (autoSaveState.value === 'saving') return '自动保存中...'
  if (autoSaveState.value === 'saved') return '已自动保存'
  if (autoSaveState.value === 'error') return '自动保存失败'
  return '已同步'
})

const persistSettings = async (silent = false) => {
  if (isInitializing.value || isAutoSaving.value) {
    return
  }
  let selectedProvider = form.aiProvider
  if (selectedProvider === 'custom') {
    if (!form.customProviderName) {
      return
    }
    selectedProvider = form.customProviderName
  }

  try {
    isAutoSaving.value = true
    if (autoSaveStateResetTimer) {
      clearTimeout(autoSaveStateResetTimer)
      autoSaveStateResetTimer = null
    }
    autoSaveState.value = 'saving'
    await AISettingsService.saveSettings({
      textMaxItems: form.textMaxItems,
      imageMaxItems: form.imageMaxItems,
      imageDiskLimitMb: form.imageDiskLimitMb,
      aiProvider: selectedProvider,
      aiApiUrl: form.apiUrl,
      aiModelName: form.modelName,
      aiApiKey: form.apiKey,
      hotKey: form.toggleShortcut,
      imageHotKey: form.imageToggleShortcut,
      selectionEnabled: form.selectionEnabled,
      groupedItemsProtectedFromLimit: form.groupedItemsProtectedFromLimit,
      translationPromptTemplate: form.translationPromptTemplate,
      explanationPromptTemplate: form.explanationPromptTemplate,
      imageFillVerifyMode: form.imageFillVerifyMode
    })

    if (form.aiProvider === 'custom' && form.customProviderName === selectedProvider) {
      if (aiSettingsRef.value) {
        suppressNextAutoSave.value = true
        await aiSettingsRef.value.loadAiProviders()
      }
      form.aiProvider = selectedProvider
    }
    shortcutConflictMessage.value = ''
    if (!silent) {
      ElMessage.success('已自动保存')
    }
    autoSaveState.value = 'saved'
    autoSaveStateResetTimer = window.setTimeout(() => {
      if (autoSaveState.value === 'saved') {
        autoSaveState.value = 'idle'
      }
      autoSaveStateResetTimer = null
    }, 1500)
  } catch (error) {
    const raw = String(error || '')
    if (raw.includes('快捷键被占用')) {
      shortcutConflictMessage.value = raw.replace(/^Error:\s*/i, '')
      activeTab.value = 'clipboard'
    }
    ElMessage.error(`保存失败: ${error}`)
    autoSaveState.value = 'error'
  } finally {
    isAutoSaving.value = false
  }
}

const openExternal = async (url) => {
  try {
    await openUrl(url)
  } catch (err) {
    ElMessage.error(err)
  }
}

const normalizeShortcutConflicts = (payload) => {
  if (Array.isArray(payload)) {
    return payload.filter((item) => typeof item === 'string' && item.trim())
  }
  if (payload && Array.isArray(payload.conflicts)) {
    return payload.conflicts.filter((item) => typeof item === 'string' && item.trim())
  }
  return []
}

const showShortcutConflictWarning = (payload) => {
  const conflicts = normalizeShortcutConflicts(payload)
  if (conflicts.length === 0) return
  activeTab.value = 'clipboard'
  shortcutConflictMessage.value = `快捷键被占用：${conflicts.join('；')}`
}

onMounted(async () => {
  unlistenShortcutConflict = await listen('shortcut-conflict-warning', (event) => {
    showShortcutConflictWarning(event.payload)
  })

  if (window.__SHORTCUT_CONFLICT__) {
    showShortcutConflictWarning(window.__SHORTCUT_CONFLICT__)
    window.__SHORTCUT_CONFLICT__ = null
  }

  const savedTheme = localStorage.getItem('settings-theme')
  const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches
  if (savedTheme === 'dark' || (!savedTheme && prefersDark)) {
    isDark.value = true
    document.documentElement.classList.add('dark')
  }

  try {
    const settings = await AISettingsService.getSettings()

    form.textMaxItems = settings.text_max_items || settings.max_items || 50
    form.imageMaxItems = settings.image_max_items || settings.max_items || 50
    form.imageDiskLimitMb = settings.image_disk_limit_mb || 2048
    currentVersion.value = settings.version || '0.3.1'
    form.toggleShortcut = settings.hot_key || ''
    form.imageToggleShortcut = settings.image_hot_key || ''
    form.selectionEnabled = settings.selection_enabled !== false
    form.groupedItemsProtectedFromLimit = settings.grouped_items_protected_from_limit !== false
    form.translationPromptTemplate = settings.translation_prompt_template || ''
    form.explanationPromptTemplate = settings.explanation_prompt_template || ''
    form.imageFillVerifyMode = settings.image_fill_verify_mode === 'strict' ? 'strict' : 'fast'

    if (aiSettingsRef.value) {
      aiSettingsRef.value.applyCurrentProviderConfig(settings)
    } else {
      const currentProvider = settings.ai_provider || ''
      form.aiProvider = currentProvider
      const providerConfigs = settings.provider_configs || {}
      if (currentProvider && providerConfigs[currentProvider]) {
        const config = providerConfigs[currentProvider]
        form.apiUrl = config.api_url || ''
        form.modelName = config.model_name || ''
        form.apiKey = config.api_key || ''
      }
    }
  } catch (error) {
    ElMessage.error(`加载设置失败: ${error}`)
    autoSaveState.value = 'error'
  } finally {
    isInitializing.value = false
  }
})

watch(form, () => {
  if (isInitializing.value) return
  if (suppressNextAutoSave.value) {
    suppressNextAutoSave.value = false
    return
  }
  if (!isAutoSaving.value) {
    if (autoSaveStateResetTimer) {
      clearTimeout(autoSaveStateResetTimer)
      autoSaveStateResetTimer = null
    }
    autoSaveState.value = 'pending'
  }
  if (saveTimer) {
    clearTimeout(saveTimer)
    saveTimer = null
  }
  saveTimer = window.setTimeout(() => {
    persistSettings(true)
    saveTimer = null
  }, 450)
}, {deep: true})

onBeforeUnmount(() => {
  if (saveTimer) {
    clearTimeout(saveTimer)
    saveTimer = null
  }
  if (autoSaveStateResetTimer) {
    clearTimeout(autoSaveStateResetTimer)
    autoSaveStateResetTimer = null
  }
  if (unlistenShortcutConflict) {
    unlistenShortcutConflict()
    unlistenShortcutConflict = null
  }
})
</script>

<style>
body {
  margin: 0;
  font-family: 'Helvetica Neue', Helvetica, 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', '微软雅黑', Arial, sans-serif;
}

.settings-container {
  padding: 20px;
  max-width: 800px;
  margin: 0 auto;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.autosave-status {
  font-size: 12px;
}

.autosave-idle {
  color: #909399;
}

.autosave-pending {
  color: #e6a23c;
}

.autosave-saving {
  color: #409eff;
}

.autosave-saved {
  color: #67c23a;
}

.autosave-error {
  color: #f56c6c;
}

.content {
  background: #fff;
  padding: 20px;
  border-radius: 8px;
  box-shadow: 0 2px 12px 0 rgba(0, 0, 0, 0.1);
}

.dark .content {
  background: #1d1e1f;
  box-shadow: 0 2px 12px 0 rgba(0, 0, 0, 0.3);
}

.footer-links {
  margin-top: 40px;
  text-align: center;
  color: #909399;
  font-size: 14px;
}
</style>
