import {ref} from 'vue'

/**
 * 分类搜索索引 composable
 * 管理分类索引、快照和关键词匹配缓存
 * 被 clipboard 和 image_clipboard 共享使用
 */
export function useCategorySearchIndex(categoryMap, options = {}) {
    const {extraCaches = []} = options

    const filterDataRevision = ref(0)
    const categorySearchIndex = new Map()
    const itemCategorySnapshot = new Map()
    const keywordCategoryMatchCache = new Map()

    const SEARCH_CACHE_MAX_SIZE = 50

    const bumpFilterDataRevision = () => {
        filterDataRevision.value += 1
        keywordCategoryMatchCache.clear()
        for (const cache of extraCaches) {
            if (cache && typeof cache.clear === 'function') {
                cache.clear()
            }
        }
    }

    const removeCategoryIndexForItem = (id) => {
        const oldCategory = itemCategorySnapshot.get(id)
        if (oldCategory === undefined) {
            itemCategorySnapshot.delete(id)
            return
        }
        const idSet = categorySearchIndex.get(oldCategory)
        if (idSet) {
            idSet.delete(id)
            if (idSet.size === 0) {
                categorySearchIndex.delete(oldCategory)
            }
        }
        itemCategorySnapshot.delete(id)
    }

    const applyCategoryIndexForItem = (id, category) => {
        removeCategoryIndexForItem(id)
        const normalized = String(category || '未分类')
        itemCategorySnapshot.set(id, normalized)
        let idSet = categorySearchIndex.get(normalized)
        if (!idSet) {
            idSet = new Set()
            categorySearchIndex.set(normalized, idSet)
        }
        idSet.add(id)
    }

    const setItemCategoryLocal = (id, category) => {
        if (!id) return
        const normalized = String(category || '未分类')
        categoryMap.value[id] = normalized
        applyCategoryIndexForItem(id, normalized)
        keywordCategoryMatchCache.clear()
    }

    const removeItemCategoryLocal = (id) => {
        if (!id) return
        delete categoryMap.value[id]
        removeCategoryIndexForItem(id)
        keywordCategoryMatchCache.clear()
    }

    const rebuildCategorySearchIndex = () => {
        categorySearchIndex.clear()
        itemCategorySnapshot.clear()
        keywordCategoryMatchCache.clear()
        const currentCategoryMap = categoryMap.value || {}
        for (const id of Object.keys(currentCategoryMap)) {
            applyCategoryIndexForItem(id, currentCategoryMap[id] || '未分类')
        }
    }

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
        // 清理过期缓存，保留最近的条目
        if (keywordCategoryMatchCache.size >= SEARCH_CACHE_MAX_SIZE) {
            const keysToDelete = Array.from(keywordCategoryMatchCache.keys()).slice(0, SEARCH_CACHE_MAX_SIZE / 2)
            for (const key of keysToDelete) {
                keywordCategoryMatchCache.delete(key)
            }
        }
        keywordCategoryMatchCache.set(cacheKey, matchedIds)
        return matchedIds
    }

    return {
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
    }
}
