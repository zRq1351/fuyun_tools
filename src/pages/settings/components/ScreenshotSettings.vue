<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-card" shadow="never">
      <template #header>
        <div class="section-title">截图设置</div>
      </template>
      <el-form-item label="截图功能">
        <el-switch v-model="form.screenshotEnabled" active-text="启用" inactive-text="停用"/>
        <div class="form-hint">停用后后端不再注册截图快捷键，也不会执行截图命令</div>
      </el-form-item>
      <el-form-item label="打开截图窗口快捷键">
        <el-input
            :class="{ recording: isScreenshotRecording }"
            :model-value="screenshotDisplayValue"
            placeholder="例如: Ctrl+Shift+A"
            readonly
        >
          <template #append>
            <el-button :type="isScreenshotRecording ? 'danger' : 'primary'" @click="toggleScreenshotRecording" title="修改快捷键">
              <el-icon>
                <component :is="isScreenshotRecording ? VideoPause : Edit"/>
              </el-icon>
            </el-button>
            <el-button @click="resetScreenshotRecording" title="恢复默认快捷键">
              <el-icon><RefreshLeft /></el-icon>
            </el-button>
          </template>
        </el-input>
        <div class="form-hint">点击编辑按钮来自定义打开截图窗口的快捷键</div>
      </el-form-item>
    </el-card>
  </el-form>
</template>

<script setup>
import {Edit, VideoPause, RefreshLeft} from '@element-plus/icons-vue'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {ElMessage} from 'element-plus'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const {
  isRecording: isScreenshotRecording,
  currentDisplayValue: screenshotDisplayValue,
  toggleRecording: toggleScreenshotRecording,
  stopRecording: stopScreenshotRecording
} = useShortcutRecorder(props.form, 'screenshotToggleShortcut')

const resetScreenshotRecording = () => {
  stopScreenshotRecording()
  const isMac = navigator.userAgent.toLowerCase().includes('mac')
  props.form.screenshotToggleShortcut = isMac ? 'Cmd+Shift+s' : 'Ctrl+Shift+s'
  ElMessage.success(`已恢复打开截图窗口快捷键默认值: ${props.form.screenshotToggleShortcut}`)
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
  max-width: 560px;
}

.recording :deep(.el-input__inner) {
  color: #f56c6c !important;
}
</style>
