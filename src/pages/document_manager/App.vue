<template>
  <div class="doc-manager" @dragover.prevent="dragover = true" @dragleave.prevent="dragover = false"
       @drop.prevent="handleDrop">
    <div class="dm-layout">
      <div class="dm-sidebar">
        <div class="dm-sidebar-header"><h3>文档管理</h3></div>
        <div class="dm-sidebar-stats">
          <div class="dm-stat">
            <span class="dm-stat-value">{{ stats?.totalFiles ?? 0 }}</span>
            <span class="dm-stat-label">全部文档</span>
          </div>
          <div class="dm-stat">
            <span class="dm-stat-value">{{ stats?.totalSize ? formatFileSize(stats.totalSize) : '0 B' }}</span>
            <span class="dm-stat-label">总大小</span>
          </div>
        </div>
        <div class="dm-sidebar-section">
          <div class="dm-section-title">根目录</div>
          <div class="dm-root-list">
            <div v-for="root in roots" :key="root.id" :class="{ active: rootFilter === root.id }" class="dm-root-item"
                 @click="rootFilter = rootFilter === root.id ? null : root.id">
              <el-icon>
                <Folder/>
              </el-icon>
              <span class="dm-root-name">{{ root.name }}</span>
            </div>
            <div class="dm-root-item dm-root-add" @click="showAddRoot = true">
              <el-icon>
                <Plus/>
              </el-icon>
              <span>添加目录</span>
            </div>
          </div>
        </div>
        <div class="dm-sidebar-section">
          <div class="dm-section-title">
            <span>分类</span>
            <el-button size="small" text @click="showAddCategory = true">
              <el-icon>
                <Plus/>
              </el-icon>
            </el-button>
          </div>
          <div class="dm-category-list">
            <div :class="{ active: categoryFilter === null }" class="dm-category-item" @click="categoryFilter = null">
              <el-icon>
                <Document/>
              </el-icon>
              <span>全部</span>
            </div>
            <div v-for="cat in categories" :key="cat.id" :class="{ active: categoryFilter === cat.id }"
                 class="dm-category-item"
                 @click="categoryFilter = categoryFilter === cat.id ? null : cat.id">
              <el-icon :style="{ color: cat.color }">
                <Folder/>
              </el-icon>
              <span>{{ cat.name }}</span>
              <span v-if="getCatCount(cat.id) > 0" class="dm-cat-count">{{ getCatCount(cat.id) }}</span>
              <el-dropdown trigger="click" @click.stop>
                <el-icon class="dm-cat-more">
                  <MoreFilled/>
                </el-icon>
                <template #dropdown>
                  <el-dropdown-menu>
                    <el-dropdown-item @click="startRenameCat(cat)">重命名</el-dropdown-item>
                    <el-dropdown-item style="color: var(--el-color-danger)" @click="removeCategoryFn(cat.id)">删除
                    </el-dropdown-item>
                  </el-dropdown-menu>
                </template>
              </el-dropdown>
            </div>
            <div v-if="!categories || categories.length === 0" class="dm-category-item dm-cat-empty">
              <span>暂无分类</span>
            </div>
          </div>
        </div>
      </div>

      <div class="dm-main">
        <div class="dm-toolbar">
          <div class="dm-tabs">
            <button :class="['dm-tab', { active: activeTab === 'files' }]" @click="activeTab = 'files'">文档</button>
            <button :class="['dm-tab', { active: activeTab === 'history' }]"
                    @click="activeTab = 'history'; loadImportHistory()">历史记录
            </button>
          </div>
          <template v-if="activeTab === 'files'">
            <div class="dm-search">
              <el-input v-model="searchKeyword" clearable placeholder="搜索文件名..." @clear="searchFiles"
                        @keyup.enter="searchFiles">
                <template #prefix>
                  <el-icon>
                    <Search/>
                  </el-icon>
                </template>
              </el-input>
            </div>
            <div class="dm-toolbar-actions">
              <el-button type="primary" @click="showImportDialog = true">
                <el-icon>
                  <Plus/>
                </el-icon>
                添加文档
              </el-button>
              <el-button @click="showScanDialog = true">
                <el-icon>
                  <Search/>
                </el-icon>
                扫描文件夹
              </el-button>
            </div>
          </template>
        </div>

        <template v-if="activeTab === 'files'">
          <div v-if="items.length === 0 && !loading" class="dm-empty">
            <el-empty description="暂无文档"/>
          </div>
          <div v-else-if="loading" class="dm-empty">
            <el-icon :size="32" class="is-loading">
              <Loading/>
            </el-icon>
            <p style="margin-top:12px;color:var(--el-text-color-secondary)">加载中...</p>
          </div>
          <div v-else class="dm-file-grid">
            <div v-for="item in items" :key="item.id"
                 :class="{ selected: selectedId === item.id, 'ctx-anchor': ctxAnchorId === item.id }"
                 class="dm-file-card"
                 @click="selectedId = item.id" @dblclick="openDocument(item)"
                 @contextmenu.prevent="showContextMenu($event, item)">
              <div class="dm-file-icon">
                <el-icon :color="getFileColor(item.fileExt)" :size="32">
                  <component :is="getFileIcon(item.fileExt)"/>
                </el-icon>
              </div>
              <div class="dm-file-info">
                <div :title="item.title || item.fileName" class="dm-file-name">{{ item.title || item.fileName }}</div>
                <div class="dm-file-meta">
                  <span class="dm-file-ext">{{ item.fileExt.toUpperCase() }}</span>
                  <span>{{ formatFileSize(item.fileSize) }}</span>
                </div>
              </div>
            </div>
          </div>
          <div v-if="total > pageLimit" class="dm-pagination">
            <el-pagination v-model:current-page="currentPage" :page-size="pageLimit" :total="total"
                           layout="prev,pager,next" @current-change="loadFiles"/>
          </div>
          <div v-if="selectedId !== null && selectedDoc" class="dm-detail-panel">
            <div class="dm-detail-header">
              <span>{{ selectedDoc.title || selectedDoc.fileName }}</span>
              <el-button size="small" text @click="selectedId = null">
                <el-icon>
                  <Close/>
                </el-icon>
              </el-button>
            </div>
            <div class="dm-detail-body">
              <div class="dm-detail-row"><span class="dm-detail-label">文件名</span><span>{{
                  selectedDoc.fileName
                }}</span></div>
              <div class="dm-detail-row"><span
                  class="dm-detail-label">大小</span><span>{{ formatFileSize(selectedDoc.fileSize) }}</span></div>
              <div class="dm-detail-row"><span
                  class="dm-detail-label">类型</span><span>{{ selectedDoc.fileExt.toUpperCase() }}</span></div>
              <div class="dm-detail-row"><span class="dm-detail-label">路径</span><span :title="selectedDoc.managedPath"
                                                                                        class="dm-detail-path">{{
                  selectedDoc.managedPath
                }}</span></div>
              <div class="dm-detail-row"><span class="dm-detail-label">模式</span><span>{{
                  selectedDoc.storageMode === 'repo' ? '搬迁' : '索引'
                }}</span></div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">分类</span>
                <el-select v-model="editCategoryId" clearable placeholder="未分类" size="small" @change="saveCategory">
                  <el-option v-for="cat in categories" :key="cat.id" :label="cat.name" :value="cat.id"/>
                </el-select>
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">标签</span>
                <el-input v-model="editTags" placeholder="逗号分隔" size="small" @blur="saveTags"
                          @keyup.enter="saveTags"/>
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">备注</span>
                <el-input v-model="editNotes" :rows="2" placeholder="添加备注..." size="small" type="textarea"
                          @blur="saveNotes"/>
              </div>
            </div>
            <div class="dm-detail-actions">
              <el-button size="small" @click="openDocument(selectedDoc)">打开</el-button>
              <el-button size="small" @click="openFolder(selectedDoc)">定位</el-button>
              <el-button size="small" type="danger" @click="confirmDelete(selectedDoc)">删除</el-button>
            </div>
          </div>
        </template>

        <template v-if="activeTab === 'history'">
          <div v-if="importHistory.length === 0" class="dm-empty">
            <el-empty description="暂无历史记录"/>
          </div>
          <div v-else class="dm-history-page">
            <div v-for="h in importHistory" :key="h.id" class="dm-history-card">
              <div class="dm-history-card-hd">
                <span class="dm-history-badge">{{ h.storageMode === 'repo' ? '搬迁' : '索引' }}</span>
                <span class="dm-history-fc">{{ h.fileCount }} 个文件</span>
                <span class="dm-history-time">{{ formatTime(h.createdAt) }}</span>
              </div>
              <div class="dm-history-card-body">
                <div class="dm-history-row"><span class="dm-history-label">源目录</span><span
                    class="dm-history-val">{{ h.sourceDir }}</span></div>
                <div class="dm-history-row"><span class="dm-history-label">目标目录</span><span class="dm-history-val">{{
                    h.targetDir
                  }}</span></div>
              </div>
              <div class="dm-history-card-ft">
                <el-button size="small" text @click="toggleHistoryFiles(h.id)">
                  {{ h._files ? '收起' : '展开' }} ({{ h.fileCount }}个文件)
                </el-button>
                <el-popconfirm title="确定撤销这次导入吗？搬迁的文件将回退到原位置" @confirm="undoImportFn(h.id)">
                  <template #reference>
                    <el-button plain size="small" type="danger">撤销</el-button>
                  </template>
                </el-popconfirm>
              </div>
              <div v-if="h._files" class="dm-history-files">
                <div v-for="f in h._files" :key="f.sourcePath" class="dm-history-file-item">
                  <el-icon>
                    <Document/>
                  </el-icon>
                  <span>{{ f.fileName }}</span>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>

    <el-dialog v-model="showAddRoot" title="添加管理目录" width="480px">
      <el-form label-width="80px">
        <el-form-item label="目录别名">
          <el-input v-model="newRootName" placeholder="如：工作资料"/>
        </el-form-item>
        <el-form-item label="目录路径">
          <div style="display:flex;gap:8px;width:100%">
            <el-input v-model="newRootPath" placeholder="如：D:\工作资料"/>
            <el-button @click="browseRootPath">浏览</el-button>
          </div>
        </el-form-item>
      </el-form>
      <div class="dm-hint-warn">
        <el-icon>
          <Warning/>
        </el-icon>
        <span>导入文档时，文件将被移动到该目录下，按分类组织为子目录</span></div>
      <template #footer>
        <el-button @click="showAddRoot = false">取消</el-button>
        <el-button :disabled="!newRootName || !newRootPath" type="primary" @click="confirmAddRoot">确认</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showAddCategory" title="新建分类" width="400px">
      <el-form label-width="80px">
        <el-form-item label="分类名称">
          <el-input v-model="newCategoryName" placeholder="如：合同、报表"/>
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showAddCategory = false">取消</el-button>
        <el-button :disabled="!newCategoryName.trim()" type="primary" @click="confirmAddCategory">确认</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showRenameCatDialog" title="重命名分类" width="400px">
      <el-form label-width="80px">
        <el-form-item label="新名称">
          <el-input v-model="renameCatName" placeholder="输入新名称"/>
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showRenameCatDialog = false">取消</el-button>
        <el-button type="primary" @click="confirmRenameCat">确认</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showImportDialog" title="添加文档" width="500px">
      <el-form label-width="80px">
        <el-form-item label="目标目录">
          <el-select v-model="importRootId" placeholder="选择管理目录" style="width:100%">
            <el-option v-for="r in roots" :key="r.id" :label="r.name" :value="r.id"/>
          </el-select>
        </el-form-item>
        <el-form-item label="目标分类">
          <el-select v-model="importCategoryId" clearable placeholder="未分类" style="width:100%">
            <el-option v-for="c in categories" :key="c.id" :label="c.name" :value="c.id"/>
          </el-select>
        </el-form-item>
        <el-form-item label="导入方式">
          <el-radio-group v-model="importMode">
            <el-radio value="index">索引（文件不动，只记录位置）</el-radio>
            <el-radio value="repo">搬迁（移动到管理目录）</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="选择文件">
          <el-button @click="browseImportFiles">选择文件</el-button>
          <div v-if="importFiles.length > 0" class="dm-import-files">
            <el-tag v-for="(f,i) in importFiles" :key="i" closable style="margin:2px" @close="importFiles.splice(i,1)">
              {{ getFileName(f) }}
            </el-tag>
          </div>
        </el-form-item>
      </el-form>
      <div v-if="importRootId != null && importMode === 'repo'" class="dm-hint-warn">
        <el-icon>
          <Warning/>
        </el-icon>
        <span>文件将被移动到：{{ getImportTargetPath() }}</span></div>
      <template #footer>
        <el-button @click="showImportDialog = false">取消</el-button>
        <el-button :disabled="!importRootId || importFiles.length === 0" type="primary" @click="confirmImport">
          确认导入
        </el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showScanDialog" title="扫描文件夹" width="500px">
      <el-form label-width="80px">
        <el-form-item label="扫描路径">
          <div style="display:flex;gap:8px;width:100%">
            <el-input v-model="scanPath" placeholder="如：C:\Users\...\Desktop"/>
            <el-button @click="browseScanPath">浏览</el-button>
          </div>
        </el-form-item>
        <el-form-item label="目标目录">
          <el-select v-model="scanImportRootId" placeholder="选择管理目录" style="width:100%">
            <el-option v-for="r in roots" :key="r.id" :label="r.name" :value="r.id"/>
          </el-select>
        </el-form-item>
        <el-form-item label="目标分类">
          <el-select v-model="importCategoryId" clearable placeholder="未分类" style="width:100%">
            <el-option v-for="c in categories" :key="c.id" :label="c.name" :value="c.id"/>
          </el-select>
        </el-form-item>
        <el-form-item label="导入方式">
          <el-radio-group v-model="importMode">
            <el-radio value="index">索引</el-radio>
            <el-radio value="repo">搬迁</el-radio>
          </el-radio-group>
        </el-form-item>
      </el-form>
      <div v-if="scanning" style="text-align:center;padding:20px">
        <el-icon :size="32" class="is-loading">
          <Loading/>
        </el-icon>
        <p>正在扫描...</p></div>
      <div v-else-if="scannedFiles.length > 0" class="dm-scan-list">
        <p>找到 {{ scannedFiles.length }} 个文件</p>
        <div class="dm-scan-files">
          <div v-for="f in scannedFiles" :key="f.path" :class="{ checked: scanSelected.has(f.path) }"
               class="dm-scan-file-item" @click="toggleScanSelect(f.path)">
            <el-icon v-if="scanSelected.has(f.path)"><Select/></el-icon>
            <el-icon v-else>
              <Document/>
            </el-icon>
            <span>{{ f.name }}</span>
            <span class="dm-scan-size">{{ formatFileSize(f.size) }}</span>
          </div>
        </div>
      </div>
      <template #footer>
        <el-button @click="showScanDialog = false">取消</el-button>
        <el-button :disabled="scannedFiles.length === 0" @click="toggleScanSelectAll">
          {{ scanSelected.size === scannedFiles.length ? '取消全选' : '全选' }}
        </el-button>
        <el-button :disabled="scanSelected.size === 0 || !scanImportRootId" type="primary" @click="importScanned">导入选中
          ({{ scanSelected.size }})
        </el-button>
      </template>
    </el-dialog>

    <div v-if="dragover" class="dm-drop-overlay">
      <el-icon :size="48">
        <UploadFilled/>
      </el-icon>
      <p>释放文件以添加</p>
    </div>
  </div>

  <ContextMenu :show="ctxMenuVisible" :x="ctxMenuX" :y="ctxMenuY" @close="closeCtxMenu">
    <div class="context-menu-item" @click="startMove(ctxMenuDoc)">
      <el-icon :size="14">
        <Folder/>
      </el-icon>
      <span>移动</span>
    </div>
    <div class="context-menu-divider"></div>
    <div class="context-menu-item context-menu-item-danger" @click="contextDelete(ctxMenuDoc)">
      <el-icon :size="14">
        <Close/>
      </el-icon>
      <span>删除</span>
    </div>
  </ContextMenu>

  <el-dialog v-model="showMoveDialog" title="移动文件" width="420px" @closed="closeCtxMenu">
    <div class="dm-move-body">
      <div class="dm-move-section">
        <div class="dm-move-section-title">移动到分类</div>
        <el-select v-model="moveTargetCategoryId" clearable placeholder="未分类" style="width:100%">
          <el-option v-for="cat in categories" :key="cat.id" :label="cat.name" :value="cat.id"/>
        </el-select>
      </div>
      <div class="dm-move-section">
        <div class="dm-move-section-title">移动到根目录</div>
        <el-select v-model="moveTargetRootId" clearable placeholder="不更改根目录" style="width:100%">
          <el-option v-for="root in roots" :key="root.id" :label="root.name" :value="root.id"/>
        </el-select>
        <div v-if="moveDoc.storageMode === 'repo'" class="dm-move-hint">搬迁模式文件将被物理移动到目标目录</div>
      </div>
    </div>
    <template #footer>
      <el-button @click="showMoveDialog = false">取消</el-button>
      <el-button :disabled="!hasMoveChange" type="primary" @click="confirmMove">确定</el-button>
    </template>
  </el-dialog>
