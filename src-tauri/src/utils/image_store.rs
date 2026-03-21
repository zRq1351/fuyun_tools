use crate::utils::image_clipboard::{
    ImageHistoryData, ImageHistoryItem, ImageHistoryPageData, ImageHistoryPageItem,
};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqliteSynchronous};
use sqlx::{Connection, Row, SqliteConnection};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;

/// 全局数据库连接池
static DB_POOL: OnceCell<Arc<SqlitePool>> = OnceCell::const_new();

fn get_image_store_db_path() -> PathBuf {
    let mut db_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    db_path.pop();
    db_path.push("image_history.db");
    db_path
}

fn image_store_options(db_path: &PathBuf) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_millis(1200))
}

async fn open_image_store_async() -> Result<SqliteConnection, String> {
    let db_path = get_image_store_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建图片历史数据库目录失败: {}", e))?;
    }
    let mut conn = SqliteConnection::connect_with(&image_store_options(&db_path))
        .await
        .map_err(|e| format!("打开图片历史数据库失败: {}", e))?;
    init_image_store_schema_async(&mut conn).await?;
    Ok(conn)
}

async fn exec(conn: &mut SqliteConnection, sql: &str) -> Result<(), String> {
    sqlx::query(sql)
        .execute(conn)
        .await
        .map_err(|e| format!("初始化图片历史数据库失败: {}", e))?;
    Ok(())
}

async fn init_image_store_schema_async(conn: &mut SqliteConnection) -> Result<(), String> {
    exec(
        conn,
        "
        CREATE TABLE IF NOT EXISTS image_items (
            position INTEGER NOT NULL,
            item_id TEXT PRIMARY KEY,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            preview_width INTEGER NOT NULL,
            preview_height INTEGER NOT NULL,
            preview_rgba_base64 TEXT NOT NULL,
            image_path TEXT NOT NULL
        )
        ",
    )
        .await?;
    exec(
        conn,
        "
        CREATE TABLE IF NOT EXISTS image_async_previews (
            item_id TEXT PRIMARY KEY,
            preview_width INTEGER NOT NULL,
            preview_height INTEGER NOT NULL,
            preview_base64 TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )
        ",
    )
        .await?;
    exec(
        conn,
        "CREATE INDEX IF NOT EXISTS idx_image_items_position ON image_items(position)",
    )
        .await?;
    exec(
        conn,
        "
        CREATE TABLE IF NOT EXISTS image_categories (
            item_id TEXT PRIMARY KEY,
            category TEXT NOT NULL
        )
        ",
    )
        .await?;
    exec(
        conn,
        "
        CREATE TABLE IF NOT EXISTS image_category_list (
            position INTEGER NOT NULL,
            category TEXT NOT NULL
        )
        ",
    )
        .await?;
    exec(
        conn,
        "CREATE INDEX IF NOT EXISTS idx_image_category_list_position ON image_category_list(position)",
    )
        .await?;
    exec(
        conn,
        "
        CREATE TABLE IF NOT EXISTS image_tags (
            item_id TEXT NOT NULL,
            tag TEXT NOT NULL,
            position INTEGER NOT NULL,
            PRIMARY KEY (item_id, tag)
        )
        ",
    )
        .await?;
    exec(
        conn,
        "CREATE INDEX IF NOT EXISTS idx_image_tags_item_position ON image_tags(item_id, position)",
    )
        .await?;
    exec(
        conn,
        "
        CREATE TABLE IF NOT EXISTS image_pinned (
            item_id TEXT PRIMARY KEY,
            position INTEGER NOT NULL
        )
        ",
    )
        .await?;
    exec(
        conn,
        "CREATE INDEX IF NOT EXISTS idx_image_pinned_position ON image_pinned(position)",
    )
        .await?;
    Ok(())
}

/// 获取或初始化数据库连接池
async fn get_pool() -> Result<Arc<SqlitePool>, String> {
    if let Some(pool) = DB_POOL.get() {
        return Ok(pool.clone());
    }
    
    let db_path = get_image_store_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建图片历史数据库目录失败: {}", e))?;
    }
    
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_millis(1200))
    )
    .await
    .map_err(|e| format!("创建数据库连接池失败: {}", e))?;
    
    let pool_arc = Arc::new(pool);
    
    // 初始化表结构
    let mut conn = pool_arc.acquire().await.map_err(|e| format!("获取连接失败: {}", e))?;
    init_image_store_schema_async(&mut conn).await?;
    
    DB_POOL.set(pool_arc.clone()).map_err(|_| "连接池已初始化".to_string())?;
    Ok(pool_arc)
}

