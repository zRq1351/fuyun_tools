use std::sync::{Arc, Mutex};

use crate::utils::utils::{find_best_replacement_candidate, load_history, save_history_with_retry};

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

    /// 将内容添加到剪贴板历史记录中
    ///
    /// 该函数实现了智能历史记录管理，包括版本优化和去重功能。
    /// 当检测到相似的不完整版本时，会自动用完整版本替换旧版本。
    ///
    /// # 参数
    ///
    /// * `content` - 要添加到历史记录的字符串内容
    ///
    /// # 功能说明
    ///
    /// 1. 版本优化：使用0.8的相似度阈值检测并替换不完整的早期版本
    /// 2. 去重处理：移除完全相同的重复项
    /// 3. 数量限制：确保历史记录不超过最大条数限制
    /// 4. 持久化保存：将更新后的历史记录保存到文件
    pub fn add_to_history(&self, content: String) {
        let mut history = self.history.lock().unwrap();

        log::debug!("添加到历史记录: '{}'", content);
        log::debug!("当前历史记录数量: {}", history.len());

        let similarity_threshold = 0.8;

        // 调试：显示所有历史记录用于对比
        for (i, item) in history.iter().enumerate() {
            log::debug!("历史记录[{}]: '{}'", i, item);
        }

        if let Some((replace_index, comparison)) =
            find_best_replacement_candidate(&content, &history, similarity_threshold)
        {
            log::info!("检测到相似版本，正在处理: {}", comparison.reason);
            log::info!("相似度: {:.4}, 完整性: {:?}", 
                      comparison.similarity_score, 
                      comparison.new_completeness);

            if comparison.reason.contains("子集") || comparison.reason.contains("找回完整版本") {
                // 如果是子集关系或找回完整版本，将完整版本移动到最前面
                let complete_version = history.remove(replace_index);
                history.insert(0, complete_version);
                log::info!("已将完整版本移动到最前面");
            } else {
                // 正常替换逻辑
                history[replace_index] = content.clone();
                let item = history.remove(replace_index);
                history.insert(0, item);
                log::info!("已用完整版本替换不完整版本");
            }
        } else {
            log::debug!("未找到相似版本，直接添加");
            history.retain(|item| item != &content);

            history.insert(0, content);
        }

        if history.len() > self.max_items {
            history.truncate(self.max_items);
        }

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
