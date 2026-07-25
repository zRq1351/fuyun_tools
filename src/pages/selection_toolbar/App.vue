<template>
  <div class="container">
    <div class="interactive-area" @mouseleave="onMouseLeave">
      <div :class="{ 'active': !isHovered }" class="mini-icon"
           data-tauri-drag-region
           @mouseenter="onMouseEnter">
        <el-icon class="magic-icon">
          <magic-stick/>
        </el-icon>
      </div>

      <div :class="{ 'active': isHovered }" class="toolbar">
        <div :class="{ disabled: actionLoading }" class="toolbar-button translate-btn" @click="handleTranslate">
          <el-icon class="btn-icon">
            <collection/>
          </el-icon>
          <span class="btn-text">{{ t('selectionToolbar.translate') }}</span>
        </div>

        <div :class="{ disabled: actionLoading }" class="toolbar-button explain-btn" @click="handleExplain">
          <el-icon class="btn-icon">
            <chat-line-round/>
          </el-icon>
          <span class="btn-text">{{ t('selectionToolbar.explain') }}</span>
        </div>

        <div :class="{ disabled: actionLoading }" class="toolbar-button copy-btn" @click="handleCopy">
          <el-icon class="btn-icon">
            <document-copy/>
          </el-icon>
          <span class="btn-text">{{ t('selectionToolbar.copy') }}</span>
        </div>

        <div :class="{ disabled: actionLoading }" class="toolbar-button search-btn" @click="handleWebSearch">
          <el-icon class="btn-icon">
            <search/>
          </el-icon>
          <span class="btn-text">{{ t('selectionToolbar.search') }}</span>
        </div>

        <div v-for="prompt in enabledCustomPrompts" :key="prompt.name"
             :class="{ disabled: actionLoading }"
             :style="{ color: prompt.color || 'var(--fy-text-muted)', background: parseBackground(prompt.bg_color) }"
             class="toolbar-button custom-btn"
             @click="handleCustomPrompt(prompt.name)">
          <el-icon class="btn-icon">
            <component :is="getIconComponent(prompt.icon || 'Star')"/>
          </el-icon>
          <span class="btn-text">{{ prompt.name }}</span>
        </div>
      </div>
  </div>
</div>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import {ChatLineRound, Collection, DocumentCopy, MagicStick, Search} from '@element-plus/icons-vue'
import {listen} from '@tauri-apps/api/event'
import {currentMonitor, getCurrentWindow} from '@tauri-apps/api/window'
import {openUrl} from '@tauri-apps/plugin-opener'
import {AIService, AISettingsService, ClipboardService, WindowService} from '../../services/ipc'
import {handleAppError} from '@/utils/errorHandler.js'

const {t} = useI18n()

const selectedText = ref('')
const actionLoading = ref(false)
const isHovered = ref(false)
const webSearchEngine = ref('bing')
const customPrompts = ref([])
let unlistenSelectedText = null
let unlistenDomText = null
let unlistenFocus = null
let unlistenMouseOut = null
let unlistenMouseLeave = null
let hoverTimeout = null
let enterTimeout = null
let isAnimating = false // 添加动画锁，防止重复触发
let cachedSettings = null // 缓存AI设置，避免每次操作都请求IPC
let settingsCacheTime = 0 // 设置缓存时间戳
const SETTINGS_CACHE_TTL = 5000 // 缓存有效期5秒

const appWindow = getCurrentWindow()

// 动态获取图标组件（仅支持 Element Plus）
const getIconComponent = (iconName) => {
  if (!iconName) return ElementPlusIconsVue.Star

  const icon = ElementPlusIconsVue[iconName]
  if (icon) {
    return icon
  }

  // 默认返回 Star
  return ElementPlusIconsVue.Star
}

// 过滤出启用的自定义按钮
const enabledCustomPrompts = computed(() => {
  return customPrompts.value.filter(prompt => prompt.enabled !== false)
})

