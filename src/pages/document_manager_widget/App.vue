<template>
  <div class="dmw-root">
    <div class="dmw-container" @mouseleave="onContainerMouseLeave">
      <div class="dmw-header" @mousedown.left.prevent="handleHeaderDrag">
        <div class="dmw-header-left">
          <el-icon :size="14">
            <FolderOpened/>
          </el-icon>
          <span class="dmw-title">{{ t('settings.docManager.title') }}</span>
        </div>
        <div class="dmw-stats">
          <span class="dmw-stat">{{ stats?.totalFiles ?? 0 }} {{ t('common.files') }}</span>
          <span class="dmw-stat-sep">·</span>
          <span class="dmw-stat">{{ formatSize(stats?.totalSize) }}</span>
        </div>
        <div class="dmw-header-actions">
          <button :title="t('common.refresh')" class="dmw-btn-icon" @click="refreshData" @mousedown.stop>
            <el-icon :size="13">
              <Refresh/>
            </el-icon>
          </button>
          <button :title="t('common.settings')" class="dmw-btn-icon" @click="openFullManager" @mousedown.stop>
            <el-icon :size="13">
              <Setting/>
            </el-icon>
          </button>
          <button :title="t('common.close')" class="dmw-btn-icon dmw-btn-close" @click="closeWidget" @mousedown.stop>
            <el-icon :size="13">
              <Close/>
            </el-icon>
          </button>
        </div>
      </div>

      <div v-if="loading" class="dmw-loading">
        <el-icon :size="20" class="is-loading">
          <Loading/>
        </el-icon>
      </div>
      <template v-else>
        <div ref="rootScrollRef" class="dmw-roots" @mousedown="onDragStart($event, rootScrollRef)"
             @mouseenter="onRootsMouseEnter">
          <div
              v-for="root in roots"
              :key="root.id"
              :class="['dmw-root-tab', { active: selectedRootId === root.id }]"
              @click="selectRoot(root.id)"
          >
            <el-icon :size="13">
              <Folder/>
            </el-icon>
            <span class="dmw-root-name">{{ root.name }}</span>
          </div>
        </div>
        <div ref="catScrollRef" class="dmw-categories" @mousedown="onDragStart($event, catScrollRef)"
             @mouseenter="onCategoriesMouseEnter">
          <div
              v-for="cat in categories"
              :key="cat.id"
              :class="['dmw-cat-card', { active: selectedCategoryId === cat.id, 'drag-over': dragOverCatId === cat.id }]"
              :data-cat-id="cat.id"
              @mouseenter="onCatCardEnter(cat.id)"
          >
            <div :style="{ background: cat.color + '22', color: cat.color }" class="dmw-cat-icon">
              <el-icon :size="18">
                <component :is="getCatIcon(cat.icon)"/>
              </el-icon>
            </div>
            <div class="dmw-cat-info">
              <span class="dmw-cat-name">{{ cat.name }}</span>
              <span class="dmw-cat-count">{{ catCount(cat.id) }} {{ t('common.files') }}</span>
            </div>
          </div>
          <div
              :class="['dmw-cat-card', { active: selectedCategoryId === -1, 'drag-over': dragOverCatId === -1 }]"
              :data-cat-id="-1"
              @mouseenter="onCatCardEnter(-1)"
          >
            <div :style="{ background: 'var(--fy-bg-hover)', color: 'var(--fy-text-muted)' }" class="dmw-cat-icon">
              <el-icon :size="18">
                <Folder/>
              </el-icon>
            </div>
            <div class="dmw-cat-info">
              <span class="dmw-cat-name">{{ t('documentManager.uncategorized') }}</span>
              <span class="dmw-cat-count">{{ uncatCount }} {{ t('common.files') }}</span>
            </div>
          </div>
        </div>

        <div v-show="hovered" class="dmw-files">
          <div v-if="displayFiles.length === 0" class="dmw-empty">
            {{ t('documentManager.noDocs') }}
          </div>
          <div
              v-for="file in displayFiles"
              :key="file.id"
              :title="file.title || file.fileName"
              :class="['dmw-file-item', { active: ctxMenuFile?.id === file.id }]"
              @dblclick="openFile(file)"
              @contextmenu.prevent="showFileMenu($event, file)"
          >
            <el-icon :color="getFileColor(file.fileExt)" :size="16">
              <component :is="getFileIcon(file.fileExt)"/>
            </el-icon>
            <span class="dmw-file-name">{{ file.title || file.fileName }}</span>
            <span class="dmw-file-size">{{ formatSize(file.fileSize) }}</span>
            <span class="dmw-file-ext">{{ file.fileExt.toUpperCase() }}</span>
          </div>
        </div>
      </template>
    </div>

    <ContextMenu :show="ctxMenuShow" :x="ctxMenuX" :y="ctxMenuY" @close="ctxMenuShow = false; ctxMenuFile = null">
      <div class="context-menu-item" @click="openFile(ctxMenuFile)">{{ t('documentManager.open') }}</div>
      <div class="context-menu-divider"></div>
      <div class="context-menu-item context-menu-item-danger" @click="deleteFile(ctxMenuFile)">{{
          t('common.delete')
        }}
      </div>
    </ContextMenu>
  </div>
