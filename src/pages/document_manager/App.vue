<template>
  <div
    class="doc-manager"
    @dragover.prevent="dragover = true"
    @dragleave.prevent="dragover = false"
    @drop.prevent="handleDrop"
  >
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
        <div
          v-if="orphanCount > 0"
          class="dm-orphan-banner"
          role="button"
          tabindex="0"
          @click="showOrphanDialog = true"
          @keydown.enter.prevent="showOrphanDialog = true"
          @keydown.space.prevent="showOrphanDialog = true"
        >
          <span class="dm-orphan-badge">{{ orphanCount }}</span>
          <span>个未管理文件</span>
          <el-icon :size="12">
            <ArrowRight />
          </el-icon>
        </div>
        <div
          v-else-if="!orphanChecked"
          class="dm-orphan-banner dm-orphan-banner--hint"
          role="button"
          tabindex="0"
          @click="detectOrphans"
          @keydown.enter.prevent="detectOrphans"
          @keydown.space.prevent="detectOrphans"
        >
          <el-icon :size="14">
            <Search />
          </el-icon>
          <span>{{ t('documentManager.detectOrphan') }}</span>
        </div>
        <div class="dm-sidebar-section">
          <div class="dm-section-title">
            <span>{{ t('documentManager.rootDir') }}</span>
            <el-button
              size="small"
              text
              @click="showAddRoot = true"
            >
              <el-icon>
                <Plus />
              </el-icon>
            </el-button>
          </div>
          <div
            ref="rootListRef"
            class="dm-root-list"
          >
            <div
              v-for="root in roots"
              :key="root.id"
              :class="{ active: rootFilter === root.id }"
              :data-root-id="root.id"
              class="dm-root-item sortable-root"
              role="button"
              tabindex="0"
              @click="selectRoot(root, $event)"
              @keydown.enter.prevent="selectRoot(root, $event)"
              @keydown.space.prevent="selectRoot(root, $event)"
            >
              <el-icon>
                <Folder />
              </el-icon>
              <span class="dm-root-name">{{ root.name }}</span>
              <el-dropdown trigger="click">
                <span class="dm-cat-more">
                  <el-icon>
                    <MoreFilled />
                  </el-icon>
                </span>
                <template #dropdown>
                  <el-dropdown-menu>
                    <el-dropdown-item
                      style="color: var(--el-color-danger)"
                      @click="removeRootFn(root.id)"
                    >
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
            <el-button
              :disabled="rootFilter === null"
              size="small"
              text
              @click="showAddCategory = true"
            >
              <el-icon>
                <Plus />
              </el-icon>
            </el-button>
          </div>
          <div class="dm-category-list">
            <div
              v-if="rootFilter !== null"
              :class="{ active: categoryFilter === null }"
              class="dm-category-item dm-cat-all"
              role="button"
              tabindex="0"
              @click="categoryFilter = null"
              @keydown.enter.prevent="categoryFilter = null"
              @keydown.space.prevent="categoryFilter = null"
            >
              <el-icon>
                <Document />
              </el-icon>
              <span class="dm-root-name">{{ t('documentManager.all') }}</span>
              <span
                v-if="stats?.totalFiles"
                class="dm-cat-count"
              >{{ stats.totalFiles }}</span>
              <el-icon class="dm-dots-spacer">
                <MoreFilled />
              </el-icon>
            </div>
            <div
              ref="catListRef"
              class="dm-sort-list"
            >
              <div
                v-for="cat in visibleCategories"
                :key="cat.id"
                :class="{ active: categoryFilter === cat.id }"
                :data-cat-id="cat.id"
                class="dm-category-item sortable-category"
                role="button"
                tabindex="0"
                @click="selectCategory(cat, $event)"
                @keydown.enter.prevent="selectCategory(cat, $event)"
                @keydown.space.prevent="selectCategory(cat, $event)"
              >
                <el-icon :style="{ color: cat.color }">
                  <component :is="getCatIcon(cat.icon)" />
                </el-icon>
                <span class="dm-root-name">{{ cat.name }}</span>
                <span
                  v-if="(catCountMap.get(cat.id) || 0) > 0"
                  class="dm-cat-count"
                >{{ catCountMap.get(cat.id) || 0 }}</span>
                <el-dropdown trigger="click">
                  <span class="dm-cat-more">
                    <el-icon>
                      <MoreFilled />
                    </el-icon>
                  </span>
                  <template #dropdown>
                    <el-dropdown-menu>
                      <el-dropdown-item @click="startRenameCat(cat)">
                        {{
                          t('documentManager.rename')
                        }}
                      </el-dropdown-item>
                      <el-dropdown-item
                        style="color: var(--el-color-danger)"
                        @click="removeCategoryFn(cat.id)"
                      >
                        {{ t('common.delete') }}
                      </el-dropdown-item>
                    </el-dropdown-menu>
                  </template>
                </el-dropdown>
              </div>
            </div>
            <div
              v-if="rootFilter !== null && categories && categories.length > 0"
              :class="{ active: categoryFilter === -1 }"
              class="dm-category-item dm-cat-uncat"
              role="button"
              tabindex="0"
              @click="categoryFilter = categoryFilter === -1 ? null : -1"
              @keydown.enter.prevent="categoryFilter = categoryFilter === -1 ? null : -1"
              @keydown.space.prevent="categoryFilter = categoryFilter === -1 ? null : -1"
            >
              <el-icon>
                <Folder />
              </el-icon>
              <span class="dm-root-name">{{ t('documentManager.uncategorized') }}</span>
              <span
                v-if="uncatCount > 0"
                class="dm-cat-count"
              >{{ uncatCount }}</span>
              <el-icon class="dm-dots-spacer">
                <MoreFilled />
              </el-icon>
            </div>
            <div
              v-if="!visibleCategories || visibleCategories.length === 0"
              class="dm-category-item dm-cat-empty"
            >
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
            <button
              :class="['dm-tab', { active: activeTab === 'files' }]"
              @click="activeTab = 'files'"
            >
              {{ t('documentManager.documents') }}
            </button>
            <button
              :class="['dm-tab', { active: activeTab === 'history' }]"
              @click="activeTab = 'history'; loadImportHistory()"
            >
              {{ t('documentManager.history') }}
            </button>
          </div>
          <template v-if="activeTab === 'files'">
            <div class="dm-search">
              <el-input
                v-model="searchKeyword"
                :placeholder="t('documentManager.searchPlaceholder')"
                clearable
                @clear="searchFiles"
              >
                <template #prefix>
                  <el-icon>
                    <Search />
                  </el-icon>
                </template>
              </el-input>
              <el-select
                v-model="fileExtFilter"
                :placeholder="t('documentManager.allTypes')"
                clearable
                size="small"
                style="width:100px;margin-left:8px"
                @change="searchFiles"
              >
                <el-option
                  v-for="ext in commonExts"
                  :key="ext.value"
                  :label="ext.label"
                  :value="ext.value"
                />
              </el-select>
              <el-select
                v-model="tagFilter"
                :placeholder="t('documentManager.tagFilter')"
                clearable
                multiple
                size="small"
                style="width:160px;margin-left:8px"
                @change="searchFiles"
              >
                <el-option
                  v-for="tg in allTags"
                  :key="tg.tag"
                  :label="`${tg.tag} (${tg.count})`"
                  :value="tg.tag"
                />
              </el-select>
            </div>
            <div
              v-if="multiSelectedIds.size > 0"
              class="dm-batch-bar"
            >
              <span>{{ t('documentManager.batchSelected', {n: multiSelectedIds.size}) }}</span>
              <el-button
                size="small"
                @click="promptBatchTags('add')"
              >
                {{ t('documentManager.batchAddTags') }}
              </el-button>
              <el-button
                size="small"
                @click="promptBatchTags('remove')"
              >
                {{ t('documentManager.batchRemoveTags') }}
              </el-button>
              <el-button
                size="small"
                @click="promptBatchTags('replace')"
              >
                {{ t('documentManager.batchReplaceTags') }}
              </el-button>
              <el-button
                size="small"
                @click="multiSelectedIds.clear()"
              >
                {{ t('documentManager.clearMultiSelect') }}
              </el-button>
            </div>
            <div class="dm-toolbar-actions">
              <el-button
                size="small"
                @click="openTagManager"
              >
                {{ t('documentManager.tagManager') }}
              </el-button>
              <el-button
                size="small"
                @click="rebuildFtsIndex"
              >
                {{ t('documentManager.rebuildFts') }}
              </el-button>
              <el-button
                type="primary"
                @click="openImportDialog"
              >
                <el-icon>
                  <Plus />
                </el-icon>
                {{ t('documentManager.addDocument') }}
              </el-button>
              <el-button @click="openScanDialog">
                <el-icon>
                  <Search />
                </el-icon>
                {{ t('documentManager.scanFolder') }}
              </el-button>
            </div>
          </template>
        </div>

        <template v-if="activeTab === 'files'">
          <div
            v-if="items.length === 0 && !loading"
            class="dm-empty"
          >
            <el-empty :description="t('documentManager.noDocs')" />
          </div>
          <div
            v-else-if="loading"
            class="dm-empty"
          >
            <el-icon
              :size="32"
              class="is-loading"
            >
              <Loading />
            </el-icon>
            <p style="margin-top:12px;color:var(--el-text-color-secondary)">
              {{ t('documentManager.loading') }}
            </p>
          </div>
          <div
            v-else
            ref="fileGridRef"
            class="dm-file-grid"
            @click.self="selectedId = null"
          >
            <div
              v-for="item in items"
              :key="item.id"
              :class="{ selected: selectedId === item.id || multiSelectedIds.has(item.id), 'ctx-anchor': ctxAnchorId === item.id }"
              :data-file-id="item.id"
              class="dm-file-card sortable-file"
              role="button"
              tabindex="0"
              @click="onFileCardClick(item, $event)"
              @keydown.enter.prevent="onFileCardClick(item, $event)"
              @keydown.space.prevent="onFileCardClick(item, $event)"
              @dblclick="openDocument(item)"
              @contextmenu.prevent="showContextMenu($event, item)"
            >
              <span
                v-if="item.storageMode === 'repo'"
                class="dm-mode-badge repo"
              >{{
                t('documentManager.migrate')
              }}</span>
              <span
                v-else
                class="dm-mode-badge index"
              >{{ t('documentManager.index') }}</span>
              <div class="dm-file-icon">
                <img
                  v-if="fileIconCache[item.fileExt?.toLowerCase()]"
                  :src="fileIconCache[item.fileExt?.toLowerCase()]"
                  class="dm-file-icon-img"
                >
                <el-icon
                  v-else
                  :color="getFileColor(item.fileExt)"
                  :size="32"
                >
                  <component :is="getFileIcon(item.fileExt)" />
                </el-icon>
              </div>
              <div class="dm-file-info">
                <div
                  :title="item.title || item.fileName"
                  class="dm-file-name"
                >
                  {{ item.title || item.fileName }}
                </div>
                <div
                  v-if="item.snippet"
                  class="dm-file-snippet"
                >
                  <template
                    v-for="(part, pi) in snippetParts(item.snippet, searchKeyword)"
                    :key="pi"
                  >
                    <span :class="{ 'dm-snippet-hit': part.hit }">{{ part.text }}</span>
                  </template>
                </div>
                <div class="dm-file-meta">
                  <span class="dm-file-ext">{{ item.fileExt.toUpperCase() }}</span>
                  <span>{{ formatFileSize(item.fileSize) }}</span>
                </div>
              </div>
            </div>
          </div>
          <div
            v-if="total > pageLimit"
            class="dm-pagination"
          >
            <el-pagination
              v-model:current-page="currentPage"
              :page-size="pageLimit"
              :total="total"
              layout="prev,pager,next"
              @current-change="loadFiles"
            />
          </div>
          <div
            v-if="selectedId !== null && selectedDoc"
            class="dm-detail-panel"
          >
            <div class="dm-detail-header">
              <span>{{ selectedDoc.title || selectedDoc.fileName }}</span>
              <el-button
                size="small"
                text
                @click="selectedId = null"
              >
                <el-icon>
                  <Close />
                </el-icon>
              </el-button>
            </div>
            <div class="dm-detail-body">
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.fileName') }}</span><span>{{
                  selectedDoc.fileName
                }}</span>
              </div>
              <div class="dm-detail-row">
                <span
                  class="dm-detail-label"
                >{{
                  t('documentManager.size')
                }}</span><span>{{ formatFileSize(selectedDoc.fileSize) }}</span>
              </div>
              <div class="dm-detail-row">
                <span
                  class="dm-detail-label"
                >{{
                  t('documentManager.type')
                }}</span><span>{{ selectedDoc.fileExt.toUpperCase() }}</span>
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.path') }}</span><span
                  :title="selectedDoc.managedPath"
                  class="dm-detail-path"
                >{{
                  selectedDoc.managedPath
                }}</span>
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.mode') }}</span><span>{{
                  selectedDoc.storageMode === 'repo' ? t('documentManager.migrate') : t('documentManager.index')
                }}</span>
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.category') }}</span>
                <el-select
                  v-model="editCategoryId"
                  :placeholder="t('documentManager.uncategorized')"
                  clearable
                  size="small"
                  @change="saveCategory"
                >
                  <el-option
                    v-for="cat in categories"
                    :key="cat.id"
                    :label="cat.name"
                    :value="cat.id"
                  />
                </el-select>
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.tags') }}</span>
                <el-input
                  v-model="editTags"
                  :placeholder="t('documentManager.tagsHint')"
                  size="small"
                  @blur="saveTags"
                  @keyup.enter="saveTags"
                />
              </div>
              <div class="dm-detail-row">
                <span class="dm-detail-label">{{ t('documentManager.notes') }}</span>
                <el-input
                  v-model="editNotes"
                  :placeholder="t('documentManager.notesPlaceholder')"
                  :rows="2"
                  size="small"
                  type="textarea"
                  @blur="saveNotes"
                />
              </div>
            </div>
            <div class="dm-detail-actions">
              <el-button
                size="small"
                @click="openDocument(selectedDoc)"
              >
                {{ t('documentManager.open') }}
              </el-button>
              <el-button
                size="small"
                @click="openFolder(selectedDoc)"
              >
                {{ t('documentManager.locate') }}
              </el-button>
              <el-button
                size="small"
                type="danger"
                @click="confirmDelete(selectedDoc)"
              >
                {{
                  t('common.delete')
                }}
              </el-button>
            </div>
          </div>
        </template>

        <template v-if="activeTab === 'history'">
          <div
            v-if="importHistory.length === 0"
            class="dm-empty"
          >
            <el-empty :description="t('documentManager.noHistory')" />
          </div>
          <div
            v-else
            class="dm-history-page"
          >
            <div
              v-for="h in importHistory"
              :key="h.id"
              class="dm-history-card"
            >
              <div class="dm-history-card-hd">
                <span class="dm-history-badge">{{
                  h.storageMode === 'repo' ? t('documentManager.migrate') : t('documentManager.index')
                }}</span>
                <span class="dm-history-fc">{{ h.fileCount }} {{ t('documentManager.fileCount') }}</span>
                <span class="dm-history-time">{{ formatTime(h.createdAt) }}</span>
              </div>
              <div class="dm-history-card-body">
                <div class="dm-history-row">
                  <span class="dm-history-label">{{
                    t('documentManager.sourceDir')
                  }}</span><span
                    class="dm-history-val"
                  >{{ h.sourceDir }}</span>
                </div>
                <div class="dm-history-row">
                  <span class="dm-history-label">{{
                    t('documentManager.targetDir')
                  }}</span><span class="dm-history-val">{{
                    h.targetDir
                  }}</span>
                </div>
              </div>
              <div class="dm-history-card-ft">
                <el-button
                  size="small"
                  text
                  @click="toggleHistoryFiles(h.id)"
                >
                  {{ h._files ? t('documentManager.collapse') : t('documentManager.expand') }} ({{
                    h.fileCount
                  }}{{ t('documentManager.fileCount') }})
                </el-button>
                <el-popconfirm
                  :title="t('documentManager.undoConfirm')"
                  @confirm="undoImportFn(h.id)"
                >
                  <template #reference>
                    <el-button
                      plain
                      size="small"
                      type="danger"
                    >
                      {{ t('documentManager.undo') }}
                    </el-button>
                  </template>
                </el-popconfirm>
              </div>
              <div
                v-if="h._files"
                class="dm-history-files"
              >
                <div
                  v-for="f in h._files"
                  :key="f.sourcePath"
                  class="dm-history-file-item"
                >
                  <el-icon>
                    <Document />
                  </el-icon>
                  <span>{{ f.fileName }}</span>
                  <el-button
                    size="small"
                    text
                    type="danger"
                    @click="undoImportItemFn(h.id, f.docFileId, h)"
                  >
                    {{ t('documentManager.undo') }}
                  </el-button>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>

    <el-dialog
      v-model="showAddRoot"
      :title="t('documentManager.addRootDir')"
      width="480px"
    >
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.dirAlias')">
          <el-input
            v-model="newRootName"
            :placeholder="t('documentManager.dirAliasPlaceholder')"
          />
        </el-form-item>
        <el-form-item :label="t('documentManager.dirPath')">
          <div style="display:flex;gap:8px;width:100%">
            <el-input
              v-model="newRootPath"
              :placeholder="t('documentManager.dirPathPlaceholder')"
            />
            <el-button @click="browseRootPath">
              {{ t('common.browse') }}
            </el-button>
          </div>
        </el-form-item>
      </el-form>
      <div class="dm-hint-warn">
        <el-icon>
          <Warning />
        </el-icon>
        <span>{{ t('documentManager.dirPathHint') }}</span>
      </div>
      <template #footer>
        <el-button @click="showAddRoot = false">
          {{ t('common.cancel') }}
        </el-button>
        <el-button
          :disabled="!newRootName || !newRootPath"
          type="primary"
          @click="confirmAddRoot"
        >
          {{ t('common.confirm') }}
        </el-button>
      </template>
    </el-dialog>

    <el-dialog
      v-model="showAddCategory"
      :title="t('documentManager.newCategory')"
      width="420px"
    >
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.categoryName')">
          <el-input
            v-model="newCategoryName"
            :placeholder="t('documentManager.categoryNameHint')"
          />
        </el-form-item>
        <el-form-item :label="t('documentManager.icon')">
          <div class="dm-icon-picker">
            <span
              v-for="ic in catIcons"
              :key="ic.value"
              :class="{ active: newCategoryIcon === ic.value }"
              class="dm-icon-option"
              @click="newCategoryIcon = ic.value"
            >
              <el-icon :size="18"><component :is="ic.component" /></el-icon>
            </span>
          </div>
        </el-form-item>
        <el-form-item :label="t('documentManager.color')">
          <div class="dm-color-picker">
            <span
              v-for="c in catColors"
              :key="c"
              :class="{ active: newCategoryColor === c }"
              :style="{ background: c }"
              class="dm-color-option"
              @click="newCategoryColor = c"
            />
          </div>
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showAddCategory = false">
          {{ t('common.cancel') }}
        </el-button>
        <el-button
          :disabled="!newCategoryName.trim()"
          type="primary"
          @click="confirmAddCategory"
        >
          {{
            t('common.confirm')
          }}
        </el-button>
      </template>
    </el-dialog>

    <el-dialog
      v-model="showRenameCatDialog"
      :title="t('documentManager.renameCategory')"
      width="400px"
    >
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.newName')">
          <el-input
            v-model="renameCatName"
            :placeholder="t('documentManager.newNamePlaceholder')"
          />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showRenameCatDialog = false">
          {{ t('common.cancel') }}
        </el-button>
        <el-button
          type="primary"
          @click="confirmRenameCat"
        >
          {{ t('common.confirm') }}
        </el-button>
      </template>
    </el-dialog>

    <el-dialog
      v-model="showImportDialog"
      :close-on-click-modal="!importing"
      :close-on-press-escape="!importing"
      :show-close="!importing"
      :title="t('documentManager.addDoc')"
      width="500px"
    >
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.targetDir')">
          <el-select
            v-model="importRootId"
            :placeholder="t('documentManager.selectRoot')"
            style="width:100%"
          >
            <el-option
              v-for="r in roots"
              :key="r.id"
              :label="r.name"
              :value="r.id"
            />
          </el-select>
        </el-form-item>
        <el-form-item :label="t('documentManager.targetCategory')">
          <el-select
            v-model="importCategoryId"
            :placeholder="t('documentManager.uncategorized')"
            clearable
            style="width:100%"
          >
            <el-option
              v-for="c in importCategories"
              :key="c.id"
              :label="c.name"
              :value="c.id"
            />
          </el-select>
        </el-form-item>
        <el-form-item :label="t('documentManager.importMode')">
          <el-radio-group v-model="importMode">
            <el-radio value="index">
              {{ t('documentManager.modeIndex') }}
            </el-radio>
            <el-radio value="repo">
              {{ t('documentManager.modeMigrate') }}
            </el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item :label="t('documentManager.selectFiles')">
          <el-button @click="browseImportFiles">
            {{ t('documentManager.selectFiles') }}
          </el-button>
          <div
            v-if="importFiles.length > 0"
            class="dm-import-files"
          >
            <el-tag
              v-for="(f,i) in importFiles"
              :key="i"
              closable
              style="margin:2px"
              @close="importFiles.splice(i,1)"
            >
              {{ getFileName(f) }}
            </el-tag>
          </div>
        </el-form-item>
      </el-form>
      <div
        v-if="importRootId != null && importMode === 'repo'"
        class="dm-hint-warn"
      >
        <el-icon>
          <Warning />
        </el-icon>
        <span>{{ t('documentManager.fileMoveHint', {path: getImportTargetPath()}) }}</span>
      </div>
      <template #footer>
        <el-button
          :disabled="importing"
          @click="showImportDialog = false"
        >
          {{ t('common.cancel') }}
        </el-button>
        <el-button
          :disabled="!importRootId || importFiles.length === 0 || importing"
          :loading="importing"
          type="primary"
          @click="confirmImport"
        >
          {{
            importing ? t('documentManager.importing', {count: importFiles.length}) : t('documentManager.confirmImportBtn')
          }}
        </el-button>
      </template>
    </el-dialog>

    <el-dialog
      v-model="showScanDialog"
      :close-on-click-modal="!scanImporting"
      :close-on-press-escape="!scanImporting"
      :show-close="!scanImporting"
      :title="t('documentManager.scanFolder')"
      width="500px"
    >
      <el-form label-width="80px">
        <el-form-item :label="t('documentManager.scanPath')">
          <div style="display:flex;gap:8px;width:100%">
            <el-input
              v-model="scanPath"
              :disabled="scanImporting"
              :placeholder="t('documentManager.scanPathPlaceholder')"
            />
            <el-button
              :disabled="scanImporting"
              @click="browseScanPath"
            >
              {{ t('common.browse') }}
            </el-button>
          </div>
        </el-form-item>
        <el-form-item :label="t('documentManager.targetDir')">
          <el-select
            v-model="scanImportRootId"
            :disabled="scanImporting"
            :placeholder="t('documentManager.selectRoot')"
            style="width:100%"
          >
            <el-option
              v-for="r in roots"
              :key="r.id"
              :label="r.name"
              :value="r.id"
            />
          </el-select>
        </el-form-item>
        <el-form-item :label="t('documentManager.targetCategory')">
          <el-select
            v-model="scanCategoryId"
            :disabled="scanImporting"
            :placeholder="t('documentManager.uncategorized')"
            clearable
            style="width:100%"
          >
            <el-option
              v-for="c in scanCategories"
              :key="c.id"
              :label="c.name"
              :value="c.id"
            />
          </el-select>
        </el-form-item>
        <el-form-item :label="t('documentManager.importMode')">
          <el-radio-group
            v-model="importMode"
            :disabled="scanImporting"
          >
            <el-radio value="index">
              {{ t('documentManager.index') }}
            </el-radio>
            <el-radio value="repo">
              {{ t('documentManager.migrate') }}
            </el-radio>
          </el-radio-group>
        </el-form-item>
      </el-form>
      <div
        v-if="scanning"
        style="text-align:center;padding:20px"
      >
        <el-icon
          :size="32"
          class="is-loading"
        >
          <Loading />
        </el-icon>
        <p>{{ t('documentManager.scanning') }}</p>
      </div>
      <div
        v-else-if="scannedFiles.length > 0"
        class="dm-scan-list"
      >
        <p>{{ t('documentManager.foundFiles', {count: scannedFiles.length}) }}</p>
        <div class="dm-scan-files">
          <div
            v-for="f in scannedFiles"
            :key="f.path"
            :class="{ checked: scanSelected.has(f.path) }"
            class="dm-scan-file-item"
            role="button"
            tabindex="0"
            @click="toggleScanSelect(f.path)"
            @keydown.enter.prevent="toggleScanSelect(f.path)"
            @keydown.space.prevent="toggleScanSelect(f.path)"
          >
            <el-icon v-if="scanSelected.has(f.path)">
              <Select />
            </el-icon>
            <el-icon v-else>
              <Document />
            </el-icon>
            <span>{{ f.name }}</span>
            <span class="dm-scan-size">{{ formatFileSize(f.size) }}</span>
          </div>
        </div>
      </div>
      <template #footer>
        <el-button
          :disabled="scanImporting"
          @click="showScanDialog = false"
        >
          {{ t('common.cancel') }}
        </el-button>
        <el-button
          :disabled="scannedFiles.length === 0 || scanImporting"
          @click="toggleScanSelectAll"
        >
          {{
            scanSelected.size === scannedFiles.length ? t('documentManager.deselectAll') : t('documentManager.selectAll')
          }}
        </el-button>
        <el-button
          :disabled="scanSelected.size === 0 || !scanImportRootId || scanImporting"
          :loading="scanImporting"
          type="primary"
          @click="importScanned"
        >
          {{
            scanImporting ? t('documentManager.importing', {count: scanSelected.size}) : t('documentManager.importSelected', {count: scanSelected.size})
          }}
        </el-button>
      </template>
    </el-dialog>

    <div
      v-if="dragover"
      class="dm-drop-overlay"
    >
      <el-icon :size="48">
        <UploadFilled />
      </el-icon>
      <p>{{ t('documentManager.dropToAdd') }}</p>
    </div>
  </div>

  <ContextMenu
    :show="ctxMenuVisible"
    :x="ctxMenuX"
    :y="ctxMenuY"
    @close="closeCtxMenu"
  >
    <div
      class="context-menu-item"
      @click="startMove(ctxMenuDoc)"
    >
      <el-icon :size="14">
        <Folder />
      </el-icon>
      <span>{{ t('common.move') }}</span>
    </div>
    <div class="context-menu-divider" />
    <div
      class="context-menu-item context-menu-item-danger"
      @click="contextDelete(ctxMenuDoc)"
    >
      <el-icon :size="14">
        <Close />
      </el-icon>
      <span>{{ t('common.delete') }}</span>
    </div>
  </ContextMenu>

  <el-dialog
    v-model="showMoveDialog"
    :title="t('documentManager.moveFile')"
    width="420px"
    @closed="closeCtxMenu"
  >
    <div class="dm-move-body">
      <div class="dm-move-section">
        <div class="dm-move-section-title">
          {{ t('documentManager.moveToRoot') }}
        </div>
        <el-select
          v-model="moveTargetRootId"
          :placeholder="t('documentManager.selectRoot')"
          style="width:100%"
          @change="onMoveRootChange"
        >
          <el-option
            v-for="root in roots"
            :key="root.id"
            :label="root.name"
            :value="root.id"
          />
        </el-select>
        <div
          v-if="moveDoc.storageMode === 'repo'"
          class="dm-move-hint"
        >
          {{ t('documentManager.migrateHint') }}
        </div>
      </div>
      <div class="dm-move-section">
        <div class="dm-move-section-title">
          {{ t('documentManager.moveToCategory') }}
        </div>
        <el-select
          v-model="moveTargetCategoryId"
          :placeholder="t('documentManager.uncategorized')"
          clearable
          style="width:100%"
        >
          <el-option
            v-for="cat in moveCategories"
            :key="cat.id"
            :label="cat.name"
            :value="cat.id"
          />
        </el-select>
      </div>
    </div>
    <template #footer>
      <el-button @click="showMoveDialog = false">
        {{ t('common.cancel') }}
      </el-button>
      <el-button
        :disabled="!hasMoveChange"
        type="primary"
        @click="confirmMove"
      >
        {{ t('common.ok') }}
      </el-button>
    </template>
  </el-dialog>

  <el-dialog
    v-model="showGuide"
    :title="t('documentManager.welcome')"
    width="560px"
  >
    <div class="dm-guide-body">
      <p class="dm-guide-desc">
        {{ t('documentManager.welcomeDesc') }}
      </p>
      <div class="dm-guide-steps">
        <div
          v-for="(step, idx) in guideSteps"
          :key="idx"
          class="dm-guide-step"
        >
          <div class="dm-guide-step-num">
            {{ idx + 1 }}
          </div>
          <div class="dm-guide-step-content">
            <div class="dm-guide-step-title">
              {{ step.title }}
            </div>
            <div class="dm-guide-step-desc">
              {{ step.desc }}
            </div>
          </div>
        </div>
      </div>
      <div class="dm-guide-footer">
        <el-checkbox v-model="guideNoMore">
          {{ t('documentManager.dontShowAgain') }}
        </el-checkbox>
      </div>
    </div>
    <template #footer>
      <el-button
        type="primary"
        @click="dismissGuide"
      >
        {{ t('documentManager.startUsing') }}
      </el-button>
    </template>
  </el-dialog>

  <el-dialog
    v-model="showOrphanDialog"
    :title="t('documentManager.orphanFilesTitle')"
    width="560px"
  >
    <div
      v-if="orphanLoading"
      style="text-align:center;padding:20px"
    >
      <el-icon
        :size="32"
        class="is-loading"
      >
        <Loading />
      </el-icon>
      <p>{{ t('documentManager.scanning') }}</p>
    </div>
    <div
      v-else-if="orphanResults.length === 0"
      style="text-align:center;padding:20px"
    >
      <el-empty :description="t('documentManager.noOrphanFiles')" />
    </div>
    <div
      v-else
      class="dm-orphan-list"
    >
      <div
        v-for="result in orphanResults"
        :key="result.rootId"
        class="dm-orphan-group"
      >
        <div class="dm-orphan-group-title">
          {{ result.rootName }}
        </div>
        <div
          v-for="f in result.files"
          :key="f.path"
          :class="{ checked: orphanSelected.has(f.path) }"
          class="dm-scan-file-item"
          role="button"
          tabindex="0"
          @click="toggleOrphan(f.path)"
          @keydown.enter.prevent="toggleOrphan(f.path)"
          @keydown.space.prevent="toggleOrphan(f.path)"
        >
          <el-icon v-if="orphanSelected.has(f.path)">
            <Select />
          </el-icon>
          <el-icon v-else>
            <Document />
          </el-icon>
          <span>{{ f.name }}</span>
          <span
            v-if="f.categoryName"
            class="dm-orphan-cat"
          >{{ f.categoryName }}</span>
          <span class="dm-scan-size">{{ formatFileSize(f.size) }}</span>
        </div>
      </div>
    </div>
    <template #footer>
      <el-button @click="showOrphanDialog = false">
        {{ t('documentManager.close') }}
      </el-button>
      <el-button
        v-if="orphanResults.length > 0"
        @click="toggleOrphanSelectAll"
      >
        {{
          orphanSelected.size === totalOrphanCount ? t('documentManager.deselectAll') : t('documentManager.selectAll')
        }}
      </el-button>
      <el-button
        :disabled="orphanSelected.size === 0"
        type="primary"
        @click="importOrphans"
      >
        {{ t('documentManager.importSelected', {count: orphanSelected.size}) }}
      </el-button>
    </template>
  </el-dialog>

  <el-dialog
    v-model="tagManagerVisible"
    :title="t('documentManager.tagManager')"
    width="420px"
  >
    <div
      v-if="allTags.length === 0"
      class="dm-empty"
    >
      {{ t('documentManager.tagEmpty') }}
    </div>
    <div
      v-for="tg in allTags"
      :key="tg.tag"
      class="dm-tag-row"
    >
      <span class="dm-tag-name">{{ tg.tag }}</span>
      <span class="dm-tag-count">{{ t('documentManager.tagCount', {n: tg.count}) }}</span>
      <el-button
        size="small"
        @click="renameTag(tg.tag)"
      >
        {{ t('documentManager.tagRename') }}
      </el-button>
      <el-button
        size="small"
        type="danger"
        @click="removeTag(tg.tag)"
      >
        {{ t('documentManager.tagRemove') }}
      </el-button>
    </div>
  </el-dialog>
