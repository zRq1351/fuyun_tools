//! useManualLongshot — 手动长截图 composable。
//! 职责：长截图会话状态与可用性、启动/暂停/继续/完成/取消、
//! 长截图结果应用，以及长截图视图变换（缩放/平移/场景层样式）。
//! 跨域依赖（选区矩形、截图源加载、截图窗口显隐）通过 deps 注入。

import {computed, reactive, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {parseErrorMessage} from '../../../utils/errorHandler.js'
import {IPC_COMMANDS, ScreenshotService} from '../../../services/ipc.js'

export function useManualLongshot(deps) {
    const {
        t,
        regionSelectMode,
        sourceImagePath,
        longshotResultActive,
        longshotRawPngBase64,
        longshotViewScale,
        longshotViewOffset,
        manualLongshotSessionId,
        manualLongshotRunning,
        state,
        rect,
        scheduleManualLongshotAvailabilityRefresh,
        getGlobalSelectionRect,
        loadImageFromBase64,
        loadImageFromPath
    } = deps

    const manualLongshotPhase = ref('idle')
    const manualLongshotHint = ref('')

    const manualLongshotAvailability = reactive({
      status: 'unknown',
      phase: 'idle',
      summary: '',
      details: [],
      sessionId: null
    })
    const longshotOverlayOnly = ref(false)

    const pendingLongshotBorderAnchor = ref(null)
    const longshotBorderShown = ref(false)

    const manualLongshotButtonTitle = computed(() => {
      if (manualLongshotAvailability.status === 'available') return t('screenshot.longshot')
      if (manualLongshotAvailability.status === 'busy') return t('screenshot.longshotRunning')
      return manualLongshotAvailability.summary || t('screenshot.longshotUnavailable')
    })

    async function refreshManualLongshotAvailability() {
      try {
        const payload = await ScreenshotService.getManualLongshotAvailability()
        manualLongshotAvailability.status = payload?.status || 'unknown'
        manualLongshotAvailability.phase = payload?.phase || 'idle'
        manualLongshotAvailability.summary = payload?.summary || ''
        manualLongshotAvailability.details = Array.isArray(payload?.details) ? payload.details : []
        manualLongshotAvailability.sessionId = payload?.sessionId ?? null
        manualLongshotPhase.value = manualLongshotAvailability.phase
      } catch (error) {
        manualLongshotAvailability.status = 'unknown'
        manualLongshotAvailability.phase = 'idle'
        manualLongshotAvailability.summary = String(error || t('screenshot.longshotStatusFailed'))
        manualLongshotAvailability.details = []
        manualLongshotAvailability.sessionId = null
        manualLongshotPhase.value = 'idle'
      }
    }

    function cleanupManualLongshotUi(restoreVisibility = true) {
      manualLongshotRunning.value = false
      manualLongshotSessionId.value = 0
      longshotOverlayOnly.value = false
      pendingLongshotBorderAnchor.value = null
      longshotBorderShown.value = false
      regionSelectMode.value = 'screenshot'
      if (restoreVisibility) {
        invoke(IPC_COMMANDS.SET_SCREENSHOT_WINDOW_VISIBLE, {visible: true}).catch(() => {
        })
      }
      invoke(IPC_COMMANDS.HIDE_LONGSHOT_BORDER).catch(() => {
      })
      invoke(IPC_COMMANDS.HIDE_LONGSHOT_TOOLBAR).catch(() => {
      })
      scheduleManualLongshotAvailabilityRefresh()
    }

    function applyManualLongshotPhase(payload = {}) {
      const phase = String(payload.phase || '')
      const userMessage = String(payload.userMessage || '')
      if (phase) {
        manualLongshotPhase.value = phase
      }
      if (phase === 'paused') {
        manualLongshotRunning.value = false
      } else if (phase === 'running' || phase === 'starting' || phase === 'finishing') {
        manualLongshotRunning.value = true
      } else if (phase === 'canceling' || phase === 'done' || phase === 'failed') {
        manualLongshotRunning.value = false
      }
      if (userMessage) {
        manualLongshotHint.value = userMessage
      }
    }

    const sceneLayerStyle = computed(() => {
      if (!longshotResultActive.value) {
        return {}
      }
      return {
        transformOrigin: '0 0',
        transform: `translate(${longshotViewOffset.x}px, ${longshotViewOffset.y}px) scale(${longshotViewScale.value})`
      }
    })

    function onMouseWheel(event) {
      if (!longshotResultActive.value) return
      const delta = event.deltaY < 0 ? 1.12 : 0.89
      const oldScale = longshotViewScale.value
      const nextScale = Math.max(0.35, Math.min(4, oldScale * delta))
      if (Math.abs(nextScale - oldScale) < 0.0001) return
      const anchorX = event.clientX
      const anchorY = event.clientY
      const sceneX = (anchorX - longshotViewOffset.x) / oldScale
      const sceneY = (anchorY - longshotViewOffset.y) / oldScale
      longshotViewScale.value = nextScale
      longshotViewOffset.x = anchorX - sceneX * nextScale
      longshotViewOffset.y = anchorY - sceneY * nextScale
    }

    async function toggleManualLongshotRunning() {
      try {
        if (manualLongshotSessionId.value <= 0) {
          const region = getGlobalSelectionRect()
          longshotOverlayOnly.value = false
          pendingLongshotBorderAnchor.value = region
          longshotBorderShown.value = false
          await invoke(IPC_COMMANDS.SET_SCREENSHOT_WINDOW_VISIBLE, {visible: false})

          await new Promise(resolve => setTimeout(resolve, 80))
          const result = await ScreenshotService.startManualLongshot({
            region,
            fps: 10,
            minConfidence: 0.65,
            maxDurationSec: 120,
            previewIntervalMs: 300
          })
          const sid = Number(result?.sessionId || 0)
          if (sid > 0) {
            manualLongshotSessionId.value = sid
            manualLongshotRunning.value = true
            manualLongshotPhase.value = 'starting'
            await invoke(IPC_COMMANDS.SHOW_LONGSHOT_TOOLBAR, {anchor: region})
            manualLongshotHint.value = t('screenshot.longshotStarted')
          } else {
            await invoke(IPC_COMMANDS.SET_SCREENSHOT_WINDOW_VISIBLE, {visible: true}).catch(() => {
            })
            pendingLongshotBorderAnchor.value = null
            longshotBorderShown.value = false
          }
          return
        }
        if (manualLongshotRunning.value) {
          await ScreenshotService.pauseManualLongshot(manualLongshotSessionId.value)
          manualLongshotRunning.value = false
          manualLongshotPhase.value = 'paused'
          manualLongshotHint.value = t('screenshot.longshotPaused')
        } else {
          await ScreenshotService.resumeManualLongshot(manualLongshotSessionId.value)
          manualLongshotRunning.value = true
          manualLongshotPhase.value = 'running'
          manualLongshotHint.value = t('screenshot.longshotResumed')
        }
      } catch (error) {
        await invoke(IPC_COMMANDS.SET_SCREENSHOT_WINDOW_VISIBLE, {visible: true}).catch(() => {})
        cleanupManualLongshotUi(false)
        manualLongshotPhase.value = 'failed'
        manualLongshotHint.value = t('screenshot.longshotOpFailed', {error: parseErrorMessage(String(error)) || String(error)})
      }
    }

    async function finishManualLongshotCapture() {
      if (manualLongshotSessionId.value <= 0) return
      try {
        const sessionId = manualLongshotSessionId.value
        await invoke(IPC_COMMANDS.SET_SCREENSHOT_WINDOW_VISIBLE, {visible: true})
        cleanupManualLongshotUi(false)
        const result = await ScreenshotService.finishManualLongshot(sessionId)
        applyManualLongshotResult(result || {})
      } catch (error) {
        cleanupManualLongshotUi(true)
        manualLongshotHint.value = t('screenshot.longshotFinishFailed', {error: parseErrorMessage(String(error)) || String(error)})
      }
    }

    function cancelManualLongshotCapture(updateHint = true, restoreVisibility = true) {
      const sid = manualLongshotSessionId.value
      const hadLongshotRuntime =
          sid > 0 ||
          longshotOverlayOnly.value ||
          !!pendingLongshotBorderAnchor.value ||
          longshotBorderShown.value
      manualLongshotRunning.value = false
      manualLongshotPhase.value = 'canceling'
      manualLongshotSessionId.value = 0
      cleanupManualLongshotUi(restoreVisibility && hadLongshotRuntime)
      if (sid > 0) {
        ScreenshotService.cancelManualLongshot(sid).catch(() => {
        })
      }
      if (updateHint) {
        manualLongshotHint.value = t('screenshot.longshotCancelled')
      }
    }

    function applyManualLongshotResult(result) {
      const base64 = String(result?.pngBase64 || result?.png_base64 || '')
      const imagePath = String(result?.imagePath || result?.image_path || '')
      if (!base64 && !imagePath) {
        throw new Error(t('screenshot.longshotNoResult'))
      }
      if (base64) {
        loadImageFromBase64(base64)
      } else if (imagePath) {
        loadImageFromPath(imagePath)
      }
      longshotResultActive.value = true
      longshotRawPngBase64.value = base64
      sourceImagePath.value = imagePath
      longshotViewScale.value = 1
      longshotViewOffset.x = 0
      longshotViewOffset.y = 0
      regionSelectMode.value = 'screenshot'
      state.value = 'selected'
      rect.x = 0
      rect.y = 0
      rect.width = window.innerWidth
      rect.height = window.innerHeight
      manualLongshotSessionId.value = 0
      manualLongshotRunning.value = false
      manualLongshotPhase.value = 'done'
      cleanupManualLongshotUi(true)
      manualLongshotHint.value = ''
    }

    return {
        manualLongshotAvailability,
        manualLongshotPhase,
        manualLongshotHint,
        longshotOverlayOnly,
        pendingLongshotBorderAnchor,
        longshotBorderShown,
        refreshManualLongshotAvailability,
        cleanupManualLongshotUi,
        applyManualLongshotPhase,
        toggleManualLongshotRunning,
        finishManualLongshotCapture,
        cancelManualLongshotCapture,
        applyManualLongshotResult,
        sceneLayerStyle,
        onMouseWheel,
        manualLongshotButtonTitle
    }
}
