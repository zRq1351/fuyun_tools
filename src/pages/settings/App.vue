<template>
  <el-config-provider :locale="elLocale">
  <div class="settings-container">
      <div class="header">
        <div class="header-title">
          <h1>{{ $t('settings.title') }}</h1>
        </div>
        <div class="header-actions">
          <span :class="['autosave-status', `autosave-${autoSaveState}`]">{{ autoSaveText }}</span>
          <el-dropdown trigger="click" @command="changeLocale">
            <el-button>
              <template #icon>
                <el-icon>
                  <icon-menu/>
                </el-icon>
              </template>
              {{ currentLocale === 'zh-CN' ? '中文' : 'EN' }}
            </el-button>
            <template #dropdown>
              <el-dropdown-menu>
                <el-dropdown-item command="zh-CN">
                  <span>中文</span>
                </el-dropdown-item>
                <el-dropdown-item command="en-US">
                  <span>English</span>
                </el-dropdown-item>
              </el-dropdown-menu>
            </template>
          </el-dropdown>
          <el-dropdown trigger="click" @command="changeTheme">
            <el-button>
              <template #icon>
                <component :is="themeIcon"/>
              </template>
              {{ themeLabel }}
            </el-button>
            <template #dropdown>
              <el-dropdown-menu>
                <el-dropdown-item command="dark">
                  <el-icon>
                    <Moon/>
                  </el-icon>
                  {{ $t('theme.dark') }}
                </el-dropdown-item>
                <el-dropdown-item command="light">
                  <el-icon>
                    <Sunny/>
                  </el-icon>
                  {{ $t('theme.light') }}
                </el-dropdown-item>
                <el-dropdown-item command="eye-care">
                  <el-icon>
                    <View/>
                  </el-icon>
                  {{ $t('theme.eyeCare') }}
                </el-dropdown-item>
              </el-dropdown-menu>
            </template>
          </el-dropdown>
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
            <ClipboardSettings :form="form" :on-feature-toggle="handleFeatureToggle"/>
          </div>
          <div v-else-if="activeTab === 'screenshot'">
            <ScreenshotSettings :form="form" :on-feature-toggle="handleFeatureToggle"/>
          </div>
          <div v-else-if="activeTab === 'recording'">
            <RecordingSettings :form="form" :on-feature-toggle="handleFeatureToggle"/>
          </div>

          <div v-else-if="activeTab === 'selection'">
            <SelectionSettings :form="form" :on-feature-toggle="handleFeatureToggle"/>
          </div>

          <div v-else-if="activeTab === 'launcher'">
            <LauncherSettings :form="form" :on-feature-toggle="handleFeatureToggle"/>
          </div>

          <div v-else-if="activeTab === 'doc_manager'">
            <DocumentManagerSettings :form="form" :on-feature-toggle="handleFeatureToggle"/>
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
          {{ $t('settings.needHelp') }}
          <el-link type="primary" @click="openExternal('https://github.com/zRq1351/fuyun_tools')">
            {{ $t('settings.viewDocs') }}
          </el-link>
          |
          <el-link type="primary" @click="openExternal('https://github.com/zRq1351/fuyun_tools/issues')">
            {{ $t('settings.reportIssue') }}
          </el-link>
        </p>
        <p>{{ $t('settings.version') }} {{ currentVersion }} | &copy; {{ new Date().getFullYear() }} fuyun_tools</p>
      </div>
  </div>
  </el-config-provider>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref, watch} from 'vue'
import {ElLoading, ElMessage, ElMessageBox} from 'element-plus'
import {useI18n} from 'vue-i18n'
import {
  Camera,
  Cpu,
  DocumentCopy,
  Folder,
  FolderOpened,
  InfoFilled,
  Menu as IconMenu,
  Moon,
  Operation,
  Setting,
  Sunny,
  VideoCamera,
  View,
  WarningFilled
} from '@element-plus/icons-vue'
import {useTheme} from '@/composables/useTheme.js'
import {useLocale} from '@/composables/useLocale.js'
import {openUrl} from '@tauri-apps/plugin-opener'
import {listen} from '@tauri-apps/api/event'
import {AISettingsService, RecordingService} from '@/services/ipc.js'
import ClipboardSettings from './components/ClipboardSettings.vue'
import ScreenshotSettings from './components/ScreenshotSettings.vue'
import RecordingSettings from './components/RecordingSettings.vue'
import SelectionSettings from './components/SelectionSettings.vue'
import LauncherSettings from './components/LauncherSettings.vue'
import DocumentManagerSettings from './components/DocumentManagerSettings.vue'
import AISettings from './components/AISettings.vue'
import BackupSettings from './components/BackupSettings.vue'
import DiagnosticSettings from './components/DiagnosticSettings.vue'
import AboutSettings from './components/AboutSettings.vue'
import DeveloperSettings from '@dev/DeveloperSettings'

