import {computed, ref, shallowRef, watch} from 'vue'
import {CategoryService, ClipboardService} from '../../../services/ipc'
import {useCategorySearchIndex} from '../../shared/useCategorySearchIndex'

export function useClipboardHistory(pinnedItems = ref([])) {
    const pagedHistory = shallowRef([])
    const selectedItemId = ref('')
    const searchKeyword = ref('')
    const categoryFilter = ref('全部')
    const categoryMap = ref({})
    let syncSequence = 0
    const pageOffset = ref(0)
    const pageSize = ref(50)
    const totalCount = ref(0)
    const hasMore = ref(false)
    const isLoadingPage = ref(false)
    const sortBy = ref('pinnedFirst')
    const sortOrder = ref('asc')
    const searchMatchedIds = ref(null)

    // Reusable sort cache to avoid redundant sorts
    let _lastSortedData = null
    let _lastSortedResult = null
    let _lastSortBy = null
    let _lastSortOrder = null

    const invalidateSortCache = () => {
        _lastSortedData = null
        _lastSortedResult = null
        _lastSortBy = null
        _lastSortOrder = null
    }

    // Invalidate sort cache when sort params change
    watch(sortBy, invalidateSortCache)
    watch(sortOrder, invalidateSortCache)

    const {
        filterDataRevision,
        categorySearchIndex,
        itemCategorySnapshot,
        keywordCategoryMatchCache,
        bumpFilterDataRevision,
        removeCategoryIndexForItem,
        applyCategoryIndexForItem,
        setItemCategoryLocal,
        removeItemCategoryLocal,
        rebuildCategorySearchIndex,
        getKeywordCategoryMatchedIds,
    } = useCategorySearchIndex(categoryMap)

    const getItemCategory = (item_id) => {
        return categoryMap.value[item_id] || '未分类'
    }


    const visibleHistory = computed(() => {
        filterDataRevision.value
        // 建立对 pinnedItems 的响应式依赖，使 v-memo 能检测到置顶状态变化
        void pinnedItems.value

        const activeCategory = categoryFilter.value === '全部' ? null : categoryFilter.value
        const keyword = searchKeyword.value.trim().toLowerCase()

        const categoryFilteredIds = activeCategory
            ? (categorySearchIndex.get(activeCategory) || new Set())
            : null

        const contentMatchedIds = searchMatchedIds.value
        const keywordCategoryIds = (!contentMatchedIds && keyword) ? getKeywordCategoryMatchedIds(keyword) : null

        return pagedHistory.value
            .filter((entry) => {
                const id = entry.id

                if (categoryFilteredIds && !categoryFilteredIds.has(id)) {
                    return false
                }

                if (contentMatchedIds && !contentMatchedIds.has(id)) {
                    return false
                }

                if (keywordCategoryIds && !keywordCategoryIds.has(id)) {
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

    const sortPageItems = (entries) => {
        // Skip sort if same array reference and same sort params
        if (entries === _lastSortedData && sortBy.value === _lastSortBy && sortOrder.value === _lastSortOrder) {
            return _lastSortedResult
        }
        const merged = entries.slice()
        const orderKey = sortBy.value
        const order = sortOrder.value
        merged.sort((a, b) => {
            if (orderKey === 'pinnedFirst') {
                const pinDiff = (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0)
                if (pinDiff !== 0) return pinDiff
                return a.position - b.position
            }
            if (orderKey === 'updatedAt') {
                const diff = (b.updatedAt || 0) - (a.updatedAt || 0)
                if (diff !== 0) return order === 'asc' ? -diff : diff
                return a.position - b.position
            }
            return a.position - b.position
        })
        _lastSortedData = entries
        _lastSortedResult = merged
        _lastSortBy = sortBy.value
        _lastSortOrder = order
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
            categorySearchIndex.clear()
            itemCategorySnapshot.clear()
            keywordCategoryMatchCache.clear()

            const resetItems = items.map((item, index) => ({
                ...item,
                position: index
            }))

            pagedHistory.value = sortPageItems(resetItems)

            // Update positions after sort
            for (let i = 0; i < pagedHistory.value.length; i++) {
                pagedHistory.value[i].position = i
            }

            for (const item of items) {
                if (item.id) {
                    setItemCategoryLocal(item.id, item.category || '未分类')
                }
            }
            return
        }

        // Build lookup for existing items
        const existingIds = new Set(pagedHistory.value.map(entry => entry.id))
        const newItems = []

        for (const item of items) {
            if (!existingIds.has(item.id)) {
                newItems.push({...item, position: 0})
            }
            if (item.id) {
                // Only update category if backend provides explicit category
                // Avoid overwriting frontend-set categories during incremental sync
                if (item.category) {
                    setItemCategoryLocal(item.id, item.category)
                } else if (!categoryMap.value[item.id]) {
                    setItemCategoryLocal(item.id, '未分类')
                }
            }
        }

        if (newItems.length === 0) return

        const merged = [...pagedHistory.value, ...newItems]
        for (let i = 0; i < merged.length; i++) {
            merged[i].position = i
        }

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

            // Update selection
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
        searchMatchedIds.value = null
        await loadHistoryPage({reset: true})
    }


    const syncHistoryIncremental = async () => {
        if (isLoadingPage.value) return
        isLoadingPage.value = true
        const currentSequence = ++syncSequence
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

            // If a newer sync was triggered, discard this result
            if (currentSequence !== syncSequence) return

            const items = Array.isArray(response?.items) ? response.items : []

            if (keyword) {
                const matchedIds = new Set(items.map(item => item.id).filter(Boolean))
                searchMatchedIds.value = matchedIds
            } else {
                searchMatchedIds.value = null
            }

            if (items.length === 0) {
                if (Number.isFinite(response?.total)) {
                    totalCount.value = Math.max(Number(response.total), getActiveCategoryCount())
                    pageOffset.value = pagedHistory.value.length
                    hasMore.value = getActiveCategoryCount() < totalCount.value
                }
                bumpFilterDataRevision()
                return
            }

            // Build lookup structures once
            const existingById = new Map(pagedHistory.value.map(entry => [entry.id, entry]))
            const incomingIds = new Set(items.map(item => item.id).filter(Boolean))
            const hasKeyword = keyword.length > 0

            // Merge incoming items with existing data
            const front = []
            for (const item of items) {
                if (!item.id) continue
                const existing = existingById.get(item.id)
                front.push({
                    ...(existing || {}),
                    id: item.id,
                    content: item.content,
                    position: item.position ?? (existing?.position ?? 0),
                    snippet: hasKeyword ? (item.snippet ?? existing?.snippet ?? '') : '',
                    pinned: item.pinned ?? existing?.pinned ?? false,
                    category: item.category || existing?.category || '未分类'
                })

                // Only update category if backend provides explicit category
                // Avoid overwriting frontend-set categories during incremental sync
                if (item.category) {
                    setItemCategoryLocal(item.id, item.category)
                } else if (!categoryMap.value[item.id]) {
                    setItemCategoryLocal(item.id, '未分类')
                }
            }

            // Keep items not in incoming set
            const rest = hasKeyword
                ? pagedHistory.value.filter(entry => !incomingIds.has(entry.id))
                : pagedHistory.value.filter(entry => !incomingIds.has(entry.id))

            const merged = [...front, ...rest]

            // Update positions in-place
            for (let i = 0; i < merged.length; i++) {
                merged[i].position = i
            }

            invalidateSortCache()
            pagedHistory.value = sortPageItems(merged)

            totalCount.value = Number.isFinite(response?.total)
                ? Math.max(Number(response.total), getActiveCategoryCount())
                : Math.max(totalCount.value || 0, getActiveCategoryCount())
            pageOffset.value = pagedHistory.value.length
            hasMore.value = getActiveCategoryCount() < totalCount.value

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
            hasMore.value = getActiveCategoryCount() < totalCount.value

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
        if (!itemId || isLoadingPage.value) return

        const localIndex = pagedHistory.value.findIndex(
            (entry) => entry.id === itemId
        )

        if (localIndex >= 0) {
            // Remove item from array
            const newArr = [...pagedHistory.value]
            newArr.splice(localIndex, 1)
            pagedHistory.value = newArr

            // Rebuild sorted groups and apply
            const {pinned, unpinned} = buildSortedGroups()
            applyGroupedEntries(pinned, unpinned)

            // Update counts
            totalCount.value = Math.max(0, (Number.isFinite(totalCount.value) ? totalCount.value : getActiveCategoryCount() + 1) - 1)
            pageOffset.value = pagedHistory.value.length
            hasMore.value = getActiveCategoryCount() < totalCount.value

            // Update selection
            if (pagedHistory.value.length === 0) {
                selectedItemId.value = ''
            } else if (!pagedHistory.value.some((entry) => entry.id === selectedItemId.value)) {
                selectedItemId.value = pagedHistory.value[0].id
            }
        }

        try {
            // Clean up category if exists
            if (categoryMap.value[itemId]) {
                removeItemCategoryLocal(itemId)
                try {
                    await CategoryService.setItemCategory(itemId, "")
                } catch (error) {
                    console.error('移除分类失败:', error)
                    // Don't rollback category index since item is being deleted
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

        categorySearchIndex.clear()
        itemCategorySnapshot.clear()
        keywordCategoryMatchCache.clear()

        const activeCategory = categoryFilter.value === '全部' ? null : categoryFilter.value
        const keyword = searchKeyword.value.trim().toLowerCase()
        const pinnedSet = new Set(Array.isArray(payload.pinned_items) ? payload.pinned_items : [])
        const hasCategoryFilter = activeCategory !== null
        const hasKeyword = keyword.length > 0

        // Build filtered list in single pass
        const filtered = []
        for (let i = 0; i < incomingHistory.length; i++) {
            const item = incomingHistory[i]
            const category = categoriesFromPayload?.[item.id] || '未分类'
            setItemCategoryLocal(item.id, category)

            // Apply filters
            if (hasCategoryFilter && category !== activeCategory) continue
            if (hasKeyword && !item.content.toLowerCase().includes(keyword)) continue

            filtered.push({
                id: item.id,
                content: item.content,
                position: i,
                snippet: '',
                pinned: pinnedSet.has(item.id),
                category
            })
        }

        const loadedTarget = Math.max(pageOffset.value || 0, pageSize.value)
        invalidateSortCache()

        const sortedFiltered = sortPageItems(filtered)
        const loadedCount = Math.min(sortedFiltered.length, loadedTarget)

        // Create sliced array with minimal copying
        pagedHistory.value = sortedFiltered.slice(0, loadedCount)

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
        const normalizedTarget = {...target, pinned}

        // Filter out the target from both lists
        const nextPinned = pinnedEntries.filter((entry) => entry.id !== id)
        const nextUnpinned = unpinnedEntries.filter((entry) => entry.id !== id)

        if (pinned) {
            nextPinned.unshift(normalizedTarget)
        } else {
            nextUnpinned.unshift(normalizedTarget)
        }

        applyGroupedEntries(nextPinned, nextUnpinned)
    }

    const insertLocalIncomingContent = (content, id, pinned = false) => {
        if (!content || !id) return

        const existing = pagedHistory.value.find((entry) => entry.id === id)
        const {pinned: pinnedEntries, unpinned: unpinnedEntries} = buildSortedGroups()

        // Remove target from both lists
        const nextPinned = pinnedEntries.filter((entry) => entry.id !== id)
        const nextUnpinned = unpinnedEntries.filter((entry) => entry.id !== id)

        if (existing) {
            // Update existing item
            const normalized = {...existing, content, pinned}
            if (pinned) {
                nextPinned.unshift(normalized)
            } else {
                nextUnpinned.unshift(normalized)
            }
            applyGroupedEntries(nextPinned, nextUnpinned)
            setItemCategoryLocal(id, existing.category || '未分类')
        } else {
            // Insert new item
            const incoming = {id, content, position: 0, snippet: '', pinned}
            if (pinned) {
                nextPinned.unshift(incoming)
            } else {
                nextUnpinned.unshift(incoming)
            }
            totalCount.value = (Number.isFinite(totalCount.value) ? totalCount.value : getActiveCategoryCount() - 1) + 1
            applyGroupedEntries(nextPinned, nextUnpinned)
            pageOffset.value = pagedHistory.value.length
            hasMore.value = getActiveCategoryCount() < totalCount.value
            setItemCategoryLocal(id, '未分类')
        }

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

    }

    const replaceLocalById = (oldId, newId, newContent) => {
        const index = pagedHistory.value.findIndex(item => item.id === oldId)
        if (index === -1) return

        const item = pagedHistory.value[index]

        // Migrate category from old ID to new ID
        const oldCategory = categoryMap.value[oldId] || itemCategorySnapshot.get(oldId) || '未分类'
        removeItemCategoryLocal(oldId)
        setItemCategoryLocal(newId, oldCategory)

        // Replace item in array
        const newArr = [...pagedHistory.value]
        newArr.splice(index, 1, {...item, id: newId, content: newContent})
        pagedHistory.value = newArr

        // Update selection if needed
        if (selectedItemId.value === oldId) {
            selectedItemId.value = newId
        }

        bumpFilterDataRevision()
    }

    return {
        replaceLocalById,
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
