//! 剪贴板历史数据库的 schema 定义与迁移
//!
//! 从 `database.rs` 拆出：建表 DDL、表结构迁移（列增删 / 重建去重）、
//! FTS 虚拟表可用性探测。只包含 schema 生命周期逻辑，不涉及任何 CRUD。

use crate::core::error_codes::AppErrorKind;
use sqlx::{Row, SqliteConnection};

pub(crate) async fn ensure_history_db_schema_async(
    conn: &mut SqliteConnection,
) -> Result<(), String> {
    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS history_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            item_id TEXT UNIQUE,
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS categories (
            category TEXT NOT NULL,
            item_id TEXT PRIMARY KEY
        );
        CREATE TABLE IF NOT EXISTS category_list (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            category TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS pinned_items (
            pinned_at INTEGER NOT NULL DEFAULT 0,
            item_id TEXT PRIMARY KEY
        );
        CREATE INDEX IF NOT EXISTS idx_history_items_created_at ON history_items(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_history_items_item_id ON history_items(item_id);
        CREATE INDEX IF NOT EXISTS idx_categories_category ON categories(category);
                CREATE INDEX IF NOT EXISTS idx_pinned_items_pinned_at ON pinned_items(pinned_at DESC);
                ",
    )
    .execute(&mut *conn)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    let _ = sqlx::query("DROP INDEX IF EXISTS idx_history_items_content_hash")
        .execute(&mut *conn)
        .await;
    let _ = sqlx::query("ALTER TABLE history_items DROP COLUMN content_hash")
        .execute(&mut *conn)
        .await;

    // Deduplicate history_items by item_id before enforcing UNIQUE constraint.
    // Keep the row with the latest updated_at for each item_id.
    let _ = sqlx::query(
        "DELETE FROM history_items WHERE id NOT IN (
            SELECT id FROM (
                SELECT id, ROW_NUMBER() OVER (PARTITION BY item_id ORDER BY updated_at DESC) AS rn
                FROM history_items WHERE item_id IS NOT NULL
            ) WHERE rn = 1
        ) AND item_id IS NOT NULL",
    )
    .execute(&mut *conn)
    .await;

    // Also deduplicate category_list by category name
    let _ = sqlx::query(
        "DELETE FROM category_list WHERE id NOT IN (
            SELECT id FROM (
                SELECT id, ROW_NUMBER() OVER (PARTITION BY category ORDER BY id ASC) AS rn
                FROM category_list
            ) WHERE rn = 1
        )",
    )
    .execute(&mut *conn)
    .await;

    // Enforce UNIQUE constraint on item_id (idempotent for new installations)
    let _ = sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_history_items_item_id_unique ON history_items(item_id)")
        .execute(&mut *conn)
        .await;
    // Enforce UNIQUE constraint on category_list.category
    let _ = sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_category_list_category_unique ON category_list(category)")
        .execute(&mut *conn)
        .await;

    sqlx::query(
        "UPDATE history_items
         SET created_at = CAST(strftime('%s','now') AS INTEGER) * 1000
         WHERE created_at <= 0",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query(
        "UPDATE history_items
         SET updated_at = created_at
         WHERE updated_at <= 0",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    let categories_info: Vec<sqlx::sqlite::SqliteRow> =
        sqlx::query("PRAGMA table_info(categories)")
            .fetch_all(&mut *conn)
            .await
            .unwrap_or_default();
    let mut item_id_is_pk_categories = false;
    for r in categories_info {
        let name: String = r.try_get("name").unwrap_or_default();
        let pk: i32 = r.try_get("pk").unwrap_or(0);
        if name == "item_id" && pk > 0 {
            item_id_is_pk_categories = true;
        }
    }

    if !item_id_is_pk_categories {
        let result = sqlx::query(
            "
            CREATE TABLE categories_new (
                category TEXT NOT NULL,
                item_id TEXT PRIMARY KEY
            );
            INSERT OR IGNORE INTO categories_new(category, item_id)
            SELECT category, item_id FROM categories WHERE item_id IS NOT NULL AND item_id != '';
            DROP TABLE categories;
            ALTER TABLE categories_new RENAME TO categories;
            ",
        )
        .execute(&mut *conn)
        .await;
        if let Err(e) = result {
            log::error!("categories 表迁移失败，数据可能不完整: {}", e);
        }
    }

    let pinned_info: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("PRAGMA table_info(pinned_items)")
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_default();
    let mut item_id_is_pk_pinned = false;
    for r in pinned_info {
        let name: String = r.try_get("name").unwrap_or_default();
        let pk: i32 = r.try_get("pk").unwrap_or(0);
        if name == "item_id" && pk > 0 {
            item_id_is_pk_pinned = true;
        }
    }

    if !item_id_is_pk_pinned {
        let result = sqlx::query(
            "
            CREATE TABLE pinned_items_new (
                pinned_at INTEGER NOT NULL DEFAULT 0,
                item_id TEXT PRIMARY KEY
            );
            INSERT OR IGNORE INTO pinned_items_new(pinned_at, item_id)
            SELECT pinned_at, item_id FROM pinned_items WHERE item_id IS NOT NULL AND item_id != '';
            DROP TABLE pinned_items;
            ALTER TABLE pinned_items_new RENAME TO pinned_items;
            ",
        )
        .execute(&mut *conn)
        .await;
        if let Err(e) = result {
            log::error!("pinned_items 表迁移失败，数据可能不完整: {}", e);
        }
    }

    let _ = sqlx::query(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS history_items_fts USING fts5(
            item_id UNINDEXED,
            content,
            tokenize = 'unicode61'
        );
        ",
    )
    .execute(&mut *conn)
    .await;

    // P2 性能优化：先清理 FTS 孤儿记录（廉价操作），仅在必要时才重建索引
    let fts_row_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM history_items_fts")
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0);

    let history_row_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM history_items")
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0);

    // 无条件清理孤儿记录（DELETE 子查询非常廉价）
    let _ = sqlx::query(
        "DELETE FROM history_items_fts WHERE rowid NOT IN (SELECT id FROM history_items)",
    )
    .execute(&mut *conn)
    .await;

    // 仅当行数不一致时才执行全量重建
    if fts_row_count != history_row_count {
        log::info!(
            "FTS 索引需要同步 (fts={}, history={})，执行全量重建",
            fts_row_count,
            history_row_count
        );
        let insert_result = sqlx::query(
            "
            INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
            SELECT id, COALESCE(item_id, ''), content
            FROM history_items
            ",
        )
        .execute(&mut *conn)
        .await;
        if let Err(e) = insert_result {
            log::error!("FTS 索引重建（INSERT）失败: {}", e);
        }

        let delete_result = sqlx::query(
            "
            DELETE FROM history_items_fts
            WHERE rowid NOT IN (SELECT id FROM history_items)
            ",
        )
        .execute(&mut *conn)
        .await;
        if let Err(e) = delete_result {
            log::error!("FTS 索引重建（清理孤儿行）失败: {}", e);
        }
    } else {
        log::debug!("FTS 索引已同步 ({} 行)，跳过重建", fts_row_count);
    }

    let _ = sqlx::query("ALTER TABLE pinned_items ADD COLUMN position INTEGER NOT NULL DEFAULT 0")
        .execute(&mut *conn)
        .await;

    // 添加 position 字段用于排序
    let _ = sqlx::query("ALTER TABLE history_items ADD COLUMN position INTEGER NOT NULL DEFAULT 0")
        .execute(&mut *conn)
        .await;

    Ok(())
}

pub(crate) async fn history_fts_enabled_conn_async(
    conn: &mut SqliteConnection,
) -> Result<bool, String> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'history_items_fts'",
    )
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let count = row.try_get::<i64, _>("count").unwrap_or(0);
    Ok(count > 0)
}
