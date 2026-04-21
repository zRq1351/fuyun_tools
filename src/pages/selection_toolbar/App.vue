<template>
  <div class="container">
    <div class="interactive-area" @mouseenter="onMouseEnter" @mouseleave="onMouseLeave">
      <transition name="pop">
        <div v-if="!isHovered" class="mini-icon" data-tauri-drag-region>
          <el-icon class="magic-icon"><magic-stick/></el-icon>
        </div>
        
        <div v-else class="toolbar">
          <div :class="{ disabled: actionLoading }" class="toolbar-button translate-btn" @click="handleTranslate">
            <el-icon class="btn-icon">
              <collection/>
            </el-icon>
            <span class="btn-text">翻译</span>
          </div>

          <div :class="{ disabled: actionLoading }" class="toolbar-button explain-btn" @click="handleExplain">
            <el-icon class="btn-icon">
              <chat-line-round/>
            </el-icon>
            <span class="btn-text">解释</span>
          </div>

          <div :class="{ disabled: actionLoading }" class="toolbar-button copy-btn" @click="handleCopy">
            <el-icon class="btn-icon">
              <document-copy/>
            </el-icon>
            <span class="btn-text">复制</span>
          </div>
        </div>
      </transition>
  </div>
</div>
</template>

<script setup>
import {onBeforeUnmount, onMounted, ref} from 'vue'
import {ChatLineRound, Collection, DocumentCopy, MagicStick} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWindow, LogicalSize, LogicalPosition} from '@tauri-apps/api/window'
import {AIService, AISettingsService, ClipboardService, WindowService} from '../../services/ipc'
import {handleAppError} from '../../utils/errorHandler'

const selectedText = ref('')
const actionLoading = ref(false)
const isHovered = ref(false)
let unlistenSelectedText = null
let unlistenDomText = null
let unlistenFocus = null
let hoverTimeout = null
let enterTimeout = null

const appWindow = getCurrentWindow()

let stateVersion = 0
let shrunkPhysicalX = null
let shrunkPhysicalY = null

const onMouseEnter = async () => {
  if (hoverTimeout) {
    clearTimeout(hoverTimeout)
    hoverTimeout = null
  }
  if (isHovered.value) return
  if (enterTimeout) return

  enterTimeout = setTimeout(async () => {
    enterTimeout = null
    if (isHovered.value) return
    const currentVersion = ++stateVersion

    try {
      const factor = await appWindow.scaleFactor()
      const physicalPos = await appWindow.outerPosition()
      if (stateVersion !== currentVersion) return

      // 保存当前的缩小状态位置，以便后续精准恢复，防止窗口漂移
      shrunkPhysicalX = physicalPos.x
      shrunkPhysicalY = physicalPos.y

      const logicalX = physicalPos.x / factor
      const logicalY = physicalPos.y / factor

      const expandedX = logicalX - (240 - 64) / 2
      const expandedY = logicalY - (100 - 64) / 2

      const newPhysicalX = Math.round(expandedX * factor)
      const newPhysicalY = Math.round(expandedY * factor)
      const newPhysicalWidth = Math.round(240 * factor)
      const newPhysicalHeight = Math.round(100 * factor)

      // 先放大窗口，避免动画被截断
      await WindowService.resizeSelectionToolbar(newPhysicalX, newPhysicalY, newPhysicalWidth, newPhysicalHeight)
      
      if (stateVersion !== currentVersion) return
      isHovered.value = true
    } catch (e) {
      console.error(e)
    }
  }, 80)
}

