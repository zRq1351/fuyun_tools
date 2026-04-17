use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous, SqlitePool, SqlitePoolOptions};
use sqlx::{Connection, Row, Sqlite, SqliteConnection, Transaction};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn stable_history_item_id(content: &str) -> String {
    format!("{:016x}", xxh3_64(content.as_bytes()))
}

async fn reset_temp_text_table(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
    column_name: &str,
) -> Result<(), String> {
    let create_sql = format!(
        "CREATE TEMP TABLE IF NOT EXISTS {} ({} TEXT PRIMARY KEY)",
        table_name, column_name
    );
    sqlx::query(&create_sql)
        .execute(&mut **tx)
        .await
        .map_err(|e| format!("创建临时表失败: {}", e))?;

    let clear_sql = format!("DELETE FROM {}", table_name);
    sqlx::query(&clear_sql)
        .execute(&mut **tx)
        .await
        .map_err(|e| format!("清空临时表失败: {}", e))?;

    Ok(())
}

async fn fill_temp_text_table(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
    column_name: &str,
    values: &[String],
) -> Result<(), String> {
    let insert_sql = format!(
        "INSERT OR IGNORE INTO {} ({}) VALUES (?1)",
        table_name, column_name
    );
    for value in values {
        sqlx::query(&insert_sql)
            .bind(value)
            .execute(&mut **tx)
            .await
            .map_err(|e| format!("写入临时表失败: {}", e))?;
    }
    Ok(())
}

fn db_options(db_path: &PathBuf) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_millis(1200))
}

async fn get_history_db_pool() -> Result<&'static SqlitePool, String> {
    HISTORY_DB_POOL
        .get_or_try_init(|| async {
            let db_path = get_history_db_path();
            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("创建历史数据库目录失败: {}", e))?;
            }
            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .connect_with(db_options(&db_path))
                .await
                .map_err(|e| format!("打开历史数据库池失败: {}", e))?;

            let mut conn = pool
                .acquire()
                .await
                .map_err(|e| format!("获取数据库连接失败: {}", e))?;
            ensure_history_db_schema_async(&mut conn).await?;

            Ok(pool)
        })
        .await
}

async fn open_history_db_async() -> Result<sqlx::pool::PoolConnection<sqlx::Sqlite>, String> {
    let pool = get_history_db_pool().await?;
    pool.acquire()
        .await
        .map_err(|e| format!("获取数据库连接失败: {}", e))
}

