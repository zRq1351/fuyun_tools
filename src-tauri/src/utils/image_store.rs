use crate::utils::image_clipboard::{
    rgba_base64_to_png_base64, ImageHistoryData, ImageHistoryPageData, ImageHistoryPageItem,
};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, ToSql};
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::PathBuf;

fn get_image_store_db_path() -> PathBuf {
    let mut db_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    db_path.pop();
    db_path.push("image_history.db");
    db_path
}

fn open_image_store() -> Result<Connection, String> {
    let db_path = get_image_store_db_path();
    let conn = Connection::open(db_path).map_err(|e| format!("打开图片历史数据库失败: {}", e))?;
    init_image_store_schema(&conn)?;
    Ok(conn)
}

fn init_image_store_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS image_items (
            position INTEGER NOT NULL,
            item_id TEXT PRIMARY KEY,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            preview_width INTEGER NOT NULL,
            preview_height INTEGER NOT NULL,
            preview_rgba_base64 TEXT NOT NULL,
            image_path TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_image_items_position ON image_items(position);

        CREATE TABLE IF NOT EXISTS image_categories (
            item_id TEXT PRIMARY KEY,
            category TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS image_category_list (
            position INTEGER NOT NULL,
            category TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_image_category_list_position ON image_category_list(position);

        CREATE TABLE IF NOT EXISTS image_tags (
            item_id TEXT NOT NULL,
            tag TEXT NOT NULL,
            position INTEGER NOT NULL,
            PRIMARY KEY (item_id, tag)
        );
        CREATE INDEX IF NOT EXISTS idx_image_tags_item_position ON image_tags(item_id, position);

        CREATE TABLE IF NOT EXISTS image_pinned (
            item_id TEXT PRIMARY KEY,
            position INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_image_pinned_position ON image_pinned(position);
        ",
    )
    .map_err(|e| format!("初始化图片历史数据库失败: {}", e))
}

pub fn init_image_store() -> Result<(), String> {
    let _ = open_image_store()?;
    Ok(())
}

pub fn upsert_item(
    item: &crate::utils::image_clipboard::ImageHistoryItem,
    position: usize,
) -> Result<(), String> {
    let conn = open_image_store()?;
    conn.execute(
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
        params![
            position as i64,
            item.id,
            item.width as i64,
            item.height as i64,
            item.preview_width as i64,
            item.preview_height as i64,
            item.preview_rgba_base64,
            item.image_path
        ],
    )
    .map_err(|e| format!("写入图片历史数据库失败: {}", e))?;
    Ok(())
}

pub fn delete_item(item_id: &str) -> Result<(), String> {
    let conn = open_image_store()?;
    conn.execute("DELETE FROM image_items WHERE item_id = ?1", params![item_id])
        .map_err(|e| format!("删除图片历史数据库条目失败: {}", e))?;
    Ok(())
}

pub fn sync_item_positions(item_ids: &[String]) -> Result<(), String> {
    let mut conn = open_image_store()?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("创建图片位置事务失败: {}", e))?;
    {
        let mut stmt = tx
            .prepare("UPDATE image_items SET position = ?1 WHERE item_id = ?2")
            .map_err(|e| format!("准备更新图片位置失败: {}", e))?;
        for (position, item_id) in item_ids.iter().enumerate() {
            stmt.execute(params![position as i64, item_id])
                .map_err(|e| format!("更新图片位置失败: {}", e))?;
        }
    }
    tx.commit()
        .map_err(|e| format!("提交图片位置事务失败: {}", e))
}

pub fn upsert_category(item_id: &str, category: &str) -> Result<(), String> {
    let conn = open_image_store()?;
    conn.execute(
        "
        INSERT INTO image_categories (item_id, category)
        VALUES (?1, ?2)
        ON CONFLICT(item_id) DO UPDATE SET category = excluded.category
        ",
        params![item_id, category],
    )
    .map_err(|e| format!("写入图片分类数据库失败: {}", e))?;
    Ok(())
}

pub fn delete_category(item_id: &str) -> Result<(), String> {
    let conn = open_image_store()?;
    conn.execute("DELETE FROM image_categories WHERE item_id = ?1", params![item_id])
        .map_err(|e| format!("删除图片分类数据库失败: {}", e))?;
    Ok(())
}

pub fn sync_tags_for_item(item_id: &str, tags: &[String]) -> Result<(), String> {
    let mut conn = open_image_store()?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("创建图片标签事务失败: {}", e))?;
    tx.execute("DELETE FROM image_tags WHERE item_id = ?1", params![item_id])
        .map_err(|e| format!("清理图片标签失败: {}", e))?;
    {
        let mut stmt = tx
            .prepare("INSERT INTO image_tags (item_id, tag, position) VALUES (?1, ?2, ?3)")
            .map_err(|e| format!("准备写入图片标签失败: {}", e))?;
        for (position, tag) in tags.iter().enumerate() {
            stmt.execute(params![item_id, tag, position as i64])
                .map_err(|e| format!("写入图片标签失败: {}", e))?;
        }
    }
    tx.commit()
        .map_err(|e| format!("提交图片标签事务失败: {}", e))
}