const shrinkWindow = async (version) => {
  try {
    if (version && stateVersion !== version) return
    
    // 触发收起动画
    isHovered.value = false

    // 等待动画完成再缩小窗口，避免动画闪烁或被截断
    setTimeout(async () => {
      if (version && stateVersion !== version) return
      
      try {
        const factor = await appWindow.scaleFactor()
        let newPhysicalX, newPhysicalY

        if (shrunkPhysicalX !== null && shrunkPhysicalY !== null) {
          newPhysicalX = shrunkPhysicalX
          newPhysicalY = shrunkPhysicalY
        } else {
          const physicalPos = await appWindow.outerPosition()
          const logicalX = physicalPos.x / factor
          const logicalY = physicalPos.y / factor
          const shrunkX = logicalX + (240 - 64) / 2
          const shrunkY = logicalY + (100 - 64) / 2
          newPhysicalX = Math.round(shrunkX * factor)
          newPhysicalY = Math.round(shrunkY * factor)
        }

        const newPhysicalWidth = Math.round(64 * factor)
        const newPhysicalHeight = Math.round(64 * factor)

        await WindowService.resizeSelectionToolbar(newPhysicalX, newPhysicalY, newPhysicalWidth, newPhysicalHeight)
      } catch (e) {
        console.error(e)
      }
    }, 350)
  } catch (e) {
    isHovered.value = false
    console.error(e)
  }
}

const onMouseLeave = () => {
  if (enterTimeout) {
    clearTimeout(enterTimeout)
    enterTimeout = null
  }
  
  if (hoverTimeout) {
    clearTimeout(hoverTimeout)
  }
  
  hoverTimeout = setTimeout(async () => {
    if (!isHovered.value) return // 已经收起，不重复执行
    const currentVersion = ++stateVersion
    await shrinkWindow(currentVersion)
  }, 350)
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
    if (hoverTimeout) {
      clearTimeout(hoverTimeout)
      hoverTimeout = null
    }
    if (enterTimeout) {
      clearTimeout(enterTimeout)
      enterTimeout = null
    }
    const currentVersion = ++stateVersion
    await shrinkWindow(currentVersion)
    
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
      if (isHovered.value) {
        const currentVersion = ++stateVersion
        await shrinkWindow(currentVersion)
      }
      if (hoverTimeout) {
        clearTimeout(hoverTimeout)
        hoverTimeout = null
      }
      if (enterTimeout) {
        clearTimeout(enterTimeout)
        enterTimeout = null
      }
    }
    window.addEventListener('selection-toolbar-text', onDomText)
    unlistenDomText = () => window.removeEventListener('selection-toolbar-text', onDomText)
    unlistenSelectedText = await listen('selected-text', async (event) => {
      selectedText.value = typeof event.payload === 'string' ? event.payload : ''
      if (isHovered.value) {
        const currentVersion = ++stateVersion
        await shrinkWindow(currentVersion)
      }
      if (hoverTimeout) {
        clearTimeout(hoverTimeout)
        hoverTimeout = null
      }
      if (enterTimeout) {
        clearTimeout(enterTimeout)
        enterTimeout = null
      }
    })

    // Fallback listener for fast mouse exits from the window
    window.addEventListener('mouseout', (e) => {
      // 检查是否真正离开了工具栏范围
      // e.relatedTarget 不存在说明移出了窗口
      // 如果存在，说明移动到了另一个元素，我们需要判断该元素是否在我们的 toolbar 内部
      if (!e.relatedTarget) {
        onMouseLeave()
      } else {
        // 检查新目标是否还在 document.body 内（或者我们能掌控的 DOM 树内）
        // 由于这是 Tauri 应用，relatedTarget 通常在内部
        // 但保险起见，如果移到了 html/body 边缘之外
        const isInside = document.body.contains(e.relatedTarget)
        if (!isInside) {
           onMouseLeave()
        }
      }
    })

    // 原生 mouseleave 兜底
    document.documentElement.addEventListener('mouseleave', () => {
      onMouseLeave()
    })

    // Reset state when the window loses focus
    unlistenFocus = await appWindow.onFocusChanged(async ({ payload: focused }) => {
      if (!focused && isHovered.value) {
        if (hoverTimeout) {
          clearTimeout(hoverTimeout)
          hoverTimeout = null
        }
        if (enterTimeout) {
          clearTimeout(enterTimeout)
          enterTimeout = null
        }
        const currentVersion = ++stateVersion
        await shrinkWindow(currentVersion)
      }
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
  if (unlistenFocus) {
    unlistenFocus()
    unlistenFocus = null
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
html, body, #app {
  margin: 0 !important;
  padding: 0 !important;
  width: 100vw !important;
  height: 100vh !important;
  background: transparent !important;
  background-color: transparent !important;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  overflow: hidden !important;
  pointer-events: none;
}
</style>

<style scoped>
.container {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
}

.interactive-area {
  pointer-events: auto;
  display: grid;
  place-items: center;
}

.mini-icon, .toolbar {
  grid-area: 1 / 1;
}

.mini-icon {
  -webkit-app-region: drag;
  pointer-events: auto;
  width: 32px;
  height: 32px;
  background: linear-gradient(145deg, rgba(28, 35, 48, 0.98), rgba(18, 22, 32, 0.98));
  border-radius: 50%;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3), 0 0 0 1px rgba(255, 255, 255, 0.08) inset;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  border: 1px solid rgba(255, 255, 255, 0.15);
  transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1), box-shadow 0.2s ease, background 0.2s ease;
}

