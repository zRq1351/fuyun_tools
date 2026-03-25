import {ref} from 'vue'
import {resolveContextMenuPosition} from './contextMenuPosition'

export const useContextMenuState = (initialItem = null, positionOptions = {}) => {
    const contextMenuVisible = ref(false)
    const contextMenuX = ref(0)
    const contextMenuY = ref(0)
    const contextMenuItem = ref(initialItem)

    const openContextMenu = (event, item) => {
        contextMenuVisible.value = true
        contextMenuItem.value = item
        const {x, y} = resolveContextMenuPosition(event, positionOptions)
        contextMenuX.value = x
        contextMenuY.value = y
    }

    const closeContextMenu = () => {
        contextMenuVisible.value = false
        contextMenuItem.value = initialItem
    }

    return {
        contextMenuVisible,
        contextMenuX,
        contextMenuY,
        contextMenuItem,
        openContextMenu,
        closeContextMenu
    }
}
