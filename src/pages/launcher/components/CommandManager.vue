<template>
  <div v-if="visible" class="command-manager-overlay" @click.self="$emit('close')">
    <div class="command-manager">
      <div class="manager-header">
        <span class="title">{{ t('launcher.manageCommands') }}</span>
        <button :title="t('common.close')" class="close-btn" @click="$emit('close')">
          <el-icon :size="14">
            <Close/>
          </el-icon>
        </button>
      </div>

      <div class="manager-hint">输入 <code>:+命令</code> 例如 <code>:vscode</code> 回车即可快速启动，右键应用可添加命令
      </div>

      <div class="command-list">
        <div v-if="commands.length === 0" class="empty-state">
          <el-icon :size="48" color="var(--fy-text-muted)">
            <Document/>
          </el-icon>
          <p>暂无自定义命令</p>
          <p class="hint">右键应用可添加启动命令</p>
        </div>

        <div v-for="cmd in commands" :key="cmd.id" class="command-item">
          <div class="command-info">
            <div class="command-prefix">{{ cmd.prefix }}</div>
            <div class="command-title">{{ cmd.title }}</div>
            <div v-if="cmd.description" class="command-desc">{{ cmd.description }}</div>
            <div class="command-type">{{ getCommandTypeLabel(cmd.command_type) }}</div>
          </div>
          <div class="command-actions">
            <label class="toggle-switch">
              <input
                  :checked="cmd.enabled"
                  type="checkbox"
                  @change="toggleCommand(cmd)"
              />
              <span class="slider"></span>
            </label>
            <button :title="t('common.edit')" class="action-btn edit" @click="editCommand(cmd)">
              <el-icon :size="14">
                <Edit/>
              </el-icon>
            </button>
            <button :title="t('common.delete')" class="action-btn delete" @click="deleteCommand(cmd)">
              <el-icon :size="14">
                <Delete/>
              </el-icon>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 编辑命令对话框 -->
    <div v-if="showEditDialog" class="dialog-overlay">
      <div class="edit-dialog">
        <div class="dialog-title">编辑命令</div>

        <div class="form-group">
          <label>命令前缀</label>
          <div class="prefix-input-wrapper">
            <input
                v-model="editForm.prefix"
                class="form-input prefix-input"
            />
            <span class="prefix-symbol">:</span>
          </div>
        </div>

        <div class="form-group">
          <label>标题</label>
          <input
              v-model="editForm.title"
              class="form-input"
              placeholder="命令标题"
          />
        </div>

        <div class="form-group">
          <label>描述</label>
          <input
              v-model="editForm.description"
              class="form-input"
              placeholder="可选描述"
          />
        </div>

        <div class="form-group">
          <label class="checkbox-label">
            <input v-model="editForm.enabled" type="checkbox"/>
            <span>启用此命令</span>
          </label>
        </div>

        <div class="dialog-actions">
          <button class="dialog-btn cancel" @click="closeEditDialog">{{ t('common.cancel') }}</button>
          <button class="dialog-btn confirm" @click="confirmEdit">{{ t('common.save') }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {ref, watch} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage} from 'element-plus'
import {Close, Delete, Document, Edit} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'

const {t} = useI18n()

const props = defineProps({
  visible: Boolean
})

const emit = defineEmits(['close', 'updated'])

const commands = ref([])
const showEditDialog = ref(false)
const editingCommand = ref(null)
const editForm = ref({
  prefix: '',
  title: '',
  description: '',
  enabled: true
})

// 加载命令列表
const loadCommands = async () => {
  try {
    const config = await invoke('get_launcher_config')
    commands.value = config.custom_commands || []
  } catch (error) {
    console.error('加载命令失败:', error)
  }
}

// 监听可见性变化
watch(() => props.visible, (newVal) => {
  if (newVal) {
    loadCommands()
  }
})