fn block_on_result<T>(future: impl Future<Output=Result<T, String>>) -> Result<T, String> {
    tauri::async_runtime::block_on(future)
}

pub fn init_image_store() -> Result<(), String> {
    let _ = block_on_result(get_pool())?;
    Ok(())
}

pub fn upsert_item(item: &ImageHistoryItem, position: usize) -> Result<(), String> {
    block_on_result(upsert_item_async(item, position))
}

pub async fn upsert_item_async(item: &ImageHistoryItem, position: usize) -> Result<(), String> {
    let pool = get_pool().await?;
    sqlx::query(
        "
        INSERT INTO image_items (
            position, item_id, width, height, preview_width, preview_height, preview_rgba_base64, image_path
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(item_id) DO UPDATE SET
            position = excluded.position,
            width = excluded.width,
            height = excluded.height,
            preview_width = excluded.preview_width,
            preview_height = excluded.preview_height,
            preview_rgba_base64 = excluded.preview_rgba_base64,
            image_path = excluded.image_path
        ",
    )
        .bind(position as i64)
        .bind(&item.id)
        .bind(item.width as i64)
        .bind(item.height as i64)
        .bind(item.preview_width as i64)
        .bind(item.preview_height as i64)
        .bind(&item.preview_rgba_base64)
        .bind(&item.image_path)
        .execute(&**pool)
        .await
    .map_err(|e| format!("写入图片历史数据库失败: {}", e))?;
    Ok(())
}

pub fn delete_item(item_id: &str) -> Result<(), String> {
    block_on_result(delete_item_async(item_id))
}

pub async fn delete_item_async(item_id: &str) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    sqlx::query("DELETE FROM image_items WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("删除图片历史数据库条目失败: {}", e))?;
    
    // 同时删除异步预览
    delete_async_preview_async(item_id).await?;
    
    Ok(())
}

pub fn sync_item_positions(item_ids: &[String]) -> Result<(), String> {
    block_on_result(sync_item_positions_async(item_ids))
}

pub async fn sync_item_positions_async(item_ids: &[String]) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| format!("创建图片位置事务失败: {}", e))?;
    for (position, item_id) in item_ids.iter().enumerate() {
        sqlx::query("UPDATE image_items SET position = ?1 WHERE item_id = ?2")
            .bind(position as i64)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("更新图片位置失败: {}", e))?;
    }
    tx.commit()
        .await
        .map_err(|e| format!("提交图片位置事务失败: {}", e))
}

/// 增量更新图片位置 - 只更新受影响的图片
pub fn sync_item_positions_incremental(
    item_id: &str,
    old_position: usize,
    new_position: usize,
) -> Result<(), String> {
    block_on_result(sync_item_positions_incremental_async(item_id, old_position, new_position))
}

/// 异步增量更新图片位置 - 只更新受影响的图片
pub async fn sync_item_positions_incremental_async(
    item_id: &str,
    old_position: usize,
    new_position: usize,
) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| format!("创建增量更新事务失败: {}", e))?;

    // 更新移动的图片位置
    sqlx::query("UPDATE image_items SET position = ?1 WHERE item_id = ?2")
        .bind(new_position as i64)
        .bind(item_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("更新移动图片位置失败: {}", e))?;

    // 更新其他受影响图片的位置
    if old_position < new_position {
        // 图片向后移动，前面的图片位置减1
        sqlx::query("UPDATE image_items SET position = position - 1 WHERE position > ?1 AND position <= ?2")
            .bind(old_position as i64)
            .bind(new_position as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("更新受影响图片位置失败: {}", e))?;
    } else if old_position > new_position {
        // 图片向前移动，后面的图片位置加1
        sqlx::query("UPDATE image_items SET position = position + 1 WHERE position >= ?1 AND position < ?2")
            .bind(new_position as i64)
            .bind(old_position as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("更新受影响图片位置失败: {}", e))?;
    }

    tx.commit()
        .await
        .map_err(|e| format!("提交增量更新事务失败: {}", e))
}

pub fn upsert_category(item_id: &str, category: &str) -> Result<(), String> {
    block_on_result(upsert_category_async(item_id, category))
}

