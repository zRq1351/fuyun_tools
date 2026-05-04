use serde::{Deserialize, Serialize};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{Acquire, Row, Sqlite, SqliteConnection};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::OnceCell;

static DOCS_DB_POOL: OnceCell<SqlitePool> = OnceCell::const_new();

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocRoot {
    pub id: i64,
    pub name: String,
    pub root_path: String,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocCategory {
    pub id: i64,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub position: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocFile {
    pub id: i64,
    pub root_id: i64,
    pub title: String,
    pub file_name: String,
    pub file_ext: String,
    pub file_size: i64,
    pub file_hash: String,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub tags: String,
    pub notes: String,
    pub content_text: String,
    pub source_path: String,
    pub managed_path: String,
    pub storage_mode: String,
    pub is_missing: bool,
    pub added_at: i64,
    pub file_modified: i64,
    pub visit_count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocPageData {
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
    pub items: Vec<DocFile>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocStats {
    pub total_files: i64,
    pub total_size: i64,
    pub missing_files: i64,
    pub category_counts: Vec<CategoryCount>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CategoryCount {
    pub category_id: Option<i64>,
    pub category_name: String,
    pub count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportHistory {
    pub id: i64,
    pub root_id: i64,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub storage_mode: String,
    pub source_dir: String,
    pub target_dir: String,
    pub file_count: i64,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileItem {
    pub doc_file_id: i64,
    pub file_name: String,
    pub source_path: String,
    pub managed_path: String,
}

pub fn get_docs_db_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    path.pop();
    path.push("docs_data");
    fs::create_dir_all(&path).ok();
    path.push("docs.db");
    path
}

fn db_options(db_path: &PathBuf) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_millis(1200))
}

fn now_unix_ms() -> i64 {
    crate::utils::utils_helpers::now_unix_ms_i64()
}

async fn get_docs_db_pool() -> Result<&'static SqlitePool, String> {
    DOCS_DB_POOL
        .get_or_try_init(|| async {
            let db_path = get_docs_db_path();
            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("创建文档数据库目录失败: {}", e))?;
            }
            let pool = SqlitePoolOptions::new()
                .max_connections(3)
                .connect_with(db_options(&db_path))
                .await
                .map_err(|e| format!("打开文档数据库连接池失败: {}", e))?;

            let mut conn = pool
                .acquire()
                .await
                .map_err(|e| format!("获取数据库连接失败: {}", e))?;
            ensure_docs_db_schema(&mut conn).await?;

            Ok(pool)
        })
        .await
}

async fn open_docs_db() -> Result<sqlx::pool::PoolConnection<Sqlite>, String> {
    let pool = get_docs_db_pool().await?;
    pool.acquire()
        .await
        .map_err(|e| format!("获取数据库连接失败: {}", e))
}

async fn ensure_docs_db_schema(conn: &mut SqliteConnection) -> Result<(), String> {
    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS document_roots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            root_path TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS document_categories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            icon TEXT DEFAULT 'folder',
            color TEXT DEFAULT '#409EFF',
            position INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS document_files (
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
            added_at INTEGER NOT NULL DEFAULT 0,
            file_modified INTEGER NOT NULL DEFAULT 0,
            visit_count INTEGER DEFAULT 0,
            FOREIGN KEY (root_id) REFERENCES document_roots(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_doc_files_root_id ON document_files(root_id);
        CREATE INDEX IF NOT EXISTS idx_doc_files_category_id ON document_files(category_id);
        CREATE INDEX IF NOT EXISTS idx_doc_files_added_at ON document_files(added_at DESC);
        CREATE INDEX IF NOT EXISTS idx_doc_files_file_ext ON document_files(file_ext);
        ",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("初始化文档数据库失败: {}", e))?;

    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS document_imports (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            root_id INTEGER NOT NULL,
            category_id INTEGER,
            storage_mode TEXT NOT NULL DEFAULT 'index',
            source_dir TEXT DEFAULT '',
            target_dir TEXT DEFAULT '',
            file_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS document_import_items (
            import_id INTEGER NOT NULL,
            doc_file_id INTEGER NOT NULL,
            source_path TEXT DEFAULT '',
            managed_path TEXT DEFAULT '',
            PRIMARY KEY (import_id, doc_file_id),
            FOREIGN KEY (import_id) REFERENCES document_imports(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_import_items_import_id ON document_import_items(import_id);
        ",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("初始化历史记录表失败: {}", e))?;

    let _ = sqlx::query::<Sqlite>("ALTER TABLE document_files ADD COLUMN storage_mode TEXT NOT NULL DEFAULT 'index'")
        .execute(&mut *conn)
        .await;

    let _ = sqlx::query::<Sqlite>("ALTER TABLE document_roots ADD COLUMN position INTEGER DEFAULT 0")
        .execute(&mut *conn)
        .await;

    let _ = sqlx::query::<Sqlite>("ALTER TABLE document_files ADD COLUMN sort_order INTEGER DEFAULT 0")
        .execute(&mut *conn)
        .await;

    sqlx::query(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS document_files_fts USING fts5(
            title,
            content_text,
            tags,
            notes,
            tokenize = 'unicode61'
        );
        ",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("创建FTS索引失败: {}", e))?;

    let _ = sqlx::query::<Sqlite>("ALTER TABLE document_imports ADD COLUMN source_dir TEXT DEFAULT ''")
        .execute(&mut *conn)
        .await;
    let _ = sqlx::query::<Sqlite>("ALTER TABLE document_imports ADD COLUMN target_dir TEXT DEFAULT ''")
        .execute(&mut *conn)
        .await;

    let fts_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_files_fts")
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0);

    let doc_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_files")
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0);

    if fts_count != doc_count {
        let _ = sqlx::query(
            "
            INSERT OR REPLACE INTO document_files_fts(rowid, title, content_text, tags, notes)
            SELECT id, COALESCE(title, ''), content_text, COALESCE(tags, ''), COALESCE(notes, '') FROM document_files
            ",
        )
        .execute(&mut *conn)
        .await;

        let _ = sqlx::query(
            "DELETE FROM document_files_fts WHERE rowid NOT IN (SELECT id FROM document_files)",
        )
        .execute(&mut *conn)
        .await;
    }

    let default_categories = [
        ("合同", "folder", "#E74C3C"),
        ("报表", "folder", "#27AE60"),
        ("资料", "folder", "#3498DB"),
        ("其他", "folder", "#7F8C8D"),
    ];
    for (i, (name, icon, color)) in default_categories.iter().enumerate() {
        sqlx::query::<Sqlite>(
            "INSERT OR IGNORE INTO document_categories (name, icon, color, position) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(name)
        .bind(icon)
        .bind(color)
        .bind(i as i64)
        .execute(&mut *conn)
        .await
        .ok();
    }

    ensure_category_directories(conn).await?;

    Ok(())
}

async fn ensure_category_directories(conn: &mut SqliteConnection) -> Result<(), String> {
    let roots = sqlx::query("SELECT id, name, root_path, created_at FROM document_roots")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("查询根目录失败: {}", e))?;

    if roots.is_empty() {
        return Ok(());
    }

    let cats = sqlx::query("SELECT id, name FROM document_categories ORDER BY position")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("查询分类失败: {}", e))?;

    for row in &roots {
        let root_path: String = row.try_get(2).unwrap_or_default();
        for crow in &cats {
            let cat_name: String = crow.try_get(1).unwrap_or_default();
            let _ = fs::create_dir_all(Path::new(&root_path).join(&cat_name));
        }
    }

    Ok(())
}

pub async fn add_doc_root(name: &str, root_path: &str) -> Result<DocRoot, String> {
    let mut conn = open_docs_db().await?;
    let now = now_unix_ms();

    let max_pos: i64 = sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(position), -1) FROM document_roots")
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(-1);

    sqlx::query("INSERT INTO document_roots (name, root_path, created_at, position) VALUES (?1, ?2, ?3, ?4)")
        .bind(name)
        .bind(root_path)
        .bind(now)
        .bind(max_pos + 1)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("添加根目录失败: {}", e))?;

    let id = sqlx::query_scalar::<_, i64>("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("获取ID失败: {}", e))?;

    Ok(DocRoot {
        id,
        name: name.to_string(),
        root_path: root_path.to_string(),
        created_at: now,
    })
}

pub async fn get_doc_roots() -> Result<Vec<DocRoot>, String> {
    let mut conn = open_docs_db().await?;
    let rows = sqlx::query(
        "SELECT id, name, root_path, created_at FROM document_roots ORDER BY position ASC, id ASC",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("获取根目录列表失败: {}", e))?;

    let roots = rows
        .iter()
        .map(|row| DocRoot {
            id: row.try_get::<i64, _>(0).unwrap_or(0),
            name: row.try_get::<String, _>(1).unwrap_or_default(),
            root_path: row.try_get::<String, _>(2).unwrap_or_default(),
            created_at: row.try_get::<i64, _>(3).unwrap_or(0),
        })
        .collect();

    Ok(roots)
}

pub async fn reorder_doc_roots(ids: Vec<i64>) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;
    for (idx, id) in ids.iter().enumerate() {
        sqlx::query::<Sqlite>("UPDATE document_roots SET position = ?1 WHERE id = ?2")
            .bind(idx as i64)
            .bind(*id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("更新根目录顺序失败: {}", e))?;
    }
    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))
}

