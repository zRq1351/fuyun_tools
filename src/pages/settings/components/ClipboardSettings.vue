<template>
  <el-form :model="form" label-position="top">
    <el-form-item label="最大历史记录数">
      <el-input-number v-model="form.maxItems" :max="1000" :min="1"/>
      <div class="form-hint">设置剪贴板历史记录的最大保存数量 (1-1000)</div>
    </el-form-item>

    <el-form-item label="上限策略">
      <el-switch
          v-model="form.groupedItemsProtectedFromLimit"
          active-text="仅限制未分组项"
          inactive-text="限制全部项"
      />
      <div class="form-hint">开启后，已分组的文字和图片不会因上限被自动删除</div>
    </el-form-item>

    <el-form-item label="打开剪切板窗口快捷键">
      <el-input
          v-model="form.toggleShortcut"
          :class="{ recording: isTextRecording }"
          placeholder="例如: Ctrl+Shift+K"
          readonly
      >
        <template #append>
          <el-button :type="isTextRecording ? 'danger' : 'primary'" @click="toggleTextRecording">
            <el-icon>
              <component :is="isTextRecording ? VideoPause : Edit"/>
            </el-icon>
          </el-button>
        </template>
      </el-input>
      <div class="form-hint">点击编辑按钮来自定义打开剪切板窗口的快捷键</div>
    </el-form-item>

    <el-form-item label="打开图片剪切板窗口快捷键">
      <el-input
          v-model="form.imageToggleShortcut"
          :class="{ recording: isImageRecording }"
          placeholder="例如: Ctrl+Shift+X"
          readonly
      >
        <template #append>
          <el-button :type="isImageRecording ? 'danger' : 'primary'" @click="toggleImageRecording">
            <el-icon>
              <component :is="isImageRecording ? VideoPause : Edit"/>
            </el-icon>
          </el-button>
        </template>
      </el-input>
      <div class="form-hint">点击编辑按钮来自定义打开图片剪切板窗口的快捷键</div>
    </el-form-item>

    <el-divider>监听性能策略</el-divider>

    <el-form-item label="最小轮询间隔（ms）">
      <el-input-number v-model="form.clipboardPollMinIntervalMs" :max="3000" :min="20"/>
    </el-form-item>
    <el-form-item label="温和轮询间隔（ms）">
      <el-input-number v-model="form.clipboardPollWarmIntervalMs" :max="8000" :min="20"/>
    </el-form-item>
    <el-form-item label="空闲轮询上限（ms）">
      <el-input-number v-model="form.clipboardPollIdleIntervalMs" :max="20000" :min="50"/>
    </el-form-item>
    <el-form-item label="最大轮询间隔（ms）">
      <el-input-number v-model="form.clipboardPollMaxIntervalMs" :max="60000" :min="100"/>
    </el-form-item>
    <el-form-item label="指标采样周期（秒）">
      <el-input-number v-model="form.clipboardPollReportIntervalSecs" :max="3600" :min="5"/>
    </el-form-item>
    <el-form-item label="性能指标日志">
      <el-switch
          v-model="form.clipboardPollMetricsEnabled"
          active-text="开启"
          inactive-text="关闭"
      />
    </el-form-item>
    <el-form-item label="指标日志级别">
      <el-select
          v-model="form.clipboardPollMetricsLogLevel"
          :disabled="!form.clipboardPollMetricsEnabled"
          style="width: 160px"
      >
        <el-option label="trace" value="trace"/>
        <el-option label="debug" value="debug"/>
        <el-option label="info" value="info"/>
        <el-option label="warn" value="warn"/>
      </el-select>
      <div class="form-hint">建议默认 info，排查问题时切到 debug 或 trace</div>
    </el-form-item>

    <template v-if="isDev">
      <el-divider>监听指标看板</el-divider>

      <el-form-item>
        <el-button size="small" @click="refreshMetrics">刷新</el-button>
        <el-button size="small" @click="exportMetrics('json')">导出 JSON</el-button>
        <el-button size="small" @click="exportMetrics('csv')">导出 CSV</el-button>
        <span class="metrics-meta">最近 {{ metricPoints.length }} 个采样点</span>
      </el-form-item>

      <el-form-item label="文本监听">
        <div class="metrics-card">
          <div class="metrics-line">wakeups/s {{ textLatest.wakeups_per_sec?.toFixed?.(2) || '0.00' }}</div>
          <div class="metrics-line">change_ratio {{ textLatest.change_ratio?.toFixed?.(3) || '0.000' }}</div>
          <div class="metrics-line">busy_skips {{ textLatest.busy_skips ?? 0 }}</div>
          <div class="sparkline">{{ textWakeupsSparkline }}</div>
        </div>
      </el-form-item>

      <el-form-item label="图片监听">
        <div class="metrics-card">
          <div class="metrics-line">wakeups/s {{ imageLatest.wakeups_per_sec?.toFixed?.(2) || '0.00' }}</div>
          <div class="metrics-line">change_ratio {{ imageLatest.change_ratio?.toFixed?.(3) || '0.000' }}</div>
          <div class="metrics-line">busy_skips {{ imageLatest.busy_skips ?? 0 }}</div>
          <div class="sparkline">{{ imageWakeupsSparkline }}</div>
        </div>
      </el-form-item>

      <el-form-item label="分钟聚合（最近60分钟）">
        <div class="metrics-card">
          <div class="metrics-line">文本高命中桶占比 {{ textAggHighRatio }}</div>
          <div class="metrics-line">图片高命中桶占比 {{ imageAggHighRatio }}</div>
        </div>
      </el-form-item>
      <el-form-item>
        <el-table :data="aggregatePoints.slice(-12)" border size="small" style="width: 100%">
          <el-table-column label="分钟" min-width="120">
            <template #default="scope">
              {{ formatMinute(scope.row.minute_epoch_ms) }}
            </template>
          </el-table-column>
          <el-table-column label="源" prop="source" width="70"/>
          <el-table-column label="samples" prop="samples" width="80"/>
          <el-table-column label="wakeups/s(avg)" min-width="120">
            <template #default="scope">
              {{ Number(scope.row.wakeups_per_sec_avg || 0).toFixed(2) }}
            </template>
          </el-table-column>
          <el-table-column label="change_ratio(avg)" min-width="130">
            <template #default="scope">
              {{ Number(scope.row.change_ratio_avg || 0).toFixed(3) }}
            </template>
          </el-table-column>
        </el-table>
      </el-form-item>

      <el-form-item label="文本入库预算观测">
        <div class="metrics-card">
          <div class="metrics-line">budget(ms) {{ Number(dedupMetrics.budget_ms_current || 0) }}</div>
          <div class="metrics-line">total_scans {{ Number(dedupMetrics.total_scans || 0) }}</div>
          <div class="metrics-line">timeout_ratio {{
              (Number(dedupMetrics.timeout_ratio || 0) * 100).toFixed(1)
            }}%
          </div>
          <div class="metrics-line">avg_elapsed(ms) {{ Number(dedupMetrics.avg_elapsed_ms || 0).toFixed(2) }}</div>
          <div class="metrics-line">avg_scanned {{ Number(dedupMetrics.avg_scanned_items || 0).toFixed(2) }}</div>
        </div>
      </el-form-item>
    </template>
  </el-form>
