<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.clipboard.shortcut') }}</div>
      </template>
      <div class="setting-group">
        <div class="group-grid cols-2">
          <el-form-item :label="$t('settings.clipboard.textEnabled')">
            <el-switch
                :active-text="pendingToggles.textClipboard === 'disabling' ? $t('common.disabling') : $t('common.enable')"
                :inactive-text="pendingToggles.textClipboard === 'enabling' ? $t('common.enabling') : $t('common.disable')"
                :loading="!!pendingToggles.textClipboard"
                :model-value="form.textClipboardEnabled"
                @update:model-value="(val) => toggleFeature('textClipboardEnabled', val)"
            />
            <div class="form-hint">{{ $t('settings.clipboard.textDisabledHint') }}</div>
          </el-form-item>
          <el-form-item :label="$t('settings.clipboard.imageEnabled')">
            <el-switch
                :active-text="pendingToggles.imageClipboard === 'disabling' ? $t('common.disabling') : $t('common.enable')"
                :inactive-text="pendingToggles.imageClipboard === 'enabling' ? $t('common.enabling') : $t('common.disable')"
                :loading="!!pendingToggles.imageClipboard"
                :model-value="form.imageClipboardEnabled"
                @update:model-value="(val) => toggleFeature('imageClipboardEnabled', val)"
            />
            <div class="form-hint">{{ $t('settings.clipboard.imageDisabledHint') }}</div>
          </el-form-item>
        </div>
        <div class="group-grid cols-2">
          <el-form-item :label="$t('settings.clipboard.openTextWindow')">
            <el-input
                :model-value="textDisplayValue"
                :class="{ recording: isTextRecording }"
                :placeholder="$t('settings.clipboard.shortcutExample')"
                readonly
            >
              <template #append>
                <el-button-group>
                  <el-button :title="$t('settings.clipboard.modifyShortcut')" :type="isTextRecording ? 'danger' : 'primary'"
                             @click="toggleTextRecording">
                    <el-icon>
                      <component :is="isTextRecording ? VideoPause : Edit"/>
                    </el-icon>
                  </el-button>
                  <el-button :title="$t('settings.clipboard.resetShortcut')" @click="resetTextRecording">
                    <el-icon><RefreshLeft /></el-icon>
                  </el-button>
                </el-button-group>
              </template>
            </el-input>
          </el-form-item>
          <el-form-item :label="$t('settings.clipboard.openImageWindow')">
            <el-input
                :model-value="imageDisplayValue"
                :class="{ recording: isImageRecording }"
                :placeholder="$t('settings.clipboard.shortcutExample')"
                readonly
            >
              <template #append>
                <el-button-group>
                  <el-button :title="$t('settings.clipboard.modifyShortcut')" :type="isImageRecording ? 'danger' : 'primary'"
                             @click="toggleImageRecording">
                    <el-icon>
                      <component :is="isImageRecording ? VideoPause : Edit"/>
                    </el-icon>
                  </el-button>
                  <el-button :title="$t('settings.clipboard.resetShortcut')" @click="resetImageRecording">
                    <el-icon><RefreshLeft /></el-icon>
                  </el-button>
                </el-button-group>
              </template>
            </el-input>
          </el-form-item>
        </div>
      </div>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.clipboard.capacity') }}</div>
      </template>
      <div class="setting-group">
        <div class="group-grid cols-3">
          <el-form-item :label="$t('settings.clipboard.textMaxItems')">
            <el-input-number v-model="form.textMaxItems" :max="1000" :min="1"/>
            <div class="form-hint">{{ $t('settings.clipboard.textMaxHint') }}</div>
          </el-form-item>
          <el-form-item :label="$t('settings.clipboard.imageMaxItems')">
            <el-input-number v-model="form.imageMaxItems" :max="1000" :min="1"/>
            <div class="form-hint">{{ $t('settings.clipboard.textMaxHint') }}</div>
          </el-form-item>
          <el-form-item :label="$t('settings.clipboard.imageDiskLimit')">
            <el-input-number v-model="form.imageDiskLimitMb" :max="102400" :min="100"/>
            <div class="form-hint">{{ $t('settings.clipboard.diskLimitHint') }}</div>
          </el-form-item>
        </div>
        <div class="group-grid cols-2">
          <el-form-item :label="$t('settings.clipboard.imageFillMode')">
            <el-select v-model="form.imageFillVerifyMode">
              <el-option :label="$t('settings.clipboard.fillStrict')" value="strict"/>
              <el-option :label="$t('settings.clipboard.fillFast')" value="fast"/>
            </el-select>
            <div class="form-hint">{{ $t('settings.clipboard.fillModeHint') }}</div>
          </el-form-item>
          <el-form-item :label="$t('settings.clipboard.limitPolicy')">
            <el-switch
                v-model="form.groupedItemsProtectedFromLimit"
                :active-text="$t('settings.clipboard.limitUngrouped')"
                :inactive-text="$t('settings.clipboard.limitAll')"
            />
            <div class="form-hint">{{ $t('settings.clipboard.limitPolicyHint') }}</div>
          </el-form-item>
        </div>
      </div>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.clipboard.dataManagement') }}</div>
      </template>
      <div class="management-list">
        <div class="management-item">
          <div class="management-meta">
            <div class="management-title">{{ $t('settings.clipboard.textRecords') }}</div>
            <div class="form-hint">{{ $t('settings.clipboard.textCleanHint') }}</div>
          </div>
          <div class="action-row">
            <el-button class="action-button" plain type="primary" @click="clearTextHistory('unclassified_unpinned')">
              {{ $t('settings.clipboard.conditionalClean') }}
            </el-button>
            <el-button class="action-button" plain type="danger" @click="clearTextHistory('all')">
              {{ $t('settings.clipboard.clearAll') }}
            </el-button>
          </div>
        </div>
        <div class="management-item">
          <div class="management-meta">
            <div class="management-title">{{ $t('settings.clipboard.imageRecords') }}</div>
            <div class="form-hint">{{ $t('settings.clipboard.imageCleanHint') }}</div>
          </div>
          <div class="action-row">
            <el-button class="action-button" plain type="primary"
                       @click="clearImageHistory('untagged_unclassified_unpinned')">
              {{ $t('settings.clipboard.conditionalClean') }}
            </el-button>
            <el-button class="action-button" plain type="danger" @click="clearImageHistory('all')">
              {{ $t('settings.clipboard.clearAll') }}
            </el-button>
          </div>
        </div>
        <div class="management-item">
          <div class="management-meta">
            <div class="management-title">{{ $t('settings.clipboard.importImage') }}</div>
            <div class="form-hint">{{ $t('settings.clipboard.importImageHint') }}</div>
          </div>
          <el-input
              :model-value="importSourceDisplay"
              class="import-source-input"
              :placeholder="$t('settings.clipboard.noImportSource')"
              readonly
          >
            <template #prepend>
              <el-tooltip :content="$t('settings.clipboard.importFromFile')" placement="top">
                <el-button :loading="importingImages" class="import-icon-btn" @click="importImageFiles">
                  <el-icon><Picture/></el-icon>
                </el-button>
              </el-tooltip>
            </template>
            <template #append>
              <el-tooltip :content="$t('settings.clipboard.importFromDir')" placement="top">
                <el-button :loading="importingImages" class="import-icon-btn" @click="importImageFolders">
                  <el-icon><FolderOpened/></el-icon>
                </el-button>
              </el-tooltip>
            </template>
          </el-input>
          <div v-if="showImportProgressCard" class="metrics-card">
            <div class="metrics-line">{{ $t('settings.clipboard.importProgress') }} {{ importProcessed }} /
              {{ importTotal }}
            </div>
            <div class="metrics-line">{{ $t('common.success') }} {{ importImported }}，{{ $t('common.failed') }}
              {{ importFailed }}
            </div>
            <el-progress :percentage="importProgressPercent" :stroke-width="12" status="success"/>
          </div>
        </div>
      </div>
      <div class="form-hint">{{ $t('settings.clipboard.clearAllWarning') }}</div>
    </el-card>

  </el-form>