pub async fn remove_doc_root(id: i64) -> Result<(), String> {
    let mut conn = open_docs_db().await?;

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files WHERE root_id = ?1"
    )
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("查询根目录下文件失败: {}", e))?;

    if count.0 > 0 {
        return Err("该目录下存在文件，请先将文件删除或移至其他目录后再删除".to_string());
    }

    sqlx::query("DELETE FROM document_roots WHERE id = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除根目录失败: {}", e))?;

    Ok(())
}

pub async fn add_doc_category(name: &str, icon: &str, color: &str) -> Result<DocCategory, String> {
    let mut conn = open_docs_db().await?;

    let max_pos: i64 = sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(position), -1) FROM document_categories")
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(-1);

    sqlx::query(
        "INSERT INTO document_categories (name, icon, color, position) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(name)
    .bind(icon)
    .bind(color)
    .bind(max_pos + 1)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("添加分类失败: {}", e))?;

    let id = sqlx::query_scalar::<_, i64>("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("获取ID失败: {}", e))?;

    Ok(DocCategory {
        id,
        name: name.to_string(),
        icon: icon.to_string(),
        color: color.to_string(),
        position: max_pos + 1,
    })
}

pub async fn get_doc_categories() -> Result<Vec<DocCategory>, String> {
    let mut conn = open_docs_db().await?;
    let rows = sqlx::query(
        "SELECT id, name, icon, color, position FROM document_categories ORDER BY position ASC, id ASC",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("获取分类列表失败: {}", e))?;

    let categories = rows
        .iter()
        .map(|row| DocCategory {
            id: row.try_get::<i64, _>(0).unwrap_or(0),
            name: row.try_get::<String, _>(1).unwrap_or_default(),
            icon: row.try_get::<String, _>(2).unwrap_or_default(),
            color: row.try_get::<String, _>(3).unwrap_or_default(),
            position: row.try_get::<i64, _>(4).unwrap_or(0),
        })
        .collect();

    Ok(categories)
}