const {t} = useI18n()

// 语言管理
const {currentLocale, elLocale, changeLocale} = useLocale()

// 主题管理
const {currentTheme, isDark, changeTheme} = useTheme()

const themeIcon = computed(() => {
  switch (currentTheme.value) {
    case 'dark':
      return Moon
    case 'light':
      return Sunny
    case 'eye-care':
      return View
    default:
      return Moon
  }
})

const themeLabel = computed(() => {
  const labels = {
    dark: t('theme.dark'),
    light: t('theme.light'),
    'eye-care': t('theme.eyeCare')
  }
  return labels[currentTheme.value] || t('theme.dark')
})

const activeTab = ref('clipboard')
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
      label: t('settings.nav.clipboard'),
      description: t('settings.nav.clipboardDesc'),
      icon: DocumentCopy
    },
    {
      key: 'screenshot',
      label: t('settings.nav.screenshot'),
      description: t('settings.nav.screenshotDesc'),
      icon: Camera
    },
    {
      key: 'recording',
      label: t('settings.nav.recording'),
      description: t('settings.nav.recordingDesc'),
      icon: VideoCamera
    },
    {
      key: 'selection',
      label: t('settings.nav.selection'),
      description: t('settings.nav.selectionDesc'),
      icon: Setting
    },
    {
      key: 'launcher',
      label: t('settings.nav.launcher'),
      description: t('settings.nav.launcherDesc'),
      icon: Operation
    },
    {
      key: 'doc_manager',
      label: t('settings.nav.docManager'),
      description: t('settings.nav.docManagerDesc'),
      icon: Folder
    },
    {
      key: 'ai',
      label: t('settings.nav.ai'),
      description: t('settings.nav.aiDesc'),
      icon: Cpu
    },
    {
      key: 'backup',
      label: t('settings.nav.backup'),
      description: t('settings.nav.backupDesc'),
      icon: FolderOpened
    },
    {
      key: 'diagnostic',
      label: t('settings.nav.diagnostic'),
      description: t('settings.nav.diagnosticDesc'),
      icon: WarningFilled
    }
  ]
  if (isDevMode) {
    baseSections.push({
      key: 'developer',
      label: t('settings.nav.developer'),
      description: t('settings.nav.developerDesc'),
      icon: Setting
    })
  }
  baseSections.push({
    key: 'about',
    label: t('settings.nav.about'),
    description: t('settings.nav.aboutDesc'),
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
  selectionModifierKey: '',
  selectionCustomPrompts: [],
  selectionWebSearchEnabled: true,
  selectionWebSearchEngine: 'bing',
  translationPromptTemplate: '',
  explanationPromptTemplate: '',
  imageFillVerifyMode: 'fast',
  ocrEngine: 'ocr-rs',
  launcherEnabled: true,
  launcherHotKey: 'Alt+Q',
  docManagerEnabled: false,
  docManagerHotKey: 'Ctrl+Shift+D'
})

const autoSaveText = computed(() => {
  const states = {
    idle: t('settings.autoSave.idle'),
    pending: t('settings.autoSave.pending'),
    saving: t('settings.autoSave.saving'),
    saved: t('settings.autoSave.saved'),
    error: t('settings.autoSave.error')
  }
  return states[autoSaveState.value] || t('settings.autoSave.idle')
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
    selectionModifierKey: form.selectionModifierKey,
    selectionCustomPrompts: form.selectionCustomPrompts,
    selectionWebSearchEnabled: form.selectionWebSearchEnabled,
    selectionWebSearchEngine: form.selectionWebSearchEngine,
    translationPromptTemplate: form.translationPromptTemplate,
    explanationPromptTemplate: form.explanationPromptTemplate,
  imageFillVerifyMode: form.imageFillVerifyMode,
  ocrEngine: form.ocrEngine,
  launcherEnabled: form.launcherEnabled,
  launcherHotKey: form.launcherHotKey,
  docManagerEnabled: form.docManagerEnabled,
  docManagerHotKey: form.docManagerHotKey
})

