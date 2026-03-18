use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{Connection, Row, SqliteConnection};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn stable_history_content_hash(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn db_options(db_path: &PathBuf) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_millis(1200))
}

async fn open_history_db_async() -> Result<SqliteConnection, String> {
    let db_path = get_history_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建历史数据库目录失败: {}", e))?;
    }
    let mut conn = SqliteConnection::connect_with(&db_options(&db_path))
        .await
        .map_err(|e| format!("打开历史数据库失败: {}", e))?;
    ensure_history_db_schema_async(&mut conn).await?;
    Ok(conn)
}

async fn ensure_history_db_schema_async(conn: &mut SqliteConnection) -> Result<(), String> {
    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS history_items (
            position INTEGER PRIMARY KEY,
            content TEXT NOT NULL,
            item_id TEXT,
            content_hash TEXT,
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS categories (
            item TEXT PRIMARY KEY,
            category TEXT NOT NULL,
            item_id TEXT
        );
        CREATE TABLE IF NOT EXISTS category_list (
            position INTEGER PRIMARY KEY,
            category TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS pinned_items (
            position INTEGER PRIMARY KEY,
            item TEXT NOT NULL,
            item_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_history_items_position ON history_items(position);
        CREATE INDEX IF NOT EXISTS idx_history_items_item_id ON history_items(item_id);
        CREATE INDEX IF NOT EXISTS idx_history_items_updated_at ON history_items(updated_at);
        CREATE INDEX IF NOT EXISTS idx_categories_category ON categories(category);
        CREATE INDEX IF NOT EXISTS idx_categories_item_id ON categories(item_id);
        CREATE INDEX IF NOT EXISTS idx_pinned_items_item_id ON pinned_items(item_id);
        ",
    )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("初始化历史数据库失败: {}", e))?;

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
            WHERE hi.content = categories.item
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
            WHERE hi.content = pinned_items.item
            LIMIT 1
        )
        WHERE item_id IS NULL OR item_id = ''
        ",
    )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("初始化历史数据库失败: {}", e))?;

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
        SELECT position + 1, COALESCE(item_id, ''), content
        FROM history_items
        ",
    )
        .execute(&mut *conn)
        .await;

    let _ = sqlx::query(
        "
        DELETE FROM history_items_fts
        WHERE rowid NOT IN (SELECT position + 1 FROM history_items)
        ",
    )
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

    let item_rows = sqlx::query("SELECT content FROM history_items ORDER BY position ASC")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let items = item_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();

    let category_rows = sqlx::query("SELECT item, category FROM categories")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut categories = HashMap::new();
    for row in category_rows {
        let item: String = row
            .try_get(0)
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        let category: String = row
            .try_get(1)
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        categories.insert(item, category);
    }

    let category_rows = sqlx::query("SELECT category FROM category_list ORDER BY position ASC")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let category_list = category_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();

    let pinned_rows = sqlx::query("SELECT item FROM pinned_items ORDER BY position ASC")
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

async fn save_history_data_to_sqlite_async(data: &ClipboardHistoryData) -> Result<(), String> {
    let mut conn = open_history_db_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| format!("创建历史数据库事务失败: {}", e))?;

    sqlx::query("DELETE FROM history_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    sqlx::query("DELETE FROM categories")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    sqlx::query("DELETE FROM category_list")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    sqlx::query("DELETE FROM pinned_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("写入历史数据库失败: {}", e))?;

    let fts_enabled = {
        let row = sqlx::query(
            "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'history_items_fts'",
        )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        row.try_get::<i64, _>("count").unwrap_or(0) > 0
    };
    if fts_enabled {
        let _ = sqlx::query("DELETE FROM history_items_fts").execute(&mut *tx).await;
    }

    let now_ms = now_unix_ms();
    let mut item_id_by_content = HashMap::<String, String>::new();
    for (idx, content) in data.items.iter().enumerate() {
        let item_id = stable_history_item_id(content);
        item_id_by_content.insert(content.clone(), item_id.clone());
        sqlx::query(
            "
            INSERT INTO history_items(position, content, item_id, content_hash, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6)
            ",
        )
            .bind(idx as i64)
            .bind(content)
            .bind(&item_id)
            .bind(stable_history_content_hash(content))
            .bind(now_ms)
            .bind(now_ms)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入历史数据库失败: {}", e))?;

        if fts_enabled {
            let _ = sqlx::query(
                "INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content) VALUES(?1, ?2, ?3)",
            )
                .bind(idx as i64 + 1)
                .bind(&item_id)
                .bind(content)
                .execute(&mut *tx)
                .await;
        }
    }

    for (item, category) in &data.categories {
        let item_id = item_id_by_content
            .get(item)
            .cloned()
            .unwrap_or_else(|| stable_history_item_id(item));
        sqlx::query("INSERT OR REPLACE INTO categories(item, category, item_id) VALUES(?1, ?2, ?3)")
            .bind(item)
            .bind(category)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    }

    for (idx, category) in data.category_list.iter().enumerate() {
        sqlx::query("INSERT INTO category_list(position, category) VALUES(?1, ?2)")
            .bind(idx as i64)
            .bind(category)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    }

    for (idx, item) in data.pinned_items.iter().enumerate() {
        let item_id = item_id_by_content
            .get(item)
            .cloned()
            .unwrap_or_else(|| stable_history_item_id(item));
        sqlx::query("INSERT INTO pinned_items(position, item, item_id) VALUES(?1, ?2, ?3)")
            .bind(idx as i64)
            .bind(item)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    }

    tx.commit()
        .await
        .map_err(|e| format!("提交历史数据库事务失败: {}", e))
}

