use crate::core::error_codes::AppErrorKind;
use sqlx::{QueryBuilder, Sqlite, Transaction};
use std::path::PathBuf;
use std::time::Duration;

/// 验证标识符（表名、列名）是否安全
/// 只允许字母、数字和下划线，且不能为空
fn validate_identifier(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Identifier cannot be empty".to_string());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(format!("Invalid identifier: {}", name));
    }
    // 防止纯数字开头（SQLite惯例）
    if name.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return Err(format!("Identifier cannot start with a digit: {}", name));
    }
    Ok(())
}

/// FTS5 特殊字符转义
pub fn escape_fts_token(token: &str) -> String {
    let mut s = String::with_capacity(token.len());
    for ch in token.chars() {
        match ch {
            '\\' => s.push_str("\\\\"),
            '"' => s.push_str("\"\""),
            '(' | ')' | '+' | '-' | '*' | ':' | '^' => {
                s.push('\\');
                s.push(ch);
            }
            _ => s.push(ch),
        }
    }
    s
}

/// 构建 FTS5 搜索查询
/// 使用 AND 连接多个关键词（默认行为）
pub fn build_fts_query_and(keyword: &str) -> String {
    build_fts_query_internal(keyword, " AND ")
}

/// 构建 FTS5 搜索查询
/// 使用空格连接多个关键词（用于文档搜索）
pub fn build_fts_query_space(keyword: &str) -> String {
    build_fts_query_internal(keyword, " ")
}

fn build_fts_query_internal(keyword: &str, separator: &str) -> String {
    let trimmed = keyword.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let tokens: Vec<String> = trimmed
        .split_whitespace()
        .map(|token| {
            let escaped = escape_fts_token(token.trim());
            if !escaped.is_empty() {
                format!("\"{}\"*", escaped)
            } else {
                String::new()
            }
        })
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        let escaped = escape_fts_token(trimmed);
        if escaped.is_empty() {
            return String::new();
        }
        format!("\"{}\"*", escaped)
    } else {
        tokens.join(separator)
    }
}

