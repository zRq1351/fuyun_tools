<template>
  <el-config-provider :locale="elLocale">
    <div class="settings-container">
      <div class="header">
        <div class="header-title">
          <h1>{{ $t('settings.title') }}</h1>
        </div>
        <div class="header-actions">
          <span :class="['autosave-status', `autosave-${autoSaveState}`]">{{ autoSaveText }}</span>
          <el-dropdown
            trigger="click"
            @command="changeLocale"
          >
            <el-button>
              <template #icon>
                <el-icon>
                  <icon-menu />
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
          <el-dropdown
            trigger="click"
            @command="changeTheme"
          >
            <el-button>
              <template #icon>
                <component :is="themeIcon" />
              </template>
              {{ themeLabel }}
            </el-button>
            <template #dropdown>
              <el-dropdown-menu>
                <el-dropdown-item command="dark">
                  <el-icon>
                    <Moon />
                  </el-icon>
                  {{ $t('theme.dark') }}
                </el-dropdown-item>
                <el-dropdown-item command="light">
                  <el-icon>
                    <Sunny />
                  </el-icon>
                  {{ $t('theme.light') }}
                </el-dropdown-item>
                <el-dropdown-item command="eye-care">
                  <el-icon>
                    <View />
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
              <component :is="section.icon" />
            </el-icon>
            <span>{{ section.label }}</span>
            <span
              v-if="section.key === 'about' && updateAvailable"
              class="update-badge"
            >v{{ updateAvailable.version }}</span>
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
            <ClipboardSettings
              :form="form"
              :on-feature-toggle="handleFeatureToggle"
            />
          </div>
          <div v-else-if="activeTab === 'screenshot'">
            <ScreenshotSettings
              :form="form"
              :on-feature-toggle="handleFeatureToggle"
            />
          </div>
          <div v-else-if="activeTab === 'recording'">
            <RecordingSettings
              :form="form"
              :on-feature-toggle="handleFeatureToggle"
            />
          </div>

          <div v-else-if="activeTab === 'selection'">
            <SelectionSettings
              :form="form"
              :on-feature-toggle="handleFeatureToggle"
            />
          </div>

          <div v-else-if="activeTab === 'launcher'">
            <LauncherSettings
              :form="form"
              :on-feature-toggle="handleFeatureToggle"
            />
          </div>

          <div v-else-if="activeTab === 'doc_manager'">
            <DocumentManagerSettings
              :form="form"
              :on-feature-toggle="handleFeatureToggle"
            />
          </div>

          <div v-else-if="activeTab === 'ai'">
            <AISettings
              ref="aiSettingsRef"
              :form="form"
            />
          </div>
          <div v-else-if="activeTab === 'backup'">
            <BackupSettings />
          </div>
          <div v-else-if="activeTab === 'diagnostic'">
            <DiagnosticSettings
              :logging-enabled="form.loggingEnabled"
              @navigate="handleNavigateTab"
              @toggle-logging="handleLoggingToggle"
            />
          </div>

          <div v-else-if="isDevMode && activeTab === 'developer'">
            <DeveloperSettings />
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
          <el-link
            type="primary"
            @click="openExternal('https://github.com/zRq1351/fuyun_tools')"
          >
            {{ $t('settings.viewDocs') }}
          </el-link>
          |
          <el-link
            type="primary"
            @click="openExternal('https://github.com/zRq1351/fuyun_tools/issues')"
          >
            {{ $t('settings.reportIssue') }}
          </el-link>
        </p>
        <p>{{ $t('settings.version') }} {{ currentVersion }} | &copy; {{ new Date().getFullYear() }} fuyun_tools</p>
      </div>
    </div>
  </el-config-provider>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref} from 'vue'
import {ElLoading, ElMessage, ElMessageBox} from 'element-plus'
import {useI18n} from 'vue-i18n'
import {
  Menu as IconMenu,
  Moon,
  Sunny,
  View
} from '@element-plus/icons-vue'
import {useTheme} from '@/composables/useTheme.js'
import {useLocale} from '@/composables/useLocale.js'
import {openUrl} from '@tauri-apps/plugin-opener'
import {listen} from '@tauri-apps/api/event'
import {AISettingsService} from '@/services/ipc.js'
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
import {useUpdater} from './composables/useUpdater'
import {useSettingsNav} from './composables/useSettingsNav'
import {useSettingsPersistence} from './composables/useSettingsPersistence'

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

const {activeTab, isDevMode, sections, currentSection, handleNavigateTab} = useSettingsNav()

const currentVersion = ref('0.0.0')
const aiSettingsRef = ref(null)
const shortcutConflictMessage = ref('')
let unlistenShortcutConflict = null
let unlistenNavigateSettings = null
const updateAvailable = ref(null) // { version: string } 或 null
const {silentCheck} = useUpdater(currentVersion)

const form = reactive({
  textMaxItems: 100,
  imageMaxItems: 100,
  imageDiskLimitMb: 2048,
  // 与后端 serde default 对齐，避免加载失败后 autosave 把功能全部打开
  textClipboardEnabled: false,
  imageClipboardEnabled: false,
  screenshotEnabled: false,
  recordingEnabled: false,
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
  selectionEnabled: false,
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
  docManagerHotKey: 'Ctrl+Shift+D',
  docManagerWidgetEnabled: false,
  loggingEnabled: true
})

// 日志开关：写入 form 后由深度 watch 自动保存，Rust 侧保存后即时生效
const handleLoggingToggle = (val) => {
  form.loggingEnabled = val
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

const {
  autoSaveState,
  autoSaveText,
  isInitializing,
  settingsLoadFailed,
  skipNextWatch,
  saveInitialFormState,
  handleFeatureToggle
} = useSettingsPersistence({
  form,
  aiSettingsRef,
  shortcutConflictMessage,
  activeTab,
  isDark,
  showVcRuntimeMissingWarning
})

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
    form.docManagerWidgetEnabled = settings.doc_manager_widget_enabled === true
    form.loggingEnabled = settings.logging_enabled !== false

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
    settingsLoadFailed.value = true
  } finally {

    saveInitialFormState()

    skipNextWatch.value = true
    isInitializing.value = false

    // 启动后静默检查更新（useUpdater 内部已做版本比较）
    silentCheck().then(update => {
      if (update) updateAvailable.value = update
    })
  }
})

onBeforeUnmount(() => {
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

<style src="./settings.css"></style>
