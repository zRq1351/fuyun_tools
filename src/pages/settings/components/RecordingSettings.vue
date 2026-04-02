<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-card" shadow="never">
      <template #header>
        <div class="section-title">录屏设置</div>
      </template>
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
      <el-form-item label="默认帧率">
        <el-input-number v-model="form.recordingDefaultFps" :max="120" :min="1"/>
      </el-form-item>
      <el-form-item label="视频码率 (kbps)">
        <el-input-number v-model="form.recordingDefaultVideoBitrateKbps" :max="50000" :min="500" :step="500"/>
      </el-form-item>
      <el-form-item label="音频码率 (kbps)">
        <el-input-number v-model="form.recordingDefaultAudioBitrateKbps" :max="512" :min="32" :step="16"/>
      </el-form-item>
      <el-form-item label="录制选项">
        <el-space wrap>
          <el-switch v-model="form.recordingCaptureCursor" active-text="捕获鼠标"/>
          <el-switch v-model="form.recordingCaptureMicrophone" active-text="捕获麦克风"/>
          <el-switch v-model="form.recordingCaptureSystemAudio" active-text="系统音频(预留)"/>
        </el-space>
      </el-form-item>
      <el-form-item label="输出目录">
        <el-input v-model="form.recordingOutputDir" placeholder="留空默认使用程序目录下 recordings"/>
      </el-form-item>
      <el-form-item label="最长录制时长 (分钟)">
        <el-input-number v-model="form.recordingMaxDurationMinutes" :max="1440" :min="1"/>
      </el-form-item>
      <el-form-item label="录制完成自动打开目录">
        <el-switch v-model="form.recordingAutoOpenFolder" active-text="开启" inactive-text="关闭"/>
      </el-form-item>
      <el-form-item label="录屏控制台">
        <el-button type="primary" @click="openRecordingConsole">打开录屏控制台</el-button>
      </el-form-item>
    </el-card>
  </el-form>
</template>

<script setup>
import {Edit, VideoPause} from '@element-plus/icons-vue'
import {ElMessage} from 'element-plus'
import {WindowService} from '../../../services/ipc'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'

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

const openRecordingConsole = async () => {
  try {
    await WindowService.openRecordingWindow()
  } catch (error) {
    ElMessage.error(String(error))
  }
}
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
  max-width: 640px;
}

.recording :deep(.el-input__inner) {
  color: #f56c6c !important;
}
</style>