pub async fn remove_doc_category(id: i64) -> Result<(), String> {
    let mut conn = open_docs_db().await?;

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files WHERE category_id = ?1"
    )
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("查询分类下文件失败: {}", e))?;

    if count.0 > 0 {
        return Err("该分类下存在文件，请先将文件移至其他分类或取消分类后再删除".to_string());
    }

    sqlx::query("DELETE FROM document_categories WHERE id = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除分类失败: {}", e))?;

    Ok(())
}

pub async fn rename_doc_category(id: i64, name: &str) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    sqlx::query("UPDATE document_categories SET name = ?1 WHERE id = ?2")
        .bind(name)
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("重命名分类失败: {}", e))?;
    Ok(())
}

pub async fn reorder_doc_categories(ids: Vec<i64>) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| format!("创建事务失败: {}", e))?;

    for (idx, id) in ids.iter().enumerate() {
        sqlx::query::<Sqlite>("UPDATE document_categories SET position = ?1 WHERE id = ?2")
            .bind(idx as i64)
            .bind(*id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("更新分类顺序失败: {}", e))?;
    }

    tx.commit()
        .await
        .map_err(|e| format!("提交事务失败: {}", e))?;

    Ok(())
}

pub async fn insert_doc_file(
    root_id: i64,
    file_name: &str,
    file_ext: &str,
    file_size: i64,
    file_hash: &str,
    category_id: Option<i64>,
    tags: &str,
    source_path: &str,
    managed_path: &str,
    storage_mode: &str,
    file_modified: i64,
    content_text: &str,
) -> Result<i64, String> {
    let mut conn = open_docs_db().await?;
    let now = now_unix_ms();

    sqlx::query(
        "INSERT INTO document_files (root_id, title, file_name, file_ext, file_size, file_hash, category_id, tags, source_path, managed_path, storage_mode, added_at, file_modified, content_text)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )
    .bind(root_id)
    .bind(file_name)
    .bind(file_name)
    .bind(file_ext)
    .bind(file_size)
    .bind(file_hash)
    .bind(category_id)
    .bind(tags)
    .bind(source_path)
    .bind(managed_path)
    .bind(storage_mode)
    .bind(now)
    .bind(file_modified)
    .bind(content_text)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("插入文件记录失败: {}", e))?;

    let id = sqlx::query_scalar::<_, i64>("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("获取ID失败: {}", e))?;

    let _ = sqlx::query(
        "INSERT INTO document_files_fts(rowid, title, content_text, tags, notes) VALUES (?1, ?2, ?3, '', '')",
    )
    .bind(id)
    .bind(file_name)
    .bind(content_text)
    .execute(&mut *conn)
    .await;

    Ok(id)
}

pub async fn delete_doc_file(id: i64) -> Result<Option<String>, String> {
    let mut conn = open_docs_db().await?;

    let managed_path: Option<String> =
        sqlx::query_scalar::<_, String>("SELECT managed_path FROM document_files WHERE id = ?1")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| format!("查询文件记录失败: {}", e))?;

    sqlx::query("DELETE FROM document_files WHERE id = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除文件记录失败: {}", e))?;

    let _ = sqlx::query("DELETE FROM document_files_fts WHERE rowid = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await;

    Ok(managed_path)
}

pub async fn delete_doc_record(id: i64) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    sqlx::query("DELETE FROM document_files WHERE id = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除文件记录失败: {}", e))?;
    let _ = sqlx::query("DELETE FROM document_files_fts WHERE rowid = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await;
    Ok(())
}

