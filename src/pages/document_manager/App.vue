<template>
  <div class="doc-manager" @dragover.prevent="dragover = true" @dragleave.prevent="dragover = false"
       @drop.prevent="handleDrop">
    <div class="dm-layout">
      <div class="dm-sidebar">
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
        <div v-if="orphanCount > 0" class="dm-orphan-banner" @click="showOrphanDialog = true">
          <span class="dm-orphan-badge">{{ orphanCount }}</span>
          <span>个未管理文件</span>
          <el-icon :size="12">
            <ArrowRight/>
          </el-icon>
        </div>
        <div v-else-if="!orphanChecked" class="dm-orphan-banner dm-orphan-banner--hint" @click="detectOrphans">
          <el-icon :size="14">
            <Search/>
          </el-icon>
          <span>检测未管理文件</span>
        </div>
        <div class="dm-sidebar-section">
          <div class="dm-section-title">
            <span>根目录</span>
            <el-button size="small" text @click="showAddRoot = true">
              <el-icon>
                <Plus/>
              </el-icon>
            </el-button>
          </div>
          <div ref="rootListRef" class="dm-root-list">
            <div v-for="root in roots" :key="root.id" :class="{ active: rootFilter === root.id }"
                 :data-root-id="root.id" class="dm-root-item sortable-root"
                 @click="rootFilter = rootFilter === root.id ? null : root.id">
              <el-icon>
                <Folder/>
              </el-icon>
              <span class="dm-root-name">{{ root.name }}</span>
              <el-dropdown class="dm-cat-more" trigger="click" @click.stop>
                <el-icon>
                  <MoreFilled/>
                </el-icon>
                <template #dropdown>
                  <el-dropdown-menu>
                    <el-dropdown-item style="color: var(--el-color-danger)" @click="removeRootFn(root.id)">删除
                    </el-dropdown-item>
                  </el-dropdown-menu>
                </template>
              </el-dropdown>
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
            <div :class="{ active: categoryFilter === null }" class="dm-category-item dm-cat-all"
                 @click="categoryFilter = null">
              <el-icon>
                <Document/>
              </el-icon>
              <span class="dm-root-name">全部</span>
              <span v-if="stats?.totalFiles" class="dm-cat-count">{{ stats.totalFiles }}</span>
              <el-icon class="dm-dots-spacer">
                <MoreFilled/>
              </el-icon>
            </div>
            <div ref="catListRef" class="dm-sort-list">
              <div v-for="cat in categories" :key="cat.id" :class="{ active: categoryFilter === cat.id }"
                   :data-cat-id="cat.id"
                   class="dm-category-item sortable-category"
                   @click="categoryFilter = categoryFilter === cat.id ? null : cat.id">
                <el-icon :style="{ color: cat.color }">
                  <Folder/>
                </el-icon>
                <span class="dm-root-name">{{ cat.name }}</span>
                <span v-if="getCatCount(cat.id) > 0" class="dm-cat-count">{{ getCatCount(cat.id) }}</span>
                <el-dropdown class="dm-cat-more" trigger="click" @click.stop>
                  <el-icon>
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
            </div>
            <div v-if="categories && categories.length > 0" :class="{ active: categoryFilter === -1 }"
                 class="dm-category-item dm-cat-uncat" @click="categoryFilter = categoryFilter === -1 ? null : -1">
              <el-icon>
                <Folder/>
              </el-icon>
              <span class="dm-root-name">未分类</span>
              <span v-if="uncatCount > 0" class="dm-cat-count">{{ uncatCount }}</span>
              <el-icon class="dm-dots-spacer">
                <MoreFilled/>
              </el-icon>
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
              <el-input v-model="searchKeyword" clearable placeholder="搜索文件名、标签、内容..." @clear="searchFiles">
                <template #prefix>
                  <el-icon>
                    <Search/>
                  </el-icon>
                </template>
              </el-input>
              <el-select v-model="fileExtFilter" clearable placeholder="全部类型" size="small"
                         style="width:100px;margin-left:8px" @change="searchFiles">
                <el-option v-for="ext in commonExts" :key="ext.value" :label="ext.label" :value="ext.value"/>
              </el-select>
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
          <div v-else ref="fileGridRef" class="dm-file-grid" @click.self="selectedId = null">
            <div v-for="item in items" :key="item.id" :data-file-id="item.id"
                 :class="{ selected: selectedId === item.id, 'ctx-anchor': ctxAnchorId === item.id }"
                 class="dm-file-card sortable-file"
                 @click="selectedId = item.id" @dblclick="openDocument(item)"
                 @contextmenu.prevent="showContextMenu($event, item)">
              <span v-if="item.storageMode === 'repo'" class="dm-mode-badge repo">搬迁</span>
              <span v-else class="dm-mode-badge index">索引</span>
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
                  <el-button size="small" text type="danger" @click="undoImportItemFn(h.id, f.docFileId, h)">撤销
                  </el-button>
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
          <el-select v-model="scanCategoryId" clearable placeholder="未分类" style="width:100%">
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

  <el-dialog v-model="showGuide" title="欢迎使用文档管理" width="560px">
    <div class="dm-guide-body">
      <p class="dm-guide-desc">文档管理帮助你集中管理电脑中的各类文件，支持分类、搜索、搬迁或索引两种方式。</p>
      <div class="dm-guide-steps">
        <div v-for="(step, idx) in guideSteps" :key="idx" class="dm-guide-step">
          <div class="dm-guide-step-num">{{ idx + 1 }}</div>
          <div class="dm-guide-step-content">
            <div class="dm-guide-step-title">{{ step.title }}</div>
            <div class="dm-guide-step-desc">{{ step.desc }}</div>
          </div>
        </div>
      </div>
      <div class="dm-guide-footer">
        <el-checkbox v-model="guideNoMore">不再显示</el-checkbox>
      </div>
    </div>
    <template #footer>
      <el-button type="primary" @click="dismissGuide">开始使用</el-button>
    </template>
  </el-dialog>

  <el-dialog v-model="showOrphanDialog" title="未管理文件" width="560px">
    <div v-if="orphanLoading" style="text-align:center;padding:20px">
      <el-icon :size="32" class="is-loading">
        <Loading/>
      </el-icon>
      <p>正在扫描...</p>
    </div>
    <div v-else-if="orphanResults.length === 0" style="text-align:center;padding:20px">
      <el-empty description="未发现未管理文件"/>
    </div>
    <div v-else class="dm-orphan-list">
      <div v-for="result in orphanResults" :key="result.rootId" class="dm-orphan-group">
        <div class="dm-orphan-group-title">{{ result.rootName }}</div>
        <div v-for="f in result.files" :key="f.path" :class="{ checked: orphanSelected.has(f.path) }"
             class="dm-scan-file-item" @click="toggleOrphan(f.path)">
          <el-icon v-if="orphanSelected.has(f.path)"><Select/></el-icon>
          <el-icon v-else>
            <Document/>
          </el-icon>
          <span>{{ f.name }}</span>
          <span v-if="f.categoryName" class="dm-orphan-cat">{{ f.categoryName }}</span>
          <span class="dm-scan-size">{{ formatFileSize(f.size) }}</span>
        </div>
      </div>
    </div>
    <template #footer>
      <el-button @click="showOrphanDialog = false">关闭</el-button>
      <el-button v-if="orphanResults.length > 0" @click="toggleOrphanSelectAll">
        {{ orphanSelected.size === totalOrphanCount ? '取消全选' : '全选' }}
      </el-button>
      <el-button :disabled="orphanSelected.size === 0" type="primary" @click="importOrphans">
        导入选中 ({{ orphanSelected.size }})
      </el-button>
    </template>
  </el-dialog>
