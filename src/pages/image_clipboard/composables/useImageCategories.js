// 图片分类管理 composable
// 职责：分类列表增删改、条目分类设置（右键菜单 assignToCategory / 拖拽归类 handleDrop）、
// 条目标签编辑（editItemTags）与拖拽归类的交互状态（dragItemId）。
// 分类/标签索引（setItemCategoryLocal 等）、当前分类过滤（categoryFilter）、
// 分类列表（categories）与新增输入态（isAddingCategory）由 App.vue 注入：
// 它们同时被历史同步/过滤视图使用，属跨域共用状态，避免 composable 间循环依赖。
import {nextTick, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage, ElMessageBox} from 'element-plus'
import {ImageCategoryService, ImageClipboardService} from '../../../services/ipc'
import {runCategoryAssignment} from '../../shared/categoryActions'

export function useImageCategories({
    categories,
    categoryMap,
    categoryFilter,
    isAddingCategory,
    getItemTags,
    setItemCategoryLocal,
    setItemTagsLocal,
    removeItemCategoryLocal,
    bumpFilterDataRevision,
    isCtrlKeyPressed,
    flushPendingHistorySync,
    contextMenuItemId,
    closeContextMenu
}) {
    const {t} = useI18n()
    const newCategoryName = ref('')
    const newCategoryInputRef = ref(null)
    const dragItemId = ref('')
    const categoryInputOpenedAt = ref(0)

    const canDeleteCategory = (category) => {
        return category !== '未分类'
    }

    const persistImageCategory = async (itemId, category, resetTagsWhenUnclassified = false) => {
        await ImageCategoryService.setItemCategory(itemId, category)
        if (resetTagsWhenUnclassified && category === '未分类') {
            setItemTagsLocal(itemId, [])
            await ImageClipboardService.setItemTags(itemId, [])
        }
    }

    const assignToCategory = async (category) => {
        await runCategoryAssignment({
            itemKey: contextMenuItemId.value,
            category,
            applyLocal: (itemId, nextCategory) => {
                setItemCategoryLocal(itemId, nextCategory)
                bumpFilterDataRevision()
            },
            persist: (itemId, nextCategory) => persistImageCategory(itemId, nextCategory, true),
            onError: (error) => console.error('设置图片分类失败:', error),
            onFinally: closeContextMenu
        })
    }

    const editItemTags = async () => {
        const itemId = contextMenuItemId.value
        if (!itemId) return
        const current = getItemTags(itemId).join(', ')
        try {
            const {value} = await ElMessageBox.prompt(t('imageClipboard.tagInputPrompt'), t('imageClipboard.editTagsTitle'), {
                inputValue: current,
                confirmButtonText: t('common.save'),
                cancelButtonText: t('common.cancel')
            })
            const tags = String(value || '')
                .split(/[,，]/)
                .map((item) => item.trim())
                .filter((item, idx, arr) => item && arr.indexOf(item) === idx)
            setItemTagsLocal(itemId, tags)
            bumpFilterDataRevision()
            await ImageClipboardService.setItemTags(itemId, tags)
            closeContextMenu()
            ElMessage.success(t('imageClipboard.tagsUpdated'))
        } catch (error) {
            if (error !== 'cancel') {
                ElMessage.error(t('imageClipboard.updateTagsFailed', {error}))
            }
        }
    }

    const handleDragStart = (event, itemId) => {
        if (!isCtrlKeyPressed.value) {
            event.preventDefault()
            return
        }
        flushPendingHistorySync()
        dragItemId.value = itemId
        event.dataTransfer.effectAllowed = 'copy'
        event.dataTransfer.setData('text/plain', itemId)
    }

    const handleDragEnd = () => {
        dragItemId.value = ''
    }

    const handleDrop = async (event, category) => {
        event.preventDefault()
        const target = event.currentTarget
        if (target && target.classList.contains('category-pill')) {
            target.classList.remove('drag-over')
        }
        const droppedItemId = dragItemId.value || event.dataTransfer?.getData('text/plain') || ''
        await runCategoryAssignment({
            itemKey: droppedItemId,
            category,
            applyLocal: (itemId, nextCategory) => {
                setItemCategoryLocal(itemId, nextCategory)
                bumpFilterDataRevision()
            },
            persist: (itemId, nextCategory) => persistImageCategory(itemId, nextCategory),
            onError: (error) => console.error('拖拽设置图片分类失败:', error)
        })
    }

    const startCreateCategory = () => {
        isAddingCategory.value = true
        categoryInputOpenedAt.value = Date.now()
        newCategoryName.value = ''
        nextTick(() => {
            newCategoryInputRef.value?.focus()
        })
    }

    const confirmCreateCategory = async () => {
        const category = newCategoryName.value.trim()
        isAddingCategory.value = false
        newCategoryName.value = ''
        categoryInputOpenedAt.value = 0
        if (category && category !== '未分类' && category !== '全部' && !categories.value.includes(category)) {
            categories.value.push(category)
            try {
                await ImageCategoryService.addCategory(category)
            } catch (error) {
                console.error('添加图片分类失败:', error)
            }
        }
    }

    const cancelCreateCategory = () => {
        isAddingCategory.value = false
        newCategoryName.value = ''
        categoryInputOpenedAt.value = 0
    }

    const removeCategory = async (category) => {
        if (!canDeleteCategory(category)) return
        categories.value = categories.value.filter((item) => item !== category)
        Object.keys(categoryMap.value).forEach((key) => {
            if (categoryMap.value[key] === category) {
                removeItemCategoryLocal(key)
                setItemTagsLocal(key, [])
            }
        })
        bumpFilterDataRevision()
        if (categoryFilter.value === category) {
            categoryFilter.value = '全部'
        }
        try {
            await ImageCategoryService.removeCategory(category)
        } catch (error) {
            console.error('删除图片分类失败:', error)
        }
    }

    return {
        newCategoryName,
        newCategoryInputRef,
        canDeleteCategory,
        startCreateCategory,
        confirmCreateCategory,
        cancelCreateCategory,
        removeCategory,
        assignToCategory,
        editItemTags,
        handleDragStart,
        handleDragEnd,
        handleDrop
    }
}