</template>

<script setup>
import {computed, nextTick, onMounted, onBeforeUnmount, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {useWindowDrag} from '../../composables/useWindowDrag'
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow'
import {LogicalSize, PhysicalPosition} from '@tauri-apps/api/dpi'
import {currentMonitor} from '@tauri-apps/api/window'
import {DocumentService} from '../../services/ipc.js'
import ContextMenu from '../../components/ContextMenu.vue'
import {
  Close, Folder, FolderOpened, Loading, Refresh,
  Document, List, Notebook, Tickets, Setting, Connection,
  MagicStick, Monitor, Picture, Coffee, Search
} from '@element-plus/icons-vue'

const {t} = useI18n()
const {startDrag} = useWindowDrag()
const appWindow = getCurrentWebviewWindow()

const stats = ref(null)
const roots = ref([])
const selectedRootId = ref(null)
const categories = ref([])
const allFiles = ref([])
const selectedCategoryId = ref(null)
const loading = ref(true)
const hovered = ref(false)
const rootScrollRef = ref(null)
const catScrollRef = ref(null)
const ctxMenuShow = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)
const ctxMenuFile = ref(null)
const dialogOpen = ref(false)

const iconMap = {
  pdf: Document, docx: Document, doc: Document,
  xlsx: List, xls: List, pptx: Notebook, ppt: Notebook,
  txt: Tickets, md: Notebook, csv: List, log: Tickets,
  json: Setting, xml: Document, yaml: Setting, yml: Setting, toml: Setting,
  py: Monitor, js: Monitor, ts: Monitor, jsx: Monitor, tsx: Monitor,
  java: Coffee, go: Monitor, rs: Monitor, c: Monitor, cpp: Monitor, cs: Monitor,
  php: Monitor, rb: Monitor, swift: Monitor, kt: Monitor, scala: Monitor,
  lua: Monitor, r: Monitor, zig: Monitor,
  html: Connection, htm: Connection, css: MagicStick, scss: MagicStick, less: MagicStick,
  vue: Monitor, svelte: Monitor,
  png: Picture, jpg: Picture, jpeg: Picture, gif: Picture, bmp: Picture, webp: Picture, svg: Picture,
  sh: Monitor, bat: Monitor, ps1: Monitor, sql: List,
  ini: Setting, cfg: Setting, conf: Setting,
}

const fileColorMap = {
  pdf: '#E74C3C', docx: '#2980B9', doc: '#2980B9',
  xlsx: '#27AE60', xls: '#27AE60', pptx: '#E67E22', ppt: '#E67E22',
  txt: '#7F8C8D', md: '#8E44AD', csv: '#27AE60', log: '#7F8C8D',
  json: '#E67E22', xml: '#E67E22', yaml: '#E67E22', yml: '#E67E22', toml: '#E67E22',
  py: '#3498DB', js: '#F1C40F', ts: '#3178C6', jsx: '#F1C40F', tsx: '#3178C6',
  java: '#E76F00', go: '#00ADD8', rs: '#DEA584', c: '#555', cpp: '#649AD2', cs: '#68217A',
  php: '#777BB3', rb: '#CC342D', swift: '#F05138', kt: '#7F52FF', scala: '#DC322F',
  lua: '#000080', r: '#276DC3', zig: '#F7A41D',
  html: '#E34F26', htm: '#E34F26', css: '#1572B6', scss: '#CD6799', less: '#1D365D',
  vue: '#42B883', svelte: '#FF3E00',
  png: '#9B59B6', jpg: '#9B59B6', jpeg: '#9B59B6', gif: '#3498DB', bmp: '#7F8C8D', webp: '#9B59B6', svg: '#E67E22',
  sh: '#4EAA25', bat: '#555', ps1: '#012456', sql: '#E38C00',
  ini: '#7F8C8D', cfg: '#7F8C8D', conf: '#7F8C8D',
}

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

