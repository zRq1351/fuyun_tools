use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::services::app_scanner;
use crate::services::app_store;
use crate::services::launcher_config;
use crate::ui::window_manager::show_overlay_window_by_label;

const WINDOW_WIDTH: f64 = 620.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub action: String,
    pub path: Option<String>,
    pub shortcut: Option<String>,
    pub result: Option<String>,
}

/// 搜索启动器项目（应用、文件、命令等）
#[tauri::command]
pub async fn search_launcher_items(query: String, limit: usize) -> Result<Vec<SearchResult>, String> {
    let mut results = Vec::new();

    let apps = app_scanner::search_apps(&query, limit);
    for app in apps {
        results.push(SearchResult {
            id: app.id,
            title: app.title,
            description: app.description,
            icon: app.icon,
            item_type: app.item_type,
            action: app.action,
            path: app.path,
            shortcut: app.shortcut,
            result: None,
        });
    }

    Ok(results)
}

/// 计算数学表达式
#[tauri::command]
pub async fn calculate_expression(expr: String) -> Result<String, String> {
    let result = safe_eval(&expr).map_err(|e| format!("计算错误: {}", e))?;

    let formatted = if result.fract() == 0.0 {
        format!("{}", result as i64)
    } else {
        format!("{:.10}", result).trim_end_matches('0').trim_end_matches('.').to_string()
    };

    Ok(formatted)
}

/// 安全的数学表达式求值
fn safe_eval(expr: &str) -> Result<f64, String> {
    let sanitized = expr
        .replace('^', "**")
        .replace('×', "*")
        .replace('÷', "/");

    if !sanitized.chars().all(|c| c.is_ascii_digit() || "+-*/.()eE ".contains(c)) {
        return Err("无效的数学表达式".to_string());
    }

    let mut parser = Parser::new(&sanitized);
    parser.parse_expression().map_err(|e| e.to_string())
}

/// 简单的数学表达式解析器
struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn parse_expression(&mut self) -> Result<f64, String> {
        let mut result = self.parse_term()?;

        while self.pos < self.chars.len() {
            match self.current_char() {
                '+' => {
                    self.advance();
                    result += self.parse_term()?;
                }
                '-' => {
                    self.advance();
                    result -= self.parse_term()?;
                }
                _ => break,
            }
        }

        Ok(result)
    }

    fn parse_term(&mut self) -> Result<f64, String> {
        let mut result = self.parse_factor()?;

        while self.pos < self.chars.len() {
            match self.current_char() {
                '*' => {
                    self.advance();
                    if self.current_char() == '*' {
                        self.advance();
                        let exp = self.parse_factor()?;
                        result = result.powf(exp);
                    } else {
                        result *= self.parse_factor()?;
                    }
                }
                '/' => {
                    self.advance();
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        return Err("除以零".to_string());
                    }
                    result /= divisor;
                }
                _ => break,
            }
        }

        Ok(result)
    }

    fn parse_factor(&mut self) -> Result<f64, String> {
        self.skip_whitespace();

        if self.pos >= self.chars.len() {
            return Err("意外的表达式结束".to_string());
        }

        match self.current_char() {
            '(' => {
                self.advance();
                let result = self.parse_expression()?;
                if self.current_char() != ')' {
                    return Err("缺少右括号".to_string());
                }
                self.advance();
                Ok(result)
            }
            '-' => {
                self.advance();
                Ok(-self.parse_factor()?)
            }
            '+' => {
                self.advance();
                self.parse_factor()
            }
            _ => self.parse_number(),
        }
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        self.skip_whitespace();

        let start = self.pos;
        while self.pos < self.chars.len() && (self.current_char().is_ascii_digit() || self.current_char() == '.') {
            self.advance();
        }

        if start == self.pos {
            return Err("期望数字".to_string());
        }

        let num_str: String = self.chars[start..self.pos].iter().collect();
        num_str.parse::<f64>().map_err(|_| "无效的数字".to_string())
    }

    fn current_char(&self) -> char {
        if self.pos < self.chars.len() {
            self.chars[self.pos]
        } else {
            '\0'
        }
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }
}

/// 获取所有已安装的应用程序（从存储加载）
#[tauri::command]
pub async fn get_all_apps() -> Result<Vec<app_store::StoredApp>, String> {
    let store = app_store::load_app_store().await;
    if store.apps.is_empty() {
        return Err("NEED_SCAN".to_string());
    }
    Ok(store.apps)
}

/// 扫描并保存应用（首次或刷新）
#[tauri::command]
pub async fn scan_and_save_apps() -> Result<Vec<app_store::StoredApp>, String> {
    let store = app_store::scan_and_save_apps().await?;
    Ok(store.apps)
}

/// 批量提取应用图标
#[tauri::command]
pub async fn batch_extract_icons(paths: Vec<String>) -> Result<std::collections::HashMap<String, String>, String> {
    let icons = app_scanner::batch_extract_icons(&paths);
    app_store::batch_update_icons(&icons).await;
    Ok(icons)
}

/// 启动应用程序（带存在性检测）
#[tauri::command]
pub async fn launch_app(_app_id: String, path: String) -> Result<(), String> {
    log::info!("[launch_app command] 收到请求, app_id: {}, path: {}", _app_id, path);
    if !std::path::Path::new(&path).exists() {
        log::error!("[launch_app command] APP_NOT_FOUND: {}", path);
        return Err("APP_NOT_FOUND".to_string());
    }
    app_scanner::launch_app(&path)
}

