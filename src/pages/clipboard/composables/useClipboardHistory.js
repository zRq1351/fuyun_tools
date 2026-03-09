import {computed, ref} from 'vue'
import {CategoryService, ClipboardService} from '../../../services/ipc'

export function useClipboardHistory() {
    const history = ref([])
    const selectedIndex = ref(-1)
    const searchKeyword = ref('')
    const categoryFilter = ref('全部')
    const categoryMap = ref({})

    const getItemCategory = (item) => {
        return categoryMap.value[item] || '未分类'
    }

    const visibleHistory = computed(() => {
        const keyword = searchKeyword.value.trim().toLowerCase()
        const filter = categoryFilter.value
        return history.value
            .map((item, index) => ({item, index}))
            .filter((entry) => {
                const itemCategory = getItemCategory(entry.item)
                if (filter !== '全部' && itemCategory !== filter) {
                    return false
                }
                if (!keyword) return true
                return entry.item.toLowerCase().includes(keyword)
            })
    })

    const updateSelection = (index, shouldScroll = false, contentRef = null, visibleIndex = null) => {
        if (index < 0 || index >= history.value.length) return
        selectedIndex.value = index
    }

    const deleteItem = async (index) => {
        try {
            const removedItem = history.value[index]
            history.value.splice(index, 1)
            if (selectedIndex.value >= history.value.length) {
                selectedIndex.value = Math.max(0, history.value.length - 1)
            }

            if (removedItem && categoryMap.value[removedItem]) {
                delete categoryMap.value[removedItem]
                try {
                    await CategoryService.setItemCategory(removedItem, "")
                } catch (error) {
                    console.error('移除分类失败:', error)
                }
            }

            await ClipboardService.removeItem(index)
        } catch (error) {
            console.error('删除失败:', error)
        }
    }

    const moveSelection = (direction, contentRef) => {
        const visible = visibleHistory.value
        if (visible.length === 0) return
        let visibleIndex = visible.findIndex((entry) => entry.index === selectedIndex.value)
        if (visibleIndex < 0) visibleIndex = 0
        const nextVisibleIndex = Math.max(0, Math.min(visible.length - 1, visibleIndex + direction))
        updateSelection(visible[nextVisibleIndex].index, true, contentRef, nextVisibleIndex)
    }

    return {
        history,
        selectedIndex,
        searchKeyword,
        categoryFilter,
        categoryMap,
        visibleHistory,
        getItemCategory,
        updateSelection,
        deleteItem,
        moveSelection
    }
}