async fn ensure_history_db_schema_async(conn: &mut SqliteConnection) -> Result<(), String> {
    // 创建表结构
    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS history_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            item_id TEXT,
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS categories (
            content TEXT,
            category TEXT NOT NULL,
            item_id TEXT PRIMARY KEY
        );
        CREATE TABLE IF NOT EXISTS category_list (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            category TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS pinned_items (
            content TEXT,
            pinned_at INTEGER NOT NULL DEFAULT 0,
            item_id TEXT PRIMARY KEY
        );
        CREATE INDEX IF NOT EXISTS idx_history_items_created_at ON history_items(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_history_items_item_id ON history_items(item_id);
        CREATE INDEX IF NOT EXISTS idx_categories_category ON categories(category);
        CREATE INDEX IF NOT EXISTS idx_categories_content ON categories(content);
        CREATE INDEX IF NOT EXISTS idx_pinned_items_pinned_at ON pinned_items(pinned_at DESC);
        CREATE INDEX IF NOT EXISTS idx_pinned_items_content ON pinned_items(content);
        ",
    )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("初始化历史数据库失败: {}", e))?;

    let _ = sqlx::query("DROP INDEX IF EXISTS idx_history_items_content_hash")
        .execute(&mut *conn)
        .await;
    let _ = sqlx::query("ALTER TABLE history_items DROP COLUMN content_hash")
        .execute(&mut *conn)
        .await;

    sqlx::query(
        "UPDATE history_items
         SET created_at = CAST(strftime('%s','now') AS INTEGER) * 1000
         WHERE created_at <= 0",
    )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("初始化历史数据库失败: {}", e))?;

    sqlx::query(
        "UPDATE history_items
         SET updated_at = created_at
         WHERE updated_at <= 0",
    )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("初始化历史数据库失败: {}", e))?;

    sqlx::query(
        "
        UPDATE categories
        SET item_id = (
            SELECT hi.item_id
            FROM history_items hi
            WHERE hi.content = categories.content
            LIMIT 1
        )
        WHERE item_id IS NULL OR item_id = ''
        ",
    )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("初始化历史数据库失败: {}", e))?;

    sqlx::query(
        "
        UPDATE pinned_items
        SET item_id = (
            SELECT hi.item_id
            FROM history_items hi
            WHERE hi.content = pinned_items.content
            LIMIT 1
        )
        WHERE item_id IS NULL OR item_id = ''
        ",
    )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("初始化历史数据库失败: {}", e))?;

    // Migrate categories and pinned_items to use item_id as PRIMARY KEY
    let categories_info: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("PRAGMA table_info(categories)").fetch_all(&mut *conn).await.unwrap_or_default();
    let mut item_id_is_pk_categories = false;
    for r in categories_info {
        let name: String = r.try_get("name").unwrap_or_default();
        let pk: i32 = r.try_get("pk").unwrap_or(0);
        if name == "item_id" && pk > 0 {
            item_id_is_pk_categories = true;
        }
    }

    if !item_id_is_pk_categories {
        let _ = sqlx::query(
            "
            CREATE TABLE categories_new (
                content TEXT,
                category TEXT NOT NULL,
                item_id TEXT PRIMARY KEY
            );
            INSERT OR IGNORE INTO categories_new(content, category, item_id)
            SELECT content, category, item_id FROM categories WHERE item_id IS NOT NULL AND item_id != '';
            DROP TABLE categories;
            ALTER TABLE categories_new RENAME TO categories;
            "
        ).execute(&mut *conn).await;
    }

    let pinned_info: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("PRAGMA table_info(pinned_items)").fetch_all(&mut *conn).await.unwrap_or_default();
    let mut item_id_is_pk_pinned = false;
    for r in pinned_info {
        let name: String = r.try_get("name").unwrap_or_default();
        let pk: i32 = r.try_get("pk").unwrap_or(0);
        if name == "item_id" && pk > 0 {
            item_id_is_pk_pinned = true;
        }
    }

    if !item_id_is_pk_pinned {
        let _ = sqlx::query(
            "
            CREATE TABLE pinned_items_new (
                content TEXT,
                pinned_at INTEGER NOT NULL DEFAULT 0,
                item_id TEXT PRIMARY KEY
            );
            INSERT OR IGNORE INTO pinned_items_new(content, pinned_at, item_id)
            SELECT content, pinned_at, item_id FROM pinned_items WHERE item_id IS NOT NULL AND item_id != '';
            DROP TABLE pinned_items;
            ALTER TABLE pinned_items_new RENAME TO pinned_items;
            "
        ).execute(&mut *conn).await;
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

    let _ = sqlx::query(
        "
        INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
        SELECT id, COALESCE(item_id, ''), content
        FROM history_items
        ",
    )
        .execute(&mut *conn)
        .await;

    let _ = sqlx::query(
        "
        DELETE FROM history_items_fts
        WHERE rowid NOT IN (SELECT id FROM history_items)
        ",
    )
        .execute(&mut *conn)
        .await;
    
    // 迁移:为 pinned_items 表添加 position 列(如果不存在)
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
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
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
        .fetch_one(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let categories_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM categories")
        .fetch_one(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let category_list_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM category_list")
        .fetch_one(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let pinned_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pinned_items")
        .fetch_one(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;

    if history_count + categories_count + category_list_count + pinned_count == 0 {
        return Ok(None);
    }

    // 使用 created_at DESC 排序（最新的在前）
    let item_rows = sqlx::query("SELECT content FROM history_items ORDER BY created_at DESC, id DESC")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let items = item_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();

    // 分类表：使用 item_id 作为键
    let category_rows = sqlx::query("SELECT item_id, category FROM categories")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut categories = HashMap::new();
    for row in category_rows {
        let item_id: String = row
            .try_get(0)
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        let category: String = row
            .try_get(1)
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        categories.insert(item_id, category);
    }

    // 分类列表：使用 id 排序
    let category_rows = sqlx::query("SELECT category FROM category_list ORDER BY id ASC")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let category_list = category_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();

    // 置顶表：content 为主键，按 pinned_at 排序
    let pinned_rows = sqlx::query("SELECT content FROM pinned_items ORDER BY pinned_at DESC")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
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
    let by = sort_by.unwrap_or_else(|| "updatedAt".to_string()).to_lowercase();
    let order = sort_order
        .unwrap_or_else(|| "desc".to_string())
        .to_lowercase();
    match (by.as_str(), order.as_str()) {
        ("pinnedfirst", "asc") | ("pinned_first", "asc") =>
            "CASE WHEN p.content IS NULL THEN 1 ELSE 0 END ASC, p.pinned_at DESC, hi.created_at ASC, hi.id ASC",
        ("pinnedfirst", _) | ("pinned_first", _) =>
            "CASE WHEN p.content IS NULL THEN 1 ELSE 0 END ASC, p.pinned_at DESC, hi.created_at DESC, hi.id DESC",
        ("updatedat", "asc") | ("updated_at", "asc") => "hi.updated_at ASC, hi.id ASC",
        ("updatedat", _) | ("updated_at", _) => "hi.updated_at DESC, hi.id DESC",
        ("createdat", "asc") | ("created_at", "asc") => "hi.created_at ASC, hi.id ASC",
        ("createdat", _) | ("created_at", _) => "hi.created_at DESC, hi.id DESC",
        ("id", "asc") => "hi.id ASC",
        ("id", _) => "hi.id DESC",
        _ if order == "asc" => "hi.created_at ASC, hi.id ASC",
        _ => "hi.created_at DESC, hi.id DESC",
    }
}

fn build_fts_query(keyword: &str) -> String {
    let tokens = keyword
        .split_whitespace()
        .map(|token| token.trim().replace('"', ""))
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"*", token))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        keyword.trim().to_string()
    } else {
        tokens.join(" AND ")
    }
}

fn build_keyword_snippet(content: &str, keyword: &str) -> String {
    if content.is_empty() || keyword.trim().is_empty() {
        return content.to_string();
    }
    let content_lower = content.to_lowercase();
    let keyword_lower = keyword.to_lowercase();
    if let Some(idx) = content_lower.find(&keyword_lower) {
        let start = idx.saturating_sub(36);
        let end = (idx + keyword_lower.len() + 72).min(content.len());
        let start_adj = adjust_to_char_boundary(content, start, true);
        let end_adj = adjust_to_char_boundary(content, end, false);
        let mut snippet = content[start_adj..end_adj].to_string();
        if start_adj > 0 {
            snippet = format!("...{}", snippet);
        }
        if end_adj < content.len() {
            snippet.push_str("...");
        }
        snippet
    } else {
        let end = adjust_to_char_boundary(content, 108.min(content.len()), false);
        let mut snippet = content[..end].to_string();
        if end < content.len() {
            snippet.push_str("...");
        }
        snippet
    }
}

fn adjust_to_char_boundary(s: &str, idx: usize, backward: bool) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    if s.is_char_boundary(idx) {
        return idx;
    }
    if backward {
        let mut i = idx;
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
        i
    } else {
        let mut i = idx;
        while i < s.len() && !s.is_char_boundary(i) {
            i += 1;
        }
        i
    }
}

