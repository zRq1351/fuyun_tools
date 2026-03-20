import {defineStore} from 'pinia'
import {ref, computed} from 'vue'
import {ClipboardService, CategoryService} from '../services/ipc'

export const useClipboardStore = defineStore('clipboard', () => {
    // 状态
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
    const categories = ref(['未分类'])
    const pinnedItems = ref([])

    // 性能监控状态
    const performanceMetrics = ref({
        loadHistoryTime: 0,
        searchTime: 0,
        filterTime: 0,
        lastUpdate: Date.now()
    })

    // 计算属性
    const visibleHistory = computed(() => {
        return pagedHistory.value.map((entry) => ({
            item: entry.content,
            index: entry.position,
            snippet: entry.snippet || ''
        }))
    })

    const selectedStatusText = computed(() => {
        const total = totalCount.value || visibleHistory.value.length
        if (total === 0) return '当前无选中项'
        const current = visibleHistory.value.findIndex((entry) => entry.index === selectedIndex.value)
        const display = current >= 0 ? current + 1 : 1
        return `当前选中：第 ${display} / ${total} 条`
    })

    const loadStatusText = computed(() => {
        if (isLoadingPage.value) return '正在加载...'
        if (hasMore.value) return `已加载 ${visibleHistory.value.length} / ${totalCount.value || visibleHistory.value.length}`
        return `已全部加载 ${visibleHistory.value.length} 条`
    })

    const keywordHitCount = computed(() => {
        const tokens = searchKeyword.value
            .trim()
            .toLowerCase()
            .split(/\s+/)
            .map((t) => t.trim())
            .filter(Boolean)
        if (tokens.length === 0) return 0
        return visibleHistory.value.filter((entry) => {
            const text = `${entry.item || ''}\n${entry.snippet || ''}`.toLowerCase()
            return tokens.some((token) => text.includes(token))
        }).length
    })

    const sortOrderText = computed(() => sortOrder.value === 'desc' ? '降序' : '升序')

    // 方法
    const getItemCategory = (item) => {
        return categoryMap.value[item] || '未分类'
    }

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
        merged.sort((a, b) => {
            const pinDiff = (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0)
            if (pinDiff !== 0) return pinDiff
            const diff = a.position - b.position
            if (a.pinned && b.pinned) {
                return diff
            }
            return sortOrder.value === 'desc' ? -diff : diff
        })
        pagedHistory.value = merged
    }

    const loadHistoryPage = async ({reset = false} = {}) => {
        if (isLoadingPage.value) return
        isLoadingPage.value = true
        const startTime = performance.now()

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

            // 记录性能指标
            performanceMetrics.value.loadHistoryTime = performance.now() - startTime
            performanceMetrics.value.lastUpdate = Date.now()
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

    const updateCategories = (newCategories) => {
        if (Array.isArray(newCategories)) {
            categories.value = newCategories
        }
    }

    const updatePinnedItems = (newPinnedItems) => {
        if (Array.isArray(newPinnedItems)) {
            pinnedItems.value = newPinnedItems
        }
    }

    const getPerformanceMetrics = () => {
        return {...performanceMetrics.value}
    }

    const resetPerformanceMetrics = () => {
        performanceMetrics.value = {
            loadHistoryTime: 0,
            searchTime: 0,
            filterTime: 0,
            lastUpdate: Date.now()
        }
    }

    return {
        // 状态
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
        categories,
        pinnedItems,
        performanceMetrics,

        // 计算属性
        selectedStatusText,
        loadStatusText,
        keywordHitCount,
        sortOrderText,

        // 方法
        getItemCategory,
        updateSelection,
        deleteItem,
        moveSelection,
        resetAndReloadHistory,
        loadMoreHistory,
        setSort,
        setPageSize,
        promoteLocalByContent,
        applyPayloadSnapshot,
        updateCategories,
        updatePinnedItems,
        getPerformanceMetrics,
        resetPerformanceMetrics
    }
})