pub fn delete_tags_for_item(item_id: &str) -> Result<(), String> {
    let conn = open_image_store()?;
    conn.execute("DELETE FROM image_tags WHERE item_id = ?1", params![item_id])
        .map_err(|e| format!("删除图片标签失败: {}", e))?;
    Ok(())
}

pub fn sync_category_list_order(categories: &[String]) -> Result<(), String> {
    let mut conn = open_image_store()?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("创建分类列表事务失败: {}", e))?;
    let mut existing = HashMap::<String, i64>::new();
    {
        let mut stmt = tx
            .prepare("SELECT category, position FROM image_category_list")
            .map_err(|e| format!("读取分类列表失败: {}", e))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .map_err(|e| format!("读取分类列表失败: {}", e))?;
        for row in rows {
            let (category, position) = row.map_err(|e| format!("读取分类列表失败: {}", e))?;
            existing.insert(category, position);
        }
    }
    {
        let mut update_stmt = tx
            .prepare("UPDATE image_category_list SET position = ?1 WHERE category = ?2")
            .map_err(|e| format!("更新分类列表失败: {}", e))?;
        let mut insert_stmt = tx
            .prepare("INSERT INTO image_category_list (position, category) VALUES (?1, ?2)")
            .map_err(|e| format!("写入分类列表失败: {}", e))?;
        for (position, category) in categories.iter().enumerate() {
            if existing.get(category) == Some(&(position as i64)) {
                continue;
            }
            let affected = update_stmt
                .execute(params![position as i64, category])
                .map_err(|e| format!("更新分类列表失败: {}", e))?;
            if affected == 0 {
                insert_stmt
                    .execute(params![position as i64, category])
                    .map_err(|e| format!("写入分类列表失败: {}", e))?;
            }
        }
    }
    {
        let desired = categories.iter().cloned().collect::<HashSet<_>>();
        let mut delete_stmt = tx
            .prepare("DELETE FROM image_category_list WHERE category = ?1")
            .map_err(|e| format!("清理分类列表失败: {}", e))?;
        for existing_category in existing.keys() {
            if desired.contains(existing_category) {
                continue;
            }
            delete_stmt
                .execute(params![existing_category])
                .map_err(|e| format!("清理分类列表失败: {}", e))?;
        }
    }
    tx.commit()
        .map_err(|e| format!("提交分类列表事务失败: {}", e))
}

pub fn sync_pinned_order(pinned_items: &[String]) -> Result<(), String> {
    let mut conn = open_image_store()?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("创建置顶事务失败: {}", e))?;
    let mut existing = HashMap::<String, i64>::new();
    {
        let mut stmt = tx
            .prepare("SELECT item_id, position FROM image_pinned")
            .map_err(|e| format!("读取置顶失败: {}", e))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .map_err(|e| format!("读取置顶失败: {}", e))?;
        for row in rows {
            let (item_id, position) = row.map_err(|e| format!("读取置顶失败: {}", e))?;
            existing.insert(item_id, position);
        }
    }
    {
        let mut upsert = tx
            .prepare(
                "
                INSERT INTO image_pinned (item_id, position)
                VALUES (?1, ?2)
                ON CONFLICT(item_id) DO UPDATE SET position = excluded.position
                ",
            )
            .map_err(|e| format!("写入置顶失败: {}", e))?;
        for (position, item_id) in pinned_items.iter().enumerate() {
            if existing.get(item_id) == Some(&(position as i64)) {
                continue;
            }
            upsert
                .execute(params![item_id, position as i64])
                .map_err(|e| format!("写入置顶失败: {}", e))?;
        }
    }
    {
        let desired = pinned_items.iter().cloned().collect::<HashSet<_>>();
        let mut delete_stmt = tx
            .prepare("DELETE FROM image_pinned WHERE item_id = ?1")
            .map_err(|e| format!("清理置顶失败: {}", e))?;
        for existing_item in existing.keys() {
            if desired.contains(existing_item) {
                continue;
            }
            delete_stmt
                .execute(params![existing_item])
                .map_err(|e| format!("清理置顶失败: {}", e))?;
        }
    }
    tx.commit()
        .map_err(|e| format!("提交置顶事务失败: {}", e))
}

pub fn delete_categories_by_category(category: &str) -> Result<(), String> {
    let conn = open_image_store()?;
    conn.execute("DELETE FROM image_categories WHERE category = ?1", params![category])
        .map_err(|e| format!("按分类删除条目失败: {}", e))?;
    Ok(())
}