pub async fn upsert_category_async(item_id: &str, category: &str) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    sqlx::query(
        "
        INSERT INTO image_categories (item_id, category)
        VALUES (?1, ?2)
        ON CONFLICT(item_id) DO UPDATE SET category = excluded.category
        ",
    )
        .bind(item_id)
        .bind(category)
        .execute(&mut conn)
        .await
    .map_err(|e| format!("写入图片分类数据库失败: {}", e))?;
    Ok(())
}

pub fn delete_category(item_id: &str) -> Result<(), String> {
    block_on_result(delete_category_async(item_id))
}

pub async fn delete_category_async(item_id: &str) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    sqlx::query("DELETE FROM image_categories WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("删除图片分类数据库失败: {}", e))?;
    Ok(())
}

pub fn sync_tags_for_item(item_id: &str, tags: &[String]) -> Result<(), String> {
    block_on_result(sync_tags_for_item_async(item_id, tags))
}

pub async fn sync_tags_for_item_async(item_id: &str, tags: &[String]) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| format!("创建图片标签事务失败: {}", e))?;
    sqlx::query("DELETE FROM image_tags WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清理图片标签失败: {}", e))?;
    for (position, tag) in tags.iter().enumerate() {
        sqlx::query("INSERT INTO image_tags (item_id, tag, position) VALUES (?1, ?2, ?3)")
            .bind(item_id)
            .bind(tag)
            .bind(position as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入图片标签失败: {}", e))?;
    }
    tx.commit()
        .await
        .map_err(|e| format!("提交图片标签事务失败: {}", e))
}

pub fn delete_tags_for_item(item_id: &str) -> Result<(), String> {
    block_on_result(delete_tags_for_item_async(item_id))
}

pub async fn delete_tags_for_item_async(item_id: &str) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    sqlx::query("DELETE FROM image_tags WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("删除图片标签失败: {}", e))?;
    Ok(())
}

pub fn sync_category_list_order(categories: &[String]) -> Result<(), String> {
    block_on_result(sync_category_list_order_async(categories))
}

pub async fn sync_category_list_order_async(categories: &[String]) -> Result<(), String> {
    {
        let mut conn = open_image_store_async().await?;
        let mut tx = conn
            .begin()
            .await
            .map_err(|e| format!("创建分类列表事务失败: {}", e))?;
        let rows = sqlx::query("SELECT category, position FROM image_category_list")
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("读取分类列表失败: {}", e))?;
        let mut existing = HashMap::<String, i64>::new();
        for row in rows {
            let category: String = row.try_get(0).map_err(|e| format!("读取分类列表失败: {}", e))?;
            let position: i64 = row.try_get(1).map_err(|e| format!("读取分类列表失败: {}", e))?;
            existing.insert(category, position);
        }
        for (position, category) in categories.iter().enumerate() {
            if existing.get(category) == Some(&(position as i64)) {
                continue;
            }
            let affected = sqlx::query("UPDATE image_category_list SET position = ?1 WHERE category = ?2")
                .bind(position as i64)
                .bind(category)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("更新分类列表失败: {}", e))?
                .rows_affected();
            if affected == 0 {
                sqlx::query("INSERT INTO image_category_list (position, category) VALUES (?1, ?2)")
                    .bind(position as i64)
                    .bind(category)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("写入分类列表失败: {}", e))?;
            }
        }
        let desired = categories.iter().cloned().collect::<HashSet<_>>();
        for existing_category in existing.keys() {
            if desired.contains(existing_category) {
                continue;
            }
            sqlx::query("DELETE FROM image_category_list WHERE category = ?1")
                .bind(existing_category)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("清理分类列表失败: {}", e))?;
        }
        tx.commit()
            .await
            .map_err(|e| format!("提交分类列表事务失败: {}", e))
    }
}

pub fn sync_pinned_order(pinned_items: &[String]) -> Result<(), String> {
    block_on_result(sync_pinned_order_async(pinned_items))
}

