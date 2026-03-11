<template>
  <div class="container">
    <div class="header">
      <div v-if="mode === 'explanation'" class="control-group">
        <span class="label">解释语言：</span>
        <el-select v-model="explanationLanguage" size="small" style="width: 100px" @change="handleLanguageChange">
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
        <el-select v-model="targetLanguage" size="small" style="width: 100px" @change="handleLanguageChange">
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
import {computed, nextTick, onMounted, ref} from 'vue'
import {marked} from 'marked'
import {listen} from '@tauri-apps/api/event'
import {Hide, View} from '@element-plus/icons-vue'
import {AIService} from '../../services/ipc'
import {handleAppError} from '../../utils/errorHandler'

const mode = ref('translation')
const originalText = ref('')
const resultText = ref('')
const showOriginal = ref(false)

const explanationLanguage = ref('中文')
const targetLanguage = ref('简体中文')

const resultRef = ref(null)
const shouldAutoFollow = ref(true)
const originalRef = ref(null)
const isWaitingResult = ref(false)
const loadingStartedAt = ref(0)

const originalHtml = computed(() => marked.parse(originalText.value))
const resultHtml = computed(() => marked.parse(resultText.value))

onMounted(async () => {
  const loadInitialData = () => {
    const initialData = window.__INITIAL_DATA__
    if (initialData) {
      mode.value = initialData.type || 'translation'
      originalText.value = initialData.original || ''
      resultText.value = initialData.content || ''
      isWaitingResult.value = !resultText.value
      if (isWaitingResult.value) {
        loadingStartedAt.value = Date.now()
      }

      scrollToBottom()
    }
  }

  loadInitialData()
  window.addEventListener('init-data', loadInitialData)

  try {
    await listen('result-clean', (event) => {
      const data = event.payload
      if (data && data.type && data.type !== mode.value) return
      resultText.value = ''
      shouldAutoFollow.value = true
      isWaitingResult.value = true
      loadingStartedAt.value = Date.now()
    })

    await listen('result-update', (event) => {
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

</script>

<style>
html,
body {
  margin: 0;
  width: 100%;
  height: 100%;
}

body {
  padding: 14px;
  background: radial-gradient(120% 130% at 0% 0%, #20293a 0%, #161c28 46%, #111622 100%);
  color: #f2f6ff;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  overflow: hidden;
  height: 100vh;
  box-sizing: border-box;
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
  gap: 12px;
  min-height: 0;
  border-radius: 12px;
  padding: 2px;
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

.flag-icon {
  font-size: 20px;
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
