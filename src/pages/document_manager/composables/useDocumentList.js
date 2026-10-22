import {computed, nextTick, ref} from 'vue'
import {ElMessage} from 'element-plus'
import {DocumentService} from '../../../services/ipc.js'
import {useI18n} from 'vue-i18n'

/**
 * useDocumentList — 文档列表与分页域
 * 负责：文件列表数据、分页状态（currentPage/pageLimit/total）、
 * 加载与刷新（loadFiles，含竞态序号保护）、单选/多选选择状态（selectedId/multiSelectedIds/selectedDoc）。
 * 查询所需的筛选条件（分类/根目录/关键词/扩展名/标签）由页面层以 ref 传入，
 * 列表加载后的文件网格拖拽初始化（initFileSortable）也由页面层注入，避免与分类域、拖拽域互相依赖。
 */
export function useDocumentList({categoryFilter, rootFilter, searchKeyword, fileExtFilter, tagFilter, initFileSortable}) {
    const {t} = useI18n()
    const items = ref([])
    const loading = ref(false)
    const currentPage = ref(1)
    const pageLimit = 50
    const total = ref(0)
    const selectedId = ref(null)
    const multiSelectedIds = ref(new Set())

    const selectedDoc = computed(() => {
        if (selectedId.value === null) return null
        return items.value.find(i => i.id === selectedId.value) || null
    })

    let loadFilesSeq = 0
    async function loadFiles(preserveSelection) {
        const seq = ++loadFilesSeq
        loading.value = true
        try {
            const r = await DocumentService.getPage({
                offset: (currentPage.value - 1) * pageLimit, limit: pageLimit,
                categoryId: categoryFilter.value === -1 ? -1 : categoryFilter.value, rootId: rootFilter.value,
                keyword: searchKeyword.value || null,
                fileExt: fileExtFilter.value || null,
                tags: (tagFilter.value && tagFilter.value.length) ? tagFilter.value : null,
            })
            if (seq !== loadFilesSeq) return
            items.value = r.items || []
            total.value = Number(r.total) || 0
            if (!preserveSelection || !items.value.find(i => i.id === selectedId.value)) {
                selectedId.value = null
            }
        } catch (e) {
            if (seq === loadFilesSeq) {
                ElMessage.error(t('documentManager.loadFileListFailed'))
            }
        } finally {
            if (seq === loadFilesSeq) {
                loading.value = false
            }
        }
        await nextTick()
        initFileSortable()
    }

    function onFileCardClick(item, event) {
        if (event.ctrlKey || event.metaKey) {
            const next = new Set(multiSelectedIds.value)
            if (next.has(item.id)) next.delete(item.id)
            else next.add(item.id)
            multiSelectedIds.value = next
            selectedId.value = item.id
            return
        }
        if (event.shiftKey && selectedId.value != null) {
            const ids = items.value.map(i => i.id)
            const a = ids.indexOf(selectedId.value)
            const b = ids.indexOf(item.id)
            if (a >= 0 && b >= 0) {
                const [lo, hi] = a < b ? [a, b] : [b, a]
                const next = new Set(multiSelectedIds.value)
                for (let i = lo; i <= hi; i++) next.add(ids[i])
                multiSelectedIds.value = next
            }
            selectedId.value = item.id
            return
        }
        multiSelectedIds.value = new Set()
        selectedId.value = item.id
    }

    return {
        items,
        loading,
        currentPage,
        pageLimit,
        total,
        selectedId,
        selectedDoc,
        multiSelectedIds,
        loadFiles,
        onFileCardClick,
    }
}
