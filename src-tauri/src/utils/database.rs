use crate::core::error_codes::AppErrorKind;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{Acquire, QueryBuilder, Row, Sqlite, SqliteConnection, Transaction};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use tokio::sync::OnceCell;
use xxhash_rust::xxh3::xxh3_64;

static HISTORY_DB_POOL: OnceCell<SqlitePool> = OnceCell::const_new();

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ClipboardHistoryData {
    pub items: Vec<String>,
    #[serde(default)]
    pub categories: HashMap<String, String>,
    #[serde(default)]
    pub category_list: Vec<String>,
    #[serde(default)]
    pub pinned_items: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryPageItem {
    pub position: usize,
    pub id: String,
    pub content: String,
    pub category: String,
    pub pinned: bool,
    pub updated_at: i64,
    pub snippet: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryPageData {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<ClipboardHistoryPageItem>,
}

pub fn get_history_db_path() -> PathBuf {
    let mut history_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    history_dir.pop();
    history_dir.push("history.db");
    history_dir
}

/// 统一使用 utils_helpers 中的时间函数，避免重复定义
fn now_unix_ms() -> i64 {
    crate::utils::utils_helpers::now_unix_ms_i64()
}

pub fn stable_history_item_id(content: &str) -> String {
    format!("{:016x}", xxh3_64(content.as_bytes()))
}

use super::db_utils::{reset_temp_text_table, fill_temp_text_table, build_fts_query_and, build_keyword_snippet_default as build_keyword_snippet, create_db_options};

async fn bulk_upsert_history_items(
    tx: &mut Transaction<'_, Sqlite>,
    entries: &[(String, String, i64)],
) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }
    sqlx::query("CREATE TEMP TABLE IF NOT EXISTS temp_upsert_history (item_id TEXT PRIMARY KEY, content TEXT, ts INTEGER)")
        .execute(&mut **tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM temp_upsert_history")
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;

    for chunk in entries.chunks(300) {
        let mut qb: QueryBuilder<Sqlite> =
            QueryBuilder::new("INSERT INTO temp_upsert_history (item_id, content, ts) ");
        qb.push_values(chunk, |mut b, (id, c, ts)| {
            b.push_bind(id).push_bind(c).push_bind(*ts);
        });
        qb.build()
            .execute(&mut **tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    sqlx::query("
        UPDATE history_items 
        SET content = (SELECT content FROM temp_upsert_history t WHERE t.item_id = history_items.item_id),
            created_at = (SELECT ts FROM temp_upsert_history t WHERE t.item_id = history_items.item_id),
            updated_at = (SELECT ts FROM temp_upsert_history t WHERE t.item_id = history_items.item_id)
        WHERE item_id IN (SELECT item_id FROM temp_upsert_history)
    ").execute(&mut **tx).await.map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query(
        "
        INSERT INTO history_items (item_id, content, created_at, updated_at)
        SELECT item_id, content, ts, ts FROM temp_upsert_history t
        WHERE NOT EXISTS (SELECT 1 FROM history_items h WHERE h.item_id = t.item_id)
    ",
    )
    .execute(&mut **tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

async fn bulk_upsert_categories(
    tx: &mut Transaction<'_, Sqlite>,
    entries: &[(String, String)],
) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }
    sqlx::query("CREATE TEMP TABLE IF NOT EXISTS temp_upsert_categories (item_id TEXT PRIMARY KEY, category TEXT)")
        .execute(&mut **tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM temp_upsert_categories")
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;

    for chunk in entries.chunks(300) {
        let mut qb: QueryBuilder<Sqlite> =
            QueryBuilder::new("INSERT INTO temp_upsert_categories (item_id, category) ");
        qb.push_values(chunk, |mut b, (item_id, category)| {
            b.push_bind(item_id).push_bind(category);
        });
        qb.build()
            .execute(&mut **tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    sqlx::query("
        INSERT INTO categories(category, item_id)
        SELECT category, item_id FROM temp_upsert_categories
        ON CONFLICT(item_id) DO UPDATE SET category = excluded.category
    ").execute(&mut **tx).await.map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

async fn bulk_upsert_category_list(
    tx: &mut Transaction<'_, Sqlite>,
    categories: &[String],
) -> Result<(), String> {
    if categories.is_empty() {
        return Ok(());
    }
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("INSERT INTO category_list(category) ");
    qb.push_values(categories, |mut b, category| {
        b.push_bind(category);
    });
    qb.build()
        .execute(&mut **tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

async fn bulk_upsert_pinned_items(
    tx: &mut Transaction<'_, Sqlite>,
    entries: &[(String, i64)],
) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }
    sqlx::query("CREATE TEMP TABLE IF NOT EXISTS temp_upsert_pinned (item_id TEXT PRIMARY KEY, pinned_at INTEGER)")
        .execute(&mut **tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM temp_upsert_pinned")
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;

    for chunk in entries.chunks(300) {
        let mut qb: QueryBuilder<Sqlite> =
            QueryBuilder::new("INSERT INTO temp_upsert_pinned (item_id, pinned_at) ");
        qb.push_values(chunk, |mut b, (item_id, pinned_at)| {
            b.push_bind(item_id).push_bind(*pinned_at);
        });
        qb.build()
            .execute(&mut **tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    sqlx::query("
        INSERT INTO pinned_items(pinned_at, item_id)
        SELECT pinned_at, item_id FROM temp_upsert_pinned
        ON CONFLICT(item_id) DO UPDATE SET pinned_at = excluded.pinned_at
    ").execute(&mut **tx).await.map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

async fn get_history_db_pool() -> Result<&'static SqlitePool, String> {
    HISTORY_DB_POOL
        .get_or_try_init(|| async {
            let db_path = get_history_db_path();
            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
            }
            let pool = SqlitePoolOptions::new()
                .max_connections(3)
                .connect_with(create_db_options(&db_path))
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

            let mut conn = pool
                .acquire()
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
            ensure_history_db_schema_async(&mut conn).await?;

            Ok(pool)
        })
        .await
}

async fn open_history_db_async() -> Result<sqlx::pool::PoolConnection<sqlx::Sqlite>, String> {
    let pool = get_history_db_pool().await?;
    pool.acquire()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
}

async fn ensure_history_db_schema_async(conn: &mut SqliteConnection) -> Result<(), String> {
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

    // Enforce UNIQUE constraint on item_id (idempotent for new installs)
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

    // P2 性能优化：仅在 FTS 表数据不同步时才重建索引
    // 避免每次启动时对大量历史记录执行全量 INSERT OR REPLACE
    let fts_row_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM history_items_fts"
    )
    .fetch_one(&mut *conn)
    .await
    .unwrap_or(0);

    let history_row_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM history_items"
    )
    .fetch_one(&mut *conn)
    .await
    .unwrap_or(0);

    // 仅当行数不一致时才执行全量重建（跳过孤儿记录清理的额外查询）
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

    Ok(())
}

async fn history_fts_enabled_conn_async(conn: &mut SqliteConnection) -> Result<bool, String> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'history_items_fts'",
    )
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let count = row.try_get::<i64, _>("count").unwrap_or(0);
    Ok(count > 0)
}

async fn load_history_data_from_sqlite_async() -> Result<Option<ClipboardHistoryData>, String> {
    let db_path = get_history_db_path();
    if !db_path.exists() {
        return Ok(None);
    }
    let mut conn = open_history_db_async().await?;

    let history_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM history_items")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let categories_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM categories")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let category_list_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM category_list")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let pinned_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pinned_items")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    if history_count + categories_count + category_list_count + pinned_count == 0 {
        return Ok(None);
    }

    let item_rows = sqlx::query(
        "SELECT content FROM history_items ORDER BY updated_at DESC, id DESC LIMIT 100000",
    )
    .fetch_all(&mut *conn)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let items = item_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();

    let category_rows = sqlx::query(
        "SELECT item_id, category FROM categories WHERE item_id IS NOT NULL AND item_id != ''",
    )
    .fetch_all(&mut *conn)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let mut categories = HashMap::new();
    for row in category_rows {
        let item_id: String = row
            .try_get(0)
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        let category: String = row
            .try_get(1)
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        categories.insert(item_id, category);
    }

    let category_rows = sqlx::query("SELECT category FROM category_list ORDER BY id ASC")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let category_list = category_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();

    let pinned_rows = sqlx::query("SELECT item_id FROM pinned_items WHERE item_id IS NOT NULL AND item_id != '' ORDER BY pinned_at DESC")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let pinned_items = pinned_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();

    Ok(Some(ClipboardHistoryData {
        items,
        categories,
        category_list,
        pinned_items,
    }))
}

fn resolve_history_sort(sort_by: Option<String>, sort_order: Option<String>) -> &'static str {
    let by = sort_by
        .unwrap_or_else(|| "updatedAt".to_string())
        .to_lowercase();
    let order = sort_order
        .unwrap_or_else(|| "desc".to_string())
        .to_lowercase();
    match (by.as_str(), order.as_str()) {
        ("pinnedfirst", "asc") | ("pinned_first", "asc") =>
            "CASE WHEN p.item_id IS NULL THEN 1 ELSE 0 END ASC, p.pinned_at ASC, hi.updated_at ASC, hi.id ASC",
        ("pinnedfirst", _) | ("pinned_first", _) =>
            "CASE WHEN p.item_id IS NULL THEN 1 ELSE 0 END ASC, p.pinned_at DESC, hi.updated_at DESC, hi.id DESC",
        ("updatedat", "asc") | ("updated_at", "asc") => "hi.updated_at ASC, hi.id ASC",
        ("updatedat", _) | ("updated_at", _) => "hi.updated_at DESC, hi.id DESC",
        ("createdat", "asc") | ("created_at", "asc") => "hi.created_at ASC, hi.id ASC",
        ("createdat", _) | ("created_at", _) => "hi.created_at DESC, hi.id DESC",
        ("id", "asc") => "hi.id ASC",
        ("id", _) => "hi.id DESC",
        _ if order == "asc" => "hi.updated_at ASC, hi.id ASC",
        _ => "hi.updated_at DESC, hi.id DESC",
    }
}

fn block_on_result<T>(future: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err("block_on_result must not be called from within a tokio runtime; use the async variant instead".into());
    }
    tauri::async_runtime::block_on(future)
}

pub fn load_history_data() -> Result<ClipboardHistoryData, String> {
    if let Some(sqlite_data) = block_on_result(load_history_data_from_sqlite_async())? {
        return Ok(sqlite_data);
    }
    Ok(ClipboardHistoryData::default())
}

pub async fn load_history_data_async() -> Result<ClipboardHistoryData, String> {
    if let Some(sqlite_data) = load_history_data_from_sqlite_async().await? {
        return Ok(sqlite_data);
    }
    Ok(ClipboardHistoryData::default())
}

pub async fn load_history_page_data_async(
    offset: usize,
    limit: usize,
    category: Option<String>,
    pinned_only: bool,
    keyword: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
) -> Result<ClipboardHistoryPageData, String> {
    let clamp_i64_to_usize =
        |value: i64| -> usize { usize::try_from(value.max(0)).unwrap_or(usize::MAX) };
    let db_path = get_history_db_path();
    if !db_path.exists() {
        return Ok(ClipboardHistoryPageData {
            total: 0,
            offset,
            limit: limit.clamp(1, 200),
            items: Vec::new(),
        });
    }

    let mut conn = open_history_db_async().await?;
    let effective_limit = limit.clamp(1, 200);
    let category_filter = category
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && v != "全部");
    let keyword_filter = keyword
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let fts_keyword = keyword_filter.as_ref().map(|v| build_fts_query_and(v));
    let pinned_flag: i64 = if pinned_only { 1 } else { 0 };
    let offset_i64 = offset as i64;
    let limit_i64 = effective_limit as i64;
    let fts_enabled = history_fts_enabled_conn_async(&mut conn).await?;
    let order_clause = resolve_history_sort(sort_by, sort_order);

    if !fts_enabled && keyword_filter.is_some() {
        log::warn!("文本分页检索降级到 LIKE 回退（FTS 不可用）");
    }

    let (total, mut items) = if fts_enabled {
        let count_query_sql = "
            SELECT COUNT(*)
            FROM history_items hi
            LEFT JOIN categories c ON c.item_id = hi.item_id
            LEFT JOIN pinned_items p ON p.item_id = hi.item_id
            WHERE
              (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
              AND (?2 = 0 OR p.item_id IS NOT NULL)
              AND (
                ?3 IS NULL
                OR EXISTS (
                    SELECT 1 FROM history_items_fts
                    WHERE history_items_fts.rowid = hi.id
                      AND history_items_fts MATCH ?3
                )
              )
        ";
        let total = sqlx::query_scalar::<_, i64>(count_query_sql)
            .bind(category_filter.as_deref())
            .bind(pinned_flag)
            .bind(fts_keyword.as_deref())
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

        let data_query_sql = format!(
            "
            SELECT
              hi.id,
              COALESCE(hi.item_id, ''),
              hi.content,
              CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END,
              CASE WHEN p.item_id IS NULL THEN 0 ELSE 1 END,
              COALESCE(hi.updated_at, 0)
            FROM history_items hi
            LEFT JOIN categories c ON c.item_id = hi.item_id
            LEFT JOIN pinned_items p ON p.item_id = hi.item_id
            WHERE
              (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
              AND (?2 = 0 OR p.item_id IS NOT NULL)
              AND (
                ?3 IS NULL
                OR EXISTS (
                    SELECT 1 FROM history_items_fts
                    WHERE history_items_fts.rowid = hi.id
                      AND history_items_fts MATCH ?3
                )
              )
            ORDER BY {}
            LIMIT ?4 OFFSET ?5
            ",
            order_clause
        );
        let rows = sqlx::query(&data_query_sql)
            .bind(category_filter.as_deref())
            .bind(pinned_flag)
            .bind(fts_keyword.as_deref())
            .bind(limit_i64)
            .bind(offset_i64)
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        let items = rows
            .into_iter()
            .map(|row| {
                let content: String = row.try_get(2).unwrap_or_default();
                let mut id: String = row.try_get(1).unwrap_or_default();
                if id.is_empty() {
                    id = stable_history_item_id(&content);
                }
                ClipboardHistoryPageItem {
                    position: clamp_i64_to_usize(row.try_get::<i64, _>(0).unwrap_or(0)),
                    id,
                    content,
                    category: row
                        .try_get::<String, _>(3)
                        .unwrap_or_else(|_| "未分类".to_string()),
                    pinned: row.try_get::<i64, _>(4).unwrap_or(0) == 1,
                    updated_at: row.try_get::<i64, _>(5).unwrap_or(0),
                    snippet: None,
                }
            })
            .collect::<Vec<_>>();
        (total, items)
    } else {
        let count_query_sql = "
            SELECT COUNT(*)
            FROM history_items hi
            LEFT JOIN categories c ON c.item_id = hi.item_id
            LEFT JOIN pinned_items p ON p.item_id = hi.item_id
            WHERE
              (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
              AND (?2 = 0 OR p.item_id IS NOT NULL)
              AND (?3 IS NULL OR hi.content LIKE '%' || ?3 || '%')
        ";
        let total = sqlx::query_scalar::<_, i64>(count_query_sql)
            .bind(category_filter.as_deref())
            .bind(pinned_flag)
            .bind(keyword_filter.as_deref())
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

        let data_query_sql = format!(
            "
            SELECT
              hi.id,
              COALESCE(hi.item_id, ''),
              hi.content,
              CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END,
              CASE WHEN p.item_id IS NULL THEN 0 ELSE 1 END,
              COALESCE(hi.updated_at, 0)
            FROM history_items hi
            LEFT JOIN categories c ON c.item_id = hi.item_id
            LEFT JOIN pinned_items p ON p.item_id = hi.item_id
            WHERE
              (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
              AND (?2 = 0 OR p.item_id IS NOT NULL)
              AND (?3 IS NULL OR hi.content LIKE '%' || ?3 || '%')
            ORDER BY {}
            LIMIT ?4 OFFSET ?5
            ",
            order_clause
        );
        let rows = sqlx::query(&data_query_sql)
            .bind(category_filter.as_deref())
            .bind(pinned_flag)
            .bind(keyword_filter.as_deref())
            .bind(limit_i64)
            .bind(offset_i64)
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        let items = rows
            .into_iter()
            .map(|row| {
                let content: String = row.try_get(2).unwrap_or_default();
                let mut id: String = row.try_get(1).unwrap_or_default();
                if id.is_empty() {
                    id = stable_history_item_id(&content);
                }
                ClipboardHistoryPageItem {
                    position: clamp_i64_to_usize(row.try_get::<i64, _>(0).unwrap_or(0)),
                    id,
                    content,
                    category: row
                        .try_get::<String, _>(3)
                        .unwrap_or_else(|_| "未分类".to_string()),
                    pinned: row.try_get::<i64, _>(4).unwrap_or(0) == 1,
                    updated_at: row.try_get::<i64, _>(5).unwrap_or(0),
                    snippet: None,
                }
            })
            .collect::<Vec<_>>();
        (total, items)
    };

    if let Some(key) = keyword_filter.as_deref() {
        for item in &mut items {
            item.snippet = Some(build_keyword_snippet(&item.content, key));
        }
    }

    Ok(ClipboardHistoryPageData {
        total: clamp_i64_to_usize(total),
        offset,
        limit: effective_limit,
        items,
    })
}

// ==================== 增量 CRUD 操作 ====================

/// 清空所有历史记录
pub async fn clear_all_history() -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query("DELETE FROM history_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM history_items_fts")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM categories")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM category_list")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM pinned_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

pub async fn save_history_data_snapshot_async(data: &ClipboardHistoryData) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let now_ms = now_unix_ms();
    let mut seen_item_ids = HashSet::new();
    let mut history_entries: Vec<(String, String, i64)> = Vec::new();
    for (idx, item) in data.items.iter().enumerate().rev() {
        let item_id = stable_history_item_id(item);
        if !seen_item_ids.insert(item_id.clone()) {
            continue;
        }
        let ts = now_ms - (idx as i64);
        history_entries.push((item_id, item.clone(), ts));
    }
    let desired_item_ids = history_entries
        .iter()
        .map(|(item_id, _, _)| item_id.clone())
        .collect::<Vec<_>>();
    let desired_item_id_set = desired_item_ids.iter().cloned().collect::<HashSet<_>>();

    bulk_upsert_history_items(&mut tx, &history_entries).await?;

    if desired_item_ids.is_empty() {
        sqlx::query("DELETE FROM history_items")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        let _ = sqlx::query("DELETE FROM history_items_fts")
            .execute(&mut *tx)
            .await;
        sqlx::query("DELETE FROM categories")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        sqlx::query("DELETE FROM pinned_items")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    } else {
        reset_temp_text_table(&mut tx, "temp_desired_history_item_ids", "item_id").await?;
        fill_temp_text_table(
            &mut tx,
            "temp_desired_history_item_ids",
            "item_id",
            &desired_item_ids,
        )
        .await?;

        sqlx::query(
            "
            DELETE FROM history_items
            WHERE item_id IS NOT NULL
              AND item_id != ''
              AND NOT EXISTS (
                SELECT 1
                FROM temp_desired_history_item_ids desired
                WHERE desired.item_id = history_items.item_id
              )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        sqlx::query(
            "
            DELETE FROM categories
            WHERE item_id IS NOT NULL
              AND item_id != ''
              AND NOT EXISTS (
                SELECT 1
                FROM temp_desired_history_item_ids desired
                WHERE desired.item_id = categories.item_id
              )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        sqlx::query(
            "
            DELETE FROM pinned_items
            WHERE item_id IS NOT NULL
              AND item_id != ''
              AND NOT EXISTS (
                SELECT 1
                FROM temp_desired_history_item_ids desired
                WHERE desired.item_id = pinned_items.item_id
              )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        let _ = sqlx::query(
            "
            DELETE FROM history_items_fts
            WHERE item_id IS NOT NULL
              AND item_id != ''
              AND NOT EXISTS (
                SELECT 1
                FROM temp_desired_history_item_ids desired
                WHERE desired.item_id = history_items_fts.item_id
              )
            ",
        )
        .execute(&mut *tx)
        .await;
    }

    let categories_to_upsert: Vec<(String, String)> = data.categories.iter()
        .filter(|(item_id, _)| desired_item_id_set.contains(item_id.as_str()))
        .map(|(item_id, category)| (item_id.clone(), category.clone()))
        .collect();
    bulk_upsert_categories(&mut tx, &categories_to_upsert).await?;

    sqlx::query("DELETE FROM category_list")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    bulk_upsert_category_list(&mut tx, &data.category_list).await?;

    let pinned_to_upsert: Vec<(String, i64)> = data.pinned_items.iter().enumerate()
        .filter(|(_, item_id)| desired_item_id_set.contains(item_id.as_str()))
        .map(|(idx, item_id)| (item_id.clone(), now_ms - (idx as i64)))
        .collect();
    bulk_upsert_pinned_items(&mut tx, &pinned_to_upsert).await?;

    // 批量更新FTS索引：用一条SQL替代逐条N+1查询
    let _ = sqlx::query(
        "
        INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
        SELECT id, COALESCE(item_id, ''), content
        FROM history_items
        WHERE item_id IN (SELECT item_id FROM temp_desired_history_item_ids)
        ",
    )
    .execute(&mut *tx)
    .await;

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

/// 仅同步 history_items 的顺序与内容（高频路径优化），并清理失效关联。
pub async fn save_history_items_only_async(items: &[String]) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let now_ms = now_unix_ms();
    let mut seen_item_ids = HashSet::new();
    let mut history_entries: Vec<(String, String, i64)> = Vec::new();
    for (idx, item) in items.iter().enumerate().rev() {
        let item_id = stable_history_item_id(item);
        if !seen_item_ids.insert(item_id.clone()) {
            continue;
        }
        let ts = now_ms - (idx as i64);
        history_entries.push((item_id, item.clone(), ts));
    }
    let desired_item_ids = history_entries
        .iter()
        .map(|(item_id, _, _)| item_id.clone())
        .collect::<Vec<_>>();

    bulk_upsert_history_items(&mut tx, &history_entries).await?;

    if desired_item_ids.is_empty() {
        sqlx::query("DELETE FROM history_items")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        let _ = sqlx::query("DELETE FROM history_items_fts")
            .execute(&mut *tx)
            .await;
        sqlx::query("DELETE FROM categories")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        sqlx::query("DELETE FROM pinned_items")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    } else {
        reset_temp_text_table(&mut tx, "temp_desired_history_item_ids", "item_id").await?;
        fill_temp_text_table(
            &mut tx,
            "temp_desired_history_item_ids",
            "item_id",
            &desired_item_ids,
        )
        .await?;

        sqlx::query(
            "
            DELETE FROM history_items
            WHERE item_id IS NOT NULL
              AND item_id != ''
              AND NOT EXISTS (
                SELECT 1
                FROM temp_desired_history_item_ids desired
                WHERE desired.item_id = history_items.item_id
              )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        sqlx::query(
            "
            DELETE FROM categories
            WHERE item_id IS NOT NULL
              AND item_id != ''
              AND NOT EXISTS (
                SELECT 1
                FROM temp_desired_history_item_ids desired
                WHERE desired.item_id = categories.item_id
              )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        sqlx::query(
            "
            DELETE FROM pinned_items
            WHERE item_id IS NOT NULL
              AND item_id != ''
              AND NOT EXISTS (
                SELECT 1
                FROM temp_desired_history_item_ids desired
                WHERE desired.item_id = pinned_items.item_id
              )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        let _ = sqlx::query(
            "
            DELETE FROM history_items_fts
            WHERE item_id IS NOT NULL
              AND item_id != ''
              AND NOT EXISTS (
                SELECT 1
                FROM temp_desired_history_item_ids desired
                WHERE desired.item_id = history_items_fts.item_id
              )
            ",
        )
        .execute(&mut *tx)
        .await;
    }

    // 批量更新FTS索引：用一条SQL替代逐条N+1查询
    let _ = sqlx::query(
        "
        INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
        SELECT id, COALESCE(item_id, ''), content
        FROM history_items
        WHERE item_id IN (SELECT item_id FROM temp_desired_history_item_ids)
        ",
    )
    .execute(&mut *tx)
    .await;

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

/// 仅同步分类映射与分类列表。
/// 优化 (P5): 使用 UPSERT 替代 DELETE + INSERT，减少 WAL 日志写入
pub async fn save_categories_state_async(
    categories: &HashMap<String, String>,
    category_list: &[String],
) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    // 使用 UPSERT 替代 DELETE + INSERT
    for chunk in categories.iter().collect::<Vec<_>>().chunks(500) {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
            "INSERT INTO categories(category, item_id) ",
        );
        qb.push_values(chunk.iter(), |mut b, (item_id, category)| {
            b.push_bind(category.as_str()).push_bind(item_id.as_str());
        });
        qb.push(" ON CONFLICT(item_id) DO UPDATE SET category = excluded.category");
        qb.build()
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    // 清理不再需要的分类映射
    if !categories.is_empty() {
        let category_ids: Vec<&str> = categories.keys().map(|s| s.as_str()).collect();
        for chunk in category_ids.chunks(500) {
            let placeholders: Vec<String> = chunk.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
            let sql = format!(
                "DELETE FROM categories WHERE item_id NOT IN ({})",
                placeholders.join(",")
            );
            let mut query = sqlx::query(&sql);
            for id in chunk {
                query = query.bind(*id);
            }
            query.execute(&mut *tx)
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        }
    } else {
        sqlx::query("DELETE FROM categories")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    // 分类列表仍使用 DELETE + INSERT（列表通常很小）
    sqlx::query("DELETE FROM category_list")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    for category in category_list {
        sqlx::query("INSERT INTO category_list(category) VALUES(?)")
            .bind(category)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

/// 仅同步置顶顺序（按 pinned_items 向量顺序重建 pinned_at）。
/// 优化：使用批量 INSERT 替代逐条插入
pub async fn save_pinned_items_order_async(pinned_items: &[String]) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query("DELETE FROM pinned_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    if !pinned_items.is_empty() {
        let base_ts = now_unix_ms();
        for chunk in pinned_items.chunks(500) {
            let mut qb: QueryBuilder<Sqlite> =
                QueryBuilder::new("INSERT INTO pinned_items(pinned_at, item_id) ");
            qb.push_values(
                chunk.iter().enumerate(),
                |mut b, (idx, item_id)| {
                    let pinned_at = base_ts + (pinned_items.len().saturating_sub(idx) as i64);
                    b.push_bind(pinned_at).push_bind(item_id);
                },
            );
            qb.build()
                .execute(&mut *tx)
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        }
    }

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

/// 置顶记录（增量操作）
pub async fn pin_item(item_id: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let now = now_unix_ms();

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM history_items WHERE item_id = ?1)")
            .bind(item_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    if !exists {
        return Err(AppErrorKind::DatabaseTargetNotFound.to_frontend_json());
    }

    sqlx::query(
        "INSERT INTO pinned_items(pinned_at, item_id) VALUES(?1, ?2)
         ON CONFLICT(item_id) DO UPDATE SET pinned_at = ?1",
    )
    .bind(now)
    .bind(item_id)
    .execute(&mut *conn)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

/// 取消置顶（增量操作）
pub async fn unpin_item(item_id: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    sqlx::query("DELETE FROM pinned_items WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

/// 设置记录分类(增量操作)
pub async fn set_item_category(item_id: &str, category: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM history_items WHERE item_id = ?1)")
            .bind(item_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    if exists {
        sqlx::query(
            "INSERT INTO categories(category, item_id) VALUES (?1, ?2)
             ON CONFLICT(item_id) DO UPDATE SET category = ?1",
        )
        .bind(category)
        .bind(item_id)
        .execute(&mut *conn)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    } else {
        return Err(AppErrorKind::DatabaseTargetNotFound.to_frontend_json());
    }

    Ok(())
}

/// 删除记录分类(增量操作)
pub async fn remove_item_category(item_id: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    sqlx::query("DELETE FROM categories WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

/// 添加分类到列表（增量操作）
pub async fn add_category_to_list(category: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    sqlx::query("INSERT OR IGNORE INTO category_list(category) VALUES(?)")
        .bind(category)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

/// 从列表删除分类（增量操作）
pub async fn remove_category_from_list(category: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    sqlx::query("DELETE FROM category_list WHERE category = ?")
        .bind(category)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

/// 从映射表与分类列表同时删除指定分类（增量操作）
pub async fn remove_category_everywhere(category: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query("DELETE FROM categories WHERE category = ?")
        .bind(category)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query("DELETE FROM category_list WHERE category = ?")
        .bind(category)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

/// 合并历史数据:保留现有记录,只添加备份中不存在的新记录
pub async fn merge_history_data_async(data: &ClipboardHistoryData) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let now_ms = now_unix_ms();

    let existing_ids: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT item_id FROM history_items WHERE item_id IS NOT NULL AND item_id != ''",
    )
    .fetch_all(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?
    .into_iter()
    .collect();

    let mut new_entries = Vec::new();
    for (idx, item) in data.items.iter().enumerate().rev() {
        let item_id = stable_history_item_id(item);
        if existing_ids.contains(&item_id) {
            continue;
        }
        let ts = now_ms - (idx as i64);
        new_entries.push((item_id, item.clone(), ts));
    }

    let new_count = new_entries.len();
    for chunk in new_entries.chunks(300) {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
            "INSERT INTO history_items(content, item_id, created_at, updated_at) ",
        );
        qb.push_values(chunk, |mut b, (id, c, ts)| {
            b.push_bind(c).push_bind(id).push_bind(*ts).push_bind(*ts);
        });
        qb.build()
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    log::info!("合并文本历史: 新增 {} 条记录", new_count);

    for (item_id, category) in &data.categories {
        if existing_ids.contains(item_id) {
            sqlx::query(
                "INSERT INTO categories(category, item_id) VALUES(?1, ?2)
             ON CONFLICT(item_id) DO UPDATE SET category = ?1",
            )
            .bind(category)
            .bind(item_id)
            .execute(&mut *tx)
            .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        } else {
            sqlx::query("INSERT OR IGNORE INTO categories(category, item_id) VALUES(?1, ?2)")
                .bind(category)
                .bind(item_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        }
    }

    let existing_categories: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT category FROM category_list")
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?
            .into_iter()
            .collect();

    for category in &data.category_list {
        if !existing_categories.contains(category) {
            sqlx::query("INSERT INTO category_list(category) VALUES(?)")
                .bind(category)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        }
    }

    let existing_pinned: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT item_id FROM pinned_items WHERE item_id IS NOT NULL AND item_id != ''",
    )
    .fetch_all(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?
    .into_iter()
    .collect();

    let has_position =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM pinned_items")
            .fetch_one(&mut *tx)
            .await
            .is_ok();

    if has_position {
        let current_max_position =
            sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM pinned_items")
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?
                .unwrap_or(-1);

        let mut position = current_max_position + 1;
        for (idx, item_id) in data.pinned_items.iter().enumerate() {
            if existing_pinned.contains(item_id) {
                continue;
            }

            let pinned_at = now_ms - (idx as i64);
            sqlx::query(
                "INSERT INTO pinned_items(pinned_at, item_id, position) VALUES(?1, ?2, ?3)
                 ON CONFLICT(item_id) DO NOTHING",
            )
            .bind(pinned_at)
            .bind(item_id)
            .bind(position)
            .execute(&mut *tx)
            .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
            position += 1;
        }
    } else {
        for (idx, item_id) in data.pinned_items.iter().enumerate() {
            if existing_pinned.contains(item_id) {
                continue;
            }

            let pinned_at = now_ms - (idx as i64);
            sqlx::query("INSERT OR IGNORE INTO pinned_items(pinned_at, item_id) VALUES(?1, ?2)")
                .bind(pinned_at)
                .bind(item_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        }
    }

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_id_deterministic() {
        let id1 = stable_history_item_id("hello world");
        let id2 = stable_history_item_id("hello world");
        assert_eq!(id1, id2);
    }

    #[test]
    fn history_id_different_inputs() {
        let id1 = stable_history_item_id("hello");
        let id2 = stable_history_item_id("world");
        assert_ne!(id1, id2);
    }

    #[test]
    fn history_id_hex_format() {
        let id = stable_history_item_id("test");
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn history_id_empty() {
        let id = stable_history_item_id("");
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn history_id_chinese() {
        let id = stable_history_item_id("你好世界");
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn sort_default_desc() {
        let clause = resolve_history_sort(None, None);
        assert!(clause.contains("DESC"));
    }

    #[test]
    fn sort_updated_at_asc() {
        let clause = resolve_history_sort(Some("updatedAt".to_string()), Some("asc".to_string()));
        assert!(clause.contains("updated_at ASC"));
    }

    #[test]
    fn sort_created_at_desc() {
        let clause = resolve_history_sort(Some("created_at".to_string()), Some("desc".to_string()));
        assert!(clause.contains("created_at DESC"));
    }

    #[test]
    fn sort_pinned_first_asc() {
        let clause = resolve_history_sort(Some("pinnedFirst".to_string()), Some("asc".to_string()));
        assert!(clause.contains("pinned_at ASC"));
    }

    #[test]
    fn sort_pinned_first_desc() {
        let clause = resolve_history_sort(Some("pinned_first".to_string()), Some("desc".to_string()));
        assert!(clause.contains("pinned_at DESC"));
    }

    #[test]
    fn sort_by_id_asc() {
        let clause = resolve_history_sort(Some("id".to_string()), Some("asc".to_string()));
        assert!(clause.contains("hi.id ASC"));
    }

    #[test]
    fn sort_unknown_fallback_desc() {
        let clause = resolve_history_sort(Some("unknown".to_string()), Some("desc".to_string()));
        assert!(clause.contains("updated_at DESC"));
    }

    #[test]
    fn sort_unknown_fallback_asc() {
        let clause = resolve_history_sort(Some("unknown".to_string()), Some("asc".to_string()));
        assert!(clause.contains("updated_at ASC"));
    }

    #[test]
    fn fts_empty() {
        assert_eq!(build_fts_query_and(""), "");
        assert_eq!(build_fts_query_and("   "), "");
    }

    #[test]
    fn fts_single_token() {
        assert_eq!(build_fts_query_and("hello"), "\"hello\"*");
    }

    #[test]
    fn fts_multi_tokens_and() {
        let q = build_fts_query_and("hello world");
        assert!(q.contains(" AND "));
    }

    #[test]
    fn fts_escapes_quote() {
        // FTS5 inside quoted tokens: " becomes ""
        let q = build_fts_query_and("say \"hi\"");
        assert!(q.contains("\"\""), "双引号应被转义为双写: {}", q);
    }

    #[test]
    fn fts_escapes_backslash() {
        let q = build_fts_query_and("a\\b");
        assert!(q.contains("\\\\"));
    }

    #[test]
    fn fts_escapes_parens() {
        let q = build_fts_query_and("a(b)");
        assert!(q.contains("\\("));
        assert!(q.contains("\\)"));
    }

    #[test]
    fn fts_escapes_operators() {
        let q = build_fts_query_and("a+b-c:d*e^f");
        assert!(q.contains("\\+"));
        assert!(q.contains("\\-"));
        assert!(q.contains("\\:"));
        assert!(q.contains("\\*"));
        assert!(q.contains("\\^"));
    }

    #[test]
    fn snippet_empty_content() {
        assert_eq!(build_keyword_snippet("", "hello"), "");
    }

    #[test]
    fn snippet_empty_keyword() {
        assert_eq!(build_keyword_snippet("hello world", ""), "hello world");
    }

    #[test]
    fn snippet_finds_keyword() {
        let s = build_keyword_snippet("aaa bbb ccc ddd", "bbb");
        assert!(s.contains("bbb"));
    }

    #[test]
    fn snippet_case_insensitive() {
        let s = build_keyword_snippet("Hello World", "hello");
        assert!(s.contains("Hello"));
    }

    #[test]
    fn snippet_chinese() {
        let s = build_keyword_snippet("这是一段中文测试内容", "测试");
        assert!(s.contains("测试"));
    }

    #[test]
    fn boundary_on_boundary() {
        let s = "hello";
        assert_eq!(adjust_to_char_boundary(s, 3), 3);
    }

    #[test]
    fn boundary_out_of_range() {
        let s = "hello";
        assert_eq!(adjust_to_char_boundary(s, 100), 5);
    }

    #[test]
    fn boundary_chinese_backward() {
        let s = "你好世界";
        let adj = adjust_to_char_boundary(s, 1);
        assert!(s.is_char_boundary(adj));
    }

    #[test]
    fn boundary_chinese_forward() {
        let s = "你好世界";
        let adj = adjust_to_char_boundary(s, 1);
        assert!(s.is_char_boundary(adj));
    }

    // ===== get_history_db_path =====

    #[test]
    fn history_db_path_ends_with_history_db() {
        let path = get_history_db_path();
        assert!(path.ends_with("history.db"));
    }

    // ===== build_fts_query additional edge cases =====

    #[test]
    fn fts_only_whitespace_tokens_filtered() {
        let q = build_fts_query_and("   ");
        assert_eq!(q, "");
    }

    #[test]
    fn fts_mixed_empty_and_real_tokens() {
        let q = build_fts_query_and("  hello   world  ");
        assert!(q.contains("\"hello\"*"));
        assert!(q.contains("\"world\"*"));
        assert!(q.contains(" AND "));
    }

    #[test]
    fn fts_caret_escaped() {
        let q = build_fts_query_and("test^value");
        assert!(q.contains("\\^"));
    }

    // ===== build_keyword_snippet additional edge cases =====

    #[test]
    fn snippet_keyword_at_byte_boundary() {
        // Chinese characters are multi-byte
        let snippet = build_keyword_snippet("这是测试内容", "测试");
        assert!(snippet.contains("测试"));
    }

    #[test]
    fn snippet_very_long_content() {
        let long = "a".repeat(1000);
        let snippet = build_keyword_snippet(&long, "a");
        assert!(snippet.contains("a"));
        assert!(snippet.len() < 200, "摘要应被截断");
    }

    #[test]
    fn snippet_keyword_not_found() {
        let snippet = build_keyword_snippet("hello world", "xyz");
        // Keyword not found: returns content (truncated), no "..."
        // "hello world" is short enough to not need truncation
        assert!(snippet.contains("hello"));
    }

    #[test]
    fn snippet_keyword_at_start_no_ellipsis_prefix() {
        let snippet = build_keyword_snippet("hello world", "hello");
        assert!(!snippet.starts_with("..."));
    }

    #[test]
    fn snippet_keyword_at_end_no_ellipsis_suffix() {
        let snippet = build_keyword_snippet("hello world", "world");
        assert!(!snippet.ends_with("..."));
    }

    // ===== adjust_to_char_boundary additional edge cases =====

    #[test]
    fn boundary_at_start() {
        let s = "hello";
        assert_eq!(adjust_to_char_boundary(s, 0), 0);
    }

    #[test]
    fn boundary_at_end() {
        let s = "hello";
        assert_eq!(adjust_to_char_boundary(s, 5), 5);
    }

    #[test]
    fn boundary_exact_length() {
        let s = "hi";
        assert_eq!(adjust_to_char_boundary(s, 2), 2);
    }

    // ===================================================================
    // 集成测试：真实 SQLite 数据库操作
    // ===================================================================

    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        // 创建与生产环境相同的 schema
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
            CREATE VIRTUAL TABLE IF NOT EXISTS history_items_fts USING fts5(
                item_id UNINDEXED,
                content,
                tokenize = 'unicode61'
            );
            ",
        )
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn integration_crud_insert_and_query() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入
        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("hello world")
            .bind("abc123")
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        // 查询
        let row: (String,) = sqlx::query_as("SELECT content FROM history_items WHERE item_id = ?1")
            .bind("abc123")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, "hello world");
    }

    #[tokio::test]
    async fn integration_crud_update() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("original")
            .bind("id1")
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("UPDATE history_items SET content = ?1 WHERE item_id = ?2")
            .bind("updated")
            .bind("id1")
            .execute(&pool)
            .await
            .unwrap();

        let row: (String,) = sqlx::query_as("SELECT content FROM history_items WHERE item_id = ?1")
            .bind("id1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, "updated");
    }

    #[tokio::test]
    async fn integration_crud_delete() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("to delete")
            .bind("del1")
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM history_items WHERE item_id = ?1")
            .bind("del1")
            .execute(&pool)
            .await
            .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn integration_unique_constraint() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("first")
            .bind("dup1")
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        // 重复 item_id 应该失败
        let result = sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("second")
            .bind("dup1")
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await;

        assert!(result.is_err(), "重复 item_id 应该触发 UNIQUE 约束");
    }

    #[tokio::test]
    async fn integration_categories_full_flow() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入历史记录
        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("test content")
            .bind("item1")
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        // 设置分类
        sqlx::query(
            "INSERT INTO categories(category, item_id) VALUES(?1, ?2) ON CONFLICT(item_id) DO UPDATE SET category = ?1",
        )
            .bind("工作")
            .bind("item1")
            .execute(&pool)
            .await
            .unwrap();

        // 查询分类
        let cat: (String,) = sqlx::query_as("SELECT category FROM categories WHERE item_id = ?1")
            .bind("item1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cat.0, "工作");

        // 更新分类
        sqlx::query(
            "INSERT INTO categories(category, item_id) VALUES(?1, ?2) ON CONFLICT(item_id) DO UPDATE SET category = ?1",
        )
            .bind("生活")
            .bind("item1")
            .execute(&pool)
            .await
            .unwrap();

        let cat: (String,) = sqlx::query_as("SELECT category FROM categories WHERE item_id = ?1")
            .bind("item1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cat.0, "生活");

        // 删除分类
        sqlx::query("DELETE FROM categories WHERE item_id = ?1")
            .bind("item1")
            .execute(&pool)
            .await
            .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories WHERE item_id = ?1")
            .bind("item1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn integration_pinned_items_full_flow() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入多条记录
        for i in 0..5 {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(format!("content {}", i))
                .bind(format!("item{}", i))
                .bind(now + i)
                .bind(now + i)
                .execute(&pool)
                .await
                .unwrap();
        }

        // 置顶 item1 和 item3
        sqlx::query("INSERT INTO pinned_items(pinned_at, item_id) VALUES(?1, ?2)")
            .bind(now)
            .bind("item1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO pinned_items(pinned_at, item_id) VALUES(?1, ?2)")
            .bind(now + 1)
            .bind("item3")
            .execute(&pool)
            .await
            .unwrap();

        let pinned: Vec<String> = sqlx::query_scalar("SELECT item_id FROM pinned_items ORDER BY pinned_at DESC")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(pinned, vec!["item3", "item1"]);

        // 取消置顶 item1
        sqlx::query("DELETE FROM pinned_items WHERE item_id = ?1")
            .bind("item1")
            .execute(&pool)
            .await
            .unwrap();

        let pinned: Vec<String> = sqlx::query_scalar("SELECT item_id FROM pinned_items ORDER BY pinned_at DESC")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(pinned, vec!["item3"]);
    }

    #[tokio::test]
    async fn integration_fts_search() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入带 FTS 索引的数据
        let items = vec![
            ("hello world", "h1"),
            ("hello rust", "h2"),
            ("foo bar baz", "f1"),
            ("rust programming", "r1"),
        ];
        for (content, id) in &items {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(content)
                .bind(id)
                .bind(now)
                .bind(now)
                .execute(&pool)
                .await
                .unwrap();

            sqlx::query(
                "INSERT INTO history_items_fts(rowid, item_id, content) VALUES (?1, ?2, ?3)",
            )
                .bind(sqlx::query_scalar::<_, i64>("SELECT id FROM history_items WHERE item_id = ?1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap())
                .bind(id)
                .bind(content)
                .execute(&pool)
                .await
                .unwrap();
        }

        // FTS 搜索 "hello"
        let results: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM history_items_fts WHERE history_items_fts MATCH '\"hello\"*'",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(results.contains(&"h1".to_string()));
        assert!(results.contains(&"h2".to_string()));
        assert!(!results.contains(&"f1".to_string()));

        // FTS 搜索 "rust"
        let results: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM history_items_fts WHERE history_items_fts MATCH '\"rust\"*'",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(results.contains(&"h2".to_string()));
        assert!(results.contains(&"r1".to_string()));
    }

    #[tokio::test]
    async fn integration_fts_special_chars() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("test(value) with +special-chars:ok")
            .bind("s1")
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO history_items_fts(rowid, item_id, content) VALUES (?1, ?2, ?3)",
        )
            .bind(1i64)
            .bind("s1")
            .bind("test(value) with +special-chars:ok")
            .execute(&pool)
            .await
            .unwrap();

        // 转义后的 FTS 查询应该能找到
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM history_items_fts WHERE history_items_fts MATCH '\"test\\(value\\) with \\+special\\-chars\\:ok\"*'",
        )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(result, 1, "转义后的 FTS 查询应该匹配");
    }

    #[tokio::test]
    async fn integration_concurrent_writes() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();
        let pool = std::sync::Arc::new(pool);

        let mut handles = vec![];

        // 10 个并发写入任务
        for i in 0..10 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                for j in 0..20 {
                    let content = format!("item-{}-{}", i, j);
                    let item_id = format!("c{}-{}", i, j);
                    sqlx::query(
                        "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                    )
                        .bind(&content)
                        .bind(&item_id)
                        .bind(now + (i * 20 + j) as i64)
                        .bind(now + (i * 20 + j) as i64)
                        .execute(pool.as_ref())
                        .await
                        .unwrap();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
        assert_eq!(count, 200, "并发写入 200 条记录");
    }

    #[tokio::test]
    async fn integration_transaction_rollback() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 先插入一条
        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("existing")
            .bind("keep1")
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        // 开启事务，插入后回滚
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("will_rollback")
            .bind("roll1")
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .unwrap();

        tx.rollback().await.unwrap();

        // 验证回滚后只有原始数据
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "事务回滚后应该只有 1 条记录");

        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items WHERE item_id = 'roll1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(exists, 0, "回滚的记录不应该存在");
    }

    #[tokio::test]
    async fn integration_transaction_commit() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        let mut tx = pool.begin().await.unwrap();
        for i in 0..5 {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(format!("item {}", i))
                .bind(format!("batch{}", i))
                .bind(now + i)
                .bind(now + i)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn integration_bulk_upsert() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 第一批插入
        let entries: Vec<(&str, &str, i64)> = vec![
            ("a", "item_a", now),
            ("b", "item_b", now + 1),
            ("c", "item_c", now + 2),
        ];
        for (content, id, ts) in &entries {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(content)
                .bind(id)
                .bind(ts)
                .bind(ts)
                .execute(&pool)
                .await
                .unwrap();
        }

        // 第二批：更新 a，新增 d
        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(item_id) DO UPDATE SET content = excluded.content",
        )
            .bind("a_updated")
            .bind("item_a")
            .bind(now + 10)
            .bind(now + 10)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
            .bind("d")
            .bind("item_d")
            .bind(now + 3)
            .bind(now + 3)
            .execute(&pool)
            .await
            .unwrap();

        // 验证
        let row: (String,) = sqlx::query_as("SELECT content FROM history_items WHERE item_id = 'item_a'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, "a_updated", "UPSERT 应该更新已有记录");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 4, "应该有 4 条记录");
    }

    #[tokio::test]
    async fn integration_category_list_management() {
        let pool = create_test_pool().await;

        // 添加分类
        for cat in &["工作", "生活", "学习"] {
            sqlx::query("INSERT OR IGNORE INTO category_list(category) VALUES(?)")
                .bind(cat)
                .execute(&pool)
                .await
                .unwrap();
        }

        let cats: Vec<String> = sqlx::query_scalar("SELECT category FROM category_list ORDER BY id ASC")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(cats, vec!["工作", "生活", "学习"]);

        // 重复添加不报错
        sqlx::query("INSERT OR IGNORE INTO category_list(category) VALUES(?)")
            .bind("工作")
            .execute(&pool)
            .await
            .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM category_list")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 3, "重复添加不应增加记录数");

        // 删除分类
        sqlx::query("DELETE FROM category_list WHERE category = ?1")
            .bind("生活")
            .execute(&pool)
            .await
            .unwrap();

        let cats: Vec<String> = sqlx::query_scalar("SELECT category FROM category_list ORDER BY id ASC")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(cats, vec!["工作", "学习"]);
    }

    #[tokio::test]
    async fn integration_large_dataset_pagination() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入 500 条记录
        for i in 0..500 {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(format!("item content {}", i))
                .bind(format!("pg{:04}", i))
                .bind(now + i)
                .bind(now + i)
                .execute(&pool)
                .await
                .unwrap();
        }

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 500);

        // 分页查询
        let page1: Vec<String> = sqlx::query_scalar(
            "SELECT content FROM history_items ORDER BY updated_at DESC LIMIT 50 OFFSET 0",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(page1.len(), 50);
        assert!(page1[0].contains("499"), "第一页应该包含最新的记录");

        let page2: Vec<String> = sqlx::query_scalar(
            "SELECT content FROM history_items ORDER BY updated_at DESC LIMIT 50 OFFSET 50",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(page2.len(), 50);

        // 最后一页
        let last_page: Vec<String> = sqlx::query_scalar(
            "SELECT content FROM history_items ORDER BY updated_at DESC LIMIT 50 OFFSET 450",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(last_page.len(), 50);
    }

    // ===================================================================
    // 高级集成测试：FTS 重建、快照一致性、并发读写、特殊字符
    // ===================================================================

    #[tokio::test]
    async fn integration_fts_rebuild_flow() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入 10 条数据
        for i in 0..10 {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(format!("content {}", i))
                .bind(format!("fts{:02}", i))
                .bind(now + i)
                .bind(now + i)
                .execute(&pool)
                .await
                .unwrap();
        }

        // 模拟 FTS 索引不同步：手动插入一条不匹配的 FTS 记录
        sqlx::query(
            "INSERT INTO history_items_fts(rowid, item_id, content) VALUES (999, 'ghost', 'ghost data')",
        )
            .execute(&pool)
            .await
            .unwrap();

        // FTS 重建：INSERT OR REPLACE 全量同步
        sqlx::query(
            "INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
             SELECT id, COALESCE(item_id, ''), content FROM history_items",
        )
            .execute(&pool)
            .await
            .unwrap();

        // 清理孤儿记录
        sqlx::query(
            "DELETE FROM history_items_fts WHERE rowid NOT IN (SELECT id FROM history_items)",
        )
            .execute(&pool)
            .await
            .unwrap();

        // 验证搜索结果正确
        let results: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM history_items_fts WHERE history_items_fts MATCH '\"content 5\"*'",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(results, vec!["fts05"]);

        // 验证幽灵记录已清除
        let ghost: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM history_items_fts WHERE item_id = 'ghost'",
        )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(ghost, 0, "幽灵 FTS 记录应被清除");
    }

    #[tokio::test]
    async fn integration_snapshot_consistency() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 模拟 save_history_data_snapshot_async 的核心流程
        let items = vec![
            ("item_a".to_string(), "content a".to_string(), now),
            ("item_b".to_string(), "content b".to_string(), now + 1),
            ("item_c".to_string(), "content c".to_string(), now + 2),
        ];
        let categories = HashMap::from([
            ("item_a".to_string(), "工作".to_string()),
            ("item_b".to_string(), "生活".to_string()),
        ]);
        let category_list = vec!["工作".to_string(), "生活".to_string()];
        let pinned = vec!["item_a".to_string()];

        let mut tx = pool.begin().await.unwrap();

        // 1. UPSERT history_items
        for (id, content, ts) in &items {
            sqlx::query(
                "INSERT INTO history_items(content, item_id, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(item_id) DO UPDATE SET content=excluded.content, updated_at=excluded.updated_at",
            )
                .bind(content)
                .bind(id)
                .bind(ts)
                .bind(ts)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        // 2. 清理不在列表中的记录
        let desired_ids: Vec<String> = items.iter().map(|(id, _, _)| id.clone()).collect();
        sqlx::query("DELETE FROM history_items WHERE item_id NOT IN (SELECT value FROM json_each(?1))")
            .bind(serde_json::to_string(&desired_ids).unwrap())
            .execute(&mut *tx)
            .await
            .unwrap();

        // 3. 同步分类
        for (item_id, category) in &categories {
            sqlx::query(
                "INSERT INTO categories(category, item_id) VALUES(?1, ?2)
                 ON CONFLICT(item_id) DO UPDATE SET category=excluded.category",
            )
                .bind(category)
                .bind(item_id)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        // 4. 同步分类列表
        sqlx::query("DELETE FROM category_list").execute(&mut *tx).await.unwrap();
        for cat in &category_list {
            sqlx::query("INSERT INTO category_list(category) VALUES(?)")
                .bind(cat)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        // 5. 同步置顶
        sqlx::query("DELETE FROM pinned_items").execute(&mut *tx).await.unwrap();
        for (idx, item_id) in pinned.iter().enumerate() {
            sqlx::query("INSERT INTO pinned_items(pinned_at, item_id) VALUES(?1, ?2)")
                .bind(now - idx as i64)
                .bind(item_id)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        // 6. 重建 FTS
        sqlx::query(
            "INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
             SELECT id, COALESCE(item_id, ''), content FROM history_items",
        )
            .execute(&mut *tx)
            .await
            .unwrap();

        tx.commit().await.unwrap();

        // === 验证快照一致性 ===

        // 历史记录数量
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 3);

        // 分类数量
        let cat_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(cat_count, 2);

        // 分类列表
        let cats: Vec<String> = sqlx::query_scalar("SELECT category FROM category_list ORDER BY id")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(cats, vec!["工作", "生活"]);

        // 置顶
        let pinned_ids: Vec<String> = sqlx::query_scalar("SELECT item_id FROM pinned_items")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(pinned_ids, vec!["item_a"]);

        // FTS 索引一致
        let fts_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items_fts")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(fts_count, 3, "FTS 应该与 history_items 行数一致");
    }

    #[tokio::test]
    async fn integration_concurrent_read_write() {
        let pool = std::sync::Arc::new(create_test_pool().await);
        let now = now_unix_ms();

        // 先插入基础数据
        for i in 0..50 {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(format!("base item {}", i))
                .bind(format!("base{:02}", i))
                .bind(now + i)
                .bind(now + i)
                .execute(pool.as_ref())
                .await
                .unwrap();
        }

        let mut handles = vec![];

        // 5 个写入者：各写入 20 条
        for w in 0..5 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                for j in 0..20 {
                    let content = format!("writer-{}-item-{}", w, j);
                    let item_id = format!("w{}-{:02}", w, j);
                    sqlx::query(
                        "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(item_id) DO UPDATE SET content=excluded.content",
                    )
                        .bind(&content)
                        .bind(&item_id)
                        .bind(now + 1000 + (w * 20 + j) as i64)
                        .bind(now + 1000 + (w * 20 + j) as i64)
                        .execute(pool.as_ref())
                        .await
                        .unwrap();
                }
            }));
        }

        // 5 个读取者：各读取 10 次
        for _ in 0..5 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
                        .fetch_one(pool.as_ref()).await.unwrap();
                    assert!(count >= 50, "读取时至少有基础数据: {}", count);

                    let _rows: Vec<String> = sqlx::query_scalar(
                        "SELECT content FROM history_items ORDER BY updated_at DESC LIMIT 5"
                    )
                        .fetch_all(pool.as_ref()).await.unwrap();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // 最终验证：50 基础 + 100 写入 = 150
        let final_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(pool.as_ref()).await.unwrap();
        assert_eq!(final_count, 150, "并发读写后数据完整");
    }

    #[tokio::test]
    async fn integration_special_char_content() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        let long_content = "超长行：".to_string() + &"a".repeat(10000);
        let special_contents = vec![
            "中文内容包含特殊字符：「」【】、。！？",
            "emoji 🎉🔥💀🚀",
            "SQL注入尝试：'; DROP TABLE history_items; --",
            "路径遍历：../../etc/passwd",
            "null字节测试：hello\u{0000}world",
            &long_content,
            "多行内容\n第二行\n第三行",
            "HTML标签：<script>alert('xss')</script>",
            "JSON内容：{\"key\": \"value\", \"num\": 42}",
            "Unicode混合：Hello 世界 مرحبا Здравствуйте",
        ];

        for (i, content) in special_contents.iter().enumerate() {
            let item_id = format!("sp{:02}", i);
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(content)
                .bind(&item_id)
                .bind(now + i as i64)
                .bind(now + i as i64)
                .execute(&pool)
                .await
                .unwrap();

            // 验证能正确读回
            let row: (String,) = sqlx::query_as("SELECT content FROM history_items WHERE item_id = ?1")
                .bind(&item_id)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(row.0, *content, "内容应该完整保留: {}", item_id);
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, special_contents.len() as i64);
    }

    #[tokio::test]
    async fn integration_fts_mixed_language_search() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        let items = vec![
            ("hello world test", "en1"),
            ("programming in rust", "en2"),
            ("foo bar baz", "en3"),
        ];

        for (content, id) in &items {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(content)
                .bind(id)
                .bind(now)
                .bind(now)
                .execute(&pool).await.unwrap();

            let row_id: i64 = sqlx::query_scalar("SELECT id FROM history_items WHERE item_id = ?1")
                .bind(id).fetch_one(&pool).await.unwrap();
            sqlx::query("INSERT INTO history_items_fts(rowid, item_id, content) VALUES (?1, ?2, ?3)")
                .bind(row_id).bind(id).bind(content)
                .execute(&pool).await.unwrap();
        }

        // 搜索 "hello"
        let results: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM history_items_fts WHERE history_items_fts MATCH '\"hello\"*'",
        ).fetch_all(&pool).await.unwrap();
        assert!(results.contains(&"en1".to_string()));
        assert!(!results.contains(&"en2".to_string()));

        // 搜索 "rust"
        let results: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM history_items_fts WHERE history_items_fts MATCH '\"rust\"*'",
        ).fetch_all(&pool).await.unwrap();
        assert!(results.contains(&"en2".to_string()));
    }

    #[tokio::test]
    async fn integration_batch_delete_by_ids() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入 20 条
        for i in 0..20 {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
                .bind(format!("item {}", i))
                .bind(format!("del{:02}", i))
                .bind(now + i)
                .bind(now + i)
                .execute(&pool).await.unwrap();
        }

        // 批量删除 5 条
        let to_delete = vec!["del00", "del05", "del10", "del15", "del19"];
        for id in &to_delete {
            sqlx::query("DELETE FROM history_items WHERE item_id = ?1")
                .bind(id).execute(&pool).await.unwrap();
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 15, "应该删除 5 条，剩余 15 条");

        // 验证删除的确实不存在
        for id in &to_delete {
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history_items WHERE item_id = ?1")
                .bind(id).fetch_one(&pool).await.unwrap();
            assert_eq!(exists, 0, "{} 应该已删除", id);
        }
    }

    #[tokio::test]
    async fn integration_update_category_cascade() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入历史记录
        sqlx::query(
            "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        ).bind("test").bind("item1").bind(now).bind(now)
            .execute(&pool).await.unwrap();

        // 设置分类
        sqlx::query("INSERT INTO categories(category, item_id) VALUES(?1, ?2)")
            .bind("工作").bind("item1")
            .execute(&pool).await.unwrap();

        // 删除分类列表中的"工作"
        sqlx::query("DELETE FROM category_list WHERE category = '工作'")
            .execute(&pool).await.unwrap();

        // 验证：categories 表中的映射仍然存在（非级联删除）
        let cat: Option<String> = sqlx::query_scalar("SELECT category FROM categories WHERE item_id = 'item1'")
            .fetch_optional(&pool).await.unwrap();
        assert_eq!(cat, Some("工作".to_string()), "删除分类列表不应影响分类映射");
    }

    #[tokio::test]
    async fn integration_pinned_order_preserved() {
        let pool = create_test_pool().await;
        let now = now_unix_ms();

        // 插入 5 条记录
        for i in 0..5 {
            sqlx::query(
                "INSERT INTO history_items (content, item_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            ).bind(format!("item {}", i)).bind(format!("ord{}", i))
                .bind(now + i).bind(now + i)
                .execute(&pool).await.unwrap();
        }

        // 按特定顺序置顶：ord3, ord1, ord4
        let pinned_order = vec!["ord3", "ord1", "ord4"];
        for (idx, id) in pinned_order.iter().enumerate() {
            sqlx::query("INSERT INTO pinned_items(pinned_at, item_id) VALUES(?1, ?2)")
                .bind(now + 100 - idx as i64)
                .bind(id)
                .execute(&pool).await.unwrap();
        }

        // 按 pinned_at DESC 查询应该保持插入顺序
        let result: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM pinned_items ORDER BY pinned_at DESC"
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(result, vec!["ord3", "ord1", "ord4"]);

        // 重新排序：删除后按新顺序插入
        sqlx::query("DELETE FROM pinned_items").execute(&pool).await.unwrap();
        let new_order = vec!["ord4", "ord1", "ord3"];
        for (idx, id) in new_order.iter().enumerate() {
            sqlx::query("INSERT INTO pinned_items(pinned_at, item_id) VALUES(?1, ?2)")
                .bind(now + 200 - idx as i64)
                .bind(id)
                .execute(&pool).await.unwrap();
        }

        let result: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM pinned_items ORDER BY pinned_at DESC"
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(result, vec!["ord4", "ord1", "ord3"]);
    }
}
