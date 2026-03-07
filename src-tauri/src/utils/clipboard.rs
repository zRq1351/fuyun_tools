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

    pub fn get_categories(&self) -> HashMap<String, String> {
        let categories = self.categories.lock().unwrap();
        categories.clone()
    }

    pub fn get_category_list(&self) -> Vec<String> {
        let list = self.category_list.lock().unwrap();
        list.clone()
    }

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

        // 异步保存
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

    pub fn set_category(&self, item: String, category: String) -> Result<(), String> {
        // 先获取需要的数据，然后立即释放锁
        let (categories_clone, category_list_clone) = {
            let mut categories = self.categories.lock().unwrap();
            let mut category_list = self.category_list.lock().unwrap();

            // 规范化处理：如果分类是空或“未分类”，则视为移除
            let normalized_category = category.trim().to_string();

            if normalized_category.is_empty() || normalized_category == "未分类" || normalized_category == "全部" {
                categories.remove(&item);
            } else {
                categories.insert(item, normalized_category.clone());
                // 确保分类存在于列表中
                if !category_list.contains(&normalized_category) {
                    category_list.push(normalized_category);
                }
            }
            (categories.clone(), category_list.clone())
        };

        // 异步保存，不持有锁
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

    pub fn remove_category(&self, category: String) -> Result<(), String> {
        let (categories_clone, category_list_clone) = {
            let mut categories = self.categories.lock().unwrap();
            let mut category_list = self.category_list.lock().unwrap();

            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
            (categories.clone(), category_list.clone())
        };

        // 异步保存，不持有锁
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

        // 保存时也需要带上分类数据
        let categories = self.categories.lock().unwrap();
        let category_list = self.category_list.lock().unwrap();
        let data = ClipboardHistoryData {
            items: history.clone(),
            categories: categories.clone(),
            category_list: category_list.clone(),
        };

        // 异步保存
        std::thread::spawn(move || {
            if let Err(e) = save_history_data_with_retry(&data, 3) {
                log::error!("异步保存历史记录失败: {}", e);
            }
        });
    }

    pub fn clear_history(&self) -> Result<(), String> {
        let mut history = self.history.lock().unwrap();
        history.clear();

        // 同时清理分类
        let mut categories = self.categories.lock().unwrap();
        categories.clear();

        let mut category_list = self.category_list.lock().unwrap();
        category_list.clear();

        // 异步保存
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

    pub fn set_max_items(&mut self, max_items: usize) {
        self.max_items = max_items;
        log::info!("更新最大记录数为{}", max_items);

        // 如果当前历史记录超过新的限制，则截断
        let mut history = self.history.lock().unwrap();
        if history.len() > max_items {
            history.truncate(max_items);

            // 获取分类数据以便保存完整记录
            let categories = self.categories.lock().unwrap();
            let category_list = self.category_list.lock().unwrap();

            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
            };

            // 异步保存
            std::thread::spawn(move || {
                if let Err(e) = save_history_data_with_retry(&data, 3) {
                    log::error!("截断历史记录时保存失败: {}", e);
                }
            });
        }
    }

    pub fn remove_from_history(&self, index: usize) -> Result<(), String> {
        let mut history = self.history.lock().unwrap();
        if index < history.len() {
            let item = history.remove(index);

            // 同时清理分类
            let mut categories = self.categories.lock().unwrap();
            categories.remove(&item);

            // 保存完整数据
            let category_list = self.category_list.lock().unwrap();
            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
            };

            // 异步保存
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

impl Drop for ClipboardManager {
    fn drop(&mut self) {
        if let Err(e) = self.save_history_on_exit() {
            log::error!("程序退出时保存历史记录失败: {}", e);
        }
    }
}
