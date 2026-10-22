// 图片剪贴板历史列表 composable
// 职责：图片历史数据、分页/滚动加载（含预取与尾页）、增量/全量同步、
// 整页/增量合并、删除、置顶/取消置顶，以及基于分类/标签索引与关键词的
// 过滤视图（filteredHistory 等）和每页条数切换。
// 分类/标签索引、过滤缓存、预览缓存等跨域共用能力由 App.vue 以参数注入，
// 本 composable 只负责列表数据与分页状态机，不反向依赖其他 composable。
import {computed, nextTick, onBeforeUnmount, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage} from 'element-plus'
import {ImageClipboardService} from '../../../services/ipc'
import {applyImagePageToHistory, removeImageHistoryItem} from '../historyListUtils'

export function useImageHistory({
    categoryMap,
    tagMap,
    categories,
    isAddingCategory,
    warmedIndices,
    warmingIndices,
    previewCache,
    asyncPreviewCache,
    tagSearchIndex,
    itemTagSnapshot,
    categorySearchIndex,
    itemCategorySnapshot,
    keywordTagMatchCache,
    keywordCategoryMatchCache,
    filterDataRevision,
    filterEntriesCache,
    getKeywordTagMatchedIds,
    getKeywordCategoryMatchedIds,
    getItemCategory,
    getItemTags,
    getLoadedHistoryCount,
    bumpFilterDataRevision,
    rebuildFilterIndexes,
    setItemCategoryLocal,
    setItemTagsLocal,
    removeItemCategoryLocal,
    removeItemTagsLocal
}) {
    const {t} = useI18n()
    const history = ref([])
    const pinnedItems = ref([])
    const categoryFilter = ref('全部')
    const selectedIndex = ref(0)
    const searchKeyword = ref('')
    const totalCount = ref(0)
    const pageOffset = ref(0)
    const pageSize = ref(10)
    const hasMore = ref(false)
    const isLoadingPage = ref(false)
    const sortBy = ref('pinnedFirst')
    const sortOrder = ref('asc')
    const isVisible = ref(false)
    let pendingHistorySync = false
    let historyUpdateTimer = null
    let initialPageRetryTimer = null
    const loadMoreIntent = ref(false)
    const isReorderMode = ref(false)
    const prefetchedPage = ref(null)
    const isPrefetchingPage = ref(false)
    let prefetchRequestSeq = 0
    let prefetchPromise = null

    const getLoadedHistorySnapshot = () => {
        const loadedItems = []
        const existingById = new Map()
        for (const item of history.value) {
            if (!item?.id) continue
            loadedItems.push(item)
            existingById.set(item.id, item)
        }
        return {loadedItems, existingById}
    }

    const currentPageQuerySignature = () =>
        JSON.stringify({
            pageSize: pageSize.value,
            category: categoryFilter.value === '全部' ? null : categoryFilter.value,
            keyword: searchKeyword.value.trim() || null,
            sortBy: sortBy.value,
            sortOrder: sortOrder.value
        })

    const clearPrefetchedPage = () => {
        prefetchRequestSeq += 1
        prefetchedPage.value = null
        isPrefetchingPage.value = false
        prefetchPromise = null
    }

    const getHistoryRetentionLimit = (...candidateCounts) => {
        const normalizedPageSize = normalizeImagePageSize(pageSize.value)
        const loadedCount = Math.max(0, Number(pageOffset.value) || 0)
        const normalizedCandidates = candidateCounts
            .map((value) => Number(value))
            .filter((value) => Number.isFinite(value))
            .map((value) => Math.max(0, value))
        return Math.max(100, normalizedPageSize * 4, loadedCount, ...normalizedCandidates)
    }

    const prefetchNextPage = async () => {
        if (isLoadingPage.value || isPrefetchingPage.value || !hasMore.value) return
        const offset = pageOffset.value
        const signature = currentPageQuerySignature()
        if (
            prefetchedPage.value
            && prefetchedPage.value.offset === offset
            && prefetchedPage.value.signature === signature
        ) {
            return
        }
        const requestSeq = ++prefetchRequestSeq
        isPrefetchingPage.value = true
        prefetchPromise = (async () => {
            try {
                const data = await ImageClipboardService.getHistoryPage({
                    offset,
                    limit: pageSize.value,
                    category: categoryFilter.value === '全部' ? null : categoryFilter.value,
                    keyword: searchKeyword.value.trim() || null,
                    pinnedOnly: false,
                    sortBy: sortBy.value,
                    sortOrder: sortOrder.value
                })
                if (requestSeq !== prefetchRequestSeq) return
                prefetchedPage.value = {
                    offset,
                    signature,
                    data
                }
            } catch (_) {
                if (requestSeq === prefetchRequestSeq) {
                    prefetchedPage.value = null
                }
            } finally {
                if (requestSeq === prefetchRequestSeq) {
                    isPrefetchingPage.value = false
                    prefetchPromise = null
                }
            }
        })()
        await prefetchPromise
    }

    const flushPendingHistorySync = () => {
        if (pendingHistorySync) {
            pendingHistorySync = false
            scheduleHistorySync(0)
        }
    }

    const handleLoadMoreIntent = () => {
        if (!hasMore.value || isLoadingPage.value) return
        loadMoreIntent.value = true
        void tryLoadMoreByScroll()
    }

    const tryLoadMoreByScroll = async () => {
        if (!hasMore.value || isLoadingPage.value) return false
        await loadHistoryPage({reset: false})
        return true
    }

    const loadTailPage = async () => {
        if (!hasMore.value || isLoadingPage.value) return false
        const loadedCount = getLoadedHistoryCount()
        const exactTotal = Math.max(Number(totalCount.value) || 0, loadedCount)
        const targetOffset = Math.max(0, exactTotal - (Number(pageSize.value) || 10))
        if (targetOffset <= 0 && loadedCount >= exactTotal) {
            return false
        }
        clearPrefetchedPage()
        const data = await ImageClipboardService.getHistoryPage({
            offset: targetOffset,
            limit: pageSize.value,
            category: categoryFilter.value === '全部' ? null : categoryFilter.value,
            keyword: searchKeyword.value.trim() || null,
            pinnedOnly: false,
            sortBy: sortBy.value,
            sortOrder: sortOrder.value
        })
        mergeImagePageIntoState(data, false)
        return true
    }

    const pinnedItemSet = computed(() => new Set(pinnedItems.value))
    const isPinned = (itemId) => pinnedItemSet.value.has(itemId)

    const filteredHistoryState = computed(() => {
        const revision = filterDataRevision.value
        const category = categoryFilter.value
        const keyword = searchKeyword.value.trim().toLowerCase()
        const cacheKey = `${revision}|${category}|${keyword}`
        let cached = filterEntriesCache.get(cacheKey)
        if (!cached) {
            const out = []
            const displayIndexMap = new Map()
            const categoryFilteredIds = category === '全部' ? null : (categorySearchIndex.get(category) || new Set())
            const tagMatchedIds = keyword ? getKeywordTagMatchedIds(keyword) : null
            const categoryMatchedIds = keyword ? getKeywordCategoryMatchedIds(keyword) : null
            const currentPinnedSet = pinnedItemSet.value
            for (let index = 0; index < history.value.length; index++) {
                const item = history.value[index]
                if (!item) continue
                const itemId = item.id
                if (categoryFilteredIds && !categoryFilteredIds.has(itemId)) {
                    continue
                }
                if (keyword) {
                    const categoryMatched = categoryMatchedIds && categoryMatchedIds.has(itemId)
                    const tagMatched = tagMatchedIds && tagMatchedIds.has(itemId)
                    if (!categoryMatched && !tagMatched) {
                        continue
                    }
                }
                const nextDisplay = out.length + 1
                out.push({
                    item,
                    index,
                    pinned: currentPinnedSet.has(itemId),
                    category: getItemCategory(itemId),
                    tags: getItemTags(itemId)
                })
                displayIndexMap.set(index, nextDisplay)
            }
            cached = {
                entries: out,
                total: out.length,
                displayIndexMap
            }
            filterEntriesCache.set(cacheKey, cached)
        }
        const selectedDisplay = cached.displayIndexMap.get(selectedIndex.value) || 1
        return {
            entries: cached.entries,
            total: cached.total,
            selectedDisplay
        }
    })

    const filteredHistory = computed(() => filteredHistoryState.value.entries)

    // 列表组件用显示位判断选中：过滤后需把历史下标换算为过滤列表中的位置
    const selectedDisplayIndex = computed(() => {
        const idx = filteredHistory.value.findIndex((e) => e.index === selectedIndex.value)
        return idx < 0 ? -1 : idx
    })

    const selectedStatusText = computed(() => {
        const total = totalCount.value || filteredHistoryState.value.total
        if (total === 0) return t('imageClipboard.noSelection')
        const display = filteredHistoryState.value.selectedDisplay
        return t('imageClipboard.selected', {display, total})
    })

    const loadStatusText = computed(() => {
        if (isLoadingPage.value) return t('common.loading')
        const loaded = filteredHistoryState.value.total
        const total = totalCount.value || loaded
        if (hasMore.value) return t('imageClipboard.loaded', {loaded, total})
        return t('imageClipboard.allLoaded', {total: loaded})
    })

    const IMAGE_PAGE_SIZE_OPTIONS = [10, 30, 50]
    const normalizeImagePageSize = (value) => {
        const parsed = Number(value)
        return IMAGE_PAGE_SIZE_OPTIONS.includes(parsed) ? parsed : 10
    }

    const cyclePageSize = async () => {
        const current = normalizeImagePageSize(pageSize.value)
        const index = IMAGE_PAGE_SIZE_OPTIONS.indexOf(current)
        const next = IMAGE_PAGE_SIZE_OPTIONS[(index + 1) % IMAGE_PAGE_SIZE_OPTIONS.length]
        pageSize.value = next
        localStorage.setItem('image_history_page_size', String(next))
        clearPrefetchedPage()
        await syncHistory()
    }

    const toggleReorderMode = () => {
        isReorderMode.value = !isReorderMode.value
        if (!isReorderMode.value) {
            saveReorder()
        }
    }

    let reorderedEntries = null

    const handleReorder = (newList) => {
        reorderedEntries = newList
    }

    const saveReorder = async () => {
        try {
            const itemIds = (reorderedEntries || filteredHistory.value).map(item => item.item.id)
            await ImageClipboardService.reorderItems(itemIds)
        } catch (error) {
            console.error('保存排序失败:', error)
        }
    }

    const mergeIncrementalImageItem = (rawItem) => {
        if (!rawItem?.id) return
        const selectedId = history.value[selectedIndex.value]?.id
        const pinnedSet = new Set(pinnedItems.value)
        let existingIndex = -1
        let firstUnpinnedIndex = -1
        for (let i = 0; i < history.value.length; i++) {
            const item = history.value[i]
            if (!item) continue
            if (existingIndex < 0 && item.id === rawItem.id) {
                existingIndex = i
            }
            if (firstUnpinnedIndex < 0 && !pinnedSet.has(item.id)) {
                firstUnpinnedIndex = i
            }
            if (existingIndex >= 0 && firstUnpinnedIndex >= 0) {
                break
            }
        }
        const existing = existingIndex >= 0 ? history.value[existingIndex] : null
        if (existingIndex >= 0) {
            history.value.splice(existingIndex, 1)
            if (firstUnpinnedIndex > existingIndex) {
                firstUnpinnedIndex -= 1
            }
        }
        const normalized = {
            id: rawItem.id,
            width: rawItem.width ?? existing?.width ?? 0,
            height: rawItem.height ?? existing?.height ?? 0,
            preview_png_base64: rawItem.preview_png_base64 ?? rawItem.previewPngBase64 ?? existing?.preview_png_base64 ?? '',
            image_path: rawItem.image_path ?? rawItem.imagePath ?? existing?.image_path ?? ''
        }
        const isPinnedItem = pinnedSet.has(normalized.id)
        const insertIndex = isPinnedItem ? 0 : (firstUnpinnedIndex >= 0 ? firstUnpinnedIndex : history.value.length)
        history.value.splice(insertIndex, 0, normalized)
        const keepCount = getHistoryRetentionLimit(
            (Number(pageOffset.value) || 0) + (existingIndex >= 0 ? 0 : 1)
        )
        if (history.value.length > keepCount) {
            history.value = history.value.slice(0, keepCount)
            // 清理预热缓存中已不在可见列表中的条目
            const visibleIds = new Set(history.value.map((h) => h.id))
            for (const id of warmedIndices) {
                if (!visibleIds.has(id)) warmedIndices.delete(id)
            }
            for (const id of warmingIndices) {
                if (!visibleIds.has(id)) warmingIndices.delete(id)
            }
        }
        let loadedCount = 0
        let nextSelectedIndex = -1
        for (let i = 0; i < history.value.length; i++) {
            const item = history.value[i]
            if (!item) continue
            loadedCount += 1
            if (selectedId && nextSelectedIndex < 0 && item.id === selectedId) {
                nextSelectedIndex = i
            }
        }
        totalCount.value = Math.max(totalCount.value || 0, loadedCount)
        pageOffset.value = Math.max(pageOffset.value || 0, loadedCount)
        if (selectedId) {
            selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : (history.value.length > 0 ? 0 : -1)
        } else if (selectedIndex.value < 0) {
            selectedIndex.value = history.value.length > 0 ? 0 : -1
        }
        const wasHistoryMutated = existingIndex >= 0 || !!rawItem?.id
        if (wasHistoryMutated) {
            bumpFilterDataRevision()
        }
    }

    const promoteImageItem = async (itemId) => {
        try {
            const shouldPin = !isPinned(itemId)
            await ImageClipboardService.setItemPinned(itemId, shouldPin)


            if (shouldPin) {

                pinnedItems.value = [itemId, ...pinnedItems.value.filter(id => id !== itemId)]
                promoteLocalItemToTop(itemId)
            } else {

                pinnedItems.value = pinnedItems.value.filter(id => id !== itemId)
                demoteLocalItemFromTop(itemId)
            }


        } catch (error) {
            console.error('置顶图片失败:', error)
            ElMessage.error(t('imageClipboard.pinFailed', {error: String(error)}))

            await syncHistory()
        }
    }

    // 取消置顶：将图片从顶部移动到非置顶区域的开头
    const demoteLocalItemFromTop = (itemId) => {
        if (!itemId || !Array.isArray(history.value) || history.value.length < 2) return

        const currentIndex = history.value.findIndex((item) => item?.id === itemId)
        if (currentIndex < 0) return

        const selectedId = history.value[selectedIndex.value]?.id
        const [moved] = history.value.splice(currentIndex, 1)
        if (!moved) return


        const pinnedSet = new Set(pinnedItems.value)
        let insertIndex = history.value.findIndex((item) => !pinnedSet.has(item.id))


        if (insertIndex < 0) {
            insertIndex = history.value.length
        }

        history.value.splice(insertIndex, 0, moved)


        if (selectedId) {
            const nextSelectedIndex = history.value.findIndex((item) => item?.id === selectedId)
            selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : 0
        } else {
            selectedIndex.value = 0
        }
        bumpFilterDataRevision()
    }

    const deleteItem = async (itemId) => {
        if (!itemId) return
        const snapshot = {
            history: history.value.slice(),
            categoryMap: {...categoryMap.value},
            tagMap: Object.fromEntries(
                Object.entries(tagMap.value || {}).map(([key, value]) => [key, Array.isArray(value) ? [...value] : []])
            ),
            pinnedItems: pinnedItems.value.slice(),
            selectedIndex: selectedIndex.value,
            totalCount: totalCount.value,
            pageOffset: pageOffset.value,
            hasMore: hasMore.value,
            previewCacheValue: previewCache.get(itemId),
            asyncPreviewCacheValue: asyncPreviewCache.get(itemId)
        }
        try {
            previewCache.delete(itemId)
            asyncPreviewCache.delete(itemId)
            removeItemCategoryLocal(itemId)
            removeItemTagsLocal(itemId)
            pinnedItems.value = pinnedItems.value.filter((id) => id !== itemId)
            // 按 id 定位删除：过滤态下卡片传来的可见位不等于原始 history 下标
            const removed = removeImageHistoryItem(history.value, selectedIndex.value, itemId)
            history.value = removed.history
            selectedIndex.value = removed.selectedIndex
            bumpFilterDataRevision()
            await ImageClipboardService.removeItemById(itemId)
            await syncHistory()
        } catch (error) {
            console.error('删除图片记录失败:', error)
            history.value = snapshot.history
            categoryMap.value = snapshot.categoryMap
            tagMap.value = snapshot.tagMap
            pinnedItems.value = snapshot.pinnedItems
            selectedIndex.value = snapshot.selectedIndex
            totalCount.value = snapshot.totalCount
            pageOffset.value = snapshot.pageOffset
            hasMore.value = snapshot.hasMore
            if (snapshot.previewCacheValue !== undefined) {
                previewCache.set(itemId, snapshot.previewCacheValue)
            }
            if (snapshot.asyncPreviewCacheValue !== undefined) {
                asyncPreviewCache.set(itemId, snapshot.asyncPreviewCacheValue)
            }
            rebuildFilterIndexes()
            bumpFilterDataRevision()
            try {
                await syncHistory()
            } catch (syncError) {
                console.error('删除失败后重同步图片历史失败:', syncError)
            }
        }
    }

    const mergeShowWindowPayload = (data) => {
        clearPrefetchedPage()
        const incoming = Array.isArray(data?.history) ? data.history.filter((item) => item?.id) : []


        const hasHistoryField = Object.prototype.hasOwnProperty.call(data, 'history') && Array.isArray(data.history)
        const isFullSnapshot = hasHistoryField && (data.is_full_snapshot === true || incoming.length === 0)

        if (isFullSnapshot) {

            if (incoming.length === 0) {

                history.value = []
            } else {

                const front = []
                for (const item of incoming) {
                    if (!item?.id) continue
                    front.push({
                        id: item.id,
                        width: item.width ?? 0,
                        height: item.height ?? 0,
                        preview_png_base64: item.preview_png_base64 ?? item.previewPngBase64 ?? '',
                        image_path: item.image_path ?? item.imagePath ?? ''
                    })
                }
                history.value = front
            }

            const loadedCount = history.value.length
            totalCount.value = loadedCount
            pageOffset.value = loadedCount
        } else if (incoming.length > 0) {

            const {loadedItems, existingById} = getLoadedHistorySnapshot()
            const incomingIds = new Set()

            const keepCount = getHistoryRetentionLimit(incoming.length)

            const front = []
            for (const item of incoming) {
                if (!item?.id) continue
                incomingIds.add(item.id)
                const existing = existingById.get(item.id) || {}
                front.push({
                    ...existing,
                    id: item.id,
                    width: item.width ?? existing.width ?? 0,
                    height: item.height ?? existing.height ?? 0,
                    preview_png_base64: item.preview_png_base64 ?? item.previewPngBase64 ?? existing.preview_png_base64 ?? '',
                    image_path: item.image_path ?? item.imagePath ?? existing.image_path ?? ''
                })
            }

            const rest = []
            for (const item of loadedItems) {
                if (!incomingIds.has(item.id)) {
                    rest.push(item)
                }
            }

            const nextHistory = [...front, ...rest].slice(0, keepCount)
            history.value = nextHistory

            const loadedCount = history.value.length
            totalCount.value = Math.max(totalCount.value || 0, loadedCount)
            pageOffset.value = Math.max(pageOffset.value || 0, loadedCount)
        }

        if (!isAddingCategory.value) {
            if (data?.categories) {
                categoryMap.value = data.categories
            }
            if (data?.image_tags) {
                tagMap.value = data.image_tags
            }
            if (Array.isArray(data?.pinned_items)) {
                pinnedItems.value = data.pinned_items
            }
        }
        rebuildFilterIndexes()
        bumpFilterDataRevision()
    }

    const promoteLocalItemToTop = (itemId) => {
        if (!itemId || !Array.isArray(history.value) || history.value.length < 2) return
        const pinnedSet = new Set(pinnedItems.value)
        const currentIndex = history.value.findIndex((item) => item?.id === itemId)
        if (currentIndex < 0) return
        const selectedId = history.value[selectedIndex.value]?.id
        const [moved] = history.value.splice(currentIndex, 1)
        if (!moved) return
        let insertIndex = history.value.findIndex((item) => !pinnedSet.has(item?.id))
        if (insertIndex < 0) {
            insertIndex = history.value.length
        }
        if (insertIndex > history.value.length) {
            insertIndex = history.value.length
        }
        history.value.splice(insertIndex, 0, moved)
        if (selectedId) {
            const nextSelectedIndex = history.value.findIndex((item) => item?.id === selectedId)
            selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : 0
        } else {
            selectedIndex.value = 0
        }
        bumpFilterDataRevision()
    }

    const mergeImagePageIntoState = (data, reset = false) => {
        const items = Array.isArray(data?.items) ? data.items : []
        const baseOffset = Number.isFinite(data?.offset) ? Math.max(0, Number(data.offset)) : (reset ? 0 : pageOffset.value)
        if (reset) {
            clearPrefetchedPage()


            if (getLoadedHistoryCount() === 0) {
                history.value = []
                categoryMap.value = {}
                tagMap.value = {}
                tagSearchIndex.clear()
                itemTagSnapshot.clear()
                categorySearchIndex.clear()
                itemCategorySnapshot.clear()
                keywordTagMatchCache.clear()
                keywordCategoryMatchCache.clear()
                pinnedItems.value = []
            }
            previewCache.clear()
            asyncPreviewCache.clear()
            warmedIndices.clear()
            warmingIndices.clear()
        }
        // reset 恒为整页重取（offset=0）：先重建空数组再写入，截断旧数据，
        // 避免交界处残留旧条目/丢失中段条目导致加载更多跨缺口取到重复项
        history.value = applyImagePageToHistory(history.value, items, baseOffset, reset)
        for (let i = 0; i < items.length; i++) {
            const item = items[i]
            if (!item) continue
            setItemCategoryLocal(item.id, item.category || '未分类')
            setItemTagsLocal(item.id, item.tags)
        }

        const pinnedSet = new Set(pinnedItems.value)
        items.forEach((row) => {
            if (row?.pinned && row.id) {
                pinnedSet.add(row.id)
            }
        })
        const loadedItems = history.value
        pinnedItems.value = loadedItems
            .filter((item) => pinnedSet.has(item.id))
            .map((item) => item.id)

        const incomingTotal = Number.isFinite(data?.total) ? Number(data.total) : loadedItems.length
        totalCount.value = Math.max(Number(totalCount.value) || 0, incomingTotal, loadedItems.length)

        pageOffset.value = loadedItems.length


        if (Number.isFinite(data?.total)) {
            hasMore.value = data.total > loadedItems.length
        } else {
            hasMore.value = loadedItems.length < totalCount.value
        }

        if (Array.isArray(data?.categoryList)) {
            const list = data.categoryList.filter((c) => c !== '未分类' && c !== '全部')
            categories.value = ['未分类', ...Array.from(new Set(list))]
        }
        bumpFilterDataRevision()
    }

    const mergeIncrementalPageIntoState = (data) => {
        const items = Array.isArray(data?.items) ? data.items : []
        if (items.length === 0) {
            if (Number.isFinite(data?.total)) {
                const loadedCount = getLoadedHistoryCount()

                totalCount.value = Math.max(Number(data.total), loadedCount)
                pageOffset.value = loadedCount
                hasMore.value = pageOffset.value < totalCount.value
            }
            bumpFilterDataRevision()
            return
        }
        const selectedId = history.value[selectedIndex.value]?.id
        const {loadedItems, existingById} = getLoadedHistorySnapshot()
        const incomingIds = new Set()
        const front = []
        for (const row of items) {
            if (!row?.id) continue
            incomingIds.add(row.id)
            const existing = existingById.get(row.id) || {}
            front.push({
                ...existing,
                id: row.id,
                width: row.width ?? existing.width ?? 0,
                height: row.height ?? existing.height ?? 0,
                preview_png_base64: row.previewPngBase64 ?? row.preview_png_base64 ?? existing.preview_png_base64 ?? '',
                image_path: row.imagePath ?? row.image_path ?? existing.image_path ?? ''
            })
            setItemCategoryLocal(row.id, row.category || '未分类')
            if (Array.isArray(row.tags)) {
                setItemTagsLocal(row.id, row.tags)
            } else if (!(row.id in tagMap.value)) {
                setItemTagsLocal(row.id, [])
            }
        }
        if (front.length === 0) return
        const rest = []
        for (const item of loadedItems) {
            if (!incomingIds.has(item.id)) {
                rest.push(item)
            }
        }

        const nextHistory = [...front, ...rest]
        history.value = nextHistory

        const pinnedSet = new Set(pinnedItems.value)
        for (const row of items) {
            if (!row?.id) continue
            if (row.pinned) {
                pinnedSet.add(row.id)
            } else {
                pinnedSet.delete(row.id)
            }
        }
        pinnedItems.value = nextHistory
            .map((item) => item.id)
            .filter((id) => pinnedSet.has(id))

        const loadedCount = nextHistory.length
        totalCount.value = Number.isFinite(data?.total)
            ? Math.max(Number(data.total), loadedCount, Number(totalCount.value) || 0)
            : Math.max(totalCount.value || 0, loadedCount)
        pageOffset.value = loadedCount
        hasMore.value = pageOffset.value < totalCount.value
        if (selectedId) {
            const nextSelectedIndex = history.value.findIndex((item) => item?.id === selectedId)
            selectedIndex.value = nextSelectedIndex >= 0 ? nextSelectedIndex : 0
        } else if (selectedIndex.value < 0 && loadedCount > 0) {
            selectedIndex.value = 0
        }
        bumpFilterDataRevision()
    }

    const loadHistoryPage = async ({reset = false, force = false} = {}) => {
        if (isLoadingPage.value && !force) return
        isLoadingPage.value = true
        try {
            const offset = reset ? 0 : pageOffset.value
            const signature = currentPageQuerySignature()
            let data = null
            if (
                !reset
                && prefetchedPage.value
                && prefetchedPage.value.offset === offset
                && prefetchedPage.value.signature === signature
            ) {
                data = prefetchedPage.value.data
                prefetchedPage.value = null
            } else {
                if (!reset && isPrefetchingPage.value && prefetchPromise) {
                    await prefetchPromise
                    if (
                        prefetchedPage.value
                        && prefetchedPage.value.offset === offset
                        && prefetchedPage.value.signature === signature
                    ) {
                        data = prefetchedPage.value.data
                        prefetchedPage.value = null
                    }
                }
                if (!data) {
                    data = await ImageClipboardService.getHistoryPage({
                        offset,
                        limit: pageSize.value,
                        category: categoryFilter.value === '全部' ? null : categoryFilter.value,
                        keyword: searchKeyword.value.trim() || null,
                        pinnedOnly: false,
                        sortBy: sortBy.value,
                        sortOrder: sortOrder.value
                    })
                }
            }
            mergeImagePageIntoState(data, reset)
            if (selectedIndex.value < 0 || !history.value[selectedIndex.value]) {
                const firstLoaded = filteredHistory.value[0]
                selectedIndex.value = firstLoaded ? firstLoaded.index : -1
            }
            await nextTick()
            if (hasMore.value) {
                void prefetchNextPage()
            }
        } catch (error) {
            console.error('同步图片历史失败:', error)
            if (reset && getLoadedHistoryCount() === 0 && !initialPageRetryTimer) {
                initialPageRetryTimer = setTimeout(() => {
                    initialPageRetryTimer = null
                    loadHistoryPage({reset: true}).catch(() => {
                    })
                }, 800)
            }
        } finally {
            isLoadingPage.value = false
            flushPendingHistorySync()
        }
    }

    const ensureInitialPageLoaded = (force = false) => {
        if (isLoadingPage.value && !force) return
        if (!force && getLoadedHistoryCount() > 0) return
        loadHistoryPage({reset: true, force}).catch((error) => {
            console.error('初始化图片历史失败:', error)
        })
    }

    const syncHistory = async () => {
        clearPrefetchedPage()
        pageOffset.value = 0
        totalCount.value = 0
        hasMore.value = false
        loadMoreIntent.value = false
        await loadHistoryPage({reset: true})
    }

    const syncHistoryIncremental = async () => {
        const data = await ImageClipboardService.getHistoryPage({
            offset: 0,
            limit: Math.max(Number(pageSize.value) || 10, 30),
            category: categoryFilter.value === '全部' ? null : categoryFilter.value,
            keyword: searchKeyword.value.trim() || null,
            pinnedOnly: false,
            sortBy: sortBy.value,
            sortOrder: sortOrder.value
        })
        mergeIncrementalPageIntoState(data)
        await nextTick()
    }

    const scheduleHistorySync = (delay = 220) => {
        if (historyUpdateTimer) return
        historyUpdateTimer = window.setTimeout(async () => {
            historyUpdateTimer = null
            if (isLoadingPage.value) {
                pendingHistorySync = true
                return
            }
            try {
                await syncHistoryIncremental()
            } catch (error) {
                console.error('增量同步图片历史失败，回退全量同步:', error)
                await syncHistory()
            }
        }, delay)
    }

    onBeforeUnmount(() => {
        if (historyUpdateTimer) {
            clearTimeout(historyUpdateTimer)
            historyUpdateTimer = null
        }
        if (initialPageRetryTimer) {
            clearTimeout(initialPageRetryTimer)
            initialPageRetryTimer = null
        }
    })

    return {
        history,
        categoryFilter,
        searchKeyword,
        selectedIndex,
        totalCount,
        pageOffset,
        pageSize,
        hasMore,
        isLoadingPage,
        isVisible,
        loadMoreIntent,
        isReorderMode,
        pinnedItems,
        filteredHistory,
        filteredHistoryState,
        selectedDisplayIndex,
        selectedStatusText,
        loadStatusText,
        normalizeImagePageSize,
        cyclePageSize,
        handleLoadMoreIntent,
        toggleReorderMode,
        handleReorder,
        promoteImageItem,
        deleteItem,
        clearPrefetchedPage,
        syncHistory,
        loadHistoryPage,
        loadTailPage,
        flushPendingHistorySync,
        mergeShowWindowPayload,
        mergeIncrementalImageItem,
        ensureInitialPageLoaded,
        scheduleHistorySync
    }
}
