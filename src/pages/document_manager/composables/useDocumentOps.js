import {computed, ref} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {DocumentService} from '../../../services/ipc.js'
import {useI18n} from 'vue-i18n'

/**
 * useDocumentOps — 单文档操作域
 * 负责：右键菜单（showContextMenu/closeCtxMenu）、打开文档与定位所在文件夹
 * （openDocument/openFolder）、删除确认（confirmDelete）、移动到其他根目录/分类的弹窗
 * （startMove/onMoveRootChange/confirmMove/hasMoveChange）。
 * 删除后需重置选中并刷新（selectedId/loadFiles/loadData），移动前需先保存详情面板的
 * pending 编辑（savePendingEdits），均由页面层注入。
 */
export function useDocumentOps({selectedId, loadFiles, loadData, savePendingEdits}) {
    const {t} = useI18n()
    const ctxMenuVisible = ref(false)
    const ctxMenuX = ref(0)
    const ctxMenuY = ref(0)
    const ctxMenuDoc = ref(null)
    const ctxAnchorId = ref(null)
    const showMoveDialog = ref(false)
    const moveDoc = ref(null)
    const moveTargetCategoryId = ref(null)
    const moveTargetRootId = ref(null)
    const moveCategories = ref([])

    const hasMoveChange = computed(() => {
        if (!moveDoc.value) return false
        return moveTargetCategoryId.value !== (moveDoc.value.categoryId ?? null)
            || (moveTargetRootId.value && moveTargetRootId.value !== moveDoc.value.rootId)
    })

    async function openDocument(doc) {
        try {
            await DocumentService.openDoc(doc.id)
        } catch (e) {
            ElMessage.error(t('documentManager.openFailed', {error: e}))
        }
    }

    async function openFolder(doc) {
        try {
            await DocumentService.openFolder(doc.id)
        } catch (e) {
            ElMessage.error(t('documentManager.openFailed', {error: e}))
        }
    }

    async function confirmDelete(doc) {
        try {
            const repoMsg = doc.storageMode === 'repo'
                ? t('documentManager.fileWillRestore', {path: doc.sourcePath})
                : t('documentManager.fileRemainOnDisk')
            await ElMessageBox.confirm(t('documentManager.deleteFileConfirm', {
                name: doc.title || doc.fileName,
                extra: repoMsg
            }), t('common.confirmDelete'), {type: 'warning'});
            await DocumentService.deleteDoc(doc.id, false);
            ElMessage.success(t('documentManager.deleted'));
            selectedId.value = null;
            await loadFiles();
            await loadData()
        } catch (e) {
            if (e && e !== 'cancel' && e !== 'close') {
                ElMessage.error(t('documentManager.deleteFailed'))
            }
        }
    }

    function showContextMenu(event, doc) {
        ctxMenuDoc.value = doc
        ctxMenuX.value = event.clientX
        ctxMenuY.value = event.clientY
        ctxMenuVisible.value = true
        ctxAnchorId.value = doc.id
    }

    function closeCtxMenu() {
        ctxMenuVisible.value = false
        ctxAnchorId.value = null
    }

    async function startMove(doc) {
        closeCtxMenu()
        moveDoc.value = doc
        moveTargetCategoryId.value = doc.categoryId ?? null
        moveTargetRootId.value = doc.rootId
        moveCategories.value = doc.rootId ? (await DocumentService.getCategories(doc.rootId) || []) : []
        showMoveDialog.value = true
    }

    async function onMoveRootChange(rootId) {
        moveTargetCategoryId.value = null
        moveCategories.value = rootId ? (await DocumentService.getCategories(rootId) || []) : []
    }

    async function contextDelete(doc) {
        closeCtxMenu()
        await confirmDelete(doc)
    }

    async function confirmMove() {
        if (!moveDoc.value || !hasMoveChange.value) return
        const doc = moveDoc.value
        await savePendingEdits()

        try {
            await DocumentService.atomicMoveDoc({
                id: doc.id,
                newRootId: moveTargetRootId.value !== doc.rootId ? moveTargetRootId.value : null,
                newCategoryId: moveTargetCategoryId.value ?? null,
            })
            ElMessage.success(t('documentManager.moveSuccess'))
            showMoveDialog.value = false
            await loadFiles(true)
            await loadData()
        } catch (e) {
            ElMessage.error(typeof e === 'string' ? e : e?.message || t('documentManager.moveFailed'))
        }
    }

    return {
        ctxMenuVisible,
        ctxMenuX,
        ctxMenuY,
        ctxMenuDoc,
        ctxAnchorId,
        showMoveDialog,
        moveDoc,
        moveTargetCategoryId,
        moveTargetRootId,
        moveCategories,
        hasMoveChange,
        openDocument,
        openFolder,
        confirmDelete,
        showContextMenu,
        closeCtxMenu,
        startMove,
        onMoveRootChange,
        contextDelete,
        confirmMove,
    }
}
