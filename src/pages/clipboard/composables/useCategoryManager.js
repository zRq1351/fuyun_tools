import {nextTick, ref} from 'vue'
import {CategoryService} from '../../../services/ipc'

export function useCategoryManager(categories, categoryMap, categoryFilter, options = {}) {
    const {
        onCategoryAdded,
        onCategoryRemoved,
        bumpFilterDataRevision,
        setIsUpdatingCategory,
        setItemCategoryLocal,
        removeItemCategoryLocal: customRemoveItemCategoryLocal,
        categoryService = CategoryService,
        categoryInputOpenedAt = null,
    } = options
    const isAddingCategory = ref(false)
    const newCategoryName = ref('')
    const newCategoryInputRef = ref(null)

    const setItemCategory = async (itemId, value) => {
        const category = (value || '').trim()
        if (!category) {
            await removeItemCategory(itemId)
            return
        }

        if (setIsUpdatingCategory) {
            setIsUpdatingCategory(true)
        }

        // 保存旧值用于回滚
        const prevCategory = categoryMap.value[itemId]

        if (setItemCategoryLocal) {
            setItemCategoryLocal(itemId, category)
        } else {
            categoryMap.value[itemId] = category
        }

        if (!categories.value.includes(category)) {
            if (onCategoryAdded) {
                onCategoryAdded(category)
            } else {
                categories.value = [...categories.value, category]
            }
        }

        if (bumpFilterDataRevision) {
            bumpFilterDataRevision()
        }

        try {
            await categoryService.setItemCategory(itemId, category)
        } catch (error) {
            console.error('保存分类失败:', error)
            // 回滚本地状态
            if (prevCategory) {
                if (setItemCategoryLocal) {
                    setItemCategoryLocal(itemId, prevCategory)
                } else {
                    categoryMap.value[itemId] = prevCategory
                }
            } else {
                if (options.removeItemCategoryLocal) {
                    options.removeItemCategoryLocal(itemId)
                } else {
                    delete categoryMap.value[itemId]
                }
            }
            if (bumpFilterDataRevision) {
                bumpFilterDataRevision()
            }
        } finally {
            setTimeout(() => {
                if (setIsUpdatingCategory) {
                    setIsUpdatingCategory(false)
                }
            }, 800)
        }
    }

    const removeItemCategory = async (itemId) => {
        if (!itemId) return
        if (categoryMap.value[itemId]) {
            if (setIsUpdatingCategory) {
                setIsUpdatingCategory(true)
            }

            // 保存旧值用于回滚
            const prevCategory = categoryMap.value[itemId]

            if (customRemoveItemCategoryLocal) {
                customRemoveItemCategoryLocal(itemId)
            } else {
                delete categoryMap.value[itemId]
            }

            if (bumpFilterDataRevision) {
                bumpFilterDataRevision()
            }

            try {
                await categoryService.setItemCategory(itemId, "")
            } catch (error) {
                console.error('移除分类失败:', error)
                // 回滚本地状态
                if (setItemCategoryLocal) {
                    setItemCategoryLocal(itemId, prevCategory)
                } else {
                    categoryMap.value[itemId] = prevCategory
                }
                if (bumpFilterDataRevision) {
                    bumpFilterDataRevision()
                }
            } finally {
                setTimeout(() => {
                    if (setIsUpdatingCategory) {
                        setIsUpdatingCategory(false)
                    }
                }, 800)
            }
        }
    }

    const removeCategory = async (category) => {
        if (!canDeleteCategory(category)) return

        // 保存旧状态用于回滚
        const prevCategories = categories.value.slice()
        const prevCategoryMap = {...categoryMap.value}
        const prevFilter = categoryFilter.value

        if (onCategoryRemoved) {
            onCategoryRemoved(category)
        } else {
            categories.value = categories.value.filter((item) => item !== category)
        }
        Object.keys(categoryMap.value).forEach((item) => {
            if (categoryMap.value[item] === category) {
                if (customRemoveItemCategoryLocal) {
                    customRemoveItemCategoryLocal(item)
                } else {
                    delete categoryMap.value[item]
                }
            }
        })

        if (categoryFilter.value === category) {
            categoryFilter.value = '全部'
        }

        try {
            await categoryService.removeCategory(category)
        } catch (error) {
            console.error('删除分类失败:', error)
            // 回滚本地状态
            categories.value = prevCategories
            Object.keys(categoryMap.value).forEach(key => delete categoryMap.value[key])
            Object.assign(categoryMap.value, prevCategoryMap)
            categoryFilter.value = prevFilter
            if (bumpFilterDataRevision) {
                bumpFilterDataRevision()
            }
        }
    }

    const canDeleteCategory = (category) => {
        return category !== '未分类'
    }

    const startCreateCategory = () => {
        isAddingCategory.value = true
        newCategoryName.value = ''
        if (categoryInputOpenedAt) {
            categoryInputOpenedAt.value = Date.now()
        }
        nextTick(() => {
            newCategoryInputRef.value?.focus()
        })
    }

    const confirmCreateCategory = async () => {
        const category = newCategoryName.value.trim()
        isAddingCategory.value = false
        newCategoryName.value = ''
        if (categoryInputOpenedAt) {
            categoryInputOpenedAt.value = 0
        }
        if (category && category !== '未分类' && category !== '全部') {
            if (!categories.value.includes(category)) {
                const prevCategories = categories.value.slice()
                if (onCategoryAdded) {
                    onCategoryAdded(category)
                } else {
                    categories.value = [...categories.value, category]
                }
                try {
                    await categoryService.addCategory(category)
                } catch (error) {
                    console.error('添加分类失败:', error)
                    // 回滚：移除本地添加的分类
                    categories.value = prevCategories
                }
            }
        }
    }

    const cancelCreateCategory = () => {
        isAddingCategory.value = false
        newCategoryName.value = ''
        if (categoryInputOpenedAt) {
            categoryInputOpenedAt.value = 0
        }
    }

    return {
        isAddingCategory,
        newCategoryName,
        newCategoryInputRef,
        setItemCategory,
        removeItemCategory,
        removeCategory,
        canDeleteCategory,
        startCreateCategory,
        confirmCreateCategory,
        cancelCreateCategory
    }
}