/// 启动应用程序并传递参数
#[tauri::command]
pub async fn launch_app_with_args(_app_id: String, path: String, args: Option<String>) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err("APP_NOT_FOUND".to_string());
    }
    app_scanner::launch_app_with_args(&path, args.as_deref())
}

/// 删除应用记录
#[tauri::command]
pub async fn remove_app_record(app_id: String) -> Result<(), String> {
    app_store::remove_app_from_store(&app_id).await
}

/// 更新应用排序
#[tauri::command]
pub async fn update_app_sort_orders(orders: Vec<(String, i32)>) -> Result<(), String> {
    app_store::update_app_sort_orders(orders).await
}

/// 获取启动器配置
#[tauri::command]
pub async fn get_launcher_config() -> Result<launcher_config::LauncherConfig, String> {
    Ok(launcher_config::load_launcher_config().await)
}

/// 添加自定义分类
#[tauri::command]
pub async fn add_launcher_category(name: String, icon: String) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::add_category(name, icon).await
}

/// 删除自定义分类
#[tauri::command]
pub async fn remove_launcher_category(category_id: String) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::remove_category(category_id).await
}

/// 重命名分类
#[tauri::command]
pub async fn rename_launcher_category(category_id: String, new_name: String) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::rename_category(category_id, new_name).await
}

/// 设置应用分类
#[tauri::command]
pub async fn set_app_category(app_id: String, category_id: String) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::set_app_category(app_id, category_id).await
}

/// 设置视图模式
#[tauri::command]
pub async fn set_launcher_view_mode(mode: String) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::set_view_mode(mode).await
}

/// 重新排序分类
#[tauri::command]
pub async fn reorder_categories(category_ids: Vec<String>) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::reorder_categories(category_ids).await
}

/// 更新分类图标
#[tauri::command]
pub async fn update_category_icon(category_id: String, icon: String) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::update_category_icon(category_id, icon).await
}

/// 添加自定义命令
#[tauri::command]
pub async fn add_custom_command(
    prefix: String,
    title: String,
    description: Option<String>,
    icon: String,
    command_type: launcher_config::CustomCommandType,
) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::add_custom_command(prefix, title, description, icon, command_type).await
}

/// 删除自定义命令
#[tauri::command]
pub async fn remove_custom_command(command_id: String) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::remove_custom_command(command_id).await
}

/// 更新自定义命令
#[tauri::command]
pub async fn update_custom_command(
    command_id: String,
    prefix: Option<String>,
    title: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    command_type: Option<launcher_config::CustomCommandType>,
    enabled: Option<bool>,
) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::update_custom_command(command_id, prefix, title, description, icon, command_type, enabled).await
}

/// 切换自定义命令启用状态
#[tauri::command]
pub async fn toggle_custom_command(command_id: String) -> Result<launcher_config::LauncherConfig, String> {
    launcher_config::toggle_custom_command(command_id).await
}

/// 显示启动器窗口
#[tauri::command]
pub async fn show_launcher(app: AppHandle) -> Result<(), String> {
    use tauri::LogicalPosition;
    
    show_overlay_window_by_label(&app, "launcher", true)?;
    
    if let Some(window) = app.get_webview_window("launcher") {
        if let Ok(Some(monitor)) = window.primary_monitor() {
            let screen_size = monitor.size();
            let scale_factor = monitor.scale_factor();
            
            let screen_width = screen_size.width as f64 / scale_factor;
            let screen_height = screen_size.height as f64 / scale_factor;
            
            let x = (screen_width - WINDOW_WIDTH) / 2.0;
            let y = screen_height * 0.25;
            
            let position = LogicalPosition::new(x, y);
            window.set_position(position).map_err(|e| e.to_string())?;
        }
    }
    
    app.emit("show-launcher", ()).map_err(|e| e.to_string())?;
    Ok(())
}

/// 隐藏启动器窗口
#[tauri::command]
pub async fn hide_launcher(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("launcher") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 调整启动器窗口大小
#[tauri::command]
pub async fn resize_launcher(app: AppHandle, height: f64) -> Result<(), String> {
    use tauri::LogicalSize;
    if let Some(window) = app.get_webview_window("launcher") {
        let current_pos = window.outer_position().ok();
        let size = LogicalSize::new(WINDOW_WIDTH, height);
        window.set_size(size).map_err(|e| e.to_string())?;
        
        if let Some(pos) = current_pos {
            window.set_position(pos).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 切换启动器窗口显示状态
#[tauri::command]
pub async fn toggle_launcher(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("launcher") {
        if window.is_visible().unwrap_or(false) {
            window.hide().map_err(|e| e.to_string())?;
        } else {
            show_overlay_window_by_label(&app, "launcher", true)?;
            app.emit("show-launcher", ()).map_err(|e| e.to_string())?;
        }
    } else {
        show_overlay_window_by_label(&app, "launcher", true)?;
        app.emit("show-launcher", ()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 打开文件
#[tauri::command]
pub async fn open_file(path: String) -> Result<(), String> {
    app_scanner::open_file(&path)
}