// 获取缓存的AI设置（避免每次操作都发起IPC请求）
const getCachedSettings = async (forceRefresh = false) => {
  const now = Date.now()
  if (!forceRefresh && cachedSettings && (now - settingsCacheTime) < SETTINGS_CACHE_TTL) {
    return cachedSettings
  }
  try {
    const settings = await AISettingsService.getSettings()
    cachedSettings = settings
    settingsCacheTime = now
    return settings
  } catch {
    return cachedSettings // 返回旧缓存作为降级
  }
}

// 使设置缓存失效（在操作完成后调用，延迟刷新）
const invalidateSettingsCache = () => {
  settingsCacheTime = 0
}

// 解析背景颜色，使用纯色半透明
const parseBackground = (bgColor) => {
  if (!bgColor) return 'rgba(255, 255, 255, 0.06)'
  return bgColor
}


let stateVersion = 0
let shrunkPhysicalX = null
let shrunkPhysicalY = null

const onMouseEnter = async () => {
  // 如果正在动画中或已经展开，忽略重复触发
  if (isAnimating || isHovered.value) return

  if (hoverTimeout) {
    clearTimeout(hoverTimeout)
    hoverTimeout = null
  }
  if (enterTimeout) return

  enterTimeout = setTimeout(async () => {
    enterTimeout = null
    // 再次检查，防止在延迟期间状态已改变
    if (isAnimating || isHovered.value) return

    const currentVersion = ++stateVersion
    isAnimating = true // 设置动画锁

    try {
      // 先隐藏魔法棒
      const miniIcon = document.querySelector('.mini-icon')
      if (miniIcon) {
        miniIcon.style.opacity = '0'
        miniIcon.style.pointerEvents = 'none'
      }

      // 等待一帧确保魔法棒隐藏生效
    await new Promise(resolve => requestAnimationFrame(resolve))

    // 使用缓存设置获取按钮列表（避免每次展开都发起IPC请求）
    const settings = await getCachedSettings().catch(() => null)
    if (settings) {
      webSearchEngine.value = settings.selection_web_search_engine || 'bing'
      customPrompts.value = Array.isArray(settings.selection_custom_prompts) ? settings.selection_custom_prompts : []
    }

    const factor = await appWindow.scaleFactor()
      const physicalPos = await appWindow.outerPosition()
      if (stateVersion !== currentVersion) return

      // 保存当前的缩小状态位置，以便后续精准恢复，防止窗口漂移
      shrunkPhysicalX = physicalPos.x
      shrunkPhysicalY = physicalPos.y

      const logicalX = physicalPos.x / factor
      const logicalY = physicalPos.y / factor

      // 搜索按钮始终显示，基础按钮数 = 3(翻译、解释、复制) + 1(搜索)
      let buttonCount = 4
      buttonCount += enabledCustomPrompts.value.length

      const targetWidth = buttonCount * 60 + 12
      const targetHeight = 100

      const expandedX = logicalX - (targetWidth - 64) / 2
      const expandedY = logicalY - (targetHeight - 64) / 2

      let newPhysicalX = Math.round(expandedX * factor)
      let newPhysicalY = Math.round(expandedY * factor)
      const newPhysicalWidth = Math.round(targetWidth * factor)
      const newPhysicalHeight = Math.round(targetHeight * factor)

      // 获取当前显示器边界进行裁剪，防止放大后超出屏幕
      const monitor = await currentMonitor()
      if (monitor) {
        const minPhysicalX = monitor.position.x
        const minPhysicalY = monitor.position.y
        const maxPhysicalX = monitor.position.x + monitor.size.width - newPhysicalWidth
        const maxPhysicalY = monitor.position.y + monitor.size.height - newPhysicalHeight

        newPhysicalX = Math.max(minPhysicalX, Math.min(newPhysicalX, maxPhysicalX))
        newPhysicalY = Math.max(minPhysicalY, Math.min(newPhysicalY, maxPhysicalY))
      }

      // 放大窗口，此时工具栏还未显示（opacity: 0）
      await WindowService.resizeSelectionToolbar(newPhysicalX, newPhysicalY, newPhysicalWidth, newPhysicalHeight)

      // 等待窗口调整完成后，再显示工具栏
      await new Promise(resolve => setTimeout(resolve, 50))
      if (stateVersion !== currentVersion) return

      // 恢复魔法棒的CSS状态并显示工具栏
      if (miniIcon) {
        miniIcon.style.removeProperty('opacity')
        miniIcon.style.removeProperty('pointer-events')
      }
      isHovered.value = true
    } catch (e) {
      console.error(e)
      // Restore miniIcon opacity if it was hidden before the error
      if (miniIcon) {
        miniIcon.style.removeProperty('opacity')
        miniIcon.style.removeProperty('pointer-events')
      }
    } finally {
      isAnimating = false // 释放动画锁
    }
  }, 80)
}

