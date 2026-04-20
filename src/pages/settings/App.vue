<template>
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
          <div v-if="activeTab === 'clipboard'">
            <ClipboardSettings :form="form"/>
          </div>
          <div v-else-if="activeTab === 'screenshot'">
            <ScreenshotSettings :form="form"/>
          </div>
          <div v-else-if="activeTab === 'recording'">
            <RecordingSettings :form="form"/>
          </div>

          <div v-else-if="activeTab === 'selection'">
            <SelectionSettings :form="form"/>
          </div>

          <div v-else-if="activeTab === 'ai'">
            <AISettings ref="aiSettingsRef" :form="form"/>
          </div>
          <div v-else-if="activeTab === 'backup'">
            <BackupSettings/>
          </div>
          <div v-else-if="activeTab === 'diagnostic'">
            <DiagnosticSettings @navigate="handleNavigateTab"/>
          </div>

          <div v-else-if="isDevMode && activeTab === 'developer'">
            <DeveloperSettings/>
          </div>
          <div v-else-if="activeTab === 'about'">
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
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref, watch} from 'vue'
import {provideGlobalConfig} from 'element-plus'
import zhCn from 'element-plus/dist/locale/zh-cn'
import {
  Camera,
  Cpu,
  DocumentCopy,
  FolderOpened,
  InfoFilled,
  Moon,
  Setting,
  Sunny,
  VideoCamera,
  WarningFilled
} from '@element-plus/icons-vue'
import {openUrl} from '@tauri-apps/plugin-opener'
import {listen} from '@tauri-apps/api/event'
import {AISettingsService, RecordingService} from '../../services/ipc'
import ClipboardSettings from './components/ClipboardSettings.vue'
import ScreenshotSettings from './components/ScreenshotSettings.vue'
import RecordingSettings from './components/RecordingSettings.vue'
import SelectionSettings from './components/SelectionSettings.vue'
import AISettings from './components/AISettings.vue'
import BackupSettings from './components/BackupSettings.vue'
import DiagnosticSettings from './components/DiagnosticSettings.vue'
import AboutSettings from './components/AboutSettings.vue'
import DeveloperSettings from '@dev/DeveloperSettings'

provideGlobalConfig({locale: zhCn})