function getFileIcon(ext) {
  return iconMap[ext] || Document
}

function getFileColor(ext) {
  return fileColorMap[ext] || '#7F8C8D'
}

function getCatIcon(name) {
  const ic = catIcons.find(i => i.value === name)
  return ic?.component || Folder
}

function formatSize(bytes) {
  const n = Number(bytes)
  if (!Number.isFinite(n)) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB']
  let i = 0, s = n
  while (s >= 1024 && i < 3) {
    s /= 1024;
    i++
  }
  return s.toFixed(i > 0 ? 1 : 0) + ' ' + u[i]
}

const countByCategory = computed(() => {
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

function catCount(catId) {
  return countByCategory.value.get(catId) || 0
}

const displayFiles = computed(() => {
  if (selectedCategoryId.value === null) {
    return []
  }
  if (selectedCategoryId.value === -1) {
    return allFiles.value.filter(f => f.categoryId == null).slice(0, 30)
  }
  return allFiles.value.filter(f => f.categoryId === selectedCategoryId.value).slice(0, 30)
})

async function resizeToFitContent() {
  await nextTick()
  await new Promise(r => setTimeout(r, 0))
  const container = document.querySelector('.dmw-container')
  if (!container) return
  let total = 0
  for (const child of container.children) {
    if (child.classList.contains('dmw-files')) {
      total += child.scrollHeight
    } else {
      total += child.offsetHeight
    }
  }
  if (total <= 0) return
  const screenHeight = window.screen?.availHeight ?? window.innerHeight
  const maxH = Math.max(Math.round(screenHeight / 2), 80)
  const h = Math.min(total, maxH)
  try {
    await appWindow.setSize(new LogicalSize(380, h))
    await snapWindowPosition()
  } catch (e) {
    console.error('调整小部件窗口大小失败:', e)
  }
}

let isDown = false
let isDragging = false
let dragStartX = 0
let dragScrollLeft = 0
let dragScrollEl = null

function onDragStart(e, scrollRef) {
  if (e.button !== 0) return
  const el = scrollRef
  if (!el) return
  isDown = true
  isDragging = false
  dragStartX = e.pageX
  dragScrollLeft = el.scrollLeft
  dragScrollEl = el
  window.addEventListener('mousemove', onDragMove)
  window.addEventListener('mouseup', onDragEnd)
}

function onDragMove(e) {
  if (!isDown) return
  const walk = e.pageX - dragStartX
  if (!isDragging && Math.abs(walk) > 4) {
    isDragging = true
    document.body.style.userSelect = 'none'
    if (window.getSelection) window.getSelection().removeAllRanges()
  }
  if (!isDragging) return
  e.preventDefault()
  if (dragScrollEl) dragScrollEl.scrollLeft = dragScrollLeft - walk
}

function onDragEnd() {
  if (!isDown) return
  isDown = false
  isDragging = false
  document.body.style.removeProperty('user-select')
  window.removeEventListener('mousemove', onDragMove)
  window.removeEventListener('mouseup', onDragEnd)
  dragScrollEl = null
}

const dragOverCatId = ref(null)

async function importDroppedFiles(paths, catId) {
  if (!paths.length || !selectedRootId.value) return
  let mode
  dialogOpen.value = true
  try {
    const {ElMessageBox} = await import('element-plus')
    const action = await ElMessageBox.confirm(
        '',
        t('documentManager.importMode'),
        {
          confirmButtonText: t('documentManager.modeMigrateShort'),
          cancelButtonText: t('documentManager.modeIndexShort'),
          distinguishCancelAndClose: true,
          closeOnClickModal: true,
          customClass: 'fy-compact',
          confirmButtonClass: 'btn-migrate',
        }
    )
    mode = 'repo'
  } catch (e) {
    if (e === 'cancel') mode = 'index'
    else {
      return
    }
  } finally {
    dialogOpen.value = false
  }
  try {
    const result = await DocumentService.importFiles({
      paths,
      rootId: selectedRootId.value,
      categoryId: catId === -1 ? null : catId,
      storageMode: mode,
      sourceDir: '',
    })
    const {ElNotification} = await import('element-plus')
    if (result?.success?.length) {
      ElNotification({
        title: '',
        message: `+${result.success.length} ${t('common.files')}`,
        type: 'success',
        duration: 3000,
        customClass: 'fy-compact',
        offset: 60
      })
    }
    await loadDataForRoot(selectedRootId.value)
    if (catId !== -1 && categories.value.some(c => c.id === catId)) {
      selectedCategoryId.value = catId
    } else if (catId === -1) {
      selectedCategoryId.value = -1
    }
    hovered.value = true
    await nextTick()
    await resizeToFitContent()
    if (result?.errors?.length) {
      ElNotification({
        title: t('documentManager.importFailed'),
        message: result.errors.join('\n'),
        type: 'error',
        duration: 5000,
        customClass: 'fy-compact',
        offset: 60
      })
    }
  } catch (err) {
    const {ElNotification} = await import('element-plus')
    ElNotification({
      title: t('documentManager.importFailed'),
      message: String(err),
      type: 'error',
      duration: 8000,
      customClass: 'fy-compact',
      offset: 60
    })
  }
}

function onCatCardEnter(id) {
  if (isDragging) return
  selectCategory(id)
}

async function loadDataForRoot(rootId) {
  const [cats, st] = await Promise.all([
    DocumentService.getCategories(rootId),
    DocumentService.getStats(rootId),
  ])
  categories.value = cats || []
  stats.value = st
  const r = await DocumentService.getPage({
    offset: 0, limit: 100,
    categoryId: null,
    rootId: rootId,
    keyword: null, fileExt: null,
  })
  allFiles.value = r.items || []
  if (categories.value.length > 0) {
    selectedCategoryId.value = categories.value[0].id
  } else {
    selectedCategoryId.value = null
  }
}

async function onRootsMouseEnter() {
  hovered.value = true
  await nextTick()
  await resizeToFitContent()
}

async function onCategoriesMouseEnter() {
  hovered.value = true
  await nextTick()
  await resizeToFitContent()
}

async function onContainerMouseLeave() {
  if (ctxMenuShow.value || dialogOpen.value) return
  hovered.value = false
  await nextTick()
  await resizeToFitContent()
}

async function selectRoot(id) {
  if (selectedRootId.value === id) return
  selectedRootId.value = id
  hovered.value = true
  await loadDataForRoot(id)
  await nextTick()
  await resizeToFitContent()
}

async function selectCategory(id) {
  selectedCategoryId.value = id
  await nextTick()
  await resizeToFitContent()
}

async function openFile(file) {
  ctxMenuShow.value = false
  if (!file?.id) return
  try {
    await DocumentService.openDoc(file.id)
  } catch (e) {
    console.error('打开文件失败:', e)
  }
}

async function openFullManager() {
  try {
    const {invoke} = await import('@tauri-apps/api/core')
    await invoke('show_document_manager')
  } catch (e) {
    console.error('打开文档管理器失败:', e)
  }
}

async function closeWidget() {
  try {
    const {invoke} = await import('@tauri-apps/api/core')
    await invoke('hide_doc_manager_widget')
  } catch (e) {
    console.error('关闭小部件失败:', e)
  }
}

async function refreshData() {
  if (!selectedRootId.value) return
  loading.value = true
  await loadDataForRoot(selectedRootId.value)
  hovered.value = true
  await nextTick()
  await resizeToFitContent()
  loading.value = false
}

function showFileMenu(event, file) {
  ctxMenuX.value = event.clientX
  ctxMenuY.value = event.clientY
  ctxMenuFile.value = file
  ctxMenuShow.value = true
}

async function deleteFile(file) {
  ctxMenuShow.value = false
  if (!file?.id) return
  dialogOpen.value = true
  try {
    const {ElMessageBox} = await import('element-plus')
    await ElMessageBox.confirm(
        t('documentManager.confirmDelete'),
        t('common.delete'),
        {
          confirmButtonText: t('common.delete'),
          cancelButtonText: t('common.cancel'),
          type: 'warning',
          customClass: 'fy-compact'
        }
    )
    await DocumentService.deleteDoc(file.id, false)
    allFiles.value = allFiles.value.filter(f => f.id !== file.id)
    await resizeToFitContent()
    ctxMenuFile.value = null
  } catch (e) {
    if (e === 'cancel' || e === 'close') return
    console.error('删除文件失败:', e)
  } finally {
    dialogOpen.value = false
  }
}

let dragState = null

function onWinDragMove(e) {
  if (!dragState) return
  const {startX, startY, startSX, startSY, mx, my, mw, mh, sw, sh, dpr} = dragState
  dragState._px = Math.round(Math.max(mx, Math.min(startX + (e.screenX - startSX) * dpr, mx + mw - sw)))
  dragState._py = Math.round(Math.max(my, Math.min(startY + (e.screenY - startSY) * dpr, my + mh - sh)))
  if (!dragState._raf) {
    dragState._raf = requestAnimationFrame(applyWinDrag)
  }
}

function applyWinDrag() {
  if (!dragState) return
  dragState._raf = null
  const x = dragState._px, y = dragState._py
  if (x !== dragState._lx || y !== dragState._ly) {
    dragState._lx = x;
    dragState._ly = y
    appWindow.setPosition(new PhysicalPosition(x, y)).catch(() => {
    })
  }
}

async function onWinDragUp() {
  if (dragState?._raf) {
    cancelAnimationFrame(dragState._raf);
    dragState._raf = null
  }
  document.removeEventListener('mousemove', onWinDragMove)
  document.removeEventListener('mouseup', onWinDragUp)
  if (dragState) {
    const x = dragState._px, y = dragState._py
    dragState = null
    if (x !== undefined) {
      await appWindow.setPosition(new PhysicalPosition(x, y))
    }
    await snapWindowPosition()
  }
}

async function handleHeaderDrag(e) {
  const dpr = window.devicePixelRatio || 1
  const [pos, mon] = await Promise.all([appWindow.outerPosition(), currentMonitor()])
  if (!mon) {
    await startDrag();
    return
  }
  const {x: mx, y: my} = mon.position
  const {width: mw, height: mh} = mon.size
  const size = await appWindow.outerSize()
  dragState = {
    startX: pos.x,
    startY: pos.y,
    startSX: e.screenX,
    startSY: e.screenY,
    mx,
    my,
    mw,
    mh,
    sw: size.width,
    sh: size.height,
    dpr,
    _lx: pos.x,
    _ly: pos.y,
    _px: pos.x,
    _py: pos.y,
    _raf: null
  }
  document.addEventListener('mousemove', onWinDragMove)
  document.addEventListener('mouseup', onWinDragUp)
}

async function snapWindowPosition() {
  try {
    const monitor = await currentMonitor()
    if (!monitor) return
    const {x: mx, y: my} = monitor.position
    const {width: mw, height: mh} = monitor.size
    const pos = await appWindow.outerPosition()
    const size = await appWindow.outerSize()

    const SNAP = 20
    let nx = pos.x
    let ny = pos.y

    if (Math.abs(pos.x - mx) < SNAP) nx = mx
    else if (Math.abs(pos.x + size.width - (mx + mw)) < SNAP) nx = mx + mw - size.width
    if (Math.abs(pos.y - my) < SNAP) ny = my
    else if (Math.abs(pos.y + size.height - (my + mh)) < SNAP) ny = my + mh - size.height

    nx = Math.max(mx, Math.min(nx, mx + mw - size.width))
    ny = Math.max(my, Math.min(ny, my + mh - size.height))

    if (nx !== pos.x || ny !== pos.y) {
      await appWindow.setPosition(new PhysicalPosition(nx, ny))
    }
  } catch (e) {
    console.error('吸附失败:', e)
  }
}

let unlistenData = null
let unlistenDragDrop = null

function onWindowBlur() {
  onDragEnd()
  if (dragState) {
    document.removeEventListener('mousemove', onWinDragMove)
    document.removeEventListener('mouseup', onWinDragUp)
    dragState = null
  }
  hovered.value = false
  nextTick().then(() => resizeToFitContent())
  import('element-plus').then(({ElMessageBox}) => ElMessageBox.close()).catch(() => {
  })
}

onMounted(async () => {
  window.addEventListener('blur', onWindowBlur)

  const {getCurrentWebview} = await import('@tauri-apps/api/webview')
  unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
    const {type, paths, position} = event.payload
    if (type === 'over') {
      const x = position.x / window.devicePixelRatio
      const y = position.y / window.devicePixelRatio
      const el = document.elementFromPoint(x, y)
      const card = el?.closest('[data-cat-id]')
      dragOverCatId.value = card ? Number(card.dataset.catId) : null
    } else if (type === 'leave') {
      dragOverCatId.value = null
    } else if (type === 'drop') {
      const catId = dragOverCatId.value
      dragOverCatId.value = null
      if (catId != null && paths) importDroppedFiles(paths, catId)
    }
  })

  try {
    const rts = await DocumentService.getRoots()
    roots.value = rts || []
  } catch (e) {
    console.error('加载根目录失败:', e)
    loading.value = false
    return
  }

  if (roots.value.length > 0) {
    selectedRootId.value = roots.value[0].id
    try {
      await loadDataForRoot(selectedRootId.value)
    } catch (e) {
      console.error('加载文档数据失败:', e)
    }
  }

  loading.value = false
  await nextTick()
  await resizeToFitContent()

  const {listen} = await import('@tauri-apps/api/event')
  unlistenData = await listen('doc-widget-refresh', async () => {
    try {
      await loadDataForRoot(selectedRootId.value)
    } catch (e) {
      console.error('刷新文档数据失败:', e)
    }
    await resizeToFitContent()
  })
})