</template>

<script setup>
import {ref, reactive, computed, watch, onMounted} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {DocumentService} from '@/services/ipc.js'
import {open as openDialog} from '@tauri-apps/plugin-dialog'
import {
  Document, Folder, Plus, Search, Setting, Monitor,
  Picture, Notebook, Tickets, Connection, MagicStick,
  Coffee, MoreFilled, Close, Warning,
  UploadFilled, Select, Loading, List,
} from '@element-plus/icons-vue'
import ContextMenu from '../../components/ContextMenu.vue'

const iconMap = {
  pdf: Document, docx: Document, doc: Document,
  xlsx: List, xls: List, pptx: Notebook, ppt: Notebook,
  txt: Tickets, md: Notebook, csv: List, log: Tickets,
  json: Setting, xml: Document, yaml: Setting, yml: Setting,
  py: Monitor, js: Monitor, ts: Monitor, java: Coffee, go: Monitor,
  rs: Monitor, c: Monitor, cpp: Monitor, php: Monitor,
  html: Connection, css: MagicStick, vue: Monitor,
  png: Picture, jpg: Picture, jpeg: Picture, gif: Picture, bmp: Picture, webp: Picture, svg: Picture,
  sh: Monitor, bat: Monitor, ps1: Monitor, sql: List,
}
const fileColorMap = {
  pdf: '#E74C3C', docx: '#2980B9', doc: '#2980B9',
  xlsx: '#27AE60', xls: '#27AE60', pptx: '#E67E22',
  md: '#8E44AD', txt: '#7F8C8D',
  py: '#3498DB', js: '#F1C40F', ts: '#3178C6', java: '#E76F00', go: '#00ADD8',
  rs: '#DEA584', html: '#E34F26', css: '#1572B6',
  png: '#9B59B6', jpg: '#9B59B6', svg: '#E67E22',
}

