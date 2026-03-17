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
            item: entry.content,
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

    const mergePageItems = (items, reset) => {
        if (reset) {
            pagedHistory.value = items.slice()
            return
        }
        const map = new Map(pagedHistory.value.map((entry) => [entry.position, entry]))
        for (const item of items) {
            map.set(item.position, item)
        }
        const merged = Array.from(map.values())
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
        pagedHistory.value = merged
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
        await resetAndReloadHistory()
    }

    const setPageSize = async (nextPageSize) => {
        const parsed = Number(nextPageSize)
        const normalized = [10, 30, 50].includes(parsed) ? parsed : 50
        if (pageSize.value === normalized) return
        pageSize.value = normalized
        await resetAndReloadHistory()
    }

    const deleteItem = async (index) => {
        try {
            const removedItem = history.value[index]
            history.value.splice(index, 1)
            pagedHistory.value = pagedHistory.value.filter((entry) => entry.position !== index)
            if (selectedIndex.value >= history.value.length) {
                selectedIndex.value = Math.max(0, history.value.length - 1)
            }

            if (removedItem && categoryMap.value[removedItem]) {
                delete categoryMap.value[removedItem]
                try {
                    await CategoryService.setItemCategory(removedItem, "")
                } catch (error) {
                    console.error('移除分类失败:', error)
                }
            }

            await ClipboardService.removeItem(index)
            await resetAndReloadHistory()
        } catch (error) {
            console.error('删除失败:', error)
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
        if (incomingHistory.length === 0) return
        history.value = incomingHistory.slice()
        const loadedTarget = Math.max(pageOffset.value || 0, pageSize.value)
        const loadedCount = Math.min(incomingHistory.length, loadedTarget)
        const pinnedSet = new Set(Array.isArray(payload.pinned_items) ? payload.pinned_items : [])
        pagedHistory.value = incomingHistory.slice(0, loadedCount).map((content, position) => ({
            content,
            position,
            snippet: '',
            pinned: pinnedSet.has(content)
        }))
        totalCount.value = incomingHistory.length
        pageOffset.value = loadedCount
        hasMore.value = loadedCount < incomingHistory.length
        if (selectedIndex.value < 0 || selectedIndex.value >= incomingHistory.length) {
            selectedIndex.value = incomingHistory.length > 0 ? 0 : -1
        }
    }

    const promoteLocalByContent = (content) => {
        if (!content || pagedHistory.value.length < 2) return
        const targetIndex = pagedHistory.value.findIndex((entry) => entry.content === content)
        if (targetIndex <= 0) return
        const [moved] = pagedHistory.value.splice(targetIndex, 1)
        if (!moved) return
        pagedHistory.value.unshift(moved)
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
        applyPayloadSnapshot
    }
}