onBeforeUnmount(() => {
  onDragEnd()
  window.removeEventListener('blur', onWindowBlur)
  if (unlistenDragDrop) unlistenDragDrop()
  if (unlistenData) unlistenData()
})

</script>

<style scoped>
.dmw-root {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  background: transparent;
  font-family: var(--fy-font-sans, 'Inter', sans-serif);
  font-size: var(--fy-text-base, 13px);
  color: var(--fy-text-primary, #e8ecf4);
  user-select: none;
}

.dmw-container {
  position: fixed;
  top: 0;
  right: 0;
  width: 380px;
  max-height: 100vh;
  background: var(--fy-glass-bg);
  border: 1px solid var(--fy-glass-border);
  border-radius: var(--fy-radius-xl);
  box-shadow: var(--fy-glass-shadow);
  backdrop-filter: var(--fy-glass-blur);
  -webkit-backdrop-filter: var(--fy-glass-blur);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.dmw-header {
  display: flex;
  align-items: center;
  padding: 10px 12px;
  gap: 8px;
  cursor: move;
  border-bottom: 1px solid var(--fy-border-light);
  flex-shrink: 0;
}

.dmw-header-left {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--fy-accent);
}

.dmw-title {
  font-size: var(--fy-text-md, 14px);
  font-weight: var(--fy-weight-semibold, 600);
  color: var(--fy-text-primary);
}

.dmw-stats {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
  font-size: var(--fy-text-xs, 11px);
  color: var(--fy-text-muted);
}

.dmw-stat-sep {
  color: var(--fy-border);
}

.dmw-header-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  margin-left: 4px;
}

