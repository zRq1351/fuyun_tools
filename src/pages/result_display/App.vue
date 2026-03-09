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
        <div class="flag-icon">🇬🇧</div>
        <el-select v-model="sourceLanguage" size="small" style="width: 100px" @change="handleLanguageChange">
          <el-option label="英文" value="英文"/>
          <el-option label="中文" value="中文"/>
          <el-option label="日文" value="日文"/>
          <el-option label="韩文" value="韩文"/>
          <el-option label="法文" value="法文"/>
          <el-option label="德文" value="德文"/>
          <el-option label="西班牙文" value="西班牙文"/>
        </el-select>
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
        <div class="icon-btn close-btn" @click="handleClose">
          <el-icon>
            <Close/>
          </el-icon>
        </div>
      </div>
    </div>

    <div v-if="showOriginal" class="content original-content" v-html="originalHtml"></div>

    <div ref="resultRef" class="content result-content">
      <div v-html="resultHtml"></div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onMounted, ref} from 'vue'
import {marked} from 'marked'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {Close, Hide, View} from '@element-plus/icons-vue'
import {AIService} from '../../services/ipc'
import {handleAppError} from '../../utils/errorHandler'

const mode = ref('translation')
const originalText = ref('')
const resultText = ref('')
const showOriginal = ref(false)

const explanationLanguage = ref('中文')
const sourceLanguage = ref('英文')
const targetLanguage = ref('简体中文')

const resultRef = ref(null)

const originalHtml = computed(() => marked.parse(originalText.value))
const resultHtml = computed(() => marked.parse(resultText.value))

onMounted(async () => {
  const loadInitialData = () => {
    const initialData = window.__INITIAL_DATA__
    if (initialData) {
      mode.value = initialData.type || 'translation'
      originalText.value = initialData.original || ''
      resultText.value = initialData.content || ''

      scrollToBottom()
    }
  }

  loadInitialData()
  window.addEventListener('init-data', loadInitialData)

  try {
    await listen('result-clean', () => {
      resultText.value = ''
    })

    await listen('result-update', (event) => {
      const data = event.payload
      if (data.content) {
        resultText.value += data.content
        scrollToBottom()
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

const handleLanguageChange = async () => {
  if (!originalText.value) return

  resultText.value = ''

  try {
    if (mode.value === 'translation') {
      resultText.value = '正在翻译...'
      await AIService.streamTranslate(originalText.value, sourceLanguage.value, targetLanguage.value)
    } else {
      resultText.value = '正在解释...'
      await AIService.streamExplain(originalText.value, explanationLanguage.value)
    }
  } catch (error) {
    handleAppError(error, '请求失败')
    resultText.value = `Error: ${error.message || error}`
  }
}

const handleClose = async () => {
  await getCurrentWindow().close()
}
</script>

<style>
body {
  margin: 0;
  padding: 20px;
  background: #1e1e1e;
  color: #ffffff;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  overflow: hidden;
  height: 100vh;
  box-sizing: border-box;
}
</style>

<style scoped>
.container {
  display: flex;
  flex-direction: column;
  height: 100%;
  gap: 12px;
}

.header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  background: #2d2d2d;
  border-radius: 6px;
  border: 1px solid #444;
}

.control-group {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
}

.label {
  font-size: 14px;
  color: #ccc;
}

.flag-icon {
  font-size: 20px;
}

.arrow {
  color: #999;
}

.right-controls {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
}

.icon-btn {
  cursor: pointer;
  padding: 6px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
  color: #ccc;
  width: 20px;
  height: 20px;
}

.icon-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
}

.toggle-btn:hover {
  color: #409eff;
  background: rgba(64, 158, 255, 0.1);
}

.close-btn:hover {
  color: #f56c6c;
  background: rgba(245, 108, 108, 0.1);
}

.content {
  flex: 1;
  line-height: 1.6;
  overflow-y: auto;
  padding: 15px;
  background: #2d2d2d;
  border-radius: 8px;
  border: 1px solid #3a3a3a;
}

.original-content {
  flex: 0 0 auto;
  max-height: 30%;
  background: #252525;
  border-left: 4px solid #4CAF50;
  color: #cccccc;
  font-style: italic;
}

.result-content {
  border-left: 4px solid #2196F3;
}

.content::-webkit-scrollbar {
  width: 8px;
}

.content::-webkit-scrollbar-track {
  background: #333;
}

.content::-webkit-scrollbar-thumb {
  background: #555;
  border-radius: 4px;
}

.content::-webkit-scrollbar-thumb:hover {
  background: #666;
}

:deep(.content h1), :deep(.content h2), :deep(.content h3) {
  margin-top: 0.5em;
  margin-bottom: 0.5em;
  color: #ffffff;
}

:deep(.content p) {
  margin: 0.8em 0;
}

:deep(.content code) {
  background-color: #444;
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