pub async fn update_doc_file_meta(
    id: i64,
    title: Option<&str>,
    category_id: Option<i64>,
    tags: Option<&str>,
    notes: Option<&str>,
) -> Result<(), String> {
    let mut conn = open_docs_db().await?;

    let cat_change = category_id.is_some();
    let mut needs_move = false;
    let mut old_managed = String::new();
    let mut root_path = String::new();
    let mut new_cat_name = String::new();

    if cat_change {
        let row = sqlx::query(
            "SELECT df.storage_mode, df.managed_path, dr.root_path, df.category_id
             FROM document_files df
             JOIN document_roots dr ON df.root_id = dr.id
             WHERE df.id = ?1"
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| format!("查询文件信息失败: {}", e))?;

        if let Some(row) = row {
            let storage_mode: String = row.try_get(0).unwrap_or_default();
            old_managed = row.try_get(1).unwrap_or_default();
            root_path = row.try_get(2).unwrap_or_default();
            let old_cat_id: Option<i64> = row.try_get(3).unwrap_or(None);
            let new_cid = category_id.unwrap();

            if storage_mode == "repo" && !old_managed.is_empty() && old_cat_id != Some(new_cid) {
                new_cat_name = if new_cid == -1 {
                    String::new()
                } else {
                    sqlx::query_scalar::<_, String>("SELECT name FROM document_categories WHERE id = ?1")
                        .bind(new_cid)
                        .fetch_optional(&mut *conn)
                        .await
                        .map_err(|e| format!("查询分类失败: {}", e))?
                        .unwrap_or_default()
                };
                needs_move = true;
            }
        }
    }

    if let Some(t) = title {
        sqlx::query("UPDATE document_files SET title = ?1 WHERE id = ?2")
            .bind(t)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新标题失败: {}", e))?;

        let _ = sqlx::query("UPDATE document_files_fts SET title = ?1 WHERE rowid = ?2")
            .bind(t)
            .bind(id)
            .execute(&mut *conn)
            .await;
    }

    if let Some(cid) = category_id {
        if cid == -1 {
            sqlx::query("UPDATE document_files SET category_id = NULL WHERE id = ?1")
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("更新分类失败: {}", e))?;
        } else {
            sqlx::query("UPDATE document_files SET category_id = ?1 WHERE id = ?2")
                .bind(cid)
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("更新分类失败: {}", e))?;
        }
    }

    if let Some(tg) = tags {
        sqlx::query("UPDATE document_files SET tags = ?1 WHERE id = ?2")
            .bind(tg)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新标签失败: {}", e))?;

        let _ = sqlx::query("UPDATE document_files_fts SET tags = ?1 WHERE rowid = ?2")
            .bind(tg)
            .bind(id)
            .execute(&mut *conn)
            .await;
    }

    if let Some(n) = notes {
        sqlx::query("UPDATE document_files SET notes = ?1 WHERE id = ?2")
            .bind(n)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新备注失败: {}", e))?;

        let _ = sqlx::query("UPDATE document_files_fts SET notes = ?1 WHERE rowid = ?2")
            .bind(n)
            .bind(id)
            .execute(&mut *conn)
            .await;
    }

    if needs_move {
        let old_path = Path::new(&old_managed);
        if old_path.exists() {
            let file_name = old_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let target_dir = if new_cat_name.is_empty() {
                Path::new(&root_path).to_path_buf()
            } else {
                Path::new(&root_path).join(&new_cat_name)
            };
            fs::create_dir_all(&target_dir).map_err(|e| format!("创建目标目录失败: {}", e))?;
            let new_name = resolve_unused_filename(&target_dir,
                Path::new(file_name).file_stem().and_then(|s| s.to_str()).unwrap_or(file_name),
                Path::new(file_name).extension().and_then(|s| s.to_str()).unwrap_or(""));
            let dest = target_dir.join(&new_name);
            safe_move_file(old_path, &dest)?;
            let new_managed = dest.to_string_lossy().to_string();
            drop(conn);
            update_doc_managed_path(id, &new_managed).await?;
        }
    }

    Ok(())
}

pub async fn update_doc_managed_path(id: i64, managed_path: &str) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    sqlx::query("UPDATE document_files SET managed_path = ?1 WHERE id = ?2")
        .bind(managed_path)
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("更新文件路径失败: {}", e))?;
    Ok(())
}

pub async fn move_doc_file(id: i64, new_root_id: i64) -> Result<(), String> {
    let doc = get_doc_file_by_id(id)
        .await?
        .ok_or("文件不存在".to_string())?;

    if doc.root_id == new_root_id {
        return Ok(());
    }

    let new_root = get_doc_root_by_id(new_root_id)
        .await?
        .ok_or("目标根目录不存在".to_string())?;

    let mut conn = open_docs_db().await?;

    if doc.storage_mode == "repo" {
        let old_path = Path::new(&doc.managed_path);
        if old_path.exists() {
            let file_name = old_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&doc.file_name);

            let target_dir = if let Some(ref cat_name) = doc.category_name {
                if cat_name == "未分类" {
                    Path::new(&new_root.root_path).to_path_buf()
                } else {
                    Path::new(&new_root.root_path).join(cat_name)
                }
            } else {
                Path::new(&new_root.root_path).to_path_buf()
            };

            fs::create_dir_all(&target_dir)
                .map_err(|e| format!("创建目标目录失败: {}", e))?;

            let new_path = resolve_unused_filename(&target_dir, file_name, &doc.file_ext);
            let dest = target_dir.join(&new_path);

            safe_move_file(old_path, &dest)
                .map_err(|e| format!("移动文件失败: {}", e))?;

            let new_managed_path = dest.to_string_lossy().to_string();
            sqlx::query("UPDATE document_files SET managed_path = ?1, root_id = ?2 WHERE id = ?3")
                .bind(&new_managed_path)
                .bind(new_root_id)
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("更新文件记录失败: {}", e))?;
        } else {
            sqlx::query("UPDATE document_files SET root_id = ?1 WHERE id = ?2")
                .bind(new_root_id)
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("更新文件记录失败: {}", e))?;
        }
    } else {
        sqlx::query("UPDATE document_files SET root_id = ?1 WHERE id = ?2")
            .bind(new_root_id)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新文件记录失败: {}", e))?;
    }

    Ok(())
}

pub async fn reorder_doc_files(ids: Vec<i64>) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;
    for (idx, id) in ids.iter().enumerate() {
        sqlx::query::<Sqlite>("UPDATE document_files SET sort_order = ?1 WHERE id = ?2")
            .bind(idx as i64)
            .bind(*id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("更新文件顺序失败: {}", e))?;
    }
    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))
}