const shrinkWindow = async (version) => {
  try {
    if (version && stateVersion !== version) return

    // 直接切换状态，CSS会立即隐藏工具栏、显示魔法棒
    isHovered.value = false

    // 等待一帧确保CSS生效
    await new Promise(resolve => requestAnimationFrame(resolve))

    // 缩小窗口到魔法棒尺寸
    const factor = await appWindow.scaleFactor()
    let newPhysicalX, newPhysicalY

    if (shrunkPhysicalX !== null && shrunkPhysicalY !== null) {
      newPhysicalX = shrunkPhysicalX
      newPhysicalY = shrunkPhysicalY
    } else {
      const physicalPos = await appWindow.outerPosition()
      const logicalX = physicalPos.x / factor
      const logicalY = physicalPos.y / factor
      // 搜索按钮始终显示，基础按钮数 = 3(翻译、解释、复制) + 1(搜索)
      let buttonCount = 4
      buttonCount += enabledCustomPrompts.value.length
      const currentWidth = buttonCount * 60 + 12
      const currentHeight = 100

      const shrunkX = logicalX + (currentWidth - 64) / 2
      const shrunkY = logicalY + (currentHeight - 64) / 2
      newPhysicalX = Math.round(shrunkX * factor)
      newPhysicalY = Math.round(shrunkY * factor)
    }

    const newPhysicalWidth = Math.round(64 * factor)
    const newPhysicalHeight = Math.round(64 * factor)

    await WindowService.resizeSelectionToolbar(newPhysicalX, newPhysicalY, newPhysicalWidth, newPhysicalHeight)
  } catch (e) {
    isHovered.value = false
    console.error(e)
  } finally {
    isAnimating = false // 释放动画锁
  }
}