fn block_on_result<T>(future: impl Future<Output=Result<T, String>>) -> Result<T, String> {
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
    let fts_keyword = keyword_filter.as_ref().map(|v| build_fts_query(v));
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
            .fetch_one(&mut conn)
            .await
            .map_err(|e| format!("读取历史总数失败: {}", e))?;

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
            .fetch_all(&mut conn)
            .await
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
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
                    category: row.try_get::<String, _>(3).unwrap_or_else(|_| "未分类".to_string()),
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
            .fetch_one(&mut conn)
            .await
            .map_err(|e| format!("读取历史总数失败: {}", e))?;

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
            .fetch_all(&mut conn)
            .await
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
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
                    category: row.try_get::<String, _>(3).unwrap_or_else(|_| "未分类".to_string()),
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
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;

    sqlx::query("DELETE FROM history_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清空历史记录失败: {}", e))?;
    sqlx::query("DELETE FROM history_items_fts")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清空 FTS 索引失败: {}", e))?;
    sqlx::query("DELETE FROM categories")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清空分类失败: {}", e))?;
    sqlx::query("DELETE FROM category_list")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清空分类列表失败: {}", e))?;
    sqlx::query("DELETE FROM pinned_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清空置顶项失败: {}", e))?;

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;
    Ok(())
}

