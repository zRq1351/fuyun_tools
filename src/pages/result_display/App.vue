<template>
  <div :class="['container', `theme-${currentTheme}`]" @mousedown.left="handleContainerMouseDown">
    <div class="window-titlebar">
      <div class="window-title">
        {{ mode === 'translation' ? t('resultDisplay.translationResult') : t('resultDisplay.explanationResult') }}
      </div>
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
        <span class="label">{{ t('resultDisplay.explainLang') }}</span>
        <el-select v-model="explanationLanguage" class="lang-select" size="small" @change="handleLanguageChange">
          <el-option :label="t('resultDisplay.chinese')" value="中文"/>
          <el-option :label="t('resultDisplay.englishLang')" value="英文"/>
          <el-option :label="t('resultDisplay.japaneseLang')" value="日文"/>
          <el-option :label="t('resultDisplay.koreanLang')" value="韩文"/>
        </el-select>
      </div>

      <div v-if="mode === 'translation'" class="control-group">
        <span class="label">{{ t('resultDisplay.sourceText') }}</span>
        <span class="auto-source-tag">{{ t('resultDisplay.autoDetect') }}</span>
        <span class="arrow">→</span>
        <el-select v-model="targetLanguage" class="lang-select" size="small" @change="handleLanguageChange">
          <el-option :label="t('resultDisplay.simplifiedChinese')" value="简体中文"/>
          <el-option :label="t('resultDisplay.traditionalChinese')" value="繁体中文"/>
          <el-option :label="t('resultDisplay.english')" value="英语"/>
          <el-option :label="t('resultDisplay.japanese')" value="日语"/>
          <el-option :label="t('resultDisplay.korean')" value="韩语"/>
          <el-option :label="t('resultDisplay.french')" value="法语"/>
          <el-option :label="t('resultDisplay.german')" value="德语"/>
          <el-option :label="t('resultDisplay.spanish')" value="西班牙语"/>
        </el-select>
      </div>

      <div class="right-controls">
        <el-tooltip
            :content="showOriginal ? t('resultDisplay.hideSource') : t('resultDisplay.showSource')"
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

    <div v-if="showOriginal" class="content-wrapper original-wrapper">
      <div class="content-actions">
        <el-tooltip
            :show-after="500"
            :content="t('resultDisplay.copySource')"
            placement="bottom"
        >
          <div class="icon-btn action-btn copy-btn" @click="copyOriginalText">
            <el-icon>
              <DocumentCopy/>
            </el-icon>
          </div>
        </el-tooltip>
      </div>
      <!-- eslint-disable-next-line vue/no-v-html -->
      <div
          ref="originalRef"
          class="content original-content"
          v-html="originalHtml"
          @click="handleContentClick"
          @wheel.stop.prevent="handleContentWheel('original', $event)"
      ></div>
    </div>

    <div class="content-wrapper result-wrapper">
      <div class="content-actions">
        <el-tooltip
            :show-after="500"
            :content="t('resultDisplay.copyResult')"
            placement="bottom"
        >
          <div class="icon-btn action-btn copy-btn" @click="copyResultText">
            <el-icon>
              <DocumentCopy/>
            </el-icon>
          </div>
        </el-tooltip>
      </div>
      <div
          ref="resultRef"
          class="content result-content"
          @scroll="handleResultScroll"
          @click="handleContentClick"
          @wheel.stop.prevent="handleContentWheel('result', $event)"
      >
        <div v-if="isWaitingResult && !resultText" class="loading-wrap">
          <span class="loading-dot"></span>
          <span class="loading-dot"></span>
          <span class="loading-dot"></span>
          <span class="loading-text">{{ t('resultDisplay.generating') }}</span>
        </div>
        <!-- eslint-disable-next-line vue/no-v-html -->
        <div v-html="resultHtml"></div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {marked} from 'marked'
