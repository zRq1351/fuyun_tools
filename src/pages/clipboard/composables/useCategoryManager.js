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

    const setItemCategory = async (item, value) => {
        const category = (value || '').trim()
        if (!category) {
            await removeItemCategory(item)
            return
        }

        console.log('[设置分类] 开始, item:', item, 'category:', category)

        // 设置标志位，防止 watch 触发重新加载
        if (setIsUpdatingCategory) {
            setIsUpdatingCategory(true)
            console.log('[设置分类] 标志位已设置为 true')
        }

        // 使用与图片一致的 setItemCategoryLocal，更新 map + 索引
        if (setItemCategoryLocal) {
            setItemCategoryLocal(item, category)
        } else {
            // 降级方案：直接更新 map
            categoryMap.value[item] = category
        }

        if (!categories.value.includes(category)) {
            if (onCategoryAdded) {
                onCategoryAdded(category)
            } else {
                categories.value = [...categories.value, category]
            }
        }

        // 通知外部刷新过滤数据
        if (bumpFilterDataRevision) {
            bumpFilterDataRevision()
            console.log('[设置分类] 已调用 bumpFilterDataRevision')
        }

        try {
            await CategoryService.setItemCategory(item, category)
            console.log('[设置分类] 后端保存成功')
        } catch (error) {
            console.error('保存分类失败:', error)
        } finally {
            // 后端保存完成后，延迟重置标志位，给 computed 一些时间稳定
            // 增加延迟时间，确保 watch 不会在标志位清除前触发
            setTimeout(() => {
                console.log('[分类更新完成] 清除标志位, item:', item, 'category:', category)
                if (setIsUpdatingCategory) {
                    setIsUpdatingCategory(false)
                }
            }, 800)  // 从 300ms 增加到 800ms
        }
    }

    const removeItemCategory = async (item) => {
        if (!item) return
        if (categoryMap.value[item]) {
            // 设置标志位，防止 watch 触发重新加载
            if (setIsUpdatingCategory) {
                setIsUpdatingCategory(true)
            }

            // 使用与图片一致的 removeItemCategoryLocal
            if (options.removeItemCategoryLocal) {
                options.removeItemCategoryLocal(item)
            } else {
                delete categoryMap.value[item]
            }

            // 通知外部刷新过滤数据
            if (bumpFilterDataRevision) {
                bumpFilterDataRevision()
            }

            try {
                await CategoryService.setItemCategory(item, "")
            } catch (error) {
                console.error('移除分类失败:', error)
            } finally {
                // 后端保存完成后，延迟重置标志位
                setTimeout(() => {
                    console.log('[分类移除完成] 清除标志位, item:', item)
                    if (setIsUpdatingCategory) {
                        setIsUpdatingCategory(false)
                    }
                }, 800)  // 从 300ms 增加到 800ms
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