</template>

<script setup>
import {onBeforeUnmount, onMounted, reactive, ref, watch} from 'vue'
import {ElMessage} from 'element-plus'
import {DocumentService} from '@/services/ipc.js'
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
import {useCategoryTree} from './composables/useCategoryTree'
import {useDocumentEdit} from './composables/useDocumentEdit'
import {useDocumentList} from './composables/useDocumentList'
import {useDocumentOps} from './composables/useDocumentOps'
import {useDocumentSearch} from './composables/useDocumentSearch'
import {useImportManager} from './composables/useImportManager'
import {useOrphanFiles} from './composables/useOrphanFiles'
import {useTagManager} from './composables/useTagManager'

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

// 跨域共用的筛选与视图状态：列表查询、分类侧栏、导入弹窗、未管理文件检测均依赖，保留在页面层
const fileExtFilter = ref(null)
const tagFilter = ref([])
const categoryFilter = ref(null)
const rootFilter = ref(null)
const dragover = ref(false)
const activeTab = ref('files')

const rootListRef = ref(null)
const catListRef = ref(null)
const fileGridRef = ref(null)
let rootSortable = null
let catSortable = null
let fileSortable = null

const {searchKeyword, snippetParts, rebuildFtsIndex} = useDocumentSearch()

const {
  items,
  loading,
  currentPage,
  pageLimit,
  total,
  selectedId,
  selectedDoc,
  multiSelectedIds,
  loadFiles,
  onFileCardClick,
} = useDocumentList({categoryFilter, rootFilter, searchKeyword, fileExtFilter, tagFilter, initFileSortable})