pub async fn get_doc_root_by_id(id: i64) -> Result<Option<DocRoot>, String> {
    let mut conn = open_docs_db().await?;
    let row = sqlx::query("SELECT id, name, root_path, created_at FROM document_roots WHERE id = ?1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| format!("查询根目录失败: {}", e))?;

    Ok(row.map(|r| DocRoot {
        id: r.try_get::<i64, _>(0).unwrap_or(0),
        name: r.try_get::<String, _>(1).unwrap_or_default(),
        root_path: r.try_get::<String, _>(2).unwrap_or_default(),
        created_at: r.try_get::<i64, _>(3).unwrap_or(0),
    }))
}

pub async fn get_managed_paths_for_root(root_id: i64) -> Result<std::collections::HashSet<String>, String> {
    let mut conn = open_docs_db().await?;
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT managed_path FROM document_files WHERE root_id = ?1",
    )
        .bind(root_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("查询管理路径失败: {}", e))?;
    Ok(rows.into_iter().collect())
}

pub async fn get_doc_file_by_id(id: i64) -> Result<Option<DocFile>, String> {
    let mut conn = open_docs_db().await?;
    let row = sqlx::query(
        "SELECT df.id, df.root_id, df.title, df.file_name, df.file_ext, df.file_size, df.file_hash,
                df.category_id, c.name as category_name, df.tags, df.notes, df.content_text,
                df.source_path, df.managed_path, df.storage_mode, df.is_missing, df.added_at, df.file_modified, df.visit_count
         FROM document_files df
         LEFT JOIN document_categories c ON df.category_id = c.id
         WHERE df.id = ?1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| format!("查询文件失败: {}", e))?;

    Ok(row.map(|r| row_to_doc_file(&r)))
}

pub async fn increment_visit_count(id: i64) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    sqlx::query("UPDATE document_files SET visit_count = visit_count + 1 WHERE id = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("更新访问计数失败: {}", e))?;
    Ok(())
}

pub async fn mark_doc_missing(id: i64) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    sqlx::query("UPDATE document_files SET is_missing = 1 WHERE id = ?1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("标记文件缺失失败: {}", e))?;
    Ok(())
}

pub async fn doc_exists_by_hash(file_hash: &str, root_id: i64) -> Result<bool, String> {
    let mut conn = open_docs_db().await?;
    let count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM document_files WHERE file_hash = ?1 AND root_id = ?2 AND is_missing = 0",
    )
    .bind(file_hash)
    .bind(root_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| format!("查询文件重复失败: {}", e))?;
    Ok(count > 0)
}

fn row_to_doc_file(row: &sqlx::sqlite::SqliteRow) -> DocFile {
    DocFile {
        id: row.try_get::<i64, _>(0).unwrap_or(0),
        root_id: row.try_get::<i64, _>(1).unwrap_or(0),
        title: row.try_get::<String, _>(2).unwrap_or_default(),
        file_name: row.try_get::<String, _>(3).unwrap_or_default(),
        file_ext: row.try_get::<String, _>(4).unwrap_or_default(),
        file_size: row.try_get::<i64, _>(5).unwrap_or(0),
        file_hash: row.try_get::<String, _>(6).unwrap_or_default(),
        category_id: row.try_get::<Option<i64>, _>(7).unwrap_or(None),
        category_name: row.try_get::<Option<String>, _>(8).unwrap_or(None),
        tags: row.try_get::<String, _>(9).unwrap_or_default(),
        notes: row.try_get::<String, _>(10).unwrap_or_default(),
        content_text: row.try_get::<String, _>(11).unwrap_or_default(),
        source_path: row.try_get::<String, _>(12).unwrap_or_default(),
        managed_path: row.try_get::<String, _>(13).unwrap_or_default(),
        storage_mode: row.try_get::<String, _>(14).unwrap_or_default(),
        is_missing: row.try_get::<i64, _>(15).unwrap_or(0) != 0,
        added_at: row.try_get::<i64, _>(16).unwrap_or(0),
        file_modified: row.try_get::<i64, _>(17).unwrap_or(0),
        visit_count: row.try_get::<i64, _>(18).unwrap_or(0),
    }
}