const categories = ref([])
const roots = ref([])
const items = ref([])
const stats = ref(null)
const loading = ref(false)
const searchKeyword = ref('')
const categoryFilter = ref(null)
const rootFilter = ref(null)
const currentPage = ref(1)
const pageLimit = 50
const total = ref(0)
const selectedId = ref(null)
const dragover = ref(false)
const activeTab = ref('files')
const importHistory = ref([])

const showAddRoot = ref(false)
const newRootName = ref('')
const newRootPath = ref('')
const showAddCategory = ref(false)
const newCategoryName = ref('')
const showRenameCatDialog = ref(false)
const renameCatId = ref(null)
const renameCatName = ref('')
const showImportDialog = ref(false)
const importRootId = ref(null)
const importCategoryId = ref(null)
const importFiles = ref([])
const importMode = ref('index')
const showScanDialog = ref(false)
const scanPath = ref('')
const scanImportRootId = ref(null)
const scanning = ref(false)
const scannedFiles = ref([])
const scanSelected = reactive(new Set())
const editCategoryId = ref(null)
const editTags = ref('')
const editNotes = ref('')

const ctxMenuVisible = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)
const ctxMenuDoc = ref(null)
const ctxAnchorId = ref(null)
const showMoveDialog = ref(false)
const moveDoc = ref(null)
const moveTargetCategoryId = ref(null)
const moveTargetRootId = ref(null)