// 保存初始状态快照
const saveInitialFormState = (snapshot = buildFormSnapshot()) => {
  // 使用深拷贝，避免响应式对象的引用问题
  initialFormState.value = JSON.parse(JSON.stringify(snapshot))
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
  if (source.selectionModifierKey !== initial.selectionModifierKey) {
    changedFields.selectionModifierKey = source.selectionModifierKey
  }
  if (JSON.stringify(source.selectionCustomPrompts) !== JSON.stringify(initial.selectionCustomPrompts)) {
    changedFields.selectionCustomPrompts = source.selectionCustomPrompts
  }
  if (source.selectionWebSearchEnabled !== initial.selectionWebSearchEnabled) {
    changedFields.selectionWebSearchEnabled = source.selectionWebSearchEnabled
  }
  if (source.selectionWebSearchEngine !== initial.selectionWebSearchEngine) {
    changedFields.selectionWebSearchEngine = source.selectionWebSearchEngine
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
  if (source.ocrEngine !== initial.ocrEngine) {
    changedFields.ocrEngine = source.ocrEngine
  }
  if (source.launcherEnabled !== initial.launcherEnabled) {
    changedFields.launcherEnabled = source.launcherEnabled
  }
  if (source.launcherHotKey !== initial.launcherHotKey) {
    changedFields.launcherHotKey = source.launcherHotKey
  }
  if (source.docManagerEnabled !== initial.docManagerEnabled) {
    changedFields.docManagerEnabled = source.docManagerEnabled
  }
  if (source.docManagerHotKey !== initial.docManagerHotKey) {
    changedFields.docManagerHotKey = source.docManagerHotKey
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
              t('settings.recording.ffmpegNotFound', {path: ffmpegStatus.ffmpegPath}),
              t('settings.recording.needDownloadFfmpeg'),
              {
                confirmButtonText: t('settings.recording.confirmDownload'),
                cancelButtonText: t('common.cancel'),
                type: 'warning',
                closeOnClickModal: false,
                closeOnPressEscape: true
              }
          )
        } catch {
          autoSaveState.value = 'idle'
          ElMessage.info(t('settings.recording.cancelEnableRecording'))
          return
        }
        const downloading = ElLoading.service({
          lock: true,
          text: t('settings.recording.downloadingFfmpeg', {percent: '0'}),
          background: 'rgba(0, 0, 0, 0.35)'
        })
        let unlistenProgress = null
        try {
          unlistenProgress = await listen('recording-ffmpeg-download-progress', (event) => {
            const payload = event?.payload || {}
            const percent = typeof payload.progressPercent === 'number' ? payload.progressPercent : null
            if (percent !== null) {
              downloading.setText(t('settings.recording.downloadingFfmpeg', {percent: `${percent}`}))
              return
            }
            const downloaded = Number(payload.downloadedBytes || 0)
            const total = Number(payload.totalBytes || 0)
            if (total > 0) {
              const computed = Math.min(100, Math.floor(downloaded * 100 / total))
              downloading.setText(t('settings.recording.downloadingFfmpeg', {percent: `${computed}`}))
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
        ElMessage.success(t('settings.recording.ffmpegDownloaded'))
      }
    }


    await AISettingsService.saveSettings(changedFields)


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
      ElMessage.success(t('settings.autoSaved'))
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
      let raw = ''
    let errorCode = null
    let errorParams = null
      if (typeof error === 'object' && error !== null) {
        raw = error.message || JSON.stringify(error)
      } else {
        raw = String(error || '')
      }
      try {
        const parsed = JSON.parse(raw)
        if (parsed && typeof parsed.code === 'string' && parsed.code.startsWith('E_')) {
          errorCode = parsed.code
          errorParams = parsed.params || null
          const i18nKey = `errorCodes.${errorCode}`
          const translated = t(i18nKey, errorParams || {})
          raw = translated === i18nKey ? (parsed.message || raw) : translated
        } else if (parsed && parsed.message) {
          raw = parsed.message
        }
      } catch (e) {}

    if (errorCode && (errorCode.includes('HOTKEY_CONFLICT') || errorCode.includes('HOTKEY_REGISTER'))) {
      shortcutConflictMessage.value = raw
      if (errorCode.includes('RECORDING') || raw.includes('麦克风')) {
        activeTab.value = 'recording'
      } else if (errorCode.includes('SCREENSHOT') || raw.includes('截图')) {
        activeTab.value = 'screenshot'
      } else {
        activeTab.value = 'clipboard'
      }
    } else if (!errorCode && (raw.includes('快捷键被占用') || raw.includes('shortcut') || raw.includes('hotkey'))) {
        shortcutConflictMessage.value = raw.replace(/^Error:\s*/i, '')
        if (raw.includes('录屏') || raw.includes('麦克风')) {
          activeTab.value = 'recording'
        } else if (raw.includes('截图')) {
          activeTab.value = 'screenshot'
        } else {
          activeTab.value = 'clipboard'
        }
      }
    if (errorCode) {
      ElMessage.error(raw)
    } else {
      ElMessage.error(t('settings.saveFailed', {error: raw}))
    }
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
  shortcutConflictMessage.value = t('settings.shortcutConflict', {conflicts: conflicts.join('；')})
}

const showVcRuntimeMissingWarning = async (payload) => {
  const {missing, installUrl} = normalizeVcRuntimeMissing(payload)
  if (missing.length === 0) return
  const detail = missing.join('、')
  try {
    await ElMessageBox.confirm(
        t('settings.screenshot.vcRuntimeMissing', {detail}),
        t('settings.screenshot.vcRuntimeMissingTitle'),
        {
          confirmButtonText: t('settings.screenshot.downloadInstall'),
          cancelButtonText: t('settings.screenshot.laterHandle'),
          type: 'warning',
          closeOnClickModal: false,
          closeOnPressEscape: true
        }
    )
    const downloading = ElLoading.service({
      lock: true,
      text: t('settings.screenshot.downloadingVc', {percent: '0'}),
      background: 'rgba(0, 0, 0, 0.35)'
    })
    let unlistenProgress = null
    let downloaded = null
    try {
      unlistenProgress = await listen('vc-runtime-download-progress', (event) => {
        const progress = event?.payload || {}
        const percent = typeof progress.progressPercent === 'number' ? progress.progressPercent : null
        if (percent !== null) {
          downloading.setText(t('settings.screenshot.downloadingVc', {percent: `${percent}`}))
          return
        }
        const done = Number(progress.downloadedBytes || 0)
        const total = Number(progress.totalBytes || 0)
        if (total > 0) {
          const computed = Math.min(100, Math.floor(done * 100 / total))
          downloading.setText(t('settings.screenshot.downloadingVc', {percent: `${computed}`}))
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
      ElMessage.error(t('settings.screenshot.vcDownloadSuccessNoPath'))
      return
    }
    const installing = ElLoading.service({
      lock: true,
      text: t('settings.screenshot.installingVc'),
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
      ElMessage.warning(t('settings.screenshot.vcInstallCancelled'))
      return
    }
    if (!success) {
      ElMessage.error(t('settings.screenshot.vcInstallFailed', {code: exitCode ?? 'unknown'}))
      return
    }
    if (rebootRequired) {
      ElMessage.warning(t('settings.screenshot.vcRebootRequired'))
      return
    }
    ElMessage.success(t('settings.screenshot.vcInstallSuccess'))
  } catch {

  }
}

const handleNavigateSettings = (payload) => {
  const tab = typeof payload?.tab === 'string' ? payload.tab : ''
  if (tab && sections.value.some((section) => section.key === tab)) {
    activeTab.value = tab
  }
  if (payload?.reason === 'selection_ai_not_configured') {
    ElMessage.warning(t('settings.selection.aiNotConfigured'))
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
    form.selectionModifierKey = settings.selection_modifier_key || ''
    form.selectionCustomPrompts = Array.isArray(settings.selection_custom_prompts) ? settings.selection_custom_prompts : []
    form.selectionWebSearchEnabled = settings.selection_web_search_enabled !== false
    form.selectionWebSearchEngine = settings.selection_web_search_engine || 'bing'
    form.groupedItemsProtectedFromLimit = settings.grouped_items_protected_from_limit !== false
    form.translationPromptTemplate = settings.translation_prompt_template || ''
    form.explanationPromptTemplate = settings.explanation_prompt_template || ''
    form.imageFillVerifyMode = settings.image_fill_verify_mode === 'strict' ? 'strict' : 'fast'
    form.ocrEngine = settings.ocr_engine || 'ocr-rs'
    form.launcherEnabled = settings.launcher_enabled !== false
    form.launcherHotKey = settings.launcher_hot_key || 'Alt+Q'
    form.docManagerEnabled = settings.doc_manager_enabled === true
    form.docManagerHotKey = settings.doc_manager_hot_key || 'Ctrl+Shift+D'

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
    ElMessage.error(t('settings.loadSettingsFailed', {error}))
    autoSaveState.value = 'error'
  } finally {

    saveInitialFormState()

    skipNextWatch.value = true
    isInitializing.value = false
  }
})

const featureLabels = {
  textClipboardEnabled: t('settings.featureLabels.textClipboardEnabled'),
  imageClipboardEnabled: t('settings.featureLabels.imageClipboardEnabled'),
  screenshotEnabled: t('settings.featureLabels.screenshotEnabled'),
  recordingEnabled: t('settings.featureLabels.recordingEnabled'),
  selectionEnabled: t('settings.featureLabels.selectionEnabled'),
  launcherEnabled: t('settings.featureLabels.launcherEnabled'),
  docManagerEnabled: t('settings.featureLabels.docManagerEnabled'),
}

const handleFeatureToggle = async (fieldName, newValue) => {
  const label = featureLabels[fieldName] || fieldName
  const action = newValue ? t('common.enable') : t('common.disable')
  const actionVerb = newValue ? t('common.enabling') : t('common.disabling')
  const loading = ElLoading.service({
    lock: true,
    text: t('settings.enablingFeature', {action: actionVerb, feature: label}),
    background: isDark.value ? 'rgba(0, 0, 0, 0.55)' : 'rgba(255, 255, 255, 0.55)'
  })
  const minUntil = Date.now() + 1000
  const payload = {}
  payload[fieldName] = newValue
  try {
    if (fieldName === 'screenshotEnabled' && newValue) {
      const runtimeStatus = await AISettingsService.checkVcRuntimeDependencies()
      const missing = Array.isArray(runtimeStatus?.missing) ? runtimeStatus.missing : []
      if (missing.length > 0) {
        loading.close()
        await showVcRuntimeMissingWarning(runtimeStatus)
        return false
      }
    }
    if (fieldName === 'recordingEnabled' && newValue) {
      const ffmpegStatus = await RecordingService.checkFfmpeg()
      if (!ffmpegStatus?.exists) {
        loading.close()
        try {
          await ElMessageBox.confirm(
              t('settings.recording.ffmpegNotFound', {path: ffmpegStatus.ffmpegPath}),
              t('settings.recording.needDownloadFfmpeg'),
              {
                confirmButtonText: t('settings.recording.confirmDownload'),
                cancelButtonText: t('common.cancel'),
                type: 'warning',
                closeOnClickModal: false,
                closeOnPressEscape: true
              }
          )
        } catch {
          ElMessage.info(t('settings.recording.cancelEnableRecording'))
          return false
        }
        const dl = ElLoading.service({
          lock: true,
          text: t('settings.recording.downloadingFfmpeg', {percent: '0'}),
          background: isDark.value ? 'rgba(0, 0, 0, 0.55)' : 'rgba(255, 255, 255, 0.55)'
        })
        let unlistenProgress = null
        try {
          unlistenProgress = await listen('recording-ffmpeg-download-progress', (event) => {
            const p = event?.payload || {}
            const percent = typeof p.progressPercent === 'number' ? p.progressPercent : null
            if (percent !== null) {
              dl.setText(t('settings.recording.downloadingFfmpeg', {percent: `${percent}`}));
              return
            }
            const downloaded = Number(p.downloadedBytes || 0)
            const total = Number(p.totalBytes || 0)
            if (total > 0) dl.setText(t('settings.recording.downloadingFfmpeg', {percent: `${Math.min(100, Math.floor(downloaded * 100 / total))}`}))
          })
          await RecordingService.downloadFfmpeg(ffmpegStatus.downloadUrl || null)
        } finally {
          if (unlistenProgress) unlistenProgress()
          dl.close()
        }
        ElMessage.success(t('settings.recording.ffmpegDownloaded'))
      }
    }
    loading.setText(t('settings.enablingFeature', {action: actionVerb, feature: label}))
    await AISettingsService.saveSettings(payload)
    suppressNextAutoSave.value = true
    form[fieldName] = newValue
    saveInitialFormState(buildFormSnapshot())
    return true
  } catch (error) {
    ElMessage.error(t('common.operationFailed', {error: String(error)}))
    return false
  } finally {
    const remaining = minUntil - Date.now()
    if (remaining > 0) {
      await new Promise(resolve => setTimeout(resolve, remaining))
    }
    loading.close()
  }
}

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
  background-color: var(--fy-bg-primary);
  color: var(--fy-text-primary);
  transition: background-color 0.3s, color 0.3s;
}

.el-overlay {
  position: fixed !important;
  z-index: 10000 !important;
  display: flex !important;
  align-items: center !important;
  justify-content: center !important;
}

.el-overlay.is-message-box {
  z-index: 10000 !important;
}

.el-message-box__wrapper {
  position: relative !important;
  margin: 0 !important;
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
  color: var(--fy-text-primary);
}

.header-title p {
  margin: 6px 0 0;
  color: var(--fy-text-muted);
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
  color: var(--fy-text-muted);
}

.autosave-pending {
  color: var(--fy-warning);
}

.autosave-saving {
  color: var(--fy-accent);
}

.autosave-saved {
  color: var(--fy-success);
}

.autosave-error {
  color: var(--fy-danger);
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
  border: 1px solid var(--fy-border);
  border-radius: 8px;
  background: var(--fy-bg-surface);
  color: var(--fy-text-secondary);
  padding: 10px 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  transition: all 0.2s ease;
  font-size: 14px;
}

.section-nav-item:hover {
  border-color: var(--fy-border-hover);
  color: var(--fy-accent);
}

.section-nav-item.active {
  border-color: var(--fy-accent);
  color: var(--fy-accent);
  background: var(--fy-accent-bg);
}

.content {
  flex: 1;
  min-height: 0;
  background: var(--fy-content-bg);
  padding: 22px;
  border-radius: 12px;
  border: 1px solid var(--fy-content-border);
  box-shadow: var(--fy-shadow);
  overflow-y: auto;
  overscroll-behavior: contain;
  transition: background 0.3s, border-color 0.3s;
}

.content-header {
  margin-bottom: 18px;
}

.content-header h2 {
  margin: 0;
  font-size: 20px;
  color: var(--fy-text-primary);
}

.content-header p {
  margin: 6px 0 0;
  color: var(--fy-text-muted);
  font-size: 13px;
}

.content .setting-section-card {
  border: 1px solid var(--fy-border-light);
  border-radius: 12px;
  box-shadow: none;
  background: var(--fy-bg-card);
}

.content .setting-section-card + .setting-section-card {
  margin-top: 16px;
}

.content .setting-section-card .el-card__header {
  padding: 12px 16px;
  border-bottom: 1px solid var(--fy-border-light);
  background: transparent;
  color: var(--fy-text-primary);
}

.content .setting-section-card .el-card__body {
  padding: 14px 16px 12px;
  background: transparent;
  color: var(--fy-text-primary);
}

.content .section-title {
  font-size: 15px;
  font-weight: 700;
  letter-spacing: 0.2px;
  color: var(--fy-text-primary);
}

.content .el-form-item {
  margin-bottom: 14px;
}

.content .el-form-item__label {
  font-weight: 600;
  padding-bottom: 6px;
  color: var(--fy-text-secondary);
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

.footer-links {
  margin-top: 14px;
  text-align: center;
  color: var(--fy-text-muted);
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
