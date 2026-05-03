use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{Row, SqliteConnection};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::OnceCell;

static LAUNCHER_DB_POOL: OnceCell<SqlitePool> = OnceCell::const_new();

fn get_launcher_db_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    path.pop();
    path.push("launcher");
    fs::create_dir_all(&path).ok();
    path.push("launcher.db");
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

async fn get_launcher_db_pool() -> Result<&'static SqlitePool, String> {
    LAUNCHER_DB_POOL
        .get_or_try_init(|| async {
            let db_path = get_launcher_db_path();
            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("创建启动器数据库目录失败: {}", e))?;
            }
            let pool = SqlitePoolOptions::new()
                .max_connections(3)
                .connect_with(db_options(&db_path))
                .await
                .map_err(|e| format!("打开启动器数据库连接池失败: {}", e))?;

            let mut conn = pool
                .acquire()
                .await
                .map_err(|e| format!("获取数据库连接失败: {}", e))?;
            ensure_launcher_db_schema(&mut conn).await?;

            Ok(pool)
        })
        .await
}

async fn ensure_launcher_db_schema(conn: &mut SqliteConnection) -> Result<(), String> {
    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS launcher_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS launcher_categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            icon TEXT NOT NULL DEFAULT '',
            position INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS launcher_category_apps (
            category_id TEXT NOT NULL,
            app_id TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (category_id, app_id)
        );

        CREATE TABLE IF NOT EXISTS launcher_app_category_map (
            app_id TEXT PRIMARY KEY,
            category_id TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS launcher_custom_commands (
            id TEXT PRIMARY KEY,
            prefix TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            description TEXT,
            icon TEXT NOT NULL DEFAULT '',
            command_type TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS launcher_apps (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            path TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT '',
            app_type TEXT NOT NULL DEFAULT '',
            icon_base64 TEXT,
            action TEXT NOT NULL DEFAULT 'launch_app',
            sort_order INTEGER NOT NULL DEFAULT 0,
            source TEXT NOT NULL DEFAULT 'scan'
        );

        CREATE TABLE IF NOT EXISTS launcher_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("初始化启动器数据库失败: {}", e))?;

    let _ = sqlx::query("ALTER TABLE launcher_apps ADD COLUMN source TEXT NOT NULL DEFAULT 'scan'")
        .execute(&mut *conn)
        .await;

    Ok(())
}

pub async fn open_launcher_db_conn() -> Result<sqlx::pool::PoolConnection<sqlx::Sqlite>, String> {
    let pool = get_launcher_db_pool().await?;
    pool.acquire()
        .await
        .map_err(|e| format!("获取启动器数据库连接失败: {}", e))
}

// ─── Migration check ───

pub async fn is_db_empty() -> Result<bool, String> {
    let mut conn = open_launcher_db_conn().await?;
    let row = sqlx::query(
        "SELECT (SELECT COUNT(*) FROM launcher_config) + (SELECT COUNT(*) FROM launcher_categories) + (SELECT COUNT(*) FROM launcher_custom_commands) AS total"
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| format!("检查数据库状态失败: {}", e))?;
    let total: i64 = row.get("total");
    Ok(total == 0)
}

// ─── Config key-value operations ───

pub async fn get_config_value(key: &str) -> Result<Option<String>, String> {
    let mut conn = open_launcher_db_conn().await?;
    let row = sqlx::query("SELECT value FROM launcher_config WHERE key = ?")
        .bind(key)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| format!("读取配置失败: {}", e))?;
    Ok(row.map(|r| r.get::<String, _>("value")))
}

pub async fn set_config_value(key: &str, value: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("INSERT OR REPLACE INTO launcher_config (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("保存配置失败: {}", e))?;
    Ok(())
}

// ─── Categories CRUD ───

pub async fn load_categories() -> Result<Vec<CategoryRow>, String> {
    let mut conn = open_launcher_db_conn().await?;
    let rows = sqlx::query("SELECT id, name, icon, position FROM launcher_categories ORDER BY position ASC, id ASC")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("读取分类列表失败: {}", e))?;
    Ok(rows
        .into_iter()
        .map(|r| CategoryRow {
            id: r.get("id"),
            name: r.get("name"),
            icon: r.get("icon"),
            position: r.get::<i64, _>("position") as i32,
        })
        .collect())
}