.dmw-btn-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  border-radius: var(--fy-radius-sm);
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all var(--fy-duration-fast) var(--fy-ease-out);
}

.dmw-btn-icon:hover {
  background: var(--fy-bg-hover);
  color: var(--fy-accent);
}

.dmw-btn-close:hover {
  background: var(--fy-danger-bg);
  color: var(--fy-danger);
}

.dmw-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 40px;
  color: var(--fy-text-muted);
}

.dmw-roots {
  display: flex;
  gap: 6px;
  padding: 8px 12px 0;
  overflow-x: auto;
  flex-shrink: 0;
  cursor: grab;
  scrollbar-width: none;
}

.dmw-roots::-webkit-scrollbar {
  display: none;
}

.dmw-roots:active {
  cursor: grabbing;
}

.dmw-root-tab {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border-radius: var(--fy-radius-full);
  background: var(--fy-bg-card);
  border: 1px solid var(--fy-border-light);
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
  font-size: var(--fy-text-xs, 11px);
  color: var(--fy-text-muted);
  transition: all var(--fy-duration-fast) var(--fy-ease-out);
}

.dmw-root-tab:hover {
  background: var(--fy-bg-hover);
  border-color: var(--fy-border-hover);
  color: var(--fy-text-primary);
}

.dmw-root-tab.active {
  background: var(--fy-accent-bg);
  border-color: var(--fy-accent);
  color: var(--fy-accent);
}

