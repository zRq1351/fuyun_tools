import {computed, ref, shallowRef} from 'vue'
import {CategoryService, ClipboardService} from '../../../services/ipc'

export function useClipboardHistory() {
    const historyMap = ref({})
    const pagedHistory = shallowRef([])
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

    // 添加与图片一致的过滤机制
    const filterDataRevision = ref(0)

    // 添加分类搜索索引，与图片保持一致
    const categorySearchIndex = new Map()  // Map<category, Set<content>>
    const itemCategorySnapshot = new Map()  // Map<content, category>
    const keywordCategoryMatchCache = new Map()

    const getItemCategory = (item_id) => {
        return categoryMap.value[item_id] || '未分类'
    }

    const bumpFilterDataRevision = () => {
        filterDataRevision.value += 1
        // 清除缓存
        keywordCategoryMatchCache.clear()
    }

    // 移除分类索引
    const removeCategoryIndexForItem = (content) => {
        const oldCategory = itemCategorySnapshot.get(content)
        if (!oldCategory) {
            itemCategorySnapshot.delete(content)
            return
        }
        const contentSet = categorySearchIndex.get(oldCategory)
        if (contentSet) {
            contentSet.delete(content)
            if (contentSet.size === 0) {
                categorySearchIndex.delete(oldCategory)
            }
        }
        itemCategorySnapshot.delete(content)
    }

    // 应用分类索引
    const applyCategoryIndexForItem = (content, category) => {
        removeCategoryIndexForItem(content)
        const normalized = String(category || '未分类')
        itemCategorySnapshot.set(content, normalized)
        let contentSet = categorySearchIndex.get(normalized)
        if (!contentSet) {
            contentSet = new Set()
            categorySearchIndex.set(normalized, contentSet)
        }
        contentSet.add(content)
    }

    // 设置分类（本地），与图片的 setItemCategoryLocal 一致
    const setItemCategoryLocal = (content, category) => {
        if (!content) return
        const normalized = String(category || '未分类')
        categoryMap.value[content] = normalized
        applyCategoryIndexForItem(content, normalized)
        keywordCategoryMatchCache.clear()
    }

    // 移除分类（本地）
    const removeItemCategoryLocal = (content) => {
        if (!content) return
        delete categoryMap.value[content]
        removeCategoryIndexForItem(content)
        keywordCategoryMatchCache.clear()
    }

    // 重建分类搜索索引
    const rebuildCategorySearchIndex = () => {
        categorySearchIndex.clear()
        itemCategorySnapshot.clear()
        keywordCategoryMatchCache.clear()
        const currentCategoryMap = categoryMap.value || {}
        for (const content of Object.keys(currentCategoryMap)) {
            applyCategoryIndexForItem(content, currentCategoryMap[content] || '未分类')
        }
    }

    // 获取关键词匹配的分类ID集合
    const getKeywordCategoryMatchedIds = (keyword) => {
        if (!keyword) return null
        const cacheKey = `${filterDataRevision.value}|${keyword}`
        const cached = keywordCategoryMatchCache.get(cacheKey)
        if (cached) {
            return cached
        }
        const matchedContents = new Set()
        for (const [category, contentSet] of categorySearchIndex.entries()) {
            if (!String(category).toLowerCase().includes(keyword)) continue
            for (const content of contentSet) {
                matchedContents.add(content)
            }
        }
        keywordCategoryMatchCache.set(cacheKey, matchedContents)
        return matchedContents
    }

    // 使用 computed 实现与图片一致的即时响应过滤，并使用索引加速
    const visibleHistory = computed(() => {
        const activeCategory = categoryFilter.value === '全部' ? null : categoryFilter.value
        const keyword = searchKeyword.value.trim().toLowerCase()

        // 使用分类索引快速过滤
        const categoryFilteredContents = activeCategory
            ? (categorySearchIndex.get(activeCategory) || new Set())
            : null

        // 使用关键词索引（如果有关键词）
        const keywordMatchedContents = keyword ? getKeywordCategoryMatchedIds(keyword) : null

        return pagedHistory.value
            .filter((entry) => {
                const content = entry.content

                // 分类过滤：使用索引 O(1)
                if (categoryFilteredContents && !categoryFilteredContents.has(content)) {
                    return false
                }

                // 关键词过滤：使用索引 O(1)
                if (keywordMatchedContents && !keywordMatchedContents.has(content)) {
                    return false
                }

                return true
            })
            .map((entry) => ({
                id: entry.id,
                content: entry.content,
                index: entry.position,
                snippet: entry.snippet || ''
            }))
    })

    const updateSelection = (index, shouldScroll = false, contentRef = null, visibleIndex = null) => {
        if (index < 0) return
        selectedIndex.value = index
    }

    const rebuildHistoryArray = () => {
        if (pagedHistory.value.length === 0) {
            historyMap.value = {}
            return
        }
        const nextHistory = {}
        for (const entry of pagedHistory.value) {
            nextHistory[entry.position] = entry.content
        }
        historyMap.value = nextHistory
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
            // 重置时清空索引
            categorySearchIndex.clear()
            itemCategorySnapshot.clear()
            keywordCategoryMatchCache.clear()
            pagedHistory.value = sortPageItems(items)
            // 重建索引
            for (const item of items) {
                if (item.content) {
                    setItemCategoryLocal(item.content, item.category || '未分类')
                }
            }
            return
        }
        const map = new Map(pagedHistory.value.map((entry) => [entry.position, entry]))
        for (const item of items) {
            map.set(item.position, item)
            // 更新分类索引
            if (item.content) {
                setItemCategoryLocal(item.content, item.category || '未分类')
            }
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

    // 增量同步：保留现有数据，只更新前端没有的项（与图片的 mergeIncrementalPageIntoState 一致）
    const syncHistoryIncremental = async () => {
        if (isLoadingPage.value) return
        isLoadingPage.value = true
        try {
            const keyword = searchKeyword.value.trim()
            const category = categoryFilter.value === '全部' ? null : categoryFilter.value
            const response = await ClipboardService.getHistoryPage({
                offset: 0,
                limit: Math.max(pageSize.value, 30),
                category,
                pinnedOnly: false,
                keyword: keyword || null,
                sortBy: sortBy.value,
                sortOrder: sortOrder.value
            })
            const items = Array.isArray(response?.items) ? response.items : []

            if (items.length === 0) {
                // 没有新数据，只更新总数
                if (Number.isFinite(response?.total)) {
                    totalCount.value = Math.max(Number(response.total), pagedHistory.value.length, Number(totalCount.value) || 0)
                    pageOffset.value = pagedHistory.value.length
                    hasMore.value = pageOffset.value < totalCount.value
                }
                bumpFilterDataRevision()
                return
            }

            // 获取现有数据的快照
            const existingByContent = new Map(pagedHistory.value.map(entry => [entry.content, entry]))
            const incomingContents = new Set(items.map(item => item.content))

            // 构建新数据列表（前部）
            const front = []
            for (const item of items) {
                if (!item.content) continue
                const existing = existingByContent.get(item.content) || {}
                front.push({
                    ...existing,
                    content: item.content,
                    position: item.position ?? existing.position ?? 0,
                    snippet: item.snippet ?? existing.snippet ?? '',
                    pinned: item.pinned ?? existing.pinned ?? false,
                    category: item.category || existing.category || '未分类'
                })
                // 更新分类索引
                setItemCategoryLocal(item.content, item.category || '未分类')
            }

            // 保留不在新数据中的旧项（后部）
            const rest = []
            for (const entry of pagedHistory.value) {
                if (!incomingContents.has(entry.content)) {
                    rest.push(entry)
                }
            }

            // 合并：新数据在前，旧数据在后
            const merged = [...front, ...rest]
            pagedHistory.value = sortPageItems(merged)

            totalCount.value = Number.isFinite(response?.total)
                ? Math.max(Number(response.total), merged.length, Number(totalCount.value) || 0)
                : Math.max(totalCount.value || 0, merged.length)
            pageOffset.value = merged.length
            hasMore.value = pageOffset.value < totalCount.value
            rebuildHistoryArray()

            if (pagedHistory.value.length === 0) {
                selectedIndex.value = -1
            } else if (!pagedHistory.value.some((entry) => entry.position === selectedIndex.value)) {
                selectedIndex.value = pagedHistory.value[0].position
            }

            bumpFilterDataRevision()
        } catch (error) {
            console.error('增量同步历史失败:', error)
        } finally {
            isLoadingPage.value = false
        }
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
            const removedItemContent = item || removedEntry?.content || historyMap.value[index]
            const removedItemId = removedEntry?.id
            if (removedItemId && categoryMap.value[removedItemId]) {
                delete categoryMap.value[removedItemId]
                try {
                    await CategoryService.setItemCategory(removedItemContent, "")
                } catch (error) {
                    console.error('移除分类失败:', error)
                }
            }

            await ClipboardService.removeItem(index, removedItemContent || null)
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
            historyMap.value = {}
            pagedHistory.value = []
            totalCount.value = 0
            pageOffset.value = 0
            hasMore.value = false
            selectedIndex.value = -1
            return
        }
        const nextHistory = {}
        for (let i = 0; i < incomingHistory.length; i++) {
            nextHistory[i] = incomingHistory[i]
        }
        historyMap.value = nextHistory
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
        historyMap,
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
        syncHistoryIncremental,
        loadMoreHistory,
        setSort,
        setPageSize,
        promoteLocalByContent,
        setLocalPinnedByContent,
        insertLocalIncomingContent,
        applyPayloadSnapshot,
        bumpFilterDataRevision,
        setItemCategoryLocal,
        removeItemCategoryLocal,
        rebuildCategorySearchIndex
    }
}
