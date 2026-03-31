<template>
  <el-form label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">剪贴板存储占用</div>
      </template>
      <el-form-item>
        <el-button size="small" @click="refreshImageStorageMetrics">刷新占用</el-button>
      </el-form-item>
      <el-form-item>
        <div class="metrics-card">
          <div class="metrics-line">内存缓存 {{ formatBytes(imageStorageMetrics.memory_bytes) }} /
            {{ formatBytes(imageStorageMetrics.memory_budget_bytes) }}
          </div>
          <div class="metrics-line">磁盘占用 {{ formatBytes(imageStorageMetrics.disk_bytes) }} /
            {{ formatBytes(imageStorageMetrics.disk_limit_bytes) }}
          </div>
          <div class="metrics-line">图片条目 {{ Number(imageStorageMetrics.item_count || 0) }}（置顶
            {{ Number(imageStorageMetrics.pinned_count || 0) }}）
          </div>
        </div>
      </el-form-item>
    </el-card>
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">回写去重调优</div>
      </template>
      <el-form-item label="去重开关">
        <el-switch v-model="dedupConfig.enabled"/>
      </el-form-item>
      <el-form-item label="去重窗口（毫秒）">
        <el-input-number
            v-model="dedupConfig.windowMs"
            :max="10000"
            :min="50"
            :step="50"
            controls-position="right"
        />
      </el-form-item>
      <el-form-item label="日志开关">
        <el-switch v-model="dedupConfig.logEnabled"/>
      </el-form-item>
      <el-form-item>
        <el-button size="small" type="primary" @click="saveDedupConfig">保存配置</el-button>
        <el-button size="small" @click="refreshDedupState">刷新状态</el-button>
        <el-button size="small" @click="resetDedupMetrics">清零计数</el-button>
      </el-form-item>
      <el-form-item>
        <div class="metrics-card">
          <div class="metrics-line">总请求 {{ dedupMetrics.totalRequests }}</div>
          <div class="metrics-line">命中总数 {{ dedupMetrics.dedupHits }}</div>
          <div class="metrics-line">请求ID命中 {{ dedupMetrics.requestIdHits }}</div>
          <div class="metrics-line">文本哈希命中 {{ dedupMetrics.textHashHits }}</div>
          <div class="metrics-line">时间窗口请求 {{ dedupMetrics.windowRequests }}</div>
          <div class="metrics-line">时间窗口命中 {{ dedupMetrics.windowHits }}</div>
          <div class="metrics-line">时间窗口命中率 {{ dedupMetrics.windowHitRate }}</div>
          <div class="metrics-line">最近一次命中时间 {{ dedupMetrics.lastHitAt }}</div>
          <div class="metrics-line">日志计数 {{ dedupMetrics.logCount }}</div>
        </div>
      </el-form-item>
    </el-card>
  </el-form>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {ElMessage} from 'element-plus'
import {AISettingsService} from '../../../services/ipc'

const imageStorageMetrics = ref({})
const dedupConfig = ref({
  enabled: true,
  windowMs: 1200,
  logEnabled: true
})
const dedupMetrics = ref({
  totalRequests: 0,
  dedupHits: 0,
  requestIdHits: 0,
  textHashHits: 0,
  windowRequests: 0,
  windowHits: 0,
  windowHitRate: '0.00%',
  lastHitAt: '未命中',
  logCount: 0
})
let metricsTimer = null

const refreshImageStorageMetrics = async () => {
  const metrics = await AISettingsService.getImageStorageMetrics()
  imageStorageMetrics.value = metrics || {}
}

const applyDedupState = (state) => {
  const metrics = state?.metrics || {}
  const windowRequests = Number(metrics.window_requests || 0)
  const windowHits = Number(metrics.window_hits || 0)
  const windowHitRateRaw = Number(metrics.window_hit_rate_percent || 0)
  const lastHitAtMs = Number(metrics.last_hit_at_ms || 0)
  dedupConfig.value = {
    enabled: !!state?.enabled,
    windowMs: Number(state?.window_ms ?? 1200),
    logEnabled: !!state?.log_enabled
  }
  dedupMetrics.value = {
    totalRequests: Number(metrics.total_requests || 0),
    dedupHits: Number(metrics.dedup_hits || 0),
    requestIdHits: Number(metrics.request_id_hits || 0),
    textHashHits: Number(metrics.text_hash_hits || 0),
    windowRequests,
    windowHits,
    windowHitRate: `${windowHitRateRaw.toFixed(2)}%`,
    lastHitAt: formatTimestamp(lastHitAtMs),
    logCount: Number(metrics.log_count || 0)
  }
}

const formatTimestamp = (timestampMs) => {
  if (!timestampMs) return '未命中'
  const date = new Date(timestampMs)
  if (Number.isNaN(date.getTime())) return '未命中'
  return date.toLocaleString()
}

const refreshDedupState = async () => {
  const state = await AISettingsService.getCopyPasteDedupDebugState()
  applyDedupState(state || {})
}

const saveDedupConfig = async () => {
  const state = await AISettingsService.setCopyPasteDedupDebugConfig({
    enabled: dedupConfig.value.enabled,
    windowMs: Number(dedupConfig.value.windowMs || 1200),
    logEnabled: dedupConfig.value.logEnabled
  })
  applyDedupState(state || {})
  ElMessage.success('回写去重配置已保存')
}

const resetDedupMetrics = async () => {
  const state = await AISettingsService.setCopyPasteDedupDebugConfig({resetMetrics: true})
  applyDedupState(state || {})
  ElMessage.success('回写去重计数已清零')
}

const formatBytes = (bytes) => {
  const val = Number(bytes || 0)
  if (val <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let idx = 0
  let current = val
  while (current >= 1024 && idx < units.length - 1) {
    current /= 1024
    idx += 1
  }
  return `${current.toFixed(current >= 100 || idx === 0 ? 0 : 1)} ${units[idx]}`
}

onMounted(async () => {
  await refreshImageStorageMetrics()
  await refreshDedupState()
  metricsTimer = setInterval(async () => {
    await refreshImageStorageMetrics()
    await refreshDedupState()
  }, 10000)
})

onUnmounted(() => {
  if (metricsTimer) {
    clearInterval(metricsTimer)
    metricsTimer = null
  }
})
</script>

<style scoped>
.setting-section-card + .setting-section-card {
  margin-top: 16px;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
}

.metrics-card {
  width: 100%;
  padding: 10px 12px;
  border: 1px solid var(--el-border-color-light);
  border-radius: 6px;
}

.metrics-line {
  font-size: 12px;
  line-height: 20px;
}
</style>
