import {ref} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {DocumentService} from '../../../services/ipc.js'
import {useI18n} from 'vue-i18n'

/**
 * useTagManager — 标签治理域
 * 负责：全站标签列表（allTags）、标签管理弹窗显隐（tagManagerVisible）、
 * 批量打标/移除/替换（promptBatchTags）、标签改名合并（renameTag）、标签删除（removeTag）、
 * 打开标签管理（openTagManager）。
 * 多选集合（multiSelectedIds）与列表刷新（loadFiles）由页面层注入：
 * 多选状态归属列表域，刷新动作归属列表域，均跨域共用。
 */
export function useTagManager({multiSelectedIds, loadFiles}) {
    const {t} = useI18n()
    const allTags = ref([])
    const tagManagerVisible = ref(false)

    async function loadAllTags() {
        try {
            allTags.value = await DocumentService.listTags() || []
        } catch (_) {
            allTags.value = []
        }
    }

    async function promptBatchTags(mode) {
        if (multiSelectedIds.value.size === 0) return
        if (mode === 'remove' || mode === 'replace') {
            // still need tag text
        }
        const {value} = await ElMessageBox.prompt(
            t('documentManager.tagsInputPrompt'),
            t(mode === 'add' ? 'documentManager.batchAddTags' : mode === 'remove' ? 'documentManager.batchRemoveTags' : 'documentManager.batchReplaceTags'),
            {inputPattern: /\S/, inputErrorMessage: t('documentManager.tagsInputPrompt')}
        ).catch(() => ({value: null}))
        if (value == null) return
        const tags = String(value).split(/[,，;；]/).map(s => s.trim()).filter(Boolean)
        try {
            const result = await DocumentService.batchUpdateTags({
                ids: Array.from(multiSelectedIds.value),
                mode,
                tags
            })
            const failed = result?.failed?.length || 0
            if (failed > 0) {
                ElMessage.warning(t('documentManager.batchTagsPartial', {n: failed}))
            } else {
                ElMessage.success(t('documentManager.batchTagsUpdated', {n: result?.updated || 0}))
            }
            multiSelectedIds.value = new Set()
            await Promise.all([loadFiles(true), loadAllTags()])
        } catch (e) {
            ElMessage.error(String(e?.message || e))
        }
    }

    async function openTagManager() {
        await loadAllTags()
        tagManagerVisible.value = true
    }

    async function renameTag(tag) {
        const {value} = await ElMessageBox.prompt(
            t('documentManager.tagRenameTo'),
            `${t('documentManager.tagRename')}: ${tag}`,
            {inputPattern: /\S/, inputErrorMessage: t('documentManager.tagRenameTo')}
        ).catch(() => ({value: null}))
        if (value == null) return
        try {
            await DocumentService.renameTag(tag, String(value).trim())
            await Promise.all([loadFiles(true), loadAllTags()])
        } catch (e) {
            ElMessage.error(String(e?.message || e))
        }
    }

    async function removeTag(tag) {
        try {
            await ElMessageBox.confirm(
                `${t('documentManager.tagRemove')}: ${tag}?`,
                t('documentManager.tagManager')
            )
        } catch (_) {
            return
        }
        try {
            await DocumentService.removeTag(tag)
            await Promise.all([loadFiles(true), loadAllTags()])
        } catch (e) {
            ElMessage.error(String(e?.message || e))
        }
    }

    return {
        allTags,
        tagManagerVisible,
        loadAllTags,
        promptBatchTags,
        openTagManager,
        renameTag,
        removeTag,
    }
}