const selectedDoc = computed(() => {
  if (selectedId.value === null) return null
  return items.value.find(i => i.id === selectedId.value) || null
})

const hasMoveChange = computed(() => {
  if (!moveDoc.value) return false
  return moveTargetCategoryId.value !== (moveDoc.value.categoryId ?? null)
      || (moveTargetRootId.value && moveTargetRootId.value !== moveDoc.value.rootId)
})

watch(selectedDoc, (doc) => {
  if (!doc) return
  editCategoryId.value = doc.categoryId ?? null
  try {
    editTags.value = JSON.parse(doc.tags || '[]').join(', ')
  } catch {
    editTags.value = doc.tags || ''
  }
  editNotes.value = doc.notes || ''
})

function getFileName(f) {
  const p = f.replace(/\\/g, '/').split('/');
  return p[p.length - 1] || f
}

function formatFileSize(bytes) {
  if (!bytes) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB']
  let i = 0, s = Number(bytes)
  while (s >= 1024 && i < 3) {
    s /= 1024;
    i++
  }
  return s.toFixed(i > 0 ? 1 : 0) + ' ' + u[i]
}

function formatTime(ms) {
  if (!ms) return ''
  const d = new Date(Number(ms))
  const p = n => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

function getFileIcon(ext) {
  return iconMap[ext] || Document
}

function getFileColor(ext) {
  return fileColorMap[ext] || '#7F8C8D'
}

function getCatCount(catId) {
  if (!stats.value?.categoryCounts) return 0
  const e = stats.value.categoryCounts.find(c => c.categoryId === catId)
  return e?.count || 0
}

async function loadData() {
  try {
    const [cats, rts, st] = await Promise.all([
      DocumentService.getCategories(),
      DocumentService.getRoots(),
      DocumentService.getStats(rootFilter.value),
    ])
    categories.value = cats || []
    roots.value = rts || []
    stats.value = st
  } catch (e) {
    ElMessage.error('加载数据失败: ' + e)
  }
}

async function loadFiles() {
  loading.value = true
  try {
    const r = await DocumentService.getPage({
      offset: (currentPage.value - 1) * pageLimit, limit: pageLimit,
      categoryId: categoryFilter.value, rootId: rootFilter.value,
      keyword: searchKeyword.value || null,
    })
    items.value = r.items || []
    total.value = Number(r.total) || 0
    selectedId.value = null
  } catch (e) {
    ElMessage.error('加载文件列表失败')
  } finally {
    loading.value = false
  }
}

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
    ElMessage.error('加载文件列表失败')
  }
}