const onMouseLeave = () => {
  // 如果正在动画中或已经收起，忽略重复触发
  if (isAnimating || !isHovered.value) return

  if (enterTimeout) {
    clearTimeout(enterTimeout)
    enterTimeout = null
  }

  if (hoverTimeout) {
    clearTimeout(hoverTimeout)
  }

  const currentVersion = ++stateVersion
  isAnimating = true // 设置动画锁
  shrinkWindow(currentVersion)
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
    const settings = await getCachedSettings(true)
    if (hasSelectionAiConfig(settings)) {
      return true
    }
    await WindowService.selectionToolbarBlur()
    await WindowService.openSettingsWindow('ai', 'selection_ai_not_configured')
    return false
  } catch (error) {
    handleAppError(error, t('selectionToolbar.readAISettingsFailed'))
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
    invalidateSettingsCache() // 操作完成后使缓存失效，下次展开时刷新
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
    const onMouseOut = (e) => {
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
    }
    window.addEventListener('mouseout', onMouseOut)
    unlistenMouseOut = () => window.removeEventListener('mouseout', onMouseOut)

    // 原生 mouseleave 兜底
    const onMouseLeaveDoc = () => {
      onMouseLeave()
    }
    document.documentElement.addEventListener('mouseleave', onMouseLeaveDoc)
    unlistenMouseLeave = () => document.documentElement.removeEventListener('mouseleave', onMouseLeaveDoc)

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
  if (unlistenMouseOut) {
    unlistenMouseOut()
    unlistenMouseOut = null
  }
  if (unlistenMouseLeave) {
    unlistenMouseLeave()
    unlistenMouseLeave = null
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

const handleTranslate = async () => {
  await runAction(async (text) => {
    const ready = await ensureSelectionAiConfigured()
    if (!ready) return
    await WindowService.selectionToolbarBlur()
    await AIService.streamTranslate(text, t('selectionToolbar.autoDetect'), t('selectionToolbar.simplifiedChinese'))
  }, t('selectionToolbar.translateFailed'))
}

const handleExplain = async () => {
  await runAction(async (text) => {
    const ready = await ensureSelectionAiConfigured()
    if (!ready) return
    await WindowService.selectionToolbarBlur()
    await AIService.streamExplain(text, t('selectionToolbar.chinese'))
  }, t('selectionToolbar.explainFailed'))
}

const handleCopy = async () => {
  await runAction(async (text) => {
    await ClipboardService.copyText(text)
    await WindowService.selectionToolbarBlur()
  }, t('selectionToolbar.copyFailed'))
}

const handleWebSearch = async () => {
  await runAction(async (text) => {
    // 点击搜索时使用缓存设置（刷新缓存以获取最新搜索引擎）
    const settings = await getCachedSettings(true).catch(() => null)
    if (settings) {
      webSearchEngine.value = settings.selection_web_search_engine || 'bing'
    }

    const query = encodeURIComponent(text)
    let url = ''
    switch (webSearchEngine.value) {
      case 'google':
        url = `https://www.google.com/search?q=${query}`
        break
      case 'baidu':
        url = `https://www.baidu.com/s?wd=${query}`
        break
      case 'duckduckgo':
        url = `https://duckduckgo.com/?q=${query}`
        break
      case 'bing':
      default:
        url = `https://www.bing.com/search?q=${query}`
        break
    }
    await openUrl(url)
    await WindowService.selectionToolbarBlur()
  }, t('selectionToolbar.searchFailed'))
}

const handleCustomPrompt = async (promptName) => {
  await runAction(async (text) => {
    const ready = await ensureSelectionAiConfigured()
    if (!ready) return
    await WindowService.selectionToolbarBlur()
    await AIService.streamCustomPrompt(text, promptName)
  }, t('selectionToolbar.customPromptFailed'))
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
  opacity: 0;
  pointer-events: none;
  position: absolute;
  /* 使用瞬时切换，避免过渡期间被看到 */
  transition: none;
}

.mini-icon.active {
  opacity: 1;
  pointer-events: auto;
  position: static;
  -webkit-app-region: drag;
  width: 32px;
  height: 32px;
  background: rgba(30, 34, 48, 0.72);
  border-radius: 50%;
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.2);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  border: 0.5px solid rgba(255, 255, 255, 0.08);
  transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1), box-shadow 0.2s ease, background 0.2s ease;
}

.mini-icon:hover {
  transform: scale(1.1);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
}

.magic-icon {
  font-size: 18px;
  color: var(--fy-text-primary);
  transition: transform 0.2s ease;
}

.mini-icon:hover .magic-icon {
  transform: rotate(15deg);
}

.toolbar {
  opacity: 0;
  pointer-events: none;
  /* 使用瞬时切换，避免过渡期间被看到 */
  transition: none;
}

.toolbar.active {
  opacity: 1;
  pointer-events: auto;
  background: rgba(30, 34, 48, 0.72);
  backdrop-filter: blur(40px) saturate(180%);
  border-radius: 14px;
  padding: 6px;
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.2);
  border: 0.5px solid rgba(255, 255, 255, 0.08);
  display: flex;
  flex-direction: row;
  gap: 2px;
  width: auto;
  box-sizing: border-box;
  transform-origin: center center;
  transition: none;
}

