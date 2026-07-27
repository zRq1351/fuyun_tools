<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.docManager.title') }}</div>
      </template>
      <el-form-item :label="$t('settings.docManager.enabled')">
        <el-switch
            :active-text="pendingToggles.docManager === 'disabling' ? $t('common.disabling') : $t('common.enable')"
            :inactive-text="pendingToggles.docManager === 'enabling' ? $t('common.enabling') : $t('common.close')"
            :loading="!!pendingToggles.docManager"
            :model-value="form.docManagerEnabled"
            @update:model-value="(val) => toggleFeature('docManagerEnabled', val)"
        />
        <div class="form-hint">{{ $t('settings.docManager.disabledHint') }}</div>
      </el-form-item>

      <el-form-item :label="$t('settings.docManager.hotkey')">
        <el-input
            :class="{ recording: isDocManagerRecording }"
            :model-value="docManagerDisplayValue"
            :placeholder="$t('settings.clipboard.shortcutExample')"
            readonly
        >
          <template #append>
            <el-button-group>
              <el-button :title="$t('settings.clipboard.modifyShortcut')"
                         :type="isDocManagerRecording ? 'danger' : 'primary'"
                         @click="toggleDocManagerRecording">
                <el-icon>
                  <component :is="isDocManagerRecording ? VideoPause : Edit"/>
                </el-icon>
              </el-button>
              <el-button :title="$t('settings.clipboard.resetShortcut')" @click="resetDocManagerShortcut">
                <el-icon>
                  <RefreshLeft/>
                </el-icon>
              </el-button>
            </el-button-group>
          </template>
        </el-input>
      </el-form-item>

      <el-form-item :label="$t('settings.docManager.widgetEnabled')">
        <el-switch
            :model-value="form.docManagerWidgetEnabled"
            @update:model-value="onWidgetToggle"
        />
        <div class="form-hint">{{ $t('settings.docManager.widgetHint') }}</div>
      </el-form-item>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.docManager.featureDesc') }}</div>
      </template>
      <div class="feature-list">
        <div class="feature-item">
          <el-icon>
            <FolderAdd/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">{{ $t('settings.docManager.organize') }}</div>
            <div class="feature-desc">{{ $t('settings.docManager.organizeDesc') }}</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <Search/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">{{ $t('settings.docManager.fulltextSearch') }}</div>
            <div class="feature-desc">{{ $t('settings.docManager.fulltextSearchDesc') }}</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <Collection/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">{{ $t('settings.docManager.categoryTag') }}</div>
            <div class="feature-desc">{{ $t('settings.docManager.categoryTagDesc') }}</div>
          </div>
        </div>
      </div>
    </el-card>
  </el-form>
</template>

<script setup>
import {ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {Collection, Edit, FolderAdd, RefreshLeft, Search, VideoPause} from '@element-plus/icons-vue'
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
  isRecording: isDocManagerRecording,
  currentDisplayValue: docManagerDisplayValue,
  toggleRecording: toggleDocManagerRecording,
  stopRecording: stopDocManagerRecording
} = useShortcutRecorder(props.form, 'docManagerHotKey')

const resetDocManagerShortcut = () => {
  stopDocManagerRecording()
  props.form.docManagerHotKey = 'Ctrl+Shift+D'
  ElMessage.success(t('settings.docManager.shortcutReset', {shortcut: props.form.docManagerHotKey}))
}

const onWidgetToggle = async (val) => {
  props.form.docManagerWidgetEnabled = val
  try {
    const {invoke} = await import('@tauri-apps/api/core')
    if (val) {
      await invoke('show_doc_manager_widget')
    } else {
      await invoke('hide_doc_manager_widget')
    }
  } catch (e) {
    console.error('切换文档管理小部件失败:', e)
  }
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