async function undoImportFn(importId) {
  try {
    const errors = await DocumentService.undoImport(importId)
    if (errors && errors.length > 0) ElMessage.warning('部分撤销失败')
    else ElMessage.success('已撤销')
    await loadData();
    await loadFiles();
    loadImportHistory()
  } catch (e) {
    ElMessage.error('撤销失败: ' + e)
  }
}

function searchFiles() {
  currentPage.value = 1;
  loadFiles()
}

watch(categoryFilter, () => {
  currentPage.value = 1;
  loadFiles()
})
watch(rootFilter, () => {
  currentPage.value = 1;
  loadData();
  loadFiles()
})

async function confirmAddRoot() {
  try {
    await DocumentService.addRoot(newRootName.value.trim(), newRootPath.value.trim());
    ElMessage.success('已添加');
    showAddRoot.value = false;
    newRootName.value = '';
    newRootPath.value = '';
    await loadData()
  } catch (e) {
    ElMessage.error('添加失败: ' + e)
  }
}

async function browseRootPath() {
  const p = await openDialog({directory: true, multiple: false});
  if (p) newRootPath.value = p
}

async function confirmAddCategory() {
  try {
    await DocumentService.addCategory(newCategoryName.value.trim());
    ElMessage.success('已添加');
    showAddCategory.value = false;
    newCategoryName.value = '';
    await loadData()
  } catch (e) {
    ElMessage.error('添加失败: ' + e)
  }
}

