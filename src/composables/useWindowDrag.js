import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'

/**
 * 窗口拖拽 composable
 * 封装了 Tauri 窗口拖拽功能，支持防抖和可选的降级方案
 * @param {Object} [options] - 配置选项
 * @param {number} [options.debounceMs=220] - 防抖间隔（毫秒）
 * @param {Function} [options.fallback] - 降级拖拽函数，当 startDragging 失败时调用
 * @returns {{ startDrag: Function }}
 */
export function useWindowDrag(options = {}) {
    const {debounceMs = 220, fallback = null} = options
    let lastDragStartAt = 0

    async function startDrag() {
        const now = Date.now()
        if (now - lastDragStartAt < debounceMs) return
        lastDragStartAt = now

        try {
            await getCurrentWebviewWindow().startDragging()
        } catch (error) {
            if (fallback) {
                try {
                    await fallback()
                } catch (_) {
                }
            }
        }
    }

    return {startDrag}
}
