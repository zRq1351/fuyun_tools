<template>
  <div class="diagnostic-settings">
    <el-card
        class="overview-card"
        shadow="never"
    >
      <template #header>
        <div class="card-header">
          <span>{{ $t('settings.diagnostic.logging') }}</span>
        </div>
      </template>
      <div class="logging-row">
        <div class="logging-text">
          <div class="item-title">
            {{ $t('settings.diagnostic.loggingEnabled') }}
          </div>
          <div class="form-hint">
            {{ $t('settings.diagnostic.loggingHint') }}
          </div>
        </div>
        <el-switch
            :model-value="loggingEnabled"
            @update:model-value="(val) => emit('toggleLogging', val)"
        />
      </div>
    </el-card>

    <el-card
        class="overview-card"
        shadow="never"
    >
      <template #header>
        <div class="card-header">
          <span>{{ $t('settings.diagnostic.healthOverview') }}</span>
          <el-button
              :loading="loading"
              @click="loadDiagnostics"
          >
            {{
              $t('settings.diagnostic.refreshDiagnosis')
            }}
          </el-button>
        </div>
      </template>

      <div class="overview-grid">
        <div class="overview-item">
          <div class="overview-label">
            {{ $t('settings.diagnostic.overallStatus') }}
          </div>
          <el-tag :type="statusType(overview.overallStatus)">
            {{ statusText(overview.overallStatus) }}
          </el-tag>
        </div>
        <div class="overview-item">
          <div class="overview-label">
            {{ $t('settings.diagnostic.errorItems') }}
          </div>
          <div class="overview-value error">
            {{ overview.errorCount }}
          </div>
        </div>
        <div class="overview-item">
          <div class="overview-label">
            {{ $t('settings.diagnostic.warningItems') }}
          </div>
          <div class="overview-value warning">
            {{ overview.warningCount }}
          </div>
        </div>
        <div class="overview-item">
          <div class="overview-label">
            {{ $t('settings.diagnostic.lastCheck') }}
          </div>
          <div class="overview-value small">
            {{ formatTimestamp(overview.checkedAt) }}
          </div>
        </div>
      </div>
    </el-card>

    <el-card
        class="overview-card perf-card"
        shadow="never"
    >
      <template #header>
        <div class="card-header">
          <span>{{ $t('settings.diagnostic.perfTitle') }}</span>
          <div class="perf-actions">
            <el-button
                :loading="perfLoading"
                size="small"
                @click="loadPerfDashboard"
            >
              {{ $t('settings.diagnostic.perfRefresh') }}
            </el-button>
            <el-button
                size="small"
                @click="resetPerfMetrics"
            >
              {{ $t('settings.diagnostic.perfReset') }}
            </el-button>
          </div>
        </div>
      </template>

      <div class="overview-grid perf-grid">
        <div class="overview-item">
          <div class="overview-label">{{ $t('settings.diagnostic.perfProcessMemory') }}</div>
          <div class="overview-value">{{ perf.system.processMemoryMb || 0 }} MB</div>
        </div>
        <div class="overview-item">
          <div class="overview-label">{{ $t('settings.diagnostic.perfSystemMemory') }}</div>
          <div class="overview-value">
            {{ perf.system.usedMemoryMb || 0 }} / {{ perf.system.totalMemoryMb || 0 }} MB
            （{{ (perf.system.memoryUsagePercent || 0).toFixed(1) }}%）
          </div>
        </div>
        <div class="overview-item">
          <div class="overview-label">{{ $t('settings.diagnostic.perfCpu') }}</div>
          <div class="overview-value">{{ (perf.system.cpuUsagePercent || 0).toFixed(1) }}%</div>
          <div class="form-hint">{{ $t('settings.diagnostic.perfCpuSystemHint') }}</div>
        </div>
        <div class="overview-item">
          <div class="overview-label">{{ $t('settings.diagnostic.perfProcessCpu') }}</div>
          <div class="overview-value">{{ (perf.system.processCpuUsagePercent || 0).toFixed(1) }}%</div>
          <div class="form-hint">{{ $t('settings.diagnostic.perfCpuProcessHint') }}</div>
        </div>
        <div class="overview-item">
          <div class="overview-label">{{ $t('settings.diagnostic.perfSamples') }}</div>
          <div class="overview-value">{{ perf.sampleCount }}</div>
        </div>
        <div class="overview-item">
          <div class="overview-label">{{ $t('settings.diagnostic.perfSlow') }}</div>
          <div class="overview-value warning">{{ perf.slowCount }}</div>
        </div>
        <div class="overview-item">
          <div class="overview-label">{{ $t('settings.diagnostic.perfErrors') }}</div>
          <div class="overview-value error">{{ perf.errorCount }}</div>
        </div>
      </div>

      <div
          v-if="perf.sampleCount === 0"
          class="form-hint perf-empty"
      >
        {{ $t('settings.diagnostic.perfEmpty') }}
      </div>

      <div
          v-else
          class="perf-sections"
      >
        <div
            v-for="section in perfSections"
            :key="section.key"
            class="perf-section"
        >
          <div class="item-title perf-section-title">
            {{ section.title }}
          </div>
          <div
              v-if="!section.items.length"
              class="form-hint"
          >
            {{ $t('settings.diagnostic.perfEmptySection') }}
          </div>
          <table
              v-else
              class="perf-table"
          >
            <thead>
            <tr>
              <th>{{ $t('settings.diagnostic.perfColLabel') }}</th>
              <th>{{ $t('settings.diagnostic.perfColAvg') }}</th>
              <th>{{ $t('settings.diagnostic.perfColMax') }}</th>
              <th>{{ $t('settings.diagnostic.perfColSamples') }}</th>
              <th>{{ $t('settings.diagnostic.perfColStatus') }}</th>
            </tr>
            </thead>
            <tbody>
            <tr
                v-for="item in section.items"
                :key="item.key + section.key"
            >
              <td>{{ item.label }}</td>
              <td>{{ Math.round(item.avgDurationMs) }} ms</td>
              <td>{{ item.maxDurationMs }} ms</td>
              <td>{{ item.sampleCount }}</td>
              <td>
                <el-tag
                    :type="item.lastStatus === 'error' ? 'danger' : 'info'"
                    size="small"
                >
                  {{ item.lastStatus === 'error' ? $t('settings.diagnostic.statusError') : (item.lastStatus || '—') }}
                </el-tag>
              </td>
            </tr>
            </tbody>
          </table>
        </div>
      </div>
    </el-card>

    <el-card
        v-for="item in items"
        :key="item.key"
        class="diagnostic-card"
        shadow="never"
    >
      <template #header>
        <div class="card-header">
          <div>
            <div class="item-title">
              {{ item.title }}
            </div>
            <div class="item-summary">
              {{ item.summary }}
            </div>
          </div>
          <el-tag :type="statusType(item.status)">
            {{ statusText(item.status) }}
          </el-tag>
        </div>
      </template>

      <ul class="detail-list">
        <li
            v-for="detail in item.details"
            :key="detail"
        >
          {{ detail }}
        </li>
      </ul>

      <div class="action-row">
        <el-button
            v-for="action in item.actions"
            :key="action.key"
            size="small"
            @click="handleAction(action)"
        >
          {{ action.label }}
        </el-button>
      </div>

      <div class="checked-at">
        {{ $t('settings.diagnostic.lastRefresh') }}{{ formatTimestamp(item.lastCheckedAt) }}
      </div>
    </el-card>

    <el-card
        v-if="lastActionMessage"
        class="result-card"
        shadow="never"
    >
      <template #header>
        <span>{{ $t('settings.diagnostic.lastActionResult') }}</span>
      </template>
      <div>{{ lastActionMessage }}</div>
    </el-card>
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, reactive, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage} from 'element-plus'
import {listen} from '@tauri-apps/api/event'
import {openUrl} from '@tauri-apps/plugin-opener'
import {DiagnosticService} from '../../../services/ipc'

