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
      fuyun_tools 是一款常驻系统托盘的效率工具，聚焦“文字剪贴板 + 图片剪贴板 + Windows 划词 AI + Windows 截图与 OCR”
      四条高频工作流，目标是在不打断当前工作的前提下，完成快速回填、大图预览、截图提取、翻译解释与配置管理。
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
        <strong>图片剪贴板</strong> - 支持缩略图列表、双击回填与大图预览
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
          <Picture/>
        </el-icon>
        <strong>截图与 OCR</strong> - Windows 下支持快捷截图，并可在固定图片窗口进行 OCR 识别
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
      <li><strong>截图工具</strong>：按 <code>{{ screenshotToggleShortcut || 'Ctrl+Shift+S' }}</code> 打开截图窗口</li>
      <li><strong>图片 OCR</strong>：在固定图片窗口右键，选择 OCR 识别并查看文本结果</li>
      <li><strong>图片回填</strong>：在图片窗口双击目标项，自动写入剪贴板并粘贴到当前焦点应用</li>
      <li><strong>划词功能</strong>：Windows 下选中文本后自动显示工具栏</li>
      <li><strong>上限策略</strong>：可在“设置 → 剪贴板”开启“仅限制未分组项”</li>
      <li><strong>系统托盘</strong>：右键托盘图标可进入设置、开机自启切换、日志操作（调试模式）和退出</li>
    </ol>
  </div>

  <div class="about-section">
    <h3>
      <el-icon>
        <Reading/>
      </el-icon>
      开源版权声明
    </h3>
    <ul class="feature-list">
      <li>
        <strong>FFmpeg</strong> - Copyright (c) FFmpeg developers。
      </li>
      <li>
        <strong>使用说明</strong> - 本软件通过外部进程调用 ffmpeg.exe，录屏功能启用时按需下载，不随安装包默认内置。
      </li>
      <li>
        <strong>分发策略</strong> - ffmpeg.exe 下载到本地 bin 目录，下载地址可由 settings.json 中
        recording_ffmpeg_download_url 配置。
      </li>
      <li>
        <strong>授权说明</strong> - 当前使用的 FFmpeg 构建包含 GPL 组件（如 libx264），对应 ffmpeg.exe 按 GPL/LGPL 要求分发。
      </li>
      <li>
        <strong>对应源码</strong> - FFmpeg 8.0.1 源码可从 https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz 获取。
      </li>
      <li>
        <strong>上游仓库</strong> - https://git.ffmpeg.org/ffmpeg.git
      </li>
      <li>
        <strong>构建参考</strong> - https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.0.1-essentials_build.7z
      </li>
      <li>
        <strong>完整声明</strong> - 详见 docs/THIRD_PARTY_NOTICES.md（含许可证文件与源码可获取信息）。
      </li>
    </ul>
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
  },
  screenshotToggleShortcut: {
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
  max-height: 320px;
  overflow-y: auto;
  padding: 14px 16px;
  border-radius: 10px;
  background: linear-gradient(160deg, #f8fbff 0%, #f4f8ff 100%);
  border: 1px solid #dbe7ff;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.75);
  scrollbar-width: thin;
  scrollbar-color: rgba(64, 158, 255, 0.4) transparent;
}

.update-body-content::-webkit-scrollbar {
  width: 6px;
}

.update-body-content::-webkit-scrollbar-thumb {
  border-radius: 4px;
  background: rgba(64, 158, 255, 0.4);
}

.update-body-content::-webkit-scrollbar-track {
  background: transparent;
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
  margin: 10px 0;
  line-height: 1.6;
}

.update-body-content ul,
.update-body-content ol {
  margin: 10px 0;
  padding-left: 20px;
}

.update-body-content li {
  margin: 6px 0;
  line-height: 1.6;
}

.update-body-content code {
  background-color: rgba(64, 158, 255, 0.12);
  padding: 2px 6px;
  border-radius: 3px;
  font-family: Consolas, Monaco, 'Andale Mono', monospace;
  font-size: 0.9em;
  color: #476582;
}

.update-body-content pre {
  background-color: #eef4ff;
  padding: 12px;
  border-radius: 8px;
  overflow-x: auto;
  margin: 10px 0;
  border: 1px solid #d7e5ff;
}

.update-body-content pre code {
  background-color: transparent;
  padding: 0;
  font-size: 0.85em;
  color: #303133;
}

.update-body-content blockquote {
  margin: 10px 0;
  padding: 10px 12px;
  border-left: 4px solid #409eff;
  background-color: #edf5ff;
  color: #606266;
  border-radius: 0 8px 8px 0;
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
  margin: 10px 0;
  width: 100%;
  background: #fff;
  border-radius: 8px;
  overflow: hidden;
}

.update-body-content th,
.update-body-content td {
  border: 1px solid #e3ebfb;
  padding: 8px 12px;
  text-align: left;
}

.update-body-content th {
  background-color: #eff5ff;
  font-weight: 600;
}

.update-body-content hr {
  margin: 16px 0;
  border: 0;
  border-top: 1px solid #dcdfe6;
}

/* 自定义消息框样式 */
.update-message-box .el-message-box__content {
  padding: 10px 18px 6px;
}

.update-message-box .el-message-box__message {
  margin: 0;
}

.update-message-box {
  border-radius: 14px !important;
  overflow: hidden;
}

.update-message-box .el-message-box__header {
  padding: 14px 18px 0;
}

.update-message-box .el-message-box__title {
  font-size: 16px;
  font-weight: 700;
}

.update-message-box .el-message-box__btns {
  padding: 10px 18px 18px;
}

.update-dialog {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.update-dialog__hero {
  border-radius: 10px;
  border: 1px solid #dbe7ff;
  background: linear-gradient(145deg, #f6f9ff 0%, #eff5ff 100%);
  padding: 12px 14px;
}

.update-dialog__tag {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 12px;
  color: #2b6be8;
  background: rgba(64, 158, 255, 0.16);
}

.update-dialog__title {
  margin: 8px 0 4px;
  font-size: 20px;
  line-height: 1.2;
  color: #1f2d3d;
}

.update-dialog__subtitle {
  margin: 0;
  font-size: 13px;
  color: #5e6d82;
}

.update-dialog__hint {
  margin: 0;
  font-size: 12px;
  color: #7b8795;
}

html.dark .update-message-box,
.dark .update-message-box {
  background: #1f232b !important;
  border: 1px solid #343a46;
}

html.dark .update-dialog__hero,
.dark .update-dialog__hero {
  border-color: #3a4252;
  background: linear-gradient(145deg, #28303c 0%, #232a35 100%);
}

html.dark .update-dialog__tag,
.dark .update-dialog__tag {
  color: #8ec5ff;
  background: rgba(64, 158, 255, 0.22);
}

html.dark .update-dialog__title,
.dark .update-dialog__title {
  color: #eef3ff;
}

html.dark .update-dialog__subtitle,
html.dark .update-dialog__hint,
.dark .update-dialog__subtitle,
.dark .update-dialog__hint {
  color: #aab6cc;
}

html.dark .update-body-content,
.dark .update-body-content {
  background: linear-gradient(160deg, #242b36 0%, #202631 100%);
  border-color: #3a4252;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
  color: #e6ebf5;
  scrollbar-color: rgba(143, 180, 255, 0.55) transparent;
}

html.dark .update-body-content::-webkit-scrollbar-thumb,
.dark .update-body-content::-webkit-scrollbar-thumb {
  background: rgba(143, 180, 255, 0.55);
}

html.dark .update-body-content h1,
html.dark .update-body-content h2,
html.dark .update-body-content h3,
html.dark .update-body-content h4,
html.dark .update-body-content h5,
html.dark .update-body-content h6,
.dark .update-body-content h1,
.dark .update-body-content h2,
.dark .update-body-content h3,
.dark .update-body-content h4,
.dark .update-body-content h5,
.dark .update-body-content h6 {
  color: #edf3ff;
}

html.dark .update-body-content code,
.dark .update-body-content code {
  background: rgba(110, 148, 255, 0.22);
  color: #cfe0ff;
}

html.dark .update-body-content pre,
.dark .update-body-content pre {
  background: #1c212b;
  border-color: #3c4350;
}

html.dark .update-body-content pre code,
.dark .update-body-content pre code {
  color: #d7deea;
  background: transparent;
}

html.dark .update-body-content blockquote,
.dark .update-body-content blockquote {
  background: #273246;
  color: #c8d3e9;
}

html.dark .update-body-content table,
.dark .update-body-content table {
  background: #1d2430;
}

html.dark .update-body-content th,
html.dark .update-body-content td,
.dark .update-body-content th,
.dark .update-body-content td {
  border-color: #3a4352;
  color: #d6deec;
}

html.dark .update-body-content th,
.dark .update-body-content th {
  background: #263042;
}

html.dark .update-body-content hr,
.dark .update-body-content hr {
  border-top-color: #3a4352;
}
</style>