pub async fn load_category_app_ids(category_id: &str) -> Result<Vec<String>, String> {
    let mut conn = open_launcher_db_conn().await?;
    let rows = sqlx::query(
        "SELECT app_id FROM launcher_category_apps WHERE category_id = ? ORDER BY position ASC"
    )
    .bind(category_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("读取分类应用失败: {}", e))?;
    Ok(rows.into_iter().map(|r| r.get::<String, _>("app_id")).collect())
}

pub async fn upsert_category(id: &str, name: &str, icon: &str, position: i32) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query(
        "INSERT OR REPLACE INTO launcher_categories (id, name, icon, position) VALUES (?, ?, ?, ?)"
    )
    .bind(id)
    .bind(name)
    .bind(icon)
    .bind(position)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("保存分类失败: {}", e))?;
    Ok(())
}

pub async fn delete_category(category_id: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("DELETE FROM launcher_categories WHERE id = ?")
        .bind(category_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除分类失败: {}", e))?;
    sqlx::query("DELETE FROM launcher_category_apps WHERE category_id = ?")
        .bind(category_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除分类应用关联失败: {}", e))?;
    Ok(())
}

pub async fn update_category_name(category_id: &str, new_name: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("UPDATE launcher_categories SET name = ? WHERE id = ?")
        .bind(new_name)
        .bind(category_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("重命名分类失败: {}", e))?;
    Ok(())
}

pub async fn update_category_icon(category_id: &str, icon: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("UPDATE launcher_categories SET icon = ? WHERE id = ?")
        .bind(icon)
        .bind(category_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("更新分类图标失败: {}", e))?;
    Ok(())
}

pub async fn sync_category_positions(ids: &[String]) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE launcher_categories SET position = ? WHERE id = ?")
            .bind(i as i32)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新分类排序失败: {}", e))?;
    }
    Ok(())
}

pub async fn sync_category_apps(category_id: &str, app_ids: &[String]) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("DELETE FROM launcher_category_apps WHERE category_id = ?")
        .bind(category_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("清空分类应用关联失败: {}", e))?;
    for (i, app_id) in app_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO launcher_category_apps (category_id, app_id, position) VALUES (?, ?, ?)"
        )
        .bind(category_id)
        .bind(app_id)
        .bind(i as i32)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("保存分类应用关联失败: {}", e))?;
    }
    Ok(())
}

// ─── App Category Map CRUD ───

pub async fn load_app_category_map() -> Result<Vec<(String, String)>, String> {
    let mut conn = open_launcher_db_conn().await?;
    let rows = sqlx::query("SELECT app_id, category_id FROM launcher_app_category_map")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("读取应用分类映射失败: {}", e))?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<String, _>("app_id"), r.get::<String, _>("category_id")))
        .collect())
}

pub async fn set_app_category_map(app_id: &str, category_id: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query(
        "INSERT OR REPLACE INTO launcher_app_category_map (app_id, category_id) VALUES (?, ?)"
    )
    .bind(app_id)
    .bind(category_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("设置应用分类失败: {}", e))?;
    Ok(())
}

