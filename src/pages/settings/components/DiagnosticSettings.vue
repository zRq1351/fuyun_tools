<template>
  <div class="diagnostic-settings">
    <el-card class="overview-card" shadow="never">
      <template #header>
        <div class="card-header">
          <span>健康总览</span>
          <el-button :loading="loading" @click="loadDiagnostics">刷新诊断</el-button>
        </div>
      </template>

      <div class="overview-grid">
        <div class="overview-item">
          <div class="overview-label">整体状态</div>
          <el-tag :type="statusType(overview.overallStatus)">{{ statusText(overview.overallStatus) }}</el-tag>
        </div>
        <div class="overview-item">
          <div class="overview-label">错误项</div>
          <div class="overview-value error">{{ overview.errorCount }}</div>
        </div>
        <div class="overview-item">
          <div class="overview-label">警告项</div>
          <div class="overview-value warning">{{ overview.warningCount }}</div>
        </div>
        <div class="overview-item">
          <div class="overview-label">最近检查</div>
          <div class="overview-value small">{{ formatTimestamp(overview.checkedAt) }}</div>
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
            <div class="item-title">{{ item.title }}</div>
            <div class="item-summary">{{ item.summary }}</div>
          </div>
          <el-tag :type="statusType(item.status)">{{ statusText(item.status) }}</el-tag>
        </div>
      </template>

      <ul class="detail-list">
        <li v-for="detail in item.details" :key="detail">{{ detail }}</li>
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

      <div class="checked-at">最近刷新：{{ formatTimestamp(item.lastCheckedAt) }}</div>
    </el-card>

    <el-card v-if="lastActionMessage" class="result-card" shadow="never">
      <template #header>
        <span>最近一次动作结果</span>
      </template>
      <div>{{ lastActionMessage }}</div>
    </el-card>
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, reactive, ref} from 'vue'
import {ElMessage} from 'element-plus'
import {listen} from '@tauri-apps/api/event'
import {openUrl} from '@tauri-apps/plugin-opener'
import {DiagnosticService} from '../../../services/ipc'

const emit = defineEmits(['navigate'])

const overview = reactive({
  overallStatus: 'unknown',
  errorCount: 0,
  warningCount: 0,
  checkedAt: 0
})

const items = ref([])
const loading = ref(false)
const lastActionMessage = ref('')
let unlistenOverlayLifecycle = null
let unlistenWritebackResult = null
let refreshTimer = null

const statusType = (status) => {
  if (status === 'healthy') return 'success'
  if (status === 'warning') return 'warning'
  if (status === 'error') return 'danger'
  return 'info'
}

const statusText = (status) => {
  if (status === 'healthy') return '正常'
  if (status === 'warning') return '警告'
  if (status === 'error') return '错误'
  return '未知'
}

const formatTimestamp = (timestamp) => {
  if (!timestamp) return '未检查'
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
    lastActionMessage.value = result.message || '动作已执行'
    if (result.externalUrl) {
      await openUrl(result.externalUrl)
    }
    if (result.navigateTo) {
      emit('navigate', result.navigateTo)
    }
    if (result.needsRefresh) {
      await loadDiagnostics()
    }
    ElMessage.success(result.message || '动作执行成功')
  } catch (error) {
    ElMessage.error(String(error))
  }
}

onMounted(async () => {
  await loadDiagnostics()
  unlistenOverlayLifecycle = await listen('overlay-window-lifecycle', (event) => {
    const payload = event.payload || {}
    scheduleRefresh(`覆盖层生命周期更新：${String(payload.label || 'unknown')} / ${String(payload.action || 'unknown')}`)
  })
  unlistenWritebackResult = await listen('writeback-result', (event) => {
    const payload = event.payload || {}
    scheduleRefresh(`回写链路更新：${String(payload.source || 'unknown')} / ${payload.success ? 'success' : 'failed'}`)
  })
})

onUnmounted(() => {
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
  color: var(--el-text-color-secondary);
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