async function removeCategoryFn(id) {
  try {
    await ElMessageBox.confirm('确认删除该分类？', '确认删除');
    await DocumentService.removeCategory(id);
    ElMessage.success('已删除');
    await loadData();
    loadFiles()
  } catch (e) {
    if (e) {
      ElMessage.error(typeof e === 'string' ? e : e.message || '删除失败')
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
    ElMessage.success('已重命名');
    showRenameCatDialog.value = false;
    await loadData()
  } catch (e) {
    ElMessage.error('重命名失败: ' + e)
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

async function confirmImport() {
  if (!importRootId.value || importFiles.value.length === 0) return
  try {
    const r = await DocumentService.importFiles({
      paths: importFiles.value,
      rootId: importRootId.value,
      categoryId: importCategoryId.value || null,
      storageMode: importMode.value
    })
    if (r.errors && r.errors.length > 0) ElMessage.warning(`导入完成，${r.success.length} 个成功，${r.errors.length} 个失败`)
    else ElMessage.success(`成功导入 ${r.success.length} 个文件`)
    showImportDialog.value = false;
    importFiles.value = [];
    await loadData();
    loadFiles();
    loadImportHistory()
  } catch (e) {
    ElMessage.error('导入失败: ' + e)
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
    ElMessage.error('扫描失败: ' + e)
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
  try {
    const r = await DocumentService.importFiles({
      paths: Array.from(scanSelected),
      rootId: scanImportRootId.value,
      categoryId: importCategoryId.value || null,
      storageMode: importMode.value,
      sourceDir: scanPath.value
    })
    ElMessage.success(`成功导入 ${r.success.length} 个文件`);
    scanSelected.clear();
    scannedFiles.value = [];
    await loadData();
    loadFiles();
    loadImportHistory()
  } catch (e) {
    ElMessage.error('导入失败: ' + e)
  }
}

async function handleDrop(event) {
  dragover.value = false;
  const files = event.dataTransfer?.files;
  if (!files || files.length === 0) return
  if (!roots.value || roots.value.length === 0) {
    ElMessage.warning('请先添加管理目录');
    return
  }
  const paths = [];
  for (let i = 0; i < files.length; i++) {
    if (files[i].path) paths.push(files[i].path)
  }
  if (paths.length === 0) return
  importRootId.value = roots.value[0].id;
  importCategoryId.value = null;
  importFiles.value = paths;
  showImportDialog.value = true
}

async function openDocument(doc) {
  try {
    await DocumentService.openDoc(doc.id)
  } catch (e) {
    ElMessage.error('打开失败: ' + e)
  }
}

async function openFolder(doc) {
  try {
    await DocumentService.openFolder(doc.id)
  } catch (e) {
    ElMessage.error('打开失败: ' + e)
  }
}

async function confirmDelete(doc) {
  try {
    await ElMessageBox.confirm(`确定删除「${doc.title || doc.fileName}」？文件保留在磁盘上。`, '确认删除', {type: 'warning'});
    await DocumentService.deleteDoc(doc.id, false);
    ElMessage.success('已删除');
    selectedId.value = null;
    await loadFiles();
    await loadData()
  } catch {
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

function startMove(doc) {
  closeCtxMenu()
  moveDoc.value = doc
  moveTargetCategoryId.value = doc.categoryId ?? null
  moveTargetRootId.value = null
  showMoveDialog.value = true
}

async function contextDelete(doc) {
  closeCtxMenu()
  await confirmDelete(doc)
}

async function confirmMove() {
  if (!moveDoc.value || !hasMoveChange.value) return
  const doc = moveDoc.value

  try {
    if (moveTargetCategoryId.value !== (doc.categoryId ?? null)) {
      await DocumentService.updateMeta({id: doc.id, categoryId: moveTargetCategoryId.value ?? null})
    }
    if (moveTargetRootId.value && moveTargetRootId.value !== doc.rootId) {
      await DocumentService.moveDoc(doc.id, moveTargetRootId.value)
    }
    ElMessage.success('移动成功')
    showMoveDialog.value = false
    await loadFiles()
    await loadData()
  } catch (e) {
    ElMessage.error(typeof e === 'string' ? e : e?.message || '移动失败')
  }
}

onMounted(async () => {
  await loadData();
  await loadFiles();
  loadImportHistory()
})

</script>

<style scoped>
@import "../shared/contextMenu.css";

.doc-manager {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--el-bg-color);
  color: var(--el-text-color-primary);
  position: relative
}

.dm-layout {
  display: flex;
  height: 100%;
  overflow: hidden
}

.dm-sidebar {
  width: 240px;
  min-width: 240px;
  border-right: 1px solid var(--el-border-color-light);
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  background: var(--el-bg-color-page)
}

.dm-sidebar-header {
  padding: 16px;
  border-bottom: 1px solid var(--el-border-color-lighter)
}

.dm-sidebar-header h3 {
  margin: 0;
  font-size: 16px
}

.dm-sidebar-stats {
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px
}

.dm-stat {
  display: flex;
  justify-content: space-between;
  align-items: baseline
}

.dm-stat-value {
  font-size: 18px;
  font-weight: 600;
  color: var(--el-color-primary)
}

.dm-stat-label {
  font-size: 12px;
  color: var(--el-text-color-secondary)
}

.dm-sidebar-section {
  padding: 0 0 8px 0
}

.dm-section-title {
  padding: 8px 16px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  display: flex;
  align-items: center;
  justify-content: space-between
}

.dm-root-list, .dm-category-list {
  display: flex;
  flex-direction: column;
  gap: 2px
}

.dm-root-item, .dm-category-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 16px;
  cursor: pointer;
  font-size: 13px;
  border-radius: 0;
  transition: background .15s
}

.dm-root-item:hover, .dm-category-item:hover {
  background: var(--el-fill-color-light)
}

.dm-root-item.active, .dm-category-item.active {
  background: var(--el-color-primary-light-9);
  color: var(--el-color-primary)
}

.dm-root-add {
  color: var(--el-text-color-secondary);
  font-size: 12px
}

.dm-cat-count {
  margin-left: auto;
  font-size: 11px;
  color: var(--el-text-color-secondary);
  background: var(--el-fill-color);
  padding: 1px 6px;
  border-radius: 10px
}

.dm-cat-more {
  opacity: 0;
  transition: opacity .15s;
  margin-left: 4px
}

.dm-category-item:hover .dm-cat-more {
  opacity: 1
}

.dm-cat-empty {
  color: var(--el-text-color-secondary);
  cursor: default;
  font-size: 12px;
  padding-left: 16px
}

.dm-root-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap
}

.dm-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative
}

.dm-toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--el-border-color-lighter)
}