const {t} = useI18n()

const emit = defineEmits(['navigate', 'toggleLogging'])

defineProps({
  loggingEnabled: {type: Boolean, default: false}
})

const overview = reactive({
  overallStatus: 'unknown',
  errorCount: 0,
  warningCount: 0,
  checkedAt: 0
})

const items = ref([])
const loading = ref(false)
const lastActionMessage = ref('')
const perfLoading = ref(false)
const perf = reactive({
  system: {
    processMemoryMb: 0,
    usedMemoryMb: 0,
    totalMemoryMb: 0,
    memoryUsagePercent: 0,
    cpuUsagePercent: 0,
    processCpuUsagePercent: 0
  },
  sampleCount: 0,
  slowCount: 0,
  errorCount: 0,
  startupTop: [],
  ipcTop: [],
  slowTop: []
})

const perfSections = computed(() => [
  {key: 'startup', title: t('settings.diagnostic.perfStartupTop'), items: perf.startupTop},
  {key: 'ipc', title: t('settings.diagnostic.perfIpcTop'), items: perf.ipcTop},
  {key: 'slow', title: t('settings.diagnostic.perfSlowTop'), items: perf.slowTop}
])

const loadPerfDashboard = async () => {
  perfLoading.value = true
  try {
    const data = await DiagnosticService.getPerfDashboard()
    if (!data) return
    Object.assign(perf.system, data.system || {})
    perf.sampleCount = data.sampleCount || 0
    perf.slowCount = data.slowCount || 0
    perf.errorCount = data.errorCount || 0
    perf.startupTop = data.startupTop || []
    perf.ipcTop = data.ipcTop || []
    perf.slowTop = data.slowTop || []
  } catch (error) {
    console.error('加载性能面板失败:', error)
  } finally {
    perfLoading.value = false
  }
}