const activeTab = ref('clipboard')
const isDark = ref(false)
const currentVersion = ref('0.0.0')
const aiSettingsRef = ref(null)
const shortcutConflictMessage = ref('')
let unlistenShortcutConflict = null
let unlistenNavigateSettings = null
let saveTimer = null
let autoSaveStateResetTimer = null
let pendingPersistSnapshot = null
let pendingPersistVersion = 0
let formMutationVersion = 0
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
    },
    {
      key: 'backup',
      label: '数据备份',
      description: '导出备份包、恢复预览与自动备份配置',
      icon: FolderOpened
    },
    {
      key: 'diagnostic',
      label: '诊断与修复',
      description: '查看核心健康状态并执行诊断动作',
      icon: WarningFilled
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
  recordingToggleShortcut: 'Alt+R',
  recordingMicToggleShortcut: 'Ctrl+Space',
  recordingDefaultFps: 30,
  recordingDefaultVideoBitrateKbps: 6000,
  recordingDefaultAudioBitrateKbps: 160,
  recordingCaptureCursor: true,
  recordingCaptureSystemAudio: false,
  recordingCaptureMicrophone: true,
  recordingMicrophoneDeviceId: '',
  recordingOutputDir: '',
  recordingAutoOpenFolder: false,
  recordingToolbarContentProtected: false,
  recordingMaxDurationMinutes: 180,
  recordingFileNameTemplate: '{timestamp}',
  recordingWindowAudioSyncAdvanceMs: 80,
  devForceFfmpegWindowCapture: false,
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

const handleNavigateTab = (tab) => {
  if (typeof tab === 'string' && sections.value.some((section) => section.key === tab)) {
    activeTab.value = tab
  }
}

const buildFormSnapshot = () => ({
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
    recordingMicToggleShortcut: form.recordingMicToggleShortcut,
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
    recordingWindowAudioSyncAdvanceMs: form.recordingWindowAudioSyncAdvanceMs,
  devForceFfmpegWindowCapture: form.devForceFfmpegWindowCapture,
    aiProvider: form.aiProvider,
    apiUrl: form.apiUrl,
    modelName: form.modelName,
    apiKey: form.apiKey,
    customProviderName: form.customProviderName,
    selectionEnabled: form.selectionEnabled,
    translationPromptTemplate: form.translationPromptTemplate,
    explanationPromptTemplate: form.explanationPromptTemplate,
    imageFillVerifyMode: form.imageFillVerifyMode
})

// 保存初始状态快照
const saveInitialFormState = (snapshot = buildFormSnapshot()) => {
  initialFormState.value = {
    ...snapshot
  }
}

// 获取变化的字段
const getChangedFields = (snapshot = buildFormSnapshot()) => {
  if (!initialFormState.value) {
    return null
  }

  const changedFields = {}
  const initial = initialFormState.value
  const source = snapshot

  
  if (source.textMaxItems !== initial.textMaxItems) {
    changedFields.textMaxItems = source.textMaxItems
  }
  if (source.imageMaxItems !== initial.imageMaxItems) {
    changedFields.imageMaxItems = source.imageMaxItems
  }
  if (source.imageDiskLimitMb !== initial.imageDiskLimitMb) {
    changedFields.imageDiskLimitMb = source.imageDiskLimitMb
  }
  if (source.textClipboardEnabled !== initial.textClipboardEnabled) {
    changedFields.textClipboardEnabled = source.textClipboardEnabled
  }
  if (source.imageClipboardEnabled !== initial.imageClipboardEnabled) {
    changedFields.imageClipboardEnabled = source.imageClipboardEnabled
  }
  if (source.screenshotEnabled !== initial.screenshotEnabled) {
    changedFields.screenshotEnabled = source.screenshotEnabled
  }
  if (source.recordingEnabled !== initial.recordingEnabled) {
    changedFields.recordingEnabled = source.recordingEnabled
  }
  if (source.groupedItemsProtectedFromLimit !== initial.groupedItemsProtectedFromLimit) {
    changedFields.groupedItemsProtectedFromLimit = source.groupedItemsProtectedFromLimit
  }
  if (source.toggleShortcut !== initial.toggleShortcut) {
    changedFields.hotKey = source.toggleShortcut
  }
  if (source.imageToggleShortcut !== initial.imageToggleShortcut) {
    changedFields.imageHotKey = source.imageToggleShortcut
  }
  if (source.screenshotToggleShortcut !== initial.screenshotToggleShortcut) {
    changedFields.screenshotHotKey = source.screenshotToggleShortcut
  }
  if (source.recordingToggleShortcut !== initial.recordingToggleShortcut) {
    changedFields.recordingHotKey = source.recordingToggleShortcut
  }
  if (source.recordingMicToggleShortcut !== initial.recordingMicToggleShortcut) {
    changedFields.recordingMicToggleHotKey = source.recordingMicToggleShortcut
  }
  if (source.recordingDefaultFps !== initial.recordingDefaultFps) {
    changedFields.recordingDefaultFps = source.recordingDefaultFps
  }
  if (source.recordingDefaultVideoBitrateKbps !== initial.recordingDefaultVideoBitrateKbps) {
    changedFields.recordingDefaultVideoBitrateKbps = source.recordingDefaultVideoBitrateKbps
  }
  if (source.recordingDefaultAudioBitrateKbps !== initial.recordingDefaultAudioBitrateKbps) {
    changedFields.recordingDefaultAudioBitrateKbps = source.recordingDefaultAudioBitrateKbps
  }
  if (source.recordingCaptureCursor !== initial.recordingCaptureCursor) {
    changedFields.recordingCaptureCursor = source.recordingCaptureCursor
  }
  if (source.recordingCaptureSystemAudio !== initial.recordingCaptureSystemAudio) {
    changedFields.recordingCaptureSystemAudio = source.recordingCaptureSystemAudio
  }
  if (source.recordingCaptureMicrophone !== initial.recordingCaptureMicrophone) {
    changedFields.recordingCaptureMicrophone = source.recordingCaptureMicrophone
  }
  if (source.recordingMicrophoneDeviceId !== initial.recordingMicrophoneDeviceId) {
    changedFields.recordingMicrophoneDeviceId = source.recordingMicrophoneDeviceId
  }
  if (source.recordingOutputDir !== initial.recordingOutputDir) {
    changedFields.recordingOutputDir = source.recordingOutputDir
  }
  if (source.recordingAutoOpenFolder !== initial.recordingAutoOpenFolder) {
    changedFields.recordingAutoOpenFolder = source.recordingAutoOpenFolder
  }
  if (source.recordingToolbarContentProtected !== initial.recordingToolbarContentProtected) {
    changedFields.recordingToolbarContentProtected = source.recordingToolbarContentProtected
  }
  if (source.recordingMaxDurationMinutes !== initial.recordingMaxDurationMinutes) {
    changedFields.recordingMaxDurationMinutes = source.recordingMaxDurationMinutes
  }
  if (source.recordingFileNameTemplate !== initial.recordingFileNameTemplate) {
    changedFields.recordingFileNameTemplate = source.recordingFileNameTemplate
  }
  if (source.recordingWindowAudioSyncAdvanceMs !== initial.recordingWindowAudioSyncAdvanceMs) {
    changedFields.recordingWindowAudioSyncAdvanceMs = source.recordingWindowAudioSyncAdvanceMs
  }
  if (source.devForceFfmpegWindowCapture !== initial.devForceFfmpegWindowCapture) {
    changedFields.devForceFfmpegWindowCapture = source.devForceFfmpegWindowCapture
  }

  
  let selectedProvider = source.aiProvider
  if (selectedProvider === 'custom') {
    if (!source.customProviderName) {
      return null
    }
    selectedProvider = source.customProviderName
  }
  if (selectedProvider !== initial.aiProvider && selectedProvider !== initial.customProviderName) {
    changedFields.aiProvider = selectedProvider
  }

  if (source.apiUrl !== initial.apiUrl) {
    changedFields.aiApiUrl = source.apiUrl
  }
  if (source.modelName !== initial.modelName) {
    changedFields.aiModelName = source.modelName
  }
  if (source.apiKey !== initial.apiKey) {
    changedFields.aiApiKey = source.apiKey
  }
  if (source.selectionEnabled !== initial.selectionEnabled) {
    changedFields.selectionEnabled = source.selectionEnabled
  }
  if (source.translationPromptTemplate !== initial.translationPromptTemplate) {
    changedFields.translationPromptTemplate = source.translationPromptTemplate
  }
  if (source.explanationPromptTemplate !== initial.explanationPromptTemplate) {
    changedFields.explanationPromptTemplate = source.explanationPromptTemplate
  }
  if (source.imageFillVerifyMode !== initial.imageFillVerifyMode) {
    changedFields.imageFillVerifyMode = source.imageFillVerifyMode
  }

  return Object.keys(changedFields).length > 0 ? changedFields : null
}

const queuePersistSettings = (silent = true, delay = 450) => {
  pendingPersistSnapshot = buildFormSnapshot()
  pendingPersistVersion = ++formMutationVersion
  if (saveTimer) {
    clearTimeout(saveTimer)
  }
  saveTimer = window.setTimeout(() => {
    const snapshot = pendingPersistSnapshot
    const version = pendingPersistVersion
    pendingPersistSnapshot = null
    pendingPersistVersion = 0
    saveTimer = null
    void persistSettings(silent, snapshot, version)
  }, delay)
}

const persistSettings = async (
    silent = false,
    snapshot = buildFormSnapshot(),
    persistVersion = ++formMutationVersion
) => {
  if (isInitializing.value) {
    return
  }
  if (isAutoSaving.value) {
    pendingPersistSnapshot = snapshot
    pendingPersistVersion = Math.max(pendingPersistVersion, persistVersion)
    return
  }

  
  const changedFields = getChangedFields(snapshot)

  
  if (!changedFields) {
    if (!pendingPersistSnapshot && getChangedFields() === null) {
      autoSaveState.value = 'idle'
    }
    return
  }

  try {
    isAutoSaving.value = true
    if (autoSaveStateResetTimer) {
      clearTimeout(autoSaveStateResetTimer)
      autoSaveStateResetTimer = null
    }
    autoSaveState.value = 'saving'

    if (changedFields.screenshotEnabled === true) {
      const runtimeStatus = await AISettingsService.checkVcRuntimeDependencies()
      const missing = Array.isArray(runtimeStatus?.missing) ? runtimeStatus.missing : []
      if (missing.length > 0) {
        
        suppressNextAutoSave.value = true
        form.screenshotEnabled = false
        snapshot.screenshotEnabled = false
        changedFields.screenshotEnabled = false
        await showVcRuntimeMissingWarning(runtimeStatus)
      }
    }

    if (changedFields.recordingEnabled === true) {
      const ffmpegStatus = await RecordingService.checkFfmpeg()
      if (!ffmpegStatus?.exists) {
        
        suppressNextAutoSave.value = true
        form.recordingEnabled = false
        snapshot.recordingEnabled = false
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
        snapshot.recordingEnabled = true
        changedFields.recordingEnabled = true
        ElMessage.success('ffmpeg 下载完成，已启用录屏')
      }
    }

    
    await AISettingsService.savePartialSettings(changedFields)

    
    if (changedFields.aiProvider) {
      let selectedProvider = snapshot.aiProvider
      if (selectedProvider === 'custom') {
        selectedProvider = snapshot.customProviderName
      }
      if (snapshot.aiProvider === 'custom' && snapshot.customProviderName === selectedProvider) {
        if (aiSettingsRef.value) {
          suppressNextAutoSave.value = true
          await aiSettingsRef.value.loadAiProviders()
        }
        form.aiProvider = selectedProvider
        snapshot.aiProvider = selectedProvider
      }
    }

    shortcutConflictMessage.value = ''
    if (!silent) {
      ElMessage.success('已自动保存')
    }
    autoSaveState.value = 'saved'

    
    saveInitialFormState(snapshot)

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
    if (pendingPersistSnapshot && pendingPersistVersion > persistVersion) {
      const retrySnapshot = pendingPersistSnapshot
      const retryVersion = pendingPersistVersion
      pendingPersistSnapshot = null
      pendingPersistVersion = 0
      window.setTimeout(() => {
        void persistSettings(true, retrySnapshot, retryVersion)
      }, 0)
    }
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

const normalizeVcRuntimeMissing = (payload) => {
  const missing = payload && Array.isArray(payload.missing)
      ? payload.missing.filter((item) => typeof item === 'string' && item.trim())
      : []
  const installUrl = typeof payload?.installUrl === 'string' && payload.installUrl.trim()
      ? payload.installUrl.trim()
      : 'https://aka.ms/vs/17/release/vc_redist.x64.exe'
  return {missing, installUrl}
}

const showShortcutConflictWarning = (payload) => {
  const conflicts = normalizeShortcutConflicts(payload)
  if (conflicts.length === 0) return
  activeTab.value = 'clipboard'
  shortcutConflictMessage.value = `快捷键被占用：${conflicts.join('；')}`
}

const showVcRuntimeMissingWarning = async (payload) => {
  const {missing, installUrl} = normalizeVcRuntimeMissing(payload)
  if (missing.length === 0) return
  const detail = missing.join('、')
  try {
    await ElMessageBox.confirm(
        `检测到系统缺少 VC 运行库组件：${detail}。\n安装后可避免录屏/长截图相关功能在部分机器上异常。\n是否现在下载安装 Microsoft VC++ Redistributable (x64)？`,
        '检测到依赖缺失',
        {
          confirmButtonText: '下载安装',
          cancelButtonText: '稍后处理',
          type: 'warning',
          closeOnClickModal: false,
          closeOnPressEscape: true
        }
    )
    const downloading = ElLoading.service({
      lock: true,
      text: '正在下载 VC Runtime... 0%',
      background: 'rgba(0, 0, 0, 0.35)'
    })
    let unlistenProgress = null
    let downloaded = null
    try {
      unlistenProgress = await listen('vc-runtime-download-progress', (event) => {
        const progress = event?.payload || {}
        const percent = typeof progress.progressPercent === 'number' ? progress.progressPercent : null
        if (percent !== null) {
          downloading.setText(`正在下载 VC Runtime... ${percent}%`)
          return
        }
        const done = Number(progress.downloadedBytes || 0)
        const total = Number(progress.totalBytes || 0)
        if (total > 0) {
          const computed = Math.min(100, Math.floor(done * 100 / total))
          downloading.setText(`正在下载 VC Runtime... ${computed}%`)
        }
      })
      downloaded = await AISettingsService.downloadVcRuntimeInstaller(installUrl)
    } finally {
      if (unlistenProgress) {
        unlistenProgress()
      }
      downloading.close()
    }
    const installerPath = typeof downloaded?.installerPath === 'string' ? downloaded.installerPath : ''
    if (!installerPath) {
      ElMessage.error('VC Runtime 安装包下载成功，但未获取安装文件路径')
      return
    }
    const installing = ElLoading.service({
      lock: true,
      text: '正在安装 VC Runtime，请按安装向导完成...',
      background: 'rgba(0, 0, 0, 0.35)'
    })
    let installResult = null
    try {
      installResult = await AISettingsService.installVcRuntimeAndWait(installerPath)
    } finally {
      installing.close()
    }
    const cancelled = installResult?.cancelled === true
    const success = installResult?.success === true
    const rebootRequired = installResult?.rebootRequired === true
    const exitCode = Number.isInteger(installResult?.exitCode) ? installResult.exitCode : null
    if (cancelled) {
      ElMessage.warning('已取消安装，请安装 VC Runtime 后重新启用截图功能')
      return
    }
    if (!success) {
      ElMessage.error(`VC Runtime 安装失败（exitCode=${exitCode ?? 'unknown'}），请手动安装后重新启用截图`)
      return
    }
    if (rebootRequired) {
      ElMessage.warning('VC Runtime 安装完成，系统提示需要重启。请重启后重新启用截图功能')
      return
    }
    ElMessage.success('VC Runtime 安装完成，请重新启用截图功能')
  } catch {
    
  }
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
    form.recordingMicToggleShortcut = settings.recording_mic_toggle_hot_key || 'Ctrl+Space'
    form.recordingDefaultFps = Number(settings.recording_default_fps || 30)
    form.recordingDefaultVideoBitrateKbps = Number(settings.recording_default_video_bitrate_kbps || 6000)
    form.recordingDefaultAudioBitrateKbps = Number(settings.recording_default_audio_bitrate_kbps || 160)
    form.recordingCaptureCursor = settings.recording_capture_cursor !== false
    form.recordingCaptureSystemAudio = settings.recording_capture_system_audio === true
    form.recordingCaptureMicrophone = settings.recording_capture_microphone === true
    form.recordingMicrophoneDeviceId = settings.recording_microphone_device_id || ''
    form.recordingOutputDir = settings.recording_output_dir || ''
    form.recordingAutoOpenFolder = settings.recording_auto_open_folder === true
    form.recordingToolbarContentProtected = settings.recording_toolbar_content_protected === true
    form.recordingMaxDurationMinutes = Number(settings.recording_max_duration_minutes || 180)
    form.recordingFileNameTemplate = settings.recording_file_name_template || '{timestamp}'
    form.recordingWindowAudioSyncAdvanceMs = Number(settings.recording_window_audio_sync_advance_ms ?? 80)
    form.devForceFfmpegWindowCapture = settings.dev_force_ffmpeg_window_capture === true
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
    
    saveInitialFormState()
    
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
  }
  queuePersistSettings(true, 450)
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
html,
body,
#app {
  height: 100%;
}

body {
  margin: 0;
  font-family: 'Helvetica Neue', Helvetica, 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', '微软雅黑', Arial, sans-serif;
  overflow: hidden;
}

.settings-container {
  box-sizing: border-box;
  height: 100vh;
  padding: 20px;
  max-width: 1080px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.header {
  position: relative;
  display: flex;
  align-items: flex-start;
  margin-bottom: 16px;
  min-height: 40px;
  padding-right: 220px;
  flex-shrink: 0;
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
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.settings-nav {
  width: 180px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow: hidden;
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
  min-height: 0;
  background: linear-gradient(180deg, #ffffff 0%, #fbfcff 100%);
  padding: 22px;
  border-radius: 12px;
  border: 1px solid rgba(15, 23, 42, 0.08);
  box-shadow: 0 12px 28px rgba(15, 23, 42, 0.08);
  overflow-y: auto;
  overscroll-behavior: contain;
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
  background: linear-gradient(180deg, #1d1e1f 0%, #18191a 100%);
  border-color: rgba(255, 255, 255, 0.12);
  box-shadow: 0 10px 26px rgba(0, 0, 0, 0.42);
}

.content .setting-section-card {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 12px;
  box-shadow: none;
}

.content .setting-section-card + .setting-section-card {
  margin-top: 16px;
}

.content .setting-section-card .el-card__header {
  padding: 12px 16px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.content .setting-section-card .el-card__body {
  padding: 14px 16px 12px;
}

.content .section-title {
  font-size: 15px;
  font-weight: 700;
  letter-spacing: 0.2px;
}

.content .el-form-item {
  margin-bottom: 14px;
}

.content .el-form-item__label {
  font-weight: 600;
  padding-bottom: 6px;
}

.content .el-input,
.content .el-select,
.content .el-input-number {
  width: 100%;
}

.content .el-switch + .form-hint,
.content .el-input + .form-hint,
.content .el-select + .form-hint,
.content .el-input-number + .form-hint {
  margin-top: 6px;
}

.dark .content .setting-section-card {
  border-color: rgba(255, 255, 255, 0.12);
  background: rgba(255, 255, 255, 0.02);
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
  margin-top: 14px;
  text-align: center;
  color: #909399;
  font-size: 14px;
  flex-shrink: 0;
}

@media (max-width: 900px) {
  body {
    overflow: auto;
  }

  .settings-container {
    height: auto;
    min-height: 100vh;
    overflow: visible;
  }

  .settings-layout {
    flex-direction: column;
    overflow: visible;
  }

  .settings-nav {
    width: 100%;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 8px;
  }

  .section-nav-item {
    padding: 8px 10px;
  }

  .header {
    padding-right: 0;
  }

  .header-actions {
    position: static;
    margin-left: auto;
  }

  .content {
    overflow: visible;
  }
}
</style>
