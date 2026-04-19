import {computed, ref, shallowRef} from 'vue'
import {CategoryService, ClipboardService} from '../../../services/ipc'

export function useClipboardHistory() {
        const pagedHistory = shallowRef([])
    const selectedItemId = ref('')
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
    const categorySearchIndex = new Map()  // Map<category, Set<id>>
    const itemCategorySnapshot = new Map()  // Map<id, category>
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
    const removeCategoryIndexForItem = (id) => {
        const oldCategory = itemCategorySnapshot.get(id)
        if (!oldCategory) {
            itemCategorySnapshot.delete(id)
            return
        }
        const contentSet = categorySearchIndex.get(oldCategory)
        if (contentSet) {
            contentSet.delete(id)
            if (contentSet.size === 0) {
                categorySearchIndex.delete(oldCategory)
            }
        }
        itemCategorySnapshot.delete(id)
    }

    // 应用分类索引
    const applyCategoryIndexForItem = (id, category) => {
        removeCategoryIndexForItem(id)
        const normalized = String(category || '未分类')
        itemCategorySnapshot.set(id, normalized)
        let contentSet = categorySearchIndex.get(normalized)
        if (!contentSet) {
            contentSet = new Set()
            categorySearchIndex.set(normalized, contentSet)
        }
        contentSet.add(id)
    }

    // 设置分类（本地），与图片的 setItemCategoryLocal 一致
    const setItemCategoryLocal = (id, category) => {
        if (!id) return
        const normalized = String(category || '未分类')
        categoryMap.value[id] = normalized
        applyCategoryIndexForItem(id, normalized)
        keywordCategoryMatchCache.clear()
    }

    // 移除分类（本地）
    const removeItemCategoryLocal = (id) => {
        if (!id) return
        delete categoryMap.value[id]
        removeCategoryIndexForItem(id)
        keywordCategoryMatchCache.clear()
    }

    // 重建分类搜索索引
    const rebuildCategorySearchIndex = () => {
        categorySearchIndex.clear()
        itemCategorySnapshot.clear()
        keywordCategoryMatchCache.clear()
        const currentCategoryMap = categoryMap.value || {}
        for (const id of Object.keys(currentCategoryMap)) {
            applyCategoryIndexForItem(id, currentCategoryMap[id] || '未分类')
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
        const matchedIds = new Set()
        for (const [category, idSet] of categorySearchIndex.entries()) {
            if (!String(category).toLowerCase().includes(keyword)) continue
            for (const id of idSet) {
                matchedIds.add(id)
            }
        }
        keywordCategoryMatchCache.set(cacheKey, matchedIds)
        return matchedIds
    }

    // 使用 computed 实现与图片一致的即时响应过滤，并使用索引加速
    const visibleHistory = computed(() => {
        // 访问 filterDataRevision 触发响应式更新
        // eslint-disable-next-line no-unused-expressions
        filterDataRevision.value
        
        const activeCategory = categoryFilter.value === '全部' ? null : categoryFilter.value
        const keyword = searchKeyword.value.trim().toLowerCase()

        // 使用分类索引快速过滤
        const categoryFilteredIds = activeCategory
            ? (categorySearchIndex.get(activeCategory) || new Set())
            : null

        // 使用关键词索引（如果有关键词）
        const keywordMatchedIds = keyword ? getKeywordCategoryMatchedIds(keyword) : null

        return pagedHistory.value
            .filter((entry) => {
                const id = entry.id
                const content = entry.content

                // 分类过滤：使用索引 O(1)
                if (categoryFilteredIds && !categoryFilteredIds.has(id)) {
                    return false
                }

                // 关键词过滤：使用索引 O(1)
                if (keywordMatchedIds && !keywordMatchedIds.has(id)) {
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

    const updateSelection = (itemId, shouldScroll = false, contentRef = null, visibleIndex = null) => {
        if (!itemId) return
        selectedItemId.value = itemId
    }

    const rebuildHistoryArray = () => {
    }

    const sortPageItems = (entries) => {
        const merged = entries.slice()
        const orderKey = sortBy.value
        merged.sort((a, b) => {
            if (orderKey === 'pinnedFirst') {
                const pinDiff = (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0)
                if (pinDiff !== 0) return pinDiff
                const diff = a.position - b.position
                if (a.pinned && b.pinned) {
                    return diff
                }
                return diff // position 已经是全局正确排序的数字，不用管 sortOrder.value 因为拉取的时候已经排序好了
            }
            if (orderKey === 'updatedAt') {
                const diff = (b.updatedAt || 0) - (a.updatedAt || 0) // 默认 updatedAt desc (新更新的在前)
                if (diff !== 0) return sortOrder.value === 'asc' ? -diff : diff
                return a.position - b.position
            }
            return a.position - b.position
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
            
            // 为了防止乱序，强制使用从0开始的连续 position 覆盖服务端给的可能是不连续的 position
            const resetItems = items.map((item, index) => ({
                ...item,
                position: index
            }));
            
            pagedHistory.value = sortPageItems(resetItems)
            // 校准 reset 数据后的 position
            pagedHistory.value.forEach((entry, index) => {
                entry.position = index;
            });
            // 重建索引
            for (const item of items) {
                if (item.id) {
                    setItemCategoryLocal(item.id, item.category || '未分类')
                }
            }
            return
        }
        
        const existingIds = new Set(pagedHistory.value.map(entry => entry.id))
        const newItems = []
        for (const item of items) {
            if (!existingIds.has(item.id)) {
                newItems.push({
                    ...item,
                    position: 0
                })
            }
            if (item.id) {
                setItemCategoryLocal(item.id, item.category || '未分类')
            }
        }
        
        const merged = [...pagedHistory.value, ...newItems]
        merged.forEach((entry, index) => {
            entry.position = index;
        });
        
        pagedHistory.value = sortPageItems(merged)
    }

    const getActiveCategoryCount = () => {
        const activeCategory = categoryFilter.value === '全部' ? null : categoryFilter.value;
        if (!activeCategory) return pagedHistory.value.length;
        let count = 0;
        for (const item of pagedHistory.value) {
            if (item.category === activeCategory || getItemCategory(item.id) === activeCategory) {
                count++;
            }
        }
        return count;
    }

    const loadHistoryPage = async ({reset = false} = {}) => {
        if (isLoadingPage.value) return
        isLoadingPage.value = true
        try {
            const offset = reset ? 0 : getActiveCategoryCount()
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
            totalCount.value = Number.isFinite(response?.total) ? response.total : getActiveCategoryCount()
            pageOffset.value = pagedHistory.value.length
            hasMore.value = getActiveCategoryCount() < totalCount.value
            rebuildHistoryArray()
            if (pagedHistory.value.length === 0) {
                selectedItemId.value = ''
            } else if (!pagedHistory.value.some((entry) => entry.id === selectedItemId.value)) {
                selectedItemId.value = pagedHistory.value[0].id
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
                    totalCount.value = Math.max(Number(response.total), getActiveCategoryCount(), Number(totalCount.value) || 0)
                    pageOffset.value = pagedHistory.value.length
                    hasMore.value = getActiveCategoryCount() < totalCount.value
                }
                bumpFilterDataRevision()
                return
            }

            // 获取现有数据的快照
            const existingById = new Map(pagedHistory.value.map(entry => [entry.id, entry]))
            const incomingIds = new Set(items.map(item => item.id))

            // 构建新数据列表（前部）
            const front = []
            for (const item of items) {
                if (!item.id) continue
                const existing = existingById.get(item.id) || {}
                front.push({
                    ...existing,
                    id: item.id,
                    content: item.content,
                    // 不应该使用 existing.position 如果 item 有自己的 position
                    position: item.position ?? existing.position ?? 0,
                    snippet: item.snippet ?? existing.snippet ?? '',
                    pinned: item.pinned ?? existing.pinned ?? false,
                    category: item.category || existing.category || '未分类'
                })
                // 更新分类索引
                setItemCategoryLocal(item.id, item.category || '未分类')
            }

            // 保留不在新数据中的旧项（后部）
            const rest = []
            for (const entry of pagedHistory.value) {
                if (!incomingIds.has(entry.id)) {
                    rest.push(entry)
                }
            }
            
            // 合并：新数据在前，旧数据在后
            const merged = [...front, ...rest]

            // 不再按照旧的 position 排序，因为不同分类拉取的数据 position 可能重叠
            // 直接保持新拉取的数据在前面，旧数据在后面的顺序，并重新赋予连续的 position
            merged.forEach((entry, index) => {
                entry.position = index;
            });

            pagedHistory.value = sortPageItems(merged);

            totalCount.value = Number.isFinite(response?.total)
                ? Math.max(Number(response.total), getActiveCategoryCount(), Number(totalCount.value) || 0)
                : Math.max(totalCount.value || 0, getActiveCategoryCount())
            pageOffset.value = pagedHistory.value.length
            hasMore.value = getActiveCategoryCount() < totalCount.value
            rebuildHistoryArray()

            if (pagedHistory.value.length === 0) {
                selectedItemId.value = ''
            } else if (!pagedHistory.value.some((entry) => entry.id === selectedItemId.value)) {
                selectedItemId.value = pagedHistory.value[0].id
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

    const loadTailPage = async () => {
        if (!hasMore.value || isLoadingPage.value) return false
        const loadedCount = getActiveCategoryCount()
        const exactTotal = Math.max(Number(totalCount.value) || 0, loadedCount)
        const targetOffset = Math.max(0, exactTotal - (Number(pageSize.value) || 10))
        if (targetOffset <= 0 && loadedCount >= exactTotal) {
            return false
        }
        
        isLoadingPage.value = true
        try {
            const keyword = searchKeyword.value.trim()
            const category = categoryFilter.value === '全部' ? null : categoryFilter.value
            const response = await ClipboardService.getHistoryPage({
                offset: targetOffset,
                limit: pageSize.value,
                category,
                pinnedOnly: false,
                keyword: keyword || null,
                sortBy: sortBy.value,
                sortOrder: sortOrder.value
            })
            const items = Array.isArray(response?.items) ? response.items : []
            mergePageItems(items, false)
            totalCount.value = Number.isFinite(response?.total) ? response.total : getActiveCategoryCount()
            pageOffset.value = pagedHistory.value.length
            hasMore.value = false
            rebuildHistoryArray()
            return true
        } catch (error) {
            console.error('加载尾页失败:', error)
            return false
        } finally {
            isLoadingPage.value = false
        }
    }

    const setPageSize = async (nextPageSize) => {
        const parsed = Number(nextPageSize)
        const normalized = [10, 30, 50].includes(parsed) ? parsed : 50
        if (pageSize.value === normalized) return
        pageSize.value = normalized
        await resetAndReloadHistory()
    }

    const deleteItem = async (itemId) => {
        if (!itemId) return
        const localIndex = pagedHistory.value.findIndex(
            (entry) => entry.id === itemId
        )
        let removedEntry = null
        if (localIndex >= 0) {
            removedEntry = pagedHistory.value[localIndex]
            pagedHistory.value.splice(localIndex, 1)
            const {pinned, unpinned} = buildSortedGroups()
            applyGroupedEntries(pinned, unpinned)
            totalCount.value = Math.max(0, (Number.isFinite(totalCount.value) ? totalCount.value : getActiveCategoryCount() + 1) - 1)
            pageOffset.value = pagedHistory.value.length
            hasMore.value = getActiveCategoryCount() < totalCount.value
            
            if (pagedHistory.value.length === 0) {
                selectedItemId.value = ''
            } else if (!pagedHistory.value.some((entry) => entry.id === selectedItemId.value)) {
                selectedItemId.value = pagedHistory.value[0].id
            }
        }
        
        try {
            if (categoryMap.value[itemId]) {
                removeItemCategoryLocal(itemId)
                try {
                    await CategoryService.setItemCategory(itemId, "")
                } catch (error) {
                    console.error('移除分类失败:', error)
                }
            }
            await ClipboardService.removeItem(itemId)
        } catch (error) {
            console.error('删除失败:', error)
            await resetAndReloadHistory()
        }
    }

    const moveSelection = (direction, contentRef) => {
        const visible = visibleHistory.value
        if (visible.length === 0) return
        let visibleIndex = visible.findIndex((entry) => entry.id === selectedItemId.value)
        if (visibleIndex < 0) visibleIndex = 0
        const nextVisibleIndex = Math.max(0, Math.min(visible.length - 1, visibleIndex + direction))
        updateSelection(visible[nextVisibleIndex].id, true, contentRef, nextVisibleIndex)
    }

    const applyPayloadSnapshot = (payload = {}) => {
        const incomingHistory = Array.isArray(payload.history) ? payload.history : []
        if (incomingHistory.length === 0) {
            pagedHistory.value = []
            totalCount.value = 0
            pageOffset.value = 0
            hasMore.value = false
            selectedItemId.value = ''
            return
        }
        
        const categoriesFromPayload = payload?.categories && typeof payload.categories === 'object'
            ? payload.categories
            : categoryMap.value
            
        // 预先建立分类索引
        categorySearchIndex.clear()
        itemCategorySnapshot.clear()
        keywordCategoryMatchCache.clear()
        
        const activeCategory = categoryFilter.value === '全部' ? null : categoryFilter.value
        const keyword = searchKeyword.value.trim().toLowerCase()
        const pinnedSet = new Set(Array.isArray(payload.pinned_items) ? payload.pinned_items : [])
        
        const filtered = incomingHistory
            .map((item, position) => {
                const category = categoriesFromPayload?.[item.id] || '未分类'
                setItemCategoryLocal(item.id, category)
                return {
                    id: item.id,
                    content: item.content,
                    position,
                    snippet: '',
                    pinned: pinnedSet.has(item.id),
                    category
                }
            })
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
            id: entry.id,
            content: entry.content,
            position: entry.position,
            snippet: entry.snippet,
            pinned: entry.pinned
        }))
        
        totalCount.value = sortedFiltered.length
        pageOffset.value = pagedHistory.value.length
        hasMore.value = getActiveCategoryCount() < sortedFiltered.length
        
        if (pagedHistory.value.length === 0) {
            selectedItemId.value = ''
        } else if (!pagedHistory.value.some((entry) => entry.id === selectedItemId.value)) {
            selectedItemId.value = pagedHistory.value[0].id
        }
        
        bumpFilterDataRevision()
    }

    const setLocalPinnedById = (id, pinned) => {
        if (!id) return
        const target = pagedHistory.value.find((entry) => entry.id === id)
        if (!target) return
        const {pinned: pinnedEntries, unpinned: unpinnedEntries} = buildSortedGroups()
        const normalizedTarget = {
            ...target,
            pinned
        }
        const nextPinned = pinnedEntries.filter((entry) => entry.id !== id)
        const nextUnpinned = unpinnedEntries.filter((entry) => entry.id !== id)
        if (pinned) {
            nextPinned.unshift(normalizedTarget)
        } else {
            nextUnpinned.unshift(normalizedTarget)
        }
        applyGroupedEntries(nextPinned, nextUnpinned)
        rebuildHistoryArray()
    }

    const insertLocalIncomingContent = (content, id, pinned = false) => {
        if (!content || !id) return
        const existing = pagedHistory.value.find((entry) => entry.id === id)
        const {pinned: pinnedEntries, unpinned: unpinnedEntries} = buildSortedGroups()
        const nextPinned = pinnedEntries.filter((entry) => entry.id !== id)
        const nextUnpinned = unpinnedEntries.filter((entry) => entry.id !== id)
        if (existing) {
            const normalized = {...existing, content, pinned}
            if (pinned) {
                nextPinned.unshift(normalized)
            } else {
                nextUnpinned.unshift(normalized)
            }
            applyGroupedEntries(nextPinned, nextUnpinned)
            rebuildHistoryArray()
            setItemCategoryLocal(id, existing.category || '未分类')
            bumpFilterDataRevision()
            return
        }
        const incoming = {
            id,
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
        totalCount.value = (Number.isFinite(totalCount.value) ? totalCount.value : getActiveCategoryCount() - 1) + 1
        applyGroupedEntries(nextPinned, nextUnpinned)
        pageOffset.value = pagedHistory.value.length
        hasMore.value = getActiveCategoryCount() < totalCount.value
        rebuildHistoryArray()
        setItemCategoryLocal(id, '未分类')
        bumpFilterDataRevision()
    }

    const promoteLocalById = (id) => {
        if (!id || pagedHistory.value.length < 2) return
        const target = pagedHistory.value.find((entry) => entry.id === id)
        if (!target) return
        const {pinned: pinnedEntries, unpinned: unpinnedEntries} = buildSortedGroups()
        const nextPinned = pinnedEntries.filter((entry) => entry.id !== id)
        const nextUnpinned = unpinnedEntries.filter((entry) => entry.id !== id)
        if (target.pinned) {
            nextPinned.unshift({...target, pinned: true})
        } else {
            nextUnpinned.unshift({...target, pinned: false})
        }
        applyGroupedEntries(nextPinned, nextUnpinned)
        rebuildHistoryArray()
    }

    return {
                selectedItemId,
        searchKeyword,
        categoryFilter,
        categoryMap,
        visibleHistory,
        pageSize,
        totalCount,
        hasMore,
        isLoadingPage,
        sortBy,
        getItemCategory,
        updateSelection,
        deleteItem,
        moveSelection,
        resetAndReloadHistory,
        syncHistoryIncremental,
        loadMoreHistory,
        loadTailPage,
        setPageSize,
        promoteLocalById,
        setLocalPinnedById,
        insertLocalIncomingContent,
        applyPayloadSnapshot,
        bumpFilterDataRevision,
        setItemCategoryLocal,
        removeItemCategoryLocal,
        rebuildCategorySearchIndex
    }
}