pub async fn sync_pinned_order_async(pinned_items: &[String]) -> Result<(), String> {
    {
        let mut conn = open_image_store_async().await?;
        let mut tx = conn
            .begin()
            .await
            .map_err(|e| format!("创建置顶事务失败: {}", e))?;
        let rows = sqlx::query("SELECT item_id, position FROM image_pinned")
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("读取置顶失败: {}", e))?;
        let mut existing = HashMap::<String, i64>::new();
        for row in rows {
            let item_id: String = row.try_get(0).map_err(|e| format!("读取置顶失败: {}", e))?;
            let position: i64 = row.try_get(1).map_err(|e| format!("读取置顶失败: {}", e))?;
            existing.insert(item_id, position);
        }
        for (position, item_id) in pinned_items.iter().enumerate() {
            if existing.get(item_id) == Some(&(position as i64)) {
                continue;
            }
            sqlx::query(
                "
                INSERT INTO image_pinned (item_id, position)
                VALUES (?1, ?2)
                ON CONFLICT(item_id) DO UPDATE SET position = excluded.position
                ",
            )
                .bind(item_id)
                .bind(position as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("写入置顶失败: {}", e))?;
        }
        let desired = pinned_items.iter().cloned().collect::<HashSet<_>>();
        for existing_item in existing.keys() {
            if desired.contains(existing_item) {
                continue;
            }
            sqlx::query("DELETE FROM image_pinned WHERE item_id = ?1")
                .bind(existing_item)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("清理置顶失败: {}", e))?;
        }
        tx.commit()
            .await
            .map_err(|e| format!("提交置顶事务失败: {}", e))
    }
}

pub fn delete_categories_by_category(category: &str) -> Result<(), String> {
    block_on_result(delete_categories_by_category_async(category))
}

pub async fn delete_categories_by_category_async(category: &str) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    sqlx::query("DELETE FROM image_categories WHERE category = ?1")
        .bind(category)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("按分类删除条目失败: {}", e))?;
    Ok(())
}

pub fn load_all_data() -> Result<ImageHistoryData, String> {
    block_on_result(async {
        let mut conn = open_image_store_async().await?;
        let item_rows = sqlx::query(
            "
            SELECT
              hi.item_id,
              hi.width,
              hi.height,
              hi.preview_width,
              hi.preview_height,
              hi.preview_rgba_base64,
              hi.image_path
            FROM image_items hi
            ORDER BY hi.position ASC
            ",
        )
            .fetch_all(&mut conn)
            .await
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        let mut items = Vec::new();
        for row in item_rows {
            let id: String = row.try_get(0).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let width: i64 = row.try_get(1).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let height: i64 = row.try_get(2).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let preview_width: i64 =
                row.try_get(3).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let preview_height: i64 =
                row.try_get(4).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let preview_rgba_base64: String =
                row.try_get(5).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let image_path: String = row.try_get(6).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            items.push(ImageHistoryItem {
                id: id.clone(),
                width: width.max(0) as u32,
                height: height.max(0) as u32,
                preview_width: preview_width.max(0) as u32,
                preview_height: preview_height.max(0) as u32,
                preview_rgba_base64,
                image_path,
                rgba_bytes: Vec::new(),
                signature: id,
                lazy_load: true,
                cached_signature: None,
            });
        }

        let mut categories = HashMap::new();
        let category_rows = sqlx::query("SELECT item_id, category FROM image_categories")
            .fetch_all(&mut conn)
            .await
            .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        for row in category_rows {
            let item_id: String = row.try_get(0).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let category: String = row.try_get(1).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            categories.insert(item_id, category);
        }

        let mut image_tags: HashMap<String, Vec<String>> = HashMap::new();
        let tag_rows = sqlx::query("SELECT item_id, tag FROM image_tags ORDER BY item_id, position ASC")
            .fetch_all(&mut conn)
            .await
            .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        for row in tag_rows {
            let item_id: String = row.try_get(0).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let tag: String = row.try_get(1).map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            image_tags.entry(item_id).or_default().push(tag);
        }

        let category_rows = sqlx::query("SELECT category FROM image_category_list ORDER BY position ASC")
            .fetch_all(&mut conn)
            .await
            .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        let category_list = category_rows
            .into_iter()
            .filter_map(|row| row.try_get::<String, _>(0).ok())
            .collect::<Vec<_>>();

        let pinned_rows = sqlx::query("SELECT item_id FROM image_pinned ORDER BY position ASC")
            .fetch_all(&mut conn)
            .await
            .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        let pinned_items = pinned_rows
            .into_iter()
            .filter_map(|row| row.try_get::<String, _>(0).ok())
            .collect::<Vec<_>>();

        Ok(ImageHistoryData {
            items,
            categories,
            category_list,
            image_tags,
            pinned_items,
        })
    })
}

pub fn load_history_page(
    offset: usize,
    limit: usize,
    category: Option<String>,
    keyword: Option<String>,
    pinned_only: bool,
    sort_order: Option<String>,
) -> Result<ImageHistoryPageData, String> {
    block_on_result(load_history_page_async(
        offset,
        limit,
        category,
        keyword,
        pinned_only,
        sort_order,
    ))
}

