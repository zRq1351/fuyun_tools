<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-card" shadow="never">
      <template #header>
        <div class="section-title">录屏设置</div>
      </template>
      <div class="setting-group">
        <div class="group-title">基础控制</div>
        <el-form-item label="录屏功能">
          <el-switch v-model="form.recordingEnabled" active-text="启用" inactive-text="停用"/>
          <div class="form-hint">停用后后端不再注册录屏快捷键，也不会响应录制命令</div>
        </el-form-item>
        <el-form-item label="开始/停止录屏快捷键">
          <el-input
              :class="{ recording: isRecordingHotkeyRecording }"
              :model-value="recordingDisplayValue"
              placeholder="例如: Alt+R"
              readonly
          >
            <template #append>
              <el-button :type="isRecordingHotkeyRecording ? 'danger' : 'primary'" @click="toggleRecordingHotkey" title="修改快捷键">
                <el-icon>
                  <component :is="isRecordingHotkeyRecording ? VideoPause : Edit"/>
                </el-icon>
              </el-button>
              <el-button @click="resetRecordingHotkey" title="恢复默认快捷键">
                <el-icon><RefreshLeft /></el-icon>
              </el-button>
            </template>
          </el-input>
        </el-form-item>
        <el-form-item label="麦克风切换快捷键">
          <el-input
              :class="{ recording: isMicToggleHotkeyRecording }"
              :model-value="micToggleDisplayValue"
              placeholder="例如: Ctrl+Space"
              readonly
          >
            <template #append>
              <el-button :type="isMicToggleHotkeyRecording ? 'danger' : 'primary'" @click="toggleMicToggleHotkey" title="修改快捷键">
                <el-icon>
                  <component :is="isMicToggleHotkeyRecording ? VideoPause : Edit"/>
                </el-icon>
              </el-button>
              <el-button @click="resetMicToggleHotkey" title="恢复默认快捷键">
                <el-icon><RefreshLeft /></el-icon>
              </el-button>
            </template>
          </el-input>
          <div class="form-hint">录制过程中使用该快捷键快速开启/关闭麦克风</div>
        </el-form-item>
      </div>

      <div class="setting-group">
        <div class="group-title">录制校准</div>
        <div class="group-grid quality-grid">
          <el-form-item label="窗口录制音频同步补偿 (ms)">
            <el-input-number v-model="form.recordingWindowAudioSyncAdvanceMs" :max="500" :min="0" :step="5"/>
            <div class="form-hint">仅窗口录制（WGC）生效；值越大表示音频越向前对齐。</div>
          </el-form-item>
          <el-form-item label="最长录制时长 (分钟)">
            <el-input-number v-model="form.recordingMaxDurationMinutes" :max="1440" :min="1"/>
          </el-form-item>
        </div>
      </div>

      <div class="setting-group">
        <div class="group-title">输出与保护</div>
        <el-form-item label="输出目录">
          <el-input v-model="effectiveOutputDirDisplay" readonly>
            <template #append>
              <el-tooltip content="选择目录" placement="top">
                <el-button @click="selectOutputDir">
                  <el-icon>
                    <FolderOpened/>
                  </el-icon>
                </el-button>
              </el-tooltip>
            </template>
          </el-input>
          <div class="form-hint">
            选择目录会覆盖默认输出目录；留空时使用程序目录下 recordings。
            <el-link type="primary" @click="openOutputDir">打开当前目录</el-link>
          </div>
        </el-form-item>
        <div class="switch-row">
          <el-form-item label="录制完成自动打开目录">
            <el-switch v-model="form.recordingAutoOpenFolder" active-text="开启" inactive-text="关闭"/>
          </el-form-item>
        </div>
      </div>
    </el-card>
  </el-form>
</template>


<script setup>
import {computed, onMounted, ref} from 'vue'
import {Edit, FolderOpened, VideoPause, RefreshLeft} from '@element-plus/icons-vue'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {RecordingService} from '../../../services/ipc'
import {open} from '@tauri-apps/plugin-dialog'
import {ElMessage} from 'element-plus'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const {
  isRecording: isRecordingHotkeyRecording,
  currentDisplayValue: recordingDisplayValue,
  toggleRecording: toggleRecordingHotkey,
  stopRecording: stopRecordingHotkeyRecording
} = useShortcutRecorder(props.form, 'recordingToggleShortcut')

const {
  isRecording: isMicToggleHotkeyRecording,
  currentDisplayValue: micToggleDisplayValue,
  toggleRecording: toggleMicToggleHotkey,
  stopRecording: stopMicToggleHotkeyRecording
} = useShortcutRecorder(props.form, 'recordingMicToggleShortcut')

const resetRecordingHotkey = () => {
  stopRecordingHotkeyRecording()
  props.form.recordingToggleShortcut = 'Alt+R'
  ElMessage.success('已恢复录屏快捷键默认值: Alt+R')
}

const resetMicToggleHotkey = () => {
  stopMicToggleHotkeyRecording()
  props.form.recordingMicToggleShortcut = 'Ctrl+Space'
  ElMessage.success('已恢复麦克风切换快捷键默认值: Ctrl+Space')
}

const effectiveOutputDir = ref('')

const effectiveOutputDirDisplay = computed(() => {
  if (props.form.recordingOutputDir && props.form.recordingOutputDir.trim().length > 0) {
    return props.form.recordingOutputDir.trim()
  }
  return effectiveOutputDir.value
})

const openOutputDir = async () => {
  try {
    await RecordingService.openFolder()
  } catch (e) {
    ElMessage.error(`打开输出目录失败: ${String(e)}`)
  }
}

const selectOutputDir = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false
    })
    if (!selected) return
    props.form.recordingOutputDir = String(selected)
  } catch (e) {
    ElMessage.error(`选择输出目录失败: ${String(e)}`)
  }
}

onMounted(async () => {
  try {
    effectiveOutputDir.value = await RecordingService.getOutputDir()
  } catch (e) {
    effectiveOutputDir.value = ''
    ElMessage.error(`加载输出目录失败: ${String(e)}`)
  }
})
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: #909399;
  margin-top: 4px;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
}

.compact-card :deep(.el-card__body) {
  max-width: 860px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.setting-group {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
  padding: 12px 12px 6px;
  background: var(--el-bg-color);
}

.group-title {
  font-size: 13px;
  font-weight: 700;
  color: var(--el-text-color-secondary);
  margin-bottom: 10px;
}

.group-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(260px, 1fr));
  column-gap: 14px;
}

.quality-grid {
  grid-template-columns: repeat(2, minmax(220px, 1fr));
}

.setting-group :deep(.el-form-item) {
  margin-bottom: 10px;
}

.switch-row {
  display: grid;
  grid-template-columns: repeat(2, minmax(260px, 1fr));
  column-gap: 14px;
}

.recording :deep(.el-input__inner) {
  color: #f56c6c !important;
}

@media (max-width: 900px) {
  .group-grid,
  .quality-grid,
  .switch-row {
    grid-template-columns: 1fr;
  }

}
</style>