.dmw-root-name {
  max-width: 100px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.dmw-categories {
  display: flex;
  gap: 8px;
  padding: 10px 12px;
  overflow-x: auto;
  flex-shrink: 0;
  cursor: grab;
  scrollbar-width: none;
}

.dmw-categories::-webkit-scrollbar {
  display: none;
}

.dmw-categories:active {
  cursor: grabbing;
}

.dmw-cat-card {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-radius: var(--fy-radius-md);
  background: var(--fy-bg-card);
  border: 1px solid var(--fy-border-light);
  cursor: pointer;
  transition: all var(--fy-duration-fast) var(--fy-ease-out);
  white-space: nowrap;
  flex-shrink: 0;
}

.dmw-cat-card:hover {
  background: var(--fy-bg-hover);
  border-color: var(--fy-border-hover);
}

.dmw-cat-card.active {
  background: var(--fy-accent-bg);
  border-color: var(--fy-accent);
}

.dmw-cat-card.drag-over {
  border-color: var(--fy-accent);
  box-shadow: 0 0 0 2px var(--fy-accent-bg);
  transform: scale(1.05);
}

.dmw-cat-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: var(--fy-radius-sm);
  flex-shrink: 0;
}

.dmw-cat-info {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.dmw-cat-name {
  font-size: var(--fy-text-sm, 12px);
  font-weight: var(--fy-weight-medium, 500);
  color: var(--fy-text-primary);
  line-height: 1.2;
}

.dmw-cat-count {
  font-size: var(--fy-text-xs, 11px);
  color: var(--fy-text-muted);
}

.dmw-files {
  flex: 1;
  overflow-y: auto;
  padding: 4px 8px 8px;
  min-height: 0;
  scrollbar-width: thin;
  scrollbar-color: var(--fy-scrollbar-thumb) transparent;
}

.dmw-files::-webkit-scrollbar {
  width: 3px;
}

.dmw-files::-webkit-scrollbar-thumb {
  background: var(--fy-scrollbar-thumb);
  border-radius: var(--fy-radius-full);
}

.dmw-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  color: var(--fy-text-muted);
  font-size: var(--fy-text-sm, 12px);
}

