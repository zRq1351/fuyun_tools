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
              <el-button :type="isRecordingHotkeyRecording ? 'danger' : 'primary'" @click="toggleRecordingHotkey">
                <el-icon>
                  <component :is="isRecordingHotkeyRecording ? VideoPause : Edit"/>
                </el-icon>
              </el-button>
            </template>
          </el-input>
        </el-form-item>
      </div>

      <div class="setting-group">
        <div class="group-title">音频来源</div>
        <div class="group-grid">
          <el-form-item label="系统音频">
            <el-select
                :model-value="systemAudioSelectValue"
                placeholder="选择系统音频设备"
                @change="onSystemAudioDeviceChange"
            >
              <el-option label="不捕获系统音频" value=""/>
              <el-option
                  v-for="item in systemOutputs"
                  :key="item.id"
                  :label="item.name"
                  :value="item.id"
              />
            </el-select>
          </el-form-item>
          <el-form-item label="麦克风">
            <el-select
                :model-value="microphoneSelectValue"
                placeholder="选择麦克风设备"
                @change="onMicrophoneDeviceChange"
            >
              <el-option label="不捕获麦克风" value=""/>
              <el-option
                  v-for="item in microphones"
                  :key="item.id"
                  :label="item.name"
                  :value="item.id"
              />
            </el-select>
          </el-form-item>
        </div>
      </div>

      <div class="setting-group">
        <div class="group-title">录制质量</div>
        <div class="group-grid quality-grid">
          <el-form-item label="默认帧率">
            <el-input-number v-model="form.recordingDefaultFps" :max="120" :min="1"/>
          </el-form-item>
          <el-form-item label="视频码率 (kbps)">
            <el-input-number v-model="form.recordingDefaultVideoBitrateKbps" :max="50000" :min="500" :step="500"/>
          </el-form-item>
          <el-form-item label="音频码率 (kbps)">
            <el-input-number v-model="form.recordingDefaultAudioBitrateKbps" :max="512" :min="32" :step="16"/>
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
          <el-form-item label="录制选项">
            <el-switch v-model="form.recordingCaptureCursor" active-text="捕获鼠标"/>
          </el-form-item>
        </div>
        <el-form-item label="工具栏内容保护">
          <el-switch v-model="form.recordingToolbarContentProtected" active-text="开启" inactive-text="关闭"/>
          <div class="form-hint">开启后录屏工具栏窗口将尝试禁止被录制与截屏</div>
        </el-form-item>
      </div>
    </el-card>
  </el-form>
</template>


<script setup>
import {computed, onMounted, ref} from 'vue'
import {Edit, FolderOpened, VideoPause} from '@element-plus/icons-vue'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {RecordingService} from '../../../services/ipc'
import {open} from '@tauri-apps/plugin-dialog'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const {
  isRecording: isRecordingHotkeyRecording,
  currentDisplayValue: recordingDisplayValue,
  toggleRecording: toggleRecordingHotkey
} = useShortcutRecorder(props.form, 'recordingToggleShortcut')

const systemOutputs = ref([])
const microphones = ref([])
const systemOutputId = ref('')
const effectiveOutputDir = ref('')

const systemAudioSelectValue = computed(() => {
  return props.form.recordingCaptureSystemAudio ? (systemOutputId.value || '') : ''
})

const microphoneSelectValue = computed(() => {
  return props.form.recordingCaptureMicrophone ? (props.form.recordingMicrophoneDeviceId || '') : ''
})

const effectiveOutputDirDisplay = computed(() => {
  if (props.form.recordingOutputDir && props.form.recordingOutputDir.trim().length > 0) {
    return props.form.recordingOutputDir.trim()
  }
  return effectiveOutputDir.value
})

const onSystemAudioDeviceChange = (deviceId) => {
  const id = String(deviceId || '')
  systemOutputId.value = id
  props.form.recordingCaptureSystemAudio = id.length > 0
}

const onMicrophoneDeviceChange = (deviceId) => {
  const id = String(deviceId || '')
  props.form.recordingCaptureMicrophone = id.length > 0
  props.form.recordingMicrophoneDeviceId = id
}

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

  try {
    const outputs = await RecordingService.listSystemOutputs()
    systemOutputs.value = Array.isArray(outputs) ? outputs : []
    if (props.form.recordingCaptureSystemAudio) {
      const preferred = systemOutputs.value.find((item) => item.isDefault)?.id || systemOutputs.value[0]?.id || ''
      systemOutputId.value = preferred
      props.form.recordingCaptureSystemAudio = preferred.length > 0
    } else {
      systemOutputId.value = ''
    }
  } catch (e) {
    ElMessage.error(`加载系统音频设备失败: ${String(e)}`)
  }

  try {
    const mics = await RecordingService.listAudioDevices()
    microphones.value = Array.isArray(mics) ? mics : []
    if (props.form.recordingCaptureMicrophone && !props.form.recordingMicrophoneDeviceId) {
      props.form.recordingMicrophoneDeviceId =
          microphones.value.find((item) => item.isDefault)?.id || microphones.value[0]?.id || ''
      props.form.recordingCaptureMicrophone = props.form.recordingMicrophoneDeviceId.length > 0
    }
  } catch (e) {
    ElMessage.error(`加载麦克风设备失败: ${String(e)}`)
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