.dm-tabs {
  display: flex;
  gap: 4px
}

.dm-tab {
  padding: 6px 16px;
  border: none;
  background: transparent;
  cursor: pointer;
  font-size: 14px;
  color: var(--el-text-color-secondary);
  border-radius: 6px;
  transition: all .15s
}

.dm-tab:hover {
  color: var(--el-text-color-primary);
  background: var(--el-fill-color-light)
}

.dm-tab.active {
  color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
  font-weight: 500
}

.dm-search {
  flex: 1;
  max-width: 360px
}

.dm-toolbar-actions {
  display: flex;
  gap: 8px
}

.dm-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center
}

.dm-file-grid {
  flex: 1;
  display: flex;
  flex-wrap: wrap;
  align-content: flex-start;
  gap: 8px;
  padding: 12px 16px;
  overflow-y: auto;
  padding-bottom: 200px
}

.dm-file-card {
  width: 140px;
  padding: 12px 8px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  transition: all .15s;
  background: var(--el-bg-color)
}

.dm-file-card:hover {
  border-color: var(--el-color-primary-light-5);
  background: var(--el-color-primary-light-9)
}

.dm-file-card.selected {
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9)
}

.dm-file-card.ctx-anchor {
  border-color: var(--el-color-primary-light-5);
  background: var(--el-color-primary-light-9)
}

