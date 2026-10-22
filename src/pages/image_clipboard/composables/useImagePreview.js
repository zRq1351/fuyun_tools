// 图片预览 composable
// 职责：缩略图 dataURL 生成与读取（getPreviewDataUrl）、周边图片预热队列（warmup）、
// 全屏预览窗口联动（打开预览、上一张/下一张、到底加载下一页），
// 以及 preview-ready 异步缩略图合并与预览缓存剪枝（previewCacheKeepIds watch）。
// 预览缓存（previewCache/asyncPreviewCache）及其容量管控、预热标记集合由 App.vue 注入：
// 删除/整页重置等历史逻辑同样需要访问它们，属跨域共用状态。
import {computed, onBeforeUnmount, onMounted, ref, watch} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage} from 'element-plus'
import {listen} from '@tauri-apps/api/event'
import {buildFileUrlFromPath} from '../../../utils/fileUrl'
import {ImageClipboardService} from '../../../services/ipc'

export function useImagePreview({
    previewCache,
    asyncPreviewCache,
    enforcePreviewCacheSize,
    enforceAsyncPreviewCacheSize,
    warmedIndices,
    warmingIndices,
    history,
    selectedIndex,
    filteredHistory,
    filteredHistoryState,
    hasMore,
    isLoadingPage,
    totalCount,
    getLoadedHistoryCount,
    loadHistoryPage
}) {
    const {t} = useI18n()
    const pendingWarmupItemIds = new Set()
    let unlistenPreviewReady = null
    let warmupBatchTimer = null
    let warmupBatchInFlight = null

    const previewCacheKeepIds = computed(() => {
        if (filteredHistoryState.value.total === 0) return []
        const ids = filteredHistory.value
            .map((entry) => entry.item?.id)
            .filter(Boolean)
        const selectedId = history.value[selectedIndex.value]?.id
        if (selectedId) {
            ids.push(selectedId)
        }
        return Array.from(new Set(ids))
    })

    const prunePreviewCache = (keepIds) => {
        if (!Array.isArray(keepIds)) return
        const keepSet = new Set(keepIds)
        for (const key of previewCache.keys()) {
            if (!keepSet.has(key)) {
                previewCache.delete(key)
            }
        }
        enforcePreviewCacheSize()
    }

    const pruneAsyncPreviewCache = (keepIds) => {
        if (!Array.isArray(keepIds)) return
        const keepSet = new Set(keepIds)
        for (const key of asyncPreviewCache.keys()) {
            if (!keepSet.has(key)) {
                asyncPreviewCache.delete(key)
            }
        }
        enforceAsyncPreviewCacheSize()
    }

    const getPreviewDataUrl = (item) => {
        if (previewCache.has(item.id)) {
            return previewCache.get(item.id)
        }
        try {
            // 优先文件路径（asset://），回退 base64
            const fileUrl = buildFileUrlFromPath(item.image_path)
            if (fileUrl) {
                previewCache.set(item.id, fileUrl)
                enforcePreviewCacheSize()
                return fileUrl
            }

            const previewBase64 = typeof item.preview_png_base64 === 'string' ? item.preview_png_base64.trim() : ''
            if (previewBase64) {
                const previewUrl = `data:image/png;base64,${previewBase64}`
                previewCache.set(item.id, previewUrl)
                enforcePreviewCacheSize()
                return previewUrl
            }


            if (asyncPreviewCache.has(item.id)) {
                const previewUrl = asyncPreviewCache.get(item.id)
                asyncPreviewCache.delete(item.id)
                asyncPreviewCache.set(item.id, previewUrl)
                enforceAsyncPreviewCacheSize()
                previewCache.set(item.id, previewUrl)
                enforcePreviewCacheSize()
                return previewUrl
            }


            const previewUrl = buildFileUrlFromPath(item.image_path)
            previewCache.set(item.id, previewUrl)
            enforcePreviewCacheSize()
            return previewUrl
        } catch (error) {
            console.error('图片预览生成失败:', error)
            return ''
        }
    }

    const selectByIndex = (index) => {
        selectedIndex.value = index
        warmupAround(index)
    }

    const enqueueWarmupByIndex = (index) => {
        if (index < 0 || index >= history.value.length) return
        const itemId = history.value[index]?.id
        if (!itemId || warmedIndices.has(itemId) || warmingIndices.has(itemId)) return
        pendingWarmupItemIds.add(itemId)
    }

    const flushWarmupBatch = async () => {
        if (warmupBatchInFlight) return warmupBatchInFlight
        const itemIds = Array.from(pendingWarmupItemIds).filter(
            (itemId) => itemId && !warmedIndices.has(itemId) && !warmingIndices.has(itemId)
        )
        pendingWarmupItemIds.clear()
        if (itemIds.length === 0) return null
        itemIds.forEach((itemId) => warmingIndices.add(itemId))
        warmupBatchInFlight = ImageClipboardService.warmupMultipleItems(itemIds)
            .then(() => {
                itemIds.forEach((itemId) => warmedIndices.add(itemId))
            })
            .catch((error) => {
                console.error('批量预热失败:', error)
            })
            .finally(() => {
                itemIds.forEach((itemId) => warmingIndices.delete(itemId))
                warmupBatchInFlight = null
                if (pendingWarmupItemIds.size > 0) {
                    void flushWarmupBatch()
                }
            })
        return warmupBatchInFlight
    }

    const scheduleWarmupBatch = (delay = 80) => {
        if (warmupBatchTimer) {
            clearTimeout(warmupBatchTimer)
        }
        warmupBatchTimer = setTimeout(() => {
            warmupBatchTimer = null
            void flushWarmupBatch()
        }, delay)
    }

    const warmupOne = (index) => {
        enqueueWarmupByIndex(index)
        scheduleWarmupBatch(60)
    }

    const warmupBatch = (startIndex, count = 6) => {
        for (let i = startIndex; i < Math.min(startIndex + count, history.value.length); i++) {
            enqueueWarmupByIndex(i)
        }
        scheduleWarmupBatch(80)
    }

    const warmupAround = (index) => {
        warmupBatch(Math.max(0, index - 1), 3)
    }

    const openFullscreen = async (itemId) => {
        try {
            if (!itemId) return
            // 传递可见列表顺序，全屏预览按此顺序上一张/下一张
            const list = filteredHistory.value
            const orderedIds = list
                .map((entry) => entry?.item?.id)
                .filter((id) => typeof id === 'string' && id)
            const idx = list.findIndex((entry) => entry?.item?.id === itemId)
            previewingItemId.value = itemId
            await ImageClipboardService.openPreviewWindowById(itemId, orderedIds, {
                // 与卡片序号同源：位置（1 起）/ 总数
                displayIndex: idx >= 0 ? idx + 1 : 1,
                displayTotal: totalCount.value || list.length,
                // 还有下一页时，即使当前是已加载最后一项也保留「下一张」
                hasPrev: idx > 0,
                hasNext: idx >= 0 && (idx < list.length - 1 || hasMore.value)
            })
        } catch (error) {
            console.error('打开预览窗口失败:', error)
            ElMessage.error(t('imageClipboard.previewFailed', {error: String(error)}))
        }
    }

    // 全屏预览的上一张/下一张由本窗口按「当前卡片列表」现算邻居，保证与卡片展示顺序一致
    const previewingItemId = ref('')
    let unlistenPreviewNav = null

    // 全屏「下一张」到已加载末尾时拉取下一页（与卡片分页行为一致），失败则保持当前张
    const ensureNextPageForPreviewNav = async () => {
        const list = filteredHistory.value
        const idx = list.findIndex((entry) => entry?.item?.id === previewingItemId.value)
        if (idx < 0 || idx < list.length - 1) return
        let guard = 0
        while (hasMore.value && guard < 5) {
            guard++
            if (isLoadingPage.value) {
                await new Promise((resolve) => setTimeout(resolve, 80))
                continue
            }
            const before = getLoadedHistoryCount()
            await loadHistoryPage({reset: false})
            if (getLoadedHistoryCount() > before) return
            if (!hasMore.value) return
        }
    }

    const handlePreviewNavigate = async (event) => {
        const payload = event?.payload || {}
        const delta = Number(payload.delta) === -1 ? -1 : 1
        const requestId = typeof payload.request_id === 'string' ? payload.request_id : null
        // 前进到已加载末尾时先加载下一页，再在扩展后的列表中取邻居
        if (delta === 1) {
            await ensureNextPageForPreviewNav()
        }
        const list = filteredHistory.value
        const n = list.length
        if (n === 0) return
        let idx = list.findIndex((entry) => entry?.item?.id === previewingItemId.value)
        if (idx < 0) {
            // 基准丢失时退到列表首项，而不是按错误下标跳转
            idx = 0
        }
        // 边界不回绕：第一张没有上一张、最后一张（无更多页）没有下一张（与卡片语义一致）
        const nextIndex = idx + delta
        if (nextIndex < 0 || nextIndex >= n) return
        const nextId = list[nextIndex]?.item?.id
        if (!nextId || nextId === previewingItemId.value) return
        const previousItemId = previewingItemId.value
        previewingItemId.value = nextId
        try {
            await ImageClipboardService.switchPreviewById(nextId, requestId, {
                hasPrev: nextIndex > 0,
                hasNext: nextIndex < n - 1 || hasMore.value,
                // 与卡片序号同源：位置（1 起）/ 总数
                displayIndex: nextIndex + 1,
                displayTotal: totalCount.value || n
            })
        } catch (error) {
            previewingItemId.value = previousItemId
            console.error('切换预览图片失败:', error)
        }
    }

    onMounted(async () => {
        unlistenPreviewReady = await listen('preview-ready', (event) => {
            const {itemId, previewUrl} = event.payload || {}
            if (itemId && previewUrl) {

                asyncPreviewCache.set(itemId, previewUrl)
                enforceAsyncPreviewCacheSize()
                previewCache.set(itemId, previewUrl)
                enforcePreviewCacheSize()


                const itemIndex = history.value.findIndex(item => item?.id === itemId)
                if (itemIndex >= 0) {

                    const base64 = previewUrl.replace('data:image/png;base64,', '')

                    history.value[itemIndex].preview_png_base64 = base64
                }

            }
        })

        unlistenPreviewNav = await listen('image-preview-navigate', (event) => {
            void handlePreviewNavigate(event)
        })
    })

    onBeforeUnmount(() => {
        previewCache.clear()
        asyncPreviewCache.clear()
        if (warmupBatchTimer) {
            clearTimeout(warmupBatchTimer)
            warmupBatchTimer = null
        }
        if (unlistenPreviewNav) {
            unlistenPreviewNav()
            unlistenPreviewNav = null
        }
        if (unlistenPreviewReady) {
            unlistenPreviewReady()
            unlistenPreviewReady = null
        }
    })

    watch(previewCacheKeepIds, (ids) => {
        prunePreviewCache(ids)
        pruneAsyncPreviewCache(ids)
    }, {immediate: true})

    return {
        getPreviewDataUrl,
        selectByIndex,
        openFullscreen,
        warmupOne
    }
}
