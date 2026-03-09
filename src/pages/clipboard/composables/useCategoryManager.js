import {nextTick, ref} from 'vue'
import {CategoryService} from '../../../services/ipc'

export function useCategoryManager(categories, categoryMap, categoryFilter) {
    const isAddingCategory = ref(false)
    const newCategoryName = ref('')
    const newCategoryInputRef = ref(null)

    const setItemCategory = async (item, value) => {
        const category = (value || '').trim()
        if (!category) {
            await removeItemCategory(item)
            return
        }

        categoryMap.value[item] = category
        if (!categories.value.includes(category)) {
            categories.value.push(category)
        }

        try {
            await CategoryService.setItemCategory(item, category)
        } catch (error) {
            console.error('保存分类失败:', error)
        }
    }

    const removeItemCategory = async (item) => {
        if (!item) return
        if (categoryMap.value[item]) {
            delete categoryMap.value[item]
            try {
                await CategoryService.setItemCategory(item, "")
            } catch (error) {
                console.error('移除分类失败:', error)
            }
        }
    }

    const removeCategory = async (category) => {
        if (!canDeleteCategory(category)) return

        categories.value = categories.value.filter((item) => item !== category)
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
        if (category && category !== '未分类' && category !== '全部') {
            if (!categories.value.includes(category)) {
                categories.value.push(category)
                try {
                    await CategoryService.addCategory(category)
                } catch (error) {
                    console.error('添加分类失败:', error)
                }
            }
        }
        isAddingCategory.value = false
        newCategoryName.value = ''
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
