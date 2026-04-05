<template>
  <div :class="['container', `theme-${themeMode}`]" @mousedown.left="handleContainerMouseDown">
    <div class="window-titlebar">
      <div class="window-title">{{ mode === 'translation' ? '翻译结果' : '解释结果' }}</div>
      <div class="window-controls">
        <button class="window-btn" @click.stop="minimizeWindow" @mousedown.stop>
          <el-icon>
            <Minus/>
          </el-icon>
        </button>
        <button class="window-btn" @click.stop="toggleWindowMaximize" @mousedown.stop>
          <el-icon>
            <CopyDocument v-if="isWindowMaximized"/>
            <FullScreen v-else/>
          </el-icon>
        </button>
        <button class="window-btn window-btn-close" @click.stop="closeWindow" @mousedown.stop>
          <el-icon>
            <CloseBold/>
          </el-icon>
        </button>
      </div>
    </div>
    <div class="header">
      <div v-if="mode === 'explanation'" class="control-group">
        <span class="label">解释语言：</span>
        <el-select v-model="explanationLanguage" class="lang-select" size="small" @change="handleLanguageChange">
          <el-option label="中文" value="中文"/>
          <el-option label="英文" value="英文"/>
          <el-option label="日文" value="日文"/>
          <el-option label="韩文" value="韩文"/>
        </el-select>
      </div>

      <div v-if="mode === 'translation'" class="control-group">
        <span class="label">原文：</span>
        <span class="auto-source-tag">自动识别</span>
        <span class="arrow">→</span>
        <el-select v-model="targetLanguage" class="lang-select" size="small" @change="handleLanguageChange">
          <el-option label="简体中文" value="简体中文"/>
          <el-option label="繁体中文" value="繁体中文"/>
          <el-option label="英语" value="英语"/>
          <el-option label="日语" value="日语"/>
          <el-option label="韩语" value="韩语"/>
          <el-option label="法语" value="法语"/>
          <el-option label="德语" value="德语"/>
          <el-option label="西班牙语" value="西班牙语"/>
        </el-select>
      </div>

      <div class="right-controls">
        <el-tooltip
            content="回写到原应用"
            :show-after="500"
            placement="bottom"
        >
          <div class="icon-btn writeback-btn" @click="handleWriteBack">
            <el-icon>
              <Position/>
            </el-icon>
          </div>
        </el-tooltip>
        <el-tooltip
            :show-after="500"
            content="复制原文"
            placement="bottom"
        >
          <div class="icon-btn copy-btn" @click="copyOriginalText">
            <el-icon>
              <DocumentCopy/>
            </el-icon>
          </div>
        </el-tooltip>
        <el-tooltip
            :show-after="500"
            content="复制结果"
            placement="bottom"
        >
          <div class="icon-btn copy-btn" @click="copyResultText">
            <el-icon>
              <DocumentCopy/>
            </el-icon>
          </div>
        </el-tooltip>
        <el-tooltip
            :content="showOriginal ? '隐藏原文' : '显示原文'"
            :show-after="500"
            placement="bottom"
        >
          <div class="icon-btn toggle-btn" @click="toggleOriginal">
            <el-icon>
              <Hide v-if="showOriginal"/>
              <View v-else/>
            </el-icon>
          </div>
        </el-tooltip>
      </div>
    </div>

    <div
        v-if="showOriginal"
        ref="originalRef"
        class="content original-content"
        v-html="originalHtml"
        @wheel.stop.prevent="handleContentWheel('original', $event)"
    ></div>

    <div
        ref="resultRef"
        class="content result-content"
        @scroll="handleResultScroll"
        @wheel.stop.prevent="handleContentWheel('result', $event)"
    >
      <div v-if="isWaitingResult && !resultText" class="loading-wrap">
        <span class="loading-dot"></span>
        <span class="loading-dot"></span>
        <span class="loading-dot"></span>
        <span class="loading-text">正在生成结果</span>
      </div>
      <div v-html="resultHtml"></div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, ref} from 'vue'