.dmw-file-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--fy-radius-sm);
  cursor: pointer;
  transition: background var(--fy-duration-fast) var(--fy-ease-out);
}

.dmw-file-item:hover {
  background: var(--fy-bg-hover);
}

.dmw-file-item.active {
  background: var(--fy-bg-hover);
}

.dmw-file-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--fy-text-sm, 12px);
  color: var(--fy-text-primary);
  min-width: 0;
}

.dmw-file-size {
  font-size: var(--fy-text-xs, 11px);
  color: var(--fy-text-muted);
  flex-shrink: 0;
}

.dmw-file-ext {
  font-size: var(--fy-text-xs, 10px);
  color: var(--fy-text-muted);
  background: var(--fy-bg-card);
  padding: 1px 4px;
  border-radius: var(--fy-radius-xs);
  flex-shrink: 0;
  font-weight: var(--fy-weight-medium);
}
</style>

<style>
html, body, #app {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: transparent !important;
}

html, body {
  background: transparent !important;
}

#app {
  background: transparent !important;
}

.fy-compact.el-message-box {
  width: auto;
  min-width: unset;
  padding: 0;
  border-radius: 8px;
}

.fy-compact.el-message-box .el-message-box__header {
  padding: 6px 12px 0;
}

.fy-compact.el-message-box .el-message-box__title {
  font-size: 11px;
  font-weight: 500;
  color: var(--fy-text-muted, #999);
  line-height: 1;
}

.fy-compact.el-message-box .el-message-box__headerbtn {
  display: none;
}

.fy-compact.el-message-box .el-message-box__content {
  display: none;
}

.fy-compact.el-message-box .el-message-box__btns {
  padding: 6px 10px 10px;
  display: flex;
  flex-direction: row;
  justify-content: center;
  gap: 6px;
  flex-wrap: nowrap;
}

.fy-compact.el-message-box .el-message-box__btns .el-button {
  float: none;
  margin-left: 0;
  font-size: 12px;
  padding: 6px 14px;
  border-radius: 6px;
  min-width: 60px;
}

.fy-compact.el-message-box .el-message-box__btns .el-button--primary {
  font-weight: 500;
}

.fy-compact.el-message-box .el-message-box__btns .el-button--default {
  color: var(--fy-text-muted, #999);
}

.fy-compact.el-notification {
  width: 260px;
  padding: 8px 12px;
}

.fy-compact.el-notification .el-notification__title {
  font-size: 12px;
}

.fy-compact.el-notification .el-notification__content {
  font-size: 12px;
  line-height: 1.4;
}

.fy-compact.el-notification .el-notification__closeBtn {
  top: 8px;
  right: 8px;
}
</style>