</template>

<script setup>
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {Edit, FolderOpened, Picture, RefreshLeft, VideoPause} from '@element-plus/icons-vue'
import {open} from '@tauri-apps/plugin-dialog'
import {listen} from '@tauri-apps/api/event'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {ClipboardService, ImageClipboardService} from '../../../services/ipc'
import {ElMessage, ElMessageBox} from 'element-plus'

const {t} = useI18n()

const props = defineProps({
  form: {
    type: Object,
    required: true
  },
  onFeatureToggle: {
    type: Function,
    default: null
  }
})

const pendingToggles = ref({})

const toggleFeature = async (fieldName, value) => {
  if (pendingToggles.value[fieldName]) return
  if (!props.onFeatureToggle) {
    props.form[fieldName] = value
    return
  }
  pendingToggles.value = {...pendingToggles.value, [fieldName]: value ? 'enabling' : 'disabling'}
  const ok = await props.onFeatureToggle(fieldName, value)
  pendingToggles.value = {...pendingToggles.value, [fieldName]: undefined}
  if (!ok) return
}

const {
  isRecording: isTextRecording,
  currentDisplayValue: textDisplayValue,
  toggleRecording: toggleTextRecording,
  stopRecording: stopTextRecording
} = useShortcutRecorder(props.form, 'toggleShortcut')

const {
  isRecording: isImageRecording,
  currentDisplayValue: imageDisplayValue,
  toggleRecording: toggleImageRecording,
  stopRecording: stopImageRecording
} = useShortcutRecorder(props.form, 'imageToggleShortcut')

