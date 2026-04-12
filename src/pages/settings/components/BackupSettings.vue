<template>
  <div class="backup-settings">
    <el-card class="section-card" shadow="never">
      <template #header>
        <div class="card-header">
          <span>手动备份与恢复</span>
          <el-button :loading="loadingPreview" @click="loadExportPreview">刷新导出预览</el-button>
        </div>
      </template>

      <div v-if="preview" class="preview-grid">
        <div class="metric-item">
          <div class="metric-label">文字历史</div>
          <div class="metric-value">{{ preview.stats.textItemCount }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">图片历史</div>
          <div class="metric-value">{{ preview.stats.imageItemCount }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">图片文件</div>
          <div class="metric-value">{{ preview.stats.imageBlobCount }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">预计体积</div>
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
        <el-button :loading="exporting" type="primary" @click="exportBackup">导出备份</el-button>
        <el-button :loading="manualBackupLoading" @click="runManualBackup">按自动备份配置立即执行一次</el-button>
        <el-button :loading="previewingPackage" @click="selectBackupPackage">选择备份包并预览</el-button>
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
          <span>自动备份</span>
          <el-button :loading="savingSettings" type="primary" @click="saveSettings">保存自动备份配置</el-button>
        </div>
      </template>

      <el-form label-position="top">
        <el-form-item label="启用自动备份">
          <el-switch v-model="settings.enabled"/>
        </el-form-item>
        <el-form-item label="备份频率">
          <el-select v-model="settings.frequency">
            <el-option label="每天" value="daily"/>
            <el-option label="每周" value="weekly"/>
            <el-option label="仅手动触发" value="manual"/>
          </el-select>
        </el-form-item>
        <el-form-item label="目标目录">
          <div class="inline-row">
            <el-input v-model="settings.targetDir" placeholder="请选择备份目录"/>
            <el-button @click="selectBackupDirectory">选择目录</el-button>
          </div>
        </el-form-item>
        <el-form-item label="保留份数">
          <el-input-number v-model="settings.maxBackupCount" :max="50" :min="1"/>
        </el-form-item>
      </el-form>

      <div class="status-text">
        最近执行时间：{{ formatTimestamp(settings.lastRunAt) }}
      </div>
      <div class="status-text">
        最近执行状态：{{ settings.lastRunStatus || 'idle' }}
      </div>
    </el-card>

    <el-card class="section-card" shadow="never">
      <template #header>
        <div class="card-header">
          <span>备份包预览与恢复</span>
          <el-button :disabled="!packagePreview" :loading="restoring" type="danger" @click="restoreBackup">执行恢复
          </el-button>
        </div>
      </template>

      <div v-if="packagePath" class="status-text">当前备份包：{{ packagePath }}</div>
      <div v-if="packagePreview" class="preview-grid">
        <div class="metric-item">
          <div class="metric-label">备份时间</div>
          <div class="metric-value small">{{ formatTimestamp(packagePreview.manifest.createdAt) }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">应用版本</div>
          <div class="metric-value">{{ packagePreview.manifest.appVersion }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">文字历史</div>
          <div class="metric-value">{{ packagePreview.stats.textItemCount }}</div>
        </div>
        <div class="metric-item">
          <div class="metric-label">图片历史</div>
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
          <el-radio-button label="full">全量恢复</el-radio-button>
          <el-radio-button label="partial">选择性恢复</el-radio-button>
        </el-radio-group>

        <div v-if="restoreMode === 'partial'" class="checkbox-group">
          <el-checkbox v-model="restoreSettings" :disabled="!packagePreview.restoreOptions.canRestoreSettings">
            恢复设置
          </el-checkbox>
          <el-checkbox v-model="restoreTextHistory" :disabled="!packagePreview.restoreOptions.canRestoreTextHistory">
            恢复文字历史
          </el-checkbox>
          <el-checkbox v-model="restoreImageHistory" :disabled="!packagePreview.restoreOptions.canRestoreImageHistory">
            恢复图片历史
          </el-checkbox>
        </div>
      </div>
    </el-card>

    <el-card class="section-card" shadow="never">
      <template #header>
        <div class="card-header">
          <span>最近备份</span>
          <el-button :loading="historyLoading" @click="loadHistory">刷新列表</el-button>
        </div>
      </template>

      <el-empty v-if="!history.length" description="当前还没有备份记录"/>
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
            <el-button size="small" @click="previewHistoryItem(item.filePath)">预览</el-button>
            <el-button size="small" type="danger" @click="removeHistoryItem(item.filePath)">删除</el-button>
          </div>
        </div>
      </div>
    </el-card>
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, reactive, ref} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {open, save} from '@tauri-apps/plugin-dialog'
import {listen} from '@tauri-apps/api/event'
import {BackupService} from '../../../services/ipc'

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
  if (!timestamp) return '未执行'
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
    ElMessage.success('自动备份配置已保存')
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
    title: '选择自动备份目录'
  })
  if (typeof path === 'string') {
    settings.targetDir = path
  }
}

const exportBackup = async () => {
  exporting.value = true
  try {
    const targetPath = await save({
      title: '导出备份',
      defaultPath: defaultExportName(),
      filters: [{name: 'Fuyun Backup', extensions: ['zip']}]
    })
    if (!targetPath) return
    const response = await BackupService.exportToPath(targetPath)
    lastResult.value = `导出成功：${response.data.filePath}`
    ElMessage.success('备份导出成功')
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
    lastResult.value = `手动备份成功：${response.data.filePath}`
    ElMessage.success('手动备份完成')
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
    title: '选择备份包',
    filters: [{name: 'Fuyun Backup', extensions: ['zip']}]
  })
  if (typeof path === 'string') {
    await previewHistoryItem(path)
  }
}

const restoreBackup = async () => {
  if (!packagePath.value || !packagePreview.value) return
  if (restoreMode.value === 'partial' && !restoreSettings.value && !restoreTextHistory.value && !restoreImageHistory.value) {
    ElMessage.warning('请选择至少一个恢复模块')
    return
  }
  await ElMessageBox.confirm(
      '恢复会覆盖所选模块，并自动创建本地回滚点。API Key 不会自动恢复。',
      '确认恢复',
      {type: 'warning'}
  )
  restoring.value = true
  try {
    const response = await BackupService.restorePackage({
      packagePath: packagePath.value,
      mode: restoreMode.value,
      restoreSettings: restoreMode.value === 'full' ? true : restoreSettings.value,
      restoreTextHistory: restoreMode.value === 'full' ? true : restoreTextHistory.value,
      restoreImageHistory: restoreMode.value === 'full' ? true : restoreImageHistory.value,
      createRollbackPoint: true
    })
    lastResult.value = `恢复完成：${response.message}`
    ElMessage.success('备份恢复完成')
    await Promise.all([loadExportPreview(), loadSettings(), loadHistory()])
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    restoring.value = false
  }
}

const removeHistoryItem = async (filePath) => {
  await ElMessageBox.confirm('删除后无法恢复，确定继续吗？', '删除备份', {type: 'warning'})
  try {
    await BackupService.deleteHistoryItem(filePath)
    ElMessage.success('备份文件已删除')
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
      lastResult.value = '自动备份已执行完成'
    } else if (payload.status === 'failed') {
      lastResult.value = `自动备份失败：${String(payload.message || '未知错误')}`
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
  color: var(--el-text-color-secondary);
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
  color: var(--el-text-color-secondary);
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
  color: var(--el-text-color-secondary);
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