pub fn load_all_data() -> Result<ImageHistoryData, String> {
    let conn = open_image_store()?;

    let mut item_stmt = conn
        .prepare(
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
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let item_rows = item_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;

    let mut items = Vec::new();
    for row in item_rows {
        let (id, width, height, preview_width, preview_height, preview_rgba_base64, image_path) =
            row.map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        items.push(crate::utils::image_clipboard::ImageHistoryItem {
            id: id.clone(),
            width: width.max(0) as u32,
            height: height.max(0) as u32,
            preview_width: preview_width.max(0) as u32,
            preview_height: preview_height.max(0) as u32,
            preview_rgba_base64,
            image_path,
            rgba_bytes: Vec::new(),
            signature: id,
        });
    }

    let mut categories = HashMap::new();
    let mut category_stmt = conn
        .prepare("SELECT item_id, category FROM image_categories")
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let category_rows = category_stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    for row in category_rows {
        let (item_id, category) = row.map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        categories.insert(item_id, category);
    }

    let mut image_tags: HashMap<String, Vec<String>> = HashMap::new();
    let mut tag_stmt = conn
        .prepare("SELECT item_id, tag FROM image_tags ORDER BY item_id, position ASC")
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let tag_rows = tag_stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    for row in tag_rows {
        let (item_id, tag) = row.map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        image_tags.entry(item_id).or_default().push(tag);
    }

    let mut category_list = Vec::new();
    let mut category_list_stmt = conn
        .prepare("SELECT category FROM image_category_list ORDER BY position ASC")
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let category_list_rows = category_list_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    for row in category_list_rows {
        category_list.push(row.map_err(|e| format!("读取图片历史数据库失败: {}", e))?);
    }

    let mut pinned_items = Vec::new();
    let mut pinned_stmt = conn
        .prepare("SELECT item_id FROM image_pinned ORDER BY position ASC")
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let pinned_rows = pinned_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    for row in pinned_rows {
        pinned_items.push(row.map_err(|e| format!("读取图片历史数据库失败: {}", e))?);
    }

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
    sort_order: Option<String>,
) -> Result<ImageHistoryPageData, String> {
    let conn = open_image_store()?;
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
    let total: i64 = conn
        .query_row(
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
            params![category_filter.as_deref(), pinned_flag, keyword_like.as_deref()],
            |row| row.get(0),
        )
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;

    let effective_limit = limit.clamp(1, 200);
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
    let mut stmt = conn
        .prepare(&query_sql)
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let rows = stmt
        .query_map(
            params![
                category_filter.as_deref(),
                pinned_flag,
                keyword_like.as_deref(),
                effective_limit as i64,
                offset as i64
            ],
            |row| {
                let preview_width = row.get::<_, i64>(4)? as u32;
                let preview_height = row.get::<_, i64>(5)? as u32;
                let preview_rgba_base64 = row.get::<_, String>(6)?;
                let image_path = row.get::<_, String>(7)?;
                let preview_png_base64 = if preview_width > 0
                    && preview_height > 0
                    && !preview_rgba_base64.is_empty()
                {
                    rgba_base64_to_png_base64(
                        &preview_rgba_base64,
                        preview_width,
                        preview_height,
                    )
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                Ok(ImageHistoryPageItem {
                    position: row.get::<_, i64>(0)? as usize,
                    id: row.get::<_, String>(1)?,
                    width: row.get::<_, i64>(2)? as u32,
                    height: row.get::<_, i64>(3)? as u32,
                    preview_width,
                    preview_height,
                    preview_rgba_base64,
                    preview_png_base64,
                    image_path,
                    category: row.get::<_, String>(8)?,
                    tags: Vec::new(),
                    pinned: row.get::<_, i64>(9)? == 1,
                })
            },
        )
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| format!("读取图片历史数据库失败: {}", e))?);
    }

    if !items.is_empty() {
        let mut item_index = HashMap::<String, usize>::new();
        let mut args: Vec<&dyn ToSql> = Vec::new();
        let mut placeholders = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            item_index.insert(item.id.clone(), idx);
            placeholders.push("?".to_string());
        }
        for item in &items {
            args.push(&item.id as &dyn ToSql);
        }
        let tags_sql = format!(
            "SELECT item_id, tag FROM image_tags WHERE item_id IN ({}) ORDER BY item_id, position ASC",
            placeholders.join(", ")
        );
        let mut tags_stmt = conn
            .prepare(&tags_sql)
            .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        let tag_rows = tags_stmt
            .query_map(params_from_iter(args), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
        for row in tag_rows {
            let (item_id, tag) = row.map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
            if let Some(index) = item_index.get(&item_id) {
                items[*index].tags.push(tag);
            }
        }
    }

    let mut category_stmt = conn
        .prepare("SELECT category FROM image_category_list ORDER BY position ASC")
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let category_rows = category_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    let mut category_list = Vec::new();
    for row in category_rows {
        category_list.push(row.map_err(|e| format!("读取图片历史数据库失败: {}", e))?);
    }

    Ok(ImageHistoryPageData {
        total: total.max(0) as usize,
        offset,
        limit: effective_limit,
        items,
        category_list,
    })
}

pub fn has_any_data() -> Result<bool, String> {
    let conn = open_image_store()?;
    let total: Option<i64> = conn
        .query_row("SELECT COUNT(*) FROM image_items", [], |row| row.get(0))
        .optional()
        .map_err(|e| format!("读取图片历史数据库失败: {}", e))?;
    Ok(total.unwrap_or(0) > 0)
}
