<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-grid-card" shadow="never">
      <template #header>
        <div class="section-title">容量与写入策略</div>
      </template>
      <el-form-item label="文字历史记录上限">
        <el-input-number v-model="form.textMaxItems" :max="1000" :min="1"/>
        <div class="form-hint">设置文字剪贴板历史记录最大保存数量 (1-1000)</div>
      </el-form-item>

      <el-form-item label="图片历史记录上限">
        <el-input-number v-model="form.imageMaxItems" :max="1000" :min="1"/>
        <div class="form-hint">设置图片剪贴板历史记录最大保存数量 (1-1000)</div>
      </el-form-item>

      <el-form-item label="图片历史磁盘上限（MB）">
        <el-input-number v-model="form.imageDiskLimitMb" :max="102400" :min="100"/>
        <div class="form-hint">超过上限后自动清理最旧未置顶图片，建议 2048MB</div>
      </el-form-item>

      <el-form-item label="图片回填模式">
        <el-select v-model="form.imageFillVerifyMode" style="width: 220px">
          <el-option label="严格模式（写后校验）" value="strict"/>
          <el-option label="极速模式（完全不校验）" value="fast"/>
        </el-select>
        <div class="form-hint">极速模式写入系统剪贴板后立即粘贴，速度更快但成功率更依赖目标应用</div>
      </el-form-item>

      <el-form-item label="上限策略">
        <el-switch
            v-model="form.groupedItemsProtectedFromLimit"
            active-text="仅限制未分组项"
            inactive-text="限制全部项"
        />
        <div class="form-hint">开启后，已分组的文字和图片不会因上限被自动删除</div>
      </el-form-item>
    </el-card>

    <el-card class="setting-section-card compact-grid-card" shadow="never">
      <template #header>
        <div class="section-title">快捷键</div>
      </template>
      <el-form-item label="打开剪切板窗口快捷键">
        <el-input
            :model-value="textDisplayValue"
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
            :model-value="imageDisplayValue"
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
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">数据管理</div>
      </template>
      <el-form-item label="文字记录">
        <el-button class="action-button" plain type="primary" @click="clearTextHistory('unclassified_unpinned')">清除未分类未置顶</el-button>
        <el-button class="action-button" plain type="danger" @click="clearTextHistory('all')">清除全部</el-button>
      </el-form-item>
      <el-form-item label="图片记录">
        <el-button class="action-button" plain type="primary" @click="clearImageHistory('untagged_unclassified_unpinned')">清除未分类未置顶无标签
        </el-button>
        <el-button class="action-button" plain type="danger" @click="clearImageHistory('all')">清除全部</el-button>
      </el-form-item>
      <el-form-item label="导入图片">
        <el-button :loading="importingImages" class="action-button" type="primary" @click="importImageFiles">导入图片</el-button>
        <el-button :loading="importingImages" class="action-button" plain type="primary" @click="importImageFolders">导入文件夹</el-button>
      </el-form-item>
      <el-form-item v-if="importingImages || importTotal > 0">
        <div class="metrics-card">
          <div class="metrics-line">导入进度 {{ importProcessed }} / {{ importTotal }}</div>
          <div class="metrics-line">成功 {{ importImported }}，失败 {{ importFailed }}</div>
          <el-progress :percentage="importProgressPercent" :stroke-width="12" status="success"/>
        </div>
      </el-form-item>
      <div class="form-hint">“清除全部”会删除对应类型的全部历史记录，请谨慎操作。</div>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">存储占用</div>
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
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {Edit, VideoPause} from '@element-plus/icons-vue'
import {open} from '@tauri-apps/plugin-dialog'
import {listen} from '@tauri-apps/api/event'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {AISettingsService, ClipboardService, ImageClipboardService} from '../../../services/ipc'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const {
  isRecording: isTextRecording,
  currentDisplayValue: textDisplayValue,
  toggleRecording: toggleTextRecording
} = useShortcutRecorder(props.form, 'toggleShortcut')
const {
  isRecording: isImageRecording,
  currentDisplayValue: imageDisplayValue,
  toggleRecording: toggleImageRecording
} = useShortcutRecorder(props.form, 'imageToggleShortcut')