pub async fn remove_app_category_map(app_id: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("DELETE FROM launcher_app_category_map WHERE app_id = ?")
        .bind(app_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除应用分类映射失败: {}", e))?;
    Ok(())
}

pub async fn clear_category_app_map_by_category(category_id: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("DELETE FROM launcher_app_category_map WHERE category_id = ?")
        .bind(category_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("清理分类映射失败: {}", e))?;
    Ok(())
}

// ─── Custom Commands CRUD ───

pub async fn load_custom_commands() -> Result<Vec<CustomCommandRow>, String> {
    let mut conn = open_launcher_db_conn().await?;
    let rows = sqlx::query(
        "SELECT id, prefix, title, description, icon, command_type, enabled, created_at FROM launcher_custom_commands ORDER BY created_at ASC"
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("读取自定义命令失败: {}", e))?;
    Ok(rows
        .into_iter()
        .map(|r| CustomCommandRow {
            id: r.get("id"),
            prefix: r.get("prefix"),
            title: r.get("title"),
            description: r.get("description"),
            icon: r.get("icon"),
            command_type: r.get("command_type"),
            enabled: r.get::<i64, _>("enabled") != 0,
            created_at: r.get::<i64, _>("created_at"),
        })
        .collect())
}

pub async fn insert_custom_command(cmd: &CustomCommandRow) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query(
        "INSERT INTO launcher_custom_commands (id, prefix, title, description, icon, command_type, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&cmd.id)
    .bind(&cmd.prefix)
    .bind(&cmd.title)
    .bind(&cmd.description)
    .bind(&cmd.icon)
    .bind(&cmd.command_type)
    .bind(cmd.enabled as i64)
    .bind(cmd.created_at)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("添加自定义命令失败: {}", e))?;
    Ok(())
}

pub async fn delete_custom_command(command_id: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("DELETE FROM launcher_custom_commands WHERE id = ?")
        .bind(command_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除自定义命令失败: {}", e))?;
    Ok(())
}

pub async fn update_custom_command_fields(
    command_id: &str,
    prefix: Option<&str>,
    title: Option<&str>,
    description: Option<&str>,
    icon: Option<&str>,
    command_type: Option<&str>,
    enabled: Option<bool>,
) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    if let Some(p) = prefix {
        sqlx::query("UPDATE launcher_custom_commands SET prefix = ? WHERE id = ?")
            .bind(p)
            .bind(command_id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新命令前缀失败: {}", e))?;
    }
    if let Some(t) = title {
        sqlx::query("UPDATE launcher_custom_commands SET title = ? WHERE id = ?")
            .bind(t)
            .bind(command_id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新命令标题失败: {}", e))?;
    }
    if let Some(d) = description {
        sqlx::query("UPDATE launcher_custom_commands SET description = ? WHERE id = ?")
            .bind(d)
            .bind(command_id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新命令描述失败: {}", e))?;
    }
    if let Some(i) = icon {
        sqlx::query("UPDATE launcher_custom_commands SET icon = ? WHERE id = ?")
            .bind(i)
            .bind(command_id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新命令图标失败: {}", e))?;
    }
    if let Some(ct) = command_type {
        sqlx::query("UPDATE launcher_custom_commands SET command_type = ? WHERE id = ?")
            .bind(ct)
            .bind(command_id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新命令类型失败: {}", e))?;
    }
    if let Some(e) = enabled {
        sqlx::query("UPDATE launcher_custom_commands SET enabled = ? WHERE id = ?")
            .bind(e as i64)
            .bind(command_id)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("更新命令状态失败: {}", e))?;
    }
    Ok(())
}

pub async fn toggle_custom_command_enabled(command_id: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("UPDATE launcher_custom_commands SET enabled = 1 - enabled WHERE id = ?")
        .bind(command_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("切换命令状态失败: {}", e))?;
    Ok(())
}

pub async fn check_prefix_exists(prefix: &str, exclude_id: Option<&str>) -> Result<bool, String> {
    let mut conn = open_launcher_db_conn().await?;
    let row = if let Some(ex_id) = exclude_id {
        sqlx::query("SELECT COUNT(*) as cnt FROM launcher_custom_commands WHERE prefix = ? AND id != ?")
            .bind(prefix)
            .bind(ex_id)
            .fetch_one(&mut *conn)
            .await
    } else {
        sqlx::query("SELECT COUNT(*) as cnt FROM launcher_custom_commands WHERE prefix = ?")
            .bind(prefix)
            .fetch_one(&mut *conn)
            .await
    }
    .map_err(|e| format!("检查前缀失败: {}", e))?;
    let count: i64 = row.get("cnt");
    Ok(count > 0)
}

// ─── Apps CRUD ───

pub async fn load_all_apps() -> Result<Vec<AppRow>, String> {
    let mut conn = open_launcher_db_conn().await?;
    let rows = sqlx::query(
        "SELECT id, title, path, category, app_type, icon_base64, action, sort_order, source FROM launcher_apps ORDER BY app_type ASC, title ASC"
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("读取应用列表失败: {}", e))?;
    Ok(rows
        .into_iter()
        .map(|r| AppRow {
            id: r.get("id"),
            title: r.get("title"),
            path: r.get("path"),
            category: r.get("category"),
            app_type: r.get("app_type"),
            icon_base64: r.get("icon_base64"),
            action: r.get("action"),
            sort_order: r.get::<i64, _>("sort_order") as i32,
            source: r.get("source"),
        })
        .collect())
}

pub async fn replace_scan_apps(apps: &[AppRow]) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("DELETE FROM launcher_apps WHERE source = 'scan'")
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("清理扫描应用失败: {}", e))?;

    for app in apps {
        sqlx::query(
            "INSERT OR REPLACE INTO launcher_apps (id, title, path, category, app_type, icon_base64, action, sort_order, source) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'scan')"
        )
        .bind(&app.id)
        .bind(&app.title)
        .bind(&app.path)
        .bind(&app.category)
        .bind(&app.app_type)
        .bind(&app.icon_base64)
        .bind(&app.action)
        .bind(app.sort_order)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("保存应用失败: {}", e))?;
    }
    Ok(())
}

pub async fn insert_manual_app(id: &str, title: &str, path: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query(
        "INSERT OR REPLACE INTO launcher_apps (id, title, path, category, app_type, icon_base64, action, sort_order, source) VALUES (?, ?, ?, '', 'third_party', NULL, 'launch_app', 0, 'manual')"
    )
    .bind(id)
    .bind(title)
    .bind(path)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("添加手动应用失败: {}", e))?;
    Ok(())
}

