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
        <InfoFilled/>
      </el-icon>
      软件介绍
    </h3>
    <p class="intro-text">
      fuyun_tools 是一款常驻系统托盘的效率工具，聚焦“文字剪贴板 + 图片剪贴板 + Windows 划词 AI”三条高频工作流，
      目标是在不打断当前工作的前提下，完成快速回填、全屏预览、翻译解释和配置管理。
    </p>
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
        <strong>文字剪贴板</strong> - 自动记录历史，支持搜索、分类、快捷键回填
      </li>
      <li>
        <el-icon>
          <Picture/>
        </el-icon>
        <strong>图片剪贴板</strong> - 支持缩略图列表、双击回填、全屏预览与加载动画
      </li>
      <li>
        <el-icon>
          <FolderOpened/>
        </el-icon>
        <strong>历史上限策略</strong> - 可配置“仅限制未分组项”，保护已分组内容不被上限淘汰
      </li>
      <li>
        <el-icon>
          <Pointer/>
        </el-icon>
        <strong>划词助手</strong> - Windows 下选中文本后，直接翻译/解释/复制
      </li>
      <li>
        <el-icon>
          <Cpu/>
        </el-icon>
        <strong>AI 配置</strong> - 支持 OpenAI 兼容服务，自定义提供商与本地加密密钥存储
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
      <li><strong>文字剪贴板</strong>：按 <code>{{ toggleShortcut || 'Ctrl+Shift+Z' }}</code> 打开文字历史窗口
      </li>
      <li><strong>图片剪贴板</strong>：按 <code>{{ imageToggleShortcut || 'Ctrl+Shift+X' }}</code> 打开图片历史窗口</li>
      <li><strong>图片回填</strong>：在图片窗口双击目标项，自动写入剪贴板并粘贴到当前焦点应用</li>
      <li><strong>划词功能</strong>：Windows 下选中文本后自动显示工具栏</li>
      <li><strong>上限策略</strong>：可在“设置 → 剪贴板”开启“仅限制未分组项”</li>
      <li><strong>系统托盘</strong>：右键托盘图标可进入设置、清理记录、检查更新和退出</li>
    </ol>
  </div>
</template>

<script setup>
import {
  CircleCheck,
  Cpu,
  FolderOpened,
  InfoFilled,
  Picture,
  Pointer,
  Reading,
  Refresh,
  Star
} from '@element-plus/icons-vue'
import {useUpdater} from '../composables/useUpdater'

const props = defineProps({
  currentVersion: {
    type: String,
    required: true
  },
  toggleShortcut: {
    type: String,
    required: true
  },
  imageToggleShortcut: {
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

.intro-text {
  line-height: 1.8;
  color: #606266;
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

<style>
/* 全局样式，用于更新内容中的Markdown元素 */
.update-body-content {
  max-height: 300px;
  overflow-y: auto;
}

.update-body-content h1,
.update-body-content h2,
.update-body-content h3,
.update-body-content h4,
.update-body-content h5,
.update-body-content h6 {
  margin-top: 16px;
  margin-bottom: 8px;
  font-weight: 600;
  color: #303133;
}

.update-body-content h1 {
  font-size: 18px;
}

.update-body-content h2 {
  font-size: 16px;
}

.update-body-content h3 {
  font-size: 15px;
}

.update-body-content p {
  margin: 8px 0;
  line-height: 1.6;
}

.update-body-content ul,
.update-body-content ol {
  margin: 8px 0;
  padding-left: 20px;
}

.update-body-content li {
  margin: 4px 0;
  line-height: 1.6;
}

.update-body-content code {
  background-color: #f5f7fa;
  padding: 2px 6px;
  border-radius: 3px;
  font-family: Consolas, Monaco, 'Andale Mono', monospace;
  font-size: 0.9em;
  color: #476582;
}

.update-body-content pre {
  background-color: #f5f7fa;
  padding: 12px;
  border-radius: 4px;
  overflow-x: auto;
  margin: 8px 0;
}

.update-body-content pre code {
  background-color: transparent;
  padding: 0;
  font-size: 0.85em;
  color: #303133;
}

.update-body-content blockquote {
  margin: 8px 0;
  padding: 8px 12px;
  border-left: 4px solid #409eff;
  background-color: #f5f7fa;
  color: #606266;
}

.update-body-content a {
  color: #409eff;
  text-decoration: none;
}

.update-body-content a:hover {
  text-decoration: underline;
}

.update-body-content img {
  max-width: 100%;
  height: auto;
  margin: 8px 0;
}

.update-body-content table {
  border-collapse: collapse;
  margin: 8px 0;
  width: 100%;
}

.update-body-content th,
.update-body-content td {
  border: 1px solid #dcdfe6;
  padding: 8px 12px;
  text-align: left;
}

.update-body-content th {
  background-color: #f5f7fa;
  font-weight: 600;
}

.update-body-content hr {
  margin: 16px 0;
  border: 0;
  border-top: 1px solid #dcdfe6;
}

/* 自定义消息框样式 */
.update-message-box .el-message-box__content {
  padding: 10px 20px;
}

.update-message-box .el-message-box__message {
  margin: 0;
}
</style>
