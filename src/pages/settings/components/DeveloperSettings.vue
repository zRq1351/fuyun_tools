<template>
  <el-form label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.developer.clipboardStorage') }}</div>
      </template>
      <el-form-item>
        <el-button size="small" @click="refreshImageStorageMetrics">{{
            $t('settings.developer.refreshUsage')
          }}
        </el-button>
      </el-form-item>
      <el-form-item>
        <div class="metrics-card">
          <div class="metrics-line">{{ $t('settings.developer.memoryCache') }}
            {{ formatBytes(imageStorageMetrics.memory_bytes) }} /
            {{ formatBytes(imageStorageMetrics.memory_budget_bytes) }}
          </div>
          <div class="metrics-line">{{ $t('settings.developer.diskUsage') }}
            {{ formatBytes(imageStorageMetrics.disk_bytes) }} /
            {{ formatBytes(imageStorageMetrics.disk_limit_bytes) }}
          </div>
          <div class="metrics-line">{{ $t('settings.developer.imageEntries') }}
            {{ Number(imageStorageMetrics.item_count || 0) }} {{ $t('settings.developer.pinned') }}
            {{ Number(imageStorageMetrics.pinned_count || 0) }}
          </div>
        </div>
      </el-form-item>
    </el-card>
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.developer.vcRuntimeDebug') }}</div>
      </template>
      <el-form-item :label="$t('settings.developer.forceMissing')">
        <el-switch v-model="vcRuntimeDebug.forceMissing"/>
      </el-form-item>
      <el-form-item>
        <el-button size="small" type="primary" @click="saveVcRuntimeDebugConfig">{{
            $t('settings.developer.saveConfig')
          }}
        </el-button>
        <el-button size="small" @click="refreshVcRuntimeDebugState">{{
            $t('settings.developer.refreshStatus')
          }}
        </el-button>
      </el-form-item>
      <el-form-item>
        <div class="metrics-card">
          <div class="metrics-line">{{
              $t('settings.developer.currentStatus')
            }}{{
              vcRuntimeDebug.forceMissing ? $t('settings.developer.forceMissingStatus') : $t('settings.developer.realDetection')
            }}
          </div>
        </div>
      </el-form-item>
    </el-card>
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.developer.writebackDedup') }}</div>
      </template>
      <el-form-item :label="$t('settings.developer.dedupToggle')">
        <el-switch v-model="dedupConfig.enabled"/>
      </el-form-item>
      <el-form-item :label="$t('settings.developer.dedupWindow')">
        <el-input-number
            v-model="dedupConfig.windowMs"
            :max="10000"
            :min="50"
            :step="50"
            controls-position="right"
        />
      </el-form-item>
      <el-form-item :label="$t('settings.developer.logToggle')">
        <el-switch v-model="dedupConfig.logEnabled"/>
      </el-form-item>
      <el-form-item>
        <el-button size="small" type="primary" @click="saveDedupConfig">{{
            $t('settings.developer.saveConfig')
          }}
        </el-button>
        <el-button size="small" @click="refreshDedupState">{{ $t('settings.developer.refreshStatus') }}</el-button>
        <el-button size="small" @click="resetDedupMetrics">{{ $t('settings.developer.resetCount') }}</el-button>
      </el-form-item>
      <el-form-item>
        <div class="metrics-card">
          <div class="metrics-line">{{ $t('settings.developer.totalRequests') }} {{ dedupMetrics.totalRequests }}</div>
          <div class="metrics-line">{{ $t('settings.developer.totalHits') }} {{ dedupMetrics.dedupHits }}</div>
          <div class="metrics-line">{{ $t('settings.developer.requestIdHits') }} {{ dedupMetrics.requestIdHits }}</div>
          <div class="metrics-line">{{ $t('settings.developer.textHashHits') }} {{ dedupMetrics.textHashHits }}</div>
          <div class="metrics-line">{{ $t('settings.developer.windowRequests') }} {{
              dedupMetrics.windowRequests
            }}
          </div>
          <div class="metrics-line">{{ $t('settings.developer.windowHits') }} {{ dedupMetrics.windowHits }}</div>
          <div class="metrics-line">{{ $t('settings.developer.windowHitRate') }} {{ dedupMetrics.windowHitRate }}</div>
          <div class="metrics-line">{{ $t('settings.developer.lastHitTime') }} {{ dedupMetrics.lastHitAt }}</div>
          <div class="metrics-line">{{ $t('settings.developer.logCount') }} {{ dedupMetrics.logCount }}</div>
        </div>
      </el-form-item>
    </el-card>
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.developer.imageQueue') }}</div>
      </template>
      <el-form-item>
        <el-button size="small" @click="refreshImagePersistQueueMetrics">{{
            $t('settings.developer.refreshQueueMetrics')
          }}
        </el-button>
      </el-form-item>
      <el-form-item>
        <div class="metrics-card">
          <div class="metrics-line">{{ $t('settings.developer.queueCapacity') }}
            {{ Number(imagePersistQueueMetrics.queueSize || 0) }}
          </div>
          <div class="metrics-line">{{ $t('settings.developer.sendTimeout') }}
            {{ Number(imagePersistQueueMetrics.sendTimeoutMs || 0) }}ms
          </div>
          <div class="metrics-line">{{ $t('settings.developer.retryInterval') }}
            {{ Number(imagePersistQueueMetrics.retryIntervalMs || 0) }}ms
          </div>
          <div class="metrics-line">{{ $t('settings.developer.fullQueueCount') }}
            {{ Number(imagePersistQueueMetrics.fullCount || 0) }}
          </div>
          <div class="metrics-line">{{ $t('settings.developer.timeoutDrop') }}
            {{ Number(imagePersistQueueMetrics.timeoutDropCount || 0) }}
          </div>
          <div class="metrics-line">{{ $t('settings.developer.totalWait') }}
            {{ Number(imagePersistQueueMetrics.waitMsTotal || 0) }}ms
          </div>
          <div class="metrics-line">{{ $t('settings.developer.avgWait') }} {{
              imagePersistQueueMetrics.avgWaitMs
            }}
          </div>
        </div>
      </el-form-item>
    </el-card>
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.developer.recordingDebug') }}</div>
      </template>
      <el-form-item :label="$t('settings.developer.forceWgcFail')">
        <el-switch v-model="recordingDebug.forceFfmpegFallback" @change="saveRecordingDebugConfig"/>
        <div class="form-hint">{{ $t('settings.developer.forceWgcFailHint') }}</div>
      </el-form-item>
    </el-card>
  </el-form>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage} from 'element-plus'
import {AISettingsService} from '@/services/ipc.js'

const {t} = useI18n()

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
  lastHitAt: t('settings.developer.noHit'),
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
  ElMessage.success(t('settings.developer.vcDebugSaved'))
}

const refreshRecordingDebugState = async () => {
  const settings = await AISettingsService.getSettings()
  recordingDebug.value.forceFfmpegFallback = settings.dev_force_ffmpeg_window_capture === true
}

const saveRecordingDebugConfig = async (val) => {
  await AISettingsService.saveSettings({
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
  if (!timestampMs) return t('settings.developer.noHit')
  const date = new Date(timestampMs)
  if (Number.isNaN(date.getTime())) return t('settings.developer.noHit')
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
  ElMessage.success(t('settings.developer.dedupSaved'))
}

const resetDedupMetrics = async () => {
  const state = await AISettingsService.setCopyPasteDedupDebugConfig({resetMetrics: true})
  applyDedupState(state || {})
  ElMessage.success(t('settings.developer.dedupCleared'))
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
