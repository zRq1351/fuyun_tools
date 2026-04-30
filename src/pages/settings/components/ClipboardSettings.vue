<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">快捷键</div>
      </template>
      <div class="setting-group">
        <div class="group-grid cols-2">
          <el-form-item label="文字剪贴板功能">
            <el-switch v-model="form.textClipboardEnabled" active-text="启用" inactive-text="停用"/>
            <div class="form-hint">停用后不再监听文字剪贴板与快捷键</div>
          </el-form-item>
          <el-form-item label="图片剪贴板功能">
            <el-switch v-model="form.imageClipboardEnabled" active-text="启用" inactive-text="停用"/>
            <div class="form-hint">停用后不再监听图片剪贴板与快捷键</div>
          </el-form-item>
        </div>
        <div class="group-grid cols-2">
          <el-form-item label="打开剪贴板窗口快捷键">
            <el-input
                :model-value="textDisplayValue"
                :class="{ recording: isTextRecording }"
                placeholder="例如: Ctrl+Shift+K"
                readonly
            >
              <template #append>
                <el-button-group>
                  <el-button :type="isTextRecording ? 'danger' : 'primary'" @click="toggleTextRecording" title="修改快捷键">
                    <el-icon>
                      <component :is="isTextRecording ? VideoPause : Edit"/>
                    </el-icon>
                  </el-button>
                  <el-button @click="resetTextRecording" title="恢复默认快捷键">
                    <el-icon><RefreshLeft /></el-icon>
                  </el-button>
                </el-button-group>
              </template>
            </el-input>
          </el-form-item>
          <el-form-item label="打开图片剪贴板窗口快捷键">
            <el-input
                :model-value="imageDisplayValue"
                :class="{ recording: isImageRecording }"
                placeholder="例如: Ctrl+Shift+X"
                readonly
            >
              <template #append>
                <el-button-group>
                  <el-button :type="isImageRecording ? 'danger' : 'primary'" @click="toggleImageRecording" title="修改快捷键">
                    <el-icon>
                      <component :is="isImageRecording ? VideoPause : Edit"/>
                    </el-icon>
                  </el-button>
                  <el-button @click="resetImageRecording" title="恢复默认快捷键">
                    <el-icon><RefreshLeft /></el-icon>
                  </el-button>
                </el-button-group>
              </template>
            </el-input>
          </el-form-item>
        </div>
      </div>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">容量与写入策略</div>
      </template>
      <div class="setting-group">
        <div class="group-grid cols-3">
          <el-form-item label="文字历史记录上限">
            <el-input-number v-model="form.textMaxItems" :max="1000" :min="1"/>
            <div class="form-hint">最大保存数量 1-1000</div>
          </el-form-item>
          <el-form-item label="图片历史记录上限">
            <el-input-number v-model="form.imageMaxItems" :max="1000" :min="1"/>
            <div class="form-hint">最大保存数量 1-1000</div>
          </el-form-item>
          <el-form-item label="图片历史磁盘上限（MB）">
            <el-input-number v-model="form.imageDiskLimitMb" :max="102400" :min="100"/>
            <div class="form-hint">建议 2048MB</div>
          </el-form-item>
        </div>
        <div class="group-grid cols-2">
          <el-form-item label="图片回填模式">
            <el-select v-model="form.imageFillVerifyMode">
              <el-option label="严格模式（写后校验）" value="strict"/>
              <el-option label="极速模式（完全不校验）" value="fast"/>
            </el-select>
            <div class="form-hint">极速模式更快，但成功率更依赖目标应用</div>
          </el-form-item>
          <el-form-item label="上限策略">
            <el-switch
                v-model="form.groupedItemsProtectedFromLimit"
                active-text="仅限制未分组项"
                inactive-text="限制全部项"
            />
            <div class="form-hint">开启后，已分组内容不因上限被自动删除</div>
          </el-form-item>
        </div>
      </div>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">数据管理</div>
      </template>
      <div class="management-list">
        <div class="management-item">
          <div class="management-meta">
            <div class="management-title">文字记录</div>
            <div class="form-hint">按条件清理仅影响未分类且未置顶项；“清除全部”为危险操作</div>
          </div>
          <div class="action-row">
            <el-button class="action-button" plain type="primary" @click="clearTextHistory('unclassified_unpinned')">按条件清理</el-button>
            <el-button class="action-button" plain type="danger" @click="clearTextHistory('all')">清除全部</el-button>
          </div>
        </div>
        <div class="management-item">
          <div class="management-meta">
            <div class="management-title">图片记录</div>
            <div class="form-hint">按条件清理仅影响未分类、未置顶且无标签项；“清除全部”为危险操作</div>
          </div>
          <div class="action-row">
            <el-button class="action-button" plain type="primary" @click="clearImageHistory('untagged_unclassified_unpinned')">按条件清理</el-button>
            <el-button class="action-button" plain type="danger" @click="clearImageHistory('all')">清除全部</el-button>
          </div>
        </div>
        <div class="management-item">
          <div class="management-meta">
            <div class="management-title">导入图片</div>
            <div class="form-hint">支持导入图片文件或文件夹中的图片</div>
          </div>
          <el-input
              :model-value="importSourceDisplay"
              class="import-source-input"
              placeholder="未选择导入来源"
              readonly
          >
            <template #prepend>
              <el-tooltip content="导入图片" placement="top">
                <el-button :loading="importingImages" class="import-icon-btn" @click="importImageFiles">
                  <el-icon><Picture/></el-icon>
                </el-button>
              </el-tooltip>
            </template>
            <template #append>
              <el-tooltip content="导入目录" placement="top">
                <el-button :loading="importingImages" class="import-icon-btn" @click="importImageFolders">
                  <el-icon><FolderOpened/></el-icon>
                </el-button>
              </el-tooltip>
            </template>
          </el-input>
          <div v-if="showImportProgressCard" class="metrics-card">
            <div class="metrics-line">导入进度 {{ importProcessed }} / {{ importTotal }}</div>
            <div class="metrics-line">成功 {{ importImported }}，失败 {{ importFailed }}</div>
            <el-progress :percentage="importProgressPercent" :stroke-width="12" status="success"/>
          </div>
        </div>
      </div>
      <div class="form-hint">“清除全部”会删除对应类型的全部历史记录，请谨慎操作。</div>
    </el-card>

  </el-form>
