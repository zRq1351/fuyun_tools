//! 文档库数据库的 schema 定义与迁移
//!
//! 从 `document_database.rs` 拆出：建表 DDL、索引、FTS 虚拟表、分类目录初始化、
//! FTS 可用性探测。只包含 schema 生命周期逻辑，不涉及任何 CRUD。

use crate::core::error_codes::AppErrorKind;
use sqlx::{Row, SqliteConnection};
use std::fs;
use std::path::Path;
use tokio::sync::OnceCell;

static FTS_ENABLED: OnceCell<bool> = OnceCell::const_new();

pub(crate) async fn is_fts_enabled(conn: &mut SqliteConnection) -> Result<bool, String> {
    let val = FTS_ENABLED.get_or_try_init(|| async {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'document_files_fts'",
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        Ok::<bool, String>(count > 0)
    }).await?;
    Ok(*val)
}

pub(crate) async fn ensure_docs_db_schema(conn: &mut SqliteConnection) -> Result<(), String> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS document_roots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            root_path TEXT NOT NULL UNIQUE,
            position INTEGER DEFAULT 0,
            created_at INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS document_categories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            icon TEXT DEFAULT 'folder',
            color TEXT DEFAULT '#409EFF',
            position INTEGER DEFAULT 0,
            root_id INTEGER DEFAULT 0,
            UNIQUE(name, root_id)
        )",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS document_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            root_id INTEGER NOT NULL,
            title TEXT DEFAULT '',
            file_name TEXT NOT NULL,
            file_ext TEXT NOT NULL DEFAULT '',
            file_size INTEGER NOT NULL DEFAULT 0,
            file_hash TEXT DEFAULT '',
            category_id INTEGER,
            tags TEXT DEFAULT '[]',
            notes TEXT DEFAULT '',
            content_text TEXT DEFAULT '',
            source_path TEXT DEFAULT '',
            managed_path TEXT NOT NULL DEFAULT '',
            storage_mode TEXT NOT NULL DEFAULT 'index',
            is_missing INTEGER DEFAULT 0,
            sort_order INTEGER DEFAULT 0,
            added_at INTEGER NOT NULL DEFAULT 0,
            file_modified INTEGER NOT NULL DEFAULT 0,
            visit_count INTEGER DEFAULT 0,
            FOREIGN KEY (root_id) REFERENCES document_roots(id) ON DELETE CASCADE
        )",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_doc_files_root_id ON document_files(root_id)")
        .execute(&mut *conn)
        .await
        .ok();
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_doc_files_category_id ON document_files(category_id)",
    )
    .execute(&mut *conn)
    .await
    .ok();
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_doc_files_added_at ON document_files(added_at DESC)",
    )
    .execute(&mut *conn)
    .await
    .ok();
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_doc_files_file_ext ON document_files(file_ext)")
        .execute(&mut *conn)
        .await
        .ok();
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_doc_files_file_hash ON document_files(file_hash, root_id)",
    )
    .execute(&mut *conn)
    .await
    .ok();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS document_imports (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            root_id INTEGER NOT NULL,
            category_id INTEGER,
            storage_mode TEXT NOT NULL DEFAULT 'index',
            source_dir TEXT DEFAULT '',
            target_dir TEXT DEFAULT '',
            file_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS document_import_items (
            import_id INTEGER NOT NULL,
            doc_file_id INTEGER NOT NULL,
            source_path TEXT DEFAULT '',
            managed_path TEXT DEFAULT '',
            PRIMARY KEY (import_id, doc_file_id),
            FOREIGN KEY (import_id) REFERENCES document_imports(id) ON DELETE CASCADE
        )",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_import_items_import_id ON document_import_items(import_id)",
    )
    .execute(&mut *conn)
    .await
    .ok();

    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS document_files_fts USING fts5(
            title,
            content_text,
            tags,
            notes,
            tokenize = 'unicode61'
        )",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    ensure_category_directories(conn).await?;

    Ok(())
}

pub(crate) async fn ensure_category_directories(conn: &mut SqliteConnection) -> Result<(), String> {
    let roots = sqlx::query("SELECT id, root_path FROM document_roots")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    if roots.is_empty() {
        return Ok(());
    }

    let cats = sqlx::query("SELECT root_id, name FROM document_categories ORDER BY position")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    for row in &roots {
        let rid: i64 = row.try_get(0).unwrap_or(0);
        let root_path: String = row.try_get(1).unwrap_or_default();
        for crow in &cats {
            let cat_root_id: i64 = crow.try_get(0).unwrap_or(0);
            let cat_name: String = crow.try_get(1).unwrap_or_default();
            if cat_root_id == 0 || cat_root_id == rid {
                let _ = fs::create_dir_all(Path::new(&root_path).join(&cat_name));
            }
        }
    }

    Ok(())
}