pub async fn get_doc_page(
    offset: i64,
    limit: i64,
    category_id: Option<i64>,
    root_id: Option<i64>,
    keyword: Option<String>,
    file_ext: Option<String>,
) -> Result<DocPageData, String> {
    let mut conn = open_docs_db().await?;
    let effective_limit = limit.clamp(1, 200);
    let keyword_val = keyword
        .as_ref()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());

    let order_clause = "ORDER BY df.sort_order ASC, df.added_at DESC";

    let fts_query = keyword_val.as_ref().map(|k| build_fts_query(k)).filter(|q| !q.is_empty());
    let fts_enabled = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'document_files_fts'",
    )
    .fetch_one(&mut *conn)
    .await
    .unwrap_or(0) > 0;

    let use_fts = fts_enabled && fts_query.is_some();

    let (total, rows) = if use_fts {
        let fts = fts_query.unwrap();
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM document_files df
             WHERE (?1 IS NULL OR (?1 = -1 AND df.category_id IS NULL) OR df.category_id = ?1)
               AND (?2 IS NULL OR df.root_id = ?2)
               AND (?3 IS NULL OR LOWER(df.file_ext) = ?3)
               AND EXISTS (SELECT 1 FROM document_files_fts WHERE document_files_fts.rowid = df.id AND document_files_fts MATCH ?4)",
        )
        .bind(category_id)
        .bind(root_id)
        .bind(file_ext.as_deref().filter(|v| !v.is_empty()))
        .bind(&fts)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("查询总数失败: {}", e))?;

        let sql = format!(
            "SELECT df.id, df.root_id, df.title, df.file_name, df.file_ext, df.file_size, df.file_hash,
                    df.category_id, c.name as category_name, df.tags, df.notes, df.content_text,
                    df.source_path, df.managed_path, df.storage_mode, df.is_missing, df.added_at, df.file_modified, df.visit_count
             FROM document_files df
             LEFT JOIN document_categories c ON df.category_id = c.id
             WHERE (?1 IS NULL OR (?1 = -1 AND df.category_id IS NULL) OR df.category_id = ?1)
               AND (?2 IS NULL OR df.root_id = ?2)
               AND (?3 IS NULL OR LOWER(df.file_ext) = ?3)
               AND EXISTS (SELECT 1 FROM document_files_fts WHERE document_files_fts.rowid = df.id AND document_files_fts MATCH ?4)
             {}
             LIMIT ?5 OFFSET ?6",
            order_clause
        );
        let rows = sqlx::query::<Sqlite>(&sql)
        .bind(category_id)
        .bind(root_id)
        .bind(file_ext.as_deref().filter(|v| !v.is_empty()))
        .bind(&fts)
        .bind(effective_limit)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("查询文件列表失败: {}", e))?;

        (total, rows)
    } else if keyword_val.is_some() {
        let kw = keyword_val.as_deref().unwrap();
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM document_files df
             WHERE (?1 IS NULL OR (?1 = -1 AND df.category_id IS NULL) OR df.category_id = ?1)
               AND (?2 IS NULL OR df.root_id = ?2)
               AND (?3 IS NULL OR LOWER(df.file_ext) = ?3)
               AND (df.file_name LIKE '%' || ?4 || '%' OR df.title LIKE '%' || ?4 || '%' OR df.tags LIKE '%' || ?4 || '%' OR df.notes LIKE '%' || ?4 || '%')",
        )
        .bind(category_id)
        .bind(root_id)
        .bind(file_ext.as_deref().filter(|v| !v.is_empty()))
        .bind(kw)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("查询总数失败: {}", e))?;

        let sql = format!(
            "SELECT df.id, df.root_id, df.title, df.file_name, df.file_ext, df.file_size, df.file_hash,
                    df.category_id, c.name as category_name, df.tags, df.notes, df.content_text,
                    df.source_path, df.managed_path, df.storage_mode, df.is_missing, df.added_at, df.file_modified, df.visit_count
             FROM document_files df
             LEFT JOIN document_categories c ON df.category_id = c.id
             WHERE (?1 IS NULL OR (?1 = -1 AND df.category_id IS NULL) OR df.category_id = ?1)
               AND (?2 IS NULL OR df.root_id = ?2)
               AND (?3 IS NULL OR LOWER(df.file_ext) = ?3)
               AND (df.file_name LIKE '%' || ?4 || '%' OR df.title LIKE '%' || ?4 || '%' OR df.tags LIKE '%' || ?4 || '%' OR df.notes LIKE '%' || ?4 || '%')
             {}
             LIMIT ?5 OFFSET ?6",
            order_clause
        );
        let rows = sqlx::query::<Sqlite>(&sql)
        .bind(category_id)
        .bind(root_id)
        .bind(file_ext.as_deref().filter(|v| !v.is_empty()))
        .bind(kw)
        .bind(effective_limit)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("查询文件列表失败: {}", e))?;

        (total, rows)
    } else {
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM document_files df
             WHERE (?1 IS NULL OR (?1 = -1 AND df.category_id IS NULL) OR df.category_id = ?1)
               AND (?2 IS NULL OR df.root_id = ?2)
               AND (?3 IS NULL OR LOWER(df.file_ext) = ?3)",
        )
        .bind(category_id)
        .bind(root_id)
        .bind(file_ext.as_deref().filter(|v| !v.is_empty()))
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("查询总数失败: {}", e))?;

        let sql = format!(
            "SELECT df.id, df.root_id, df.title, df.file_name, df.file_ext, df.file_size, df.file_hash,
                    df.category_id, c.name as category_name, df.tags, df.notes, df.content_text,
                    df.source_path, df.managed_path, df.storage_mode, df.is_missing, df.added_at, df.file_modified, df.visit_count
             FROM document_files df
             LEFT JOIN document_categories c ON df.category_id = c.id
             WHERE (?1 IS NULL OR (?1 = -1 AND df.category_id IS NULL) OR df.category_id = ?1)
               AND (?2 IS NULL OR df.root_id = ?2)
               AND (?3 IS NULL OR LOWER(df.file_ext) = ?3)
             {}
             LIMIT ?4 OFFSET ?5",
            order_clause
        );
        let rows = sqlx::query::<Sqlite>(&sql)
        .bind(category_id)
        .bind(root_id)
        .bind(file_ext.as_deref().filter(|v| !v.is_empty()))
        .bind(effective_limit)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("查询文件列表失败: {}", e))?;

        (total, rows)
    };

    let items: Vec<DocFile> = rows.iter().map(|r| row_to_doc_file(r)).collect();

    Ok(DocPageData {
        total,
        offset,
        limit: effective_limit,
        items,
    })
}