</template>

<script setup>
import {computed, onMounted, onUnmounted, ref} from 'vue'
import {Edit, FolderOpened, Picture, RefreshLeft, VideoPause} from '@element-plus/icons-vue'
import {open} from '@tauri-apps/plugin-dialog'
import {listen} from '@tauri-apps/api/event'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {ClipboardService, ImageClipboardService} from '../../../services/ipc'
import {ElMessage, ElMessageBox} from 'element-plus'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const {
  isRecording: isTextRecording,
  currentDisplayValue: textDisplayValue,
  toggleRecording: toggleTextRecording,
  stopRecording: stopTextRecording
} = useShortcutRecorder(props.form, 'toggleShortcut')

const {
  isRecording: isImageRecording,
  currentDisplayValue: imageDisplayValue,
  toggleRecording: toggleImageRecording,
  stopRecording: stopImageRecording
} = useShortcutRecorder(props.form, 'imageToggleShortcut')

const resetTextRecording = () => {
  stopTextRecording()
  const isMac = navigator.userAgent.toLowerCase().includes('mac')
  props.form.toggleShortcut = isMac ? 'Cmd+Shift+z' : 'Ctrl+Shift+z'
  ElMessage.success(`已恢复打开剪贴板窗口快捷键默认值: ${props.form.toggleShortcut}`)
}

