use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::utils::utils_helpers::{
    find_best_replacement_candidate, load_history_data, save_history_data_with_retry,
    ClipboardHistoryData,
};

pub struct ClipboardManager {
    history: Arc<Mutex<Vec<String>>>,
    categories: Arc<Mutex<HashMap<String, String>>>,
    category_list: Arc<Mutex<Vec<String>>>,
    max_items: usize,
}

impl ClipboardManager {
    /// 创建剪贴板管理器实例
    pub fn new(max_items: usize) -> Self {
        let history_data = load_history_data().unwrap_or_else(|e| {
            log::error!("加载历史记录失败: {}，使用空历史记录", e);
            ClipboardHistoryData::default()
        });

        Self {
            history: Arc::new(Mutex::new(history_data.items)),
            categories: Arc::new(Mutex::new(history_data.categories)),
            category_list: Arc::new(Mutex::new(history_data.category_list)),
            max_items,
        }
    }

    /// 获取当前剪贴板内容
    pub fn get_content(&self, app_handle: &tauri::AppHandle) -> Option<String> {
        use tauri_plugin_clipboard_manager::ClipboardExt;

        match app_handle.clipboard().read_text() {
            Ok(content) => Some(content),
            Err(e) => {
                let msg = e.to_string();
                if !is_expected_non_text_clipboard_error(&msg) {
                    log::debug!("获取剪贴板内容失败: {}", msg);
                }
                None
            }
        }
    }

    /// 设置剪贴板内容
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

    /// 获取历史记录
    pub fn get_history(&self) -> Vec<String> {
        let history = self.history.lock().unwrap();
        history.clone()
    }

    /// 获取分类映射
    pub fn get_categories(&self) -> HashMap<String, String> {
        let categories = self.categories.lock().unwrap();
        categories.clone()
    }

    /// 获取分类列表
    pub fn get_category_list(&self) -> Vec<String> {
        let list = self.category_list.lock().unwrap();
        list.clone()
    }

    /// 添加新分类
    pub fn add_category(&self, category: String) -> Result<(), String> {
        let (categories_clone, category_list_clone) = {
            let categories = self.categories.lock().unwrap();
            let mut category_list = self.category_list.lock().unwrap();

            let normalized_category = category.trim().to_string();

            if !normalized_category.is_empty()
                && normalized_category != "未分类"
                && normalized_category != "全部"
                && !category_list.contains(&normalized_category) {
                category_list.push(normalized_category);
            }

            (categories.clone(), category_list.clone())
        };

        let history = self.history.lock().unwrap().clone();

        std::thread::spawn(move || {
            let data = ClipboardHistoryData {
                items: history,
                categories: categories_clone,
                category_list: category_list_clone,
            };
            if let Err(e) = save_history_data_with_retry(&data, 3) {
                log::error!("异步保存历史数据失败: {}", e);
            }
        });

        Ok(())
    }

    /// 设置条目分类
    pub fn set_category(&self, item: String, category: String) -> Result<(), String> {
        let (categories_clone, category_list_clone) = {
            let mut categories = self.categories.lock().unwrap();
            let mut category_list = self.category_list.lock().unwrap();

            let normalized_category = category.trim().to_string();

            if normalized_category.is_empty() || normalized_category == "未分类" || normalized_category == "全部" {
                categories.remove(&item);
            } else {
                categories.insert(item, normalized_category.clone());
                if !category_list.contains(&normalized_category) {
                    category_list.push(normalized_category);
                }
            }
            (categories.clone(), category_list.clone())
        };

        let history = self.history.lock().unwrap().clone();

        std::thread::spawn(move || {
            let data = ClipboardHistoryData {
                items: history,
                categories: categories_clone,
                category_list: category_list_clone,
            };
            if let Err(e) = save_history_data_with_retry(&data, 3) {
                log::error!("异步保存历史数据失败: {}", e);
            }
        });

        Ok(())
    }