.mini-icon:hover {
  transform: scale(1.1);
  box-shadow: 0 6px 16px rgba(0, 0, 0, 0.4), 0 0 0 1px rgba(255, 255, 255, 0.15) inset;
}

.magic-icon {
  font-size: 18px;
  color: #eef3ff;
  transition: transform 0.2s ease;
}

.mini-icon:hover .magic-icon {
  transform: rotate(15deg);
}

.toolbar {
  pointer-events: auto;
  background: linear-gradient(145deg, rgba(28, 35, 48, 0.98), rgba(18, 22, 32, 0.98));
  backdrop-filter: blur(12px);
  border-radius: 12px;
  padding: 6px;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4), 0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 1px rgba(255, 255, 255, 0.05) inset;
  border: 1px solid rgba(255, 255, 255, 0.15);
  display: flex;
  flex-direction: row;
  gap: 2px;
  width: auto;
  box-sizing: border-box;
  transform-origin: center center;
}

.pop-enter-active {
  transition: opacity 0.3s ease, transform 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.pop-leave-active {
  transition: opacity 0.2s ease, transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.pop-enter-from,
.pop-leave-to {
  opacity: 0;
  transform: scale(0.6) translateY(4px);
}

.toolbar-button {
  background: transparent;
  border: none;
  color: #eef3ff;
  width: 56px;
  height: 42px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 18px;
  transition: all 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
  margin: 0 2px;
}

.toolbar-button:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.toolbar-button:active {
  transform: translateY(0) scale(0.95);
}

.toolbar-button.disabled {
  opacity: 0.55;
  pointer-events: none;
}

.btn-icon {
  opacity: 1;
  transform: translateY(0);
  transition: all 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
}

.btn-text {
  position: absolute;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.5px;
  opacity: 0;
  transform: translateY(12px);
  transition: all 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
}

.toolbar-button:hover .btn-icon {
  opacity: 0;
  transform: translateY(-12px);
}

.toolbar-button:hover .btn-text {
  opacity: 1;
  transform: translateY(0);
}

.translate-btn {
  color: #7be682;
  background: linear-gradient(145deg, rgba(82, 165, 112, 0.22), rgba(44, 96, 65, 0.2));
}
.translate-btn:hover {
  background: linear-gradient(145deg, rgba(82, 165, 112, 0.35), rgba(44, 96, 65, 0.3));
}

.explain-btn {
  color: #72b7ff;
  background: linear-gradient(145deg, rgba(84, 148, 230, 0.22), rgba(44, 83, 150, 0.2));
}
.explain-btn:hover {
  background: linear-gradient(145deg, rgba(84, 148, 230, 0.35), rgba(44, 83, 150, 0.3));
}

.copy-btn {
  color: #f2c06d;
  background: linear-gradient(145deg, rgba(209, 152, 61, 0.22), rgba(133, 89, 35, 0.2));
}
.copy-btn:hover {
  background: linear-gradient(145deg, rgba(209, 152, 61, 0.35), rgba(133, 89, 35, 0.3));
}

</style>