pub async fn get_doc_stats(root_id: Option<i64>) -> Result<DocStats, String> {
    let mut conn = open_docs_db().await?;

    let total_files: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM document_files WHERE (?1 IS NULL OR root_id = ?1)",
    )
    .bind(root_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| format!("查询文件总数失败: {}", e))?;

    let total_size: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(file_size), 0) FROM document_files WHERE (?1 IS NULL OR root_id = ?1)",
    )
    .bind(root_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| format!("查询总大小失败: {}", e))?;

    let missing_files: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM document_files WHERE is_missing = 1 AND (?1 IS NULL OR root_id = ?1)",
    )
    .bind(root_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| format!("查询缺失文件数失败: {}", e))?;

    let rows = sqlx::query::<Sqlite>(
        "SELECT df.category_id, COALESCE(c.name, '未分类') as category_name, COUNT(*) as cnt
         FROM document_files df
         LEFT JOIN document_categories c ON df.category_id = c.id
         WHERE (?1 IS NULL OR df.root_id = ?1)
         GROUP BY df.category_id
         ORDER BY cnt DESC",
    )
    .bind(root_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("查询分类统计失败: {}", e))?;

    let category_counts: Vec<CategoryCount> = rows
        .iter()
        .map(|row| CategoryCount {
            category_id: row.try_get::<Option<i64>, _>(0).unwrap_or(None),
            category_name: row.try_get::<String, _>(1).unwrap_or_default(),
            count: row.try_get::<i64, _>(2).unwrap_or(0),
        })
        .collect();

    Ok(DocStats {
        total_files,
        total_size,
        missing_files,
        category_counts,
    })
}

fn build_fts_query(keyword: &str) -> String {
    let trimmed = keyword.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let tokens: Vec<String> = trimmed
        .split_whitespace()
        .map(|token| {
            let escaped = token.replace('\\', "\\\\").replace('"', "");
            format!("\"{}\"*", escaped)
        })
        .filter(|t| !t.is_empty() && t.len() > 3)
        .collect();
    if tokens.is_empty() {
        let escaped = trimmed.replace('\\', "\\\\").replace('"', "");
        if escaped.is_empty() {
            return String::new();
        }
        format!("\"{}\"*", escaped)
    } else {
        tokens.join(" AND ")
    }
}

pub fn compute_file_hash(path: &std::path::Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("读取文件失败: {}", e))?;
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取文件失败: {}", e))?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    let hash = hasher.digest();
    Ok(format!("{:016x}", hash))
}

pub fn safe_move_file(src: &Path, dest: &Path) -> Result<(), String> {
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    let options = fs_extra::file::CopyOptions::new().overwrite(true);
    fs_extra::file::move_file(src, dest, &options)
        .map(|_| ())
        .map_err(|e| format!("移动文件失败: {}", e))
}

pub fn resolve_unused_filename(dir: &std::path::Path, base_name: &str, ext: &str) -> String {
    let name = format!("{}.{}", base_name, ext);
    if !dir.join(&name).exists() {
        return name;
    }
    for i in 1..100 {
        let name = format!("{} ({}).{}", base_name, i, ext);
        if !dir.join(&name).exists() {
            return name;
        }
    }
    format!("{} ({}).{}", base_name, std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs(), ext)
}

pub async fn create_import_history(
    root_id: i64,
    category_id: Option<i64>,
    storage_mode: &str,
    source_dir: &str,
    target_dir: &str,
    file_count: i64,
) -> Result<i64, String> {
    let mut conn = open_docs_db().await?;
    let now = now_unix_ms();
    sqlx::query::<Sqlite>(
        "INSERT INTO document_imports (root_id, category_id, storage_mode, source_dir, target_dir, file_count, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(root_id)
    .bind(category_id)
    .bind(storage_mode)
    .bind(source_dir)
    .bind(target_dir)
    .bind(file_count)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("创建导入历史失败: {}", e))?;
    sqlx::query_scalar::<_, i64>("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("获取历史ID失败: {}", e))
}

pub async fn link_import_item(import_id: i64, doc_file_id: i64, source_path: &str, managed_path: &str) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    sqlx::query::<Sqlite>(
        "INSERT OR IGNORE INTO document_import_items (import_id, doc_file_id, source_path, managed_path) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(import_id)
    .bind(doc_file_id)
    .bind(source_path)
    .bind(managed_path)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("关联导入记录失败: {}", e))?;
    Ok(())
}

pub async fn get_import_history(limit: i64) -> Result<Vec<ImportHistory>, String> {
    let mut conn = open_docs_db().await?;
    let rows = sqlx::query::<Sqlite>(
        "SELECT di.id, di.root_id, di.category_id, COALESCE(dc.name, '未分类') as category_name, di.storage_mode, di.source_dir, di.target_dir, di.file_count, di.created_at
         FROM document_imports di
         LEFT JOIN document_categories dc ON di.category_id = dc.id
         ORDER BY di.created_at DESC
         LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("查询导入历史失败: {}", e))?;
    Ok(rows.iter().map(|r| ImportHistory {
        id: r.try_get::<i64, _>(0).unwrap_or(0),
        root_id: r.try_get::<i64, _>(1).unwrap_or(0),
        category_id: r.try_get::<Option<i64>, _>(2).unwrap_or(None),
        category_name: r.try_get::<Option<String>, _>(3).unwrap_or(None),
        storage_mode: r.try_get::<String, _>(4).unwrap_or_default(),
        source_dir: r.try_get::<String, _>(5).unwrap_or_default(),
        target_dir: r.try_get::<String, _>(6).unwrap_or_default(),
        file_count: r.try_get::<i64, _>(7).unwrap_or(0),
        created_at: r.try_get::<i64, _>(8).unwrap_or(0),
    }).collect())
}