.pop-enter-active {
  transition: opacity 0.3s ease, transform 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
}

/* 收起时禁用动画，避免工具栏被裁剪 */
.pop-leave-active {
  transition: none;
}

.pop-enter-from,
.pop-leave-to {
  opacity: 0;
  transform: scale(0.6) translateY(4px);
}

.toolbar-button {
  background: transparent;
  border: none;
  color: var(--fy-text-primary);
  width: 56px;
  height: 42px;
  border-radius: 10px;
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
  box-shadow: var(--fy-shadow);
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
  font-size: 11px;
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
  color: var(--fy-success);
  background: rgba(74, 222, 128, 0.12);
}
.translate-btn:hover {
  background: rgba(74, 222, 128, 0.22);
}

.explain-btn {
  color: var(--fy-accent);
  background: rgba(108, 140, 255, 0.12);
}
.explain-btn:hover {
  background: rgba(108, 140, 255, 0.22);
}

.copy-btn {
  color: var(--fy-warning);
  background: rgba(251, 191, 36, 0.12);
}
.copy-btn:hover {
  background: rgba(251, 191, 36, 0.22);
}

.search-btn {
  color: var(--fy-info);
  background: rgba(148, 163, 184, 0.12);
}
.search-btn:hover {
  background: rgba(148, 163, 184, 0.22);
}

.custom-btn {
  /* 背景颜色通过 :style 动态设置 */
}
.custom-btn:hover {
  filter: brightness(1.2);
}

/* 亮色主题按钮样式适配 */
html.light .translate-btn,
[data-theme="light"] .translate-btn {
  background: rgba(34, 197, 94, 0.08);
}
html.light .translate-btn:hover,
[data-theme="light"] .translate-btn:hover {
  background: rgba(34, 197, 94, 0.15);
}

html.light .explain-btn,
[data-theme="light"] .explain-btn {
  background: rgba(79, 107, 246, 0.08);
}
html.light .explain-btn:hover,
[data-theme="light"] .explain-btn:hover {
  background: rgba(79, 107, 246, 0.15);
}

html.light .copy-btn,
[data-theme="light"] .copy-btn {
  background: rgba(245, 158, 11, 0.08);
}
html.light .copy-btn:hover,
[data-theme="light"] .copy-btn:hover {
  background: rgba(245, 158, 11, 0.15);
}

html.light .search-btn,
[data-theme="light"] .search-btn {
  background: rgba(148, 163, 184, 0.08);
}
html.light .search-btn:hover,
[data-theme="light"] .search-btn:hover {
  background: rgba(148, 163, 184, 0.15);
}

/* 护眼主题按钮样式适配 */
html.eye-care .translate-btn,
[data-theme="eye-care"] .translate-btn {
  background: rgba(106, 154, 74, 0.1);
}
html.eye-care .translate-btn:hover,
[data-theme="eye-care"] .translate-btn:hover {
  background: rgba(106, 154, 74, 0.18);
}

html.eye-care .explain-btn,
[data-theme="eye-care"] .explain-btn {
  background: rgba(139, 115, 85, 0.1);
}
html.eye-care .explain-btn:hover,
[data-theme="eye-care"] .explain-btn:hover {
  background: rgba(139, 115, 85, 0.18);
}

html.eye-care .copy-btn,
[data-theme="eye-care"] .copy-btn {
  background: rgba(196, 154, 58, 0.1);
}
html.eye-care .copy-btn:hover,
[data-theme="eye-care"] .copy-btn:hover {
  background: rgba(196, 154, 58, 0.18);
}

html.eye-care .search-btn,
[data-theme="eye-care"] .search-btn {
  background: rgba(139, 115, 85, 0.1);
}
html.eye-care .search-btn:hover,
[data-theme="eye-care"] .search-btn:hover {
  background: rgba(139, 115, 85, 0.18);
}

</style>