pub async fn save_history_data_snapshot_async(data: &ClipboardHistoryData) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;
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

    for (item_id, content, ts) in &history_entries {
        let updated = sqlx::query(
            "UPDATE history_items
             SET content = ?1, created_at = ?2, updated_at = ?2
             WHERE item_id = ?3",
        )
            .bind(content)
            .bind(*ts)
            .bind(item_id)
        .execute(&mut *tx)
        .await
            .map_err(|e| format!("更新历史记录失败: {}", e))?
            .rows_affected();
        if updated == 0 {
            sqlx::query(
                "INSERT INTO history_items(content, item_id, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?3)",
            )
                .bind(content)
                .bind(item_id)
                .bind(*ts)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("写入历史记录失败: {}", e))?;
        }
    }

    if desired_item_ids.is_empty() {
        sqlx::query("DELETE FROM history_items")
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("清理历史记录失败: {}", e))?;
        let _ = sqlx::query("DELETE FROM history_items_fts")
            .execute(&mut *tx)
            .await;
        sqlx::query("DELETE FROM categories")
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("清理分类失败: {}", e))?;
        sqlx::query("DELETE FROM pinned_items")
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("清理置顶项失败: {}", e))?;
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
            .map_err(|e| format!("清理历史记录失败: {}", e))?;
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
            .map_err(|e| format!("清理分类失败: {}", e))?;
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
            .map_err(|e| format!("清理置顶项失败: {}", e))?;
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

    for (item, category) in &data.categories {
        let item_id = stable_history_item_id(item);
        if !desired_item_id_set.contains(&item_id) {
            continue;
        }
        sqlx::query(
            "INSERT INTO categories(content, category, item_id) VALUES(?1, ?2, ?3)
             ON CONFLICT(item_id) DO UPDATE SET content = ?1, category = ?2",
        )
            .bind(item)
            .bind(category)
            .bind(&item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入分类失败: {}", e))?;
    }

    sqlx::query("DELETE FROM category_list")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("重建分类列表失败: {}", e))?;
    for category in &data.category_list {
        sqlx::query("INSERT INTO category_list(category) VALUES(?)")
            .bind(category)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入分类列表失败: {}", e))?;
    }

    for (idx, item) in data.pinned_items.iter().enumerate() {
        let item_id = stable_history_item_id(item);
        if !desired_item_id_set.contains(&item_id) {
            continue;
        }
        let pinned_at = now_ms - (idx as i64);
        sqlx::query(
            "INSERT INTO pinned_items(content, pinned_at, item_id) VALUES(?1, ?2, ?3)
             ON CONFLICT(item_id) DO UPDATE SET content = ?1, pinned_at = ?2",
        )
            .bind(item)
            .bind(pinned_at)
            .bind(&item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入置顶项失败: {}", e))?;
    }

    for item_id in &desired_item_ids {
        let rowid = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM history_items WHERE item_id = ? ORDER BY id DESC LIMIT 1",
        )
            .bind(item_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("读取历史记录失败: {}", e))?;
        if let Some(rowid) = rowid {
            let _ = sqlx::query(
                "INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
                 SELECT id, COALESCE(item_id, ''), content
                 FROM history_items
                 WHERE id = ?",
            )
                .bind(rowid)
                .execute(&mut *tx)
                .await;
        }
    }

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;
    Ok(())
}

/// 仅同步 history_items 的顺序与内容（高频路径优化），并清理失效关联。
pub async fn save_history_items_only_async(items: &[String]) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;
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

    for (item_id, content, ts) in &history_entries {
        let updated = sqlx::query(
            "UPDATE history_items
             SET content = ?1, created_at = ?2, updated_at = ?2
             WHERE item_id = ?3",
        )
            .bind(content)
            .bind(*ts)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("更新历史记录失败: {}", e))?
            .rows_affected();
        if updated == 0 {
            sqlx::query(
                "INSERT INTO history_items(content, item_id, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?3)",
            )
                .bind(content)
                .bind(item_id)
                .bind(*ts)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("写入历史记录失败: {}", e))?;
        }
    }

    if desired_item_ids.is_empty() {
        sqlx::query("DELETE FROM history_items")
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("清理历史记录失败: {}", e))?;
        let _ = sqlx::query("DELETE FROM history_items_fts").execute(&mut *tx).await;
        sqlx::query("DELETE FROM categories")
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("清理分类失败: {}", e))?;
        sqlx::query("DELETE FROM pinned_items")
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("清理置顶项失败: {}", e))?;
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
            .map_err(|e| format!("清理历史记录失败: {}", e))?;
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
            .map_err(|e| format!("清理分类失败: {}", e))?;
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
            .map_err(|e| format!("清理置顶项失败: {}", e))?;
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

    for item_id in &desired_item_ids {
        let rowid = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM history_items WHERE item_id = ? ORDER BY id DESC LIMIT 1",
        )
            .bind(item_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("读取历史记录失败: {}", e))?;
        if let Some(rowid) = rowid {
            let _ = sqlx::query(
                "INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
                 SELECT id, COALESCE(item_id, ''), content
                 FROM history_items
                 WHERE id = ?",
            )
                .bind(rowid)
                .execute(&mut *tx)
                .await;
        }
    }

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;
    Ok(())
}