fn resolve_history_sort(sort_by: Option<String>, sort_order: Option<String>) -> &'static str {
    let by = sort_by.unwrap_or_else(|| "updatedAt".to_string()).to_lowercase();
    let order = sort_order
        .unwrap_or_else(|| "desc".to_string())
        .to_lowercase();
    match (by.as_str(), order.as_str()) {
        ("updatedat", "asc") | ("updated_at", "asc") => "hi.updated_at ASC, hi.position ASC",
        ("updatedat", _) | ("updated_at", _) => "hi.updated_at DESC, hi.position DESC",
        ("position", "asc") => "hi.position ASC",
        ("position", _) => "hi.position DESC",
        _ if order == "asc" => "hi.updated_at ASC, hi.position ASC",
        _ => "hi.updated_at DESC, hi.position DESC",
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

pub fn save_history(history: &[String]) -> Result<(), String> {
    let history_data = ClipboardHistoryData {
        items: history.to_vec(),
        categories: HashMap::new(),
        category_list: Vec::new(),
        pinned_items: Vec::new(),
    };
    save_history_data_with_retry(&history_data, 3)
}

pub async fn save_history_async(history: &[String]) -> Result<(), String> {
    let history_data = ClipboardHistoryData {
        items: history.to_vec(),
        categories: HashMap::new(),
        category_list: Vec::new(),
        pinned_items: Vec::new(),
    };
    save_history_data_with_retry_async(&history_data, 3).await
}

pub fn save_history_with_retry(history: &[String], max_retries: u32) -> Result<(), String> {
    save_history_data_with_retry(
        &ClipboardHistoryData {
            items: history.to_vec(),
            categories: HashMap::new(),
            category_list: Vec::new(),
            pinned_items: Vec::new(),
        },
        max_retries,
    )
}

pub async fn save_history_with_retry_async(history: &[String], max_retries: u32) -> Result<(), String> {
    save_history_data_with_retry_async(
        &ClipboardHistoryData {
            items: history.to_vec(),
            categories: HashMap::new(),
            category_list: Vec::new(),
            pinned_items: Vec::new(),
        },
        max_retries,
    )
        .await
}

pub fn save_history_data_with_retry(
    data: &ClipboardHistoryData,
    max_retries: u32,
) -> Result<(), String> {
    for i in 0..max_retries {
        match block_on_result(save_history_data_to_sqlite_async(data)) {
            Ok(_) => return Ok(()),
            Err(e) => {
                if i == max_retries - 1 {
                    return Err(e);
                }
                log::warn!("写入历史数据库失败 (重试 {}/{}): {}", i + 1, max_retries, e);
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Ok(())
}

pub async fn save_history_data_with_retry_async(
    data: &ClipboardHistoryData,
    max_retries: u32,
) -> Result<(), String> {
    for i in 0..max_retries {
        match save_history_data_to_sqlite_async(data).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                if i == max_retries - 1 {
                    return Err(e);
                }
                log::warn!("写入历史数据库失败 (重试 {}/{}): {}", i + 1, max_retries, e);
                sqlx::__rt::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    Ok(())
}

pub fn load_history() -> Result<Vec<String>, String> {
    load_history_data().map(|data| data.items)
}

pub async fn load_history_async() -> Result<Vec<String>, String> {
    load_history_data_async().await.map(|data| data.items)
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

pub fn load_history_page_data(
    offset: usize,
    limit: usize,
    category: Option<String>,
    pinned_only: bool,
    keyword: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
) -> Result<ClipboardHistoryPageData, String> {
    block_on_result(load_history_page_data_async(
        offset,
        limit,
        category,
        pinned_only,
        keyword,
        sort_by,
        sort_order,
    ))
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

    let (total, mut items) = if fts_enabled {
        let total_sql = "
            SELECT COUNT(*)
            FROM history_items hi
            LEFT JOIN categories c ON (c.item_id = hi.item_id OR c.item = hi.content)
            LEFT JOIN pinned_items p ON (p.item_id = hi.item_id OR p.item = hi.content)
            WHERE
              (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
              AND (?2 = 0 OR p.item IS NOT NULL)
              AND (
                ?3 IS NULL
                OR EXISTS (
                    SELECT 1 FROM history_items_fts
                    WHERE history_items_fts.rowid = hi.position + 1
                      AND history_items_fts MATCH ?3
                )
              )
            ";
        let total = sqlx::query_scalar::<_, i64>(total_sql)
            .bind(category_filter.as_deref())
            .bind(pinned_flag)
            .bind(fts_keyword.as_deref())
            .fetch_one(&mut conn)
            .await
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;

        let query_sql = format!(
            "
            SELECT
              hi.position,
              COALESCE(hi.item_id, ''),
              hi.content,
              CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END,
              CASE WHEN p.item IS NULL THEN 0 ELSE 1 END,
              COALESCE(hi.updated_at, 0)
            FROM history_items hi
            LEFT JOIN categories c ON (c.item_id = hi.item_id OR c.item = hi.content)
            LEFT JOIN pinned_items p ON (p.item_id = hi.item_id OR p.item = hi.content)
            WHERE
              (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
              AND (?2 = 0 OR p.item IS NOT NULL)
              AND (
                ?3 IS NULL
                OR EXISTS (
                    SELECT 1 FROM history_items_fts
                    WHERE history_items_fts.rowid = hi.position + 1
                      AND history_items_fts MATCH ?3
                )
              )
            ORDER BY {}
            LIMIT ?4 OFFSET ?5
            ",
            order_clause
        );
        let rows = sqlx::query(&query_sql)
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
                    position: row.try_get::<i64, _>(0).unwrap_or(0) as usize,
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
        let total_sql = "
            SELECT COUNT(*)
            FROM history_items hi
            LEFT JOIN categories c ON (c.item_id = hi.item_id OR c.item = hi.content)
            LEFT JOIN pinned_items p ON (p.item_id = hi.item_id OR p.item = hi.content)
            WHERE
              (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
              AND (?2 = 0 OR p.item IS NOT NULL)
              AND (?3 IS NULL OR hi.content LIKE '%' || ?3 || '%')
            ";
        let total = sqlx::query_scalar::<_, i64>(total_sql)
            .bind(category_filter.as_deref())
            .bind(pinned_flag)
            .bind(keyword_filter.as_deref())
            .fetch_one(&mut conn)
            .await
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;

        let query_sql = format!(
            "
            SELECT
              hi.position,
              COALESCE(hi.item_id, ''),
              hi.content,
              CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END,
              CASE WHEN p.item IS NULL THEN 0 ELSE 1 END,
              COALESCE(hi.updated_at, 0)
            FROM history_items hi
            LEFT JOIN categories c ON (c.item_id = hi.item_id OR c.item = hi.content)
            LEFT JOIN pinned_items p ON (p.item_id = hi.item_id OR p.item = hi.content)
            WHERE
              (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
              AND (?2 = 0 OR p.item IS NOT NULL)
              AND (?3 IS NULL OR hi.content LIKE '%' || ?3 || '%')
            ORDER BY {}
            LIMIT ?4 OFFSET ?5
            ",
            order_clause
        );
        let rows = sqlx::query(&query_sql)
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
                    position: row.try_get::<i64, _>(0).unwrap_or(0) as usize,
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
        total: total as usize,
        offset,
        limit: effective_limit,
        items,
    })
}
