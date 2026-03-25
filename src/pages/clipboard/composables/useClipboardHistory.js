import {computed, ref} from 'vue'
import {CategoryService, ClipboardService} from '../../../services/ipc'

export function useClipboardHistory() {
    const history = ref([])
    const pagedHistory = ref([])
    const selectedIndex = ref(-1)
    const searchKeyword = ref('')
    const categoryFilter = ref('全部')
    const categoryMap = ref({})
    const pageOffset = ref(0)
    const pageSize = ref(50)
    const totalCount = ref(0)
    const hasMore = ref(false)
    const isLoadingPage = ref(false)
    const sortBy = ref('pinnedFirst')
    const sortOrder = ref('asc')

    const getItemCategory = (item) => {
        return categoryMap.value[item] || '未分类'
    }

    const visibleHistory = computed(() => {
        return pagedHistory.value.map((entry) => ({
            content: entry.content,
            index: entry.position,
            snippet: entry.snippet || ''
        }))
    })

    const updateSelection = (index, shouldScroll = false, contentRef = null, visibleIndex = null) => {
        if (index < 0 || index >= history.value.length) return
        selectedIndex.value = index
    }

    const rebuildHistoryArray = () => {
        if (pagedHistory.value.length === 0) {
            history.value = []
            return
        }
        const maxPosition = pagedHistory.value.reduce((max, entry) => Math.max(max, entry.position), -1)
        const nextHistory = new Array(maxPosition + 1).fill('')
        for (const entry of pagedHistory.value) {
            nextHistory[entry.position] = entry.content
        }
        history.value = nextHistory
    }

    const sortPageItems = (entries) => {
        const merged = entries.slice()
        const orderKey = 'pinnedFirst'
        merged.sort((a, b) => {
            if (orderKey === 'pinnedFirst') {
                const pinDiff = (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0)
                if (pinDiff !== 0) return pinDiff
                const diff = a.position - b.position
                if (a.pinned && b.pinned) {
                    return diff
                }
                return sortOrder.value === 'desc' ? -diff : diff
            }
            if (orderKey === 'updatedAt') {
                const diff = (a.updatedAt || 0) - (b.updatedAt || 0)
                return sortOrder.value === 'desc' ? -diff : diff
            }
            const diff = a.position - b.position
            return sortOrder.value === 'desc' ? -diff : diff
        })
        return merged
    }

    const buildSortedGroups = () => {
        const sorted = sortPageItems(pagedHistory.value)
        const pinned = sorted.filter((entry) => entry.pinned)
        const unpinned = sorted.filter((entry) => !entry.pinned)
        return {pinned, unpinned}
    }

    const applyGroupedEntries = (pinnedEntries, unpinnedEntries) => {
        const pinned = pinnedEntries.map((entry, idx) => ({
            ...entry,
            pinned: true,
            position: idx
        }))
        const unpinnedBase = pinned.length
        const unpinnedCount = unpinnedEntries.length
        const unpinned = unpinnedEntries.map((entry, idx) => ({
            ...entry,
            pinned: false,
            position: sortOrder.value === 'desc'
                ? unpinnedBase + (unpinnedCount - idx - 1)
                : unpinnedBase + idx
        }))
        pagedHistory.value = [...pinned, ...unpinned]
    }

    const mergePageItems = (items, reset) => {
        if (reset) {
            pagedHistory.value = sortPageItems(items)
            return
        }
        const map = new Map(pagedHistory.value.map((entry) => [entry.position, entry]))
        for (const item of items) {
            map.set(item.position, item)
        }
        const merged = Array.from(map.values())
        pagedHistory.value = sortPageItems(merged)
    }

    const loadHistoryPage = async ({reset = false} = {}) => {
        if (isLoadingPage.value) return
        isLoadingPage.value = true
        try {
            const offset = reset ? 0 : pageOffset.value
            const keyword = searchKeyword.value.trim()
            const category = categoryFilter.value === '全部' ? null : categoryFilter.value
            const response = await ClipboardService.getHistoryPage({
                offset,
                limit: pageSize.value,
                category,
                pinnedOnly: false,
                keyword: keyword || null,
                sortBy: sortBy.value,
                sortOrder: sortOrder.value
            })
            const items = Array.isArray(response?.items) ? response.items : []
            mergePageItems(items, reset)
            totalCount.value = Number.isFinite(response?.total) ? response.total : pagedHistory.value.length
            const nextOffset = offset + items.length
            pageOffset.value = nextOffset
            hasMore.value = nextOffset < totalCount.value
            rebuildHistoryArray()
            if (pagedHistory.value.length === 0) {
                selectedIndex.value = -1
            } else if (!pagedHistory.value.some((entry) => entry.position === selectedIndex.value)) {
                selectedIndex.value = pagedHistory.value[0].position
            }
        } catch (error) {
            console.error('加载分页历史失败:', error)
        } finally {
            isLoadingPage.value = false
        }
    }

    const resetAndReloadHistory = async () => {
        pageOffset.value = 0
        totalCount.value = 0
        hasMore.value = false
        await loadHistoryPage({reset: true})
    }

    const loadMoreHistory = async () => {
        if (!hasMore.value || isLoadingPage.value) return
        await loadHistoryPage({reset: false})
    }

    const setSort = async (_nextSortBy, nextSortOrder) => {
        sortBy.value = 'pinnedFirst'
        sortOrder.value = nextSortOrder || 'asc'
        const {pinned, unpinned} = buildSortedGroups()
        applyGroupedEntries(pinned, unpinned)
        rebuildHistoryArray()
    }

    const setPageSize = async (nextPageSize) => {
        const parsed = Number(nextPageSize)
        const normalized = [10, 30, 50].includes(parsed) ? parsed : 50
        if (pageSize.value === normalized) return
        pageSize.value = normalized
        await resetAndReloadHistory()
    }

    const deleteItem = async (index, item = '') => {
        const localIndex = pagedHistory.value.findIndex(
            (entry) => entry.position === index || (!!item && entry.content === item)
        )
        let removedEntry = null
        if (localIndex >= 0) {
            removedEntry = pagedHistory.value[localIndex]
            pagedHistory.value.splice(localIndex, 1)
            const {pinned, unpinned} = buildSortedGroups()
            applyGroupedEntries(pinned, unpinned)
            totalCount.value = Math.max(0, (Number.isFinite(totalCount.value) ? totalCount.value : pagedHistory.value.length + 1) - 1)
            pageOffset.value = Math.max(0, Math.min(pageOffset.value, totalCount.value))
            hasMore.value = pageOffset.value < totalCount.value
            rebuildHistoryArray()
            if (pagedHistory.value.length === 0) {
                selectedIndex.value = -1
            } else if (!pagedHistory.value.some((entry) => entry.position === selectedIndex.value)) {
                selectedIndex.value = pagedHistory.value[0].position
            }
        }
        try {
            const removedItem = item || removedEntry?.content || history.value[index]
            if (removedItem && categoryMap.value[removedItem]) {
                delete categoryMap.value[removedItem]
                try {
                    await CategoryService.setItemCategory(removedItem, "")
                } catch (error) {
                    console.error('移除分类失败:', error)
                }
            }

            await ClipboardService.removeItem(index, removedItem || null)
        } catch (error) {
            console.error('删除失败:', error)
            await resetAndReloadHistory()
        }
    }

    const moveSelection = (direction, contentRef) => {
        const visible = visibleHistory.value
        if (visible.length === 0) return
        let visibleIndex = visible.findIndex((entry) => entry.index === selectedIndex.value)
        if (visibleIndex < 0) visibleIndex = 0
        const nextVisibleIndex = Math.max(0, Math.min(visible.length - 1, visibleIndex + direction))
        updateSelection(visible[nextVisibleIndex].index, true, contentRef, nextVisibleIndex)
    }

    const applyPayloadSnapshot = (payload = {}) => {
        const incomingHistory = Array.isArray(payload.history) ? payload.history : []
        if (incomingHistory.length === 0) {
            history.value = []
            pagedHistory.value = []
            totalCount.value = 0
            pageOffset.value = 0
            hasMore.value = false
            selectedIndex.value = -1
            return
        }
        history.value = incomingHistory.slice()
        const categoriesFromPayload = payload?.categories && typeof payload.categories === 'object'
            ? payload.categories
            : categoryMap.value
        const activeCategory = categoryFilter.value === '全部' ? null : categoryFilter.value
        const keyword = searchKeyword.value.trim().toLowerCase()
        const pinnedSet = new Set(Array.isArray(payload.pinned_items) ? payload.pinned_items : [])
        const filtered = incomingHistory
            .map((content, position) => ({
                content,
                position,
                snippet: '',
                pinned: pinnedSet.has(content),
                category: categoriesFromPayload?.[content] || '未分类'
            }))
            .filter((entry) => {
                if (activeCategory && entry.category !== activeCategory) {
                    return false
                }
                if (keyword && !entry.content.toLowerCase().includes(keyword)) {
                    return false
                }
                return true
            })
        const loadedTarget = Math.max(pageOffset.value || 0, pageSize.value)
        const sortedFiltered = sortPageItems(filtered)
        const loadedCount = Math.min(sortedFiltered.length, loadedTarget)
        pagedHistory.value = sortedFiltered.slice(0, loadedCount).map((entry) => ({
            content: entry.content,
            position: entry.position,
            snippet: entry.snippet,
            pinned: entry.pinned
        }))
        totalCount.value = sortedFiltered.length
        pageOffset.value = loadedCount
        hasMore.value = loadedCount < sortedFiltered.length
        if (pagedHistory.value.length === 0) {
            selectedIndex.value = -1
        } else if (!pagedHistory.value.some((entry) => entry.position === selectedIndex.value)) {
            selectedIndex.value = pagedHistory.value[0].position
        }
    }

    const setLocalPinnedByContent = (content, pinned) => {
        if (!content) return
        const target = pagedHistory.value.find((entry) => entry.content === content)
        if (!target) return
        const {pinned: pinnedEntries, unpinned: unpinnedEntries} = buildSortedGroups()
        const normalizedTarget = {
            ...target,
            pinned
        }
        const nextPinned = pinnedEntries.filter((entry) => entry.content !== content)
        const nextUnpinned = unpinnedEntries.filter((entry) => entry.content !== content)
        if (pinned) {
            nextPinned.unshift(normalizedTarget)
        } else {
            nextUnpinned.unshift(normalizedTarget)
        }
        applyGroupedEntries(nextPinned, nextUnpinned)
        rebuildHistoryArray()
    }

    const insertLocalIncomingContent = (content, pinned = false) => {
        if (!content) return
        const existing = pagedHistory.value.find((entry) => entry.content === content)
        const {pinned: pinnedEntries, unpinned: unpinnedEntries} = buildSortedGroups()
        const nextPinned = pinnedEntries.filter((entry) => entry.content !== content)
        const nextUnpinned = unpinnedEntries.filter((entry) => entry.content !== content)
        if (existing) {
            const normalized = {...existing, pinned}
            if (pinned) {
                nextPinned.unshift(normalized)
            } else {
                nextUnpinned.unshift(normalized)
            }
            applyGroupedEntries(nextPinned, nextUnpinned)
            rebuildHistoryArray()
            return
        }
        const incoming = {
            content,
            position: 0,
            snippet: '',
            pinned
        }
        if (pinned) {
            nextPinned.unshift(incoming)
        } else {
            nextUnpinned.unshift(incoming)
        }
        totalCount.value = (Number.isFinite(totalCount.value) ? totalCount.value : pagedHistory.value.length - 1) + 1
        applyGroupedEntries(nextPinned, nextUnpinned)
        pageOffset.value = Math.max(pageOffset.value, pagedHistory.value.length)
        hasMore.value = pageOffset.value < totalCount.value
        rebuildHistoryArray()
    }

    const promoteLocalByContent = (content) => {
        if (!content || pagedHistory.value.length < 2) return
        const target = pagedHistory.value.find((entry) => entry.content === content)
        if (!target) return
        const {pinned: pinnedEntries, unpinned: unpinnedEntries} = buildSortedGroups()
        const nextPinned = pinnedEntries.filter((entry) => entry.content !== content)
        const nextUnpinned = unpinnedEntries.filter((entry) => entry.content !== content)
        if (target.pinned) {
            nextPinned.unshift({...target, pinned: true})
        } else {
            nextUnpinned.unshift({...target, pinned: false})
        }
        applyGroupedEntries(nextPinned, nextUnpinned)
        rebuildHistoryArray()
    }

    return {
        history,
        selectedIndex,
        searchKeyword,
        categoryFilter,
        categoryMap,
        visibleHistory,
        pageSize,
        totalCount,
        hasMore,
        isLoadingPage,
        sortBy,
        sortOrder,
        getItemCategory,
        updateSelection,
        deleteItem,
        moveSelection,
        resetAndReloadHistory,
        loadMoreHistory,
        setSort,
        setPageSize,
        promoteLocalByContent,
        setLocalPinnedByContent,
        insertLocalIncomingContent,
        applyPayloadSnapshot
    }
}
