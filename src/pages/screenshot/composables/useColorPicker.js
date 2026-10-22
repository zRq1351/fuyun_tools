//! useColorPicker — 取色器 composable。
//! 职责：取色值状态（hex/rgb 显示模式）、取色器浮层定位与样式、
//! 场景坐标取色与复制到剪贴板。
//! 跨域依赖（绘制画布采样、放大镜渲染、当前颜色/工具）通过 deps 注入。

import {computed, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {IPC_COMMANDS} from '../../../services/ipc.js'

export function useColorPicker(deps) {
    const {
        t,
        dpr,
        drawStart,
        isDrawing,
        currentColor,
        currentTool,
        toScenePoint,
        getMergedPixelColorAt,
        renderPickerMagnifier
    } = deps

    const pickerInfoRef = ref(null)
    const pickerMagnifierCanvasRef = ref(null)

    const pickColor = ref('')
    const pickColorRgb = ref('')
    const pickerDisplayMode = ref('hex')
    const pickerCopyHint = ref(t('screenshot.pickerHint'))

    const pickerDisplayValue = computed(() => {
      return pickerDisplayMode.value === 'rgb' ? pickColorRgb.value : pickColor.value
    })

    const pickerStyle = computed(() => {
      // 取色器实际尺寸：放大镜 132px + gap 8px + meta ~130px ≈ 270px 宽，132px 高
      const pickerWidth = 276
      const pickerHeight = 140
      const margin = 16

      // 优先显示在鼠标右下方
      let left = drawStart.x + margin
      let top = drawStart.y + margin

      // 右溢出：改到鼠标左侧
      if (left + pickerWidth > window.innerWidth - margin) {
        left = drawStart.x - pickerWidth - margin
      }
      // 下溢出：改到鼠标上方
      if (top + pickerHeight > window.innerHeight - margin) {
        top = drawStart.y - pickerHeight - margin
      }
      // 边界兜底
      left = Math.max(margin, Math.min(left, window.innerWidth - pickerWidth - margin))
      top = Math.max(margin, Math.min(top, window.innerHeight - pickerHeight - margin))

      return {
        left: `${left}px`,
        top: `${top}px`
      }
    })

    function pickColorAt(event) {
      const p = toScenePoint(event)
      pickColorAtScene(p.x, p.y)
    }

    function pickColorAtScene(sceneX, sceneY) {
      const px = Math.round(sceneX * dpr)
      const py = Math.round(sceneY * dpr)
      const color = getMergedPixelColorAt(px, py)
      if (!color) return
      const hex = '#' + [color.r, color.g, color.b].map(v => v.toString(16).padStart(2, '0')).join('')
      pickColor.value = hex.toUpperCase()
      pickColorRgb.value = `RGB(${color.r}, ${color.g}, ${color.b})`
      if (isDrawing.value) {
        currentColor.value = pickColor.value
      }
      renderPickerMagnifier(px, py)
    }

    async function copyPickedColor() {
      if (!pickColor.value) return
      const value = pickerDisplayValue.value || pickColor.value
      try {
        await invoke(IPC_COMMANDS.COPY_TEXT, {text: value})
        pickerCopyHint.value = `已复制：${value}`
        window.setTimeout(() => {
          if (currentTool.value === 'picker') {
            pickerCopyHint.value = t('screenshot.pickerHint')
          }
        }, 1000)
      } catch (error) {
        pickerCopyHint.value = '复制失败，请重试'
      }
    }

    return {
        pickColor,
        pickColorRgb,
        pickerDisplayMode,
        pickerCopyHint,
        pickerInfoRef,
        pickerMagnifierCanvasRef,
        pickerDisplayValue,
        pickerStyle,
        pickColorAtScene,
        copyPickedColor
    }
}
