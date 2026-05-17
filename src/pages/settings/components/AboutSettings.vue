<template>
  <div class="about-section">
    <h3>
      <el-icon>
        <Refresh/>
      </el-icon>
      {{ $t('settings.about.checkUpdate') }}
    </h3>
    <p>{{ $t('settings.about.currentVersion') }}<strong>{{ currentVersion }}</strong></p>
    <el-button :loading="checkingUpdate" type="warning" @click="checkUpdate">
      {{ $t('settings.about.checkUpdateBtn') }}
    </el-button>
    <div v-if="updateStatus" :class="updateStatus.type" class="update-status">
      {{ updateStatus.message }}
    </div>
    <div v-if="showUpdateProgress" class="update-progress">
      <el-progress :percentage="updateProgress" :status="updateProgress === 100 ? 'success' : ''"/>
      <div class="progress-text">{{ $t('settings.about.updatingProgress', {progress: updateProgress}) }}</div>
    </div>
  </div>

  <div class="about-section">
    <h3>
      <el-icon>
        <InfoFilled/>
      </el-icon>
      {{ $t('settings.about.softwareIntro') }}
    </h3>
    <p class="intro-text">
      {{ $t('settings.about.softwareIntroText') }}
    </p>
  </div>

  <div class="about-section">
    <h3>
      <el-icon>
        <Star/>
      </el-icon>
      {{ $t('settings.about.features') }}
    </h3>
    <ul class="feature-list">
      <li v-for="key in featureKeys" :key="key">
        <el-icon>
          <component :is="featureIcons[key]"/>
        </el-icon>
        <span v-html="renderFeatureItem(key)"></span>
      </li>
    </ul>
  </div>

  <div class="about-section">
    <h3>
      <el-icon>
        <Reading/>
      </el-icon>
      {{ $t('settings.about.usage') }}
    </h3>
    <ol class="usage-list">
      <li v-html="renderUsageItem('settings.about.usage1', toggleShortcut, 'Ctrl+Shift+Z')"></li>
      <li v-html="renderUsageItem('settings.about.usage2', imageToggleShortcut, 'Ctrl+Shift+X')"></li>
      <li v-html="renderUsageItem('settings.about.usage3', screenshotToggleShortcut, 'Ctrl+Shift+S')"></li>
      <li v-html="renderUsageItem('settings.about.usage4')"></li>
      <li v-html="renderUsageItem('settings.about.usage5')"></li>
      <li v-html="renderUsageItem('settings.about.usage6')"></li>
      <li v-html="renderUsageItem('settings.about.usage7')"></li>
      <li v-html="renderUsageItem('settings.about.usage8')"></li>
    </ol>
  </div>

  <div class="about-section">
    <h3>
      <el-icon>
        <Reading/>
      </el-icon>
      {{ $t('settings.about.license') }}
    </h3>
    <ul class="feature-list">
      <li>
        <strong>FFmpeg</strong> - Copyright (c) FFmpeg developers。
      </li>
      <li>
        <strong>{{ $t('settings.about.usageGuide') }}</strong> - 本软件通过外部进程调用
        ffmpeg.exe，录屏功能启用时按需下载，不随安装包默认内置。
      </li>
      <li>
        <strong>{{ $t('settings.about.distribution') }}</strong> - ffmpeg.exe 下载到本地 bin 目录，下载地址可由
        settings.json 中
        recording_ffmpeg_download_url 配置。
      </li>
      <li>
        <strong>{{ $t('settings.about.licenseNote') }}</strong> - 当前使用的 FFmpeg 构建包含 GPL 组件（如 libx264），对应
        ffmpeg.exe 按 GPL/LGPL 要求分发。
      </li>
      <li>
        <strong>{{ $t('settings.about.correspondingSource') }}</strong> - FFmpeg 8.0.1 源码可从
        https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz 获取。
      </li>
      <li>
        <strong>{{ $t('settings.about.upstreamRepo') }}</strong> - https://git.ffmpeg.org/ffmpeg.git
      </li>
      <li>
        <strong>{{ $t('settings.about.buildRef') }}</strong> -
        https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.0.1-essentials_build.7z
      </li>
      <li>
        <strong>{{ $t('settings.about.fullStatement') }}</strong> - 详见 docs/THIRD_PARTY_NOTICES.md（含许可证文件与源码可获取信息）。
      </li>
    </ul>
  </div>
</template>

<script setup>
import {useI18n} from 'vue-i18n'
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

const {t} = useI18n()

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

const featureKeys = ['settings.about.feature1', 'settings.about.feature2', 'settings.about.feature3', 'settings.about.feature4', 'settings.about.feature5', 'settings.about.feature6']
const featureIcons = {
  'settings.about.feature1': CircleCheck,
  'settings.about.feature2': Picture,
  'settings.about.feature3': FolderOpened,
  'settings.about.feature4': Pointer,
  'settings.about.feature5': Picture,
  'settings.about.feature6': Cpu
}

const renderFeatureItem = (key) => {
  const text = t(key)
  const idx = text.indexOf(' - ')
  if (idx === -1) return text
  return `<strong>${text.slice(0, idx)}</strong> - ${text.slice(idx + 3)}`
}

const renderUsageItem = (key, shortcut, defaultShortcut) => {
  let text = t(key)
  const idx = text.indexOf('\uFF1A')
  let title = ''
  let body = text
  if (idx !== -1) {
    title = text.slice(0, idx)
    body = text.slice(idx + 1)
  }
  if (defaultShortcut && shortcut !== undefined) {
    body = body.replace(defaultShortcut, `<code>${shortcut || defaultShortcut}</code>`)
  }
  if (title) {
    return `<strong>${title}</strong>\uFF1A${body}`
  }
  return body
}
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
