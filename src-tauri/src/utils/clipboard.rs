use std::sync::{Arc, Mutex};

use crate::utils::utils::{load_history, save_history_with_retry};

pub struct ClipboardManager {
    history: Arc<Mutex<Vec<String>>>,
    max_items: usize,
}

impl ClipboardManager {
    pub fn new(max_items: usize) -> Self {
        let history = load_history().unwrap_or_else(|e| {
            log::error!("加载历史记录失败: {}，使用空历史记录", e);
            vec![]
        });

        Self {
            history: Arc::new(Mutex::new(history)),
            max_items,
        }
    }

    pub fn get_content(&self, app_handle: &tauri::AppHandle) -> Option<String> {
        use tauri_plugin_clipboard_manager::ClipboardExt;

        match app_handle.clipboard().read_text() {
            Ok(content) => Some(content),
            Err(e) => {
                log::debug!("获取剪贴板内容失败: {}", e);
                None
            }
        }
    }

    pub fn set_clipboard_content(
        &self,
        app_handle: &tauri::AppHandle,
        content: &str,
    ) -> Result<(), String> {
        use tauri_plugin_clipboard_manager::ClipboardExt;

        match app_handle.clipboard().write_text(content) {
            Ok(()) => {
                log::info!("成功设置剪贴板内容");
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("设置剪贴板内容失败: {}", e);
                log::error!("{}", error_msg);
                Err(error_msg)
            }
        }
    }

    pub fn get_history(&self) -> Vec<String> {
        let history = self.history.lock().unwrap();
        history.clone()
    }

    pub fn add_to_history(&self, content: String) {
        let mut history = self.history.lock().unwrap();
        // 移除重复项
        history.retain(|item| item != &content);
        // 在开头插入新内容
        history.insert(0, content);
        // 限制历史记录数量
        if history.len() > self.max_items {
            history.truncate(self.max_items);
        }

        // 保存到文件
        if let Err(e) = save_history_with_retry(&history, 3) {
            log::error!("保存历史记录失败: {}", e);
        }
    }

    pub fn clear_history(&self) {
        let mut history = self.history.lock().unwrap();
        *history = vec![];
        if let Err(e) = save_history_with_retry(&history, 3) {
            log::error!("清空历史记录时保存失败: {}", e);
        }
        log::info!("历史记录已清空");
    }

    pub fn set_max_items(&mut self, max_items: usize) {
        self.max_items = max_items;
        log::info!("更新最大记录数为{}", max_items);

        // 如果当前历史记录超过新的限制，则截断
        let mut history = self.history.lock().unwrap();
        if history.len() > max_items {
            history.truncate(max_items);
            if let Err(e) = save_history_with_retry(&history, 3) {
                log::error!("截断历史记录时保存失败: {}", e);
            }
        }
    }

    pub fn remove_from_history(&self, index: usize) -> Result<(), String> {
        let mut history = self.history.lock().unwrap();
        if index >= history.len() {
            return Err(format!("索引 {} 超出范围", index));
        }
        history.remove(index);

        if let Err(e) = save_history_with_retry(&history, 3) {
            return Err(format!("保存历史记录失败: {}", e));
        }
        Ok(())
    }

    pub fn save_history_on_exit(&self) -> Result<(), String> {
        let history = self.history.lock().unwrap();
        save_history_with_retry(&history, 3)
    }
}

impl Drop for ClipboardManager {
    fn drop(&mut self) {
        if let Err(e) = self.save_history_on_exit() {
            log::error!("程序退出时保存历史记录失败: {}", e);
        }
    }
}
