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
  </el-form>
</template>

<script setup>
import {onMounted, onUnmounted, ref} from 'vue'
import {AISettingsService} from '../../../services/ipc'

const imageStorageMetrics = ref({})
let metricsTimer = null

const refreshImageStorageMetrics = async () => {
  const metrics = await AISettingsService.getImageStorageMetrics()
  imageStorageMetrics.value = metrics || {}
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
  metricsTimer = setInterval(async () => {
    await refreshImageStorageMetrics()
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