const {
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
} = useCategoryTree({categoryFilter, rootFilter, loadFiles, initRootSortable, initCatSortable})

const {
  allTags,
  tagManagerVisible,
  loadAllTags,
  promptBatchTags,
  openTagManager,
  renameTag,
  removeTag,
} = useTagManager({multiSelectedIds, loadFiles})

const {
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
} = useImportManager({roots, rootFilter, categoryFilter, categories, dragover, loadData, loadFiles})

const {
  orphanCount,
  orphanChecked,
  orphanLoading,
  showOrphanDialog,
  orphanResults,
  orphanSelected,
  totalOrphanCount,
  detectOrphans,
  toggleOrphan,
  toggleOrphanSelectAll,
  importOrphans,
} = useOrphanFiles({rootFilter, loadData, loadFiles})

const {
  editCategoryId,
  editTags,
  editNotes,
  savePendingEdits,
  saveCategory,
  saveTags,
  saveNotes,
} = useDocumentEdit({selectedDoc, loadData, loadFiles})

const {
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
} = useDocumentOps({selectedId, loadFiles, loadData, savePendingEdits})

const fileIconCache = reactive({})

async function loadFileIcons(exts) {
  const unseen = []
  for (const ext of exts) {
    if (fileIconCache[ext] === undefined) {
      unseen.push(ext)
      fileIconCache[ext] = null
    }
  }
  if (unseen.length === 0) return
  try {
    const result = await DocumentService.getFileTypeIcons(unseen)
    for (const ext of unseen) {
      fileIconCache[ext] = result[ext] || null
    }
  } catch {
    for (const ext of unseen) {
      fileIconCache[ext] = null
    }
  }
}

