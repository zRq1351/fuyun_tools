// 图片剪贴板列表/历史的纯逻辑工具
// 供 App.vue 与 ImageClipboardList.vue 使用，同时便于独立单测

/**
 * 可见位 → 原始 history 下标。
 * filteredHistory 条目的 index 字段即其在原始 history 数组中的下标；
 * 组件内部（卡片位置、滚轮、跳转）一律使用可见位，只有 selectedIndex
 * 等全局状态使用原始下标，选中/回填前必须换算。
 */
export const visibleToRawIndex = (visibleEntries, visibleIndex) => {
    const entry = Array.isArray(visibleEntries) ? visibleEntries[visibleIndex] : null
    return Number.isInteger(entry?.index) ? entry.index : visibleIndex
}

/**
 * 按 id 定位删除（不信任调用方传入的下标，过滤态下可见位≠原始位），
 * 并保持选中项跟随原条目：返回新数组与修正后的 selectedIndex。
 */
export const removeImageHistoryItem = (history, selectedIndex, itemId) => {
    if (!Array.isArray(history) || !itemId) {
        return {history, selectedIndex}
    }
    const index = history.findIndex((item) => item?.id === itemId)
    if (index < 0) {
        return {history, selectedIndex}
    }
    const selectedId = history[selectedIndex]?.id
    const next = history.slice()
    next.splice(index, 1)
    let nextSelectedIndex = selectedIndex
    if (selectedId && selectedId !== itemId) {
        const found = next.findIndex((item) => item?.id === selectedId)
        nextSelectedIndex = found >= 0 ? found : (next.length > 0 ? 0 : -1)
    } else if (next.length === 0) {
        nextSelectedIndex = -1
    } else if (nextSelectedIndex >= next.length) {
        nextSelectedIndex = next.length - 1
    }
    return {history: next, selectedIndex: nextSelectedIndex}
}

/**
 * 分页写入 + 去重压缩。
 * reset=true 表示整页重取（调用方 offset 恒为 0）：先重建空数组再写入，
 * 截断旧数据，避免交界处残留旧条目/丢失中段条目。
 * reset=false 时按 baseOffset 写入对应槽位，最后按 id 去重压缩。
 */
export const applyImagePageToHistory = (currentHistory, items, baseOffset, reset) => {
    const slots = reset ? [] : (Array.isArray(currentHistory) ? currentHistory.slice() : [])
    const pageItems = Array.isArray(items) ? items : []
    for (let i = 0; i < pageItems.length; i++) {
        const item = pageItems[i]
        if (!item) continue
        slots[baseOffset + i] = {
            id: item.id,
            width: item.width,
            height: item.height,
            preview_png_base64: item.previewPngBase64,
            image_path: item.imagePath
        }
    }
    const seenIds = new Set()
    const compacted = []
    for (let i = 0; i < slots.length; i++) {
        const item = slots[i]
        if (!item) continue
        if (!seenIds.has(item.id)) {
            seenIds.add(item.id)
            compacted.push(item)
        }
    }
    return compacted
}
