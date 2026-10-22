import {computed, reactive, ref} from 'vue'
import {ElMessage} from 'element-plus'
import {DocumentService} from '../../../services/ipc.js'
import {useI18n} from 'vue-i18n'

/**
 * useOrphanFiles — 未管理文件（孤儿文件）域
 * 负责：未管理文件检测（detectOrphans）、检测结果与勾选状态
 * （orphanResults/orphanSelected/totalOrphanCount）、侧栏横幅计数（orphanCount/orphanChecked）、
 * 单条/全选勾选（toggleOrphan/toggleOrphanSelectAll）、按根目录分组导入（importOrphans）。
 * 检测范围依赖当前根目录筛选（rootFilter），导入完成后需刷新分类数据与列表
 * （loadData/loadFiles），均由页面层注入。
 */
export function useOrphanFiles({rootFilter, loadData, loadFiles}) {
    const {t} = useI18n()
    const orphanCount = ref(0)
    const orphanChecked = ref(false)
    const orphanLoading = ref(false)
    const showOrphanDialog = ref(false)
    const orphanResults = ref([])
    const orphanSelected = reactive(new Set())

    const totalOrphanCount = computed(() => {
        let n = 0
        for (const r of orphanResults.value) n += r.files?.length || 0
        return n
    })

    async function detectOrphans() {
        orphanLoading.value = true
        try {
            const r = await DocumentService.detectOrphanFiles(rootFilter.value || null)
            orphanChecked.value = true
            orphanResults.value = r || []
            let count = 0
            for (const g of orphanResults.value) count += g.files?.length || 0
            orphanCount.value = count
            orphanSelected.clear()
        } catch (e) {
            ElMessage.error(t('documentManager.detectFailed', {error: e}))
        } finally {
            orphanLoading.value = false
        }
    }

    function toggleOrphan(path) {
        orphanSelected.has(path) ? orphanSelected.delete(path) : orphanSelected.add(path)
    }

    function toggleOrphanSelectAll() {
        if (orphanSelected.size === totalOrphanCount.value) {
            orphanSelected.clear()
        } else {
            for (const r of orphanResults.value) {
                for (const f of r.files) orphanSelected.add(f.path)
            }
        }
    }

    async function importOrphans() {
        if (orphanSelected.size === 0) return
        try {
            let total = 0
            let allErrors = []
            for (const result of orphanResults.value) {
                const selected = result.files.filter(f => orphanSelected.has(f.path))
                if (selected.length === 0) continue
                const groups = new Map()
                for (const f of selected) {
                    const key = `${result.rootId}:${f.categoryId ?? ''}`
                    if (!groups.has(key)) groups.set(key, [])
                    groups.get(key).push(f)
                }
                for (const [, files] of groups) {
                    const r = await DocumentService.importFiles({
                        paths: files.map(f => f.path),
                        rootId: result.rootId,
                        categoryId: files[0].categoryId || null,
                        storageMode: 'index',
                        sourceDir: ''
                    })
                    total += r.success?.length || 0
                    if (r.errors?.length) allErrors.push(...r.errors)
                }
            }
            if (allErrors.length > 0) {
                const detail = allErrors.slice(0, 5).join('\n')
                const more = allErrors.length > 5 ? `\n...等共 ${allErrors.length} 个错误` : ''
                ElMessage.warning({message: `导入完成，${total} 个成功，${allErrors.length} 个失败`, duration: 3000})
                ElMessage({message: detail + more, type: 'warning', duration: 6000, showClose: true})
            } else {
                ElMessage.success(`成功导入 ${total} 个文件`)
            }
            showOrphanDialog.value = false
            orphanSelected.clear()
            orphanCount.value = 0
            orphanResults.value = []
            await loadData()
            loadFiles()
        } catch (e) {
            ElMessage.error('导入失败: ' + e)
        }
    }

    return {
        orphanCount,
        orphanChecked,
        orphanLoading,
        showOrphanDialog,
        orphanResults,
        orphanSelected,
        totalOrphanCount,
        detectOrphans,
        toggleOrphan,
        toggleOrphanSelectAll,
        importOrphans,
    }
}