.dm-file-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px
}

.dm-file-info {
  text-align: center;
  width: 100%;
  overflow: hidden
}

.dm-file-name {
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.4
}

.dm-file-meta {
  font-size: 11px;
  color: var(--el-text-color-secondary);
  display: flex;
  justify-content: center;
  gap: 6px;
  margin-top: 4px
}

.dm-file-ext {
  background: var(--el-fill-color);
  padding: 0 4px;
  border-radius: 3px;
  font-size: 10px;
  font-weight: bold
}

.dm-pagination {
  display: flex;
  justify-content: center;
  padding: 8px 16px 16px
}

.dm-detail-panel {
  position: absolute;
  right: 0;
  top: 0;
  bottom: 0;
  width: 280px;
  background: var(--el-bg-color);
  border-left: 1px solid var(--el-border-color-light);
  display: flex;
  flex-direction: column;
  z-index: 10;
  box-shadow: -2px 0 8px rgba(0, 0, 0, .08)
}

.dm-detail-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--el-border-color-lighter);
  font-weight: 500
}

.dm-detail-body {
  flex: 1;
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow-y: auto
}

.dm-detail-row {
  display: flex;
  flex-direction: column;
  gap: 4px
}

.dm-detail-label {
  font-size: 11px;
  color: var(--el-text-color-secondary)
}

.dm-detail-row span {
  font-size: 13px;
  word-break: break-all
}

.dm-detail-path {
  font-size: 11px !important;
  color: var(--el-text-color-secondary);
  word-break: break-all;
  max-height: 40px;
  overflow: hidden
}

.dm-detail-actions {
  display: flex;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--el-border-color-lighter)
}

.dm-drop-overlay {
  position: absolute;
  inset: 0;
  background: rgba(64, 158, 255, .08);
  border: 2px dashed var(--el-color-primary);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  z-index: 100;
  pointer-events: none
}

.dm-drop-overlay p {
  font-size: 16px;
  color: var(--el-color-primary)
}

.dm-hint-warn {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  background: var(--el-color-warning-light-9);
  border-radius: 6px;
  margin-top: 12px;
  font-size: 13px;
  color: var(--el-color-warning-dark-2)
}

.dm-import-files {
  margin-top: 8px;
  display: flex;
  flex-wrap: wrap;
  gap: 4px
}

.dm-scan-list {
  max-height: 300px;
  overflow-y: auto
}

.dm-scan-list p {
  margin: 0 0 8px;
  font-size: 13px;
  color: var(--el-text-color-secondary)
}

.dm-scan-files {
  display: flex;
  flex-direction: column;
  gap: 2px
}

.dm-scan-file-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  cursor: pointer;
  border-radius: 4px;
  font-size: 13px
}

.dm-scan-file-item:hover {
  background: var(--el-fill-color-light)
}

.dm-scan-file-item.checked {
  background: var(--el-color-primary-light-9);
  color: var(--el-color-primary)
}

.dm-scan-size {
  margin-left: auto;
  font-size: 11px;
  color: var(--el-text-color-secondary)
}

.dm-history-page {
  flex: 1;
  overflow-y: auto;
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px
}

.dm-history-card {
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  padding: 12px 16px
}

.dm-history-card-hd {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 8px
}

.dm-history-badge {
  font-weight: 600;
  color: var(--el-color-primary);
  font-size: 13px
}

.dm-history-fc {
  font-size: 13px;
  color: var(--el-text-color-secondary)
}

.dm-history-time {
  margin-left: auto;
  font-size: 12px;
  color: var(--el-text-color-disabled)
}

.dm-history-card-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 10px
}

.dm-history-row {
  display: flex;
  gap: 8px;
  font-size: 13px
}

.dm-history-label {
  color: var(--el-text-color-secondary);
  white-space: nowrap
}

.dm-history-val {
  color: var(--el-text-color-primary);
  word-break: break-all
}

.dm-history-card-ft {
  display: flex;
  justify-content: space-between;
  align-items: center
}

.dm-history-files {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px solid var(--el-border-color-lighter);
  max-height: 200px;
  overflow-y: auto
}

.dm-history-file-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 0;
  font-size: 12px;
  color: var(--el-text-color-secondary)
}

.dm-move-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.dm-move-section-title {
  font-size: 13px;
  color: var(--el-text-color-secondary);
  margin-bottom: 6px;
}

.dm-move-hint {
  font-size: 12px;
  color: var(--el-color-warning);
  margin-top: 4px;
}
</style>
