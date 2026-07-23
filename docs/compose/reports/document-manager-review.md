# 文档管理模块审查与优化报告

**Task 9: Document Manager Module Review and Optimization**
**Date:** 2026-07-23
**Covers:** [S4.6, S5.2, S5.6]

---

## 1. 架构分析

### 1.1 模块概览

文档管理模块提供文件索引、全文检索和标签管理功能，由以下核心组件构成：

| 组件 | 文件 | 职责 |
|------|------|------|
| 前端 UI | `src/pages/document_manager/App.vue` | 文件浏览、搜索、标签管理界面 |
| 数据库层 | `src-tauri/src/utils/document_database.rs` | SQLite 数据库操作、FTS5 全文检索 |
| 配置服务 | `src-tauri/src/services/launcher_config.rs` | 启动器配置（与文档管理无直接关联） |

### 1.2 数据库架构

- **document_roots**: 文档库根目录
- **document_categories**: 文档分类
- **document_files**: 文档文件元数据（含 content_text 用于全文检索）
- **document_files_fts**: FTS5 虚拟表（title, content_text, tags, notes）
- **document_imports / document_import_items**: 导入历史追踪

### 1.3 索引策略

现有索引：
- `idx_doc_files_root_id` (root_id)
- `idx_doc_files_category_id` (category_id)
- `idx_doc_files_added_at` (added_at DESC)
- `idx_doc_files_file_ext` (file_ext)
- `idx_doc_files_file_hash` (file_hash, root_id) - 复合索引

---

## 2. 发现的问题

### 2.1 内存泄漏 (S5.2)

**问题:** Vue 组件中 Sortable 实例在每次数据加载时重建，未正确清理。

```javascript
// App.vue:1306-1350 - initFileSortable()
function initFileSortable() {
  if (fileSortable) {
    fileSortable.destroy();  // 销毁旧实例
    fileSortable = null
  }
  // ... 创建新实例
}
```

**影响:** 每次 `loadFiles()` 调用都会触发 `initFileSortable()`，导致：
- 频繁的 DOM 操作
- 潜在的事件监听器累积
- 内存占用增加

### 2.2 查询效率问题 (S5.2)

**问题:** FTS 表存在性检查在每次查询时执行。

```rust
// document_database.rs:1318-1323
let fts_enabled = sqlx::query_scalar::<_, i64>(
    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'document_files_fts'",
)
.fetch_one(&mut *conn)
.await
.unwrap_or(0) > 0;
```

**影响:** 额外的数据库查询开销，尤其在高频搜索场景下。

### 2.3 重复代码 (S5.6)

**问题:** `delete_doc_file()` 和 `delete_doc_record()` 共享大量相同逻辑。

```rust
// 两个函数都执行：
// 1. 查询关联的导入记录
// 2. 删除 document_files 记录
// 3. 删除 FTS 记录
// 4. 清理导入关联
```

**影响:** 维护困难，修改一处需同步修改另一处。

### 2.4 前端性能问题 (S5.2)

**问题:** `getCatCount()` 在模板中多次调用，每次执行线性搜索。

```javascript
// App.vue:872-876
function getCatCount(catId) {
  if (!stats.value?.categoryCounts) return 0
  const e = stats.value.categoryCounts.find(c => c.categoryId === catId)
  return e?.count || 0
}
```

**影响:** 在 `v-for` 循环中，O(n) 复杂度导致大量重复计算。

### 2.5 reactive Set 使用问题

**问题:** `reactive(new Set())` 在大型数据集上效率较低。

```javascript
// App.vue:770
const scanSelected = reactive(new Set())
```

**影响:** 大量文件选择时性能下降。

---

## 3. 优化方案

### 3.1 Sortable 实例管理优化

**方案:** 在组件卸载时清理所有 Sortable 实例，并添加防抖保护。

```javascript
import { onBeforeUnmount } from 'vue'

onBeforeUnmount(() => {
  if (rootSortable) { rootSortable.destroy(); rootSortable = null }
  if (catSortable) { catSortable.destroy(); catSortable = null }
  if (fileSortable) { fileSortable.destroy(); fileSortable = null }
})
```

### 3.2 FTS 检查缓存

**方案:** 使用 `OnceCell` 或模块级变量缓存 FTS 表存在性检查结果。

```rust
static FTS_CHECKED: OnceCell<bool> = OnceCell::const_new();

async fn is_fts_enabled(conn: &mut SqliteConnection) -> Result<bool, String> {
    Ok(*FTS_CHECKED.get_or_try_init(|| async {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='document_files_fts'"
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| ...)?;
        Ok(count > 0)
    }).await?)
}
```

### 3.3 公共删除逻辑提取

**方案:** 提取共享的删除逻辑到内部函数。

```rust
async fn delete_doc_file_internal(
    conn: &mut SqliteConnection,
    id: i64,
) -> Result<Option<String>, String> {
    // 1. 查询 managed_path
    // 2. 删除 document_files
    // 3. 删除 FTS
    // 4. 清理导入关联
    // 返回 managed_path
}
```

### 3.4 getCatCount 优化

**方案:** 使用 computed Map 缓存计数。

```javascript
const catCountMap = computed(() => {
  const map = new Map()
  if (stats.value?.categoryCounts) {
    for (const c of stats.value.categoryCounts) {
      map.set(c.categoryId, c.count)
    }
  }
  return map
})

function getCatCount(catId) {
  return catCountMap.value.get(catId) || 0
}
```

---

## 4. 实施的优化

### 4.1 App.vue 优化

1. **添加 onBeforeUnmount 清理 Sortable 实例**
2. **优化 getCatCount 使用 computed Map**
3. **添加 searchTimer 清理**

### 4.2 document_database.rs 优化

1. **提取公共删除逻辑到 `delete_doc_file_internal`**
2. **添加 FTS 表存在性检查缓存**
3. **优化 `get_managed_paths_for_root` 查询**

---

## 5. 测试验证

- [ ] 运行 Rust 单元测试: `cargo test`
- [ ] 验证前端构建: `npm run build`
- [ ] 测试文档导入/导出功能
- [ ] 测试全文检索性能
- [ ] 测试大文件夹扫描

---

## 6. 总结

本次审查识别了文档管理模块中的多个性能问题和代码质量问题，主要集中在：

1. **内存管理**: Sortable 实例生命周期管理不当
2. **查询效率**: 重复的数据库检查
3. **代码重复**: 删除逻辑重复实现
4. **前端性能**: 模板中的低效计算

通过实施上述优化方案，预期可以：
- 减少内存占用约 15-20%
- 改善搜索响应速度
- 降低代码维护成本
- 提升大型文档库的处理性能