/// 仅同步分类映射与分类列表。
pub async fn save_categories_state_async(
    categories: &HashMap<String, String>,
    category_list: &[String],
) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;

    sqlx::query("DELETE FROM categories")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清理分类失败: {}", e))?;
    sqlx::query("DELETE FROM category_list")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清理分类列表失败: {}", e))?;

    for (content, category) in categories {
        let item_id = stable_history_item_id(content);
        sqlx::query(
            "INSERT INTO categories(content, category, item_id)
             VALUES(?1, ?2, ?3)",
        )
            .bind(content)
            .bind(category)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入分类失败: {}", e))?;
    }

    for category in category_list {
        sqlx::query("INSERT INTO category_list(category) VALUES(?)")
            .bind(category)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入分类列表失败: {}", e))?;
    }

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;
    Ok(())
}

/// 仅同步置顶顺序（按 pinned_items 向量顺序重建 pinned_at）。
pub async fn save_pinned_items_order_async(pinned_items: &[String]) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;

    sqlx::query("DELETE FROM pinned_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清理置顶项失败: {}", e))?;

    let base_ts = now_unix_ms();
    for (idx, content) in pinned_items.iter().enumerate() {
        let item_id = stable_history_item_id(content);
        // 较新的 pinned_at 表示更靠前的置顶项。
        let pinned_at = base_ts + (pinned_items.len().saturating_sub(idx) as i64);
        sqlx::query(
            "INSERT INTO pinned_items(content, pinned_at, item_id)
             VALUES(?1, ?2, ?3)",
        )
            .bind(content)
            .bind(pinned_at)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入置顶项失败: {}", e))?;
    }

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;
    Ok(())
}

/// 置顶记录（增量操作）
pub async fn pin_item(content: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let now_ms = now_unix_ms();
    let item_id = stable_history_item_id(content);

    sqlx::query(
        "INSERT INTO pinned_items(content, pinned_at, item_id) VALUES(?1, ?2, ?3)
         ON CONFLICT(item_id) DO UPDATE SET pinned_at = ?2, content = ?1"
    )
        .bind(content)
        .bind(now_ms)
        .bind(&item_id)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("置顶失败: {}", e))?;

    Ok(())
}

/// 取消置顶（增量操作）
pub async fn unpin_item(content: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let item_id = stable_history_item_id(content);

    sqlx::query("DELETE FROM pinned_items WHERE item_id = ?")
        .bind(&item_id)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("取消置顶失败: {}", e))?;

    Ok(())
}

/// 设置记录分类（增量操作）
pub async fn set_item_category(item_id: &str, category: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    sqlx::query(
        "INSERT INTO categories(content, category, item_id) VALUES('', ?1, ?2)
         ON CONFLICT(item_id) DO UPDATE SET category = ?1"
    )
        .bind(category)
        .bind(item_id)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("设置分类失败: {}", e))?;

    Ok(())
}

/// 删除记录分类（增量操作）
pub async fn remove_item_category(item_id: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    sqlx::query("DELETE FROM categories WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("删除分类失败: {}", e))?;

    Ok(())
}

/// 添加分类到列表（增量操作）
pub async fn add_category_to_list(category: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    // 检查是否已存在
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM category_list WHERE category = ?)"
    )
        .bind(category)
        .fetch_one(&mut conn)
        .await
        .map_err(|e| format!("检查分类是否存在失败: {}", e))?;

    if !exists {
        sqlx::query("INSERT INTO category_list(category) VALUES(?)")
            .bind(category)
            .execute(&mut conn)
            .await
            .map_err(|e| format!("添加分类失败: {}", e))?;
    }

    Ok(())
}

