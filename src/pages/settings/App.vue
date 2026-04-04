<template>
  <el-config-provider :locale="zhCn">
    <div :class="{ dark: isDark }" class="settings-container">
      <div class="header">
        <div class="header-title">
          <h1>设置中心</h1>
        </div>
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

      <div class="settings-layout">
        <aside class="settings-nav">
          <button
              v-for="section in sections"
              :key="section.key"
              :class="['section-nav-item', {active: activeTab === section.key}]"
              @click="activeTab = section.key"
          >
            <el-icon>
              <component :is="section.icon"/>
            </el-icon>
            <span>{{ section.label }}</span>
          </button>
        </aside>
        <div class="content">
          <el-alert
              v-if="shortcutConflictMessage"
              :closable="false"
              :title="shortcutConflictMessage"
              show-icon
              type="error"
          />
          <div class="content-header">
            <h2>{{ currentSection.label }}</h2>
            <p>{{ currentSection.description }}</p>
          </div>
          <div v-show="activeTab === 'clipboard'">
            <ClipboardSettings :form="form"/>
          </div>
          <div v-show="activeTab === 'screenshot'">
            <ScreenshotSettings :form="form"/>
          </div>
          <div v-show="activeTab === 'recording'">
            <RecordingSettings :form="form"/>
          </div>

          <div v-show="activeTab === 'selection'">
            <SelectionSettings :form="form"/>
          </div>

          <div v-show="activeTab === 'ai'">
            <AISettings ref="aiSettingsRef" :form="form"/>
          </div>

          <div v-if="isDevMode" v-show="activeTab === 'developer'">
            <DeveloperSettings/>
          </div>
          <div v-show="activeTab === 'about'">
            <AboutSettings
                :current-version="currentVersion"
                :image-toggle-shortcut="form.imageToggleShortcut"
                :screenshot-toggle-shortcut="form.screenshotToggleShortcut"
                :toggle-shortcut="form.toggleShortcut"
            />
          </div>
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
import {ElLoading, ElMessage, ElMessageBox} from 'element-plus'
import zhCn from 'element-plus/dist/locale/zh-cn'
import {Camera, Cpu, DocumentCopy, InfoFilled, Moon, Setting, Sunny, VideoCamera} from '@element-plus/icons-vue'
import {openUrl} from '@tauri-apps/plugin-opener'
import {listen} from '@tauri-apps/api/event'
import {AISettingsService, RecordingService} from '../../services/ipc'
import ClipboardSettings from './components/ClipboardSettings.vue'
import ScreenshotSettings from './components/ScreenshotSettings.vue'
import RecordingSettings from './components/RecordingSettings.vue'
import SelectionSettings from './components/SelectionSettings.vue'
import AISettings from './components/AISettings.vue'
import AboutSettings from './components/AboutSettings.vue'
import DeveloperSettings from '@dev/DeveloperSettings'

const activeTab = ref('clipboard')
const isDark = ref(false)
const currentVersion = ref('0.0.0')
const aiSettingsRef = ref(null)
const shortcutConflictMessage = ref('')
let unlistenShortcutConflict = null
let unlistenNavigateSettings = null
let saveTimer = null
let autoSaveStateResetTimer = null
const isInitializing = ref(true)
const isAutoSaving = ref(false)
const suppressNextAutoSave = ref(false)
const autoSaveState = ref('idle')
// 保存初始状态用于差异比较
const initialFormState = ref(null)
// 阻止初始化后的第一次 watch 触发
const skipNextWatch = ref(false)
const isDevMode = __DEV_PANEL__

const sections = computed(() => {
  const baseSections = [
    {
      key: 'clipboard',
      label: '剪贴板',
      description: '管理历史记录、快捷键与导入',
      icon: DocumentCopy
    },
    {
      key: 'screenshot',
      label: '截图',
      description: '管理截图功能相关的快捷键设置',
      icon: Camera
    },
    {
      key: 'recording',
      label: '录屏',
      description: '管理录屏录音参数与录制快捷键',
      icon: VideoCamera
    },
    {
      key: 'selection',
      label: '划词',
      description: '管理划词开关与翻译解释提示词模板',
      icon: Setting
    },
    {
      key: 'ai',
      label: 'AI',
      description: '配置服务提供商、模型参数与提示词模板',
      icon: Cpu
    }
  ]
  if (isDevMode) {
    baseSections.push({
      key: 'developer',
      label: '开发者',
      description: '开发调试信息与存储占用监控',
      icon: Setting
    })
  }
  baseSections.push({
    key: 'about',
    label: '关于',
    description: '查看版本信息与使用说明',
    icon: InfoFilled
  })
  return baseSections
})