import {marked} from 'marked'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {CloseBold, CopyDocument, DocumentCopy, FullScreen, Hide, Minus, Position, View} from '@element-plus/icons-vue'
import {AIService, ClipboardService} from '@/services/ipc.js'
import {handleAppError} from '@/utils/errorHandler.js'

const mode = ref('translation')
const originalText = ref('')
const resultText = ref('')
const showOriginal = ref(false)
const themeMode = ref('dark')

const explanationLanguage = ref('中文')
const targetLanguage = ref('简体中文')

const resultRef = ref(null)
const shouldAutoFollow = ref(true)
const originalRef = ref(null)
const isWaitingResult = ref(false)
const loadingStartedAt = ref(0)
const isWindowMaximized = ref(false)
const isWriteBackInFlight = ref(false)
let unlistenResultClean = null
let unlistenResultUpdate = null
let initDataHandler = null
let onStorageThemeChange = null
let unlistenWindowResize = null
const currentWindow = getCurrentWindow()

const getCurrentTheme = () => {
  const saved = localStorage.getItem('settings-theme')
  if (saved === 'dark' || saved === 'light') {
    return saved
  }
  return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

const applyTheme = (value) => {
  const next = value === 'light' ? 'light' : 'dark'
  themeMode.value = next
  document.documentElement.classList.toggle('theme-light', next === 'light')
  document.documentElement.classList.toggle('theme-dark', next === 'dark')
  document.body.classList.toggle('theme-light', next === 'light')
  document.body.classList.toggle('theme-dark', next === 'dark')
}

const syncWindowMaximized = async () => {
  try {
    const [isMaximized, isFullscreen] = await Promise.all([
      currentWindow.isMaximized(),
      currentWindow.isFullscreen()
    ])
    isWindowMaximized.value = isMaximized || isFullscreen
  } catch (_) {
    isWindowMaximized.value = false
  }
}

const startWindowDrag = async () => {
  try {
    await currentWindow.startDragging()
  } catch (_) {
  }
}

const handleContainerMouseDown = (event) => {
  const rawTarget = event.target
  const target = rawTarget instanceof Element
      ? rawTarget
      : (rawTarget instanceof Node ? rawTarget.parentElement : null)
  if (!target) return
  if (target.closest('.original-content') || target.closest('.result-content')) return
  if (target.closest('button, input, textarea, select, option, a, [role="button"], [contenteditable="true"]')) return
  if (target.closest('.el-select, .el-input, .el-textarea, .el-button, .icon-btn')) return
  startWindowDrag()
}

const minimizeWindow = async () => {
  try {
    await currentWindow.minimize()
  } catch (error) {
    handleAppError(error, '最小化窗口失败')
  }
}

const toggleWindowMaximize = async () => {
  try {
    const fullscreen = await currentWindow.isFullscreen()
    if (fullscreen) {
      await currentWindow.setFullscreen(false)
    } else {
      await currentWindow.setFullscreen(true)
    }
    isWindowMaximized.value = !fullscreen
  } catch (error) {
    handleAppError(error, '切换窗口放大状态失败')
  }
}

const closeWindow = async () => {
  try {
    await currentWindow.close()
  } catch (_) {
  }
}

const escapeHtml = (value = '') =>
    value
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;')

const isSafeUrl = (rawUrl) => {
  if (!rawUrl) return false
  if (rawUrl.startsWith('#') || rawUrl.startsWith('/')) return true
  try {
    const parsed = new URL(rawUrl, 'https://local.invalid')
    return ['http:', 'https:', 'mailto:', 'tel:'].includes(parsed.protocol)
  } catch {
    return false
  }
}

const renderer = new marked.Renderer()
renderer.html = (...args) => {
  const raw = typeof args[0] === 'string' ? args[0] : (args[0]?.text || '')
  return escapeHtml(raw)
}
renderer.link = (...args) => {
  let href = ''
  let title = ''
  let text = ''
  if (args.length === 1 && typeof args[0] === 'object') {
    href = args[0]?.href || ''
    title = args[0]?.title || ''
    text = args[0]?.text || ''
  } else {
    href = args[0] || ''
    title = args[1] || ''
    text = args[2] || ''
  }
  if (!isSafeUrl(href)) {
    return `<span>${escapeHtml(text || href)}</span>`
  }
  const safeTitle = title ? ` title="${escapeHtml(title)}"` : ''
  return `<a href="${escapeHtml(href)}"${safeTitle} target="_blank" rel="noopener noreferrer nofollow">${escapeHtml(text || href)}</a>`
}
renderer.image = (...args) => {
  let href = ''
  let title = ''
  let text = ''
  if (args.length === 1 && typeof args[0] === 'object') {
    href = args[0]?.href || ''
    title = args[0]?.title || ''
    text = args[0]?.text || ''
  } else {
    href = args[0] || ''
    title = args[1] || ''
    text = args[2] || ''
  }
  if (!isSafeUrl(href)) {
    return ''
  }
  const safeTitle = title ? ` title="${escapeHtml(title)}"` : ''
  return `<img src="${escapeHtml(href)}" alt="${escapeHtml(text || '')}"${safeTitle} loading="lazy">`
}

const renderMarkdownSafely = (markdownText) =>
    marked.parse(markdownText || '', {
      renderer,
      gfm: true,
      breaks: true
    })

const originalHtml = computed(() => renderMarkdownSafely(originalText.value))
const resultHtml = computed(() => renderMarkdownSafely(resultText.value))

onMounted(async () => {
  applyTheme(getCurrentTheme())
  onStorageThemeChange = (event) => {
    if (!event || event.key === 'settings-theme') {
      applyTheme(getCurrentTheme())
    }
  }
  window.addEventListener('storage', onStorageThemeChange)
  await syncWindowMaximized()
  unlistenWindowResize = await currentWindow.onResized(async () => {
    await syncWindowMaximized()
  })

  initDataHandler = () => {
    const initialData = window.__INITIAL_DATA__
    if (initialData) {
      mode.value = initialData.type || 'translation'
      originalText.value = initialData.original || ''
      resultText.value = initialData.content || ''
      if (initialData.targetLanguage) {
        if (mode.value === 'translation') {
          targetLanguage.value = initialData.targetLanguage
        } else {
          explanationLanguage.value = initialData.targetLanguage
        }
      }
      isWaitingResult.value = !resultText.value
      if (isWaitingResult.value) {
        loadingStartedAt.value = Date.now()
      }

      scrollToBottom()
    }
  }

  initDataHandler()
  window.addEventListener('init-data', initDataHandler)

  try {
    unlistenResultClean = await listen('result-clean', (event) => {
      const data = event.payload
      if (data && data.type && data.type !== mode.value) return
      resultText.value = ''
      shouldAutoFollow.value = true
      isWaitingResult.value = true
      loadingStartedAt.value = Date.now()
    })

    unlistenResultUpdate = await listen('result-update', (event) => {
      const data = event.payload
      if (data && data.type && data.type !== mode.value) return
      if (data.content) {
        resultText.value += data.content
        const elapsed = Date.now() - loadingStartedAt.value
        if (isWaitingResult.value && elapsed < 280) {
          window.setTimeout(() => {
            isWaitingResult.value = false
          }, 280 - elapsed)
        } else {
          isWaitingResult.value = false
        }
        if (shouldAutoFollow.value) {
          scrollToBottom()
        }
      }
    })
  } catch (error) {
    console.error('Failed to setup listeners:', error)
  }
})

onBeforeUnmount(() => {
  if (unlistenWindowResize) {
    unlistenWindowResize()
    unlistenWindowResize = null
  }
  if (onStorageThemeChange) {
    window.removeEventListener('storage', onStorageThemeChange)
    onStorageThemeChange = null
  }
  if (initDataHandler) {
    window.removeEventListener('init-data', initDataHandler)
    initDataHandler = null
  }
  if (unlistenResultClean) {
    unlistenResultClean()
    unlistenResultClean = null
  }
  if (unlistenResultUpdate) {
    unlistenResultUpdate()
    unlistenResultUpdate = null
  }
})

const scrollToBottom = () => {
  nextTick(() => {
    if (resultRef.value) {
      resultRef.value.scrollTop = resultRef.value.scrollHeight
    }
  })
}

const toggleOriginal = () => {
  showOriginal.value = !showOriginal.value
}

const handleResultScroll = () => {
  if (!resultRef.value) return
  const remain = resultRef.value.scrollHeight - resultRef.value.scrollTop - resultRef.value.clientHeight
  shouldAutoFollow.value = remain <= 24
}

const handleContentWheel = (target, event) => {
  const container = target === 'result' ? resultRef.value : originalRef.value
  if (!container) return
  container.scrollTop += event.deltaY
  if (target === 'result') {
    const remain = container.scrollHeight - container.scrollTop - container.clientHeight
    shouldAutoFollow.value = remain <= 24
  }
}

const handleLanguageChange = async () => {
  if (!originalText.value) return

  resultText.value = ''
  isWaitingResult.value = true
  loadingStartedAt.value = Date.now()

  try {
    if (mode.value === 'translation') {
      await AIService.streamTranslate(originalText.value, '自动识别', targetLanguage.value)
    } else {
      await AIService.streamExplain(originalText.value, explanationLanguage.value)
    }
  } catch (error) {
    isWaitingResult.value = false
    handleAppError(error, '请求失败')
    resultText.value = `Error: ${error.message || error}`
  }
}

const handleWriteBack = async () => {
  if (isWriteBackInFlight.value) return
  const text = resultText.value.trim()
  if (!text) return
  const requestId = `wb-${Date.now()}-${text.length}`
  isWriteBackInFlight.value = true
  try {
    await ClipboardService.copyAndPasteText(text, requestId)
  } catch (error) {
    handleAppError(error, '回写失败')
  } finally {
    isWriteBackInFlight.value = false
  }
}

const copyOriginalText = async () => {
  const text = originalText.value.trim()
  if (!text) return
  try {
    await ClipboardService.copyText(text)
    ElMessage.success('已复制原文')
  } catch (error) {
    handleAppError(error, '复制原文失败')
  }
}

const copyResultText = async () => {
  const text = resultText.value.trim()
  if (!text) return
  try {
    await ClipboardService.copyText(text)
    ElMessage.success('已复制结果')
  } catch (error) {
    handleAppError(error, '复制结果失败')
  }
}

</script>

<style>
html,
body {
  margin: 0;
  width: 100%;
  height: 100%;
}

body {
  padding: 0;
  background: radial-gradient(120% 130% at 0% 0%, #20293a 0%, #161c28 46%, #111622 100%);
  color: #f2f6ff;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  overflow: hidden;
  height: 100vh;
  box-sizing: border-box;
}

body.theme-light {
  background: radial-gradient(120% 130% at 0% 0%, #f2f6ff 0%, #eef3ff 46%, #e8eefb 100%);
  color: #1f2a3d;
}

#app {
  width: 100%;
  height: 100%;
  overflow: hidden;
}
</style>

<style scoped>
.container {
  display: flex;
  flex-direction: column;
  height: 100%;
  gap: 10px;
  min-height: 0;
  border-radius: 12px;
  padding: 14px;
  box-sizing: border-box;
}

.window-titlebar {
  height: 34px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 8px 0 12px;
  border-radius: 10px;
  user-select: none;
  background: linear-gradient(145deg, rgba(31, 39, 56, 0.9), rgba(23, 30, 45, 0.9));
  border: 1px solid rgba(151, 174, 224, 0.18);
}

.window-title {
  font-size: 13px;
  color: #dbe7ff;
  font-weight: 600;
}

.window-controls {
  display: flex;
  align-items: center;
  gap: 6px;
}

.window-btn {
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: #c9d8f6;
  cursor: pointer;
  transition: all 0.2s;
}

.window-btn:hover {
  color: #fff;
  background: rgba(140, 168, 220, 0.2);
}

.window-btn-close:hover {
  background: rgba(245, 108, 108, 0.9);
  color: #fff;
}

.container.theme-light .header {
  background: linear-gradient(145deg, rgba(251, 253, 255, 0.96), rgba(244, 248, 255, 0.95));
  border: 1px solid rgba(154, 172, 206, 0.45);
  box-shadow: 0 8px 22px rgba(120, 140, 180, 0.14);
}

.container.theme-light .window-titlebar {
  background: linear-gradient(145deg, rgba(250, 253, 255, 0.96), rgba(242, 248, 255, 0.95));
  border-color: rgba(154, 172, 206, 0.45);
}

.container.theme-light .window-title {
  color: #2a4163;
}

.container.theme-light .window-btn {
  color: #4c6084;
}

.container.theme-light .window-btn:hover {
  background: rgba(122, 155, 220, 0.16);
  color: #1d3158;
}

.container.theme-light .window-btn-close:hover {
  background: rgba(245, 108, 108, 0.9);
  color: #fff;
}

.container.theme-light .label {
  color: #314464;
}

.container.theme-light .arrow {
  color: #63789d;
}

.container.theme-light .auto-source-tag {
  color: #325186;
  background: rgba(64, 158, 255, 0.14);
  border-color: rgba(64, 158, 255, 0.32);
}

.container.theme-light .right-controls {
  border-left: 1px solid rgba(134, 156, 192, 0.35);
}

.container.theme-light .icon-btn {
  color: #4c6084;
}

.container.theme-light .icon-btn:hover {
  background: rgba(122, 155, 220, 0.16);
  color: #1d3158;
}

.container.theme-light .writeback-btn:hover {
  color: #2b8a3e;
  background: rgba(103, 194, 58, 0.16);
}

.container.theme-light .content {
  background: linear-gradient(150deg, rgba(249, 252, 255, 0.97), rgba(241, 247, 255, 0.96));
  border: 1px solid rgba(162, 182, 218, 0.45);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.82);
  color: #243957;
}

.container.theme-light .original-content {
  background: linear-gradient(150deg, rgba(236, 249, 241, 0.95), rgba(226, 243, 233, 0.95));
  border-left-color: #4cb77b;
  color: #355746;
}

.container.theme-light .result-content {
  border-left-color: #4d97ea;
}

.container.theme-light .loading-wrap {
  color: #3d557d;
  background: linear-gradient(160deg, rgba(240, 246, 255, 0.92), rgba(234, 241, 253, 0.86));
}

.lang-select {
  width: 100px;
}

.container.theme-light .loading-dot {
  background: #6599e8;
}

.container.theme-light .content::-webkit-scrollbar-track {
  background: rgba(208, 220, 243, 0.72);
}

.container.theme-light .content::-webkit-scrollbar-thumb {
  background: rgba(112, 141, 193, 0.52);
}

.container.theme-light .content::-webkit-scrollbar-thumb:hover {
  background: rgba(97, 128, 183, 0.72);
}

.container.theme-light :deep(.content h1),
.container.theme-light :deep(.content h2),
.container.theme-light :deep(.content h3) {
  color: #1f2f49;
}

.container.theme-light :deep(.content p) {
  color: #2e4467;
}

.container.theme-light :deep(.content code) {
  background-color: rgba(96, 127, 188, 0.16);
}

.container.theme-light :deep(.content pre) {
  background-color: #f0f4fc;
}

.container.theme-light :deep(.content a) {
  color: #2f73cf;
}

.container.theme-light :deep(.content blockquote) {
  border-left-color: #8ea6d1;
  color: #56709a;
}

.header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  background: linear-gradient(145deg, rgba(35, 43, 60, 0.92), rgba(26, 33, 48, 0.9));
  border-radius: 10px;
  border: 1px solid rgba(173, 198, 255, 0.18);
  box-shadow: 0 8px 22px rgba(5, 10, 20, 0.28);
}

