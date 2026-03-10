<template>
  <el-form :model="form" label-position="top">
    <el-form-item label="最大历史记录数">
      <el-input-number v-model="form.maxItems" :max="1000" :min="1"/>
      <div class="form-hint">设置剪贴板历史记录的最大保存数量 (1-1000)</div>
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
  </el-form>
</template>

<script setup>
import {Edit, VideoPause} from '@element-plus/icons-vue'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const {
  isRecording: isTextRecording,
  toggleRecording: toggleTextRecording
} = useShortcutRecorder(props.form, 'toggleShortcut')
const {
  isRecording: isImageRecording,
  toggleRecording: toggleImageRecording
} = useShortcutRecorder(props.form, 'imageToggleShortcut')
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
</style>