</template>

<script setup>
import {computed, nextTick, onMounted, reactive, ref, watch} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {DocumentService} from '@/services/ipc.js'
import {open as openDialog} from '@tauri-apps/plugin-dialog'
import {
  ArrowRight,
  Close,
  Coffee,
  Connection,
  Document,
  Folder,
  List,
  Loading,
  MagicStick,
  Monitor,
  MoreFilled,
  Notebook,
  Picture,
  Plus,
  Search,
  Select,
  Setting,
  Tickets,
  UploadFilled,
  Warning,
} from '@element-plus/icons-vue'
import ContextMenu from '../../components/ContextMenu.vue'
import Sortable from 'sortablejs'

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

const commonExts = [
  {label: 'PDF', value: 'pdf'},
  {label: 'Word', value: 'docx'},
  {label: 'Excel', value: 'xlsx'},
  {label: 'PPT', value: 'pptx'},
  {label: 'Markdown', value: 'md'},
  {label: 'Text', value: 'txt'},
  {label: 'JSON', value: 'json'},
  {label: 'Python', value: 'py'},
  {label: 'JS/TS', value: 'js'},
  {label: '图片', value: 'png'},
  {label: 'HTML', value: 'html'},
  {label: 'CSS', value: 'css'},
  {label: 'CSV', value: 'csv'},
  {label: 'SQL', value: 'sql'},
  {label: 'XML', value: 'xml'},
  {label: 'YAML', value: 'yaml'},
  {label: 'Go', value: 'go'},
  {label: 'Rust', value: 'rs'},
  {label: 'Java', value: 'java'},
]