const resetPerfMetrics = async () => {
  try {
    const result = await DiagnosticService.runAction('perf-metrics.reset')
    if (result?.message) {
      ElMessage.success(result.message)
    } else {
      ElMessage.success(t('settings.diagnostic.perfResetDone'))
    }
    await Promise.all([loadPerfDashboard(), loadDiagnostics()])
  } catch (error) {
    ElMessage.error(String(error?.message || error))
  }
}

const statusType = (status) => {
  if (status === 'healthy') return 'success'
  if (status === 'warning') return 'warning'
  if (status === 'error') return 'danger'
  return 'info'
}

const statusText = (status) => {
  if (status === 'healthy') return t('settings.diagnostic.statusNormal')
  if (status === 'warning') return t('settings.diagnostic.statusWarning')
  if (status === 'error') return t('settings.diagnostic.statusError')
  return t('settings.diagnostic.statusUnknown')
}

const formatTimestamp = (timestamp) => {
  if (!timestamp) return t('settings.diagnostic.notChecked')
  return new Date(Number(timestamp)).toLocaleString()
}

const loadDiagnostics = async () => {
  loading.value = true
  try {
    const [overviewResult, itemsResult] = await Promise.all([
      DiagnosticService.getOverview(),
      DiagnosticService.getItems()
    ])
    overview.overallStatus = overviewResult.overallStatus || 'unknown'
    overview.errorCount = overviewResult.errorCount || 0
    overview.warningCount = overviewResult.warningCount || 0
    overview.checkedAt = overviewResult.checkedAt || 0
    items.value = itemsResult || []
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    loading.value = false
  }
  loadPerfDashboard()
}

