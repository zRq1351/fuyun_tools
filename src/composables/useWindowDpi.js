import {onUnmounted, ref} from 'vue'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {currentMonitor} from '@tauri-apps/api/window'
import {PhysicalPosition} from '@tauri-apps/api/dpi'

/**
 * 窗口 DPI composable
 * 封装 scaleFactor 监听、logical/physical 像素换算，
 * 以及将窗口位置 clamp 到当前显示器边界内的工具方法。
 * @returns {{
 *   scaleFactor: import('vue').Ref<number>,
 *   toPhysical: Function,
 *   toLogical: Function,
 *   clampToMonitor: Function,
 *   clampPositionToMonitor: Function,
 * }}
 */
export function useWindowDpi() {
    const appWindow = getCurrentWebviewWindow()
    const scaleFactor = ref(1)
    let unlistenScale = null

    /** logical → physical（四舍五入，与 Tauri 内部取整一致） */
    function toPhysical(logical) {
        return Math.round(logical * scaleFactor.value)
    }

    /** physical → logical */
    function toLogical(physical) {
        return physical / scaleFactor.value
    }

    /**
     * 将物理坐标 (x, y) + 物理尺寸 (width, height) clamp 到当前显示器边界内
     * @returns {Promise<{x: number, y: number}>}
     */
    async function clampToMonitor(x, y, width, height) {
        const monitor = await currentMonitor()
        if (!monitor) return {x, y}
        const minX = monitor.position.x
        const minY = monitor.position.y
        const maxX = monitor.position.x + monitor.size.width - width
        const maxY = monitor.position.y + monitor.size.height - height
        return {
            x: Math.max(minX, Math.min(x, maxX)),
            y: Math.max(minY, Math.min(y, maxY)),
        }
    }

    /** 把当前窗口位置 clamp 回显示器边界内（DPI / 显示器切换后调用） */
    async function clampPositionToMonitor() {
        try {
            const pos = await appWindow.outerPosition()
            const size = await appWindow.outerSize()
            const {x, y} = await clampToMonitor(pos.x, pos.y, size.width, size.height)
            if (x !== pos.x || y !== pos.y) {
                await appWindow.setPosition(new PhysicalPosition(x, y))
            }
        } catch (e) {
            console.error('校准窗口位置失败:', e)
        }
    }

    // 立即初始化，保证首次换算前 scaleFactor 已就绪
    ;(async () => {
        try {
            scaleFactor.value = await appWindow.scaleFactor()
            unlistenScale = await appWindow.onScaleChanged(({payload}) => {
                scaleFactor.value = payload.scaleFactor
            })
        } catch {
        }
    })()

    onUnmounted(() => {
        if (unlistenScale) unlistenScale()
    })

    return {scaleFactor, toPhysical, toLogical, clampToMonitor, clampPositionToMonitor}
}