.control-group {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
}

.label {
  font-size: 14px;
  color: #d8e2f7;
}

.arrow {
  color: #9fb3d9;
}

.auto-source-tag {
  font-size: 13px;
  color: #d6e3ff;
  background: rgba(128, 164, 255, 0.18);
  border: 1px solid rgba(151, 184, 255, 0.36);
  border-radius: 6px;
  padding: 4px 8px;
}

.right-controls {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
  padding-left: 8px;
  border-left: 1px solid rgba(170, 190, 230, 0.2);
}

.icon-btn {
  cursor: pointer;
  padding: 6px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
  color: #cfdbf6;
  width: 22px;
  height: 22px;
}

.icon-btn:hover {
  background: rgba(146, 176, 237, 0.18);
  color: #fff;
}

.toggle-btn:hover {
  color: #409eff;
  background: rgba(64, 158, 255, 0.18);
}

.writeback-btn:hover {
  color: #67c23a;
  background: rgba(103, 194, 58, 0.18);
}

.content {
  flex: 1;
  line-height: 1.6;
  overflow-y: auto;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
  touch-action: pan-y;
  padding: 15px;
  background: linear-gradient(150deg, rgba(29, 37, 54, 0.96), rgba(20, 27, 41, 0.94));
  border-radius: 10px;
  border: 1px solid rgba(166, 189, 240, 0.18);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.03);
  min-height: 0;
  color: #eaf1ff;
}