import DOMPurify from 'dompurify'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {invoke} from '@tauri-apps/api/core'
import {openUrl} from '@tauri-apps/plugin-opener'
import {ElMessage} from 'element-plus'
import {CloseBold, CopyDocument, DocumentCopy, FullScreen, Hide, Minus, View} from '@element-plus/icons-vue'
import {useI18n} from 'vue-i18n'
import {AIService, ClipboardService} from '@/services/ipc.js'
import {handleAppError} from '@/utils/errorHandler.js'
import {useTheme} from '../../composables/useTheme'
import {useEventListeners} from '../../composables/useEventListeners'

const {t} = useI18n()
const mode = ref('translation')
const originalText = ref('')
const resultText = ref('')
const showOriginal = ref(false)
const {currentTheme} = useTheme()

const explanationLanguage = ref('中文')
const targetLanguage = ref('简体中文')

const resultRef = ref(null)
const shouldAutoFollow = ref(true)
const originalRef = ref(null)
const isWaitingResult = ref(false)
const loadingStartedAt = ref(0)
const isWindowMaximized = ref(false)
const currentWindowLabel = ref('')
let initDataHandler = null
const currentWindow = getCurrentWindow()
const {listenEvent} = useEventListeners()
let unlistenResize = null

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
    handleAppError(error, t('resultDisplay.minimizeFailed'))
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
    handleAppError(error, t('resultDisplay.toggleSizeFailed'))
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
    DOMPurify.sanitize(marked.parse(markdownText || '', {
      renderer,
      gfm: true,
      breaks: true
    }))

const originalHtml = computed(() => renderMarkdownSafely(originalText.value))

// 流式更新时使用requestAnimationFrame节流，避免每次chunk都重新解析markdown
const resultHtmlRaw = ref('')
let rafId = null
let scrollRafId = null
let waitingTimeout = null
watch(resultText, (newText) => {
  if (rafId) cancelAnimationFrame(rafId)
  rafId = requestAnimationFrame(() => {
    resultHtmlRaw.value = renderMarkdownSafely(newText)
    rafId = null
    // 合并滚动操作到同一帧，避免频繁触发滚动
    if (shouldAutoFollow.value && !scrollRafId) {
      scrollRafId = requestAnimationFrame(() => {
        scrollToBottom()
        scrollRafId = null
      })
    }
  })
}, {immediate: true})
const resultHtml = computed(() => resultHtmlRaw.value)

onMounted(async () => {
  await syncWindowMaximized()
  unlistenResize = await currentWindow.onResized(async () => {
    await syncWindowMaximized()
  })

  // 获取当前窗口标签
  try {
    // Tauri v2 中窗口标签可以通过 label 属性直接访问
    currentWindowLabel.value = currentWindow.label || ''
  } catch (error) {
    console.error('Failed to get window label:', error)
  }

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
    await listenEvent('result-clean', (event) => {
      const data = event.payload
      // 验证窗口标签，只处理当前窗口的事件
      if (data && data.windowLabel && data.windowLabel !== currentWindowLabel.value) return
      if (data && data.type && data.type !== mode.value) return
      resultText.value = ''
      shouldAutoFollow.value = true
      isWaitingResult.value = true
      loadingStartedAt.value = Date.now()
    })

    await listenEvent('result-update', (event) => {
      const data = event.payload
      // 验证窗口标签，只处理当前窗口的事件
      if (data && data.windowLabel && data.windowLabel !== currentWindowLabel.value) return
      if (data && data.type && data.type !== mode.value) return
      if (data.content) {
        resultText.value += data.content
        const elapsed = Date.now() - loadingStartedAt.value
        if (isWaitingResult.value && elapsed < 280) {
          if (waitingTimeout) clearTimeout(waitingTimeout)
          waitingTimeout = window.setTimeout(() => {
            isWaitingResult.value = false
            waitingTimeout = null
          }, 280 - elapsed)
        } else {
          isWaitingResult.value = false
        }
        if (shouldAutoFollow.value) {
          scrollToBottom()
        }
      }
    })

    if (currentWindowLabel.value) {
      try {
        await invoke('notify_result_window_ready', {windowLabel: currentWindowLabel.value})
      } catch (e) {
        console.error('Failed to notify window ready:', e)
      }
    }
  } catch (error) {
    console.error('Failed to setup listeners:', error)
  }
})