/// 重置临时文本表
pub async fn reset_temp_text_table(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
    column_name: &str,
) -> Result<(), String> {
    validate_identifier(table_name)?;
    validate_identifier(column_name)?;
    
    let create_sql = format!(
        "CREATE TEMP TABLE IF NOT EXISTS {} ({} TEXT PRIMARY KEY)",
        table_name, column_name
    );
    sqlx::query(&create_sql)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    let clear_sql = format!("DELETE FROM {}", table_name);
    sqlx::query(&clear_sql)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

/// 填充临时文本表
pub async fn fill_temp_text_table(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
    column_name: &str,
    values: &[String],
) -> Result<(), String> {
    validate_identifier(table_name)?;
    validate_identifier(column_name)?;
    
    if values.is_empty() {
        return Ok(());
    }
    for chunk in values.chunks(500) {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(format!(
            "INSERT OR IGNORE INTO {} ({}) ",
            table_name, column_name
        ));
        query_builder.push_values(chunk, |mut b, val| {
            b.push_bind(val);
        });
        let query = query_builder.build();
        query
            .execute(&mut **tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }
    Ok(())
}

/// 重置临时位置表
pub async fn reset_temp_position_table(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
    key_column: &str,
) -> Result<(), String> {
    validate_identifier(table_name)?;
    validate_identifier(key_column)?;
    
    let create_sql = format!(
        "CREATE TEMP TABLE IF NOT EXISTS {} ({} TEXT PRIMARY KEY, position INTEGER NOT NULL)",
        table_name, key_column
    );
    sqlx::query(&create_sql)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    let clear_sql = format!("DELETE FROM {}", table_name);
    sqlx::query(&clear_sql)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;

    Ok(())
}

/// 填充临时位置表
pub async fn fill_temp_position_table(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
    key_column: &str,
    values: &[String],
) -> Result<(), String> {
    validate_identifier(table_name)?;
    validate_identifier(key_column)?;
    
    if values.is_empty() {
        return Ok(());
    }
    let chunk_size = 500;
    for (chunk_idx, chunk) in values.chunks(chunk_size).enumerate() {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(format!(
            "INSERT OR REPLACE INTO {} ({}, position) ",
            table_name, key_column
        ));
        query_builder.push_values(chunk.iter().enumerate(), |mut b, (i, val)| {
            b.push_bind(val)
                .push_bind((chunk_idx * chunk_size + i) as i64);
        });
        let query = query_builder.build();
        query
            .execute(&mut **tx)
            .await
            .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    }
    Ok(())
}

/// 创建标准的 SQLite 连接选项
pub fn create_db_options(db_path: &PathBuf) -> sqlx::sqlite::SqliteConnectOptions {
    use sqlx::sqlite::{SqliteJournalMode, SqliteSynchronous};
    sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_millis(1200))
}

/// 生成搜索关键词的上下文摘要片段
/// 注意：使用字节偏移进行截取，对纯 ASCII 文本精确，
/// 对多字节 UTF-8（如中文）截取位置可能不是精确的字符边界，
/// 但 adjust_to_char_boundary 确保不会 panic。
pub fn build_keyword_snippet(
    content: &str,
    keyword: &str,
    max_len: usize,
) -> String {
    if keyword.is_empty() || content.is_empty() {
        let truncated = if content.len() > max_len {
            let end = adjust_to_char_boundary(content, max_len);
            format!("{}...", &content[..end])
        } else {
            content.to_string()
        };
        return truncated;
    }

    let lower_content = content.to_lowercase();
    let lower_keyword = keyword.to_lowercase();

    if let Some(pos) = lower_content.find(&lower_keyword) {
        let keyword_len = lower_keyword.len();
        let start = if pos > 20 { pos - 20 } else { 0 };
        let end = std::cmp::min(content.len(), pos + keyword_len + 60);

        let start = adjust_to_char_boundary(content, start);
        let end = adjust_to_char_boundary(content, end);

        let prefix = if start > 0 { "..." } else { "" };
        let suffix = if end < content.len() { "..." } else { "" };

        format!("{}{}{}", prefix, &content[start..end], suffix)
    } else {
        let end = adjust_to_char_boundary(content, max_len);
        let suffix = if end < content.len() { "..." } else { "" };
        format!("{}{}", &content[..end], suffix)
    }
}

/// 生成搜索关键词的上下文摘要片段（使用默认最大长度 108）
pub fn build_keyword_snippet_default(
    content: &str,
    keyword: &str,
) -> String {
    build_keyword_snippet(content, keyword, 108)
}

/// 调整字节偏移到有效的 UTF-8 字符边界
pub fn adjust_to_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_fts_token() {
        assert_eq!(escape_fts_token("hello"), "hello");
        assert_eq!(escape_fts_token("hello world"), "hello world");
        assert_eq!(escape_fts_token("a\\b"), "a\\\\b");
        assert_eq!(escape_fts_token("a\"b"), "a\"\"b");
        assert_eq!(escape_fts_token("a+b-c:d*e^f"), "a\\+b\\-c\\:d\\*e\\^f");
    }

    #[test]
    fn test_build_fts_query_and() {
        assert_eq!(build_fts_query_and(""), "");
        assert_eq!(build_fts_query_and("   "), "");
        assert_eq!(build_fts_query_and("hello"), "\"hello\"*");
        let q = build_fts_query_and("hello world");
        assert!(q.contains("AND"));
        let q = build_fts_query_and("say \"hi\"");
        assert!(q.contains("\"\"hi\"\"*"));
    }

    #[test]
    fn test_build_fts_query_space() {
        assert_eq!(build_fts_query_space(""), "");
        assert_eq!(build_fts_query_space("   "), "");
        assert_eq!(build_fts_query_space("hello"), "\"hello\"*");
        let q = build_fts_query_space("hello world");
        assert!(!q.contains("AND"));
    }

    #[test]
    fn test_adjust_to_char_boundary() {
        let s = "hello world";
        assert_eq!(adjust_to_char_boundary(s, 5), 5);
        assert_eq!(adjust_to_char_boundary(s, 100), s.len());

        let s = "你好世界";
        assert_eq!(adjust_to_char_boundary(s, 1), 0);
        assert_eq!(adjust_to_char_boundary(s, 2), 2);
        assert_eq!(adjust_to_char_boundary(s, 3), 2);
        assert_eq!(adjust_to_char_boundary(s, 4), 4);
    }
}
