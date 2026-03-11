<template>
  <div class="toolbar">
    <el-tooltip :show-after="500" content="翻译" placement="top">
      <div :class="{ disabled: actionLoading }" class="toolbar-button translate-btn" @click="handleTranslate">
        <el-icon class="btn-icon">
          <collection/>
        </el-icon>
        <span class="btn-text">翻译</span>
      </div>
    </el-tooltip>

    <el-tooltip :show-after="500" content="解释" placement="top">
      <div :class="{ disabled: actionLoading }" class="toolbar-button explain-btn" @click="handleExplain">
        <el-icon class="btn-icon">
          <chat-line-round/>
        </el-icon>
        <span class="btn-text">解释</span>
      </div>
    </el-tooltip>

    <el-tooltip :show-after="500" content="复制" placement="top">
      <div :class="{ disabled: actionLoading }" class="toolbar-button copy-btn" @click="handleCopy">
        <el-icon class="btn-icon">
          <document-copy/>
        </el-icon>
        <span class="btn-text">复制</span>
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
const actionLoading = ref(false)

const getSafeSelectedText = () => selectedText.value.trim()

onMounted(async () => {
  try {
    await listen('selected-text', (event) => {
      selectedText.value = typeof event.payload === 'string' ? event.payload : ''
    })
  } catch (error) {
    console.error('Listen error:', error)
  }
})

const handleTranslate = async () => {
  const text = getSafeSelectedText()
  if (!text || actionLoading.value) return
  actionLoading.value = true
  try {
    await WindowService.selectionToolbarBlur()
    await AIService.streamTranslate(text, '自动识别', '简体中文')
  } catch (error) {
    handleAppError(error, '翻译请求失败')
  } finally {
    actionLoading.value = false
  }
}

const handleExplain = async () => {
  const text = getSafeSelectedText()
  if (!text || actionLoading.value) return
  actionLoading.value = true
  try {
    await WindowService.selectionToolbarBlur()
    await AIService.streamExplain(text, '中文')
  } catch (error) {
    handleAppError(error, '解释请求失败')
  } finally {
    actionLoading.value = false
  }
}

const handleCopy = async () => {
  const text = getSafeSelectedText()
  if (!text || actionLoading.value) return
  actionLoading.value = true
  try {
    await ClipboardService.copyText(text)
    await WindowService.selectionToolbarBlur()
  } catch (error) {
    handleAppError(error, '复制失败')
  } finally {
    actionLoading.value = false
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
  background: linear-gradient(145deg, rgba(22, 28, 38, 0.95), rgba(14, 18, 26, 0.95));
  border-radius: 10px;
  padding: 5px;
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.35), 0 2px 8px rgba(0, 0, 0, 0.2);
  backdrop-filter: blur(14px);
  border: 1px solid rgba(255, 255, 255, 0.12);
  display: flex;
  flex-direction: row;
  gap: 5px;
  width: auto;
  box-sizing: border-box;
}

.toolbar-button {
  background: rgba(255, 255, 255, 0.08);
  border: none;
  color: #eef3ff;
  width: 52px;
  height: 38px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 17px;
  transition: all 0.18s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid rgba(255, 255, 255, 0.08);
  position: relative;
  overflow: hidden;
}

.toolbar-button:hover {
  transform: translateY(-1px);
}

.toolbar-button:active {
  transform: scale(0.97);
}

.toolbar-button.disabled {
  opacity: 0.55;
  pointer-events: none;
}

.btn-icon {
  opacity: 1;
  transform: translateY(0);
  transition: all 0.18s ease;
}

.btn-text {
  position: absolute;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.5px;
  opacity: 0;
  transform: translateY(8px);
  transition: all 0.18s ease;
}

.toolbar-button:hover .btn-icon {
  opacity: 0;
  transform: translateY(-8px);
}

.toolbar-button:hover .btn-text {
  opacity: 1;
  transform: translateY(0);
}

.translate-btn {
  color: #7be682;
  background: linear-gradient(145deg, rgba(82, 165, 112, 0.22), rgba(44, 96, 65, 0.2));
}

.explain-btn {
  color: #72b7ff;
  background: linear-gradient(145deg, rgba(84, 148, 230, 0.22), rgba(44, 83, 150, 0.2));
}

.copy-btn {
  color: #f2c06d;
  background: linear-gradient(145deg, rgba(209, 152, 61, 0.22), rgba(133, 89, 35, 0.2));
}

</style>