onBeforeUnmount(() => {
  if (rafId) {
    cancelAnimationFrame(rafId)
    rafId = null
  }
  if (scrollRafId) {
    cancelAnimationFrame(scrollRafId)
    scrollRafId = null
  }
  if (waitingTimeout) {
    clearTimeout(waitingTimeout)
    waitingTimeout = null
  }
  if (unlistenResize) {
    unlistenResize()
  }
  if (initDataHandler) {
    window.removeEventListener('init-data', initDataHandler)
    initDataHandler = null
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

const handleContentClick = (event) => {
  const anchor = event.target.closest('a')
  if (!anchor) return
  const href = anchor.getAttribute('href')
  if (!href) return
  event.preventDefault()
  event.stopPropagation()
  openUrl(href).catch(() => {
    window.open(href, '_blank', 'noopener,noreferrer')
  })
}

const handleLanguageChange = async () => {
  if (!originalText.value) return

  resultText.value = ''
  isWaitingResult.value = true
  loadingStartedAt.value = Date.now()

  try {
    if (mode.value === 'translation') {
      await AIService.streamTranslate(originalText.value, '自动识别', targetLanguage.value, null, null, currentWindowLabel.value)
    } else {
      await AIService.streamExplain(originalText.value, explanationLanguage.value, null, null, currentWindowLabel.value)
    }
  } catch (error) {
    isWaitingResult.value = false
    handleAppError(error, t('resultDisplay.requestFailed'))
    resultText.value = `Error: ${error.message || error}`
  }
}

const copyOriginalText = async () => {
  const text = originalText.value.trim()
  if (!text) return
  try {
    await ClipboardService.copyText(text)
    ElMessage.success(t('resultDisplay.sourceCopied'))
  } catch (error) {
    handleAppError(error, t('resultDisplay.copySourceFailed'))
  }
}

const copyResultText = async () => {
  const text = resultText.value.trim()
  if (!text) return
  try {
    await ClipboardService.copyText(text)
    ElMessage.success(t('resultDisplay.resultCopied'))
  } catch (error) {
    handleAppError(error, t('resultDisplay.copyResultFailed'))
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
  background: var(--fy-bg-primary);
  color: var(--fy-text-primary);
  font-family: var(--fy-font-sans);
  overflow: hidden;
  height: 100vh;
  box-sizing: border-box;
}

#app {
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: var(--fy-container-bg);
  border-radius: var(--fy-radius-xl);
}
</style>

<style scoped>
.container {
  display: flex;
  flex-direction: column;
  height: 100%;
  gap: 10px;
  min-height: 0;
  border-radius: 16px;
  padding: 14px;
  box-sizing: border-box;
}

.window-titlebar {
  height: 34px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 10px 0 12px;
  border-radius: 10px;
  user-select: none;
  background: var(--fy-bg-overlay);
  border: 0.5px solid var(--fy-border-light);
}

.window-title {
  font-size: var(--fy-text-base);
  color: var(--fy-text-primary);
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
  border-radius: var(--fy-radius-sm);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: var(--fy-text-secondary);
  cursor: pointer;
  transition: all 0.15s ease;
}

.window-btn:hover {
  color: var(--fy-text-inverse);
  background: var(--fy-bg-hover);
}

.window-btn-close:hover {
  background: var(--fy-danger);
  color: var(--fy-text-primary);
}

.header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  background: var(--fy-bg-overlay);
  border-radius: var(--fy-radius-lg);
  border: 0.5px solid var(--fy-border-light);
  box-shadow: var(--fy-shadow);
}

.control-group {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
}

.label {
  font-size: var(--fy-text-md);
  color: var(--fy-text-primary);
}

.arrow {
  color: var(--fy-text-muted);
}

.auto-source-tag {
  font-size: var(--fy-text-base);
  color: var(--fy-text-accent);
  background: var(--fy-accent-bg);
  border: 0.5px solid var(--fy-border-active);
  border-radius: var(--fy-radius-sm);
  padding: 4px 8px;
}

.right-controls {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
  padding-left: 8px;
  border-left: 0.5px solid var(--fy-border-light);
}

.icon-btn {
  cursor: pointer;
  padding: 4px;
  border-radius: var(--fy-radius-xs);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s ease;
  color: var(--fy-text-secondary);
  width: 22px;
  height: 22px;
}

.icon-btn:hover {
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
}

.toggle-btn:hover {
  color: var(--fy-accent);
  background: var(--fy-accent-bg);
}

.content-wrapper {
  position: relative;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.original-wrapper {
  flex: 0 0 auto;
  max-height: 30%;
}

.result-wrapper {
  flex: 1;
}

.content-actions {
  position: absolute;
  top: 10px;
  right: 14px;
  display: flex;
  gap: 6px;
  z-index: 10;
}

.content-actions .action-btn {
  background: var(--fy-glass-bg);
  backdrop-filter: var(--fy-backdrop-blur-light);
  border: 0.5px solid var(--fy-glass-border);
}

.content-actions .action-btn:hover {
  background: var(--fy-bg-hover);
  border-color: var(--fy-border-hover);
}

.content {
  flex: 1;
  line-height: 1.6;
  overflow-y: auto;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
  touch-action: pan-y;
  padding: 16px;
  padding-top: 36px;
  background: var(--fy-content-bg);
  border-radius: var(--fy-radius-lg);
  border: 0.5px solid var(--fy-content-border);
  min-height: 0;
  color: var(--fy-text-primary);
}

.original-content {
  background: var(--fy-bg-surface);
  border-left: 4px solid var(--fy-success);
  color: var(--fy-text-primary);
  font-style: italic;
}

.result-content {
  border-left: 4px solid var(--fy-accent);
  position: relative;
}

.loading-wrap {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  color: var(--fy-text-primary);
  font-size: var(--fy-text-base);
  letter-spacing: 0.4px;
  background: var(--fy-bg-primary);
  border-radius: var(--fy-radius-md);
  z-index: 2;
}

.loading-dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: var(--fy-accent);
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
  background: var(--fy-scrollbar-track);
}

.content::-webkit-scrollbar-thumb {
  background: var(--fy-scrollbar-thumb);
  border-radius: var(--fy-radius-xs);
}

.content::-webkit-scrollbar-thumb:hover {
  background: var(--fy-scrollbar-thumb-hover);
}

:deep(.content h1), :deep(.content h2), :deep(.content h3) {
  margin-top: 0.5em;
  margin-bottom: 0.5em;
  color: var(--fy-text-primary);
}

:deep(.content p) {
  margin: 0.8em 0;
  color: var(--fy-text-primary);
}

:deep(.content code) {
  background-color: var(--fy-bg-hover);
  padding: 0.2em 0.4em;
  border-radius: var(--fy-radius-xs);
  font-family: var(--fy-font-mono);
}

:deep(.content pre) {
  background-color: var(--fy-bg-primary);
  padding: 1em;
  border-radius: var(--fy-radius-sm);
  overflow-x: auto;
  margin: 0.8em 0;
}

:deep(.content pre code) {
  background: none;
  padding: 0;
}

:deep(.content a) {
  color: var(--fy-accent);
}

:deep(.content blockquote) {
  border-left: 3px solid var(--fy-border);
  padding-left: 1em;
  margin: 0.8em 0;
  color: var(--fy-text-muted);
}
</style>