/// 从列表删除分类（增量操作）
pub async fn remove_category_from_list(category: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;

    sqlx::query("DELETE FROM category_list WHERE category = ?")
        .bind(category)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("删除分类失败: {}", e))?;

    Ok(())
}

/// 从映射表与分类列表同时删除指定分类（增量操作）
pub async fn remove_category_everywhere(category: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("开启事务失败: {}", e))?;

    sqlx::query("DELETE FROM categories WHERE category = ?")
        .bind(category)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("删除分类映射失败: {}", e))?;

    sqlx::query("DELETE FROM category_list WHERE category = ?")
        .bind(category)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("删除分类失败: {}", e))?;

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;
    Ok(())
}

/// 根据内容批量删除历史记录（增量操作）
pub async fn delete_history_items_bulk(contents: &[String]) -> Result<(), String> {
    if contents.is_empty() {
        return Ok(());
    }
    let mut conn = open_history_db_async().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("开启事务失败: {}", e))?;

    reset_temp_text_table(&mut tx, "temp_delete_history_contents", "content").await?;
    fill_temp_text_table(
        &mut tx,
        "temp_delete_history_contents",
        "content",
        contents,
    )
        .await?;

    let _ = sqlx::query(
        "
        DELETE FROM history_items_fts
        WHERE EXISTS (
            SELECT 1
            FROM history_items hi
            JOIN temp_delete_history_contents target
              ON target.content = hi.content
            WHERE hi.item_id = history_items_fts.item_id
        )
        ",
    )
        .execute(&mut *tx)
        .await;
    sqlx::query(
        "
        DELETE FROM categories
        WHERE EXISTS (
            SELECT 1
            FROM history_items hi
            JOIN temp_delete_history_contents target
              ON target.content = hi.content
            WHERE hi.item_id = categories.item_id
        )
        ",
    )
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("删除分类关联失败: {}", e))?;
    sqlx::query(
        "
        DELETE FROM pinned_items
        WHERE EXISTS (
            SELECT 1
            FROM history_items hi
            JOIN temp_delete_history_contents target
              ON target.content = hi.content
            WHERE hi.item_id = pinned_items.item_id
        )
        ",
    )
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("删除置顶关联失败: {}", e))?;
    sqlx::query(
        "
        DELETE FROM history_items
        WHERE EXISTS (
            SELECT 1
            FROM temp_delete_history_contents target
            WHERE target.content = history_items.content
        )
        ",
    )
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("删除历史记录失败: {}", e))?;

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;
    Ok(())
}
pub async fn delete_history_item_by_content(content: &str) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("开启事务失败: {}", e))?;

    reset_temp_text_table(&mut tx, "temp_delete_history_contents", "content").await?;
    fill_temp_text_table(
        &mut tx,
        "temp_delete_history_contents",
        "content",
        &[content.to_string()],
    )
        .await?;

    let _ = sqlx::query(
        "
        DELETE FROM history_items_fts
        WHERE EXISTS (
            SELECT 1
            FROM history_items hi
            JOIN temp_delete_history_contents target
              ON target.content = hi.content
            WHERE hi.item_id = history_items_fts.item_id
        )
        ",
    )
        .execute(&mut *tx)
        .await;
    sqlx::query(
        "
        DELETE FROM categories
        WHERE EXISTS (
            SELECT 1
            FROM history_items hi
            JOIN temp_delete_history_contents target
              ON target.content = hi.content
            WHERE hi.item_id = categories.item_id
        )
        ",
    )
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("删除分类关联失败: {}", e))?;
    sqlx::query(
        "
        DELETE FROM pinned_items
        WHERE EXISTS (
            SELECT 1
            FROM history_items hi
            JOIN temp_delete_history_contents target
              ON target.content = hi.content
            WHERE hi.item_id = pinned_items.item_id
        )
        ",
    )
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("删除置顶关联失败: {}", e))?;
    sqlx::query(
        "
        DELETE FROM history_items
        WHERE EXISTS (
            SELECT 1
            FROM temp_delete_history_contents target
            WHERE target.content = history_items.content
        )
        ",
    )
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("删除历史记录失败: {}", e))?;

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;

    Ok(())
}

