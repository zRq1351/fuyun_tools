import {ref, watch} from 'vue'
import {ElMessage} from 'element-plus'
import {DocumentService} from '../../../services/ipc.js'
import {useI18n} from 'vue-i18n'

/**
 * useDocumentEdit — 文档详情面板的元数据编辑域
 * 负责：详情面板的分类/标签/备注编辑态（editCategoryId/editTags/editNotes）、
 * 选中文档变化时同步编辑表单（watch selectedDoc）、
 * 保存 pending 编辑（savePendingEdits，供移动文档前调用）、
 * 分类/标签/备注的分别保存（saveCategory/saveTags/saveNotes）。
 * 选中文档（selectedDoc）来自列表域，保存后需刷新分类数据与列表
 * （loadData/loadFiles），均由页面层注入。
 */
export function useDocumentEdit({selectedDoc, loadData, loadFiles}) {
    const {t} = useI18n()
    const editCategoryId = ref(null)
    const editTags = ref('')
    const editNotes = ref('')

    watch(selectedDoc, (doc) => {
        if (!doc) return
        editCategoryId.value = doc.categoryId ?? null
        editTags.value = (() => {
            try {
                return JSON.parse(doc.tags || '[]').join('; ')
            } catch {
                return doc.tags || ''
            }
        })()
        editNotes.value = doc.notes || ''
    })

    async function savePendingEdits() {
        if (!selectedDoc.value) return
        const doc = selectedDoc.value
        const currentTags = JSON.stringify(editTags.value.split(/[,，;；]/).map(t => t.trim()).filter(Boolean))
        const currentNotes = editNotes.value
        if (currentTags !== (doc.tags || '[]')) {
            await DocumentService.updateMeta({id: doc.id, tags: currentTags}).catch(() => {
            })
        }
        if (currentNotes !== (doc.notes || '')) {
            await DocumentService.updateMeta({id: doc.id, notes: currentNotes}).catch(() => {
            })
        }
        doc.tags = currentTags
        doc.notes = currentNotes
    }

    async function saveCategory() {
        if (!selectedDoc.value) return;
        await savePendingEdits()
        try {
            await DocumentService.updateMeta({id: selectedDoc.value.id, categoryId: editCategoryId.value ?? -1})
            selectedDoc.value.categoryId = editCategoryId.value
        } catch (e) {
            ElMessage.error(t('documentManager.saveCategoryFailed'))
        }
        await loadData()
        await loadFiles(true)
    }

    async function saveTags() {
        if (!selectedDoc.value) return
        try {
            const tagsJson = JSON.stringify(editTags.value.split(/[,，;；]/).map(t => t.trim()).filter(Boolean))
            await DocumentService.updateMeta({id: selectedDoc.value.id, tags: tagsJson})
            selectedDoc.value.tags = tagsJson
        } catch (e) {
            ElMessage.error(t('documentManager.saveTagsFailed'))
        }
    }

    async function saveNotes() {
        if (!selectedDoc.value) return;
        try {
            await DocumentService.updateMeta({id: selectedDoc.value.id, notes: editNotes.value})
            selectedDoc.value.notes = editNotes.value
        } catch (e) {
            ElMessage.error(t('documentManager.saveNotesFailed'))
        }
    }

    return {
        editCategoryId,
        editTags,
        editNotes,
        savePendingEdits,
        saveCategory,
        saveTags,
        saveNotes,
    }
}