.original-content {
  flex: 0 0 auto;
  max-height: 30%;
  background: linear-gradient(150deg, rgba(28, 48, 40, 0.9), rgba(20, 35, 30, 0.9));
  border-left: 4px solid #53c58a;
  color: #d5eee2;
  font-style: italic;
}

.result-content {
  border-left: 4px solid #63aaf6;
  min-height: 0;
  position: relative;
}

.loading-wrap {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  color: #cfe0ff;
  font-size: 13px;
  letter-spacing: 0.4px;
  background: linear-gradient(160deg, rgba(23, 31, 47, 0.82), rgba(17, 24, 38, 0.7));
  border-radius: 8px;
  z-index: 2;
}

.loading-dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: #8bb6ff;
  display: inline-block;
  animation: loading-bounce 1s ease-in-out infinite;
}

.loading-dot:nth-child(2) {
  animation-delay: 0.15s;
}

.loading-dot:nth-child(3) {
  animation-delay: 0.3s;
}

.loading-text {
  margin-left: 4px;
}

@keyframes loading-bounce {
  0%,
  80%,
  100% {
    transform: translateY(0);
    opacity: 0.45;
  }
  40% {
    transform: translateY(-5px);
    opacity: 1;
  }
}

.content::-webkit-scrollbar {
  width: 8px;
}

.content::-webkit-scrollbar-track {
  background: rgba(34, 44, 66, 0.78);
}

.content::-webkit-scrollbar-thumb {
  background: rgba(136, 164, 222, 0.6);
  border-radius: 4px;
}

.content::-webkit-scrollbar-thumb:hover {
  background: rgba(165, 189, 236, 0.8);
}

:deep(.content h1), :deep(.content h2), :deep(.content h3) {
  margin-top: 0.5em;
  margin-bottom: 0.5em;
  color: #f7fbff;
}

:deep(.content p) {
  margin: 0.8em 0;
  color: #dde8ff;
}

:deep(.content code) {
  background-color: rgba(105, 135, 194, 0.2);
  padding: 0.2em 0.4em;
  border-radius: 3px;
  font-family: 'Courier New', monospace;
}

:deep(.content pre) {
  background-color: #222;
  padding: 1em;
  border-radius: 5px;
  overflow-x: auto;
  margin: 0.8em 0;
}

:deep(.content pre code) {
  background: none;
  padding: 0;
}

:deep(.content a) {
  color: #4CAF50;
}

:deep(.content blockquote) {
  border-left: 3px solid #666;
  padding-left: 1em;
  margin: 0.8em 0;
  color: #ccc;
}
</style>
