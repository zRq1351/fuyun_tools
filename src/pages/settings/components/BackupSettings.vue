<template>
  <div class="backup-settings">
    <el-card class="section-card" shadow="never">
      <template #header>
        <div class="card-header">
          <span>{{ $t('settings.backup.manualBackup') }}</span>
          <el-button :loading="loadingPreview" @click="loadExportPreview">{{
              $t('settings.backup.refreshPreview')
            }}
          </el-button>
        </div>
      </template>

      <div v-if="preview" class="preview-grid">
        <div class="metric-item">
          <div class="metric-label">{{ $t('settings.backup.textHistory') }}</div>
          <div class="metric-value">{{ preview.stats.textItemCount }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">{{ $t('settings.backup.imageHistory') }}</div>
          <div class="metric-value">{{ preview.stats.imageItemCount }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">{{ $t('settings.backup.imageFiles') }}</div>
          <div class="metric-value">{{ preview.stats.imageBlobCount }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">{{ $t('settings.backup.estimatedSize') }}</div>
          <div class="metric-value">{{ formatBytes(preview.estimatedBytes) }}</div>
        </div>
      </div>

      <el-alert
          v-for="warning in preview?.warnings || []"
          :key="warning"
          :closable="false"
          :title="warning"
          show-icon
          type="warning"
      />

      <div class="action-row">
        <el-button :loading="exporting" type="primary" @click="exportBackup">{{
            $t('settings.backup.exportBackup')
          }}
        </el-button>
        <el-button :loading="manualBackupLoading" @click="runManualBackup">{{
            $t('settings.backup.runAutoBackup')
          }}
        </el-button>
        <el-button :loading="previewingPackage" @click="selectBackupPackage">
          {{ $t('settings.backup.selectBackupPreview') }}
        </el-button>
      </div>

      <el-alert
          v-if="lastResult"
          :closable="false"
          :title="lastResult"
          show-icon
          type="success"
      />
    </el-card>

    <el-card class="section-card" shadow="never">
      <template #header>
        <div class="card-header">
          <span>{{ $t('settings.backup.autoBackup') }}</span>
          <el-button :loading="savingSettings" type="primary" @click="saveSettings">
            {{ $t('settings.backup.saveAutoBackup') }}
          </el-button>
        </div>
      </template>

      <el-form label-position="top">
        <el-form-item :label="$t('settings.backup.autoBackupEnabled')">
          <el-switch v-model="settings.enabled"/>
        </el-form-item>
        <el-form-item :label="$t('settings.backup.backupFrequency')">
          <el-select v-model="settings.frequency">
            <el-option :label="$t('settings.backup.daily')" value="daily"/>
            <el-option :label="$t('settings.backup.weekly')" value="weekly"/>
            <el-option :label="$t('settings.backup.manualOnly')" value="manual"/>
          </el-select>
        </el-form-item>
        <el-form-item :label="$t('settings.backup.targetDir')">
          <div class="inline-row">
            <el-input v-model="settings.targetDir" :placeholder="$t('settings.backup.selectBackupDir')"/>
            <el-button @click="selectBackupDirectory">{{ $t('common.selectDir') }}</el-button>
          </div>
        </el-form-item>
        <el-form-item :label="$t('settings.backup.retentionCount')">
          <el-input-number v-model="settings.maxBackupCount" :max="50" :min="1"/>
        </el-form-item>
      </el-form>

      <div class="status-text">
        {{ $t('settings.backup.lastRunTime') }}{{ formatTimestamp(settings.lastRunAt) }}
      </div>
      <div class="status-text">
        {{ $t('settings.backup.lastRunStatus') }}{{ settings.lastRunStatus || 'idle' }}
      </div>
    </el-card>

    <el-card class="section-card" shadow="never">
      <template #header>
        <div class="card-header">
          <span>{{ $t('settings.backup.backupPreview') }}</span>
          <el-button :disabled="!packagePreview" :loading="restoring" type="danger" @click="restoreBackup">
            {{ $t('settings.backup.executeRestore') }}
          </el-button>
        </div>
      </template>

      <div v-if="packagePath" class="status-text">{{ $t('settings.backup.currentBackup') }}{{ packagePath }}</div>
      <div v-if="packagePreview" class="preview-grid">
        <div class="metric-item">
          <div class="metric-label">{{ $t('settings.backup.backupTime') }}</div>
          <div class="metric-value small">{{ formatTimestamp(packagePreview.manifest.createdAt) }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">{{ $t('settings.backup.appVersion') }}</div>
          <div class="metric-value">{{ packagePreview.manifest.appVersion }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">{{ $t('settings.backup.textHistory') }}</div>
          <div class="metric-value">{{ packagePreview.stats.textItemCount }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">{{ $t('settings.backup.imageHistory') }}</div>
          <div class="metric-value">{{ packagePreview.stats.imageItemCount }}</div>
        </div>
      </div>

      <el-alert
          v-for="warning in packagePreview?.warnings || []"
          :key="warning"
          :closable="false"
          :title="warning"
          show-icon
          type="warning"
      />

      <div v-if="packagePreview" class="restore-options">
        <el-radio-group v-model="restoreMode">
          <el-radio-button label="full">{{ $t('settings.backup.fullRestore') }}</el-radio-button>
          <el-radio-button label="partial">{{ $t('settings.backup.selectiveRestore') }}</el-radio-button>
        </el-radio-group>

        <div v-if="restoreMode === 'partial'" class="checkbox-group">
          <el-checkbox v-model="restoreSettings" :disabled="!packagePreview.restoreOptions.canRestoreSettings">
            {{ $t('settings.backup.restoreSettings') }}
          </el-checkbox>
          <el-checkbox v-model="restoreTextHistory" :disabled="!packagePreview.restoreOptions.canRestoreTextHistory">
            {{ $t('settings.backup.restoreTextHistory') }}
          </el-checkbox>
          <el-checkbox v-model="restoreImageHistory" :disabled="!packagePreview.restoreOptions.canRestoreImageHistory">
            {{ $t('settings.backup.restoreImageHistory') }}
          </el-checkbox>
        </div>

        <div class="checkbox-group" style="margin-top: 12px;">
          <span style="color: var(--el-text-color-regular); font-size: 14px;">{{
              $t('settings.backup.restoreStrategy')
            }}</span>
          <el-radio-group v-model="restoreStrategy">
            <el-radio-button label="merge">
              <el-tooltip :content="$t('settings.backup.mergeModeTooltip')" placement="top">
                <span>{{ $t('settings.backup.mergeMode') }}</span>
              </el-tooltip>
            </el-radio-button>
            <el-radio-button label="overwrite">
              <el-tooltip :content="$t('settings.backup.overwriteModeTooltip')" placement="top">
                <span>{{ $t('settings.backup.overwriteMode') }}</span>
              </el-tooltip>
            </el-radio-button>
          </el-radio-group>
        </div>
      </div>
    </el-card>

    <el-card class="section-card" shadow="never">
      <template #header>
        <div class="card-header">
          <span>{{ $t('settings.backup.recentBackups') }}</span>
          <el-button :loading="historyLoading" @click="loadHistory">{{ $t('settings.backup.refreshList') }}</el-button>
        </div>
      </template>

      <el-empty v-if="!history.length" :description="$t('settings.backup.noBackupRecords')"/>
      <div v-else class="history-list">
        <div v-for="item in history" :key="item.filePath" class="history-item">
          <div class="history-main">
            <div class="history-name">{{ item.fileName }}</div>
            <div class="history-meta">{{ formatTimestamp(item.createdAt) }} · {{
                formatBytes(item.fileSizeBytes)
              }}
            </div>
          </div>
          <div class="history-actions">
            <el-button size="small" @click="previewHistoryItem(item.filePath)">{{ $t('common.preview') }}</el-button>
            <el-button size="small" type="danger" @click="removeHistoryItem(item.filePath)">{{
                $t('common.delete')
              }}
            </el-button>
          </div>
        </div>
      </div>
    </el-card>
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, reactive, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage, ElMessageBox} from 'element-plus'
import {open, save} from '@tauri-apps/plugin-dialog'
import {listen} from '@tauri-apps/api/event'
import {BackupService} from '../../../services/ipc'

const {t} = useI18n()

const settings = reactive({
  enabled: false,
  frequency: 'weekly',
  targetDir: '',
  maxBackupCount: 5,
  lastRunAt: 0,
  lastRunStatus: 'idle'
})

const preview = ref(null)
const packagePreview = ref(null)
const packagePath = ref('')
const history = ref([])
const lastResult = ref('')
const restoreMode = ref('full')
const restoreStrategy = ref('merge')  // 'merge' or 'overwrite'
const restoreSettings = ref(true)
const restoreTextHistory = ref(true)
const restoreImageHistory = ref(true)

const loadingPreview = ref(false)
const exporting = ref(false)
const previewingPackage = ref(false)
const restoring = ref(false)
const historyLoading = ref(false)
const savingSettings = ref(false)
const manualBackupLoading = ref(false)
let unlistenBackupRunUpdated = null

const formatBytes = (bytes = 0) => {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  let size = bytes
  let index = 0
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024
    index += 1
  }
  return `${size.toFixed(size >= 10 || index === 0 ? 0 : 1)} ${units[index]}`
}

const formatTimestamp = (timestamp) => {
  if (!timestamp) return t('settings.backup.notExecuted')
  return new Date(Number(timestamp)).toLocaleString()
}

const syncBackupSettings = (payload) => {
  settings.enabled = !!payload?.enabled
  settings.frequency = payload?.frequency || 'weekly'
  settings.targetDir = payload?.targetDir || ''
  settings.maxBackupCount = payload?.maxBackupCount || 5
  settings.lastRunAt = payload?.lastRunAt || 0
  settings.lastRunStatus = payload?.lastRunStatus || 'idle'
}

const loadSettings = async () => {
  const payload = await BackupService.getSettings()
  syncBackupSettings(payload)
}

const loadExportPreview = async () => {
  loadingPreview.value = true
  try {
    const response = await BackupService.previewExport()
    preview.value = response.data
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    loadingPreview.value = false
  }
}

const loadHistory = async () => {
  historyLoading.value = true
  try {
    history.value = await BackupService.listHistory()
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    historyLoading.value = false
  }
}

const saveSettings = async () => {
  savingSettings.value = true
  try {
    const payload = await BackupService.saveSettings({
      enabled: settings.enabled,
      frequency: settings.frequency,
      targetDir: settings.targetDir,
      maxBackupCount: settings.maxBackupCount
    })
    syncBackupSettings(payload)
    ElMessage.success(t('settings.backup.autoBackupSaved'))
    await loadHistory()
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    savingSettings.value = false
  }
}

const selectBackupDirectory = async () => {
  const path = await open({
    directory: true,
    multiple: false,
    title: t('settings.backup.selectAutoBackupDir')
  })
  if (typeof path === 'string') {
    settings.targetDir = path
  }
}

const exportBackup = async () => {
  exporting.value = true
  try {
    const targetPath = await save({
      title: t('settings.backup.exportBackupTitle'),
      defaultPath: defaultExportName(),
      filters: [{name: 'Fuyun Backup', extensions: ['zip']}]
    })
    if (!targetPath) return
    const response = await BackupService.exportToPath(targetPath)
    lastResult.value = t('settings.backup.exportSuccess', {path: response.data.filePath})
    ElMessage.success(t('settings.backup.backupExportSuccess'))
    await Promise.all([loadExportPreview(), loadSettings(), loadHistory()])
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    exporting.value = false
  }
}

const runManualBackup = async () => {
  manualBackupLoading.value = true
  try {
    const response = await BackupService.runManualBackup()
    lastResult.value = t('settings.backup.manualBackupSuccess', {path: response.data.filePath})
    ElMessage.success(t('settings.backup.manualBackupCompleted'))
    await Promise.all([loadSettings(), loadHistory()])
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    manualBackupLoading.value = false
  }
}

const previewHistoryItem = async (path) => {
  packagePath.value = path
  previewingPackage.value = true
  try {
    const response = await BackupService.previewPackage(path)
    packagePreview.value = response.data
    restoreSettings.value = response.data.restoreOptions.canRestoreSettings
    restoreTextHistory.value = response.data.restoreOptions.canRestoreTextHistory
    restoreImageHistory.value = response.data.restoreOptions.canRestoreImageHistory
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    previewingPackage.value = false
  }
}

const selectBackupPackage = async () => {
  const path = await open({
    directory: false,
    multiple: false,
    title: t('settings.backup.selectBackupPackage'),
    filters: [{name: 'Fuyun Backup', extensions: ['zip']}]
  })
  if (typeof path === 'string') {
    await previewHistoryItem(path)
  }
}

const restoreBackup = async () => {
  if (!packagePath.value || !packagePreview.value) return
  if (restoreMode.value === 'partial' && !restoreSettings.value && !restoreTextHistory.value && !restoreImageHistory.value) {
    ElMessage.warning(t('settings.backup.selectRestoreModule'))
    return
  }

  const strategyText = restoreStrategy.value === 'merge' ? t('documentManager.mergeModeLabel') : t('documentManager.overwriteModeLabel')
  await ElMessageBox.confirm(
      t('settings.backup.restoreConfirm', {strategy: strategyText}),
      t('common.confirmRestore'),
      {type: restoreStrategy.value === 'overwrite' ? 'error' : 'warning'}
  )
  restoring.value = true
  try {
    const response = await BackupService.restorePackage({
      packagePath: packagePath.value,
      mode: restoreMode.value,
      restoreSettings: restoreMode.value === 'full' ? true : restoreSettings.value,
      restoreTextHistory: restoreMode.value === 'full' ? true : restoreTextHistory.value,
      restoreImageHistory: restoreMode.value === 'full' ? true : restoreImageHistory.value,
      createRollbackPoint: true,
      restoreStrategy: restoreStrategy.value
    })
    lastResult.value = t('settings.backup.restoreCompleted', {result: response.message})
    ElMessage.success(t('settings.backup.backupRestoreCompleted'))
    await Promise.all([loadExportPreview(), loadSettings(), loadHistory()])
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    restoring.value = false
  }
}

const removeHistoryItem = async (filePath) => {
  await ElMessageBox.confirm(t('settings.backup.deleteBackupConfirm'), t('settings.backup.deleteBackupTitle'), {type: 'warning'})
  try {
    await BackupService.deleteHistoryItem(filePath)
    ElMessage.success(t('settings.backup.backupDeleted'))
    await loadHistory()
    if (packagePath.value === filePath) {
      packagePath.value = ''
      packagePreview.value = null
    }
  } catch (error) {
    ElMessage.error(String(error))
  }
}

const defaultExportName = () => `fuyun_tools_${Date.now()}.fytbk.zip`

onMounted(async () => {
  await Promise.all([loadSettings(), loadExportPreview(), loadHistory()])
  unlistenBackupRunUpdated = await listen('backup-run-updated', async (event) => {
    const payload = event.payload || {}
    if (payload.status === 'success') {
      lastResult.value = t('settings.backup.autoBackupCompleted')
    } else if (payload.status === 'failed') {
      lastResult.value = t('settings.backup.autoBackupFailed', {error: String(payload.message || '')})
    }
    await Promise.all([loadSettings(), loadExportPreview(), loadHistory()])
  })
})

onUnmounted(() => {
  if (typeof unlistenBackupRunUpdated === 'function') {
    unlistenBackupRunUpdated()
    unlistenBackupRunUpdated = null
  }
})
</script>

<style scoped>
.backup-settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.section-card {
  border-radius: 16px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
}

.preview-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 12px;
  margin-bottom: 16px;
}

.metric-item {
  padding: 12px;
  border-radius: 12px;
  background: var(--el-fill-color-light);
}

.metric-label {
  color: var(--fy-text-muted);
  font-size: 12px;
}

.metric-value {
  margin-top: 4px;
  font-size: 20px;
  font-weight: 600;
}

.metric-value.small {
  font-size: 14px;
}

.action-row,
.inline-row,
.checkbox-group {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  margin-top: 16px;
}

.inline-row :deep(.el-input) {
  flex: 1;
}

.status-text {
  color: var(--fy-text-muted);
  margin-top: 8px;
}

.history-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.history-item {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 12px;
  border-radius: 12px;
  background: var(--el-fill-color-light);
}

.history-main {
  min-width: 0;
}

.history-name {
  font-weight: 600;
  word-break: break-all;
}

.history-meta {
  margin-top: 6px;
  color: var(--fy-text-muted);
  font-size: 12px;
}

.history-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.restore-options {
  margin-top: 16px;
}
</style>
