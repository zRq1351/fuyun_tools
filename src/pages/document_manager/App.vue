<template>
  <div class="doc-manager" @dragover.prevent="dragover = true" @dragleave.prevent="dragover = false"
       @drop.prevent="handleDrop">
    <div class="dm-layout">
      <div class="dm-sidebar">
        <div class="dm-sidebar-stats">
          <div class="dm-stat">
            <span class="dm-stat-value">{{ stats?.totalFiles ?? 0 }}</span>
            <span class="dm-stat-label">{{ t('documentManager.allDocs') }}</span>
          </div>
          <div class="dm-stat">
            <span class="dm-stat-value">{{ stats?.totalSize ? formatFileSize(stats.totalSize) : '0 B' }}</span>
            <span class="dm-stat-label">{{ t('documentManager.totalSize') }}</span>
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
          <span>{{ t('documentManager.detectOrphan') }}</span>
        </div>
        <div class="dm-sidebar-section">
          <div class="dm-section-title">
            <span>{{ t('documentManager.rootDir') }}</span>
            <el-button size="small" text @click="showAddRoot = true">
              <el-icon>
                <Plus/>
              </el-icon>
            </el-button>
          </div>
          <div ref="rootListRef" class="dm-root-list">
            <div v-for="root in roots" :key="root.id" :class="{ active: rootFilter === root.id }"
                 :data-root-id="root.id" class="dm-root-item sortable-root"
                 @click="selectRoot(root, $event)">
              <el-icon>
                <Folder/>
              </el-icon>
              <span class="dm-root-name">{{ root.name }}</span>
              <el-dropdown trigger="click">
                <span class="dm-cat-more">
                  <el-icon>
                    <MoreFilled/>
                  </el-icon>
                </span>
                <template #dropdown>
                  <el-dropdown-menu>
                    <el-dropdown-item style="color: var(--el-color-danger)" @click="removeRootFn(root.id)">
                      {{ t('common.delete') }}
                    </el-dropdown-item>
                  </el-dropdown-menu>
                </template>
              </el-dropdown>
            </div>
          </div>
        </div>
        <div class="dm-sidebar-section">
          <div class="dm-section-title">
            <span>{{ t('documentManager.categories') }}</span>
            <el-button :disabled="rootFilter === null" size="small" text @click="showAddCategory = true">
              <el-icon>
                <Plus/>
              </el-icon>
            </el-button>
          </div>
          <div class="dm-category-list">
            <div v-if="rootFilter !== null" :class="{ active: categoryFilter === null }"
                 class="dm-category-item dm-cat-all"
                 @click="categoryFilter = null">
              <el-icon>
                <Document/>
              </el-icon>
              <span class="dm-root-name">{{ t('documentManager.all') }}</span>
              <span v-if="stats?.totalFiles" class="dm-cat-count">{{ stats.totalFiles }}</span>
              <el-icon class="dm-dots-spacer">
                <MoreFilled/>
              </el-icon>
            </div>
            <div ref="catListRef" class="dm-sort-list">
              <div v-for="cat in visibleCategories" :key="cat.id" :class="{ active: categoryFilter === cat.id }"
                   :data-cat-id="cat.id"
                   class="dm-category-item sortable-category"
                   @click="selectCategory(cat, $event)">
                <el-icon :style="{ color: cat.color }">
                  <component :is="getCatIcon(cat.icon)"/>
                </el-icon>
                <span class="dm-root-name">{{ cat.name }}</span>
                <span v-if="getCatCount(cat.id) > 0" class="dm-cat-count">{{ catCountMap.get(cat.id) || 0 }}</span>
                <el-dropdown trigger="click">
                  <span class="dm-cat-more">
                    <el-icon>
                      <MoreFilled/>
                    </el-icon>
                  </span>
                  <template #dropdown>
                    <el-dropdown-menu>
                      <el-dropdown-item @click="startRenameCat(cat)">{{
                          t('documentManager.rename')
                        }}
                      </el-dropdown-item>
                      <el-dropdown-item style="color: var(--el-color-danger)" @click="removeCategoryFn(cat.id)">
                        {{ t('common.delete') }}
                      </el-dropdown-item>
                    </el-dropdown-menu>
                  </template>
                </el-dropdown>
              </div>
            </div>
            <div v-if="rootFilter !== null && categories && categories.length > 0"
                 :class="{ active: categoryFilter === -1 }"
                 class="dm-category-item dm-cat-uncat" @click="categoryFilter = categoryFilter === -1 ? null : -1">
              <el-icon>
                <Folder/>
              </el-icon>
              <span class="dm-root-name">{{ t('documentManager.uncategorized') }}</span>
              <span v-if="uncatCount > 0" class="dm-cat-count">{{ uncatCount }}</span>
              <el-icon class="dm-dots-spacer">
                <MoreFilled/>
              </el-icon>
            </div>
            <div v-if="!visibleCategories || visibleCategories.length === 0" class="dm-category-item dm-cat-empty">
              <span>{{
                  rootFilter === null ? t('documentManager.selectRootDir') : t('documentManager.noCategories')
                }}</span>
            </div>
          </div>
        </div>
      </div>

      <div class="dm-main">
        <div class="dm-toolbar">
          <div class="dm-tabs">
            <button :class="['dm-tab', { active: activeTab === 'files' }]" @click="activeTab = 'files'">
              {{ t('documentManager.documents') }}
            </button>
            <button :class="['dm-tab', { active: activeTab === 'history' }]"
                    @click="activeTab = 'history'; loadImportHistory()">{{ t('documentManager.history') }}
            </button>
          </div>
          <template v-if="activeTab === 'files'">
            <div class="dm-search">
              <el-input v-model="searchKeyword" :placeholder="t('documentManager.searchPlaceholder')" clearable
                        @clear="searchFiles">
                <template #prefix>
                  <el-icon>
                    <Search/>
                  </el-icon>
                </template>
              </el-input>
              <el-select v-model="fileExtFilter" :placeholder="t('documentManager.allTypes')" clearable size="small"
                         style="width:100px;margin-left:8px" @change="searchFiles">
                <el-option v-for="ext in commonExts" :key="ext.value" :label="ext.label" :value="ext.value"/>
              </el-select>
            </div>
            <div class="dm-toolbar-actions">
              <el-button type="primary" @click="openImportDialog">
                <el-icon>
                  <Plus/>
                </el-icon>
                {{ t('documentManager.addDocument') }}
              </el-button>
              <el-button @click="openScanDialog">
                <el-icon>
                  <Search/>
                </el-icon>
                {{ t('documentManager.scanFolder') }}
              </el-button>
            </div>
          </template>
        </div>

        <template v-if="activeTab === 'files'">
          <div v-if="items.length === 0 && !loading" class="dm-empty">
            <el-empty :description="t('documentManager.noDocs')"/>
          </div>
          <div v-else-if="loading" class="dm-empty">
            <el-icon :size="32" class="is-loading">
              <Loading/>
            </el-icon>
            <p style="margin-top:12px;color:var(--el-text-color-secondary)">{{ t('documentManager.loading') }}</p>
          </div>
          <div v-else ref="fileGridRef" class="dm-file-grid" @click.self="selectedId = null">
            <div v-for="item in items" :key="item.id" :data-file-id="item.id"
                 :class="{ selected: selectedId === item.id, 'ctx-anchor': ctxAnchorId === item.id }"
                 class="dm-file-card sortable-file"
                 @click="selectedId = item.id" @dblclick="openDocument(item)"
                 @contextmenu.prevent="showContextMenu($event, item)">
              <span v-if="item.storageMode === 'repo'" class="dm-mode-badge repo">{{
                  t('documentManager.migrate')
                }}</span>
              <span v-else class="dm-mode-badge index">{{ t('documentManager.index') }}</span>
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
              <div class="dm-detail-row"><span class="dm-detail-label">{{ t('documentManager.fileName') }}</span><span>{{
                  selectedDoc.fileName
                }}</span></div>
              <div class="dm-detail-row"><span
                  class="dm-detail-label">{{
                  t('documentManager.size')
                }}</span><span>{{ formatFileSize(selectedDoc.fileSize) }}</span></div>
              <div class="dm-detail-row"><span
                  class="dm-detail-label">{{
                  t('documentManager.type')
                }}</span><span>{{ selectedDoc.fileExt.toUpperCase() }}</span></div>
              <div class="dm-detail-row"><span class="dm-detail-label">{{ t('documentManager.path') }}</span><span
                  :title="selectedDoc.managedPath"
                                                                                        class="dm-detail-path">{{
                  selectedDoc.managedPath
                }}</span></div>
              <div class="dm-detail-row"><span class="dm-detail-label">{{ t('documentManager.mode') }}</span><span>{{
                  selectedDoc.storageMode === 'repo' ? t('documentManager.migrate') : t('documentManager.index')
                }}</span></div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.category') }}</span>
                <el-select v-model="editCategoryId" :placeholder="t('documentManager.uncategorized')" clearable
                           size="small" @change="saveCategory">
                  <el-option v-for="cat in categories" :key="cat.id" :label="cat.name" :value="cat.id"/>
                </el-select>
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.tags') }}</span>
                <el-input v-model="editTags" :placeholder="t('documentManager.tagsHint')" size="small" @blur="saveTags"
                          @keyup.enter="saveTags"/>
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.notes') }}</span>
                <el-input v-model="editNotes" :placeholder="t('documentManager.notesPlaceholder')" :rows="2"
                          size="small" type="textarea"
                          @blur="saveNotes"/>
              </div>
            </div>
            <div class="dm-detail-actions">
              <el-button size="small" @click="openDocument(selectedDoc)">{{ t('documentManager.open') }}</el-button>
              <el-button size="small" @click="openFolder(selectedDoc)">{{ t('documentManager.locate') }}</el-button>
              <el-button size="small" type="danger" @click="confirmDelete(selectedDoc)">{{
                  t('common.delete')
                }}
              </el-button>
            </div>
          </div>
        </template>

        <template v-if="activeTab === 'history'">
          <div v-if="importHistory.length === 0" class="dm-empty">
            <el-empty :description="t('documentManager.noHistory')"/>
          </div>
          <div v-else class="dm-history-page">
            <div v-for="h in importHistory" :key="h.id" class="dm-history-card">
              <div class="dm-history-card-hd">
                <span class="dm-history-badge">{{
                    h.storageMode === 'repo' ? t('documentManager.migrate') : t('documentManager.index')
                  }}</span>
                <span class="dm-history-fc">{{ h.fileCount }} {{ t('documentManager.fileCount') }}</span>
                <span class="dm-history-time">{{ formatTime(h.createdAt) }}</span>
              </div>
              <div class="dm-history-card-body">
                <div class="dm-history-row"><span class="dm-history-label">{{
                    t('documentManager.sourceDir')
                  }}</span><span
                    class="dm-history-val">{{ h.sourceDir }}</span></div>
                <div class="dm-history-row"><span class="dm-history-label">{{
                    t('documentManager.targetDir')
                  }}</span><span class="dm-history-val">{{
                    h.targetDir
                  }}</span></div>
              </div>
              <div class="dm-history-card-ft">
                <el-button size="small" text @click="toggleHistoryFiles(h.id)">
                  {{ h._files ? t('documentManager.collapse') : t('documentManager.expand') }} ({{
                    h.fileCount
                  }}{{ t('documentManager.fileCount') }})
                </el-button>
                <el-popconfirm :title="t('documentManager.undoConfirm')" @confirm="undoImportFn(h.id)">
                  <template #reference>
                    <el-button plain size="small" type="danger">{{ t('documentManager.undo') }}</el-button>
                  </template>
                </el-popconfirm>
              </div>
              <div v-if="h._files" class="dm-history-files">
                <div v-for="f in h._files" :key="f.sourcePath" class="dm-history-file-item">
                  <el-icon>
                    <Document/>
                  </el-icon>
                  <span>{{ f.fileName }}</span>
                  <el-button size="small" text type="danger" @click="undoImportItemFn(h.id, f.docFileId, h)">
                    {{ t('documentManager.undo') }}
                  </el-button>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>

    <el-dialog v-model="showAddRoot" :title="t('documentManager.addRootDir')" width="480px">
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.dirAlias')">
          <el-input v-model="newRootName" :placeholder="t('documentManager.dirAliasPlaceholder')"/>
        </el-form-item>
        <el-form-item :label="t('documentManager.dirPath')">
          <div style="display:flex;gap:8px;width:100%">
            <el-input v-model="newRootPath" :placeholder="t('documentManager.dirPathPlaceholder')"/>
            <el-button @click="browseRootPath">{{ t('common.browse') }}</el-button>
          </div>
        </el-form-item>
      </el-form>
      <div class="dm-hint-warn">
        <el-icon>
          <Warning/>
        </el-icon>
        <span>{{ t('documentManager.dirPathHint') }}</span></div>
      <template #footer>
        <el-button @click="showAddRoot = false">{{ t('common.cancel') }}</el-button>
        <el-button :disabled="!newRootName || !newRootPath" type="primary" @click="confirmAddRoot">
          {{ t('common.confirm') }}
        </el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showAddCategory" :title="t('documentManager.newCategory')" width="420px">
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.categoryName')">
          <el-input v-model="newCategoryName" :placeholder="t('documentManager.categoryNameHint')"/>
        </el-form-item>
        <el-form-item :label="t('documentManager.icon')">
          <div class="dm-icon-picker">
            <span v-for="ic in catIcons" :key="ic.value"
                  :class="{ active: newCategoryIcon === ic.value }"
                  class="dm-icon-option" @click="newCategoryIcon = ic.value">
              <el-icon :size="18"><component :is="ic.component"/></el-icon>
            </span>
          </div>
        </el-form-item>
        <el-form-item :label="t('documentManager.color')">
          <div class="dm-color-picker">
            <span v-for="c in catColors" :key="c"
                  :class="{ active: newCategoryColor === c }"
                  :style="{ background: c }"
                  class="dm-color-option" @click="newCategoryColor = c"/>
          </div>
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showAddCategory = false">{{ t('common.cancel') }}</el-button>
        <el-button :disabled="!newCategoryName.trim()" type="primary" @click="confirmAddCategory">{{
            t('common.confirm')
          }}
        </el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showRenameCatDialog" :title="t('documentManager.renameCategory')" width="400px">
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.newName')">
          <el-input v-model="renameCatName" :placeholder="t('documentManager.newNamePlaceholder')"/>
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showRenameCatDialog = false">{{ t('common.cancel') }}</el-button>
        <el-button type="primary" @click="confirmRenameCat">{{ t('common.confirm') }}</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showImportDialog" :close-on-click-modal="!importing" :close-on-press-escape="!importing"
               :show-close="!importing" :title="t('documentManager.addDoc')"
               width="500px">
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.targetDir')">
          <el-select v-model="importRootId" :placeholder="t('documentManager.selectRoot')" style="width:100%">
            <el-option v-for="r in roots" :key="r.id" :label="r.name" :value="r.id"/>
          </el-select>
        </el-form-item>
        <el-form-item :label="t('documentManager.targetCategory')">
          <el-select v-model="importCategoryId" :placeholder="t('documentManager.uncategorized')" clearable
                     style="width:100%">
            <el-option v-for="c in importCategories" :key="c.id" :label="c.name" :value="c.id"/>
          </el-select>
        </el-form-item>
        <el-form-item :label="t('documentManager.importMode')">
          <el-radio-group v-model="importMode">
            <el-radio value="index">{{ t('documentManager.modeIndex') }}</el-radio>
            <el-radio value="repo">{{ t('documentManager.modeMigrate') }}</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item :label="t('documentManager.selectFiles')">
          <el-button @click="browseImportFiles">{{ t('documentManager.selectFiles') }}</el-button>
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
        <span>{{ t('documentManager.fileMoveHint', {path: getImportTargetPath()}) }}</span></div>
      <template #footer>
        <el-button :disabled="importing" @click="showImportDialog = false">{{ t('common.cancel') }}</el-button>
        <el-button :disabled="!importRootId || importFiles.length === 0 || importing" :loading="importing"
                   type="primary" @click="confirmImport">
          {{
            importing ? t('documentManager.importing', {count: importFiles.length}) : t('documentManager.confirmImportBtn')
          }}
        </el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showScanDialog" :close-on-click-modal="!scanImporting" :close-on-press-escape="!scanImporting" :show-close="!scanImporting"
               :title="t('documentManager.scanFolder')" width="500px">
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.scanPath')">
          <div style="display:flex;gap:8px;width:100%">
            <el-input v-model="scanPath" :disabled="scanImporting"
                      :placeholder="t('documentManager.scanPathPlaceholder')"/>
            <el-button :disabled="scanImporting" @click="browseScanPath">{{ t('common.browse') }}</el-button>
          </div>
        </el-form-item>
        <el-form-item :label="t('documentManager.targetDir')">
          <el-select v-model="scanImportRootId" :disabled="scanImporting" :placeholder="t('documentManager.selectRoot')"
                     style="width:100%">
            <el-option v-for="r in roots" :key="r.id" :label="r.name" :value="r.id"/>
          </el-select>
        </el-form-item>
        <el-form-item :label="t('documentManager.targetCategory')">
          <el-select v-model="scanCategoryId" :disabled="scanImporting" :placeholder="t('documentManager.uncategorized')"
                     clearable
                     style="width:100%">
            <el-option v-for="c in scanCategories" :key="c.id" :label="c.name" :value="c.id"/>
          </el-select>
        </el-form-item>
        <el-form-item :label="t('documentManager.importMode')">
          <el-radio-group v-model="importMode" :disabled="scanImporting">
            <el-radio value="index">{{ t('documentManager.index') }}</el-radio>
            <el-radio value="repo">{{ t('documentManager.migrate') }}</el-radio>
          </el-radio-group>
        </el-form-item>
      </el-form>
      <div v-if="scanning" style="text-align:center;padding:20px">
        <el-icon :size="32" class="is-loading">
          <Loading/>
        </el-icon>
        <p>{{ t('documentManager.scanning') }}</p></div>
      <div v-else-if="scannedFiles.length > 0" class="dm-scan-list">
        <p>{{ t('documentManager.foundFiles', {count: scannedFiles.length}) }}</p>
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
        <el-button :disabled="scanImporting" @click="showScanDialog = false">{{ t('common.cancel') }}</el-button>
        <el-button :disabled="scannedFiles.length === 0 || scanImporting" @click="toggleScanSelectAll">
          {{
            scanSelected.size === scannedFiles.length ? t('documentManager.deselectAll') : t('documentManager.selectAll')
          }}
        </el-button>
        <el-button :disabled="scanSelected.size === 0 || !scanImportRootId || scanImporting"
                   :loading="scanImporting" type="primary" @click="importScanned">
          {{
            scanImporting ? t('documentManager.importing', {count: scanSelected.size}) : t('documentManager.importSelected', {count: scanSelected.size})
          }}
        </el-button>
      </template>
    </el-dialog>

    <div v-if="dragover" class="dm-drop-overlay">
      <el-icon :size="48">
        <UploadFilled/>
      </el-icon>
      <p>{{ t('documentManager.dropToAdd') }}</p>
    </div>
  </div>

  <ContextMenu :show="ctxMenuVisible" :x="ctxMenuX" :y="ctxMenuY" @close="closeCtxMenu">
    <div class="context-menu-item" @click="startMove(ctxMenuDoc)">
      <el-icon :size="14">
        <Folder/>
      </el-icon>
      <span>{{ t('common.move') }}</span>
    </div>
    <div class="context-menu-divider"></div>
    <div class="context-menu-item context-menu-item-danger" @click="contextDelete(ctxMenuDoc)">
      <el-icon :size="14">
        <Close/>
      </el-icon>
      <span>{{ t('common.delete') }}</span>
    </div>
  </ContextMenu>

  <el-dialog v-model="showMoveDialog" :title="t('documentManager.moveFile')" width="420px" @closed="closeCtxMenu">
    <div class="dm-move-body">
      <div class="dm-move-section">
        <div class="dm-move-section-title">{{ t('documentManager.moveToRoot') }}</div>
        <el-select v-model="moveTargetRootId" :placeholder="t('documentManager.selectRoot')" style="width:100%"
                   @change="onMoveRootChange">
          <el-option v-for="root in roots" :key="root.id" :label="root.name" :value="root.id"/>
        </el-select>
        <div v-if="moveDoc.storageMode === 'repo'" class="dm-move-hint">{{ t('documentManager.migrateHint') }}</div>
      </div>
      <div class="dm-move-section">
        <div class="dm-move-section-title">{{ t('documentManager.moveToCategory') }}</div>
        <el-select v-model="moveTargetCategoryId" :placeholder="t('documentManager.uncategorized')" clearable
                   style="width:100%">
          <el-option v-for="cat in moveCategories" :key="cat.id" :label="cat.name" :value="cat.id"/>
        </el-select>
      </div>
    </div>
    <template #footer>
      <el-button @click="showMoveDialog = false">{{ t('common.cancel') }}</el-button>
      <el-button :disabled="!hasMoveChange" type="primary" @click="confirmMove">{{ t('common.ok') }}</el-button>
    </template>
  </el-dialog>

  <el-dialog v-model="showGuide" :title="t('documentManager.welcome')" width="560px">
    <div class="dm-guide-body">
      <p class="dm-guide-desc">{{ t('documentManager.welcomeDesc') }}</p>
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
        <el-checkbox v-model="guideNoMore">{{ t('documentManager.dontShowAgain') }}</el-checkbox>
      </div>
    </div>
    <template #footer>
      <el-button type="primary" @click="dismissGuide">{{ t('documentManager.startUsing') }}</el-button>
    </template>
  </el-dialog>

  <el-dialog v-model="showOrphanDialog" :title="t('documentManager.orphanFilesTitle')" width="560px">
    <div v-if="orphanLoading" style="text-align:center;padding:20px">
      <el-icon :size="32" class="is-loading">
        <Loading/>
      </el-icon>
      <p>{{ t('documentManager.scanning') }}</p>
    </div>
    <div v-else-if="orphanResults.length === 0" style="text-align:center;padding:20px">
      <el-empty :description="t('documentManager.noOrphanFiles')"/>
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
      <el-button @click="showOrphanDialog = false">{{ t('documentManager.close') }}</el-button>
      <el-button v-if="orphanResults.length > 0" @click="toggleOrphanSelectAll">
        {{
          orphanSelected.size === totalOrphanCount ? t('documentManager.deselectAll') : t('documentManager.selectAll')
        }}
      </el-button>
      <el-button :disabled="orphanSelected.size === 0" type="primary" @click="importOrphans">
        {{ t('documentManager.importSelected', {count: orphanSelected.size}) }}
      </el-button>
    </template>
  </el-dialog>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch} from 'vue'
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
import {useI18n} from 'vue-i18n'

