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
import {ChatLineRound, Collection, DocumentCopy} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {AIService, ClipboardService, WindowService} from '../../services/ipc'
import {handleAppError} from '../../utils/errorHandler'

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
    await AIService.streamTranslate(selectedText.value, '英文', '简体中文')
  } catch (error) {
    handleAppError(error, '翻译请求失败')
  }
}

const handleExplain = async () => {
  if (!selectedText.value) return
  try {
    await AIService.streamExplain(selectedText.value, '中文')
  } catch (error) {
    handleAppError(error, '解释请求失败')
  }
}

const handleCopy = async () => {
  if (!selectedText.value) return
  try {
    await ClipboardService.copyText(selectedText.value)
    await WindowService.selectionToolbarBlur()
  } catch (error) {
    handleAppError(error, '复制失败')
  }
}
</script>

<style>
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
  color: #67c23a;
}

.explain-btn {
  color: #409eff;
}

.copy-btn {
  color: #e6a23c;
}

.btn-text {
  font-weight: 500;
}
</style>
