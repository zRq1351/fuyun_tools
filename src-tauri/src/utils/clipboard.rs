use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::utils::utils_helpers::{
    find_best_replacement_candidate, load_history_data, save_history_data_with_retry,
    ClipboardHistoryData,
};

pub struct ClipboardManager {
    history: Arc<Mutex<Vec<String>>>,
    history_fingerprints: Arc<Mutex<Vec<(usize, u64)>>>,
    history_cache_dirty: Arc<AtomicBool>,
    persist_tx: Sender<ClipboardHistoryData>,
    categories: Arc<Mutex<HashMap<String, String>>>,
    category_list: Arc<Mutex<Vec<String>>>,
    max_items: usize,
    grouped_items_protected_from_limit: bool,
}

const LONG_TEXT_DEDUP_THRESHOLD: usize = 4000;
const LONG_TEXT_DEDUP_SCAN_LIMIT: usize = 24;

fn stable_text_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn build_history_fingerprints(history: &[String]) -> Vec<(usize, u64)> {
    history
        .iter()
        .map(|item| (item.chars().count(), stable_text_hash(item)))
        .collect()
}

impl ClipboardManager {
    /// 创建剪贴板管理器实例
    pub fn new(max_items: usize, grouped_items_protected_from_limit: bool) -> Self {
        let history_data = load_history_data().unwrap_or_else(|e| {
            log::error!("加载历史记录失败: {}，使用空历史记录", e);
            ClipboardHistoryData::default()
        });
        let history_fingerprints = build_history_fingerprints(&history_data.items);
        let (persist_tx, persist_rx) = mpsc::channel::<ClipboardHistoryData>();
        std::thread::spawn(move || {
            const DEBOUNCE_MS: u64 = 180;
            loop {
                let mut latest = match persist_rx.recv() {
                    Ok(data) => data,
                    Err(_) => break,
                };
                loop {
                    match persist_rx.recv_timeout(Duration::from_millis(DEBOUNCE_MS)) {
                        Ok(newer) => latest = newer,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => {
                            let _ = save_history_data_with_retry(&latest, 3);
                            return;
                        }
                    }
                }
                if let Err(e) = save_history_data_with_retry(&latest, 3) {
                    log::error!("异步保存历史记录失败: {}", e);
                }
            }
        });

        Self {
            history: Arc::new(Mutex::new(history_data.items)),
            history_fingerprints: Arc::new(Mutex::new(history_fingerprints)),
            history_cache_dirty: Arc::new(AtomicBool::new(false)),
            persist_tx,
            categories: Arc::new(Mutex::new(history_data.categories)),
            category_list: Arc::new(Mutex::new(history_data.category_list)),
            max_items,
            grouped_items_protected_from_limit,
        }
    }