const {t} = useI18n()

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
const moveCategories = ref([])

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
  editTags.value = (() => {
    try {
      return JSON.parse(doc.tags || '[]').join('; ')
    } catch {
      return doc.tags || ''
    }
  })()
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

function selectRoot(root, event) {
  if (event.target.closest('.dm-cat-more')) return
  rootFilter.value = root.id
}

function selectCategory(cat, event) {
  if (event.target.closest('.dm-cat-more')) return
  categoryFilter.value = categoryFilter.value === cat.id ? null : cat.id
}

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

function getCatCount(catId) {
  if (!stats.value?.categoryCounts) return 0
  const e = stats.value.categoryCounts.find(c => c.categoryId === catId)
  return e?.count || 0
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

async function loadFiles(preserveSelection) {
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
    if (!preserveSelection || !items.value.find(i => i.id === selectedId.value)) {
      selectedId.value = null
    }
  } catch (e) {
    ElMessage.error(t('documentManager.loadFileListFailed'))
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
  categories.value = [];
  loadData();
  loadFiles()
})

watch(importRootId, async (rid) => {
  importCategoryId.value = null
  importCategories.value = rid ? (await DocumentService.getCategories(rid) || []) : []
})

watch(scanImportRootId, async (rid) => {
  scanCategoryId.value = null
  scanCategories.value = rid ? (await DocumentService.getCategories(rid) || []) : []
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

function initRootSortable() {
  if (!rootListRef.value) {
    if (rootSortable) {
      rootSortable.destroy();
      rootSortable = null
    }
    return
  }
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
  if (!catListRef.value) {
    if (catSortable) {
      catSortable.destroy();
      catSortable = null
    }
    return
  }
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

const GUIDE_KEY = 'dm_guide_dismissed'

const guideSteps = [
  {
    title: t('documentManager.guideAddDir'),
    desc: t('documentManager.guideAddDirDesc')
  },
  {title: t('documentManager.guideCreateCat'), desc: t('documentManager.guideCreateCatDesc')},
  {
    title: t('documentManager.guideImport'),
    desc: t('documentManager.guideImportDesc')
  },
  {
    title: t('documentManager.guideManage'),
    desc: t('documentManager.guideManageDesc')
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

onMounted(async () => {
  await loadData();
  if (roots.value.length > 0) {
    rootFilter.value = roots.value[0].id;
  } else {
    await loadFiles();
  }
  loadImportHistory();
  detectOrphans()
})

onBeforeUnmount(() => {
  if (rootSortable) { rootSortable.destroy(); rootSortable = null }
  if (catSortable) { catSortable.destroy(); catSortable = null }
  if (fileSortable) { fileSortable.destroy(); fileSortable = null }
  if (searchTimer) { clearTimeout(searchTimer); searchTimer = null }
})

</script>

<style scoped>
@import "../shared/contextMenu.css";

.doc-manager {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--fy-bg-primary);
  color: var(--fy-text-primary);
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
  border-right: 0.5px solid var(--fy-border-light);
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  background: var(--fy-bg-secondary)
}

.dm-sidebar-header {
  padding: 16px;
  border-bottom: 0.5px solid var(--fy-border-light)
}

.dm-sidebar-header h3 {
  margin: 0;
  font-size: var(--fy-text-lg)
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
  color: var(--fy-accent)
}

.dm-stat-label {
  font-size: var(--fy-text-sm);
  color: var(--fy-text-secondary)
}

.dm-sort-ghost {
  opacity: 0.4;
  background: var(--fy-accent-bg) !important;
  border: 2px dashed var(--fy-accent) !important
}

.dm-sort-drag {
  opacity: 0.3
}

.dm-sort-chosen {
  opacity: 1 !important;
  transform: scale(1.05);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  z-index: 10;
  border-color: var(--fy-accent) !important
}

.dm-sort-fallback {
  opacity: 0.95 !important;
  background: var(--fy-bg-primary) !important;
  border: 2px solid var(--fy-accent) !important;
  border-radius: var(--fy-radius-md);
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
  font-size: var(--fy-text-sm);
  color: var(--fy-text-secondary);
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
  font-size: var(--fy-text-base);
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
  background: var(--fy-bg-hover)
}

.dm-root-item.active, .dm-category-item.active {
  background: var(--fy-accent-bg);
  color: var(--fy-accent)
}

.dm-root-add {
  color: var(--fy-text-secondary);
  font-size: var(--fy-text-sm)
}

.dm-cat-count {
  font-size: 11px;
  color: var(--fy-text-secondary);
  background: var(--fy-bg-surface);
  padding: 1px 6px;
  border-radius: var(--fy-radius-full)
}

.dm-cat-more {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--fy-radius-sm);
  opacity: 0.6;
  transition: opacity .15s, background .15s;
  margin-left: auto;
  flex-shrink: 0
}

.dm-category-item:hover .dm-cat-more,
.dm-root-item:hover .dm-cat-more {
  opacity: 1;
  background: var(--fy-bg-surface)
}

.dm-cat-more:active {
  background: var(--fy-bg-hover)
}

.dm-dots-spacer {
  visibility: hidden;
  flex-shrink: 0
}

.dm-cat-empty {
  color: var(--fy-text-secondary);
  cursor: default;
  font-size: var(--fy-text-sm);
  padding-left: 16px
}

.dm-icon-picker {
  display: flex;
  flex-wrap: wrap;
  gap: 4px
}

.dm-icon-option {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--fy-radius-sm);
  cursor: pointer;
  border: 2px solid transparent;
  transition: all .15s;
  color: var(--fy-text-secondary)
}

.dm-icon-option:hover {
  background: var(--fy-bg-hover);
  color: var(--fy-accent)
}

.dm-icon-option.active {
  border-color: var(--fy-accent);
  color: var(--fy-accent);
  background: var(--fy-accent-bg)
}

.dm-color-picker {
  display: flex;
  flex-wrap: wrap;
  gap: 6px
}

.dm-color-option {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  cursor: pointer;
  border: 2px solid transparent;
  transition: all .15s
}

.dm-color-option:hover {
  transform: scale(1.15)
}

.dm-color-option.active {
  border-color: var(--fy-text-primary);
  box-shadow: 0 0 0 2px var(--fy-bg-primary), 0 0 0 4px currentColor
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
  border-bottom: 0.5px solid var(--fy-border-light)
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
  font-size: var(--fy-text-md);
  color: var(--fy-text-secondary);
  border-radius: var(--fy-radius-sm);
  transition: all .15s
}

.dm-tab:hover {
  color: var(--fy-text-primary);
  background: var(--fy-bg-hover)
}

.dm-tab.active {
  color: var(--fy-accent);
  background: var(--fy-accent-bg);
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
  border: 0.5px solid var(--fy-border-light);
  border-radius: var(--fy-radius-md);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  transition: all .15s;
  background: var(--fy-bg-surface);
  position: relative
}

.dm-mode-badge {
  position: absolute;
  top: 2px;
  right: 2px;
  font-size: 10px;
  padding: 0 4px;
  border-radius: var(--fy-radius-xs);
  line-height: 16px;
  pointer-events: none
}

.dm-mode-badge.repo {
  color: var(--fy-accent);
  background: var(--fy-accent-bg)
}

.dm-mode-badge.index {
  color: var(--fy-text-secondary);
  background: var(--fy-bg-hover)
}

.dm-file-card.sortable-file {
  cursor: grab;
  user-select: none
}

.dm-file-card.sortable-file:active {
  cursor: grabbing
}

.dm-file-card:hover {
  border-color: var(--fy-border-hover);
  background: var(--fy-accent-bg)
}

.dm-file-card.selected {
  border-color: var(--fy-accent);
  background: var(--fy-accent-bg)
}

.dm-file-card.ctx-anchor {
  border-color: var(--fy-border-hover);
  background: var(--fy-accent-bg)
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
  font-size: var(--fy-text-sm);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.4
}

.dm-file-meta {
  font-size: 11px;
  color: var(--fy-text-secondary);
  display: flex;
  justify-content: center;
  gap: 6px;
  margin-top: 4px
}

.dm-file-ext {
  background: var(--fy-bg-surface);
  padding: 0 4px;
  border-radius: var(--fy-radius-xs);
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
  background: var(--fy-bg-surface);
  border-left: 0.5px solid var(--fy-border-light);
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
  border-bottom: 0.5px solid var(--fy-border-light);
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
  color: var(--fy-text-secondary)
}

.dm-detail-row span {
  font-size: var(--fy-text-base);
  word-break: break-all
}

.dm-detail-path {
  font-size: 11px !important;
  color: var(--fy-text-secondary);
  word-break: break-all;
}

.dm-detail-actions {
  display: flex;
  gap: 8px;
  padding: 12px 16px;
  border-top: 0.5px solid var(--fy-border-light)
}

.dm-drop-overlay {
  position: absolute;
  inset: 0;
  background: var(--fy-accent-bg);
  border: 2px dashed var(--fy-accent);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  z-index: 100;
  pointer-events: none
}

.dm-drop-overlay p {
  font-size: var(--fy-text-lg);
  color: var(--fy-accent)
}

.dm-hint-warn {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  background: var(--fy-warning-bg);
  border-radius: var(--fy-radius-sm);
  margin-top: 12px;
  font-size: var(--fy-text-base);
  color: var(--fy-warning)
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
  font-size: var(--fy-text-base);
  color: var(--fy-text-secondary)
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
  border-radius: var(--fy-radius-xs);
  font-size: var(--fy-text-base)
}

.dm-scan-file-item:hover {
  background: var(--fy-bg-hover)
}

.dm-scan-file-item.checked {
  background: var(--fy-accent-bg);
  color: var(--fy-accent)
}

.dm-scan-size {
  margin-left: auto;
  font-size: 11px;
  color: var(--fy-text-secondary)
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
  background: var(--fy-bg-surface);
  border: 0.5px solid var(--fy-border-light);
  border-radius: var(--fy-radius-md);
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
  color: var(--fy-accent);
  font-size: var(--fy-text-base)
}

.dm-history-fc {
  font-size: var(--fy-text-base);
  color: var(--fy-text-secondary)
}

.dm-history-time {
  margin-left: auto;
  font-size: var(--fy-text-sm);
  color: var(--fy-text-muted)
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
  font-size: var(--fy-text-base)
}

.dm-history-label {
  color: var(--fy-text-secondary);
  white-space: nowrap
}

.dm-history-val {
  color: var(--fy-text-primary);
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
  border-top: 0.5px solid var(--fy-border-light);
  max-height: 200px;
  overflow-y: auto
}

.dm-history-file-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 0;
  font-size: var(--fy-text-sm);
  color: var(--fy-text-secondary)
}

.dm-move-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.dm-move-section-title {
  font-size: var(--fy-text-base);
  color: var(--fy-text-secondary);
  margin-bottom: 6px;
}

.dm-move-hint {
  font-size: var(--fy-text-sm);
  color: var(--fy-warning);
  margin-top: 4px;
}

.dm-guide-body {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.dm-guide-desc {
  font-size: var(--fy-text-md);
  color: var(--fy-text-secondary);
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
  background: var(--fy-accent);
  color: var(--fy-text-primary);
  font-size: var(--fy-text-base);
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  line-height: 1;
}

.dm-guide-step-title {
  font-size: var(--fy-text-md);
  font-weight: 600;
  color: var(--fy-text-primary);
  margin-bottom: 4px;
}

.dm-guide-step-desc {
  font-size: var(--fy-text-base);
  color: var(--fy-text-secondary);
  line-height: 1.5;
}

.dm-guide-footer {
  display: flex;
  justify-content: flex-start;
  padding-top: 4px;
}
</style>