</template>

<script setup>
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {ElMessage} from 'element-plus'
import {Edit, VideoPause} from '@element-plus/icons-vue'
import {save} from '@tauri-apps/plugin-dialog'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {AISettingsService} from '../../../services/ipc'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})
const isDev = import.meta.env.DEV

const {
  isRecording: isTextRecording,
  toggleRecording: toggleTextRecording
} = useShortcutRecorder(props.form, 'toggleShortcut')
const {
  isRecording: isImageRecording,
  toggleRecording: toggleImageRecording
} = useShortcutRecorder(props.form, 'imageToggleShortcut')

const metricPoints = ref([])
const aggregatePoints = ref([])
const dedupMetrics = ref({})
let metricsTimer = null

const textPoints = computed(() => metricPoints.value.filter(item => item.source === 'text'))
const imagePoints = computed(() => metricPoints.value.filter(item => item.source === 'image'))
const textAggPoints = computed(() => aggregatePoints.value.filter(item => item.source === 'text'))
const imageAggPoints = computed(() => aggregatePoints.value.filter(item => item.source === 'image'))
const textLatest = computed(() => textPoints.value[textPoints.value.length - 1] || {})
const imageLatest = computed(() => imagePoints.value[imagePoints.value.length - 1] || {})

const computeHighRatio = (rows) => {
  if (!rows.length) return '0.0%'
  const high = rows.reduce((acc, item) => acc + Number(item.hit_bucket_high || 0), 0)
  const total = rows.reduce((acc, item) => acc + Number(item.samples || 0), 0)
  if (!total) return '0.0%'
  return `${((high / total) * 100).toFixed(1)}%`
}
const textAggHighRatio = computed(() => computeHighRatio(textAggPoints.value.slice(-60)))
const imageAggHighRatio = computed(() => computeHighRatio(imageAggPoints.value.slice(-60)))

