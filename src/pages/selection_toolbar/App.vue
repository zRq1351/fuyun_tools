<template>
  <div class="toolbar">
    <el-tooltip :show-after="500" content="翻译" placement="top">
      <div class="toolbar-button translate-btn" @click="handleTranslate">
        <el-icon>
          <collection/>
        </el-icon>
      </div>
    </el-tooltip>

    <el-tooltip :show-after="500" content="解释" placement="top">
      <div class="toolbar-button explain-btn" @click="handleExplain">
        <el-icon>
          <chat-line-round/>
        </el-icon>
      </div>
    </el-tooltip>

    <el-tooltip :show-after="500" content="复制" placement="top">
      <div class="toolbar-button copy-btn" @click="handleCopy">
        <el-icon>
          <document-copy/>
        </el-icon>
      </div>
    </el-tooltip>
  </div>
</template>

<script setup>
import {onMounted, ref} from 'vue'
import {ElMessage} from 'element-plus'
import {ChatLineRound, Collection, DocumentCopy} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'
import {listen} from '@tauri-apps/api/event'

const selectedText = ref('')

onMounted(async () => {
  try {
    await listen('selected-text', (event) => {
      selectedText.value = event.payload
    })
  } catch (error) {
    console.error('Listen error:', error)
  }
})

const handleTranslate = async () => {
  if (!selectedText.value) return
  try {
    await invoke('stream_translate_text', {
      text: selectedText.value,
      sourceLanguage: '英文',
      targetLanguage: '简体中文'
    })
  } catch (error) {
    handleError('翻译失败', error)
  }
}

const handleExplain = async () => {
  if (!selectedText.value) return
  try {
    await invoke('stream_explain_text', {
      text: selectedText.value,
      targetLanguage: '中文'
    })
  } catch (error) {
    handleError('解释失败', error)
  }
}

const handleCopy = async () => {
  if (!selectedText.value) return
  try {
    await invoke('copy_text', {text: selectedText.value})
    ElMessage.success('文本已复制')
  } catch (error) {
    console.error('复制失败:', error)
  }
}

const handleError = (context, error) => {
  console.error(`${context}:`, error)
  const errorMessage = error.toString()

  if (errorMessage.includes('未配置AI提供商')) {
    ElMessage.error('未配置 AI 提供商，请在设置中填写 API Key 与 Endpoint 后重试。')
  } else if (errorMessage.includes('API地址不能为空')) {
    ElMessage.error('API地址未配置，请在设置中填写正确的API地址。')
  } else if (errorMessage.includes('API密钥未配置')) {
    ElMessage.error('API密钥未配置，请在设置中填写正确的API密钥。')
  } else if (errorMessage.includes('模型名称不能为空')) {
    ElMessage.error('模型名称未配置，请在设置中填写正确的模型名称。')
  } else {
    ElMessage.error(`${context}: ${errorMessage}`)
  }
}
</script>

<style>
/* Global reset for this window */
body {
  margin: 0;
  padding: 0;
  background: transparent;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  overflow: hidden;
}
</style>

<style scoped>
.toolbar {
  background: rgba(25, 25, 25, 0.95);
  border-radius: 8px;
  padding: 4px;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
  backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.1);
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  box-sizing: border-box;
}

.toolbar-button {
  background: rgba(255, 255, 255, 0.1);
  border: none;
  color: white;
  padding: 8px 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 12px;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
}

.toolbar-button:hover {
  background: rgba(255, 255, 255, 0.2);
}

.toolbar-button:active {
  transform: scale(0.95);
}

.translate-btn {
  color: #67c23a; /* Element Plus Success color */
}

.explain-btn {
  color: #409eff; /* Element Plus Primary color */
}

.copy-btn {
  color: #e6a23c; /* Element Plus Warning color */
}

.btn-text {
  font-weight: 500;
}
</style>
