use super::db_utils::{reset_temp_text_table, fill_temp_text_table, reset_temp_position_table, fill_temp_position_table};
use crate::core::error_codes::AppErrorKind;
use crate::utils::image_clipboard::{
    ImageHistoryData, ImageHistoryItem, ImageHistoryPageData, ImageHistoryPageItem,
};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqliteSynchronous};
use sqlx::{Row, SqliteConnection};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

/// 全局数据库连接池
static DB_POOL: OnceLock<Arc<SqlitePool>> = OnceLock::new();
static CATEGORY_LIST_CACHE: OnceLock<Arc<StdMutex<Option<(Instant, Vec<String>)>>>> =
    OnceLock::new();
const CATEGORY_LIST_CACHE_TTL: Duration = Duration::from_secs(2);

fn get_category_list_cache() -> &'static Arc<StdMutex<Option<(Instant, Vec<String>)>>> {
    CATEGORY_LIST_CACHE.get_or_init(|| Arc::new(StdMutex::new(None)))
}

fn invalidate_category_list_cache() {
    if let Ok(mut guard) = get_category_list_cache().lock() {
        *guard = None;
    }
}

async fn load_category_list_cached(conn: &mut SqliteConnection) -> Result<Vec<String>, String> {
    if let Ok(guard) = get_category_list_cache().lock() {
        if let Some((cached_at, categories)) = guard.as_ref() {
            if cached_at.elapsed() < CATEGORY_LIST_CACHE_TTL {
                return Ok(categories.clone());
            }
        }
    }
    let category_rows =
        sqlx::query("SELECT category FROM image_category_list ORDER BY position ASC")
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
    let category_list = category_rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>(0).ok())
        .collect::<Vec<_>>();
    if let Ok(mut guard) = get_category_list_cache().lock() {
        *guard = Some((Instant::now(), category_list.clone()));
    }
    Ok(category_list)
}

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

async fn exec(conn: &mut SqliteConnection, sql: &str) -> Result<(), String> {
    sqlx::query(sql)
        .execute(conn)
        .await
        .map_err(|e| AppErrorKind::ImageStoreInitFailed.to_frontend_json_with_details(format!("{}", e)))?;
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

    // ========== 数据库迁移:为旧版本表添加缺失的 position 列 ==========

    // 迁移 1: image_items 表添加 position 列
    let _ = sqlx::query("ALTER TABLE image_items ADD COLUMN position INTEGER NOT NULL DEFAULT 0")
        .execute(&mut *conn)
        .await;

    // 迁移 2: image_category_list 表添加 position 列
    let _ = sqlx::query(
        "ALTER TABLE image_category_list ADD COLUMN position INTEGER NOT NULL DEFAULT 0",
    )
    .execute(&mut *conn)
    .await;

    // 迁移 3: image_tags 表添加 position 列
    let _ = sqlx::query("ALTER TABLE image_tags ADD COLUMN position INTEGER NOT NULL DEFAULT 0")
        .execute(&mut *conn)
        .await;

    // 迁移 4: image_pinned 表添加 position 列
    let _ = sqlx::query("ALTER TABLE image_pinned ADD COLUMN position INTEGER NOT NULL DEFAULT 0")
        .execute(&mut *conn)
        .await;

    Ok(())
}

/// 获取或初始化数据库连接池
async fn get_pool() -> Result<Arc<SqlitePool>, String> {
    if let Some(pool) = DB_POOL.get() {
        return Ok(pool.clone());
    }

    let db_path = get_image_store_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    let pool = SqlitePool::connect_with(image_store_options(&db_path))
        .await
        .map_err(|e| AppErrorKind::ImageStorePoolFailed.to_frontend_json_with_details(format!("{}", e)))?;

    let pool_arc = Arc::new(pool);

    // 初始化表结构
    let mut conn = pool_arc
        .acquire()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    init_image_store_schema_async(&mut conn).await?;

    match DB_POOL.set(pool_arc.clone()) {
        Ok(()) => Ok(pool_arc),
        Err(_) => DB_POOL
            .get()
            .cloned()
            .ok_or_else(|| AppErrorKind::ImageStoreReadFailed.to_frontend_json()),
    }
}