const buildSparkline = (values) => {
  if (!values.length) return '暂无数据'
  const chars = '▁▂▃▄▅▆▇█'
  let min = Math.min(...values)
  let max = Math.max(...values)
  if (min === max) {
    min = 0
    max = max || 1
  }
  return values.map(v => {
    const ratio = (v - min) / (max - min)
    const idx = Math.min(chars.length - 1, Math.max(0, Math.round(ratio * (chars.length - 1))))
    return chars[idx]
  }).join('')
}

const textWakeupsSparkline = computed(() =>
    buildSparkline(textPoints.value.slice(-30).map(item => Number(item.wakeups_per_sec || 0)))
)

const imageWakeupsSparkline = computed(() =>
    buildSparkline(imagePoints.value.slice(-30).map(item => Number(item.wakeups_per_sec || 0)))
)

const refreshMetrics = async () => {
  const [points, aggregate, dedup] = await Promise.all([
    AISettingsService.getPollMetricsHistory(240),
    AISettingsService.getPollMetricsMinuteAggregates(60),
    AISettingsService.getTextDedupMetrics()
  ])
  metricPoints.value = Array.isArray(points) ? points : []
  aggregatePoints.value = Array.isArray(aggregate) ? aggregate : []
  dedupMetrics.value = dedup || {}
}

const formatMinute = (epochMs) => {
  const d = new Date(epochMs || 0)
  const hh = `${d.getHours()}`.padStart(2, '0')
  const mm = `${d.getMinutes()}`.padStart(2, '0')
  return `${hh}:${mm}`
}

const exportMetrics = async (format) => {
  try {
    const now = new Date()
    const pad = (n, len = 2) => `${n}`.padStart(len, '0')
    const stamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}_${pad(now.getMilliseconds(), 3)}`
    const fileName = `poll_metrics_${stamp}.${format}`
    const selectedPath = await save({
      defaultPath: fileName,
      filters: [
        format === 'csv'
            ? {name: 'CSV', extensions: ['csv']}
            : {name: 'JSON', extensions: ['json']}
      ]
    })
    if (!selectedPath) {
      return
    }
    const finalPath = selectedPath.toLowerCase().endsWith(`.${format}`)
        ? selectedPath
        : `${selectedPath}.${format}`
    await AISettingsService.exportPollMetricsToFile({
      format,
      limit: 720,
      filePath: finalPath
    })
    ElMessage.success(`已导出 ${format.toUpperCase()}`)
  } catch (error) {
    ElMessage.error(`导出失败: ${error}`)
  }
}

onMounted(async () => {
  if (!isDev) return
  await refreshMetrics()
  metricsTimer = setInterval(refreshMetrics, 10000)
})

onUnmounted(() => {
  if (metricsTimer) {
    clearInterval(metricsTimer)
    metricsTimer = null
  }
})
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: #909399;
  margin-top: 4px;
}

.recording :deep(.el-input__inner) {
  color: #f56c6c !important;
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

.metrics-meta {
  margin-left: 10px;
  color: #909399;
  font-size: 12px;
}

.sparkline {
  margin-top: 8px;
  font-size: 16px;
  letter-spacing: 1px;
}
</style>