const categories = ref([])
const roots = ref([])
const items = ref([])
const stats = ref(null)
const loading = ref(false)
const searchKeyword = ref('')
const fileExtFilter = ref(null)
const categoryFilter = ref(null)
const rootFilter = ref(null)
const currentPage = ref(1)
const pageLimit = 50
const total = ref(0)
const selectedId = ref(null)
const dragover = ref(false)
const activeTab = ref('files')
const importHistory = ref([])

const rootListRef = ref(null)
const catListRef = ref(null)
const fileGridRef = ref(null)
let rootSortable = null
let catSortable = null
let fileSortable = null

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
const scanCategoryId = ref(null)
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

const orphanCount = ref(0)
const orphanChecked = ref(false)
const orphanLoading = ref(false)
const showOrphanDialog = ref(false)
const orphanResults = ref([])
const orphanSelected = reactive(new Set())

const selectedDoc = computed(() => {
  if (selectedId.value === null) return null
  return items.value.find(i => i.id === selectedId.value) || null
})

const hasMoveChange = computed(() => {
  if (!moveDoc.value) return false
  return moveTargetCategoryId.value !== (moveDoc.value.categoryId ?? null)
      || (moveTargetRootId.value && moveTargetRootId.value !== moveDoc.value.rootId)
})

const totalOrphanCount = computed(() => {
  let n = 0
  for (const r of orphanResults.value) n += r.files?.length || 0
  return n
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

const uncatCount = computed(() => {
  if (!stats.value?.categoryCounts) return 0
  const e = stats.value.categoryCounts.find(c => c.categoryId === null)
  return e?.count || 0
})

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
  nextTick(() => {
    initRootSortable();
    initCatSortable()
  })
}

async function loadFiles() {
  loading.value = true
  try {
    const r = await DocumentService.getPage({
      offset: (currentPage.value - 1) * pageLimit, limit: pageLimit,
      categoryId: categoryFilter.value === -1 ? -1 : categoryFilter.value, rootId: rootFilter.value,
      keyword: searchKeyword.value || null,
      fileExt: fileExtFilter.value || null,
    })
    items.value = r.items || []
    total.value = Number(r.total) || 0
    selectedId.value = null
  } catch (e) {
    ElMessage.error('加载文件列表失败')
  } finally {
    loading.value = false
  }
  await nextTick()
  initFileSortable()
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

async function undoImportItemFn(importId, docFileId, historyItem) {
  try {
    await DocumentService.undoImportItem(importId, docFileId)
    ElMessage.success('已撤销')
    if (historyItem._files) {
      historyItem._files = historyItem._files.filter(f => f.docFileId !== docFileId)
    }
    historyItem.fileCount = Math.max(0, (historyItem.fileCount || 1) - 1)
    if (historyItem.fileCount === 0) {
      importHistory.value = importHistory.value.filter(h => h.id !== importId)
    }
    await loadData();
    await loadFiles()
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

let searchTimer = null
watch(searchKeyword, (val, old) => {
  if (val === old) return
  clearTimeout(searchTimer)
  searchTimer = setTimeout(() => {
    currentPage.value = 1
    loadFiles()
  }, 250)
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
    if (e && e !== 'cancel' && e !== 'close') {
      ElMessage.error(typeof e === 'string' ? e : e.message || '删除失败')
    }
  }
}

async function removeRootFn(id) {
  try {
    await ElMessageBox.confirm('确认删除该管理目录？', '确认删除');
    await DocumentService.removeRoot(id);
    ElMessage.success('已删除');
    rootFilter.value = null;
    await loadData();
    loadFiles()
  } catch (e) {
    if (e && e !== 'cancel' && e !== 'close') {
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
      categoryId: scanCategoryId.value || null,
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

function initRootSortable() {
  if (!rootListRef.value) return
  if (rootSortable) rootSortable.destroy()
  rootSortable = Sortable.create(rootListRef.value, {
    animation: 200,
    ghostClass: 'dm-sort-ghost',
    dragClass: 'dm-sort-drag',
    chosenClass: 'dm-sort-chosen',
    forceFallback: true,
    delay: 500,
    fallbackClass: 'dm-sort-fallback',
    fallbackTolerance: 3,
    fallbackOnBody: true,
    swapThreshold: 0.65,
    filter: '.dm-cat-more',
    preventOnFilter: false,
    onEnd: async (evt) => {
      const {oldIndex, newIndex} = evt
      if (oldIndex !== newIndex && oldIndex !== undefined && newIndex !== undefined) {
        const reordered = [...roots.value]
        const [moved] = reordered.splice(oldIndex, 1)
        reordered.splice(newIndex, 0, moved)
        roots.value = reordered
        const ids = roots.value.map(r => r.id)
        await DocumentService.reorderRoots(ids)
      }
    }
  })
}

function initCatSortable() {
  if (!catListRef.value) return
  if (catSortable) catSortable.destroy()
  const el = catListRef.value
  catSortable = Sortable.create(el, {
    animation: 200,
    ghostClass: 'dm-sort-ghost',
    dragClass: 'dm-sort-drag',
    chosenClass: 'dm-sort-chosen',
    forceFallback: true,
    delay: 500,
    fallbackClass: 'dm-sort-fallback',
    fallbackTolerance: 3,
    fallbackOnBody: true,
    swapThreshold: 0.65,
    filter: '.dm-cat-more, .dm-cat-empty',
    preventOnFilter: false,
    onEnd: async (evt) => {
      const {oldIndex, newIndex} = evt
      if (oldIndex !== newIndex && oldIndex !== undefined && newIndex !== undefined) {
        const reordered = [...categories.value]
        const [moved] = reordered.splice(oldIndex, 1)
        reordered.splice(newIndex, 0, moved)
        categories.value = reordered
        const ids = categories.value.map(c => c.id)
        await DocumentService.reorderCategories(ids)
      }
    }
  })
}

function initFileSortable() {
  if (fileSortable) {
    fileSortable.destroy();
    fileSortable = null
  }
  if (!fileGridRef.value || items.value.length === 0) return
  fileSortable = Sortable.create(fileGridRef.value, {
    animation: 200,
    ghostClass: 'dm-sort-ghost',
    dragClass: 'dm-sort-drag',
    chosenClass: 'dm-sort-chosen',
    forceFallback: true,
    delay: 500,
    fallbackClass: 'dm-sort-fallback',
    fallbackTolerance: 3,
    fallbackOnBody: true,
    swapThreshold: 0.65,
    onStart: () => {
      setTimeout(() => {
        const el = document.querySelector('.dm-sort-fallback')
        if (!el) return
        const move = (e) => {
          el.style.left = (e.clientX - el.offsetWidth / 2) + 'px';
          el.style.top = (e.clientY - el.offsetHeight / 2) + 'px'
        }
        document.addEventListener('mousemove', move)
        document.addEventListener('mouseup', function up() {
          document.removeEventListener('mousemove', move);
          document.removeEventListener('mouseup', up)
        })
      }, 50)
    },
    onEnd: async (evt) => {
      const {oldIndex, newIndex} = evt
      if (oldIndex !== newIndex && oldIndex !== undefined && newIndex !== undefined) {
        const reordered = [...items.value]
        const [moved] = reordered.splice(oldIndex, 1)
        reordered.splice(newIndex, 0, moved)
        items.value = reordered
        const ids = items.value.map(f => f.id)
        await DocumentService.reorderFiles(ids)
      }
    }
  })
}

async function detectOrphans() {
  orphanLoading.value = true
  orphanChecked.value = true
  try {
    const r = await DocumentService.detectOrphanFiles(rootFilter.value || null)
    orphanResults.value = r || []
    let count = 0
    for (const g of orphanResults.value) count += g.files?.length || 0
    orphanCount.value = count
    orphanSelected.clear()
  } catch (e) {
    ElMessage.error('检测失败: ' + e)
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
      }
    }
    ElMessage.success(`成功导入 ${total} 个文件`)
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
    const repoMsg = doc.storageMode === 'repo'
        ? `文件将搬回原位置: ${doc.sourcePath}`
        : `文件保留在磁盘上`
    await ElMessageBox.confirm(`确定删除「${doc.title || doc.fileName}」？${repoMsg}。`, '确认删除', {type: 'warning'});
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
      await DocumentService.updateMeta({id: doc.id, categoryId: moveTargetCategoryId.value ?? -1})
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

const GUIDE_KEY = 'dm_guide_dismissed'

const guideSteps = [
  {
    title: '添加根目录',
    desc: '点击左侧「添加目录」，选择一个本地文件夹作为文档仓库。可添加多个根目录，支持搬迁和索引两种模式。'
  },
  {title: '创建分类', desc: '点击分类栏的 + 按钮创建分类（如"工作""学习"），用于归类文档。支持自定义图标颜色、拖曳排序。'},
  {
    title: '导入文档',
    desc: '点击「添加文档」选择文件，或「扫描文件夹」批量导入。搬迁模式会将文件复制到仓库，索引模式仅记录位置。'
  },
  {
    title: '管理文档',
    desc: '单击文件查看详细信息和编辑标签，双击直接打开，右键可移动文件到其他分类/目录或删除。支持文件名和内容全文搜索。'
  },
]

const showGuide = ref(localStorage.getItem(GUIDE_KEY) !== '1')
const guideNoMore = ref(false)

function dismissGuide() {
  if (guideNoMore.value) {
    localStorage.setItem(GUIDE_KEY, '1')
  }
  showGuide.value = false
}

async function saveCategory() {
  if (!selectedDoc.value) return;
  try {
    await DocumentService.updateMeta({id: selectedDoc.value.id, categoryId: editCategoryId.value ?? -1})
    selectedDoc.value.categoryId = editCategoryId.value
  } catch {
  }
  ;loadData()
}

async function saveTags() {
  if (!selectedDoc.value) return
  try {
    const tagsJson = JSON.stringify(editTags.value.split(/[,，]/).map(t => t.trim()).filter(Boolean))
    await DocumentService.updateMeta({id: selectedDoc.value.id, tags: tagsJson})
    selectedDoc.value.tags = tagsJson
  } catch {
  }
}

async function saveNotes() {
  if (!selectedDoc.value) return;
  try {
    await DocumentService.updateMeta({id: selectedDoc.value.id, notes: editNotes.value})
    selectedDoc.value.notes = editNotes.value
  } catch {
  }
}

onMounted(async () => {
  await loadData();
  await loadFiles();
  loadImportHistory();
  detectOrphans()
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

.dm-sort-ghost {
  opacity: 0.4;
  background: var(--el-color-primary-light-9) !important;
  border: 2px dashed var(--el-color-primary) !important
}

.dm-sort-drag {
  opacity: 0.3
}

.dm-sort-chosen {
  opacity: 1 !important;
  transform: scale(1.05);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  z-index: 10;
  border-color: var(--el-color-primary) !important
}

.dm-sort-fallback {
  opacity: 0.95 !important;
  background: var(--el-bg-color) !important;
  border: 2px solid var(--el-color-primary) !important;
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  cursor: grabbing !important;
  pointer-events: none;
  position: fixed !important;
  z-index: 9999 !important;
  overflow: hidden
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

.dm-sort-list {
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

.dm-root-item.sortable-root,
.dm-category-item.sortable-category {
  cursor: grab;
  user-select: none
}

.dm-root-item.sortable-root:active,
.dm-category-item.sortable-category:active {
  cursor: grabbing
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
  font-size: 11px;
  color: var(--el-text-color-secondary);
  background: var(--el-fill-color);
  padding: 1px 6px;
  border-radius: 10px
}

.dm-cat-more {
  opacity: 0;
  transition: opacity .15s;
  margin-left: auto
}

.dm-category-item:hover .dm-cat-more {
  opacity: 1
}

.dm-root-item:hover .dm-cat-more {
  opacity: 1
}

.dm-dots-spacer {
  visibility: hidden;
  flex-shrink: 0
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
  max-width: 500px;
  display: flex;
  align-items: center
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
  overflow-y: auto;
  padding: 12px 16px;
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
  background: var(--el-bg-color);
  position: relative
}

.dm-mode-badge {
  position: absolute;
  top: 2px;
  right: 2px;
  font-size: 10px;
  padding: 0 4px;
  border-radius: 3px;
  line-height: 16px;
  pointer-events: none
}

.dm-mode-badge.repo {
  color: var(--el-color-primary);
  background: var(--el-color-primary-light-9)
}

.dm-mode-badge.index {
  color: var(--el-text-color-secondary);
  background: var(--el-fill-color-light)
}

.dm-file-card.sortable-file {
  cursor: grab;
  user-select: none
}

.dm-file-card.sortable-file:active {
  cursor: grabbing
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
  font-weight: 500;
  gap: 8px;
}

.dm-detail-header > span:first-child {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.dm-detail-header .el-button {
  flex-shrink: 0;
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

.dm-guide-body {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.dm-guide-desc {
  font-size: 14px;
  color: var(--el-text-color-regular);
  margin: 0;
  line-height: 1.6;
}

.dm-guide-steps {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.dm-guide-step {
  display: flex;
  gap: 14px;
  align-items: flex-start;
}

.dm-guide-step-num {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  background: var(--el-color-primary);
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  line-height: 1;
}

.dm-guide-step-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  margin-bottom: 4px;
}

.dm-guide-step-desc {
  font-size: 13px;
  color: var(--el-text-color-secondary);
  line-height: 1.5;
}

.dm-guide-footer {
  display: flex;
  justify-content: flex-start;
  padding-top: 4px;
}
</style>
