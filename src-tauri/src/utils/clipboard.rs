use crate::sync::Mutex;
use bloom::{BloomFilter, ASMS};
use lru::LruCache;
use parking_lot::Mutex as ParkingMutex;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

use crate::utils::utils_helpers::{
    find_best_replacement_candidate, load_history_data,
    ClipboardHistoryData,
};

#[derive(Clone)]
pub struct ClipboardManager {
    history: Arc<Mutex<Vec<String>>>,
    history_fingerprints: Arc<Mutex<Vec<(usize, u64)>>>,
    exact_index_cache: Arc<ParkingMutex<LruCache<u64, usize>>>,
    history_cache_dirty: Arc<AtomicBool>,
    persist_tx: Sender<ClipboardHistoryData>,
    categories: Arc<Mutex<HashMap<String, String>>>,
    category_list: Arc<Mutex<Vec<String>>>,
    pinned_items: Arc<Mutex<Vec<String>>>,
    max_items: usize,
    grouped_items_protected_from_limit: bool,
    bloom_filter: Arc<ParkingMutex<BloomFilter>>,
}

const LONG_TEXT_DEDUP_THRESHOLD: usize = 4000;
const LONG_TEXT_DEDUP_SCAN_LIMIT: usize = 24;
const EXACT_INDEX_CACHE_CAPACITY: usize = 2048;
const BLOOM_FILTER_CAPACITY: u32 = 10000;
const BLOOM_FILTER_ERROR_RATE: f32 = 0.01; // 1% 误判率