pub async fn delete_app(app_id: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("DELETE FROM launcher_apps WHERE id = ?")
        .bind(app_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("删除应用失败: {}", e))?;
    Ok(())
}

pub async fn update_app_icon(app_id: &str, icon_base64: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("UPDATE launcher_apps SET icon_base64 = ? WHERE id = ?")
        .bind(icon_base64)
        .bind(app_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("更新应用图标失败: {}", e))?;
    Ok(())
}

pub async fn update_app_icon_by_path(path: &str, icon_base64: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("UPDATE launcher_apps SET icon_base64 = ? WHERE path = ?")
        .bind(icon_base64)
        .bind(path)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("批量更新图标失败: {}", e))?;
    Ok(())
}

pub async fn update_app_sort_order(app_id: &str, sort_order: i32) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("UPDATE launcher_apps SET sort_order = ? WHERE id = ?")
        .bind(sort_order)
        .bind(app_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("更新应用排序失败: {}", e))?;
    Ok(())
}

pub async fn get_app_count() -> Result<i64, String> {
    let mut conn = open_launcher_db_conn().await?;
    let row = sqlx::query("SELECT COUNT(*) as cnt FROM launcher_apps")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("查询应用计数失败: {}", e))?;
    Ok(row.get("cnt"))
}

// ─── Meta CRUD ───

pub async fn get_meta(key: &str) -> Result<Option<String>, String> {
    let mut conn = open_launcher_db_conn().await?;
    let row = sqlx::query("SELECT value FROM launcher_meta WHERE key = ?")
        .bind(key)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| format!("读取元数据失败: {}", e))?;
    Ok(row.map(|r| r.get("value")))
}

pub async fn set_meta(key: &str, value: &str) -> Result<(), String> {
    let mut conn = open_launcher_db_conn().await?;
    sqlx::query("INSERT OR REPLACE INTO launcher_meta (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("保存元数据失败: {}", e))?;
    Ok(())
}

// ─── Row types for use by launcher_config.rs and app_store.rs ───

#[derive(Debug, Clone)]
pub struct CategoryRow {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub position: i32,
}

#[derive(Debug, Clone)]
pub struct CustomCommandRow {
    pub id: String,
    pub prefix: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: String,
    pub command_type: String,
    pub enabled: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AppRow {
    pub id: String,
    pub title: String,
    pub path: String,
    pub category: String,
    pub app_type: String,
    pub icon_base64: Option<String>,
    pub action: String,
    pub sort_order: i32,
    pub source: String,
}