fn block_on_result<T>(future: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err("block_on_result must not be called from within a tokio runtime; use the async variant instead".into());
    }
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
            position, item_id, width, height, image_path
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(item_id) DO UPDATE SET
            position = excluded.position,
            width = excluded.width,
            height = excluded.height,
            image_path = excluded.image_path
        ",
    )
    .bind(position as i64)
    .bind(&item.id)
    .bind(item.width as i64)
    .bind(item.height as i64)
    .bind(&item.image_path)
    .execute(pool.as_ref())
    .await
        .map_err(|e| AppErrorKind::ImageStoreWriteFailed.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

pub async fn item_exists_async(item_id: &str) -> Result<bool, String> {
    let pool = get_pool().await?;
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM image_items WHERE item_id = ?1 LIMIT 1")
            .bind(item_id)
            .fetch_optional(pool.as_ref())
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?
            .is_some();
    Ok(exists)
}

pub fn delete_item(item_id: &str) -> Result<(), String> {
    block_on_result(delete_item_async(item_id))
}

pub async fn delete_item_async(item_id: &str) -> Result<(), String> {
    let pool = get_pool().await?;
    sqlx::query("DELETE FROM image_items WHERE item_id = ?1")
        .bind(item_id)
        .execute(pool.as_ref())
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    // 同时删除异步预览
    delete_async_preview_async(item_id).await?;

    Ok(())
}

/// 批量删除图片及其相关数据（开启事务）
pub async fn delete_items_bulk_async(item_ids: &[String]) -> Result<(), String> {
    if item_ids.is_empty() {
        return Ok(());
    }
    let pool = get_pool().await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    reset_temp_text_table(&mut tx, "temp_delete_image_item_ids", "item_id").await?;
    fill_temp_text_table(&mut tx, "temp_delete_image_item_ids", "item_id", item_ids).await?;

    sqlx::query(
        "
        DELETE FROM image_items
        WHERE EXISTS (
            SELECT 1
            FROM temp_delete_image_item_ids target
            WHERE target.item_id = image_items.item_id
        )
        ",
    )
    .execute(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::ImageStoreBatchDeleteFailed.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query(
        "
        DELETE FROM image_categories
        WHERE EXISTS (
            SELECT 1
            FROM temp_delete_image_item_ids target
            WHERE target.item_id = image_categories.item_id
        )
        ",
    )
    .execute(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query(
        "
        DELETE FROM image_tags
        WHERE EXISTS (
            SELECT 1
            FROM temp_delete_image_item_ids target
            WHERE target.item_id = image_tags.item_id
        )
        ",
    )
    .execute(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query(
        "
        DELETE FROM image_pinned
        WHERE EXISTS (
            SELECT 1
            FROM temp_delete_image_item_ids target
            WHERE target.item_id = image_pinned.item_id
        )
        ",
    )
    .execute(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query(
        "
        DELETE FROM image_async_previews
        WHERE EXISTS (
            SELECT 1
            FROM temp_delete_image_item_ids target
            WHERE target.item_id = image_async_previews.item_id
        )
        ",
    )
    .execute(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

/// 清空所有图片历史记录
pub async fn clear_all_history_async() -> Result<(), String> {
    let pool = get_pool().await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    sqlx::query("DELETE FROM image_items")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM image_categories")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM image_category_list")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM image_tags")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM image_pinned")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM image_async_previews")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    invalidate_category_list_cache();
    Ok(())
}

pub fn sync_item_positions(item_ids: &[String]) -> Result<(), String> {
    block_on_result(sync_item_positions_async(item_ids))
}

pub async fn sync_item_positions_async(item_ids: &[String]) -> Result<(), String> {
    let pool = get_pool().await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    reset_temp_position_table(&mut tx, "temp_target_image_items_position", "item_id").await?;
    fill_temp_position_table(
        &mut tx,
        "temp_target_image_items_position",
        "item_id",
        item_ids,
    )
    .await?;

    sqlx::query(
        "
        UPDATE image_items
        SET position = (
            SELECT target.position
            FROM temp_target_image_items_position target
            WHERE target.item_id = image_items.item_id
        )
        WHERE EXISTS (
            SELECT 1
            FROM temp_target_image_items_position target
            WHERE target.item_id = image_items.item_id
              AND target.position != image_items.position
        )
        ",
    )
    .execute(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
}

/// 增量更新图片位置 - 只更新受影响的图片
pub fn sync_item_positions_incremental(
    item_id: &str,
    old_position: usize,
    new_position: usize,
) -> Result<(), String> {
    block_on_result(sync_item_positions_incremental_async(
        item_id,
        old_position,
        new_position,
    ))
}

/// 异步增量更新图片位置 - 只更新受影响的图片
pub async fn sync_item_positions_incremental_async(
    item_id: &str,
    old_position: usize,
    new_position: usize,
) -> Result<(), String> {
    if old_position == new_position {
        return Ok(());
    }

    let pool = get_pool().await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    // 先把移动项挪到事务内哨兵位置，避免后续区间更新再次命中自身。
    sqlx::query("UPDATE image_items SET position = -1 WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    // 更新其他受影响图片的位置
    if old_position < new_position {
        // 图片向后移动，前面的图片位置减1
        sqlx::query(
            "UPDATE image_items SET position = position - 1 WHERE position > ?1 AND position <= ?2",
        )
        .bind(old_position as i64)
        .bind(new_position as i64)
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    } else if old_position > new_position {
        // 图片向前移动，后面的图片位置加1
        sqlx::query(
            "UPDATE image_items SET position = position + 1 WHERE position >= ?1 AND position < ?2",
        )
        .bind(new_position as i64)
        .bind(old_position as i64)
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    // 最后把目标项写回最终位置。
    sqlx::query("UPDATE image_items SET position = ?1 WHERE item_id = ?2")
        .bind(new_position as i64)
        .bind(item_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
}

pub fn upsert_category(item_id: &str, category: &str) -> Result<(), String> {
    block_on_result(upsert_category_async(item_id, category))
}

pub async fn upsert_category_async(item_id: &str, category: &str) -> Result<(), String> {
    let pool = get_pool().await?;
    sqlx::query(
        "
        INSERT INTO image_categories (item_id, category)
        VALUES (?1, ?2)
        ON CONFLICT(item_id) DO UPDATE SET category = excluded.category
        ",
    )
    .bind(item_id)
    .bind(category)
    .execute(pool.as_ref())
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

pub fn delete_category(item_id: &str) -> Result<(), String> {
    block_on_result(delete_category_async(item_id))
}

pub async fn delete_category_async(item_id: &str) -> Result<(), String> {
    let pool = get_pool().await?;
    sqlx::query("DELETE FROM image_categories WHERE item_id = ?1")
        .bind(item_id)
        .execute(pool.as_ref())
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

pub fn sync_tags_for_item(item_id: &str, tags: &[String]) -> Result<(), String> {
    block_on_result(sync_tags_for_item_async(item_id, tags))
}

pub async fn sync_tags_for_item_async(item_id: &str, tags: &[String]) -> Result<(), String> {
    let pool = get_pool().await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    sqlx::query("DELETE FROM image_tags WHERE item_id = ?1")
        .bind(item_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    for (position, tag) in tags.iter().enumerate() {
        sqlx::query("INSERT INTO image_tags (item_id, tag, position) VALUES (?1, ?2, ?3)")
            .bind(item_id)
            .bind(tag)
            .bind(position as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }
    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
}

pub fn delete_tags_for_item(item_id: &str) -> Result<(), String> {
    block_on_result(delete_tags_for_item_async(item_id))
}

pub async fn delete_tags_for_item_async(item_id: &str) -> Result<(), String> {
    let pool = get_pool().await?;
    sqlx::query("DELETE FROM image_tags WHERE item_id = ?1")
        .bind(item_id)
        .execute(pool.as_ref())
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

pub fn sync_category_list_order(categories: &[String]) -> Result<(), String> {
    block_on_result(sync_category_list_order_async(categories))
}

pub async fn sync_category_list_order_async(categories: &[String]) -> Result<(), String> {
    {
        let pool = get_pool().await?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        reset_temp_position_table(&mut tx, "temp_target_image_category_list", "category").await?;
        fill_temp_position_table(
            &mut tx,
            "temp_target_image_category_list",
            "category",
            categories,
        )
        .await?;

        sqlx::query(
            "
            DELETE FROM image_category_list
            WHERE NOT EXISTS (
                SELECT 1
                FROM temp_target_image_category_list target
                WHERE target.category = image_category_list.category
            )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

        sqlx::query(
            "
            UPDATE image_category_list
            SET position = (
                SELECT target.position
                FROM temp_target_image_category_list target
                WHERE target.category = image_category_list.category
            )
            WHERE EXISTS (
                SELECT 1
                FROM temp_target_image_category_list target
                WHERE target.category = image_category_list.category
                  AND target.position != image_category_list.position
            )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

        sqlx::query(
            "
            INSERT INTO image_category_list (position, category)
            SELECT target.position, target.category
            FROM temp_target_image_category_list target
            WHERE NOT EXISTS (
                SELECT 1
                FROM image_category_list existing
                WHERE existing.category = target.category
            )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        tx.commit()
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        invalidate_category_list_cache();
        Ok(())
    }
}

pub fn sync_pinned_order(pinned_items: &[String]) -> Result<(), String> {
    block_on_result(sync_pinned_order_async(pinned_items))
}

pub async fn sync_pinned_order_async(pinned_items: &[String]) -> Result<(), String> {
    let pool = get_pool().await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    // 检测 position 列是否存在
    let has_position =
        match sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM image_pinned")
            .fetch_one(&mut *tx)
            .await
        {
            Ok(_) => {
                log::info!("检测到 image_pinned 表有 position 列");
                true
            }
            Err(e) => {
                log::warn!("image_pinned 表没有 position 列,使用兼容模式: {}", e);
                false
            }
        };

    reset_temp_position_table(&mut tx, "temp_target_image_pinned", "item_id").await?;
    fill_temp_position_table(&mut tx, "temp_target_image_pinned", "item_id", pinned_items).await?;

    sqlx::query(
        "
        DELETE FROM image_pinned
        WHERE NOT EXISTS (
            SELECT 1
            FROM temp_target_image_pinned target
            WHERE target.item_id = image_pinned.item_id
        )
        ",
    )
    .execute(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    if has_position {
        // 有 position 列,使用完整逻辑
        sqlx::query(
            "
            UPDATE image_pinned
            SET position = (
                SELECT target.position
                FROM temp_target_image_pinned target
                WHERE target.item_id = image_pinned.item_id
            )
            WHERE EXISTS (
                SELECT 1
                FROM temp_target_image_pinned target
                WHERE target.item_id = image_pinned.item_id
                  AND target.position != image_pinned.position
            )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

        sqlx::query(
            "
            INSERT INTO image_pinned (item_id, position)
            SELECT target.item_id, target.position
            FROM temp_target_image_pinned target
            WHERE NOT EXISTS (
                SELECT 1
                FROM image_pinned existing
                WHERE existing.item_id = target.item_id
            )
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    } else {
        // 没有 position 列,只插入 item_id
        log::info!("使用兼容模式插入置顶项,共 {} 个", pinned_items.len());
        sqlx::query(
            "
            INSERT OR IGNORE INTO image_pinned (item_id)
            SELECT target.item_id
            FROM temp_target_image_pinned target
            ",
        )
        .execute(&mut *tx)
        .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
}

pub fn delete_categories_by_category(category: &str) -> Result<(), String> {
    block_on_result(delete_categories_by_category_async(category))
}

pub async fn delete_categories_by_category_async(category: &str) -> Result<(), String> {
    let pool = get_pool().await?;
    sqlx::query("DELETE FROM image_categories WHERE category = ?1")
        .bind(category)
        .execute(pool.as_ref())
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

pub fn load_all_data() -> Result<ImageHistoryData, String> {
    block_on_result(load_all_data_async())
}

pub async fn load_all_data_async() -> Result<ImageHistoryData, String> {
    let pool = get_pool().await?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    log::info!("开始加载图片历史数据...");

    // 检测 image_items 表是否有 position 列
    let has_items_position =
        match sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM image_items")
            .fetch_one(conn.as_mut())
            .await
        {
            Ok(_) => {
                log::info!("image_items 表有 position 列");
                true
            }
            Err(e) => {
                log::warn!("image_items 表没有 position 列: {}", e);
                false
            }
        };

    let item_rows = if has_items_position {
        sqlx::query(
            "
            SELECT
              hi.item_id,
              hi.width,
              hi.height,
              hi.image_path
            FROM image_items hi
            ORDER BY hi.position ASC
            LIMIT 100000
            ",
        )
        .fetch_all(conn.as_mut())
        .await
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?
    } else {
        sqlx::query(
            "
            SELECT
              hi.item_id,
              hi.width,
              hi.height,
              hi.image_path
            FROM image_items hi
            LIMIT 100000
            ",
        )
        .fetch_all(conn.as_mut())
        .await
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?
    };
    let mut items = Vec::new();
    for row in item_rows {
        let id: String = row
            .try_get(0)
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
        let width: i64 = row
            .try_get(1)
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
        let height: i64 = row
            .try_get(2)
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
        let image_path: String = row
            .try_get(3)
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
        items.push(ImageHistoryItem {
            id: id.clone(),
            width: width.max(0) as u32,
            height: height.max(0) as u32,
            image_path,
            rgba_bytes: Vec::new(),
            signature: id,
            lazy_load: true,
            cached_signature: None,
        });
    }

    let mut categories = HashMap::new();
    let category_rows = sqlx::query("SELECT item_id, category FROM image_categories")
        .fetch_all(conn.as_mut())
        .await
        .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
    for row in category_rows {
        let item_id: String = row
            .try_get(0)
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
        let category: String = row
            .try_get(1)
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
        categories.insert(item_id, category);
    }

    let mut image_tags: HashMap<String, Vec<String>> = HashMap::new();

    // 检测 image_tags 表是否有 position 列
    let has_tags_position =
        match sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM image_tags")
            .fetch_one(conn.as_mut())
            .await
        {
            Ok(_) => {
                log::info!("image_tags 表有 position 列");
                true
            }
            Err(e) => {
                log::warn!("image_tags 表没有 position 列: {}", e);
                false
            }
        };

    let tag_rows = if has_tags_position {
        sqlx::query("SELECT item_id, tag FROM image_tags ORDER BY item_id, position ASC")
            .fetch_all(conn.as_mut())
            .await
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?
    } else {
        sqlx::query("SELECT item_id, tag FROM image_tags")
            .fetch_all(conn.as_mut())
            .await
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?
    };
    for row in tag_rows {
        let item_id: String = row
            .try_get(0)
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
        let tag: String = row
            .try_get(1)
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
        image_tags.entry(item_id).or_default().push(tag);
    }

    let category_list = load_category_list_cached(conn.as_mut()).await?;

    // 尝试使用 position 排序,如果失败则不使用排序(兼容旧数据库)
    let has_pinned_position =
        match sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM image_pinned")
            .fetch_one(conn.as_mut())
            .await
        {
            Ok(_) => {
                log::info!("image_pinned 表有 position 列");
                true
            }
            Err(e) => {
                log::warn!("image_pinned 表没有 position 列: {}", e);
                false
            }
        };

    let pinned_rows = if has_pinned_position {
        // 有 position 列,使用排序
        sqlx::query("SELECT item_id FROM image_pinned ORDER BY position ASC")
            .fetch_all(conn.as_mut())
            .await
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?
    } else {
        // 没有 position 列,不排序
        sqlx::query("SELECT item_id FROM image_pinned")
            .fetch_all(conn.as_mut())
            .await
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?
    };
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
}

pub fn load_history_page(
    offset: usize,
    limit: usize,
    category: Option<String>,
    keyword: Option<String>,
    pinned_only: bool,
    sort_by: Option<String>,
    sort_order: Option<String>,
) -> Result<ImageHistoryPageData, String> {
    block_on_result(load_history_page_async(
        offset,
        limit,
        category,
        keyword,
        pinned_only,
        sort_by,
        sort_order,
    ))
}

pub async fn load_history_page_async(
    offset: usize,
    limit: usize,
    category: Option<String>,
    keyword: Option<String>,
    pinned_only: bool,
    sort_by: Option<String>,
    sort_order: Option<String>,
) -> Result<ImageHistoryPageData, String> {
    let pool = get_pool().await?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
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
    let pinned_first = sort_by
        .as_deref()
        .map(|v| matches!(v, "pinnedFirst" | "pinned_first" | "pinnedfirst"))
        .unwrap_or(true);
    let order_clause = if pinned_first {
        format!(
            "CASE WHEN p.item_id IS NULL THEN 1 ELSE 0 END ASC,
             CASE WHEN p.item_id IS NOT NULL THEN COALESCE(p.position, 2147483647) END ASC,
             CASE WHEN p.item_id IS NULL THEN hi.position END {},
             hi.item_id {}",
            order, order
        )
    } else {
        format!("hi.position {}, hi.item_id {}", order, order)
    };
    let keyword_like = keyword_filter.as_ref().map(|v| format!("%{}%", v));
    let effective_limit = limit.clamp(1, 200);
    let fetch_limit = effective_limit.saturating_add(1);
    let total: i64 = sqlx::query_scalar(
        "
        SELECT COUNT(*)
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
        ",
    )
    .bind(category_filter.as_deref())
    .bind(pinned_flag)
    .bind(keyword_like.as_deref())
    .fetch_one(conn.as_mut())
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let query_sql = format!(
        "
        SELECT
          hi.position,
          hi.item_id,
          hi.width,
          hi.height,
          hi.image_path,
          COALESCE(c.category, '未分类') AS category,
          CASE WHEN p.item_id IS NULL THEN 0 ELSE 1 END AS pinned,
          COALESCE(ap.preview_base64, '') AS preview_base64,
          COALESCE(
            (
              SELECT GROUP_CONCAT(tag, '||')
              FROM (
                SELECT t.tag
                FROM image_tags t
                WHERE t.item_id = hi.item_id
                ORDER BY t.position ASC
              )
            ),
            ''
          ) AS tags_joined
        FROM image_items hi
        LEFT JOIN image_categories c ON c.item_id = hi.item_id
        LEFT JOIN image_pinned p ON p.item_id = hi.item_id
        LEFT JOIN image_async_previews ap ON ap.item_id = hi.item_id
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
          {}
        LIMIT ?4 OFFSET ?5
        ",
        order_clause
    );
    let rows = sqlx::query(&query_sql)
        .bind(category_filter.as_deref())
        .bind(pinned_flag)
        .bind(keyword_like.as_deref())
        .bind(fetch_limit as i64)
        .bind(offset as i64)
        .fetch_all(conn.as_mut())
        .await
        .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
    let items = rows
        .into_iter()
        .take(effective_limit)
        .map(|row| {
            let item_id = row.try_get::<String, _>(1).unwrap_or_default();
            let image_path = row.try_get::<String, _>(4).unwrap_or_default();
            let tags_joined = row.try_get::<String, _>(8).unwrap_or_default();
            let tags = if tags_joined.is_empty() {
                Vec::new()
            } else {
                tags_joined
                    .split("||")
                    .map(|tag| tag.to_string())
                    .collect::<Vec<_>>()
            };
            ImageHistoryPageItem {
                position: row.try_get::<i64, _>(0).unwrap_or(0).max(0) as usize,
                id: item_id,
                width: row.try_get::<i64, _>(2).unwrap_or(0).max(0) as u32,
                height: row.try_get::<i64, _>(3).unwrap_or(0).max(0) as u32,
                preview_png_base64: row.try_get::<String, _>(7).unwrap_or_default(),
                image_path,
                category: row
                    .try_get::<String, _>(5)
                    .unwrap_or_else(|_| "未分类".to_string()),
                tags,
                pinned: row.try_get::<i64, _>(6).unwrap_or(0) == 1,
            }
        })
        .collect::<Vec<_>>();
    let category_list = load_category_list_cached(conn.as_mut()).await?;

    Ok(ImageHistoryPageData {
        total: total.max(0) as usize,
        offset,
        limit: effective_limit,
        items,
        category_list,
    })
}

pub fn has_any_data() -> Result<bool, String> {
    block_on_result(async {
        let pool = get_pool().await?;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_items")
            .fetch_one(pool.as_ref())
            .await
            .map_err(|e| AppErrorKind::ImageStoreReadFailed.to_frontend_json_with_details(format!("{}", e)))?;
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
    block_on_result(save_async_preview_async(
        item_id,
        preview_width,
        preview_height,
        preview_base64,
    ))
}

/// 异步保存预览到数据库
pub async fn save_async_preview_async(
    item_id: &str,
    preview_width: u32,
    preview_height: u32,
    preview_base64: &str,
) -> Result<(), String> {
    let pool = get_pool().await?;
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
        .execute(pool.as_ref())
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

/// 从数据库加载异步预览
pub fn load_async_preview(item_id: &str) -> Result<Option<(u32, u32, String)>, String> {
    block_on_result(load_async_preview_async(item_id))
}

/// 异步加载预览
pub async fn load_async_preview_async(item_id: &str) -> Result<Option<(u32, u32, String)>, String> {
    let pool = get_pool().await?;
    let row = sqlx::query(
        "SELECT preview_width, preview_height, preview_base64 FROM image_async_previews WHERE item_id = ?1"
    )
        .bind(item_id)
        .fetch_optional(pool.as_ref())
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    match row {
        Some(row) => {
            let preview_width: i64 = row
                .try_get(0)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
            let preview_height: i64 = row
                .try_get(1)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
            let preview_base64: String = row
                .try_get(2)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
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
    let pool = get_pool().await?;
    sqlx::query("DELETE FROM image_async_previews WHERE item_id = ?1")
        .bind(item_id)
        .execute(pool.as_ref())
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
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
    let pool = get_pool().await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    reset_temp_text_table(&mut tx, "temp_delete_image_item_ids", "item_id").await?;
    fill_temp_text_table(&mut tx, "temp_delete_image_item_ids", "item_id", item_ids).await?;
    sqlx::query(
        "
        DELETE FROM image_async_previews
        WHERE EXISTS (
            SELECT 1
            FROM temp_delete_image_item_ids target
            WHERE target.item_id = image_async_previews.item_id
        )
        ",
    )
    .execute(&mut *tx)
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    tx.commit()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    Ok(())
}

/// 添加分类(如果不存在)
pub async fn add_category_if_not_exists_async(category: &str) -> Result<(), String> {
    let pool = get_pool().await?;
    sqlx::query("INSERT OR IGNORE INTO image_category_list(category) VALUES(?)")
        .bind(category)
        .execute(pool.as_ref())
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    invalidate_category_list_cache();
    Ok(())
}

/// 合并且置顶项:将新项追加到当前置顶列表末尾,跳过已存在的
pub async fn merge_pinned_items_async(item_ids: &[String]) -> Result<(), String> {
    if item_ids.is_empty() {
        return Ok(());
    }

    let pool = get_pool().await?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    // 获取当前已置顶的 item_id 集合
    let existing_pinned: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT item_id FROM image_pinned WHERE item_id IS NOT NULL AND item_id != ''",
    )
    .fetch_all(conn.as_mut())
    .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?
    .into_iter()
    .collect();

    // 尝试获取当前最大 position,如果失败则使用简单插入
    let use_position =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM image_pinned")
            .fetch_one(conn.as_mut())
            .await
            .is_ok();

    if use_position {
        // 有 position 列,使用位置排序
        let current_max_position =
            match sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(position) FROM image_pinned")
                .fetch_one(conn.as_mut())
                .await
            {
                Ok(val) => val.unwrap_or(-1),
                Err(_) => -1, // 如果失败,从 -1 开始
            };

        let mut position = current_max_position + 1;
        for item_id in item_ids {
            if existing_pinned.contains(item_id) {
                continue; // 跳过已置顶的
            }

            sqlx::query(
                "INSERT INTO image_pinned(item_id, position) VALUES(?1, ?2)
                 ON CONFLICT(item_id) DO NOTHING",
            )
            .bind(item_id)
            .bind(position)
            .execute(conn.as_mut())
            .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
            position += 1;
        }
    } else {
        // 没有 position 列,简单插入
        for item_id in item_ids {
            if existing_pinned.contains(item_id) {
                continue; // 跳过已置顶的
            }

            sqlx::query("INSERT OR IGNORE INTO image_pinned(item_id) VALUES(?1)")
                .bind(item_id)
                .execute(conn.as_mut())
                .await
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_image_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS image_items (
                position INTEGER NOT NULL,
                item_id TEXT PRIMARY KEY,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                image_path TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS image_async_previews (
                item_id TEXT PRIMARY KEY,
                preview_width INTEGER NOT NULL,
                preview_height INTEGER NOT NULL,
                preview_base64 TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS image_categories (
                item_id TEXT PRIMARY KEY,
                category TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS image_category_list (
                position INTEGER NOT NULL,
                category TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS image_tags (
                item_id TEXT NOT NULL,
                tag TEXT NOT NULL,
                position INTEGER NOT NULL,
                PRIMARY KEY (item_id, tag)
            );
            CREATE TABLE IF NOT EXISTS image_pinned (
                item_id TEXT PRIMARY KEY,
                position INTEGER NOT NULL
            );
            ",
        )
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn integration_image_upsert_and_query() {
        let pool = create_test_image_pool().await;

        sqlx::query(
            "INSERT INTO image_items (position, item_id, width, height, image_path)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
            .bind(0i64)
            .bind("img001")
            .bind(1920i64)
            .bind(1080i64)
            .bind("/tmp/img001.png")
            .execute(&pool)
            .await
            .unwrap();

        let row: (i64, i64, String) = sqlx::query_as(
            "SELECT width, height, image_path FROM image_items WHERE item_id = 'img001'",
        )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 1920);
        assert_eq!(row.1, 1080);
        assert_eq!(row.2, "/tmp/img001.png");
    }

    #[tokio::test]
    async fn integration_image_upsert_update() {
        let pool = create_test_image_pool().await;

        sqlx::query(
            "INSERT INTO image_items (position, item_id, width, height, image_path)
             VALUES (0, 'img001', 100, 100, '/old.png')",
        )
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO image_items (position, item_id, width, height, image_path)
             VALUES (1, 'img001', 200, 200, '/new.png')
             ON CONFLICT(item_id) DO UPDATE SET
                position=excluded.position, width=excluded.width,
                height=excluded.height, image_path=excluded.image_path",
        )
            .execute(&pool)
            .await
            .unwrap();

        let row: (i64, String) = sqlx::query_as(
            "SELECT width, image_path FROM image_items WHERE item_id = 'img001'",
        )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 200);
        assert_eq!(row.1, "/new.png");
    }

    #[tokio::test]
    async fn integration_image_categories_full_flow() {
        let pool = create_test_image_pool().await;

        sqlx::query("INSERT INTO image_categories (item_id, category) VALUES (?1, ?2)")
            .bind("img001")
            .bind("截图")
            .execute(&pool)
            .await
            .unwrap();

        let cat: String = sqlx::query_scalar("SELECT category FROM image_categories WHERE item_id = 'img001'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cat, "截图");

        sqlx::query(
            "INSERT INTO image_categories (item_id, category) VALUES (?1, ?2)
             ON CONFLICT(item_id) DO UPDATE SET category=excluded.category",
        )
            .bind("img001")
            .bind("照片")
            .execute(&pool)
            .await
            .unwrap();

        let cat: String = sqlx::query_scalar("SELECT category FROM image_categories WHERE item_id = 'img001'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cat, "照片");

        sqlx::query("DELETE FROM image_categories WHERE item_id = 'img001'")
            .execute(&pool)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_categories WHERE item_id = 'img001'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn integration_image_tags_flow() {
        let pool = create_test_image_pool().await;

        let tags = vec!["风景", "旅行", "2024"];
        for (pos, tag) in tags.iter().enumerate() {
            sqlx::query("INSERT INTO image_tags (item_id, tag, position) VALUES (?1, ?2, ?3)")
                .bind("img001")
                .bind(tag)
                .bind(pos as i64)
                .execute(&pool)
                .await
                .unwrap();
        }

        let result: Vec<String> = sqlx::query_scalar(
            "SELECT tag FROM image_tags WHERE item_id = 'img001' ORDER BY position ASC",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result, vec!["风景", "旅行", "2024"]);

        sqlx::query("DELETE FROM image_tags WHERE item_id = 'img001'")
            .execute(&pool)
            .await
            .unwrap();
        let new_tags = vec!["美食", "城市"];
        for (pos, tag) in new_tags.iter().enumerate() {
            sqlx::query("INSERT INTO image_tags (item_id, tag, position) VALUES (?1, ?2, ?3)")
                .bind("img001")
                .bind(tag)
                .bind(pos as i64)
                .execute(&pool)
                .await
                .unwrap();
        }

        let result: Vec<String> = sqlx::query_scalar(
            "SELECT tag FROM image_tags WHERE item_id = 'img001' ORDER BY position ASC",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result, vec!["美食", "城市"]);
    }

    #[tokio::test]
    async fn integration_image_pinned_with_position() {
        let pool = create_test_image_pool().await;

        sqlx::query("INSERT INTO image_pinned (item_id, position) VALUES (?1, ?2)")
            .bind("img001")
            .bind(0i64)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO image_pinned (item_id, position) VALUES (?1, ?2)")
            .bind("img002")
            .bind(1i64)
            .execute(&pool)
            .await
            .unwrap();

        let result: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM image_pinned ORDER BY position ASC",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result, vec!["img001", "img002"]);

        sqlx::query("UPDATE image_pinned SET position = 10 WHERE item_id = 'img001'")
            .execute(&pool)
            .await
            .unwrap();

        let result: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM image_pinned ORDER BY position ASC",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result, vec!["img002", "img001"]);
    }

    #[tokio::test]
    async fn integration_image_category_list_with_position() {
        let pool = create_test_image_pool().await;

        let cats = vec!["风景", "人物", "建筑"];
        for (pos, cat) in cats.iter().enumerate() {
            sqlx::query("INSERT INTO image_category_list (position, category) VALUES (?1, ?2)")
                .bind(pos as i64)
                .bind(cat)
                .execute(&pool)
                .await
                .unwrap();
        }

        let result: Vec<String> = sqlx::query_scalar(
            "SELECT category FROM image_category_list ORDER BY position ASC",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result, vec!["风景", "人物", "建筑"]);

        sqlx::query("DELETE FROM image_category_list").execute(&pool).await.unwrap();
        let new_order = vec!["建筑", "风景", "人物"];
        for (pos, cat) in new_order.iter().enumerate() {
            sqlx::query("INSERT INTO image_category_list (position, category) VALUES (?1, ?2)")
                .bind(pos as i64)
                .bind(cat)
                .execute(&pool)
                .await
                .unwrap();
        }

        let result: Vec<String> = sqlx::query_scalar(
            "SELECT category FROM image_category_list ORDER BY position ASC",
        )
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result, vec!["建筑", "风景", "人物"]);
    }

    #[tokio::test]
    async fn integration_image_bulk_delete() {
        let pool = create_test_image_pool().await;

        for i in 0..10 {
            sqlx::query(
                "INSERT INTO image_items (position, item_id, width, height, image_path)
                 VALUES (?1, ?2, 100, 100, ?3)",
            )
                .bind(i as i64)
                .bind(format!("img{:02}", i))
                .bind(format!("/tmp/img{:02}.png", i))
                .execute(&pool)
                .await
                .unwrap();

            sqlx::query("INSERT INTO image_categories (item_id, category) VALUES (?1, '测试')")
                .bind(format!("img{:02}", i))
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO image_tags (item_id, tag, position) VALUES (?1, 'tag', 0)")
                .bind(format!("img{:02}", i))
                .execute(&pool)
                .await
                .unwrap();
        }

        let to_delete = vec!["img00", "img02", "img04", "img06", "img08"];
        for id in &to_delete {
            sqlx::query("DELETE FROM image_items WHERE item_id = ?1")
                .bind(id).execute(&pool).await.unwrap();
            sqlx::query("DELETE FROM image_categories WHERE item_id = ?1")
                .bind(id).execute(&pool).await.unwrap();
            sqlx::query("DELETE FROM image_tags WHERE item_id = ?1")
                .bind(id).execute(&pool).await.unwrap();
        }

        let item_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_items")
            .fetch_one(&pool).await.unwrap();
        let cat_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_categories")
            .fetch_one(&pool).await.unwrap();
        let tag_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_tags")
            .fetch_one(&pool).await.unwrap();

        assert_eq!(item_count, 5);
        assert_eq!(cat_count, 5);
        assert_eq!(tag_count, 5);
    }

    #[tokio::test]
    async fn integration_image_preview_roundtrip() {
        let pool = create_test_image_pool().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO image_async_previews (item_id, preview_width, preview_height, preview_base64, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(item_id) DO UPDATE SET
                preview_width=excluded.preview_width, preview_height=excluded.preview_height,
                preview_base64=excluded.preview_base64, created_at=excluded.created_at",
        )
            .bind("img001")
            .bind(200i64)
            .bind(150i64)
            .bind("base64data123")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        let row: (i64, i64, String) = sqlx::query_as(
            "SELECT preview_width, preview_height, preview_base64 FROM image_async_previews WHERE item_id = 'img001'",
        )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 200);
        assert_eq!(row.1, 150);
        assert_eq!(row.2, "base64data123");

        sqlx::query(
            "INSERT INTO image_async_previews (item_id, preview_width, preview_height, preview_base64, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(item_id) DO UPDATE SET
                preview_width=excluded.preview_width, preview_base64=excluded.preview_base64",
        )
            .bind("img001")
            .bind(400i64)
            .bind(300i64)
            .bind("newbase64data")
            .bind(now + 100)
            .execute(&pool)
            .await
            .unwrap();

        let row: (i64, String) = sqlx::query_as(
            "SELECT preview_width, preview_base64 FROM image_async_previews WHERE item_id = 'img001'",
        )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 400);
        assert_eq!(row.1, "newbase64data");
    }

    #[tokio::test]
    async fn integration_image_concurrent_position_updates() {
        let pool = std::sync::Arc::new(create_test_image_pool().await);

        for i in 0..20 {
            sqlx::query(
                "INSERT INTO image_items (position, item_id, width, height, image_path)
                 VALUES (?1, ?2, 100, 100, ?3)",
            )
                .bind(i as i64)
                .bind(format!("img{:02}", i))
                .bind(format!("/tmp/img{:02}.png", i))
                .execute(pool.as_ref())
                .await
                .unwrap();
        }

        let mut handles = vec![];

        for w in 0..5 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                for j in 0..4 {
                    let item_id = format!("img{:02}", w * 4 + j);
                    let new_pos = (w * 4 + j + 10) as i64;
                    sqlx::query("UPDATE image_items SET position = ?1 WHERE item_id = ?2")
                        .bind(new_pos)
                        .bind(&item_id)
                        .execute(pool.as_ref())
                        .await
                        .unwrap();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_items")
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
        assert_eq!(count, 20, "并发更新后图片数量不变");
    }
}