pub async fn load_history_page_async(
    offset: usize,
    limit: usize,
    category: Option<String>,
    keyword: Option<String>,
    pinned_only: bool,
    sort_order: Option<String>,
) -> Result<ImageHistoryPageData, String> {
    let mut conn = open_image_store_async().await?;
    let category_filter = category
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && v != "全部");
    let keyword_filter = keyword
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty());
    let pinned_flag = if pinned_only { 1_i64 } else { 0_i64 };
    let order = match sort_order.as_deref() {
        Some("desc") | Some("DESC") => "DESC",
        _ => "ASC",
    };
    let keyword_like = keyword_filter.as_ref().map(|v| format!("%{}%", v));
    let effective_limit = limit.clamp(1, 200);
    let fetch_limit = effective_limit.saturating_add(1);
    let query_sql = format!(
        "
        SELECT
          hi.position,
          hi.item_id,
          hi.width,
          hi.height,
          hi.preview_width,
          hi.preview_height,
          hi.preview_rgba_base64,
          hi.image_path,
          COALESCE(c.category, '未分类') AS category,
          CASE WHEN p.item_id IS NULL THEN 0 ELSE 1 END AS pinned
        FROM image_items hi
        LEFT JOIN image_categories c ON c.item_id = hi.item_id
        LEFT JOIN image_pinned p ON p.item_id = hi.item_id
        WHERE
          (?1 IS NULL OR COALESCE(c.category, '未分类') = ?1)
          AND (?2 = 0 OR p.item_id IS NOT NULL)
          AND (
            ?3 IS NULL
            OR LOWER(COALESCE(c.category, '未分类')) LIKE ?3
            OR EXISTS (
                SELECT 1 FROM image_tags t
                WHERE t.item_id = hi.item_id
                  AND LOWER(t.tag) LIKE ?3
            )
          )
        ORDER BY
          CASE WHEN p.item_id IS NULL THEN 1 ELSE 0 END ASC,
          CASE WHEN p.item_id IS NOT NULL THEN hi.position END ASC,
          CASE WHEN p.item_id IS NULL THEN hi.position END {}
        LIMIT ?4 OFFSET ?5
        ",
        order
    );
    let rows = sqlx::query(&query_sql)
        .bind(category_filter.as_deref())
        .bind(pinned_flag)
        .bind(keyword_like.as_deref())
        .bind(fetch_limit as i64)
        .bind(offset as i64)
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let has_more = rows.len() > effective_limit;
    let mut items = rows
        .into_iter()
        .take(effective_limit)
        .map(|row| {
            let preview_width = row.try_get::<i64, _>(4).unwrap_or(0).max(0) as u32;
            let preview_height = row.try_get::<i64, _>(5).unwrap_or(0).max(0) as u32;
            let preview_rgba_base64 = row.try_get::<String, _>(6).unwrap_or_default();
            let item_id = row.try_get::<String, _>(1).unwrap_or_default();
            let image_path = row.try_get::<String, _>(7).unwrap_or_default();
            ImageHistoryPageItem {
                position: row.try_get::<i64, _>(0).unwrap_or(0).max(0) as usize,
                id: item_id,
                width: row.try_get::<i64, _>(2).unwrap_or(0).max(0) as u32,
                height: row.try_get::<i64, _>(3).unwrap_or(0).max(0) as u32,
                preview_width,
                preview_height,
                preview_rgba_base64,
                preview_png_base64: String::new(),
                image_path,
                category: row
                    .try_get::<String, _>(8)
                    .unwrap_or_else(|_| "未分类".to_string()),
                tags: Vec::new(),
                pinned: row.try_get::<i64, _>(9).unwrap_or(0) == 1,
            }
        })
        .collect::<Vec<_>>();

    if !items.is_empty() {
        let mut item_index = HashMap::<String, usize>::new();
        for (idx, item) in items.iter().enumerate() {
            item_index.insert(item.id.clone(), idx);
        }
        let placeholders = vec!["?"; items.len()].join(", ");
        let tags_sql = format!(
            "SELECT item_id, tag FROM image_tags WHERE item_id IN ({}) ORDER BY item_id, position ASC",
            placeholders
        );
        let mut query = sqlx::query(&tags_sql);
        for item in &items {
            query = query.bind(&item.id);
        }
        let tag_rows = query
            .fetch_all(&mut conn)
            .await
            .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        for row in tag_rows {
            let item_id: String = row
                .try_get(0)
                .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            let tag: String = row
                .try_get(1)
                .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            if let Some(index) = item_index.get(&item_id) {
                items[*index].tags.push(tag);
            }
        }
    }

    let category_rows = sqlx::query("SELECT category FROM image_category_list ORDER BY position ASC")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let category_list = category_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();

    Ok(ImageHistoryPageData {
        total: if has_more {
            offset.saturating_add(effective_limit).saturating_add(1)
        } else {
            offset.saturating_add(items.len())
        },
        offset,
        limit: effective_limit,
        items,
        category_list,
    })
}

