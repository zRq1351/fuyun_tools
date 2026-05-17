<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.screenshot.title') }}</div>
      </template>
      <el-form-item :label="$t('settings.screenshot.enabled')">
        <el-switch
            :active-text="pendingToggles.screenshot === 'disabling' ? $t('common.disabling') : $t('common.enable')"
            :inactive-text="pendingToggles.screenshot === 'enabling' ? $t('common.enabling') : $t('common.disable')"
            :loading="!!pendingToggles.screenshot"
            :model-value="form.screenshotEnabled"
            @update:model-value="(val) => toggleFeature('screenshotEnabled', val)"
        />
        <div class="form-hint">{{ $t('settings.screenshot.disabledHint') }}</div>
      </el-form-item>
      <el-form-item :label="$t('settings.screenshot.openWindowHotkey')">
        <el-input
            :class="{ recording: isScreenshotRecording }"
            :model-value="screenshotDisplayValue"
            :placeholder="$t('settings.clipboard.shortcutExample')"
            readonly
        >
          <template #append>
            <el-button-group>
              <el-button :title="$t('settings.clipboard.modifyShortcut')" :type="isScreenshotRecording ? 'danger' : 'primary'"
                         @click="toggleScreenshotRecording">
                <el-icon>
                  <component :is="isScreenshotRecording ? VideoPause : Edit"/>
                </el-icon>
              </el-button>
              <el-button :title="$t('settings.clipboard.resetShortcut')" @click="resetScreenshotRecording">
                <el-icon><RefreshLeft /></el-icon>
              </el-button>
            </el-button-group>
          </template>
        </el-input>
        <div class="form-hint">{{ $t('settings.screenshot.hotkeyHint') }}</div>
      </el-form-item>
      <el-form-item :label="$t('settings.screenshot.ocrEngine')">
        <el-select v-model="form.ocrEngine" :placeholder="$t('settings.screenshot.ocrSelectPlaceholder')"
                   style="width: 100%">
          <el-option :label="$t('settings.screenshot.ocrNative')" value="windows-native"/>
          <el-option :label="$t('settings.screenshot.ocrRs')" value="ocr-rs"/>
        </el-select>
        <div class="form-hint">
          <div>{{ $t('settings.screenshot.ocrNativeHint') }}</div>
          <div>{{ $t('settings.screenshot.ocrRsHint') }}</div>
        </div>
      </el-form-item>
    </el-card>
  </el-form>
</template>

<script setup>
import {ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {Edit, RefreshLeft, VideoPause} from '@element-plus/icons-vue'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {ElMessage} from 'element-plus'

const {t} = useI18n()

const props = defineProps({
  form: {
    type: Object,
    required: true
  },
  onFeatureToggle: {
    type: Function,
    default: null
  }
})

const pendingToggles = ref({})

const toggleFeature = async (fieldName, value) => {
  if (pendingToggles.value[fieldName]) return
  if (!props.onFeatureToggle) {
    props.form[fieldName] = value
    return
  }
  pendingToggles.value = {...pendingToggles.value, [fieldName]: value ? 'enabling' : 'disabling'}
  const ok = await props.onFeatureToggle(fieldName, value)
  pendingToggles.value = {...pendingToggles.value, [fieldName]: undefined}
}

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
  ElMessage.success(t('settings.screenshot.shortcutReset', {shortcut: props.form.screenshotToggleShortcut}))
}
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: var(--fy-text-muted);
  margin-top: 4px;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
}

.compact-card :deep(.el-card__body) {
  max-width: 560px;
}

.compact-card :deep(.el-input-group__append) {
  display: flex;
  flex-wrap: nowrap;
  width: auto;
}

.compact-card :deep(.el-button-group) {
  display: flex;
  flex-wrap: nowrap;
}

.recording :deep(.el-input__inner) {
  color: var(--fy-danger) !important;
}
</style>
