<template>
  <div class="about-section">
    <h3>
      <el-icon>
        <Refresh/>
      </el-icon>
      检查更新
    </h3>
    <p>当前版本：<strong>{{ currentVersion }}</strong></p>
    <el-button :loading="checkingUpdate" type="warning" @click="checkUpdate">
      检查更新
    </el-button>
    <div v-if="updateStatus" :class="updateStatus.type" class="update-status">
      {{ updateStatus.message }}
    </div>
    <div v-if="showUpdateProgress" class="update-progress">
      <el-progress :percentage="updateProgress" :status="updateProgress === 100 ? 'success' : ''"/>
      <div class="progress-text">正在更新... {{ updateProgress }}%</div>
    </div>
  </div>

  <div class="about-section">
    <h3>
      <el-icon>
        <Star/>
      </el-icon>
      软件功能
    </h3>
    <ul class="feature-list">
      <li>
        <el-icon>
          <CircleCheck/>
        </el-icon>
        <strong>剪贴板管理</strong> - 自动记录剪贴板历史，支持快速选择和粘贴
      </li>
      <li>
        <el-icon>
          <Pointer/>
        </el-icon>
        <strong>划词翻译</strong> - 选中文本后自动显示翻译和解释选项
      </li>
      <li>
        <el-icon>
          <Cpu/>
        </el-icon>
        <strong>AI集成</strong> - 支持OpenAI等AI服务，提供智能翻译和解释
      </li>
      <li>
        <el-icon>
          <Lightning/>
        </el-icon>
        <strong>快捷键操作</strong> - 支持自定义快捷键，提高工作效率
      </li>
      <li>
        <el-icon>
          <Timer/>
        </el-icon>
        <strong>历史记录</strong> - 保存剪贴板历史，方便重复使用
      </li>
    </ul>
  </div>

  <div class="about-section">
    <h3>
      <el-icon>
        <Reading/>
      </el-icon>
      使用方法
    </h3>
    <ol class="usage-list">
      <li><strong>剪贴板使用</strong>：按 <code>{{ toggleShortcut || 'Ctrl+Shift+K' }}</code> 显示剪贴板历史窗口
      </li>
      <li><strong>划词功能</strong>：选中文本后显示工具栏</li>
      <li><strong>AI设置</strong>：在AI设置页面配置API密钥和服务地址</li>
      <li><strong>更新检查</strong>：点击按钮检查软件更新</li>
      <li><strong>系统托盘</strong>：右键系统托盘图标访问更多功能</li>
    </ol>
  </div>
</template>

<script setup>
import {CircleCheck, Cpu, Lightning, Pointer, Reading, Refresh, Star, Timer} from '@element-plus/icons-vue'
import {useUpdater} from '../composables/useUpdater'

const props = defineProps({
  currentVersion: {
    type: String,
    required: true
  },
  toggleShortcut: {
    type: String,
    required: true
  }
})

const {
  checkingUpdate,
  updateStatus,
  updateProgress,
  showUpdateProgress,
  checkUpdate
} = useUpdater(props.currentVersion)
</script>

<style scoped>
.feature-list, .usage-list {
  padding-left: 20px;
  line-height: 1.8;
}

.feature-list li, .usage-list li {
  margin-bottom: 8px;
}

.update-status {
  margin-top: 10px;
  padding: 10px;
  border-radius: 4px;
}

.update-status.success {
  background-color: #f0f9eb;
  color: #67c23a;
}

.update-status.error {
  background-color: #fef0f0;
  color: #f56c6c;
}

.update-status.info {
  background-color: #f4f4f5;
  color: #909399;
}
</style>