fn lock_arc_mutex<'a, T>(mutex: &'a Arc<Mutex<T>>) -> crate::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(e) => match e {},
    }
}

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
        let mut pinned_items = history_data.pinned_items.clone();
        normalize_pinned_items(&mut pinned_items, &history_data.items);
        let initial_snapshot = ClipboardHistoryData {
            items: history_data.items.clone(),
            categories: history_data.categories.clone(),
            category_list: history_data.category_list.clone(),
            pinned_items: pinned_items.clone(),
        };
        let history_fingerprints = build_history_fingerprints(&history_data.items);
        let (persist_tx, persist_rx) = mpsc::channel::<ClipboardHistoryData>();
        std::thread::spawn(move || {
            while let Ok(mut latest) = persist_rx.recv() {
                while let Ok(next) = persist_rx.try_recv() {
                    latest = next;
                }
                if let Err(e) = tauri::async_runtime::block_on(
                    crate::utils::database::save_history_data_snapshot_async(&latest)
                ) {
                    log::error!("保存历史记录失败: {}", e);
                }
            }
        });

        if !initial_snapshot.items.is_empty()
            || !initial_snapshot.categories.is_empty()
            || !initial_snapshot.category_list.is_empty()
            || !initial_snapshot.pinned_items.is_empty() {
            if let Err(e) = persist_tx.send(initial_snapshot) {
                log::error!("提交初始历史记录保存任务失败: {}", e);
            }
        }

        // 初始化布隆过滤器
        let mut bloom_filter = BloomFilter::with_rate(BLOOM_FILTER_ERROR_RATE, BLOOM_FILTER_CAPACITY);

        // 将现有历史记录添加到布隆过滤器
        for item in &history_data.items {
            bloom_filter.insert(item);
        }

        Self {
            history: Arc::new(Mutex::new(history_data.items)),
            history_fingerprints: Arc::new(Mutex::new(history_fingerprints)),
            exact_index_cache: Arc::new(ParkingMutex::new(LruCache::new(
                NonZeroUsize::new(EXACT_INDEX_CACHE_CAPACITY).unwrap_or(NonZeroUsize::MIN),
            ))),
            history_cache_dirty: Arc::new(AtomicBool::new(false)),
            persist_tx,
            categories: Arc::new(Mutex::new(history_data.categories)),
            category_list: Arc::new(Mutex::new(history_data.category_list)),
            pinned_items: Arc::new(Mutex::new(pinned_items)),
            max_items,
            grouped_items_protected_from_limit,
            bloom_filter: Arc::new(ParkingMutex::new(bloom_filter)),
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

        match crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
            app_handle.clipboard().read_text()
        }) {
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
        let history = lock_arc_mutex(&self.history);
        history.clone()
    }

    /// 获取分类映射
    pub fn get_categories(&self) -> HashMap<String, String> {
        let categories = lock_arc_mutex(&self.categories);
        categories.clone()
    }

    /// 获取分类列表
    pub fn get_category_list(&self) -> Vec<String> {
        let list = lock_arc_mutex(&self.category_list);
        list.clone()
    }

    pub fn get_pinned_items(&self) -> Vec<String> {
        let pinned_items = lock_arc_mutex(&self.pinned_items);
        pinned_items.clone()
    }

    /// 添加新分类
    pub fn add_category(&self, category: String) -> Result<(), String> {
        let (categories_clone, category_list_clone) = {
            let categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);

            let normalized_category = category.trim().to_string();

            if !normalized_category.is_empty()
                && normalized_category != "未分类"
                && normalized_category != "全部"
                && !category_list.contains(&normalized_category) {
                category_list.push(normalized_category);
            }

            (categories.clone(), category_list.clone())
        };

        let history = lock_arc_mutex(&self.history).clone();
        let pinned_items = lock_arc_mutex(&self.pinned_items).clone();

        self.enqueue_persist(ClipboardHistoryData {
            items: history,
            categories: categories_clone,
            category_list: category_list_clone,
            pinned_items,
        });

        Ok(())
    }

    pub async fn add_category_async(&self, category: String) -> Result<(), String> {
        let normalized_category = category.trim().to_string();
        if !normalized_category.is_empty()
            && normalized_category != "未分类"
            && normalized_category != "全部" {
            {
                let mut category_list = lock_arc_mutex(&self.category_list);
                if !category_list.contains(&normalized_category) {
                    category_list.push(normalized_category.clone());
                }
            }
            crate::utils::database::add_category_to_list(&normalized_category).await?;
        }
        Ok(())
    }

    /// 设置条目分类
    pub fn set_category(&self, item: String, category: String) -> Result<(), String> {
        let (categories_clone, category_list_clone) = {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);

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

        let history = lock_arc_mutex(&self.history).clone();
        let pinned_items = lock_arc_mutex(&self.pinned_items).clone();

        self.enqueue_persist(ClipboardHistoryData {
            items: history,
            categories: categories_clone,
            category_list: category_list_clone,
            pinned_items,
        });

        Ok(())
    }

    pub async fn set_category_async(&self, item: String, category: String) -> Result<(), String> {
        let normalized_category = category.trim().to_string();
        if normalized_category.is_empty() || normalized_category == "未分类" || normalized_category == "全部" {
            {
                let mut categories = lock_arc_mutex(&self.categories);
                categories.remove(&item);
            }
            crate::utils::database::remove_item_category(&item).await?;
        } else {
            {
                let mut categories = lock_arc_mutex(&self.categories);
                let mut category_list = lock_arc_mutex(&self.category_list);
                categories.insert(item.clone(), normalized_category.clone());
                if !category_list.contains(&normalized_category) {
                    category_list.push(normalized_category.clone());
                }
            }
            crate::utils::database::set_item_category(&item, &normalized_category).await?;
            crate::utils::database::add_category_to_list(&normalized_category).await?;
        }
        Ok(())
    }

    /// 移除分类
    pub fn remove_category(&self, category: String) -> Result<(), String> {
        let (categories_clone, category_list_clone) = {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);

            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
            (categories.clone(), category_list.clone())
        };

        let history = lock_arc_mutex(&self.history).clone();
        let pinned_items = lock_arc_mutex(&self.pinned_items).clone();

        self.enqueue_persist(ClipboardHistoryData {
            items: history,
            categories: categories_clone,
            category_list: category_list_clone,
            pinned_items,
        });

        Ok(())
    }

    pub async fn remove_category_async(&self, category: String) -> Result<(), String> {
        {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
        }
        crate::utils::database::remove_category_from_list(&category).await?;
        Ok(())
    }

    /// 将内容添加到剪贴板历史记录中
    pub fn add_to_history(&self, content: String) {
        let mut history = lock_arc_mutex(&self.history);

        let content_len = content.chars().count();
        log::debug!("添加到历史记录，长度: {}, 当前数量: {}", content_len, history.len());

        // 优化：布隆过滤器预筛选 - O(1) 快速检查
        let bloom_filter = self.bloom_filter.lock();
        if bloom_filter.contains(&content) {
            drop(bloom_filter);
            // 布隆过滤器提示可能存在，进行精确检查
            let content_hash = stable_text_hash(&content);
            let mut fingerprints = lock_arc_mutex(&self.history_fingerprints);
            let mut exact_index_cache = self.exact_index_cache.lock();

            // 检查精确缓存
            if let Some(cached_index) = exact_index_cache.get(&content_hash).copied() {
                if history.get(cached_index).is_some_and(|item| item == &content) {
                    if cached_index != 0 {
                        let exact_item = history.remove(cached_index);
                        history.insert(0, exact_item);
                    }
                    exact_index_cache.clear();
                    exact_index_cache.put(content_hash, 0);
                    let mut categories = lock_arc_mutex(&self.categories);
                    let mut pinned_items = lock_arc_mutex(&self.pinned_items);
                    shrink_text_history_with_group_protection(
                        &mut history,
                        self.max_items,
                        &mut categories,
                        self.grouped_items_protected_from_limit,
                    );
                    normalize_pinned_items(&mut pinned_items, &history);
                    apply_pin_order(&mut history, &pinned_items);
                    let category_list = lock_arc_mutex(&self.category_list);
                    let data = ClipboardHistoryData {
                        items: history.clone(),
                        categories: categories.clone(),
                        category_list: category_list.clone(),
                        pinned_items: pinned_items.clone(),
                    };
                    self.enqueue_persist(data);
                    *fingerprints = build_history_fingerprints(&history);
                    self.history_cache_dirty.store(false, Ordering::Relaxed);
                    return;
                }
                exact_index_cache.pop(&content_hash);
            }

            // 检查指纹索引
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
                exact_index_cache.clear();
                exact_index_cache.put(content_hash, 0);
                let mut categories = lock_arc_mutex(&self.categories);
                let mut pinned_items = lock_arc_mutex(&self.pinned_items);
                shrink_text_history_with_group_protection(
                    &mut history,
                    self.max_items,
                    &mut categories,
                    self.grouped_items_protected_from_limit,
                );
                normalize_pinned_items(&mut pinned_items, &history);
                apply_pin_order(&mut history, &pinned_items);
                let category_list = lock_arc_mutex(&self.category_list);
                let data = ClipboardHistoryData {
                    items: history.clone(),
                    categories: categories.clone(),
                    category_list: category_list.clone(),
                    pinned_items: pinned_items.clone(),
                };
                self.enqueue_persist(data);
                *fingerprints = build_history_fingerprints(&history);
                self.history_cache_dirty.store(false, Ordering::Relaxed);
                return;
            }
        } else {
            drop(bloom_filter);
            // 布隆过滤器明确不存在，直接添加到历史记录
            log::debug!("布隆过滤器预筛选：内容不存在，直接添加");
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

            history.insert(0, content.clone());

            // 优化：将新内容添加到布隆过滤器
            let mut bloom_filter = self.bloom_filter.lock();
            bloom_filter.insert(&content);
        }

        let mut categories = lock_arc_mutex(&self.categories);
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        shrink_text_history_with_group_protection(
            &mut history,
            self.max_items,
            &mut categories,
            self.grouped_items_protected_from_limit,
        );
        normalize_pinned_items(&mut pinned_items, &history);
        apply_pin_order(&mut history, &pinned_items);
        let category_list = lock_arc_mutex(&self.category_list);
        let data = ClipboardHistoryData {
            items: history.clone(),
            categories: categories.clone(),
            category_list: category_list.clone(),
            pinned_items: pinned_items.clone(),
        };

        self.enqueue_persist(data);
        let mut exact_index_cache = self.exact_index_cache.lock();
        exact_index_cache.clear();
        if let Some(first) = history.first() {
            exact_index_cache.put(stable_text_hash(first), 0);
        }
        let mut fingerprints = lock_arc_mutex(&self.history_fingerprints);
        *fingerprints = build_history_fingerprints(&history);
        self.history_cache_dirty.store(false, Ordering::Relaxed);
    }

    /// 清空历史记录
    pub fn clear_history(&self) -> Result<(), String> {
        let mut history = lock_arc_mutex(&self.history);
        history.clear();
        self.exact_index_cache.lock().clear();
        self.history_cache_dirty.store(true, Ordering::Relaxed);

        let mut categories = lock_arc_mutex(&self.categories);
        categories.clear();

        let mut category_list = lock_arc_mutex(&self.category_list);
        category_list.clear();
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        pinned_items.clear();

        self.enqueue_persist(ClipboardHistoryData {
            items: Vec::new(),
            categories: HashMap::new(),
            category_list: Vec::new(),
            pinned_items: Vec::new(),
        });
        
        log::info!("历史记录已清空");
        Ok(())
    }

    /// 设置最大历史记录数量
    pub fn set_max_items(&mut self, max_items: usize) {
        self.max_items = max_items;
        log::info!("更新最大记录数为{}", max_items);

        let mut history = lock_arc_mutex(&self.history);
        if history.len() > max_items {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            shrink_text_history_with_group_protection(
                &mut history,
                max_items,
                &mut categories,
                self.grouped_items_protected_from_limit,
            );
            normalize_pinned_items(&mut pinned_items, &history);
            apply_pin_order(&mut history, &pinned_items);
            let category_list = lock_arc_mutex(&self.category_list);

            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
                pinned_items: pinned_items.clone(),
            };

            self.enqueue_persist(data);
            self.exact_index_cache.lock().clear();
            self.history_cache_dirty.store(true, Ordering::Relaxed);
        }
    }

    /// 移除指定历史记录
    pub fn remove_from_history(&self, index: usize) -> Result<String, String> {
        let mut history = lock_arc_mutex(&self.history);
        if index < history.len() {
            let item = history.remove(index);
            self.exact_index_cache.lock().clear();
            self.history_cache_dirty.store(true, Ordering::Relaxed);

            let mut categories = lock_arc_mutex(&self.categories);
            categories.remove(&item);
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            normalize_pinned_items(&mut pinned_items, &history);

            let category_list = lock_arc_mutex(&self.category_list);
            let data = ClipboardHistoryData {
                items: history.clone(),
                categories: categories.clone(),
                category_list: category_list.clone(),
                pinned_items: pinned_items.clone(),
            };

            self.enqueue_persist(data);
            Ok(item)
        } else {
            Err("索引超出范围".to_string())
        }
    }

    pub fn promote_to_top(&self, index: usize) -> Result<String, String> {
        let (item, categories_clone, category_list_clone, history_clone, pinned_items) = {
            let mut history = lock_arc_mutex(&self.history);
            if index >= history.len() {
                return Err("索引超出范围".to_string());
            }
            if index == 0 {
                let item = history[0].clone();
                return Ok(item);
            }
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            normalize_pinned_items(&mut pinned_items, &history);
            let item = history.remove(index);
            if pinned_items.iter().any(|p| p == &item) {
                pinned_items.retain(|p| p != &item);
                pinned_items.insert(0, item.clone());
                history.insert(0, item.clone());
            } else {
                let insert_pos = pinned_items.len().min(history.len());
                history.insert(insert_pos, item.clone());
            }
            apply_pin_order(&mut history, &pinned_items);
            self.exact_index_cache.lock().clear();
            self.history_cache_dirty.store(true, Ordering::Relaxed);

            let categories = lock_arc_mutex(&self.categories).clone();
            let category_list = lock_arc_mutex(&self.category_list).clone();
            (item, categories, category_list, history.clone(), pinned_items.clone())
        };

        self.enqueue_persist(ClipboardHistoryData {
            items: history_clone,
            categories: categories_clone,
            category_list: category_list_clone,
            pinned_items,
        });

        Ok(item)
    }

    pub async fn promote_to_top_async(&self, index: usize) -> Result<String, String> {
        let item = {
            let mut history = lock_arc_mutex(&self.history);
            if index >= history.len() {
                return Err("索引超出范围".to_string());
            }
            if index == 0 {
                return Ok(history[0].clone());
            }
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            normalize_pinned_items(&mut pinned_items, &history);
            let item = history.remove(index);
            if pinned_items.iter().any(|p| p == &item) {
                pinned_items.retain(|p| p != &item);
                pinned_items.insert(0, item.clone());
                history.insert(0, item.clone());
            } else {
                let insert_pos = pinned_items.len().min(history.len());
                history.insert(insert_pos, item.clone());
            }
            apply_pin_order(&mut history, &pinned_items);
            self.exact_index_cache.lock().clear();
            self.history_cache_dirty.store(true, Ordering::Relaxed);
            item
        };
        Ok(item)
    }

    pub fn set_pinned(&self, item: String, pinned: bool) -> Result<(), String> {
        let mut history = lock_arc_mutex(&self.history);
        if !history.iter().any(|existing| existing == &item) {
            return Err("目标条目不存在".to_string());
        }
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        if pinned {
            if !pinned_items.iter().any(|p| p == &item) {
                pinned_items.insert(0, item.clone());
            }
        } else {
            pinned_items.retain(|p| p != &item);
        }
        normalize_pinned_items(&mut pinned_items, &history);
        apply_pin_order(&mut history, &pinned_items);
        self.history_cache_dirty.store(true, Ordering::Relaxed);

        let categories = lock_arc_mutex(&self.categories).clone();
        let category_list = lock_arc_mutex(&self.category_list).clone();
        self.enqueue_persist(ClipboardHistoryData {
            items: history.clone(),
            categories,
            category_list,
            pinned_items: pinned_items.clone(),
        });
        Ok(())
    }

    pub async fn set_pinned_async(&self, item: String, pinned: bool) -> Result<(), String> {
        {
            let mut history = lock_arc_mutex(&self.history);
            if !history.iter().any(|existing| existing == &item) {
                return Err("目标条目不存在".to_string());
            }
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            if pinned {
                if !pinned_items.iter().any(|p| p == &item) {
                    pinned_items.insert(0, item.clone());
                }
            } else {
                pinned_items.retain(|p| p != &item);
            }
            normalize_pinned_items(&mut pinned_items, &history);
            apply_pin_order(&mut history, &pinned_items);
            self.history_cache_dirty.store(true, Ordering::Relaxed);
        }

        let db_result = if pinned {
            crate::utils::database::pin_item(&item).await
        } else {
            crate::utils::database::unpin_item(&item).await
        };

        if db_result.is_err() {
            let mut history = lock_arc_mutex(&self.history);
            if history.iter().any(|existing| existing == &item) {
                let mut pinned_items = lock_arc_mutex(&self.pinned_items);
                if pinned {
                    pinned_items.retain(|p| p != &item);
                } else if !pinned_items.iter().any(|p| p == &item) {
                    pinned_items.insert(0, item.clone());
                }
                normalize_pinned_items(&mut pinned_items, &history);
                apply_pin_order(&mut history, &pinned_items);
                self.history_cache_dirty.store(true, Ordering::Relaxed);
            }
        }

        db_result?;

        Ok(())
    }

    pub async fn set_pinned_by_selector_async(
        &self,
        index: Option<usize>,
        item: Option<String>,
        pinned: bool,
    ) -> Result<(), String> {
        let resolved_item = item.or_else(|| index.and_then(|idx| self.get_history().get(idx).cloned()))
            .ok_or_else(|| "索引超出范围".to_string())?;
        self.set_pinned_async(resolved_item, pinned).await
    }

    pub fn clear_history_by_mode(&self, mode: &str) -> Result<usize, String> {
        let mut history = lock_arc_mutex(&self.history);
        let mut categories = lock_arc_mutex(&self.categories);
        let mut category_list = lock_arc_mutex(&self.category_list);
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        let before = history.len();

        match mode {
            "all" => {
                history.clear();
                categories.clear();
                category_list.clear();
                pinned_items.clear();
            }
            "unclassified" | "unclassified_unpinned" => {
                let classified: HashSet<String> = categories.keys().cloned().collect();
                let pinned: HashSet<String> = pinned_items.iter().cloned().collect();
                history.retain(|item| classified.contains(item) || pinned.contains(item));
                categories.retain(|item, _| history.contains(item));
                normalize_pinned_items(&mut pinned_items, &history);
                apply_pin_order(&mut history, &pinned_items);
            }
            _ => return Err("不支持的清理模式".to_string()),
        }

        self.history_cache_dirty.store(true, Ordering::Relaxed);
        self.enqueue_persist(ClipboardHistoryData {
            items: history.clone(),
            categories: categories.clone(),
            category_list: category_list.clone(),
            pinned_items: pinned_items.clone(),
        });
        Ok(before.saturating_sub(history.len()))
    }

    pub async fn clear_history_by_mode_async(&self, mode: &str) -> Result<usize, String> {
        // 先收集需要的数据，释放锁后再进行异步操作
        let (before, items_to_remove, should_clear_all) = {
            let history = lock_arc_mutex(&self.history);
            let categories = lock_arc_mutex(&self.categories);
            let pinned_items = lock_arc_mutex(&self.pinned_items);
            let before = history.len();

            match mode {
                "all" => {
                    (before, Vec::new(), true)
                }
                "unclassified" | "unclassified_unpinned" => {
                    let classified: HashSet<String> = categories.keys().cloned().collect();
                    let pinned: HashSet<String> = pinned_items.iter().cloned().collect();
                    let to_remove: Vec<String> = history
                        .iter()
                        .filter(|item| !classified.contains(*item) && !pinned.contains(*item))
                        .cloned()
                        .collect();
                    (before, to_remove, false)
                }
                _ => return Err("不支持的清理模式".to_string()),
            }
        };

        let removed_count = items_to_remove.len();

        // 释放锁后执行异步数据库操作
        if should_clear_all {
            crate::utils::database::clear_all_history().await?;
        } else if !items_to_remove.is_empty() {
            crate::utils::database::delete_history_items_bulk(&items_to_remove).await?;
        }

        // 重新获取锁更新本地状态
        {
            let mut history = lock_arc_mutex(&self.history);
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);

            if should_clear_all {
                history.clear();
                categories.clear();
                category_list.clear();
                pinned_items.clear();
            } else {
                let classified: HashSet<String> = categories.keys().cloned().collect();
                let pinned: HashSet<String> = pinned_items.iter().cloned().collect();
                history.retain(|item| classified.contains(item) || pinned.contains(item));
                categories.retain(|item, _| history.contains(item));
                normalize_pinned_items(&mut pinned_items, &history);
                apply_pin_order(&mut history, &pinned_items);
            }
            
            self.history_cache_dirty.store(true, Ordering::Relaxed);
        }

        let removed = if should_clear_all { before } else { removed_count };
        Ok(removed)
    }

    /// 退出时无需额外操作，数据已通过增量操作实时保存
    pub fn save_history_on_exit(&self) -> Result<(), String> {
        Ok(())
    }

    pub fn set_grouped_items_protected_from_limit(&mut self, enabled: bool) {
        self.grouped_items_protected_from_limit = enabled;
        let mut history = lock_arc_mutex(&self.history);
        let mut categories = lock_arc_mutex(&self.categories);
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        shrink_text_history_with_group_protection(
            &mut history,
            self.max_items,
            &mut categories,
            self.grouped_items_protected_from_limit,
        );
        normalize_pinned_items(&mut pinned_items, &history);
        apply_pin_order(&mut history, &pinned_items);
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

fn normalize_pinned_items(pinned_items: &mut Vec<String>, history: &[String]) {
    let existing: HashSet<&String> = history.iter().collect();
    let mut seen = HashSet::new();
    pinned_items.retain(|item| existing.contains(item) && seen.insert(item.clone()));
}

fn apply_pin_order(history: &mut Vec<String>, pinned_items: &[String]) {
    if pinned_items.is_empty() || history.is_empty() {
        return;
    }
    let mut ordered = Vec::with_capacity(history.len());
    for pinned in pinned_items {
        if let Some(pos) = history.iter().position(|item| item == pinned) {
            ordered.push(history.remove(pos));
        }
    }
    ordered.append(history);
    *history = ordered;
}
