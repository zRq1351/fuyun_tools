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
        <div class="section-title">VC Runtime 调试</div>
      </template>
      <el-form-item label="强制模拟缺失（仅开发模式）">
        <el-switch v-model="vcRuntimeDebug.forceMissing"/>
      </el-form-item>
      <el-form-item>
        <el-button size="small" type="primary" @click="saveVcRuntimeDebugConfig">保存配置</el-button>
        <el-button size="small" @click="refreshVcRuntimeDebugState">刷新状态</el-button>
      </el-form-item>
      <el-form-item>
        <div class="metrics-card">
          <div class="metrics-line">当前状态：{{ vcRuntimeDebug.forceMissing ? '强制缺失' : '真实检测' }}</div>
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
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">图片持久化队列</div>
      </template>
      <el-form-item>
        <el-button size="small" @click="refreshImagePersistQueueMetrics">刷新队列指标</el-button>
      </el-form-item>
      <el-form-item>
        <div class="metrics-card">
          <div class="metrics-line">队列容量 {{ Number(imagePersistQueueMetrics.queueSize || 0) }}</div>
          <div class="metrics-line">发送超时 {{ Number(imagePersistQueueMetrics.sendTimeoutMs || 0) }}ms</div>
          <div class="metrics-line">重试间隔 {{ Number(imagePersistQueueMetrics.retryIntervalMs || 0) }}ms</div>
          <div class="metrics-line">满队次数 {{ Number(imagePersistQueueMetrics.fullCount || 0) }}</div>
          <div class="metrics-line">超时丢弃 {{ Number(imagePersistQueueMetrics.timeoutDropCount || 0) }}</div>
          <div class="metrics-line">累计等待 {{ Number(imagePersistQueueMetrics.waitMsTotal || 0) }}ms</div>
          <div class="metrics-line">平均等待 {{ imagePersistQueueMetrics.avgWaitMs }}</div>
        </div>
      </el-form-item>
    </el-card>
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">录制调试</div>
      </template>
      <el-form-item label="强制模拟 WGC 失败（一键降级为 FFmpeg 窗口模式）">
        <el-switch v-model="recordingDebug.forceFfmpegFallback" @change="saveRecordingDebugConfig"/>
        <div class="form-hint">开启后，窗口录制将直接使用 FFmpeg gdigrab 模式，跳过原生 WGC 捕获。</div>
      </el-form-item>
    </el-card>
  </el-form>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {ElMessage} from 'element-plus'
import {AISettingsService} from '@/services/ipc.js'

const imageStorageMetrics = ref({})
const vcRuntimeDebug = ref({
  forceMissing: false
})
const recordingDebug = ref({
  forceFfmpegFallback: false
})
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
const imagePersistQueueMetrics = ref({
  queueSize: 0,
  sendTimeoutMs: 0,
  retryIntervalMs: 0,
  fullCount: 0,
  timeoutDropCount: 0,
  waitMsTotal: 0,
  avgWaitMs: '0.0ms'
})
let metricsTimer = null

const refreshImageStorageMetrics = async () => {
  const metrics = await AISettingsService.getImageStorageMetrics()
  imageStorageMetrics.value = metrics || {}
}

const refreshImagePersistQueueMetrics = async () => {
  const metrics = await AISettingsService.getImagePersistQueueMetrics()
  imagePersistQueueMetrics.value = {
    queueSize: Number(metrics?.queue_size || 0),
    sendTimeoutMs: Number(metrics?.send_timeout_ms || 0),
    retryIntervalMs: Number(metrics?.retry_interval_ms || 0),
    fullCount: Number(metrics?.full_count || 0),
    timeoutDropCount: Number(metrics?.timeout_drop_count || 0),
    waitMsTotal: Number(metrics?.wait_ms_total || 0),
    avgWaitMs: `${Number(metrics?.avg_wait_ms || 0).toFixed(1)}ms`
  }
}

const refreshVcRuntimeDebugState = async () => {
  const state = await AISettingsService.getVcRuntimeDebugState()
  vcRuntimeDebug.value = {
    forceMissing: !!state?.forceMissing
  }
}

const saveVcRuntimeDebugConfig = async () => {
  const state = await AISettingsService.setVcRuntimeDebugConfig({
    forceMissing: !!vcRuntimeDebug.value.forceMissing
  })
  vcRuntimeDebug.value = {
    forceMissing: !!state?.forceMissing
  }
  ElMessage.success('VC Runtime 调试配置已保存')
}

const refreshRecordingDebugState = async () => {
  const settings = await AISettingsService.getSettings()
  recordingDebug.value.forceFfmpegFallback = settings.dev_force_ffmpeg_window_capture === true
}

const saveRecordingDebugConfig = async (val) => {
  await AISettingsService.savePartialSettings({
    devForceFfmpegWindowCapture: val
  })
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
  await refreshVcRuntimeDebugState()
  await refreshImagePersistQueueMetrics()
  await refreshRecordingDebugState()
  await refreshDedupState()
  metricsTimer = setInterval(async () => {
    await refreshImageStorageMetrics()
    await refreshVcRuntimeDebugState()
    await refreshImagePersistQueueMetrics()
    await refreshRecordingDebugState()
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