const imageStorageMetrics = ref({})
let metricsTimer = null
let unlistenImportProgress = null
const importingImages = ref(false)
const importTotal = ref(0)
const importProcessed = ref(0)
const importImported = ref(0)
const importFailed = ref(0)

const importProgressPercent = computed(() => {
  const total = Number(importTotal.value || 0)
  if (!total) return 0
  const processed = Number(importProcessed.value || 0)
  return Math.min(100, Math.max(0, Math.round((processed / total) * 100)))
})

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

const clearTextHistory = async (mode) => {
  try {
    if (mode === 'all') {
      await ElMessageBox.confirm('将清除全部文字历史记录，且不可恢复，是否继续？', '警告', {
        type: 'warning',
        confirmButtonText: '继续清除',
        cancelButtonText: '取消'
      })
    }
    const removed = await ClipboardService.clearHistory(mode)
    ElMessage.success(`已清理 ${removed} 条文字记录`)
  } catch (error) {
    if (error !== 'cancel') {
      ElMessage.error(`清理失败: ${error}`)
    }
  }
}

const clearImageHistory = async (mode) => {
  try {
    if (mode === 'all') {
      await ElMessageBox.confirm('将清除全部图片历史记录，且不可恢复，是否继续？', '警告', {
        type: 'warning',
        confirmButtonText: '继续清除',
        cancelButtonText: '取消'
      })
    }
    const removed = await ImageClipboardService.clearHistory(mode)
    ElMessage.success(`已清理 ${removed} 条图片记录`)
  } catch (error) {
    if (error !== 'cancel') {
      ElMessage.error(`清理失败: ${error}`)
    }
  }
}

const resetImportProgress = () => {
  importTotal.value = 0
  importProcessed.value = 0
  importImported.value = 0
  importFailed.value = 0
}

const runImageImport = async (paths) => {
  if (!paths || !paths.length) return
  importingImages.value = true
  resetImportProgress()
  try {
    const imported = await ImageClipboardService.importImageFiles(paths)
    ElMessage.success(`已导入 ${imported} 张图片`)
    await refreshImageStorageMetrics()
  } catch (error) {
    ElMessage.error(`导入失败: ${error}`)
  } finally {
    importingImages.value = false
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
  await runImageImport(paths)
}

const importImageFolders = async () => {
  const selected = await open({
    directory: true,
    multiple: true
  })
  if (!selected) return
  const paths = Array.isArray(selected) ? selected : [selected]
  await runImageImport(paths)
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
    } else if (payload.status === 'finish') {
      importingImages.value = false
    }
  })
  await refreshImageStorageMetrics()
  metricsTimer = setInterval(async () => {
    await refreshImageStorageMetrics()
  }, 10000)
})

onUnmounted(() => {
  document.removeEventListener('visibilitychange', handleDocumentVisibilityChange)
  if (metricsTimer) {
    clearInterval(metricsTimer)
    metricsTimer = null
  }
  if (unlistenImportProgress) {
    unlistenImportProgress()
    unlistenImportProgress = null
  }
})
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: #909399;
  margin-top: 4px;
}

.setting-section-card + .setting-section-card {
  margin-top: 16px;
}

.compact-grid-card :deep(.el-card__body) {
  display: grid;
  grid-template-columns: repeat(2, minmax(260px, 1fr));
  column-gap: 16px;
}

.compact-grid-card :deep(.el-form-item) {
  margin-bottom: 12px;
}

.action-button {
  min-width: 120px;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
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

@media (max-width: 900px) {
  .compact-grid-card :deep(.el-card__body) {
    grid-template-columns: 1fr;
  }
}
</style>