const resetTextRecording = () => {
  stopTextRecording()
  const isMac = navigator.userAgent.toLowerCase().includes('mac')
  props.form.toggleShortcut = isMac ? 'Cmd+Shift+z' : 'Ctrl+Shift+z'
  ElMessage.success(t('settings.clipboard.shortcutResetText', {shortcut: props.form.toggleShortcut}))
}

const resetImageRecording = () => {
  stopImageRecording()
  const isMac = navigator.userAgent.toLowerCase().includes('mac')
  props.form.imageToggleShortcut = isMac ? 'Cmd+Shift+x' : 'Ctrl+Shift+x'
  ElMessage.success(t('settings.clipboard.shortcutResetImage', {shortcut: props.form.imageToggleShortcut}))
}

let unlistenImportProgress = null
const importingImages = ref(false)
const importTotal = ref(0)
const importProcessed = ref(0)
const importImported = ref(0)
const importFailed = ref(0)
let importProgressResetTimer = null
const importSourceDisplay = ref('')

const importProgressPercent = computed(() => {
  const total = Number(importTotal.value || 0)
  if (!total) return 0
  const processed = Number(importProcessed.value || 0)
  return Math.min(100, Math.max(0, Math.round((processed / total) * 100)))
})

const showImportProgressCard = computed(() => {
  if (importingImages.value) return true
  const total = Number(importTotal.value || 0)
  const processed = Number(importProcessed.value || 0)
  return total > 0 && processed < total
})

const scheduleResetImportProgress = () => {
  if (importProgressResetTimer) {
    clearTimeout(importProgressResetTimer)
    importProgressResetTimer = null
  }
  importProgressResetTimer = window.setTimeout(() => {
    resetImportProgress()
    importProgressResetTimer = null
  }, 800)
}

const clearTextHistory = async (mode) => {
  try {
    if (mode === 'all') {
      const msgBox = ElMessageBox.confirm(
          t('settings.clipboard.clearTextConfirm'),
          t('common.warning'),
          {
            type: 'warning',
            confirmButtonText: t('settings.clipboard.continueClear'),
            cancelButtonText: t('common.cancel')
          }
      )
      await msgBox
    }
    const removed = await ClipboardService.clearHistory(mode)
    ElMessage.success(t('settings.clipboard.textCleaned', {count: removed}))
  } catch (error) {
    if (error === 'cancel' || error?.action === 'cancel') return
    ElMessage.error(t('settings.clipboard.cleanFailed', {error: String(error)}))
  }
}

const clearImageHistory = async (mode) => {
  try {
    if (mode === 'all') {
      await ElMessageBox.confirm(
          t('settings.clipboard.clearImageConfirm'),
          t('common.warning'),
          {
            type: 'warning',
            confirmButtonText: t('settings.clipboard.continueClear'),
            cancelButtonText: t('common.cancel')
          }
      )
    }
    const removed = await ImageClipboardService.clearHistory(mode)
    ElMessage.success(t('settings.clipboard.imageCleaned', {count: removed}))
  } catch (error) {
    if (error === 'cancel' || error?.action === 'cancel') return
    ElMessage.error(t('settings.clipboard.cleanFailed', {error: String(error)}))
  }
}

const resetImportProgress = () => {
  importTotal.value = 0
  importProcessed.value = 0
  importImported.value = 0
  importFailed.value = 0
}

const runImageImport = async (paths) => {
  if (!paths || !paths.length) return false
  importingImages.value = true
  resetImportProgress()
  try {
    const imported = await ImageClipboardService.importImageFiles(paths)
    ElMessage.success(t('settings.clipboard.imagesImported', {count: imported}))
    return true
  } catch (error) {
    ElMessage.error(t('settings.clipboard.importFailed', {error}))
    return false
  } finally {
    importingImages.value = false
    scheduleResetImportProgress()
  }
}

const confirmImport = async (kind, paths) => {
  let total
  try {
    total = Number(await ImageClipboardService.countImportImageFiles(paths)) || 0
  } catch {
    total = 0
  }
  const summary = kind === 'folder'
      ? t('settings.clipboard.importDirConfirm', {path: String(paths[0] || ''), total})
      : t('settings.clipboard.importFileConfirm', {count: paths.length, total})
  try {
    await ElMessageBox.confirm(summary, t('common.confirmImport'), {
      confirmButtonText: t('common.confirmImport'),
      cancelButtonText: t('common.cancel'),
      type: 'info'
    })
    return true
  } catch {
    return false
  }
}