// 获取命令类型标签
const getCommandTypeLabel = (type) => {
  if (type.RunProgram) return t('common.open') + '程序'
  if (type.OpenWindow) return `${t('common.open')}窗口: ${type.OpenWindow.label}`
  if (type.ExecuteAction) return `${t('common.open')}操作: ${type.ExecuteAction.action}`
  if (type.CopyText) return t('common.copy') + '文本'
  return t('common.unknown')
}

// 切换启用状态
const toggleCommand = async (cmd) => {
  try {
    await invoke('toggle_custom_command', {commandId: cmd.id})
    await loadCommands()
    emit('updated')
  } catch (error) {
    console.error('切换失败:', error)
  }
}

// 删除命令
const deleteCommand = async (cmd) => {
  if (__DEV_PANEL__) console.log('删除命令:', cmd.title, cmd.id)

  try {
    await invoke('remove_custom_command', {commandId: cmd.id})
    await loadCommands()
    emit('updated')
    ElMessage.success(t('launcher.appRemoved'))
  } catch (error) {
    console.error('删除失败:', error)
    ElMessage.error(t('common.operationFailed') + ': ' + error)
  }
}

// 编辑命令
const editCommand = (cmd) => {
  editingCommand.value = cmd
  // 去除前缀的 : 符号
  const prefixWithoutColon = cmd.prefix.startsWith(':') ? cmd.prefix.substring(1) : cmd.prefix
  editForm.value = {
    prefix: prefixWithoutColon,
    title: cmd.title,
    description: cmd.description || '',
    enabled: cmd.enabled
  }
  showEditDialog.value = true
}

// 关闭编辑对话框
const closeEditDialog = () => {
  showEditDialog.value = false
  editingCommand.value = null
}

// 确认编辑
const confirmEdit = async () => {
  if (!editingCommand.value || !editForm.value.prefix.trim()) return

  try {
    // 自动添加 : 前缀
    const finalPrefix = ':' + editForm.value.prefix.trim()

    // 如果前缀没有变化，不传递 prefix 参数
    const originalPrefix = editingCommand.value.prefix
    const prefixParam = finalPrefix !== originalPrefix ? finalPrefix : null

    await invoke('update_custom_command', {
      commandId: editingCommand.value.id,
      prefix: prefixParam,
      title: editForm.value.title !== editingCommand.value.title ? editForm.value.title : null,
      description: editForm.value.description !== (editingCommand.value.description || '') ? (editForm.value.description || null) : null,
      enabled: editForm.value.enabled !== editingCommand.value.enabled ? editForm.value.enabled : null
    })

    closeEditDialog()
    await loadCommands()
    emit('updated')
    ElMessage.success(t('common.success'))
  } catch (error) {
    console.error('编辑失败:', error)
    ElMessage.error(error)
  }
}
</script>

<style scoped>
.command-manager-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
}

.command-manager {
  width: 600px;
  max-height: 80vh;
  background: var(--fy-bg-surface);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  box-shadow: var(--fy-shadow-lg);
}

.manager-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 16px 20px;
  border-bottom: 1px solid var(--fy-border-light);
}

.title {
  flex: 1;
  font-size: 16px;
  font-weight: 600;
  color: var(--fy-text-primary);
}

.close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.2s;
}

.close-btn:hover {
  background: var(--fy-danger-bg);
  color: var(--fy-danger);
}

.manager-hint {
  padding: 8px 20px;
  font-size: 12px;
  color: var(--fy-text-muted);
  border-bottom: 1px solid var(--fy-border-light);
}

.manager-hint code {
  background: var(--fy-bg-hover);
  padding: 0 4px;
  border-radius: 3px;
  color: var(--fy-accent);
}

.command-list {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 60px 20px;
  color: var(--fy-text-muted);
  text-align: center;
}

.empty-state p {
  margin: 8px 0;
}

.empty-state .hint {
  font-size: 12px;
  opacity: 0.7;
}

