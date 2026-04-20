<template>
  <div class="container" @mouseenter="onMouseEnter" @mouseleave="onMouseLeave">
    <div v-if="!isHovered" class="mini-icon" data-tauri-drag-region>
      <el-icon class="magic-icon"><magic-stick/></el-icon>
    </div>
    
    <div v-else class="toolbar" data-tauri-drag-region>
      <el-tooltip :show-after="500" content="翻译" placement="top">
        <div :class="{ disabled: actionLoading }" class="toolbar-button translate-btn no-drag" @click="handleTranslate">
          <el-icon class="btn-icon">
            <collection/>
          </el-icon>
          <span class="btn-text">翻译</span>
        </div>
      </el-tooltip>

      <el-tooltip :show-after="500" content="解释" placement="top">
        <div :class="{ disabled: actionLoading }" class="toolbar-button explain-btn no-drag" @click="handleExplain">
          <el-icon class="btn-icon">
            <chat-line-round/>
          </el-icon>
          <span class="btn-text">解释</span>
        </div>
      </el-tooltip>

      <el-tooltip :show-after="500" content="复制" placement="top">
        <div :class="{ disabled: actionLoading }" class="toolbar-button copy-btn no-drag" @click="handleCopy">
          <el-icon class="btn-icon">
            <document-copy/>
          </el-icon>
          <span class="btn-text">复制</span>
        </div>
      </el-tooltip>
    </div>
  </div>
</template>

<script setup>
import {onBeforeUnmount, onMounted, ref} from 'vue'
import {ChatLineRound, Collection, DocumentCopy, MagicStick} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {AIService, AISettingsService, ClipboardService, WindowService} from '../../services/ipc'
import {handleAppError} from '../../utils/errorHandler'

const selectedText = ref('')
const actionLoading = ref(false)
const isHovered = ref(false)
let unlistenSelectedText = null
let unlistenDomText = null

const onMouseEnter = async () => {
  if (isHovered.value) return
  isHovered.value = true
}

const onMouseLeave = async () => {
  if (!isHovered.value) return
  isHovered.value = false
}

const getSafeSelectedText = () => selectedText.value.trim()
const hasSelectionAiConfig = (settings) => {
  const provider = String(settings?.ai_provider || '').trim()
  if (!provider) return false
  const providerConfig = settings?.provider_configs?.[provider]
  if (!providerConfig) return false
  const apiUrl = String(providerConfig.api_url || '').trim()
  const modelName = String(providerConfig.model_name || '').trim()
  const apiKey = String(providerConfig.api_key || '').trim()
  return apiUrl.length > 0 && modelName.length > 0 && apiKey.length > 0
}
const ensureSelectionAiConfigured = async () => {
  try {
    const settings = await AISettingsService.getSettings()
    if (hasSelectionAiConfig(settings)) {
      return true
    }
    await WindowService.selectionToolbarBlur()
    await WindowService.openSettingsWindow('ai', 'selection_ai_not_configured')
    return false
  } catch (error) {
    handleAppError(error, '读取AI设置失败')
    return false
  }
}

const runAction = async (executor, errorMessage) => {
  const text = getSafeSelectedText()
  if (!text || actionLoading.value) return
  actionLoading.value = true
  try {
    await executor(text)
  } catch (error) {
    handleAppError(error, errorMessage)
  } finally {
    actionLoading.value = false
  }
}

onMounted(async () => {
  try {
    if (window.__SELECTION_TOOLBAR_TEXT__) {
      selectedText.value = String(window.__SELECTION_TOOLBAR_TEXT__)
    }
    const onDomText = async (event) => {
      selectedText.value = typeof event?.detail === 'string' ? event.detail : ''
      isHovered.value = false
    }
    window.addEventListener('selection-toolbar-text', onDomText)
    unlistenDomText = () => window.removeEventListener('selection-toolbar-text', onDomText)
    unlistenSelectedText = await listen('selected-text', async (event) => {
      selectedText.value = typeof event.payload === 'string' ? event.payload : ''
      isHovered.value = false
    })
  } catch (error) {
    console.error('Listen error:', error)
  }
})

onBeforeUnmount(() => {
  if (unlistenSelectedText) {
    unlistenSelectedText()
    unlistenSelectedText = null
  }
  if (unlistenDomText) {
    unlistenDomText()
    unlistenDomText = null
  }
})

const handleTranslate = async () => {
  await runAction(async (text) => {
    const ready = await ensureSelectionAiConfigured()
    if (!ready) return
    await WindowService.selectionToolbarBlur()
    await AIService.streamTranslate(text, '自动识别', '简体中文')
  }, '翻译请求失败')
}

const handleExplain = async () => {
  await runAction(async (text) => {
    const ready = await ensureSelectionAiConfigured()
    if (!ready) return
    await WindowService.selectionToolbarBlur()
    await AIService.streamExplain(text, '中文')
  }, '解释请求失败')
}

const handleCopy = async () => {
  await runAction(async (text) => {
    await ClipboardService.copyText(text)
    await WindowService.selectionToolbarBlur()
  }, '复制失败')
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
.container {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}

.mini-icon {
  -webkit-app-region: drag;
  width: 32px;
  height: 32px;
  background: linear-gradient(145deg, rgba(22, 28, 38, 0.95), rgba(14, 18, 26, 0.95));
  border-radius: 50%;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  border: 1px solid rgba(255, 255, 255, 0.12);
  transition: all 0.2s ease;
}

.magic-icon {
  font-size: 18px;
  color: #eef3ff;
}

.toolbar {
  -webkit-app-region: drag;
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

.no-drag {
  -webkit-app-region: no-drag;
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