pub async fn undo_import(import_id: i64) -> Result<Vec<String>, String> {
    let mut conn = open_docs_db().await?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|e| format!("创建事务失败: {}", e))?;

    let items = sqlx::query::<Sqlite>(
        "SELECT dii.doc_file_id, dii.source_path, dii.managed_path, di.storage_mode
         FROM document_import_items dii
         JOIN document_imports di ON di.id = dii.import_id
         WHERE dii.import_id = ?1",
    )
    .bind(import_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| format!("查询导入项失败: {}", e))?;

    let mut errors = Vec::new();
    for row in &items {
        let doc_id: i64 = row.try_get(0).unwrap_or(0);
        let source: String = row.try_get(1).unwrap_or_default();
        let managed: String = row.try_get(2).unwrap_or_default();
        let mode: String = row.try_get(3).unwrap_or_default();

        if mode == "repo" && !managed.is_empty() {
            let managed_path = std::path::Path::new(&managed);
            let source_path = std::path::Path::new(&source);
            if managed_path.exists() {
                if let Some(parent) = source_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = safe_move_file(managed_path, source_path) {
                    errors.push(format!("回退失败 {}: {}", doc_id, e));
                    continue;
                }
            }
        }

        sqlx::query::<Sqlite>("DELETE FROM document_files WHERE id = ?1")
            .bind(doc_id)
            .execute(&mut *tx)
            .await
            .ok();
        sqlx::query::<Sqlite>("DELETE FROM document_files_fts WHERE rowid = ?1")
            .bind(doc_id)
            .execute(&mut *tx)
            .await
            .ok();
    }

    sqlx::query::<Sqlite>("DELETE FROM document_import_items WHERE import_id = ?1")
        .bind(import_id)
        .execute(&mut *tx)
        .await
        .ok();
    sqlx::query::<Sqlite>("DELETE FROM document_imports WHERE id = ?1")
        .bind(import_id)
        .execute(&mut *tx)
        .await
        .ok();

    tx.commit()
        .await
        .map_err(|e| format!("提交事务失败: {}", e))?;

    Ok(errors)
}

pub async fn undo_import_item(import_id: i64, doc_file_id: i64) -> Result<(), String> {
    let mut conn = open_docs_db().await?;
    let mut tx = conn.begin().await.map_err(|e| format!("创建事务失败: {}", e))?;

    let item = sqlx::query::<Sqlite>(
        "SELECT dii.source_path, dii.managed_path, di.storage_mode
         FROM document_import_items dii
         JOIN document_imports di ON di.id = dii.import_id
         WHERE dii.import_id = ?1 AND dii.doc_file_id = ?2",
    )
    .bind(import_id)
    .bind(doc_file_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| format!("查询导入项失败: {}", e))?
    .ok_or("导入项不存在".to_string())?;

    let source: String = item.try_get(0).unwrap_or_default();
    let managed: String = item.try_get(1).unwrap_or_default();
    let mode: String = item.try_get(2).unwrap_or_default();

    if mode == "repo" && !managed.is_empty() {
        let managed_path = std::path::Path::new(&managed);
        let source_path = std::path::Path::new(&source);
        if managed_path.exists() {
            if let Some(parent) = source_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            safe_move_file(managed_path, source_path)
                .map_err(|e| format!("回退失败: {}", e))?;
        }
    }

    sqlx::query::<Sqlite>("DELETE FROM document_files WHERE id = ?1")
        .bind(doc_file_id).execute(&mut *tx).await.ok();
    sqlx::query::<Sqlite>("DELETE FROM document_files_fts WHERE rowid = ?1")
        .bind(doc_file_id).execute(&mut *tx).await.ok();
    sqlx::query::<Sqlite>("DELETE FROM document_import_items WHERE import_id = ?1 AND doc_file_id = ?2")
        .bind(import_id).bind(doc_file_id).execute(&mut *tx).await.ok();

    let remaining: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM document_import_items WHERE import_id = ?1"
    ).bind(import_id).fetch_one(&mut *tx).await.unwrap_or(0);

    if remaining == 0 {
        sqlx::query::<Sqlite>("DELETE FROM document_imports WHERE id = ?1")
            .bind(import_id).execute(&mut *tx).await.ok();
    } else {
        sqlx::query::<Sqlite>("UPDATE document_imports SET file_count = ?1 WHERE id = ?2")
            .bind(remaining).bind(import_id).execute(&mut *tx).await.ok();
    }

    tx.commit().await.map_err(|e| format!("提交事务失败: {}", e))
}

pub async fn get_import_files(import_id: i64) -> Result<Vec<ImportFileItem>, String> {
    let mut conn = open_docs_db().await?;
    let rows = sqlx::query::<Sqlite>(
        "SELECT dii.doc_file_id, COALESCE(df.file_name, dii.managed_path) as file_name, dii.source_path, dii.managed_path
         FROM document_import_items dii
         LEFT JOIN document_files df ON df.id = dii.doc_file_id
         WHERE dii.import_id = ?1
         ORDER BY dii.doc_file_id",
    )
    .bind(import_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("查询导入文件列表失败: {}", e))?;
    Ok(rows.iter().map(|r| ImportFileItem {
        doc_file_id: r.try_get::<i64, _>(0).unwrap_or(0),
        file_name: r.try_get::<String, _>(1).unwrap_or_default(),
        source_path: r.try_get::<String, _>(2).unwrap_or_default(),
        managed_path: r.try_get::<String, _>(3).unwrap_or_default(),
    }).collect())
}
