//! useScreenshotCapture — 截图采集与导出 composable。
//! 职责：截图源（screenshotSrc/screenshotImg/canvas）与就绪状态、会话加载、
//! 渲染导出 PNG（长截图全图/裁剪画布）、保存到文件、复制到剪贴板、贴图，
//! 以及导出链路指示等 computeds。
//! 跨域依赖（选区矩形、标注条目、后端导出请求、像素源探测）通过 deps 注入。

import {computed, nextTick, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {emit} from '@tauri-apps/api/event'
import {buildFileUrlFromPath} from '../../../utils/fileUrl'
import {hexToRgba} from '../../../utils/screenshotAnnotation'
import {IPC_COMMANDS} from '../../../services/ipc.js'

export function useScreenshotCapture(deps) {
    const {
        t,
        dpr,
        isDevMode,
        sourceImagePath,
        longshotResultActive,
        longshotRawPngBase64,
        longshotViewScale,
        longshotViewOffset,
        overlayDirty,
        regionSelectMode,
        manualLongshotSessionId,
        manualLongshotRunning,
        captureOriginX,
        captureOriginY,
        activeSessionId,
        payloadSessionId,
        screenshotSessionRequested,
        hasScreenshotPayload,
        fetchWindows,
        state,
        rect,
        currentTool,
        textItems,
        shapeItems,
        hasSelection,
        SHAPE_TOOLS,
        getCroppedCanvas,
        buildBackendExportRequest,
        requestScreenshot,
        showCaptureError,
        saveToHistory,
        isPixelCtxReadable,
        loadScreenshotPixelFromBlob,
        loadImageFromSrc,
        close
    } = deps

    const screenshotSrc = ref('')
    const screenshotImg = ref(null)
    const canvas = ref(null)

    const isCaptureReady = ref(false)
    const captureReadyError = ref('')

    const canExport = computed(() => hasSelection.value)
    const canCropToSelection = computed(() =>
        regionSelectMode.value === 'screenshot' &&
        hasSelection.value &&
        state.value === 'selected' &&
        !longshotResultActive.value &&
        (currentTool.value === 'select' || SHAPE_TOOLS.includes(currentTool.value) || currentTool.value === 'pen' || currentTool.value === 'highlight' || currentTool.value === 'text' || currentTool.value === 'mosaic')
    )

    async function resolveCropDataUrl() {
      // asset:// 等会污染 canvas；有后端源路径时走 render，避免 toDataURL SecurityError
      if (sourceImagePath.value) {
        const renderResult = await invoke(IPC_COMMANDS.RENDER_SCREENSHOT_TO_PNG_DATA, {
          request: buildBackendExportRequest('')
        })
        if (!renderResult?.success || !renderResult?.pngBase64) {
          throw new Error(renderResult?.error || renderResult?.message || t('screenshot.cropUnavailable'))
        }
        return {
          dataUrl: `data:image/png;base64,${renderResult.pngBase64}`,
          cssW: Math.max(1, Number(renderResult.width || 0) / dpr),
          cssH: Math.max(1, Number(renderResult.height || 0) / dpr)
        }
      }
      if (!isPixelCtxReadable()) {
        try {
          await loadScreenshotPixelFromBlob(screenshotSrc.value, {reset: false})
        } catch (_) { /* fall through to canvas path */ }
      }
      const cropCanvas = getCroppedCanvas()
      return {
        dataUrl: cropCanvas.toDataURL('image/png'),
        cssW: Math.max(1, cropCanvas.width / dpr),
        cssH: Math.max(1, cropCanvas.height / dpr)
      }
    }

    const showExportRouteIndicator = computed(() =>
        isDevMode &&
        regionSelectMode.value === 'screenshot' &&
        hasSelection.value &&
        (state.value === 'selected' || state.value === 'drawing')
    )
    const exportRouteIndicator = computed(() => {
      const sourcePath = String(sourceImagePath.value || '').trim()
      const hasBackendPath = !!sourcePath
      const displaySource = hasBackendPath
          ? `imagePath -> objectURL 显示 (${getPathBasename(sourcePath)})`
          : (screenshotSrc.value?.startsWith('data:image/')
              ? 'fallback -> data:image/base64 显示'
              : (screenshotSrc.value ? 'fallback -> 内存 URL 显示' : '未就绪'))
      const rawLongshotAvailable = longshotResultActive.value && !!longshotRawPngBase64.value && !hasOverlayForLongshotExport()
      const fallbackExport = rawLongshotAvailable ? 'fallback -> 原始长图 base64' : 'fallback -> canvas/base64'
      const saveRoute = hasBackendPath ? 'imagePath -> 后端导出' : fallbackExport
      const copyRoute = hasBackendPath ? 'imagePath -> 后端剪贴板' : 'fallback -> 浏览器剪贴板'
      const pinRoute = hasBackendPath ? 'imagePath -> 后端渲染 PNG' : fallbackExport
      if (sourcePath) {
        return {
          kind: 'backend',
          items: [
            {key: 'display', label: t('screenshot.source'), value: displaySource},
            {key: 'save', label: t('screenshot.linkSave'), value: saveRoute},
            {key: 'copy', label: t('screenshot.linkCopy'), value: copyRoute},
            {key: 'pin', label: t('screenshot.linkPin'), value: pinRoute},
            {key: 'path', label: t('screenshot.linkCurrentPath'), value: sourcePath}
          ]
        }
      }
      return {
        kind: 'fallback',
        items: [
          {key: 'display', label: t('screenshot.source'), value: displaySource},
          {key: 'save', label: t('screenshot.linkSave'), value: saveRoute},
          {key: 'copy', label: t('screenshot.linkCopy'), value: copyRoute},
          {key: 'pin', label: t('screenshot.linkPin'), value: pinRoute},
          {key: 'path', label: t('screenshot.linkCurrentPath'), value: t('screenshot.linkMemoryFallback')}
        ]
      }
    })

    function getPathBasename(filePath) {
      const normalized = String(filePath || '').replace(/\\/g, '/')
      const parts = normalized.split('/')
      return parts[parts.length - 1] || normalized
    }

    async function loadScreenshotSession() {
      try {
        const result = await invoke(IPC_COMMANDS.GET_SCREENSHOT_DATA)
        if (result?.success && result.image_path) {
          sourceImagePath.value = String(result.image_path || '')
          captureOriginX.value = Number(result.origin_x) || 0
          captureOriginY.value = Number(result.origin_y) || 0
          const sessionId = Number(result.session_id) || 0
          if (sessionId > 0) {
            activeSessionId.value = sessionId
            payloadSessionId.value = sessionId
          }
          screenshotSessionRequested.value = true
          hasScreenshotPayload.value = true
          await fetchWindows()
          regionSelectMode.value = String(result.selection_mode || 'screenshot')
          state.value = 'idle'
          currentTool.value = 'select'
          rect.width = 0
          rect.height = 0
          manualLongshotSessionId.value = 0
          manualLongshotRunning.value = false
          longshotResultActive.value = false
          longshotRawPngBase64.value = ''
          longshotViewScale.value = 1
          longshotViewOffset.x = 0
          longshotViewOffset.y = 0
          loadImageFromPath(result.image_path)
          // 图片开始加载，显示窗口
          invoke(IPC_COMMANDS.SET_SCREENSHOT_WINDOW_VISIBLE, {visible: true}).catch(() => {
          })
          return
        }
        // 无会话数据，窗口为预创建状态，不显示
      } catch (e) {
        console.error('加载截图会话失败:', e)
      }
    }

    function hasOverlayForLongshotExport() {
      return overlayDirty.value
    }

    function base64ToBlob(base64, mime = 'image/png') {
      const binary = atob(base64)
      const len = binary.length
      const bytes = new Uint8Array(len)
      for (let i = 0; i < len; i++) {
        bytes[i] = binary.charCodeAt(i)
      }
      return new Blob([bytes], {type: mime})
    }

    function resolveExportBase64() {
      if (longshotResultActive.value && longshotRawPngBase64.value && !hasOverlayForLongshotExport()) {

        return longshotRawPngBase64.value
      }
      const cropCanvas = getCroppedCanvas()
      const dataUrl = cropCanvas.toDataURL('image/png')
      return dataUrl.split(',')[1]
    }

    async function commitRecordingRegionSelection() {
      const payload = getGlobalSelectionRect()
      try {
        await invoke(IPC_COMMANDS.NOTIFY_RECORDING_REGION_SELECTED, {payload})
      } catch (_e) {
        await emit('recording-region-selected', payload)
      }
      await close()
    }

    function getGlobalSelectionRect() {
      const viewportW = Math.max(1, Number(window.innerWidth) || 1)
      const viewportH = Math.max(1, Number(window.innerHeight) || 1)
      const imageW = Math.max(1, Number(screenshotImg.value?.width) || viewportW)
      const imageH = Math.max(1, Number(screenshotImg.value?.height) || viewportH)
      const scaleX = imageW / viewportW
      const scaleY = imageH / viewportH
      const rawX = Math.round(captureOriginX.value + rect.x * scaleX)
      const rawY = Math.round(captureOriginY.value + rect.y * scaleY)
      const rawW = Math.max(1, Math.round(rect.width * scaleX))
      const rawH = Math.max(1, Math.round(rect.height * scaleY))
      const minX = Math.round(captureOriginX.value)
      const minY = Math.round(captureOriginY.value)
      const maxX = minX + imageW
      const maxY = minY + imageH
      const x = Math.max(minX, Math.min(maxX - 1, rawX))
      const y = Math.max(minY, Math.min(maxY - 1, rawY))
      const width = Math.max(1, Math.min(maxX - x, rawW))
      const height = Math.max(1, Math.min(maxY - y, rawH))
      return {
        x,
        y,
        width,
        height,
      }
    }

    let screenshotObjectUrl = ''

    function revokeScreenshotObjectUrl() {
      if (screenshotObjectUrl) {
        URL.revokeObjectURL(screenshotObjectUrl)
        screenshotObjectUrl = ''
      }
    }

    function loadImageFromBase64(base64Data) {
      loadImageFromSrc(`data:image/png;base64,${base64Data}`)
    }

    function loadImageFromPath(imagePath) {
      const fileUrl = buildFileUrlFromPath(imagePath)
      if (!fileUrl) {
        console.error('❌ 无法构建文件URL:', imagePath)
        isCaptureReady.value = false
        return
      }

      revokeScreenshotObjectUrl()
      loadImageFromSrc(fileUrl)
    }

    function initCanvas() {
      if (!canvas.value || !screenshotImg.value) return

      canvas.value.width = window.innerWidth * dpr
      canvas.value.height = window.innerHeight * dpr

      const ctx = canvas.value.getContext('2d')
      if (!ctx) return

      ctx.scale(dpr, dpr)


      saveToHistory()
      isCaptureReady.value = true
    }

    async function ensureCaptureReady() {
      if (isCaptureReady.value && screenshotImg.value && canvas.value && canvas.value.width > 0 && canvas.value.height > 0) {
        return true
      }
      if (!screenshotImg.value && !screenshotSrc.value) {
        await requestScreenshot()
      }
      if (!screenshotImg.value && screenshotSrc.value) {
        try {
          const img = new Image()
          await new Promise((resolve, reject) => {
            img.onload = resolve
            img.onerror = reject
            img.src = screenshotSrc.value
          })
          screenshotImg.value = img
        } catch (err) {
          console.error('加载截图源图失败:', err)
        }
      }
      await nextTick()
      if (screenshotImg.value && canvas.value) {
        initCanvas()
      }
      return Boolean(isCaptureReady.value && screenshotImg.value && canvas.value && canvas.value.width > 0 && canvas.value.height > 0)
    }

    function onContextMenu(e) {
      close()
    }

    function getLongshotFullCanvas() {
      const imageW = Math.max(1, Number(screenshotImg.value?.width) || 1)
      const imageH = Math.max(1, Number(screenshotImg.value?.height) || 1)
      const fullCanvas = document.createElement('canvas')
      fullCanvas.width = imageW
      fullCanvas.height = imageH
      const ctx = fullCanvas.getContext('2d')
      if (!ctx) {
        throw new Error('长截图导出画布上下文创建失败')
      }
      ctx.drawImage(screenshotImg.value, 0, 0, imageW, imageH)


      const view = getLongshotImageViewportRect(imageW, imageH)
      if (canvas.value) {
        ctx.drawImage(
            canvas.value,
            view.x,
            view.y,
            view.width,
            view.height,
            0,
            0,
            imageW,
            imageH
        )
      }
      drawShapeItemsOnLongshotCanvas(ctx, view)
      drawTextItemsOnLongshotCanvas(ctx, view)
      return fullCanvas
    }

    function getLongshotImageViewportRect(imageW, imageH) {
      const vw = Math.max(1, Number(window.innerWidth) || 1)
      const vh = Math.max(1, Number(window.innerHeight) || 1)
      const fit = Math.min(vw / imageW, vh / imageH)
      const width = imageW * fit
      const height = imageH * fit
      const x = (vw - width) / 2
      const y = (vh - height) / 2
      return {x, y, width, height, fit}
    }

    function sceneToImagePoint(x, y, view) {
      return {
        x: (x - view.x) / view.fit,
        y: (y - view.y) / view.fit
      }
    }

    function drawShapeItemsOnLongshotCanvas(ctx, view) {
      ctx.save()
      ctx.lineCap = 'round'
      for (const item of shapeItems.value) {
        ctx.strokeStyle = item.color
        ctx.lineWidth = Math.max(1, item.lineWidth / Math.max(0.0001, view.fit))
        const p = sceneToImagePoint(item.x, item.y, view)
        const x = p.x
        const y = p.y
        const w = item.width / view.fit
        const h = item.height / view.fit
        if (item.type === 'rect') {
          if (item.filled) {
            ctx.fillStyle = hexToRgba(item.color, item.fillOpacity ?? 0.35)
            ctx.fillRect(x, y, w, h)
          }
          ctx.strokeRect(x, y, w, h)
          continue
        }
        if (item.type === 'redact') {
          ctx.fillStyle = '#000000'
          ctx.fillRect(x, y, w, h)
          continue
        }
        if (item.type === 'circle' || item.type === 'number') {
          const cx = x + w / 2
          const cy = y + h / 2
          const rx = Math.abs(w / 2)
          const ry = Math.abs(h / 2)
          if (item.type === 'number' || item.filled) {
            ctx.fillStyle = item.type === 'number'
                ? item.color
                : hexToRgba(item.color, item.fillOpacity ?? 0.35)
            ctx.beginPath()
            ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2)
            ctx.fill()
          }
          if (item.type === 'number') {
            ctx.strokeStyle = item.color
            ctx.lineWidth = Math.max(1, (item.lineWidth || 2) / Math.max(0.0001, view.fit))
            ctx.beginPath()
            ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2)
            ctx.stroke()
            ctx.fillStyle = '#ffffff'
            const fontSize = Math.max(8, Math.min(w, h) * 0.48)
            ctx.font = `700 ${fontSize}px Arial`
            ctx.textAlign = 'center'
            ctx.textBaseline = 'middle'
            ctx.fillText(String(item.n || ''), cx, cy)
            continue
          }
          ctx.beginPath()
          ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2)
          ctx.stroke()
          continue
        }
        ctx.beginPath()
        const s1 = sceneToImagePoint(item.x + item.x1, item.y + item.y1, view)
        const s2 = sceneToImagePoint(item.x + item.x2, item.y + item.y2, view)
        ctx.moveTo(s1.x, s1.y)
        ctx.lineTo(s2.x, s2.y)
        ctx.stroke()
        if (item.type === 'arrow') {
          ctx.beginPath()
          const head = sceneToImagePoint(item.x + item.x2, item.y + item.y2, view)
          const left = sceneToImagePoint(item.x + item.arrowLeft.x, item.y + item.arrowLeft.y, view)
          const right = sceneToImagePoint(item.x + item.arrowRight.x, item.y + item.arrowRight.y, view)
          ctx.moveTo(head.x, head.y)
          ctx.lineTo(left.x, left.y)
          ctx.moveTo(head.x, head.y)
          ctx.lineTo(right.x, right.y)
          ctx.stroke()
        }
      }
      ctx.restore()
    }

    function drawTextItemsOnLongshotCanvas(ctx, view) {
      for (const item of textItems.value) {
        const lines = String(item.text || '').split('\n')
        const p = sceneToImagePoint(item.x, item.y, view)
        const x = p.x
        const y = p.y
        const fontSize = Math.max(8, item.fontSize / Math.max(0.0001, view.fit))
        const lineHeight = Math.max(fontSize * 1.25, fontSize + 4)
        const fontWeight = item.bold ? '700' : '400'
        ctx.save()
        ctx.fillStyle = item.color
        ctx.font = `${fontWeight} ${fontSize}px ${item.fontFamily || 'Arial'}`
        ctx.textBaseline = 'top'
        if (item.shadow) {
          ctx.shadowColor = 'rgba(0, 0, 0, 0.65)'
          ctx.shadowBlur = Math.max(4, Math.round(fontSize * 0.35))
          ctx.shadowOffsetX = 0
          ctx.shadowOffsetY = Math.max(1, Math.round(fontSize * 0.1))
        } else {
          ctx.shadowColor = 'transparent'
          ctx.shadowBlur = 0
          ctx.shadowOffsetX = 0
          ctx.shadowOffsetY = 0
        }
        for (let i = 0; i < lines.length; i++) {
          const lineY = y + i * lineHeight
          if (item.stroke) {
            ctx.strokeStyle = item.strokeColor || '#000000'
            ctx.lineWidth = Math.max(1, Math.round(fontSize / 14))
            ctx.strokeText(lines[i], x, lineY)
          }
          ctx.fillText(lines[i], x, lineY)
        }
        ctx.restore()
      }
    }

    function drawTextItemsOnCroppedCanvas(ctx, sourceX, sourceY, sourceWidth, sourceHeight) {
      const cropLeft = sourceX / dpr
      const cropTop = sourceY / dpr
      const cropRight = cropLeft + sourceWidth / dpr
      const cropBottom = cropTop + sourceHeight / dpr
      for (const item of textItems.value) {
        if (item.x > cropRight || item.y > cropBottom) continue
        if (item.x < cropLeft - 400 || item.y < cropTop - 200) continue
        drawTextItemToContext(ctx, item, item.x - cropLeft, item.y - cropTop)
      }
    }

    function drawTextItemToContext(ctx, item, x, y) {
      const lines = item.text.split('\n')
      const fontWeight = item.bold ? '700' : '400'
      ctx.save()
      ctx.scale(dpr, dpr)
      ctx.fillStyle = item.color
      ctx.font = `${fontWeight} ${item.fontSize}px ${item.fontFamily || 'Arial'}`
      ctx.textBaseline = 'top'
      if (item.shadow) {
        ctx.shadowColor = 'rgba(0, 0, 0, 0.65)'
        ctx.shadowBlur = Math.max(4, Math.round(item.fontSize * 0.35))
        ctx.shadowOffsetX = 0
        ctx.shadowOffsetY = Math.max(1, Math.round(item.fontSize * 0.1))
      } else {
        ctx.shadowColor = 'transparent'
        ctx.shadowBlur = 0
        ctx.shadowOffsetX = 0
        ctx.shadowOffsetY = 0
      }
      const lineHeight = Math.max(item.fontSize * 1.25, item.fontSize + 4)
      for (let i = 0; i < lines.length; i++) {
        const lineY = y + i * lineHeight
        if (item.stroke) {
          ctx.strokeStyle = item.strokeColor || '#000000'
          ctx.lineWidth = Math.max(1, Math.round(item.fontSize / 14))
          ctx.strokeText(lines[i], x, lineY)
        }
        ctx.fillText(lines[i], x, lineY)
      }
      ctx.restore()
    }

    function drawShapeItemsOnCroppedCanvas(ctx, sourceX, sourceY, sourceWidth, sourceHeight) {
      const cropLeft = sourceX / dpr
      const cropTop = sourceY / dpr
      const cropRight = cropLeft + sourceWidth / dpr
      const cropBottom = cropTop + sourceHeight / dpr
      for (const item of shapeItems.value) {
        const itemRight = item.x + item.width
        const itemBottom = item.y + item.height
        if (itemRight < cropLeft || itemBottom < cropTop || item.x > cropRight || item.y > cropBottom) continue
        drawShapeItemToContext(ctx, item, cropLeft, cropTop)
      }
    }

    function drawShapeItemToContext(ctx, item, cropLeft, cropTop) {
      const x = item.x - cropLeft
      const y = item.y - cropTop
      ctx.save()
      ctx.scale(dpr, dpr)
      ctx.strokeStyle = item.color
      ctx.lineWidth = item.lineWidth
      ctx.lineCap = 'round'
      if (item.type === 'rect') {
        if (item.filled) {
          ctx.fillStyle = hexToRgba(item.color, item.fillOpacity ?? 0.35)
          ctx.fillRect(x, y, item.width, item.height)
        }
        if (item.type !== 'redact') {
          ctx.strokeRect(x, y, item.width, item.height)
        }
        ctx.restore()
        return
      }
      if (item.type === 'redact') {
        ctx.fillStyle = '#000000'
        ctx.fillRect(x, y, item.width, item.height)
        ctx.restore()
        return
      }
      if (item.type === 'circle' || item.type === 'number') {
        const cx = x + item.width / 2
        const cy = y + item.height / 2
        const rx = item.width / 2
        const ry = item.height / 2
        if (item.type === 'number' || item.filled) {
          ctx.fillStyle = item.type === 'number'
              ? item.color
              : hexToRgba(item.color, item.fillOpacity ?? 0.35)
          ctx.beginPath()
          ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2)
          ctx.fill()
        }
        if (item.type === 'number') {
          ctx.strokeStyle = item.color
          ctx.lineWidth = Math.max(2, item.lineWidth || 2)
          ctx.beginPath()
          ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2)
          ctx.stroke()
          ctx.fillStyle = '#ffffff'
          const fontSize = Math.max(10, Math.min(item.width, item.height) * 0.48)
          ctx.font = `700 ${fontSize}px Arial`
          ctx.textAlign = 'center'
          ctx.textBaseline = 'middle'
          ctx.fillText(String(item.n || ''), cx, cy)
          ctx.restore()
          return
        }
        ctx.beginPath()
        ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2)
        ctx.stroke()
        ctx.restore()
        return
      }
      ctx.beginPath()
      ctx.moveTo(x + item.x1, y + item.y1)
      ctx.lineTo(x + item.x2, y + item.y2)
      ctx.stroke()
      if (item.type === 'arrow') {
        ctx.beginPath()
        ctx.moveTo(x + item.x2, y + item.y2)
        ctx.lineTo(x + item.arrowLeft.x, y + item.arrowLeft.y)
        ctx.moveTo(x + item.x2, y + item.y2)
        ctx.lineTo(x + item.arrowRight.x, y + item.arrowRight.y)
        ctx.stroke()
      }
      ctx.restore()
    }

    async function writeClipboardImage(linked, closeAfterCopy) {
      try {
        if (!(await ensureCaptureReady())) {
          showCaptureError('截图源尚未就绪，请稍后重试')
          return
        }
        await invoke(IPC_COMMANDS.SET_SCREENSHOT_CLIPBOARD_LINK_ONCE, {linked})
        if (sourceImagePath.value) {
          const result = await invoke(IPC_COMMANDS.COPY_SCREENSHOT_TO_CLIPBOARD, {
            request: buildBackendExportRequest('')
          })
          if (!result?.success) {
            throw new Error(result?.error || result?.message || '复制失败')
          }
        } else {
          const pngBase64 = resolveExportBase64()
          const blob = base64ToBlob(pngBase64, 'image/png')
          await navigator.clipboard.write([new ClipboardItem({'image/png': blob})])
        }
        if (closeAfterCopy) {
          close()
        }
      } catch (error) {
        console.error('复制失败:', error)
        showCaptureError('复制失败')
      }
    }

    async function copyToClipboardLinked() {
      await writeClipboardImage(true, true)
    }

    async function completeAndCopyUnlinked() {
      await writeClipboardImage(false, true)
    }

    async function pinToScreenAndClose() {
      try {
        if (!(await ensureCaptureReady())) {
          showCaptureError('截图源尚未就绪，请稍后重试')
          return
        }
        let base64 = ''
        let pinWidth = 0
        let pinHeight = 0
        const region = getGlobalSelectionRect()
        if (sourceImagePath.value) {
          const renderResult = await invoke(IPC_COMMANDS.RENDER_SCREENSHOT_TO_PNG_DATA, {
            request: buildBackendExportRequest('')
          })
          if (!renderResult?.success) {
            throw new Error(renderResult?.error || renderResult?.message || '生成固定图片失败')
          }
          base64 = String(renderResult?.pngBase64 || '')
          pinWidth = Math.max(1, Number(renderResult?.width) || 1)
          pinHeight = Math.max(1, Number(renderResult?.height) || 1)
        } else {
          base64 = resolveExportBase64()
          pinWidth = region.width
          pinHeight = region.height
        }

        const payload = {
          request: {
            pngBase64: base64,
            x: region.x,
            y: region.y,
            width: pinWidth,
            height: pinHeight
          }
        }
        let result = null
        try {
          result = await invoke(IPC_COMMANDS.PIN_SCREENSHOT_ON_SCREEN, payload)
        } catch (firstError) {
          await new Promise(resolve => setTimeout(resolve, 80))
          result = await invoke(IPC_COMMANDS.PIN_SCREENSHOT_ON_SCREEN, payload)
          if (!result?.success) {
            throw firstError
          }
        }
        if (result?.success) {
          close()
        }
      } catch (error) {
        const reason = error?.message
            || (typeof error === 'string' ? error : JSON.stringify(error))
            || '未知错误'
        console.error('固定图片失败:', error)
        showCaptureError(`固定图片失败：${reason}`)
      }
    }

    async function saveAndClose() {
      try {
        if (!(await ensureCaptureReady())) {
          showCaptureError('截图源尚未就绪，请稍后重试')
          return
        }

        const pathResult = await invoke(IPC_COMMANDS.CHOOSE_SCREENSHOT_SAVE_PATH)
        if (!pathResult?.success) {
          if (pathResult?.cancelled) {
            return
          }
          showCaptureError(pathResult?.error || pathResult?.message || '选择保存路径失败')
          return
        }




        const shouldSaveRenderedImageDirectly = !sourceImagePath.value

        if (shouldSaveRenderedImageDirectly) {
          const base64 = resolveExportBase64()
          const result = await invoke(IPC_COMMANDS.SAVE_SCREENSHOT_TO_PATH, {
            pngBase64: base64,
            outputPath: pathResult.path
          })
          if (result?.success) {
            close()
            return
          }
          showCaptureError(result?.error || result?.message || '保存失败')
          return
        }

        if (sourceImagePath.value) {
          const exportResult = await invoke(IPC_COMMANDS.EXPORT_SCREENSHOT_TO_PATH, {
            request: buildBackendExportRequest(pathResult.path)
          })
          if (exportResult?.success) {
            close()
            return
          }
          showCaptureError(exportResult?.error || exportResult?.message || '保存失败')
          return
        }
      } catch (error) {
        const reason = error?.message
            || (typeof error === 'string' ? error : JSON.stringify(error))
            || '未知错误'
        console.error('保存失败:', error)
        showCaptureError(`保存失败：${reason}`)
      }
    }

    return {
        screenshotSrc,
        screenshotImg,
        canvas,
        isCaptureReady,
        captureReadyError,
        canExport,
        canCropToSelection,
        showExportRouteIndicator,
        exportRouteIndicator,
        revokeScreenshotObjectUrl,
        loadScreenshotSession,
        initCanvas,
        loadImageFromBase64,
        loadImageFromPath,
        getGlobalSelectionRect,
        getLongshotImageViewportRect,
        getLongshotFullCanvas,
        sceneToImagePoint,
        drawShapeItemsOnCroppedCanvas,
        drawTextItemsOnCroppedCanvas,
        copyToClipboardLinked,
        completeAndCopyUnlinked,
        pinToScreenAndClose,
        saveAndClose,
        commitRecordingRegionSelection,
        onContextMenu,
        resolveCropDataUrl
    }
}