    fn enqueue_persist(&self, data: ClipboardHistoryData) {
        if let Err(e) = self.persist_tx.send(data) {
            log::error!("提交历史记录保存任务失败: {}", e);
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

        self.enqueue_persist(ClipboardHistoryData {
            items: history,
            categories: categories_clone,
            category_list: category_list_clone,
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

        self.enqueue_persist(ClipboardHistoryData {
            items: history,
            categories: categories_clone,
            category_list: category_list_clone,
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

        self.enqueue_persist(ClipboardHistoryData {
            items: history,
            categories: categories_clone,
            category_list: category_list_clone,
        });

        Ok(())
    }

    /// 将内容添加到剪贴板历史记录中
    pub fn add_to_history(&self, content: String) {
        let mut history = self.history.lock().unwrap();

        let content_len = content.chars().count();
        log::debug!("添加到历史记录，长度: {}, 当前数量: {}", content_len, history.len());

        let content_hash = stable_text_hash(&content);
        let mut fingerprints = self.history_fingerprints.lock().unwrap();
        let cache_dirty = self.history_cache_dirty.load(Ordering::Relaxed);
        if cache_dirty || fingerprints.len() != history.len() {
            *fingerprints = build_history_fingerprints(&history);
            self.history_cache_dirty.store(false, Ordering::Relaxed);
        }
        if let Some(exact_index) = fingerprints
            .iter()
            .enumerate()
            .position(|(idx, (item_len, item_hash))| {
                *item_len == content_len
                    && *item_hash == content_hash
                    && history.get(idx).is_some_and(|item| item == &content)
            })
        {
            if exact_index != 0 {
                let exact_item = history.remove(exact_index);
                history.insert(0, exact_item);
            }
            let mut categories = self.categories.lock().unwrap();
            shrink_text_history_with_group_protection(
                &mut history,
                self.max_items,
                &mut categories,
                self.grouped_items_protected_from_limit,
            );
            let category_list = self.category_list.lock().unwrap();
            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
            };
            self.enqueue_persist(data);
            *fingerprints = build_history_fingerprints(&history);
            self.history_cache_dirty.store(false, Ordering::Relaxed);
            return;
        }

        let similarity_threshold = 0.8;

        let scan_len = if content_len >= LONG_TEXT_DEDUP_THRESHOLD {
            history.len().min(LONG_TEXT_DEDUP_SCAN_LIMIT)
        } else {
            history.len()
        };
        let candidate_history = &history[..scan_len];

        if let Some((replace_index, comparison)) =
            find_best_replacement_candidate(&content, candidate_history, similarity_threshold)
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

        let mut categories = self.categories.lock().unwrap();
        shrink_text_history_with_group_protection(
            &mut history,
            self.max_items,
            &mut categories,
            self.grouped_items_protected_from_limit,
        );
        let category_list = self.category_list.lock().unwrap();
        let data = ClipboardHistoryData {
            items: history.clone(),
            categories: categories.clone(),
            category_list: category_list.clone(),
        };

        self.enqueue_persist(data);
        *fingerprints = build_history_fingerprints(&history);
        self.history_cache_dirty.store(false, Ordering::Relaxed);
    }

    /// 清空历史记录
    pub fn clear_history(&self) -> Result<(), String> {
        let mut history = self.history.lock().unwrap();
        history.clear();
        self.history_cache_dirty.store(true, Ordering::Relaxed);

        let mut categories = self.categories.lock().unwrap();
        categories.clear();

        let mut category_list = self.category_list.lock().unwrap();
        category_list.clear();

        self.enqueue_persist(ClipboardHistoryData {
            items: Vec::new(),
            categories: HashMap::new(),
            category_list: Vec::new(),
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
            let mut categories = self.categories.lock().unwrap();
            shrink_text_history_with_group_protection(
                &mut history,
                max_items,
                &mut categories,
                self.grouped_items_protected_from_limit,
            );
            let category_list = self.category_list.lock().unwrap();

            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
            };

            self.enqueue_persist(data);
            self.history_cache_dirty.store(true, Ordering::Relaxed);
        }
    }

    /// 移除指定历史记录
    pub fn remove_from_history(&self, index: usize) -> Result<String, String> {
        let mut history = self.history.lock().unwrap();
        if index < history.len() {
            let item = history.remove(index);
            self.history_cache_dirty.store(true, Ordering::Relaxed);

            let mut categories = self.categories.lock().unwrap();
            categories.remove(&item);

            let category_list = self.category_list.lock().unwrap();
            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
            };

            self.enqueue_persist(data);
            Ok(item)
        } else {
            Err("索引超出范围".to_string())
        }
    }

    pub fn promote_to_top(&self, index: usize) -> Result<String, String> {
        let (item, categories_clone, category_list_clone, history_clone) = {
            let mut history = self.history.lock().unwrap();
            if index >= history.len() {
                return Err("索引超出范围".to_string());
            }
            if index == 0 {
                let item = history[0].clone();
                return Ok(item);
            }
            let item = history.remove(index);
            history.insert(0, item.clone());
            self.history_cache_dirty.store(true, Ordering::Relaxed);

            let categories = self.categories.lock().unwrap().clone();
            let category_list = self.category_list.lock().unwrap().clone();
            (item, categories, category_list, history.clone())
        };

        self.enqueue_persist(ClipboardHistoryData {
            items: history_clone,
            categories: categories_clone,
            category_list: category_list_clone,
        });

        Ok(item)
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

    pub fn set_grouped_items_protected_from_limit(&mut self, enabled: bool) {
        self.grouped_items_protected_from_limit = enabled;
        let mut history = self.history.lock().unwrap();
        let mut categories = self.categories.lock().unwrap();
        shrink_text_history_with_group_protection(
            &mut history,
            self.max_items,
            &mut categories,
            self.grouped_items_protected_from_limit,
        );
        self.history_cache_dirty.store(true, Ordering::Relaxed);
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

fn shrink_text_history_with_group_protection(
    history: &mut Vec<String>,
    max_items: usize,
    categories: &mut HashMap<String, String>,
    grouped_items_protected_from_limit: bool,
) {
    if !grouped_items_protected_from_limit {
        if history.len() > max_items {
            let removed = history.split_off(max_items);
            for item in removed {
                categories.remove(&item);
            }
        }
        return;
    }
    while history.len() > max_items {
        if let Some(pos) = history
            .iter()
            .rposition(|item| !categories.contains_key(item))
        {
            let removed = history.remove(pos);
            categories.remove(&removed);
        } else {
            break;
        }
    }
}
