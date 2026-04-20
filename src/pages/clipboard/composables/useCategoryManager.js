import {nextTick, ref} from 'vue'
import {CategoryService} from '../../../services/ipc'

export function useCategoryManager(categories, categoryMap, categoryFilter, options = {}) {
    const {
        onCategoryAdded,
        onCategoryRemoved,
        bumpFilterDataRevision,
        setIsUpdatingCategory,
        setItemCategoryLocal
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

        console.log('[设置分类] 开始, itemId:', itemId, 'category:', category)

        
        if (setIsUpdatingCategory) {
            setIsUpdatingCategory(true)
            console.log('[设置分类] 标志位已设置为 true')
        }

        
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
            console.log('[设置分类] 已调用 bumpFilterDataRevision')
        }

        try {
            await CategoryService.setItemCategory(itemId, category)
            console.log('[设置分类] 后端保存成功')
        } catch (error) {
            console.error('保存分类失败:', error)
        } finally {
            
            
            setTimeout(() => {
                console.log('[分类更新完成] 清除标志位, itemId:', itemId, 'category:', category)
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

            
            if (options.removeItemCategoryLocal) {
                options.removeItemCategoryLocal(itemId)
            } else {
                delete categoryMap.value[itemId]
            }

            
            if (bumpFilterDataRevision) {
                bumpFilterDataRevision()
            }

            try {
                await CategoryService.setItemCategory(itemId, "")
            } catch (error) {
                console.error('移除分类失败:', error)
            } finally {
                
                setTimeout(() => {
                    console.log('[分类移除完成] 清除标志位, itemId:', itemId)
                    if (setIsUpdatingCategory) {
                        setIsUpdatingCategory(false)
                    }
                }, 800)  
            }
        }
    }

    const removeCategory = async (category) => {
        if (!canDeleteCategory(category)) return

        if (onCategoryRemoved) {
            onCategoryRemoved(category)
        } else {
            categories.value = categories.value.filter((item) => item !== category)
        }
        Object.keys(categoryMap.value).forEach((item) => {
            if (categoryMap.value[item] === category) {
                delete categoryMap.value[item]
            }
        })

        if (categoryFilter.value === category) {
            categoryFilter.value = '全部'
        }

        try {
            await CategoryService.removeCategory(category)
        } catch (error) {
            console.error('删除分类失败:', error)
        }
    }

    const canDeleteCategory = (category) => {
        return category !== '未分类'
    }

    const startCreateCategory = () => {
        isAddingCategory.value = true
        newCategoryName.value = ''
        nextTick(() => {
            newCategoryInputRef.value?.focus()
        })
    }

    const confirmCreateCategory = async () => {
        const category = newCategoryName.value.trim()
        isAddingCategory.value = false
        newCategoryName.value = ''
        if (category && category !== '未分类' && category !== '全部') {
            if (!categories.value.includes(category)) {
                if (onCategoryAdded) {
                    onCategoryAdded(category)
                } else {
                    categories.value = [...categories.value, category]
                }
                try {
                    await CategoryService.addCategory(category)
                } catch (error) {
                    console.error('添加分类失败:', error)
                }
            }
        }
    }

    const cancelCreateCategory = () => {
        isAddingCategory.value = false
        newCategoryName.value = ''
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