const resetImageRecording = () => {
  stopImageRecording()
  const isMac = navigator.userAgent.toLowerCase().includes('mac')
  props.form.imageToggleShortcut = isMac ? 'Cmd+Shift+x' : 'Ctrl+Shift+x'
  ElMessage.success(`已恢复打开图片剪贴板窗口快捷键默认值: ${props.form.imageToggleShortcut}`)
}

let unlistenImportProgress = null
const importingImages = ref(false)
const importTotal = ref(0)
const importProcessed = ref(0)
const importImported = ref(0)
const importFailed = ref(0)
let importProgressResetTimer = null
const importSourceDisplay = ref('')

const importProgressPercent = computed(() => {
  const total = Number(importTotal.value || 0)
  if (!total) return 0
  const processed = Number(importProcessed.value || 0)
  return Math.min(100, Math.max(0, Math.round((processed / total) * 100)))
})

const showImportProgressCard = computed(() => {
  if (importingImages.value) return true
  const total = Number(importTotal.value || 0)
  const processed = Number(importProcessed.value || 0)
  return total > 0 && processed < total
})

const scheduleResetImportProgress = () => {
  if (importProgressResetTimer) {
    clearTimeout(importProgressResetTimer)
    importProgressResetTimer = null
  }
  importProgressResetTimer = window.setTimeout(() => {
    resetImportProgress()
    importProgressResetTimer = null
  }, 800)
}

const clearTextHistory = async (mode) => {
  try {
    if (mode === 'all') {
      const msgBox = ElMessageBox.confirm(
          '将清除全部文字历史记录，且不可恢复，是否继续？',
          '警告',
          {
            type: 'warning',
            confirmButtonText: '继续清除',
            cancelButtonText: '取消'
          }
      )
      await msgBox
    }
    const removed = await ClipboardService.clearHistory(mode)
    ElMessage.success(`已清理 ${removed} 条文字记录`)
  } catch (error) {
    if (error === 'cancel' || error?.action === 'cancel') return
    ElMessage.error(`清理失败: ${String(error)}`)
  }
}

const clearImageHistory = async (mode) => {
  try {
    if (mode === 'all') {
      await ElMessageBox.confirm(
          '将清除全部图片历史记录，且不可恢复，是否继续？',
          '警告',
          {
            type: 'warning',
            confirmButtonText: '继续清除',
            cancelButtonText: '取消'
          }
      )
    }
    const removed = await ImageClipboardService.clearHistory(mode)
    ElMessage.success(`已清理 ${removed} 条图片记录`)
  } catch (error) {
    if (error === 'cancel' || error?.action === 'cancel') return
    ElMessage.error(`清理失败: ${String(error)}`)
  }
}

const resetImportProgress = () => {
  importTotal.value = 0
  importProcessed.value = 0
  importImported.value = 0
  importFailed.value = 0
}

const runImageImport = async (paths) => {
  if (!paths || !paths.length) return false
  importingImages.value = true
  resetImportProgress()
  try {
    const imported = await ImageClipboardService.importImageFiles(paths)
    ElMessage.success(`已导入 ${imported} 张图片`)
    return true
  } catch (error) {
    ElMessage.error(`导入失败: ${error}`)
    return false
  } finally {
    importingImages.value = false
    scheduleResetImportProgress()
  }
}

