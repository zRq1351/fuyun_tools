<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">快捷启动器</div>
      </template>
      <el-form-item label="启动器功能">
        <el-switch v-model="form.launcherEnabled" active-text="启用" inactive-text="关闭"/>
        <div class="form-hint">关闭后将无法使用快捷键唤起启动器</div>
      </el-form-item>

      <el-form-item label="启动器快捷键">
        <div class="shortcut-input-wrapper">
          <el-input
              v-model="form.launcherHotKey"
              placeholder="点击录制快捷键"
              readonly
              @keydown="handleShortcutKeydown"
          />
          <el-button size="small" @click="resetShortcut">重置</el-button>
        </div>
        <div class="form-hint">按下组合键设置快捷键，例如 Ctrl+Space</div>
      </el-form-item>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">启动器功能说明</div>
      </template>
      <div class="feature-list">
        <div class="feature-item">
          <el-icon>
            <Search/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">快速搜索</div>
            <div class="feature-desc">搜索已安装的应用程序和常用文件</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <Operation/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">快捷命令</div>
            <div class="feature-desc">输入 :settings、:clipboard 等命令快速执行操作</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <DataLine/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">计算器</div>
            <div class="feature-desc">直接输入数学表达式进行计算</div>
          </div>
        </div>
        <div class="feature-item">
          <el-icon>
            <Keyboard/>
          </el-icon>
          <div class="feature-content">
            <div class="feature-title">键盘导航</div>
            <div class="feature-desc">使用方向键选择，回车执行，Esc 关闭</div>
          </div>
        </div>
      </div>
    </el-card>
  </el-form>
</template>

<script setup>
import {DataLine, Operation, Search} from '@element-plus/icons-vue'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const DEFAULT_LAUNCHER_SHORTCUT = 'Ctrl+K'

const handleShortcutKeydown = (event) => {
  event.preventDefault()

  const modifiers = []
  if (event.ctrlKey) modifiers.push('Ctrl')
  if (event.altKey) modifiers.push('Alt')
  if (event.shiftKey) modifiers.push('Shift')
  if (event.metaKey) modifiers.push('Meta')

  // 忽略单独的修饰键
  if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) {
    return
  }

  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key
  const shortcut = [...modifiers, key].join('+')

  if (modifiers.length > 0) {
    props.form.launcherHotKey = shortcut
  }
}

const resetShortcut = () => {
  props.form.launcherHotKey = DEFAULT_LAUNCHER_SHORTCUT
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

.shortcut-input-wrapper {
  display: flex;
  gap: 8px;
  width: 100%;
}

.shortcut-input-wrapper .el-input {
  flex: 1;
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
