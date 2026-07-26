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

        // Save previous category for rollback
        const prevCategory = categoryMap.value[itemId]

        // Optimistically update local state
        if (setItemCategoryLocal) {
            setItemCategoryLocal(itemId, category)
        } else {
            categoryMap.value[itemId] = category
        }

        // Add category to list if not exists
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
            // Rollback local state on error
            if (prevCategory) {
                if (setItemCategoryLocal) {
                    setItemCategoryLocal(itemId, prevCategory)
                } else {
                    categoryMap.value[itemId] = prevCategory
                }
            } else {
                // Item had no previous category, remove it
                if (customRemoveItemCategoryLocal) {
                    customRemoveItemCategoryLocal(itemId)
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
            }, 300)
        }
    }

    const removeItemCategory = async (itemId) => {
        if (!itemId) return
        if (!categoryMap.value[itemId]) return

        if (setIsUpdatingCategory) {
            setIsUpdatingCategory(true)
        }

        // Save previous category for rollback
        const prevCategory = categoryMap.value[itemId]

        // Optimistically remove category locally
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
            // Rollback local state on error
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
            }, 300)
        }
    }

    const removeCategory = async (category) => {
        if (!canDeleteCategory(category)) return

        // Save previous state for rollback
        const prevCategories = categories.value.slice()
        const prevCategoryMap = {...categoryMap.value}
        const prevFilter = categoryFilter.value

        // Optimistically update local state
        if (onCategoryRemoved) {
            onCategoryRemoved(category)
        } else {
            categories.value = categories.value.filter(item => item !== category)
        }

        // Remove category from all items locally
        for (const item of Object.keys(categoryMap.value)) {
            if (categoryMap.value[item] === category) {
                if (customRemoveItemCategoryLocal) {
                    customRemoveItemCategoryLocal(item)
                } else {
                    delete categoryMap.value[item]
                }
            }
        }

        // Reset filter if it was showing the deleted category
        if (categoryFilter.value === category) {
            categoryFilter.value = '全部'
        }

        try {
            await categoryService.removeCategory(category)
        } catch (error) {
            console.error('删除分类失败:', error)
            // Rollback local state on error
            categories.value = prevCategories

            // Restore categoryMap by clearing and reassigning
            for (const key of Object.keys(categoryMap.value)) {
                delete categoryMap.value[key]
            }
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

        // Validate category name
        if (!category || category === '未分类' || category === '全部') return

        // Skip if category already exists
        if (categories.value.includes(category)) return

        // Save previous state for rollback
        const prevCategories = categories.value.slice()

        // Optimistically add category locally
        if (onCategoryAdded) {
            onCategoryAdded(category)
        } else {
            categories.value = [...categories.value, category]
        }

        try {
            await categoryService.addCategory(category)
        } catch (error) {
            console.error('添加分类失败:', error)
            // Rollback: remove locally added category
            categories.value = prevCategories
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