const confirmImport = async (kind, paths) => {
  let total = 0
  try {
    total = Number(await ImageClipboardService.countImportImageFiles(paths)) || 0
  } catch {
    total = 0
  }
  const summary = kind === 'folder'
      ? `已选择目录：\n${String(paths[0] || '')}\n\n预计导入 ${total} 张图片，确认开始导入吗？`
      : `已选择 ${paths.length} 个文件，预计导入 ${total} 张图片，确认开始导入吗？`
  try {
    await ElMessageBox.confirm(summary, '确认导入', {
      confirmButtonText: '确认导入',
      cancelButtonText: '取消',
      type: 'info'
    })
    return true
  } catch {
    return false
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
  const names = paths.map((item) => {
    const path = String(item || '')
    const parts = path.split(/[\\/]/).filter(Boolean)
    return parts[parts.length - 1] || path
  })
  importSourceDisplay.value = names.length > 1 ? `${names[0]} 等 ${names.length} 个文件` : (names[0] || '')
  const confirmed = await confirmImport('file', paths)
  if (!confirmed) return
  const ok = await runImageImport(paths)
  if (ok) {
    importSourceDisplay.value = ''
  }
}

const importImageFolders = async () => {
  const selected = await open({
    directory: true,
    multiple: true
  })
  if (!selected) return
  const paths = Array.isArray(selected) ? selected : [selected]
  importSourceDisplay.value = String(paths[0] || '')
  const confirmed = await confirmImport('folder', paths)
  if (!confirmed) return
  const ok = await runImageImport(paths)
  if (ok) {
    importSourceDisplay.value = ''
  }
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
      if (importProgressResetTimer) {
        clearTimeout(importProgressResetTimer)
        importProgressResetTimer = null
      }
    } else if (payload.status === 'finish') {
      importingImages.value = false
      scheduleResetImportProgress()
    }
  })
})

onUnmounted(() => {
  if (importProgressResetTimer) {
    clearTimeout(importProgressResetTimer)
    importProgressResetTimer = null
  }
  document.removeEventListener('visibilitychange', handleDocumentVisibilityChange)
  if (unlistenImportProgress) {
    unlistenImportProgress()
    unlistenImportProgress = null
  }
})
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: var(--fy-text-muted);
  margin-top: 4px;
}

.setting-section-card + .setting-section-card {
  margin-top: 16px;
}

.action-button {
  min-width: 120px;
  border-radius: 8px;
  font-weight: 600;
}

.action-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.import-source-input {
  margin-bottom: 8px;
}

.import-icon-btn {
  border: none;
  padding: 0 10px;
  min-width: auto;
}

.management-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.management-item {
  border: 1px solid var(--fy-border-light);
  border-radius: 10px;
  padding: 10px 12px;
}

.management-meta {
  margin-bottom: 8px;
}

.management-title {
  font-size: 14px;
  font-weight: 700;
  margin-bottom: 2px;
  color: var(--fy-text-primary);
}

.section-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--fy-text-primary);
}

.setting-group {
  border: 1px solid var(--fy-border-light);
  border-radius: 10px;
  padding: 12px 12px 6px;
  background: var(--fy-bg-card);
}

.group-grid {
  display: grid;
  column-gap: 14px;
}

.group-grid.cols-2 {
  grid-template-columns: repeat(2, minmax(260px, 1fr));
}

.group-grid.cols-3 {
  grid-template-columns: repeat(3, minmax(180px, 1fr));
}

.setting-group :deep(.el-input-group__append) {
  display: flex;
  flex-wrap: nowrap;
  width: auto;
}

.setting-group :deep(.el-button-group) {
  display: flex;
  flex-wrap: nowrap;
}

.setting-group :deep(.el-form-item) {
  margin-bottom: 12px;
}

.setting-group :deep(.el-form-item__label) {
  color: var(--fy-text-secondary);
}

.recording :deep(.el-input__inner) {
  color: var(--fy-danger) !important;
}

.metrics-card {
  width: 100%;
  box-sizing: border-box;
  padding: 10px 12px;
  border: 1px solid var(--fy-border-light);
  border-radius: 6px;
  overflow: hidden;
  background: var(--fy-bg-surface);
}

.metrics-line {
  font-size: 12px;
  line-height: 20px;
  color: var(--fy-text-secondary);
}

.metrics-card :deep(.el-progress) {
  width: 100%;
  max-width: 100%;
}

.metrics-meta {
  margin-left: 10px;
  color: var(--fy-text-muted);
  font-size: 12px;
}

.sparkline {
  margin-top: 8px;
  font-size: 16px;
  letter-spacing: 1px;
}

@media (max-width: 900px) {
  .group-grid.cols-2,
  .group-grid.cols-3 {
    grid-template-columns: 1fr;
  }
}
</style>
