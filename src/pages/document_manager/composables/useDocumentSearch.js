import {ref} from 'vue'
import {ElMessage} from 'element-plus'
import {DocumentService} from '../../../services/ipc.js'
import {useI18n} from 'vue-i18n'

/**
 * useDocumentSearch — FTS 全文检索域
 * 负责：搜索关键词状态（searchKeyword）、搜索结果摘要高亮（snippetParts）、
 * FTS 索引重建（rebuildFtsIndex）。
 * 关键词防抖（250ms）与筛选变化触发的重载留在页面层：其定时器由 onBeforeUnmount 统一清理，
 * 且重载动作横跨列表域与标签域，属于跨域粘合逻辑。
 */
export function useDocumentSearch() {
    const {t} = useI18n()
    const searchKeyword = ref('')

    function snippetParts(snippet, keyword) {
        const text = String(snippet || '')
        const kw = String(keyword || '').trim()
        if (!text) return []
        if (!kw) return [{text, hit: false}]
        const lower = text.toLowerCase()
        const lowerKw = kw.toLowerCase()
        const parts = []
        let i = 0
        while (i < text.length) {
            const idx = lower.indexOf(lowerKw, i)
            if (idx < 0) {
                parts.push({text: text.slice(i), hit: false})
                break
            }
            if (idx > i) parts.push({text: text.slice(i, idx), hit: false})
            parts.push({text: text.slice(idx, idx + kw.length), hit: true})
            i = idx + kw.length
        }
        return parts
    }

    async function rebuildFtsIndex() {
        try {
            await DocumentService.rebuildFts()
            ElMessage.success(t('documentManager.rebuildFtsDone'))
        } catch (e) {
            ElMessage.error(String(e?.message || e))
        }
    }

    return {
        searchKeyword,
        snippetParts,
        rebuildFtsIndex,
    }
}