.command-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  background: var(--fy-bg-card);
  border: 1px solid var(--fy-border-light);
  border-radius: 8px;
  margin-bottom: 8px;
  transition: all 0.2s;
}

.command-item:hover {
  border-color: var(--fy-accent);
}

.command-info {
  flex: 1;
  min-width: 0;
}

.command-prefix {
  font-size: 13px;
  font-weight: 600;
  color: var(--fy-accent);
  font-family: monospace;
}

.command-title {
  font-size: 14px;
  color: var(--fy-text-primary);
  margin-top: 2px;
}

.command-desc {
  font-size: 12px;
  color: var(--fy-text-muted);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.command-type {
  font-size: 11px;
  color: var(--fy-text-muted);
  margin-top: 4px;
  padding: 2px 6px;
  background: var(--fy-bg-hover);
  border-radius: 4px;
  display: inline-block;
}

.command-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.toggle-switch {
  position: relative;
  display: inline-block;
  width: 40px;
  height: 20px;
}

.toggle-switch input {
  opacity: 0;
  width: 0;
  height: 0;
}

.slider {
  position: absolute;
  cursor: pointer;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: var(--fy-border);
  transition: 0.3s;
  border-radius: 20px;
}

.slider:before {
  position: absolute;
  content: "";
  height: 16px;
  width: 16px;
  left: 2px;
  bottom: 2px;
  background-color: var(--fy-text-primary);
  transition: 0.3s;
  border-radius: 50%;
}

input:checked + .slider {
  background-color: var(--fy-accent);
}

input:checked + .slider:before {
  transform: translateX(20px);
}

.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  border-radius: 4px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.2s;
}

.action-btn.delete:hover {
  background: var(--fy-danger-bg);
  color: var(--fy-danger);
}

.action-btn.edit:hover {
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

/* Edit Dialog Styles */
.dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10001;
}

.edit-dialog {
  width: 400px;
  background: var(--fy-bg-surface);
  border-radius: 12px;
  padding: 20px;
  box-shadow: var(--fy-shadow-lg);
}

.edit-dialog .dialog-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--fy-text-primary);
  margin-bottom: 16px;
}

.edit-dialog .form-group {
  margin-bottom: 16px;
}

.edit-dialog .form-group label {
  display: block;
  font-size: 12px;
  color: var(--fy-text-muted);
  margin-bottom: 6px;
}

.edit-dialog .checkbox-label {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}

.edit-dialog .checkbox-label input[type="checkbox"] {
  width: 16px;
  height: 16px;
  cursor: pointer;
}

.edit-dialog .form-input {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--fy-border);
  border-radius: 6px;
  background: var(--fy-bg-card);
  color: var(--fy-text-primary);
  font-size: 14px;
  outline: none;
  transition: border-color 0.2s;
  box-sizing: border-box;
}

.edit-dialog .form-input:focus {
  border-color: var(--fy-accent);
}

.edit-dialog .prefix-input-wrapper {
  position: relative;
  display: flex;
  align-items: center;
}

.edit-dialog .prefix-symbol {
  position: absolute;
  left: 12px;
  font-size: 14px;
  color: var(--fy-text-muted);
  user-select: none;
  pointer-events: none;
  z-index: 10;
  background: var(--fy-bg-card);
  padding: 0 2px;
}

.edit-dialog .prefix-input {
  padding-left: 24px;
}

.edit-dialog .dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 20px;
}

.edit-dialog .dialog-btn {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
  transition: all 0.2s;
}

.edit-dialog .dialog-btn.cancel {
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
}

.edit-dialog .dialog-btn.cancel:hover {
  background: var(--fy-border);
}

.edit-dialog .dialog-btn.confirm {
  background: var(--fy-accent);
  color: white;
}

.edit-dialog .dialog-btn.confirm:hover {
  background: var(--fy-accent-hover);
}
</style>
