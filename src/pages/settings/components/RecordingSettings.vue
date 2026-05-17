<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.recording.title') }}</div>
      </template>
      <div class="setting-group">
        <div class="group-title">{{ $t('settings.recording.basicControl') }}</div>
        <el-form-item :label="$t('settings.recording.enabled')">
          <el-switch
              :active-text="pendingToggles.recording === 'disabling' ? $t('common.disabling') : $t('common.enable')"
              :inactive-text="pendingToggles.recording === 'enabling' ? $t('common.enabling') : $t('common.disable')"
              :loading="!!pendingToggles.recording"
              :model-value="form.recordingEnabled"
              @update:model-value="(val) => toggleFeature('recordingEnabled', val)"
          />
          <div class="form-hint">{{ $t('settings.recording.disabledHint') }}</div>
        </el-form-item>
        <el-form-item :label="$t('settings.recording.recordingHotkey')">
          <el-input
              :class="{ recording: isRecordingHotkeyRecording }"
              :model-value="recordingDisplayValue"
              :placeholder="$t('settings.clipboard.shortcutExample')"
              readonly
          >
            <template #append>
              <el-button-group>
                <el-button :title="$t('settings.clipboard.modifyShortcut')" :type="isRecordingHotkeyRecording ? 'danger' : 'primary'"
                           @click="toggleRecordingHotkey">
                  <el-icon>
                    <component :is="isRecordingHotkeyRecording ? VideoPause : Edit"/>
                  </el-icon>
                </el-button>
                <el-button :title="$t('settings.clipboard.resetShortcut')" @click="resetRecordingHotkey">
                  <el-icon><RefreshLeft /></el-icon>
                </el-button>
              </el-button-group>
            </template>
          </el-input>
        </el-form-item>
        <el-form-item :label="$t('settings.recording.micToggleHotkey')">
          <el-input
              :class="{ recording: isMicToggleHotkeyRecording }"
              :model-value="micToggleDisplayValue"
              :placeholder="$t('settings.clipboard.shortcutExample')"
              readonly
          >
            <template #append>
              <el-button-group>
                <el-button :title="$t('settings.clipboard.modifyShortcut')" :type="isMicToggleHotkeyRecording ? 'danger' : 'primary'"
                           @click="toggleMicToggleHotkey">
                  <el-icon>
                    <component :is="isMicToggleHotkeyRecording ? VideoPause : Edit"/>
                  </el-icon>
                </el-button>
                <el-button :title="$t('settings.clipboard.resetShortcut')" @click="resetMicToggleHotkey">
                  <el-icon><RefreshLeft /></el-icon>
                </el-button>
              </el-button-group>
            </template>
          </el-input>
          <div class="form-hint">{{ $t('settings.recording.micToggleHint') }}</div>
        </el-form-item>
      </div>

      <div class="setting-group">
        <div class="group-title">{{ $t('settings.recording.calibration') }}</div>
        <div class="group-grid quality-grid">
          <el-form-item :label="$t('settings.recording.audioSync')">
            <el-input-number v-model="form.recordingWindowAudioSyncAdvanceMs" :max="500" :min="0" :step="5"/>
            <div class="form-hint">{{ $t('settings.recording.audioSyncHint') }}</div>
          </el-form-item>
          <el-form-item :label="$t('settings.recording.maxDuration')">
            <el-input-number v-model="form.recordingMaxDurationMinutes" :max="1440" :min="1"/>
          </el-form-item>
        </div>
      </div>

      <div class="setting-group">
        <div class="group-title">{{ $t('settings.recording.output') }}</div>
        <el-form-item :label="$t('settings.recording.outputDir')">
          <el-input v-model="effectiveOutputDirDisplay" readonly>
            <template #append>
              <el-tooltip :content="$t('common.selectDir')" placement="top">
                <el-button @click="selectOutputDir">
                  <el-icon>
                    <FolderOpened/>
                  </el-icon>
                </el-button>
              </el-tooltip>
            </template>
          </el-input>
          <div class="form-hint">
            {{ $t('settings.recording.outputDirHint') }}
            <el-link type="primary" @click="openOutputDir">{{ $t('settings.recording.openDir') }}</el-link>
          </div>
        </el-form-item>
        <div class="switch-row">
          <el-form-item :label="$t('settings.recording.autoOpenDir')">
            <el-switch v-model="form.recordingAutoOpenFolder" :active-text="$t('common.open')"
                       :inactive-text="$t('common.close')"/>
          </el-form-item>
        </div>
      </div>
    </el-card>
  </el-form>
</template>


<script setup>
import {computed, onMounted, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {Edit, FolderOpened, RefreshLeft, VideoPause} from '@element-plus/icons-vue'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'
import {RecordingService} from '../../../services/ipc'
import {open} from '@tauri-apps/plugin-dialog'
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
  ElMessage.success(t('settings.recording.shortcutResetRecording'))
}

const resetMicToggleHotkey = () => {
  stopMicToggleHotkeyRecording()
  props.form.recordingMicToggleShortcut = 'Ctrl+Space'
  ElMessage.success(t('settings.recording.shortcutResetMic'))
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
    ElMessage.error(t('settings.recording.openDirFailed', {error: String(e)}))
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
    ElMessage.error(t('settings.recording.selectDirFailed', {error: String(e)}))
  }
}

onMounted(async () => {
  try {
    effectiveOutputDir.value = await RecordingService.getOutputDir()
  } catch (e) {
    effectiveOutputDir.value = ''
    ElMessage.error(t('settings.recording.loadDirFailed', {error: String(e)}))
  }
})
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
  max-width: 860px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.setting-group {
  border: 1px solid var(--fy-border-light);
  border-radius: 10px;
  padding: 12px 12px 6px;
  background: var(--fy-bg-primary);
}

.group-title {
  font-size: 13px;
  font-weight: 700;
  color: var(--fy-text-muted);
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

.setting-group :deep(.el-input-group__append) {
  display: flex;
  flex-wrap: nowrap;
  width: auto;
}

.setting-group :deep(.el-button-group) {
  display: flex;
  flex-wrap: nowrap;
}

.recording :deep(.el-input__inner) {
  color: var(--fy-danger) !important;
}

@media (max-width: 900px) {
  .group-grid,
  .quality-grid,
  .switch-row {
    grid-template-columns: 1fr;
  }

}
</style>
