import {ref} from 'vue'
import {WindowService} from '../../../services/ipc'

export function useWindowOffset() {
    const bottomOffset = ref(8)
    let isWindowOffsetDragging = false
    let dragStartScreenY = 0
    let dragStartBottomOffset = 0
    let previewFramePending = false
    let previewRequestInFlight = false
    let queuedPreviewOffset = null
    let pendingPreviewOffset = 0

    const clampBottomOffset = (offset) => {
        return Math.max(0, Math.min(400, Math.round(offset)))
    }

    const flushPreviewBottomOffset = () => {
        previewFramePending = false
        const offset = pendingPreviewOffset
        if (previewRequestInFlight) {
            queuedPreviewOffset = offset
            return
        }
        previewRequestInFlight = true
        WindowService.previewBottomOffset(offset)
            .catch((error) => {
                console.error('预览窗口偏移失败:', error)
            })
            .finally(() => {
                previewRequestInFlight = false
                if (queuedPreviewOffset !== null && queuedPreviewOffset !== offset) {
                    pendingPreviewOffset = queuedPreviewOffset
                    queuedPreviewOffset = null
                    flushPreviewBottomOffset()
                } else {
                    queuedPreviewOffset = null
                }
            })
    }

    const schedulePreviewBottomOffset = (offset) => {
        pendingPreviewOffset = offset
        if (previewFramePending) return
        previewFramePending = true
        requestAnimationFrame(flushPreviewBottomOffset)
    }

    const handleWindowOffsetDrag = (event) => {
        if (!isWindowOffsetDragging) return
        const deltaY = event.screenY - dragStartScreenY
        const nextOffset = clampBottomOffset(dragStartBottomOffset - deltaY)
        if (nextOffset === bottomOffset.value) return
        bottomOffset.value = nextOffset
        schedulePreviewBottomOffset(nextOffset)
    }

    const endWindowOffsetDrag = async () => {
        if (!isWindowOffsetDragging) return
        isWindowOffsetDragging = false
        window.removeEventListener('mousemove', handleWindowOffsetDrag)
        window.removeEventListener('mouseup', endWindowOffsetDrag)
        try {
            await WindowService.saveBottomOffset(bottomOffset.value)
        } catch (error) {
            console.error('保存窗口偏移失败:', error)
        }
    }

    const startWindowOffsetDrag = (event) => {
        isWindowOffsetDragging = true
        dragStartScreenY = event.screenY
        dragStartBottomOffset = bottomOffset.value
        window.addEventListener('mousemove', handleWindowOffsetDrag)
        window.addEventListener('mouseup', endWindowOffsetDrag)
    }

    return {
        bottomOffset,
        clampBottomOffset,
        startWindowOffsetDrag
    }
}
