import {computed, nextTick, ref} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {DocumentService} from '../../../services/ipc.js'
import {open as openDialog} from '@tauri-apps/plugin-dialog'
import {
    Coffee,
    Connection,
    Document,
    Folder,
    List,
    MagicStick,
    Monitor,
    Notebook,
    Picture,
    Search,
    Setting,
    Tickets,
} from '@element-plus/icons-vue'
import {useI18n} from 'vue-i18n'

/**
 * useCategoryTree — 分类/根目录域
 * 负责：分类与根目录数据（categories/roots/stats）、当前筛选高亮所需的计数计算
 * （catCountMap/uncatCount/visibleCategories）、分类图标/颜色候选项（catIcons/catColors/getCatIcon）、
 * 新增/重命名/删除分类与根目录的弹窗状态及 CRUD（confirmAddRoot/browseRootPath/confirmAddCategory/
 * removeCategoryFn/removeRootFn/startRenameCat/confirmRenameCat）、侧栏选中（selectRoot/selectCategory）、
 * 分类/根目录/统计数据的统一加载（loadData）。
 * 分类筛选值（categoryFilter）与根目录筛选值（rootFilter）由页面层持有并传入：
 * 二者同时被列表查询、导入弹窗、未管理文件检测使用，属跨域共用状态。
 * loadData 中的侧栏拖拽初始化（initRootSortable/initCatSortable）由页面层注入：
 * 拖拽排序与页面卸载清理逻辑耦合，保留在页面层。
 */
export function useCategoryTree({categoryFilter, rootFilter, loadFiles, initRootSortable, initCatSortable}) {
    const {t} = useI18n()
    const categories = ref([])
    const roots = ref([])
    const stats = ref(null)

    const showAddRoot = ref(false)
    const newRootName = ref('')
    const newRootPath = ref('')
    const showAddCategory = ref(false)
    const newCategoryName = ref('')
    const newCategoryIcon = ref('folder')
    const newCategoryColor = ref('#409EFF')

    const catIcons = [
        {value: 'folder', component: Folder},
        {value: 'document', component: Document},
        {value: 'notebook', component: Notebook},
        {value: 'tickets', component: Tickets},
        {value: 'setting', component: Setting},
        {value: 'connection', component: Connection},
        {value: 'magicstick', component: MagicStick},
        {value: 'monitor', component: Monitor},
        {value: 'picture', component: Picture},
        {value: 'coffee', component: Coffee},
        {value: 'search', component: Search},
        {value: 'list', component: List},
    ]

    const catColors = [
        '#409EFF', '#E74C3C', '#27AE60', '#E67E22', '#8E44AD',
        '#3498DB', '#1ABC9C', '#F39C12', '#E91E63', '#7F8C8D',
        '#00BCD4', '#795548', '#607D8B', '#FF5722', '#9B59B6',
    ]
    const showRenameCatDialog = ref(false)
    const renameCatId = ref(null)
    const renameCatName = ref('')

    function selectRoot(root, event) {
        if (event.target.closest('.dm-cat-more')) return
        rootFilter.value = root.id
    }

    function selectCategory(cat, event) {
        if (event.target.closest('.dm-cat-more')) return
        categoryFilter.value = categoryFilter.value === cat.id ? null : cat.id
    }

    function getCatIcon(name) {
        const ic = catIcons.find(i => i.value === name)
        return ic?.component || Folder
    }

    const catCountMap = computed(() => {
        const map = new Map()
        if (stats.value?.categoryCounts) {
            for (const c of stats.value.categoryCounts) {
                map.set(c.categoryId, c.count)
            }
        }
        return map
    })

    const uncatCount = computed(() => {
        if (!stats.value?.categoryCounts) return 0
        const e = stats.value.categoryCounts.find(c => c.categoryId === null)
        return e?.count || 0
    })

    const visibleCategories = computed(() => {
        if (rootFilter.value === null) return []
        return categories.value
    })

    async function loadData() {
        try {
            const [cats, rts, st] = await Promise.all([
                DocumentService.getCategories(rootFilter.value),
                DocumentService.getRoots(),
                DocumentService.getStats(rootFilter.value),
            ])
            categories.value = cats || []
            roots.value = rts || []
            stats.value = st
        } catch (e) {
            ElMessage.error(t('documentManager.loadFailed', {error: e}))
        }
        nextTick(() => {
            initRootSortable();
            initCatSortable()
        })
    }

    async function confirmAddRoot() {
        try {
            await DocumentService.addRoot(newRootName.value.trim(), newRootPath.value.trim());
            ElMessage.success(t('documentManager.added'));
            showAddRoot.value = false;
            newRootName.value = '';
            newRootPath.value = '';
            await loadData()
        } catch (e) {
            ElMessage.error(t('documentManager.addFailed', {error: e}))
        }
    }

    async function browseRootPath() {
        const p = await openDialog({directory: true, multiple: false});
        if (p) newRootPath.value = p
    }

    async function confirmAddCategory() {
        try {
            await DocumentService.addCategory(newCategoryName.value.trim(), newCategoryIcon.value, newCategoryColor.value, rootFilter.value);
            ElMessage.success(t('documentManager.added'));
            showAddCategory.value = false;
            newCategoryName.value = '';
            newCategoryIcon.value = 'folder';
            newCategoryColor.value = '#409EFF';
            await loadData()
        } catch (e) {
            ElMessage.error('添加失败: ' + e)
        }
    }

    async function removeCategoryFn(id) {
        try {
            await ElMessageBox.confirm(t('documentManager.deleteCategoryConfirm'), t('common.confirmDelete'));
            await DocumentService.removeCategory(id);
            ElMessage.success(t('documentManager.deleted'));
            await loadData();
            loadFiles()
        } catch (e) {
            if (e && e !== 'cancel' && e !== 'close') {
                ElMessage.error(typeof e === 'string' ? e : e.message || t('documentManager.deleteFailed'))
            }
        }
    }

    async function removeRootFn(id) {
        try {
            await ElMessageBox.confirm(t('documentManager.deleteDirConfirm'), t('common.confirmDelete'));
            await DocumentService.removeRoot(id);
            ElMessage.success(t('documentManager.deleted'));
            rootFilter.value = null;
            await loadData();
            loadFiles()
        } catch (e) {
            if (e && e !== 'cancel' && e !== 'close') {
                ElMessage.error(typeof e === 'string' ? e : e.message || t('documentManager.deleteFailed'))
            }
        }
    }

    function startRenameCat(cat) {
        renameCatId.value = cat.id;
        renameCatName.value = cat.name;
        showRenameCatDialog.value = true
    }

    async function confirmRenameCat() {
        try {
            await DocumentService.renameCategory(renameCatId.value, renameCatName.value.trim());
            ElMessage.success(t('documentManager.renamed'));
            showRenameCatDialog.value = false;
            await loadData();
            loadFiles()
        } catch (e) {
            ElMessage.error(t('documentManager.renameFailed', {error: e}))
        }
    }

    return {
        categories,
        roots,
        stats,
        catCountMap,
        uncatCount,
        visibleCategories,
        showAddRoot,
        newRootName,
        newRootPath,
        showAddCategory,
        newCategoryName,
        newCategoryIcon,
        newCategoryColor,
        catIcons,
        catColors,
        showRenameCatDialog,
        renameCatName,
        selectRoot,
        selectCategory,
        getCatIcon,
        loadData,
        confirmAddRoot,
        browseRootPath,
        confirmAddCategory,
        removeCategoryFn,
        removeRootFn,
        startRenameCat,
        confirmRenameCat,
    }
}