const scheduleRefresh = (reason = '') => {
  if (refreshTimer) {
    clearTimeout(refreshTimer)
    refreshTimer = null
  }
  refreshTimer = setTimeout(async () => {
    refreshTimer = null
    if (reason) {
      lastActionMessage.value = reason
    }
    await loadDiagnostics()
  }, 120)
}

const handleAction = async (action) => {
  try {
    const result = await DiagnosticService.runAction(action.key)
    lastActionMessage.value = result.message || t('settings.diagnostic.actionExecuted')
    if (result.externalUrl) {
      await openUrl(result.externalUrl)
    }
    if (result.navigateTo) {
      emit('navigate', result.navigateTo)
    }
    if (result.needsRefresh) {
      await loadDiagnostics()
    }
    ElMessage.success(result.message || t('settings.diagnostic.actionSuccess'))
  } catch (error) {
    ElMessage.error(String(error))
  }
}

let unlistenOverlayLifecycle = null
let unlistenWritebackResult = null
let refreshTimer = null
let disposed = false

onMounted(async () => {
  await loadDiagnostics()
  if (disposed) return
  const overlayUnlisten = await listen('overlay-window-lifecycle', (event) => {
    const payload = event.payload || {}
    scheduleRefresh(t('settings.diagnostic.overlayLifecycle', {reason: `${String(payload.label || 'unknown')} / ${String(payload.action || 'unknown')}`}))
  })
  const writebackUnlisten = await listen('writeback-result', (event) => {
    const payload = event.payload || {}
    scheduleRefresh(t('settings.diagnostic.writebackLink', {reason: `${String(payload.source || 'unknown')} / ${payload.success ? 'success' : 'failed'}`}))
  })
  // 竞态：await 期间可能已卸载
  if (disposed) {
    overlayUnlisten?.()
    writebackUnlisten?.()
    return
  }
  unlistenOverlayLifecycle = overlayUnlisten
  unlistenWritebackResult = writebackUnlisten
})

onUnmounted(() => {
  disposed = true
  if (typeof unlistenOverlayLifecycle === 'function') {
    unlistenOverlayLifecycle()
    unlistenOverlayLifecycle = null
  }
  if (typeof unlistenWritebackResult === 'function') {
    unlistenWritebackResult()
    unlistenWritebackResult = null
  }
  if (refreshTimer) {
    clearTimeout(refreshTimer)
    refreshTimer = null
  }
})
</script>

<style scoped>
.diagnostic-settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.overview-card,
.diagnostic-card,
.result-card {
  border-radius: 16px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
}

.logging-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
}

.logging-text {
  flex: 1;
}

.form-hint {
  margin-top: 4px;
  font-size: 12px;
  color: var(--fy-text-muted);
}

.overview-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 12px;
}

.overview-item {
  padding: 12px;
  border-radius: 12px;
  background: var(--el-fill-color-light);
}

.overview-label,
.item-summary,
.checked-at {
  color: var(--fy-text-muted);
}

.overview-value {
  margin-top: 4px;
  font-size: 24px;
  font-weight: 700;
}

.overview-value.small {
  font-size: 14px;
  font-weight: 500;
}

.overview-value.error {
  color: var(--el-color-danger);
}

.overview-value.warning {
  color: var(--el-color-warning);
}

.perf-actions {
  display: flex;
  gap: 8px;
}

.perf-grid {
  margin-bottom: 12px;
}

.perf-empty {
  padding: 8px 0;
}

.perf-sections {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.perf-section-title {
  margin-bottom: 8px;
}

.perf-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.perf-table th,
.perf-table td {
  padding: 6px 8px;
  border-bottom: 1px solid var(--el-border-color-lighter);
  text-align: left;
}

.perf-table th {
  color: var(--fy-text-muted);
  font-weight: 500;
}

.item-title {
  font-size: 16px;
  font-weight: 600;
}

.detail-list {
  margin: 0;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.action-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 16px;
}

.checked-at {
  margin-top: 12px;
  font-size: 12px;
}
</style>