const form = reactive({
  textMaxItems: 100,
  imageMaxItems: 100,
  imageDiskLimitMb: 2048,
  textClipboardEnabled: true,
  imageClipboardEnabled: true,
  screenshotEnabled: true,
  recordingEnabled: true,
  groupedItemsProtectedFromLimit: true,
  toggleShortcut: '',
  imageToggleShortcut: '',
  screenshotToggleShortcut: '',
  recordingToggleShortcut: '',
  recordingDefaultFps: 30,
  recordingDefaultVideoBitrateKbps: 6000,
  recordingDefaultAudioBitrateKbps: 160,
  recordingCaptureCursor: true,
  recordingCaptureSystemAudio: false,
  recordingCaptureMicrophone: true,
  recordingMicrophoneDeviceId: '',
  recordingOutputDir: '',
  recordingAutoOpenFolder: true,
  recordingToolbarContentProtected: false,
  recordingMaxDurationMinutes: 180,
  recordingFileNameTemplate: '{timestamp}',
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

const currentSection = computed(() => sections.value.find((section) => section.key === activeTab.value) || sections.value[0])

// 保存初始状态快照
const saveInitialFormState = () => {
  initialFormState.value = {
    textMaxItems: form.textMaxItems,
    imageMaxItems: form.imageMaxItems,
    imageDiskLimitMb: form.imageDiskLimitMb,
    textClipboardEnabled: form.textClipboardEnabled,
    imageClipboardEnabled: form.imageClipboardEnabled,
    screenshotEnabled: form.screenshotEnabled,
    recordingEnabled: form.recordingEnabled,
    groupedItemsProtectedFromLimit: form.groupedItemsProtectedFromLimit,
    toggleShortcut: form.toggleShortcut,
    imageToggleShortcut: form.imageToggleShortcut,
    screenshotToggleShortcut: form.screenshotToggleShortcut,
    recordingToggleShortcut: form.recordingToggleShortcut,
    recordingDefaultFps: form.recordingDefaultFps,
    recordingDefaultVideoBitrateKbps: form.recordingDefaultVideoBitrateKbps,
    recordingDefaultAudioBitrateKbps: form.recordingDefaultAudioBitrateKbps,
    recordingCaptureCursor: form.recordingCaptureCursor,
    recordingCaptureSystemAudio: form.recordingCaptureSystemAudio,
    recordingCaptureMicrophone: form.recordingCaptureMicrophone,
    recordingMicrophoneDeviceId: form.recordingMicrophoneDeviceId,
    recordingOutputDir: form.recordingOutputDir,
    recordingAutoOpenFolder: form.recordingAutoOpenFolder,
    recordingToolbarContentProtected: form.recordingToolbarContentProtected,
    recordingMaxDurationMinutes: form.recordingMaxDurationMinutes,
    recordingFileNameTemplate: form.recordingFileNameTemplate,
    aiProvider: form.aiProvider,
    apiUrl: form.apiUrl,
    modelName: form.modelName,
    apiKey: form.apiKey,
    customProviderName: form.customProviderName,
    selectionEnabled: form.selectionEnabled,
    translationPromptTemplate: form.translationPromptTemplate,
    explanationPromptTemplate: form.explanationPromptTemplate,
    imageFillVerifyMode: form.imageFillVerifyMode
  }
}

// 获取变化的字段
const getChangedFields = () => {
  if (!initialFormState.value) {
    return null
  }

  const changedFields = {}
  const initial = initialFormState.value

  // 检查每个字段是否有变化
  if (form.textMaxItems !== initial.textMaxItems) {
    changedFields.textMaxItems = form.textMaxItems
  }
  if (form.imageMaxItems !== initial.imageMaxItems) {
    changedFields.imageMaxItems = form.imageMaxItems
  }
  if (form.imageDiskLimitMb !== initial.imageDiskLimitMb) {
    changedFields.imageDiskLimitMb = form.imageDiskLimitMb
  }
  if (form.textClipboardEnabled !== initial.textClipboardEnabled) {
    changedFields.textClipboardEnabled = form.textClipboardEnabled
  }
  if (form.imageClipboardEnabled !== initial.imageClipboardEnabled) {
    changedFields.imageClipboardEnabled = form.imageClipboardEnabled
  }
  if (form.screenshotEnabled !== initial.screenshotEnabled) {
    changedFields.screenshotEnabled = form.screenshotEnabled
  }
  if (form.recordingEnabled !== initial.recordingEnabled) {
    changedFields.recordingEnabled = form.recordingEnabled
  }
  if (form.groupedItemsProtectedFromLimit !== initial.groupedItemsProtectedFromLimit) {
    changedFields.groupedItemsProtectedFromLimit = form.groupedItemsProtectedFromLimit
  }
  if (form.toggleShortcut !== initial.toggleShortcut) {
    changedFields.hotKey = form.toggleShortcut
  }
  if (form.imageToggleShortcut !== initial.imageToggleShortcut) {
    changedFields.imageHotKey = form.imageToggleShortcut
  }
  if (form.screenshotToggleShortcut !== initial.screenshotToggleShortcut) {
    changedFields.screenshotHotKey = form.screenshotToggleShortcut
  }
  if (form.recordingToggleShortcut !== initial.recordingToggleShortcut) {
    changedFields.recordingHotKey = form.recordingToggleShortcut
  }
  if (form.recordingDefaultFps !== initial.recordingDefaultFps) {
    changedFields.recordingDefaultFps = form.recordingDefaultFps
  }
  if (form.recordingDefaultVideoBitrateKbps !== initial.recordingDefaultVideoBitrateKbps) {
    changedFields.recordingDefaultVideoBitrateKbps = form.recordingDefaultVideoBitrateKbps
  }
  if (form.recordingDefaultAudioBitrateKbps !== initial.recordingDefaultAudioBitrateKbps) {
    changedFields.recordingDefaultAudioBitrateKbps = form.recordingDefaultAudioBitrateKbps
  }
  if (form.recordingCaptureCursor !== initial.recordingCaptureCursor) {
    changedFields.recordingCaptureCursor = form.recordingCaptureCursor
  }
  if (form.recordingCaptureSystemAudio !== initial.recordingCaptureSystemAudio) {
    changedFields.recordingCaptureSystemAudio = form.recordingCaptureSystemAudio
  }
  if (form.recordingCaptureMicrophone !== initial.recordingCaptureMicrophone) {
    changedFields.recordingCaptureMicrophone = form.recordingCaptureMicrophone
  }
  if (form.recordingMicrophoneDeviceId !== initial.recordingMicrophoneDeviceId) {
    changedFields.recordingMicrophoneDeviceId = form.recordingMicrophoneDeviceId
  }
  if (form.recordingOutputDir !== initial.recordingOutputDir) {
    changedFields.recordingOutputDir = form.recordingOutputDir
  }
  if (form.recordingAutoOpenFolder !== initial.recordingAutoOpenFolder) {
    changedFields.recordingAutoOpenFolder = form.recordingAutoOpenFolder
  }
  if (form.recordingToolbarContentProtected !== initial.recordingToolbarContentProtected) {
    changedFields.recordingToolbarContentProtected = form.recordingToolbarContentProtected
  }
  if (form.recordingMaxDurationMinutes !== initial.recordingMaxDurationMinutes) {
    changedFields.recordingMaxDurationMinutes = form.recordingMaxDurationMinutes
  }
  if (form.recordingFileNameTemplate !== initial.recordingFileNameTemplate) {
    changedFields.recordingFileNameTemplate = form.recordingFileNameTemplate
  }

  // 处理 AI 提供商
  let selectedProvider = form.aiProvider
  if (selectedProvider === 'custom') {
    if (!form.customProviderName) {
      return null
    }
    selectedProvider = form.customProviderName
  }
  if (selectedProvider !== initial.aiProvider && selectedProvider !== initial.customProviderName) {
    changedFields.aiProvider = selectedProvider
  }

  if (form.apiUrl !== initial.apiUrl) {
    changedFields.aiApiUrl = form.apiUrl
  }
  if (form.modelName !== initial.modelName) {
    changedFields.aiModelName = form.modelName
  }
  if (form.apiKey !== initial.apiKey) {
    changedFields.aiApiKey = form.apiKey
  }
  if (form.selectionEnabled !== initial.selectionEnabled) {
    changedFields.selectionEnabled = form.selectionEnabled
  }
  if (form.translationPromptTemplate !== initial.translationPromptTemplate) {
    changedFields.translationPromptTemplate = form.translationPromptTemplate
  }
  if (form.explanationPromptTemplate !== initial.explanationPromptTemplate) {
    changedFields.explanationPromptTemplate = form.explanationPromptTemplate
  }
  if (form.imageFillVerifyMode !== initial.imageFillVerifyMode) {
    changedFields.imageFillVerifyMode = form.imageFillVerifyMode
  }

  return Object.keys(changedFields).length > 0 ? changedFields : null
}

const persistSettings = async (silent = false) => {
  if (isInitializing.value || isAutoSaving.value) {
    return
  }

  // 获取变化的字段
  const changedFields = getChangedFields()

  // 如果没有变化，不执行保存
  if (!changedFields) {
    return
  }

  try {
    isAutoSaving.value = true
    if (autoSaveStateResetTimer) {
      clearTimeout(autoSaveStateResetTimer)
      autoSaveStateResetTimer = null
    }
    autoSaveState.value = 'saving'

    if (changedFields.recordingEnabled === true) {
      const ffmpegStatus = await RecordingService.checkFfmpeg()
      if (!ffmpegStatus?.exists) {
        // 依赖缺失时先回退开关状态，避免出现“已启用但不可用”的中间态
        suppressNextAutoSave.value = true
        form.recordingEnabled = false
        changedFields.recordingEnabled = false
        try {
          await ElMessageBox.confirm(
              `检测到首次启用录屏，未找到 ffmpeg.exe。\n将下载到：${ffmpegStatus.ffmpegPath}`,
              '需要下载 ffmpeg',
              {
                confirmButtonText: '确认下载',
                cancelButtonText: '取消',
                type: 'warning',
                closeOnClickModal: false,
                closeOnPressEscape: true
              }
          )
        } catch {
          autoSaveState.value = 'idle'
          ElMessage.info('已取消启用录屏')
          return
        }
        const downloading = ElLoading.service({
          lock: true,
          text: '正在下载 ffmpeg... 0%',
          background: 'rgba(0, 0, 0, 0.35)'
        })
        let unlistenProgress = null
        try {
          unlistenProgress = await listen('recording-ffmpeg-download-progress', (event) => {
            const payload = event?.payload || {}
            const percent = typeof payload.progressPercent === 'number' ? payload.progressPercent : null
            if (percent !== null) {
              downloading.setText(`正在下载 ffmpeg... ${percent}%`)
              return
            }
            const downloaded = Number(payload.downloadedBytes || 0)
            const total = Number(payload.totalBytes || 0)
            if (total > 0) {
              const computed = Math.min(100, Math.floor(downloaded * 100 / total))
              downloading.setText(`正在下载 ffmpeg... ${computed}%`)
            }
          })
          await RecordingService.downloadFfmpeg(ffmpegStatus.downloadUrl || null)
        } finally {
          if (unlistenProgress) {
            unlistenProgress()
          }
          downloading.close()
        }
        suppressNextAutoSave.value = true
        form.recordingEnabled = true
        changedFields.recordingEnabled = true
        ElMessage.success('ffmpeg 下载完成，已启用录屏')
      }
    }

    // 只保存变化的字段
    await AISettingsService.savePartialSettings(changedFields)

    // 处理自定义提供商
    if (changedFields.aiProvider) {
      let selectedProvider = form.aiProvider
      if (selectedProvider === 'custom') {
        selectedProvider = form.customProviderName
      }
      if (form.aiProvider === 'custom' && form.customProviderName === selectedProvider) {
        if (aiSettingsRef.value) {
          suppressNextAutoSave.value = true
          await aiSettingsRef.value.loadAiProviders()
        }
        form.aiProvider = selectedProvider
      }
    }
    
    shortcutConflictMessage.value = ''
    if (!silent) {
      ElMessage.success('已自动保存')
    }
    autoSaveState.value = 'saved'

    // 保存成功后更新初始状态
    saveInitialFormState()
    
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

const handleNavigateSettings = (payload) => {
  const tab = typeof payload?.tab === 'string' ? payload.tab : ''
  if (tab && sections.value.some((section) => section.key === tab)) {
    activeTab.value = tab
  }
  if (payload?.reason === 'selection_ai_not_configured') {
    ElMessage.warning('划词翻译/解释需要先配置 AI。请先在当前页面完成提供商、地址、模型和 API 密钥配置。')
  }
}

onMounted(async () => {
  unlistenShortcutConflict = await listen('shortcut-conflict-warning', (event) => {
    showShortcutConflictWarning(event.payload)
  })
  unlistenNavigateSettings = await listen('navigate-settings-tab', (event) => {
    handleNavigateSettings(event.payload)
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
    form.textClipboardEnabled = settings.text_clipboard_enabled === true
    form.imageClipboardEnabled = settings.image_clipboard_enabled === true
    form.screenshotEnabled = settings.screenshot_enabled === true
    form.recordingEnabled = settings.recording_enabled === true
    currentVersion.value = settings.version || '0.3.1'
    form.toggleShortcut = settings.hot_key || ''
    form.imageToggleShortcut = settings.image_hot_key || ''
    form.screenshotToggleShortcut = settings.screenshot_hot_key || ''
    form.recordingToggleShortcut = settings.recording_hot_key || 'Alt+R'
    form.recordingDefaultFps = Number(settings.recording_default_fps || 30)
    form.recordingDefaultVideoBitrateKbps = Number(settings.recording_default_video_bitrate_kbps || 6000)
    form.recordingDefaultAudioBitrateKbps = Number(settings.recording_default_audio_bitrate_kbps || 160)
    form.recordingCaptureCursor = settings.recording_capture_cursor !== false
    form.recordingCaptureSystemAudio = settings.recording_capture_system_audio === true
    form.recordingCaptureMicrophone = settings.recording_capture_microphone === true
    form.recordingMicrophoneDeviceId = settings.recording_microphone_device_id || ''
    form.recordingOutputDir = settings.recording_output_dir || ''
    form.recordingAutoOpenFolder = settings.recording_auto_open_folder !== false
    form.recordingToolbarContentProtected = settings.recording_toolbar_content_protected === true
    form.recordingMaxDurationMinutes = Number(settings.recording_max_duration_minutes || 180)
    form.recordingFileNameTemplate = settings.recording_file_name_template || '{timestamp}'
    form.selectionEnabled = settings.selection_enabled === true
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
    // 先保存初始状态，再设置 isInitializing 为 false
    saveInitialFormState()
    // 设置跳过下一次 watch 触发的标志
    skipNextWatch.value = true
    isInitializing.value = false
  }
})

watch(form, () => {
  if (isInitializing.value) return
  if (skipNextWatch.value) {
    skipNextWatch.value = false
    return
  }
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
  if (unlistenNavigateSettings) {
    unlistenNavigateSettings()
    unlistenNavigateSettings = null
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
  max-width: 980px;
  margin: 0 auto;
}

.header {
  position: relative;
  display: flex;
  align-items: flex-start;
  margin-bottom: 16px;
  min-height: 40px;
  padding-right: 220px;
}

.header-title h1 {
  margin: 0;
  font-size: 24px;
  line-height: 1.2;
}

.header-title p {
  margin: 6px 0 0;
  color: #909399;
  font-size: 13px;
}

.header-actions {
  position: absolute;
  top: 0;
  right: 0;
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

.settings-layout {
  display: flex;
  gap: 16px;
}

.settings-nav {
  width: 180px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.section-nav-item {
  width: 100%;
  border: 1px solid var(--el-border-color);
  border-radius: 8px;
  background: #fff;
  color: #606266;
  padding: 10px 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  transition: all 0.2s ease;
  font-size: 14px;
}

.section-nav-item:hover {
  border-color: var(--el-color-primary-light-5);
  color: var(--el-color-primary);
}

.section-nav-item.active {
  border-color: var(--el-color-primary);
  color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}

.content {
  flex: 1;
  background: #fff;
  padding: 20px;
  border-radius: 8px;
  box-shadow: 0 2px 12px 0 rgba(0, 0, 0, 0.1);
}

.content-header {
  margin-bottom: 18px;
}

.content-header h2 {
  margin: 0;
  font-size: 20px;
}

.content-header p {
  margin: 6px 0 0;
  color: #909399;
  font-size: 13px;
}

.dark .content {
  background: #1d1e1f;
  box-shadow: 0 2px 12px 0 rgba(0, 0, 0, 0.3);
}

.dark .section-nav-item {
  background: #1d1e1f;
  border-color: #4c4d4f;
  color: #cfd3dc;
}

.dark .section-nav-item.active {
  border-color: var(--el-color-primary);
  background: rgba(64, 158, 255, 0.15);
}

.footer-links {
  margin-top: 40px;
  text-align: center;
  color: #909399;
  font-size: 14px;
}

@media (max-width: 900px) {
  .settings-nav {
    width: 140px;
  }

  .section-nav-item {
    padding: 8px 10px;
  }
}
</style>
