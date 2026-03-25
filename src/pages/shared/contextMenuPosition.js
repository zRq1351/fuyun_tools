export const resolveContextMenuPosition = (event, options = {}) => {
    const menuWidth = Number.isFinite(options.menuWidth) ? options.menuWidth : 160
    const maxHeightPx = Number.isFinite(options.maxHeightPx) ? options.maxHeightPx : 300
    const maxHeightRatio = Number.isFinite(options.maxHeightRatio) ? options.maxHeightRatio : 0.6
    const edgePadding = Number.isFinite(options.edgePadding) ? options.edgePadding : 8
    const viewportWidth = window.innerWidth || 0
    const viewportHeight = window.innerHeight || 0
    const menuHeight = Math.min(maxHeightPx, viewportHeight * maxHeightRatio)

    let x = event.clientX
    let y = event.clientY

    if (x + menuWidth > viewportWidth - edgePadding) {
        x = Math.max(edgePadding, viewportWidth - menuWidth - edgePadding)
    }
    if (y + menuHeight > viewportHeight - edgePadding) {
        y = Math.max(edgePadding, viewportHeight - menuHeight - edgePadding)
    }

    x = Math.max(edgePadding, x)
    y = Math.max(edgePadding, y)

    return {x, y}
}
