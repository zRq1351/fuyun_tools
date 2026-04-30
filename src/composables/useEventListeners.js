import {onUnmounted} from 'vue'
import {listen} from '@tauri-apps/api/event'

/**
 * 事件监听 composable
 * 自动管理 Tauri 事件监听器的注册和清理
 * @returns {{ listenEvent: Function, cleanupAll: Function }}
 */
export function useEventListeners() {
    const unlisteners = []

    /**
     * 注册一个事件监听器，组件卸载时自动清理
     * @param {string} eventName - 事件名称
     * @param {Function} handler - 事件处理函数
     * @returns {Promise<void>}
     */
    async function listenEvent(eventName, handler) {
        const unlisten = await listen(eventName, handler)
        unlisteners.push(unlisten)
    }

    /**
     * 手动清理所有已注册的监听器
     */
    function cleanupAll() {
        for (const unlisten of unlisteners) {
            try {
                unlisten()
            } catch (_) {
            }
        }
        unlisteners.length = 0
    }

    onUnmounted(cleanupAll)

    return {listenEvent, cleanupAll}
}
