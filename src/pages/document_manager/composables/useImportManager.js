import {reactive, ref, watch} from 'vue'
import {ElMessage} from 'element-plus'
import {DocumentService} from '../../../services/ipc.js'
import {open as openDialog} from '@tauri-apps/plugin-dialog'
import {useI18n} from 'vue-i18n'

/**
 * useImportManager — 文件导入/文件夹扫描/导入历史域
 * 负责：导入弹窗与扫描弹窗的状态及流程（openImportDialog/browseImportFiles/confirmImport/
 * openScanDialog/browseScanPath/runScan/toggleScanSelect/toggleScanSelectAll/importScanned）、
 * 弹窗内分类下拉的联动加载（watch importRootId/scanImportRootId，共用 categoryLoadSeq 竞态序号）、
 * 导入历史面板（importHistory/loadImportHistory/toggleHistoryFiles/undoImportFn/undoImportItemFn）、
 * 拖拽入框的导入落点处理（handleDrop/handleDroppedPaths）。
 * 根目录/分类数据（roots/categories）、当前筛选（rootFilter/categoryFilter）、拖拽蒙层状态（dragover）
 * 以及导入完成后的数据刷新（loadData/loadFiles）由页面层注入，均跨域共用。
 */
export function useImportManager({roots, rootFilter, categoryFilter, categories, dragover, loadData, loadFiles}) {
    const {t} = useI18n()
    const importHistory = ref([])
    const showImportDialog = ref(false)
    const importRootId = ref(null)
    const importCategoryId = ref(null)
    const importFiles = ref([])
    const importMode = ref('index')
    const importCategories = ref([])
    const importing = ref(false)
    const showScanDialog = ref(false)
    const scanPath = ref('')
    const scanImportRootId = ref(null)
    const scanCategoryId = ref(null)
    const scanning = ref(false)
    const scannedFiles = ref([])
    const scanSelected = reactive(new Set())
    const scanCategories = ref([])
    const scanImporting = ref(false)

    let categoryLoadSeq = 0
    watch(importRootId, async (rid) => {
        importCategoryId.value = null
        const seq = ++categoryLoadSeq
        const list = rid ? (await DocumentService.getCategories(rid) || []) : []
        if (seq !== categoryLoadSeq) return
        importCategories.value = list
    })

    watch(scanImportRootId, async (rid) => {
        scanCategoryId.value = null
        const seq = ++categoryLoadSeq
        const list = rid ? (await DocumentService.getCategories(rid) || []) : []
        if (seq !== categoryLoadSeq) return
        scanCategories.value = list
    })

    async function loadImportHistory() {
        try {
            importHistory.value = await DocumentService.getImportHistory(20)
        } catch (e) {
        }
    }

    async function toggleHistoryFiles(importId) {
        const h = importHistory.value.find(h => h.id === importId)
        if (!h) return
        if (h._files) {
            h._files = null;
            return
        }
        try {
            h._files = await DocumentService.getImportFiles(importId)
        } catch (e) {
            ElMessage.error(t('documentManager.loadFileListFailed'))
        }
    }

    async function undoImportFn(importId) {
        try {
            const errors = await DocumentService.undoImport(importId)
            if (errors && errors.length > 0) ElMessage.warning(t('documentManager.partialUndoFailed'))
            else ElMessage.success(t('documentManager.undone'))
            await loadData();
            await loadFiles();
            loadImportHistory()
        } catch (e) {
            ElMessage.error('撤销失败: ' + e)
        }
    }

    async function undoImportItemFn(importId, docFileId, historyItem) {
        try {
            await DocumentService.undoImportItem(importId, docFileId)
            ElMessage.success(t('documentManager.undone'))
            if (historyItem._files) {
                const updated = await DocumentService.getImportFiles(importId)
                historyItem._files = updated || []
                historyItem.fileCount = historyItem._files.length
            }
            if (historyItem.fileCount === 0) {
                importHistory.value = importHistory.value.filter(h => h.id !== importId)
            }
            await loadData();
            await loadFiles()
        } catch (e) {
            ElMessage.error(t('documentManager.undoFailed', {error: e}))
        }
    }

    async function browseImportFiles() {
        const p = await openDialog({multiple: true, directory: false});
        if (p && Array.isArray(p)) importFiles.value = p
    }

    function getImportTargetPath() {
        const r = roots.value.find(r => r.id === importRootId.value);
        if (!r) return ''
        const c = categories.value.find(c => c.id === importCategoryId.value)
        return r.rootPath.replace(/\\/g, '/') + '/' + (c ? c.name : '未分类') + '/'
    }

    async function openImportDialog() {
        if (importing.value) return
        importRootId.value = rootFilter.value || roots.value[0]?.id || null
        importCategoryId.value = null
        importFiles.value = []
        showImportDialog.value = true
        importCategories.value = importRootId.value ? (await DocumentService.getCategories(importRootId.value) || []) : []
    }

    async function openScanDialog() {
        if (scanImporting.value) return
        scanImportRootId.value = rootFilter.value || roots.value[0]?.id || null
        scanCategoryId.value = null
        scanPath.value = ''
        scannedFiles.value = []
        scanSelected.clear()
        showScanDialog.value = true
        scanCategories.value = scanImportRootId.value ? (await DocumentService.getCategories(scanImportRootId.value) || []) : []
    }

    async function confirmImport() {
        if (!importRootId.value || importFiles.value.length === 0) return
        importing.value = true
        try {
            const r = await DocumentService.importFiles({
                paths: importFiles.value,
                rootId: importRootId.value,
                categoryId: importCategoryId.value || null,
                storageMode: importMode.value,
                sourceDir: ''
            })
            if (r.errors && r.errors.length > 0) {
                const detail = r.errors.slice(0, 5).join('\n')
                const more = r.errors.length > 5 ? `\n...等共 ${r.errors.length} 个错误` : ''
                ElMessage.warning({
                    message: t('documentManager.importPartial', {
                        success: r.success.length,
                        errors: r.errors.length
                    }), duration: 3000
                })
                ElMessage({message: detail + more, type: 'warning', duration: 6000, showClose: true})
            } else {
                ElMessage.success(t('documentManager.importSuccess', {count: r.success.length}))
            }
            showImportDialog.value = false;
            importFiles.value = [];
            await loadData();
            loadFiles();
            loadImportHistory()
        } catch (e) {
            ElMessage.error(t('documentManager.importFailed', {error: e}))
        } finally {
            importing.value = false
        }
    }

    async function browseScanPath() {
        const p = await openDialog({directory: true, multiple: false});
        if (p) {
            scanPath.value = p;
            runScan()
        }
    }

    async function runScan() {
        scanning.value = true
        try {
            const r = await DocumentService.scanFolder(scanPath.value, true);
            scannedFiles.value = r.files || [];
            scanSelected.clear()
        } catch (e) {
            ElMessage.error(t('documentManager.scanFailed', {error: e}))
        } finally {
            scanning.value = false
        }
    }

    function toggleScanSelect(path) {
        scanSelected.has(path) ? scanSelected.delete(path) : scanSelected.add(path)
    }

    function toggleScanSelectAll() {
        if (scanSelected.size === scannedFiles.value.length) scanSelected.clear()
        else scannedFiles.value.forEach(f => scanSelected.add(f.path))
    }

    async function importScanned() {
        if (scanSelected.size === 0 || !scanImportRootId.value) return
        scanImporting.value = true
        try {
            const r = await DocumentService.importFiles({
                paths: Array.from(scanSelected),
                rootId: scanImportRootId.value,
                categoryId: scanCategoryId.value || null,
                storageMode: importMode.value,
                sourceDir: scanPath.value
            })
            if (r.errors && r.errors.length > 0) {
                const detail = r.errors.slice(0, 5).join('\n')
                const more = r.errors.length > 5 ? `\n...等共 ${r.errors.length} 个错误` : ''
                ElMessage.warning({
                    message: t('documentManager.importPartial', {
                        success: r.success.length,
                        errors: r.errors.length
                    }), duration: 3000
                })
                ElMessage({message: detail + more, type: 'warning', duration: 6000, showClose: true})
            } else {
                ElMessage.success(t('documentManager.importSuccess', {count: r.success.length}));
            }
            scanSelected.clear();
            scannedFiles.value = [];
            await loadData();
            loadFiles();
            loadImportHistory()
        } catch (e) {
            ElMessage.error(t('documentManager.importFailed', {error: e}))
        } finally {
            scanImporting.value = false
        }
    }

    function handleDroppedPaths(paths) {
        if (!paths || paths.length === 0) return
        if (!roots.value || roots.value.length === 0) {
            ElMessage.warning(t('documentManager.addDirFirst'));
            return
        }
        importRootId.value = rootFilter.value || roots.value[0].id;
        importCategoryId.value = categoryFilter.value === -1 ? null : categoryFilter.value;
        importFiles.value = paths;
        importMode.value = 'index';
        showImportDialog.value = true
    }

    async function handleDrop(event) {
        dragover.value = false;
        const files = event.dataTransfer?.files;
        if (!files || files.length === 0) return
        if (!roots.value || roots.value.length === 0) {
            ElMessage.warning(t('documentManager.addDirFirst'));
            return
        }
        const paths = [];
        for (let i = 0; i < files.length; i++) {
            if (files[i].path) paths.push(files[i].path)
        }
        if (paths.length === 0) return
        importRootId.value = rootFilter.value || roots.value[0].id;
        importCategoryId.value = categoryFilter.value === -1 ? null : categoryFilter.value;
        importFiles.value = paths;
        importMode.value = 'index';
        showImportDialog.value = true
    }

    return {
        importHistory,
        showImportDialog,
        importRootId,
        importCategoryId,
        importFiles,
        importMode,
        importCategories,
        importing,
        showScanDialog,
        scanPath,
        scanImportRootId,
        scanCategoryId,
        scanning,
        scannedFiles,
        scanSelected,
        scanCategories,
        scanImporting,
        loadImportHistory,
        toggleHistoryFiles,
        undoImportFn,
        undoImportItemFn,
        browseImportFiles,
        getImportTargetPath,
        openImportDialog,
        openScanDialog,
        confirmImport,
        browseScanPath,
        toggleScanSelect,
        toggleScanSelectAll,
        importScanned,
        handleDroppedPaths,
        handleDrop,
    }
}