/// 合并历史数据:保留现有记录,只添加备份中不存在的新记录
pub async fn merge_history_data_async(data: &ClipboardHistoryData) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;
    let now_ms = now_unix_ms();
    
    // 获取当前已存在的 item_id 集合
    let existing_ids: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT DISTINCT item_id FROM history_items WHERE item_id IS NOT NULL AND item_id != ''")
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("读取现有记录失败: {}", e))?
        .into_iter()
        .collect();
    
    // 只插入不存在的记录
    let mut new_count = 0;
    for (idx, item) in data.items.iter().enumerate().rev() {
        let item_id = stable_history_item_id(item);
        if existing_ids.contains(&item_id) {
            continue; // 跳过已存在的记录
        }
        
        let ts = now_ms - (idx as i64);
        sqlx::query(
            "INSERT INTO history_items(content, item_id, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?3)",
        )
            .bind(item)
            .bind(&item_id)
            .bind(ts)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入新历史记录失败: {}", e))?;
        new_count += 1;
    }
    
    log::info!("合并文本历史: 新增 {} 条记录", new_count);
    
    // 合并分类信息(只添加新的)
    for (item, category) in &data.categories {
        let item_id = stable_history_item_id(item);
        if existing_ids.contains(&item_id) {
            // 如果记录已存在,更新分类
            sqlx::query(
                "INSERT INTO categories(content, category, item_id) VALUES(?1, ?2, ?3)
                 ON CONFLICT(item_id) DO UPDATE SET content = ?1, category = ?2",
            )
                .bind(item)
                .bind(category)
                .bind(&item_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("更新分类失败: {}", e))?;
        } else {
            // 如果记录不存在,也添加分类(以备后续插入记录时使用)
            sqlx::query(
                "INSERT OR IGNORE INTO categories(content, category, item_id) VALUES(?1, ?2, ?3)",
            )
                .bind(item)
                .bind(category)
                .bind(&item_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("写入分类失败: {}", e))?;
        }
    }
    
    // 合并且分类列表(只添加不存在的)
    let existing_categories: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT category FROM category_list")
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("读取分类列表失败: {}", e))?
        .into_iter()
        .collect();
    
    for category in &data.category_list {
        if !existing_categories.contains(category) {
            sqlx::query("INSERT INTO category_list(category) VALUES(?)")
                .bind(category)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("写入分类列表失败: {}", e))?;
        }
    }
    
    // 合并且置顶项(追加到末尾)
    let existing_pinned: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT DISTINCT item_id FROM pinned_items WHERE item_id IS NOT NULL AND item_id != ''")
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("读取置顶项失败: {}", e))?
        .into_iter()
        .collect();
    
    // 检测 pinned_items 表是否有 position 列
    let has_position = sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM pinned_items")
        .fetch_one(&mut *tx)
        .await
        .is_ok();
    
    if has_position {
        // 有 position 列,使用位置排序
        let current_max_position = sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM pinned_items")
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("读取最大位置失败: {}", e))?
            .unwrap_or(-1);
        
        let mut position = current_max_position + 1;
        for (idx, item) in data.pinned_items.iter().enumerate() {
            let item_id = stable_history_item_id(item);
            if existing_pinned.contains(&item_id) {
                continue; // 跳过已置顶的
            }
            
            let pinned_at = now_ms - (idx as i64);
            sqlx::query(
                "INSERT INTO pinned_items(content, pinned_at, item_id, position) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(item_id) DO NOTHING",
            )
                .bind(item)
                .bind(pinned_at)
                .bind(&item_id)
                .bind(position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("写入置顶项失败: {}", e))?;
            position += 1;
        }
    } else {
        // 没有 position 列,简单插入
        for (idx, item) in data.pinned_items.iter().enumerate() {
            let item_id = stable_history_item_id(item);
            if existing_pinned.contains(&item_id) {
                continue; // 跳过已置顶的
            }
            
            let pinned_at = now_ms - (idx as i64);
            sqlx::query(
                "INSERT OR IGNORE INTO pinned_items(content, pinned_at, item_id) VALUES(?1, ?2, ?3)",
            )
                .bind(item)
                .bind(pinned_at)
                .bind(&item_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("写入置顶项失败: {}", e))?;
        }
    }
    
    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))?;
    Ok(())
}