pub fn has_any_data() -> Result<bool, String> {
    block_on_result(async {
        let mut conn = open_image_store_async().await?;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_items")
            .fetch_one(&mut conn)
            .await
            .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        Ok(total > 0)
    })
}

/// 保存异步生成的预览到数据库
pub fn save_async_preview(
    item_id: &str,
    preview_width: u32,
    preview_height: u32,
    preview_base64: &str,
) -> Result<(), String> {
    block_on_result(save_async_preview_async(item_id, preview_width, preview_height, preview_base64))
}

/// 异步保存预览到数据库
pub async fn save_async_preview_async(
    item_id: &str,
    preview_width: u32,
    preview_height: u32,
    preview_base64: &str,
) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    
    sqlx::query(
        "
        INSERT INTO image_async_previews (item_id, preview_width, preview_height, preview_base64, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(item_id) DO UPDATE SET
            preview_width = excluded.preview_width,
            preview_height = excluded.preview_height,
            preview_base64 = excluded.preview_base64,
            created_at = excluded.created_at
        ",
    )
        .bind(item_id)
        .bind(preview_width as i64)
        .bind(preview_height as i64)
        .bind(preview_base64)
        .bind(created_at)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("保存异步预览失败: {}", e))?;
    Ok(())
}

/// 从数据库加载异步预览
pub fn load_async_preview(item_id: &str) -> Result<Option<(u32, u32, String)>, String> {
    block_on_result(load_async_preview_async(item_id))
}

/// 异步加载预览
pub async fn load_async_preview_async(item_id: &str) -> Result<Option<(u32, u32, String)>, String> {
    let mut conn = open_image_store_async().await?;
    let row = sqlx::query(
        "SELECT preview_width, preview_height, preview_base64 FROM image_async_previews WHERE item_id = ?1"
    )
        .bind(item_id)
        .fetch_optional(&mut conn)
        .await
        .map_err(|e| format!("加载异步预览失败: {}", e))?;
    
    match row {
        Some(row) => {
            let preview_width: i64 = row.try_get(0).map_err(|e| format!("读取预览宽度失败: {}", e))?;
            let preview_height: i64 = row.try_get(1).map_err(|e| format!("读取预览高度失败: {}", e))?;
            let preview_base64: String = row.try_get(2).map_err(|e| format!("读取预览数据失败: {}", e))?;
            Ok(Some((
                preview_width.max(0) as u32,
                preview_height.max(0) as u32,
                preview_base64,
            )))
        }
        None => Ok(None),
    }
}

/// 删除图片时同时删除异步预览
pub fn delete_async_preview(item_id: &str) -> Result<(), String> {
    block_on_result(delete_async_preview_async(item_id))
}

/// 异步删除预览
pub async fn delete_async_preview_async(item_id: &str) -> Result<(), String> {
    let mut conn = open_image_store_async().await?;
    sqlx::query("DELETE FROM image_async_previews WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut conn)
        .await
        .map_err(|e| format!("删除异步预览失败: {}", e))?;
    Ok(())
}

/// 批量删除图片的异步预览
pub fn delete_async_previews_bulk(item_ids: &[String]) -> Result<(), String> {
    block_on_result(delete_async_previews_bulk_async(item_ids))
}

/// 异步批量删除预览
pub async fn delete_async_previews_bulk_async(item_ids: &[String]) -> Result<(), String> {
    if item_ids.is_empty() {
        return Ok(());
    }
    let mut conn = open_image_store_async().await?;
    let placeholders = vec!["?"; item_ids.len()].join(", ");
    let sql = format!("DELETE FROM image_async_previews WHERE item_id IN ({})", placeholders);
    let mut query = sqlx::query(&sql);
    for item_id in item_ids {
        query = query.bind(item_id);
    }
    query.execute(&mut conn).await.map_err(|e| format!("批量删除异步预览失败: {}", e))?;
    Ok(())
}
