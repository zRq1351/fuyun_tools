<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card compact-card" shadow="never">
      <template #header>
        <div class="section-title">文档管理</div>
      </template>
      <el-form-item label="文档管理功能">
        <el-switch
            :active-text="pendingToggles.docManager === 'disabling' ? '正在禁用...' : '启用'"
            :inactive-text="pendingToggles.docManager === 'enabling' ? '正在启用...' : '关闭'"
            :loading="!!pendingToggles.docManager"
            :model-value="form.docManagerEnabled"
            @update:model-value="(val) => toggleFeature('docManagerEnabled', val)"
        />
        <div class="form-hint">关闭后快捷键将不可用，但可通过托盘菜单打开</div>
      </el-form-item>

      <el-form-item label="文档管理快捷键">
        <el-input
            :class="{ recording: isDocManagerRecording }"
            :model-value="docManagerDisplayValue"
            placeholder="例如: Ctrl+Shift+D"
            readonly
        >
          <template #append>
            <el-button-group>
              <el-button :type="isDocManagerRecording ? 'danger' : 'primary'" title="修改快捷键"
                         @click="toggleDocManagerRecording">
                <el-icon>
                  <component :is="isDocManagerRecording ? VideoPause : Edit"/>
                </el-icon>
              </el-button>
              <el-button title="恢复默认快捷键" @click="resetDocManagerShortcut">
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
        <div class="section-title">功能说明</div>
      </template>
      <div class="feature-list">
        <div class="feature-item">
          <el-icon>
            <FolderAdd/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">文件搬迁收纳</div>
            <div class="feature-desc">将桌面及任意文件夹中的文档移动到统一管理目录，按分类自动组织为子目录</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <Search/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">全文搜索</div>
            <div class="feature-desc">支持按文件名、文本内容全文搜索 PDF/Word/代码等所有文本文件</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <Collection/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">分类与标签</div>
            <div class="feature-desc">自定义分类目录和标签，双击打开文件，一键在资源管理器中定位</div>
          </div>
        </div>
      </div>
    </el-card>
  </el-form>
</template>

<script setup>
import {ref} from 'vue'
import {Collection, Edit, FolderAdd, RefreshLeft, Search, VideoPause} from '@element-plus/icons-vue'
import {ElMessage} from 'element-plus'
import {useShortcutRecorder} from '../composables/useShortcutRecorder'

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
  ElMessage.success(`已恢复文档管理快捷键默认值: ${props.form.docManagerHotKey}`)
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