watch(items, (files) => {
  const exts = [...new Set(files.map(f => f.fileExt?.toLowerCase()).filter(Boolean))]
  if (exts.length > 0) loadFileIcons(exts)
}, {immediate: true})

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

function searchFiles() {
  currentPage.value = 1
  loadFiles(false)
  loadAllTags()
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

let searchTimer = null
let unlistenDragDrop = null
watch(searchKeyword, (val, old) => {
  if (val === old) return
  clearTimeout(searchTimer)
  searchTimer = setTimeout(() => {
    currentPage.value = 1
    loadFiles()
  }, 250)
})

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
        try {
          await DocumentService.reorderRoots(ids)
        } catch (e) {
          ElMessage.error(t('documentManager.reorderFailed', {error: e}))
          roots.value = [...roots.value]
        }
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
        try {
          await DocumentService.reorderCategories(ids)
        } catch (e) {
          ElMessage.error(t('documentManager.reorderFailed', {error: e}))
          categories.value = [...categories.value]
        }
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
        try {
          await DocumentService.reorderFiles(ids)
        } catch (e) {
          ElMessage.error(t('documentManager.reorderFailed', {error: e}))
          items.value = [...items.value]
        }
      }
    }
  })
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

onMounted(async () => {
  await loadData();
  if (roots.value.length > 0) {
    rootFilter.value = roots.value[0].id;
  } else {
    await loadFiles();
  }
  loadImportHistory();
  detectOrphans()
  loadAllTags()
  // Tauri v2 的 HTML5 File 无 path 属性，改用原生拖拽事件获取文件路径
  try {
    const {getCurrentWebview} = await import('@tauri-apps/api/webview')
    unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
      const {type, paths} = event.payload
      if (type === 'drop' && paths && paths.length > 0) {
        handleDroppedPaths(paths)
      }
    })
  } catch (e) {
    console.error('注册拖拽事件失败:', e)
  }
})

onBeforeUnmount(() => {
  if (rootSortable) { rootSortable.destroy(); rootSortable = null }
  if (catSortable) { catSortable.destroy(); catSortable = null }
  if (fileSortable) { fileSortable.destroy(); fileSortable = null }
  if (searchTimer) { clearTimeout(searchTimer); searchTimer = null }
  if (unlistenDragDrop) unlistenDragDrop()
})
</script>

<style src="./document_manager.scoped.css" scoped></style>
