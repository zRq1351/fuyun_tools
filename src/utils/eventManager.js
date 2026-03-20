/**
 * 统一事件监听器管理器
 * 解决事件监听器生命周期管理问题，防止内存泄漏
 */

class EventManager {
    constructor() {
        this.listeners = new Map()
        this.tauriListeners = new Map()
        this.domListeners = new Map()
        this.intervals = new Map()
        this.timeouts = new Map()
    }

    /**
     * 添加 Tauri 事件监听器
     */
    async addTauriListener(event, callback, options = {}) {
        const {once = false, key = null} = options
        const listenerKey = key || `tauri_${event}_${Date.now()}`

        try {
            const {listen} = await import('@tauri-apps/api/event')

            let unlisten
            if (once) {
                const {once: onceFn} = await import('@tauri-apps/api/event')
                unlisten = await onceFn(event, callback)
            } else {
                unlisten = await listen(event, callback)
            }

            this.tauriListeners.set(listenerKey, {
                unlisten,
                event,
                callback,
                createdAt: Date.now()
            })

            return listenerKey
        } catch (error) {
            console.error(`添加 Tauri 事件监听器失败 (${event}):`, error)
            return null
        }
    }

    /**
     * 添加 DOM 事件监听器
     */
    addDomListener(element, event, callback, options = {}) {
        const {once = false, capture = false, passive = false, key = null} = options
        const listenerKey = key || `dom_${event}_${element?.tagName || 'unknown'}_${Date.now()}`

        if (!element || !element.addEventListener) {
            console.warn('无效的 DOM 元素')
            return null
        }

        const wrappedCallback = once ? (...args) => {
            callback(...args)
            this.removeDomListener(listenerKey)
        } : callback

        element.addEventListener(event, wrappedCallback, {capture, passive})

        this.domListeners.set(listenerKey, {
            element,
            event,
            callback: wrappedCallback,
            originalCallback: callback,
            options: {once, capture, passive},
            createdAt: Date.now()
        })

        return listenerKey
    }

    /**
     * 添加定时器
     */
    addInterval(callback, delay, key = null) {
        const intervalKey = key || `interval_${Date.now()}`
        const intervalId = setInterval(callback, delay)

        this.intervals.set(intervalKey, {
            id: intervalId,
            callback,
            delay,
            createdAt: Date.now()
        })

        return intervalKey
    }

    /**
     * 添加超时器
     */
    addTimeout(callback, delay, key = null) {
        const timeoutKey = key || `timeout_${Date.now()}`
        const timeoutId = setTimeout(() => {
            callback()
            this.timeouts.delete(timeoutKey)
        }, delay)

        this.timeouts.set(timeoutKey, {
            id: timeoutId,
            callback,
            delay,
            createdAt: Date.now()
        })

        return timeoutKey
    }

    /**
     * 移除 Tauri 事件监听器
     */
    removeTauriListener(key) {
        const listener = this.tauriListeners.get(key)
        if (listener) {
            try {
                if (listener.unlisten && typeof listener.unlisten === 'function') {
                    listener.unlisten()
                }
            } catch (error) {
                console.warn(`移除 Tauri 事件监听器失败 (${key}):`, error)
            }
            this.tauriListeners.delete(key)
            return true
        }
        return false
    }

    /**
     * 移除 DOM 事件监听器
     */
    removeDomListener(key) {
        const listener = this.domListeners.get(key)
        if (listener) {
            try {
                if (listener.element && listener.element.removeEventListener) {
                    listener.element.removeEventListener(
                        listener.event,
                        listener.callback,
                        listener.options
                    )
                }
            } catch (error) {
                console.warn(`移除 DOM 事件监听器失败 (${key}):`, error)
            }
            this.domListeners.delete(key)
            return true
        }
        return false
    }

    /**
     * 移除定时器
     */
    removeInterval(key) {
        const interval = this.intervals.get(key)
        if (interval) {
            clearInterval(interval.id)
            this.intervals.delete(key)
            return true
        }
        return false
    }

    /**
     * 移除超时器
     */
    removeTimeout(key) {
        const timeout = this.timeouts.get(key)
        if (timeout) {
            clearTimeout(timeout.id)
            this.timeouts.delete(key)
            return true
        }
        return false
    }

    /**
     * 移除指定事件的所有监听器
     */
    removeAllListenersForEvent(event) {
        let removed = 0

        // 移除 Tauri 事件监听器
        for (const [key, listener] of this.tauriListeners) {
            if (listener.event === event) {
                this.removeTauriListener(key)
                removed++
            }
        }

        // 移除 DOM 事件监听器
        for (const [key, listener] of this.domListeners) {
            if (listener.event === event) {
                this.removeDomListener(key)
                removed++
            }
        }

        return removed
    }

    /**
     * 移除所有事件监听器
     */
    removeAllListeners() {
        let removed = 0

        // 移除所有 Tauri 事件监听器
        for (const key of this.tauriListeners.keys()) {
            this.removeTauriListener(key)
            removed++
        }

        // 移除所有 DOM 事件监听器
        for (const key of this.domListeners.keys()) {
            this.removeDomListener(key)
            removed++
        }

        // 移除所有定时器
        for (const key of this.intervals.keys()) {
            this.removeInterval(key)
            removed++
        }

        // 移除所有超时器
        for (const key of this.timeouts.keys()) {
            this.removeTimeout(key)
            removed++
        }

        return removed
    }

    /**
     * 获取监听器统计信息
     */
    getStats() {
        return {
            tauriListeners: this.tauriListeners.size,
            domListeners: this.domListeners.size,
            intervals: this.intervals.size,
            timeouts: this.timeouts.size,
            total: this.tauriListeners.size + this.domListeners.size + this.intervals.size + this.timeouts.size
        }
    }

    /**
     * 检查是否存在指定事件的监听器
     */
    hasListener(event) {
        for (const listener of this.tauriListeners.values()) {
            if (listener.event === event) return true
        }
        for (const listener of this.domListeners.values()) {
            if (listener.event === event) return true
        }
        return false
    }

    /**
     * 获取指定键的监听器信息
     */
    getListenerInfo(key) {
        return this.tauriListeners.get(key) ||
            this.domListeners.get(key) ||
            this.intervals.get(key) ||
            this.timeouts.get(key) ||
            null
    }
}

// 创建全局实例
const eventManager = new EventManager()

// 导出实例和类
export {eventManager, EventManager}
export default eventManager