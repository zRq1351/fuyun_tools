<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.launcher.title') }}</div>
      </template>
      <el-form-item :label="$t('settings.launcher.enabled')">
        <el-switch
            :active-text="pendingToggles.launcher === 'disabling' ? $t('common.disabling') : $t('common.enable')"
            :inactive-text="pendingToggles.launcher === 'enabling' ? $t('common.enabling') : $t('common.close')"
            :loading="!!pendingToggles.launcher"
            :model-value="form.launcherEnabled"
            @update:model-value="(val) => toggleFeature('launcherEnabled', val)"
        />
        <div class="form-hint">{{ $t('settings.launcher.disabledHint') }}</div>
      </el-form-item>

      <el-form-item :label="$t('settings.launcher.launcherHotkey')">
        <el-input
            :class="{ recording: isLauncherRecording }"
            :model-value="launcherDisplayValue"
            :placeholder="$t('settings.clipboard.shortcutExample')"
            readonly
        >
          <template #append>
            <el-button-group>
              <el-button :title="$t('settings.clipboard.modifyShortcut')"
                         :type="isLauncherRecording ? 'danger' : 'primary'"
                         @click="toggleLauncherRecording">
                <el-icon>
                  <component :is="isLauncherRecording ? VideoPause : Edit"/>
                </el-icon>
              </el-button>
              <el-button :title="$t('settings.clipboard.resetShortcut')" @click="resetLauncherShortcut">
                <el-icon>
                  <RefreshLeft/>
                </el-icon>
              </el-button>
            </el-button-group>
          </template>
        </el-input>
      </el-form-item>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.launcher.featureDesc') }}</div>
      </template>
      <div class="feature-list">
        <div class="feature-item">
          <el-icon>
            <Search/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">{{ $t('settings.launcher.quickSearch') }}</div>
            <div class="feature-desc">{{ $t('settings.launcher.quickSearchDesc') }}</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <Operation/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">{{ $t('settings.launcher.quickCommands') }}</div>
            <div class="feature-desc">{{ $t('settings.launcher.quickCommandsDesc') }}</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <Key/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">{{ $t('settings.launcher.keyboardNav') }}</div>
            <div class="feature-desc">{{ $t('settings.launcher.keyboardNavDesc') }}</div>
          </div>
        </div>
      </div>
    </el-card>
  </el-form>
</template>

<script setup>
import {ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {Edit, Key, Operation, RefreshLeft, Search, VideoPause} from '@element-plus/icons-vue'
import {ElMessage} from 'element-plus'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'

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
  isRecording: isLauncherRecording,
  currentDisplayValue: launcherDisplayValue,
  toggleRecording: toggleLauncherRecording,
  stopRecording: stopLauncherRecording
} = useShortcutRecorder(props.form, 'launcherHotKey')

const resetLauncherShortcut = () => {
  stopLauncherRecording()
  const isMac = navigator.userAgent.toLowerCase().includes('mac')
  props.form.launcherHotKey = isMac ? 'Cmd+Q' : 'Alt+Q'
  ElMessage.success(t('settings.launcher.shortcutReset', {shortcut: props.form.launcherHotKey}))
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

.recording :deep(.el-input__inner) {
  color: var(--fy-danger) !important;
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

.feature-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.feature-item {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px;
  background: var(--fy-bg-card);
  border-radius: 8px;
  border: 1px solid var(--fy-border-light);
}

.feature-item .el-icon {
  font-size: 24px;
  color: var(--fy-accent);
  margin-top: 2px;
}

.feature-content {
  flex: 1;
}

.feature-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--fy-text-primary);
  margin-bottom: 4px;
}

.feature-desc {
  font-size: 12px;
  color: var(--fy-text-muted);
  line-height: 1.5;
}
</style>