    /// 移除分类
    pub fn remove_category(&self, category: String) -> Result<(), String> {
        let (categories_clone, category_list_clone) = {
            let mut categories = self.categories.lock().unwrap();
            let mut category_list = self.category_list.lock().unwrap();

            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
            (categories.clone(), category_list.clone())
        };

        let history = self.history.lock().unwrap().clone();

        std::thread::spawn(move || {
            let data = ClipboardHistoryData {
                items: history,
                categories: categories_clone,
                category_list: category_list_clone,
            };
            if let Err(e) = save_history_data_with_retry(&data, 3) {
                log::error!("异步保存历史数据失败: {}", e);
            }
        });

        Ok(())
    }

    /// 将内容添加到剪贴板历史记录中
    pub fn add_to_history(&self, content: String) {
        let mut history = self.history.lock().unwrap();

        log::debug!("添加到历史记录: '{}'", content);
        log::debug!("当前历史记录数量: {}", history.len());

        let similarity_threshold = 0.8;

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
                let complete_version = history.remove(replace_index);
                history.insert(0, complete_version);
                log::info!("已将完整版本移动到最前面");
            } else {
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

        let categories = self.categories.lock().unwrap();
        let category_list = self.category_list.lock().unwrap();
        let data = ClipboardHistoryData {
            items: history.clone(),
            categories: categories.clone(),
            category_list: category_list.clone(),
        };

        std::thread::spawn(move || {
            if let Err(e) = save_history_data_with_retry(&data, 3) {
                log::error!("异步保存历史记录失败: {}", e);
            }
        });
    }

    /// 清空历史记录
    pub fn clear_history(&self) -> Result<(), String> {
        let mut history = self.history.lock().unwrap();
        history.clear();

        let mut categories = self.categories.lock().unwrap();
        categories.clear();

        let mut category_list = self.category_list.lock().unwrap();
        category_list.clear();

        std::thread::spawn(move || {
            let data = ClipboardHistoryData {
                items: Vec::new(),
                categories: HashMap::new(),
                category_list: Vec::new(),
            };
            if let Err(e) = save_history_data_with_retry(&data, 3) {
                log::error!("异步清空历史记录保存失败: {}", e);
            }
        });
        
        log::info!("历史记录已清空");
        Ok(())
    }

    /// 设置最大历史记录数量
    pub fn set_max_items(&mut self, max_items: usize) {
        self.max_items = max_items;
        log::info!("更新最大记录数为{}", max_items);

        let mut history = self.history.lock().unwrap();
        if history.len() > max_items {
            history.truncate(max_items);

            let categories = self.categories.lock().unwrap();
            let category_list = self.category_list.lock().unwrap();

            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
            };

            std::thread::spawn(move || {
                if let Err(e) = save_history_data_with_retry(&data, 3) {
                    log::error!("截断历史记录时保存失败: {}", e);
                }
            });
        }
    }

    /// 移除指定历史记录
    pub fn remove_from_history(&self, index: usize) -> Result<(), String> {
        let mut history = self.history.lock().unwrap();
        if index < history.len() {
            let item = history.remove(index);

            let mut categories = self.categories.lock().unwrap();
            categories.remove(&item);

            let category_list = self.category_list.lock().unwrap();
            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
            };

            std::thread::spawn(move || {
                if let Err(e) = save_history_data_with_retry(&data, 3) {
                    log::error!("异步保存历史记录失败: {}", e);
                }
            });
            Ok(())
        } else {
            Err("索引超出范围".to_string())
        }
    }

    /// 退出时保存历史记录
    pub fn save_history_on_exit(&self) -> Result<(), String> {
        let history = self.history.lock().unwrap();
        let categories = self.categories.lock().unwrap();
        let category_list = self.category_list.lock().unwrap();

        let data = ClipboardHistoryData {
            items: history.clone(),
            categories: categories.clone(),
            category_list: category_list.clone(),
        };
        save_history_data_with_retry(&data, 3)
    }
}

fn is_expected_non_text_clipboard_error(msg: &str) -> bool {
    msg.contains("requested format")
        || msg.contains("clipboard is empty")
        || msg.contains("not available in the requested format")
}

impl Drop for ClipboardManager {
    /// 析构时自动保存
    fn drop(&mut self) {
        if let Err(e) = self.save_history_on_exit() {
            log::error!("程序退出时保存历史记录失败: {}", e);
        }
    }
}