const importImageFiles = async () => {
  const selected = await open({
    multiple: true,
    filters: [
      {name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'bmp', 'webp', 'gif', 'tif', 'tiff']}
    ]
  })
  if (!selected) return
  const paths = Array.isArray(selected) ? selected : [selected]
  const names = paths.map((item) => {
    const path = String(item || '')
    const parts = path.split(/[\\/]/).filter(Boolean)
    return parts[parts.length - 1] || path
  })
  importSourceDisplay.value = names.length > 1 ? t('settings.clipboard.selectedFiles', {
    first: names[0],
    count: names.length
  }) : (names[0] || '')
  const confirmed = await confirmImport('file', paths)
  if (!confirmed) return
  const ok = await runImageImport(paths)
  if (ok) {
    importSourceDisplay.value = ''
  }
}

const importImageFolders = async () => {
  const selected = await open({
    directory: true,
    multiple: true
  })
  if (!selected) return
  const paths = Array.isArray(selected) ? selected : [selected]
  importSourceDisplay.value = String(paths[0] || '')
  const confirmed = await confirmImport('folder', paths)
  if (!confirmed) return
  const ok = await runImageImport(paths)
  if (ok) {
    importSourceDisplay.value = ''
  }
}

const handleDocumentVisibilityChange = () => {
  if (document.hidden) {
    importingImages.value = false
    resetImportProgress()
  }
}

onMounted(async () => {
  document.addEventListener('visibilitychange', handleDocumentVisibilityChange)
  unlistenImportProgress = await listen('image-import-progress', (event) => {
    const payload = event.payload || {}
    importTotal.value = Number(payload.total || 0)
    importProcessed.value = Number(payload.processed || 0)
    importImported.value = Number(payload.imported || 0)
    importFailed.value = Number(payload.failed || 0)
    if (payload.status === 'start') {
      importingImages.value = true
      if (importProgressResetTimer) {
        clearTimeout(importProgressResetTimer)
        importProgressResetTimer = null
      }
    } else if (payload.status === 'finish') {
      importingImages.value = false
      scheduleResetImportProgress()
    }
  })
})

onUnmounted(() => {
  if (importProgressResetTimer) {
    clearTimeout(importProgressResetTimer)
    importProgressResetTimer = null
  }
  document.removeEventListener('visibilitychange', handleDocumentVisibilityChange)
  if (unlistenImportProgress) {
    unlistenImportProgress()
    unlistenImportProgress = null
  }
})
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: var(--fy-text-muted);
  margin-top: 4px;
}

.setting-section-card + .setting-section-card {
  margin-top: 16px;
}

.action-button {
  min-width: 120px;
  border-radius: 8px;
  font-weight: 600;
}

.action-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.import-source-input {
  margin-bottom: 8px;
}

.import-icon-btn {
  border: none;
  padding: 0 10px;
  min-width: auto;
}

.management-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.management-item {
  border: 1px solid var(--fy-border-light);
  border-radius: 10px;
  padding: 10px 12px;
}

.management-meta {
  margin-bottom: 8px;
}

.management-title {
  font-size: 14px;
  font-weight: 700;
  margin-bottom: 2px;
  color: var(--fy-text-primary);
}

.section-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--fy-text-primary);
}

.setting-group {
  border: 1px solid var(--fy-border-light);
  border-radius: 10px;
  padding: 12px 12px 6px;
  background: var(--fy-bg-card);
}

.group-grid {
  display: grid;
  column-gap: 14px;
}

.group-grid.cols-2 {
  grid-template-columns: repeat(2, minmax(260px, 1fr));
}

.group-grid.cols-3 {
  grid-template-columns: repeat(3, minmax(180px, 1fr));
}

.setting-group :deep(.el-input-group__append) {
  display: flex;
  flex-wrap: nowrap;
  width: auto;
}

.setting-group :deep(.el-button-group) {
  display: flex;
  flex-wrap: nowrap;
}

.setting-group :deep(.el-form-item) {
  margin-bottom: 12px;
}

.setting-group :deep(.el-form-item__label) {
  color: var(--fy-text-secondary);
}

.recording :deep(.el-input__inner) {
  color: var(--fy-danger) !important;
}

.metrics-card {
  width: 100%;
  box-sizing: border-box;
  padding: 10px 12px;
  border: 1px solid var(--fy-border-light);
  border-radius: 6px;
  overflow: hidden;
  background: var(--fy-bg-surface);
}

.metrics-line {
  font-size: 12px;
  line-height: 20px;
  color: var(--fy-text-secondary);
}

.metrics-card :deep(.el-progress) {
  width: 100%;
  max-width: 100%;
}

.metrics-meta {
  margin-left: 10px;
  color: var(--fy-text-muted);
  font-size: 12px;
}

.sparkline {
  margin-top: 8px;
  font-size: 16px;
  letter-spacing: 1px;
}

@media (max-width: 900px) {
  .group-grid.cols-2,
  .group-grid.cols-3 {
    grid-template-columns: 1fr;
  }
}
</style>
