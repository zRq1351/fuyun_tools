use crate::core::error_codes::AppErrorKind;
use crate::sync::{lock_arc_mutex, Mutex};
use bloom::{BloomFilter, ASMS};
use lru::LruCache;
use parking_lot::Condvar as ParkingCondvar;
use parking_lot::Mutex as ParkingMutex;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use xxhash_rust::xxh3::xxh3_64;

use crate::utils::utils_helpers::{
    find_best_replacement_candidate, load_history_data, ClipboardHistoryData,
};

struct PersistState {
    history_dirty: bool,
    categories_dirty: bool,
    pinned_dirty: bool,
    clear_all: bool,
    save_completed: bool,
}

#[derive(Clone)]
pub struct ClipboardManager {
    history: Arc<Mutex<Vec<String>>>,
    history_fingerprints: Arc<Mutex<Vec<(usize, u64)>>>,
    exact_index_cache: Arc<ParkingMutex<LruCache<u64, usize>>>,
    history_cache_dirty: Arc<AtomicBool>,
    persist_state: Arc<(ParkingMutex<PersistState>, ParkingCondvar)>,
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

fn stable_text_hash(text: &str) -> u64 {
    xxh3_64(text.as_bytes())
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
        let persist_state = Arc::new((
            ParkingMutex::new(PersistState {
                history_dirty: !initial_snapshot.items.is_empty(),
                categories_dirty: !initial_snapshot.categories.is_empty()
                    || !initial_snapshot.category_list.is_empty(),
                pinned_dirty: !initial_snapshot.pinned_items.is_empty(),
                clear_all: false,
                save_completed: false,
            }),
            ParkingCondvar::new(),
        ));

        let history_arc = Arc::new(Mutex::new(history_data.items.clone()));
        let categories_arc = Arc::new(Mutex::new(history_data.categories.clone()));
        let category_list_arc = Arc::new(Mutex::new(history_data.category_list.clone()));
        let pinned_items_arc = Arc::new(Mutex::new(pinned_items.clone()));

        let state_clone = persist_state.clone();
        let hist_clone = history_arc.clone();
        let cat_clone = categories_arc.clone();
        let cat_list_clone = category_list_arc.clone();
        let pinned_clone = pinned_items_arc.clone();

        std::thread::spawn(move || {
            let (lock, cvar) = &*state_clone;
            loop {
                let mut state = lock.lock();
                while !state.history_dirty
                    && !state.categories_dirty
                    && !state.pinned_dirty
                    && !state.clear_all
                {
                    cvar.wait(&mut state);
                }

                let history_dirty = state.history_dirty;
                let categories_dirty = state.categories_dirty;
                let pinned_dirty = state.pinned_dirty;
                let clear_all = state.clear_all;

                state.history_dirty = false;
                state.categories_dirty = false;
                state.pinned_dirty = false;
                state.clear_all = false;
                drop(state);

                if clear_all {
                    if let Err(e) =
                        tauri::async_runtime::block_on(crate::utils::database::clear_all_history())
                    {
                        log::error!("清理历史记录失败: {}", e);
                    }
                } else {
                    if history_dirty {
                        let items = lock_arc_mutex(&hist_clone).clone();
                        if let Err(e) = tauri::async_runtime::block_on(
                            crate::utils::database::save_history_items_only_async(&items),
                        ) {
                            log::error!("保存历史记录失败: {}", e);
                        }
                    }
                    if categories_dirty {
                        let categories = lock_arc_mutex(&cat_clone).clone();
                        let category_list = lock_arc_mutex(&cat_list_clone).clone();
                        if let Err(e) = tauri::async_runtime::block_on(
                            crate::utils::database::save_categories_state_async(
                                &categories,
                                &category_list,
                            ),
                        ) {
                            log::error!("保存分类状态失败: {}", e);
                        }
                    }
                    if pinned_dirty {
                        let pinned_items = lock_arc_mutex(&pinned_clone).clone();
                        if let Err(e) = tauri::async_runtime::block_on(
                            crate::utils::database::save_pinned_items_order_async(&pinned_items),
                        ) {
                            log::error!("保存置顶状态失败: {}", e);
                        }
                    }
                }

                // 通知等待的线程保存已完成
                // 仅当没有新的脏数据时才标记完成，否则循环继续处理
                let mut state = lock.lock();
                if state.history_dirty || state.categories_dirty || state.pinned_dirty || state.clear_all {
                    // save_history_on_exit 在我们做 I/O 期间设置了新的脏标记
                    // 不设置 save_completed，让循环继续处理新数据
                } else {
                    state.save_completed = true;
                    cvar.notify_all();
                }
            }
        });

        // 初始化布隆过滤器
        let mut bloom_filter =
            BloomFilter::with_rate(BLOOM_FILTER_ERROR_RATE, BLOOM_FILTER_CAPACITY);

        // 将现有历史记录添加到布隆过滤器
        for item in &history_data.items {
            bloom_filter.insert(item);
        }

        Self {
            history: history_arc,
            history_fingerprints: Arc::new(Mutex::new(history_fingerprints)),
            exact_index_cache: Arc::new(ParkingMutex::new(LruCache::new(
                NonZeroUsize::new(EXACT_INDEX_CACHE_CAPACITY).unwrap_or(NonZeroUsize::MIN),
            ))),
            history_cache_dirty: Arc::new(AtomicBool::new(false)),
            persist_state,
            categories: categories_arc,
            category_list: category_list_arc,
            pinned_items: pinned_items_arc,
            max_items,
            grouped_items_protected_from_limit,
            bloom_filter: Arc::new(ParkingMutex::new(bloom_filter)),
        }
    }

    fn enqueue_history_only_persist(&self) {
        let (lock, cvar) = &*self.persist_state;
        let mut state = lock.lock();
        state.history_dirty = true;
        cvar.notify_one();
    }

    fn enqueue_categories_only_persist(&self) {
        let (lock, cvar) = &*self.persist_state;
        let mut state = lock.lock();
        state.categories_dirty = true;
        cvar.notify_one();
    }

    fn enqueue_pinned_only_persist(&self) {
        let (lock, cvar) = &*self.persist_state;
        let mut state = lock.lock();
        state.pinned_dirty = true;
        cvar.notify_one();
    }

    fn enqueue_clear_all_persist(&self) {
        let (lock, cvar) = &*self.persist_state;
        let mut state = lock.lock();
        state.clear_all = true;
        cvar.notify_one();
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

        let result = crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
            app_handle.clipboard().write_text(content)
        });

        match result {
            Ok(()) => {
                log::info!("成功设置剪贴板内容");
                Ok(())
            }
            Err(e) => {
                let error_msg = AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e));
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

    pub fn get_history_len(&self) -> usize {
        lock_arc_mutex(&self.history).len()
    }

    pub fn get_history_item(&self, item_id: &str) -> Option<String> {
        let history = lock_arc_mutex(&self.history);
        let index = self.find_index_by_id_with_lock(&history, item_id)?;
        history.get(index).cloned()
    }

    pub fn get_latest_item(&self) -> Option<String> {
        lock_arc_mutex(&self.history).first().cloned()
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
        {
            let mut category_list = lock_arc_mutex(&self.category_list);

            let normalized_category = category.trim().to_string();

            if !normalized_category.is_empty()
                && normalized_category != "未分类"
                && normalized_category != "全部"
                && !category_list.contains(&normalized_category)
            {
                category_list.push(normalized_category);
            }
        }

        self.enqueue_categories_only_persist();

        Ok(())
    }

    pub async fn add_category_async(&self, category: String) -> Result<(), String> {
        let normalized_category = category.trim().to_string();
        if !normalized_category.is_empty()
            && normalized_category != "未分类"
            && normalized_category != "全部"
        {
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
    pub fn set_category(&self, item_id: String, category: String) -> Result<(), String> {
        {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);

            let normalized_category = category.trim().to_string();

            if normalized_category.is_empty()
                || normalized_category == "未分类"
                || normalized_category == "全部"
            {
                categories.remove(&item_id);
            } else {
                categories.insert(item_id, normalized_category.clone());
                if !category_list.contains(&normalized_category) {
                    category_list.push(normalized_category);
                }
            }
        }

        self.enqueue_categories_only_persist();

        Ok(())
    }

    pub async fn set_category_async(
        &self,
        item_id: String,
        category: String,
    ) -> Result<(), String> {
        let normalized_category = category.trim().to_string();
        if normalized_category.is_empty()
            || normalized_category == "未分类"
            || normalized_category == "全部"
        {
            {
                let mut categories = lock_arc_mutex(&self.categories);
                categories.remove(&item_id);
            }
            crate::utils::database::remove_item_category(&item_id).await?;
        } else {
            {
                let mut categories = lock_arc_mutex(&self.categories);
                let mut category_list = lock_arc_mutex(&self.category_list);
                categories.insert(item_id.clone(), normalized_category.clone());
                if !category_list.contains(&normalized_category) {
                    category_list.push(normalized_category.clone());
                }
            }
            crate::utils::database::set_item_category(&item_id, &normalized_category).await?;
            crate::utils::database::add_category_to_list(&normalized_category).await?;
        }
        Ok(())
    }

    /// 移除分类
    pub fn remove_category(&self, category: String) -> Result<(), String> {
        {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);

            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
        }

        self.enqueue_categories_only_persist();

        Ok(())
    }

    pub async fn remove_category_async(&self, category: String) -> Result<(), String> {
        {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
        }
        crate::utils::database::remove_category_everywhere(&category).await?;
        Ok(())
    }

    /// 将内容添加到剪贴板历史记录中
    pub fn add_to_history(&self, content: String) {
        let mut history = lock_arc_mutex(&self.history);

        // 优化：使用 len() 获取字节长度，避免 O(N) 遍历超大字符串的 chars
        let content_len = content.len();
        log::debug!(
            "添加到历史记录，长度: {}, 当前数量: {}",
            content_len,
            history.len()
        );

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
                if history
                    .get(cached_index)
                    .is_some_and(|item| item == &content)
                {
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
                        &pinned_items,
                        self.grouped_items_protected_from_limit,
                    );
                    normalize_pinned_items(&mut pinned_items, &history);
                    apply_pin_order(&mut history, &pinned_items);
                    self.enqueue_history_only_persist();
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
            if let Some(exact_index) =
                fingerprints
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
                    &pinned_items,
                    self.grouped_items_protected_from_limit,
                );
                normalize_pinned_items(&mut pinned_items, &history);
                apply_pin_order(&mut history, &pinned_items);
                self.enqueue_history_only_persist();
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
            // 优化：限制小文本的扫描范围，避免历史记录很大时出现 O(N) 遍历阻塞
            history.len().min(LONG_TEXT_DEDUP_SCAN_LIMIT * 2)
        };
        let candidate_history = &history[..scan_len];

        if let Some((replace_index, comparison)) =
            find_best_replacement_candidate(&content, candidate_history, similarity_threshold)
        {
            log::info!("检测到相似版本，正在处理: {}", comparison.reason);
            log::info!(
                "相似度: {:.4}, 完整性: {:?}",
                comparison.similarity_score,
                comparison.new_completeness
            );

            if comparison.reason.contains("子集") || comparison.reason.contains("找回完整版本")
            {
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
            &pinned_items,
            self.grouped_items_protected_from_limit,
        );
        normalize_pinned_items(&mut pinned_items, &history);
        apply_pin_order(&mut history, &pinned_items);
        self.enqueue_history_only_persist();
        // 锁顺序必须与 Path A 一致: history_fingerprints -> exact_index_cache
        let mut fingerprints = lock_arc_mutex(&self.history_fingerprints);
        let mut exact_index_cache = self.exact_index_cache.lock();
        exact_index_cache.clear();
        if let Some(first) = history.first() {
            exact_index_cache.put(stable_text_hash(first), 0);
        }
        *fingerprints = build_history_fingerprints(&history);
        self.history_cache_dirty.store(false, Ordering::Relaxed);
    }

    pub async fn update_item_content(&self, old_item_id: &str, new_content: String) -> Result<(), String> {
        let new_item_id = crate::utils::database::stable_history_item_id(&new_content);

        // === 阶段一：持有 history 锁，原子完成所有内存状态修改 ===
        // 这确保 index 在整个操作期间不会因并发修改而失效
        let (category_to_db, was_pinned) = {
            let mut history = lock_arc_mutex(&self.history);
            let index = self
                .find_index_by_id_with_lock(&history, old_item_id)
                .ok_or_else(|| AppErrorKind::ClipboardItemNotFound.to_frontend_json())?;

            let old_content = history[index].clone();
            if old_content == new_content {
                return Ok(());
            }

            // 更新 history 内容
            history[index] = new_content.clone();

            // 同步更新 categories 内存状态
            let category_to_db = {
                let mut categories = lock_arc_mutex(&self.categories);
                if let Some(cat) = categories.remove(old_item_id) {
                    categories.insert(new_item_id.clone(), cat.clone());
                    Some((new_item_id.clone(), cat))
                } else {
                    None
                }
            };

            // 同步更新 pinned_items 内存状态
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            let was_pinned = if let Some(pos) = pinned_items.iter().position(|id| id == old_item_id) {
                pinned_items[pos] = new_item_id.clone();
                true
            } else {
                false
            };

            // 清理缓存
            self.exact_index_cache.lock().clear();
            self.history_cache_dirty.store(true, Ordering::Relaxed);

            // 重建指纹索引（在 history 锁内完成，保证一致性）
            let mut fingerprints = lock_arc_mutex(&self.history_fingerprints);
            *fingerprints = build_history_fingerprints(&history);

            (category_to_db, was_pinned)
        };
        // history 锁在此释放，所有内存状态已一致

        // === 阶段二：异步持久化到数据库（无需持有锁）===
        if let Some((item_id, cat)) = category_to_db {
            let _ = crate::utils::database::set_item_category(&item_id, &cat).await;
        }
        let _ = crate::utils::database::remove_item_category(old_item_id).await;

        if was_pinned {
            let _ = crate::utils::database::pin_item(&new_item_id).await;
            let _ = crate::utils::database::unpin_item(old_item_id).await;
        }

        self.enqueue_history_only_persist();

        Ok(())
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

        // 重置布隆过滤器
        let mut bloom_filter = self.bloom_filter.lock();
        *bloom_filter = BloomFilter::with_rate(BLOOM_FILTER_ERROR_RATE, BLOOM_FILTER_CAPACITY);
        drop(bloom_filter);

        // 清空指纹缓存
        let mut fingerprints = lock_arc_mutex(&self.history_fingerprints);
        fingerprints.clear();

        self.enqueue_clear_all_persist();

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
                &pinned_items,
                self.grouped_items_protected_from_limit,
            );
            normalize_pinned_items(&mut pinned_items, &history);
            apply_pin_order(&mut history, &pinned_items);
            self.enqueue_history_only_persist();
            self.exact_index_cache.lock().clear();
            self.history_cache_dirty.store(true, Ordering::Relaxed);
        }
    }

    fn find_index_by_id_with_lock(
        &self,
        history: &crate::sync::MutexGuard<'_, Vec<String>>,
        item_id: &str,
    ) -> Option<usize> {
        let target_hash = u64::from_str_radix(item_id, 16).ok()?;

        {
            let mut cache = self.exact_index_cache.lock();
            if let Some(&idx) = cache.get(&target_hash) {
                if idx < history.len() && stable_text_hash(&history[idx]) == target_hash {
                    return Some(idx);
                }
            }
        }

        history
            .iter()
            .position(|entry| stable_text_hash(entry) == target_hash)
    }

    /// 移除指定历史记录
    pub fn remove_from_history(&self, item_id: &str) -> Result<String, String> {
        let mut history = lock_arc_mutex(&self.history);
        let index = self
            .find_index_by_id_with_lock(&history, item_id)
            .ok_or_else(|| AppErrorKind::ClipboardItemNotFound.to_frontend_json())?;

        if index < history.len() {
            let item = history.remove(index);
            self.exact_index_cache.lock().clear();
            self.history_cache_dirty.store(true, Ordering::Relaxed);

            let mut categories = lock_arc_mutex(&self.categories);
            categories.remove(item_id);
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            pinned_items.retain(|p| p != item_id);
            normalize_pinned_items(&mut pinned_items, &history);

            self.enqueue_history_only_persist();
            Ok(item)
        } else {
            Err(AppErrorKind::SystemIndexOutOfRange.to_frontend_json())
        }
    }

    pub fn promote_to_top(&self, item_id: &str) -> Result<String, String> {
        let item = {
            let mut history = lock_arc_mutex(&self.history);
            let index = self
                .find_index_by_id_with_lock(&history, item_id)
                .ok_or_else(|| AppErrorKind::ClipboardItemNotFound.to_frontend_json())?;

            if index >= history.len() {
                return Err(AppErrorKind::SystemIndexOutOfRange.to_frontend_json());
            }
            if index == 0 {
                let item = history[0].clone();
                return Ok(item);
            }
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            normalize_pinned_items(&mut pinned_items, &history);
            let item = history.remove(index);
            let item_id_val = crate::utils::database::stable_history_item_id(&item);
            if pinned_items.iter().any(|p| p == &item_id_val) {
                pinned_items.retain(|p| p != &item_id_val);
                pinned_items.insert(0, item_id_val.clone());
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

        self.enqueue_history_only_persist();
        self.enqueue_pinned_only_persist();

        Ok(item)
    }

    pub async fn promote_to_top_async(&self, item_id: &str) -> Result<String, String> {
        let item = {
            let mut history = lock_arc_mutex(&self.history);
            let index = self
                .find_index_by_id_with_lock(&history, item_id)
                .ok_or_else(|| AppErrorKind::ClipboardItemNotFound.to_frontend_json())?;

            if index >= history.len() {
                return Err(AppErrorKind::SystemIndexOutOfRange.to_frontend_json());
            }
            if index == 0 {
                return Ok(history[0].clone());
            }
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            normalize_pinned_items(&mut pinned_items, &history);
            let item = history.remove(index);
            let item_id_val = crate::utils::database::stable_history_item_id(&item);
            if pinned_items.iter().any(|p| p == &item_id_val) {
                pinned_items.retain(|p| p != &item_id_val);
                pinned_items.insert(0, item_id_val.clone());
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
        // Bug修复 (B4): 异步版本也需要触发持久化，否则应用重启后数据丢失
        self.enqueue_history_only_persist();
        self.enqueue_pinned_only_persist();
        Ok(item)
    }

    pub fn set_pinned(&self, item_id: String, pinned: bool) -> Result<(), String> {
        let mut history = lock_arc_mutex(&self.history);
        let exists = history
            .iter()
            .any(|existing| crate::utils::database::stable_history_item_id(existing) == item_id);
        if !exists {
            return Err(AppErrorKind::InternalError.to_frontend_json());
        }
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        if pinned {
            if !pinned_items.iter().any(|p| p == &item_id) {
                pinned_items.insert(0, item_id.clone());
            }
        } else {
            pinned_items.retain(|p| p != &item_id);
        }
        normalize_pinned_items(&mut pinned_items, &history);
        apply_pin_order(&mut history, &pinned_items);
        self.history_cache_dirty.store(true, Ordering::Relaxed);

        self.enqueue_history_only_persist();
        self.enqueue_pinned_only_persist();
        Ok(())
    }

    pub async fn set_pinned_async(&self, item_id: String, pinned: bool) -> Result<(), String> {
        {
            let mut history = lock_arc_mutex(&self.history);
            let exists = history.iter().any(|existing| {
                crate::utils::database::stable_history_item_id(existing) == item_id
            });
            if !exists {
                return Err(AppErrorKind::InternalError.to_frontend_json());
            }
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            if pinned {
                if !pinned_items.iter().any(|p| p == &item_id) {
                    pinned_items.insert(0, item_id.clone());
                }
            } else {
                pinned_items.retain(|p| p != &item_id);
            }
            normalize_pinned_items(&mut pinned_items, &history);
            apply_pin_order(&mut history, &pinned_items);
            self.history_cache_dirty.store(true, Ordering::Relaxed);
        }

        let db_result = if pinned {
            crate::utils::database::pin_item(&item_id).await
        } else {
            crate::utils::database::unpin_item(&item_id).await
        };

        if let Err(ref e) = db_result {
            log::warn!("set_pinned_async: 数据库操作失败，执行回滚: item_id={}, pinned={}, error={}", item_id, pinned, e);
            let mut history = lock_arc_mutex(&self.history);
            let exists = history.iter().any(|existing| {
                crate::utils::database::stable_history_item_id(existing) == item_id
            });
            if exists {
                let mut pinned_items = lock_arc_mutex(&self.pinned_items);
                // 回滚：撤销之前的内存修改
                if pinned {
                    // 如果是要置顶但失败了，移除刚添加的置顶
                    pinned_items.retain(|p| p != &item_id);
                } else if !pinned_items.iter().any(|p| p == &item_id) {
                    // 如果是要取消置顶但失败了，恢复置顶
                    pinned_items.insert(0, item_id.clone());
                }
                normalize_pinned_items(&mut pinned_items, &history);
                apply_pin_order(&mut history, &pinned_items);
                self.history_cache_dirty.store(true, Ordering::Relaxed);
                log::info!("set_pinned_async: 回滚完成: item_id={}", item_id);
            }
        }

        db_result?;

        Ok(())
    }

    pub async fn set_pinned_by_selector_async(
        &self,
        item_id: &str,
        pinned: bool,
    ) -> Result<(), String> {
        self.set_pinned_async(item_id.to_string(), pinned).await
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
                history.retain(|item| {
                    let item_id = crate::utils::database::stable_history_item_id(item);
                    classified.contains(&item_id) || pinned.contains(&item_id)
                });
                let history_ids: HashSet<String> = history
                    .iter()
                    .map(|item| crate::utils::database::stable_history_item_id(item))
                    .collect();
                categories.retain(|item_id, _| history_ids.contains(item_id));
                normalize_pinned_items(&mut pinned_items, &history);
                apply_pin_order(&mut history, &pinned_items);
            }
            _ => return Err(AppErrorKind::SystemUnsupportedCleanMode.to_frontend_json()),
        }

        self.history_cache_dirty.store(true, Ordering::Relaxed);
        if mode == "all" {
            self.enqueue_clear_all_persist();
        } else {
            self.enqueue_history_only_persist();
        }
        Ok(before.saturating_sub(history.len()))
    }

    pub async fn clear_history_by_mode_async(&self, mode: &str) -> Result<usize, String> {
        self.clear_history_by_mode(mode)
    }

    /// 退出时强制刷新所有脏数据到数据库
    /// Bug修复 (B6): 确保应用退出前数据不丢失
    pub fn save_history_on_exit(&self) -> Result<(), String> {
        // 标记所有数据为脏，通知持久化线程立即执行
        {
            let (lock, cvar) = &*self.persist_state;
            let mut state = lock.lock();
            state.history_dirty = true;
            state.categories_dirty = true;
            state.pinned_dirty = true;
            state.save_completed = false;
            cvar.notify_one();
        }
        // 等待持久化线程完成（最多 2 秒）
        let (lock, cvar) = &*self.persist_state;
        let mut state = lock.lock();
        if !state.save_completed {
            let result = cvar.wait_for(&mut state, std::time::Duration::from_secs(2));
            if result.timed_out() {
                log::warn!("等待持久化线程完成超时");
            }
        }
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
            &pinned_items,
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
    pinned_items: &[String],
    grouped_items_protected_from_limit: bool,
) {
    if !grouped_items_protected_from_limit {
        if history.len() > max_items {
            let removed = history.split_off(max_items);
            for item in removed {
                let item_id = crate::utils::database::stable_history_item_id(&item);
                categories.remove(&item_id);
            }
        }
        return;
    }

    let excess = history.len().saturating_sub(max_items);
    if excess == 0 {
        return;
    }

    let mut removed_count = 0;
    let mut to_remove = HashSet::new();
    let pinned_set: HashSet<&String> = pinned_items.iter().collect();

    for (i, item) in history.iter().enumerate().rev() {
        let item_id = crate::utils::database::stable_history_item_id(item);
        if !categories.contains_key(&item_id) && !pinned_set.contains(&item_id) {
            to_remove.insert(i);
            removed_count += 1;
            if removed_count == excess {
                break;
            }
        }
    }

    if removed_count > 0 {
        let mut idx = 0;
        history.retain(|item| {
            let keep = !to_remove.contains(&idx);
            if !keep {
                let item_id = crate::utils::database::stable_history_item_id(item);
                categories.remove(&item_id);
            }
            idx += 1;
            keep
        });
    }
}

fn normalize_pinned_items(pinned_items: &mut Vec<String>, history: &[String]) {
    let history_ids: HashSet<String> = history
        .iter()
        .map(|item| crate::utils::database::stable_history_item_id(item))
        .collect();
    pinned_items.retain(|p| history_ids.contains(p));
}

fn apply_pin_order(history: &mut Vec<String>, pinned_items: &[String]) {
    let mut pinned_set: HashSet<String> = pinned_items.iter().cloned().collect();
    let mut pinned_list = Vec::new();
    let mut unpinned_list = Vec::new();

    for item in history.drain(..) {
        let item_id = crate::utils::database::stable_history_item_id(&item);
        if pinned_set.remove(&item_id) {
            pinned_list.push(item);
        } else {
            unpinned_list.push(item);
        }
    }

    // Bug修复 (B1): 清理 pinned_items 中不在 history 中的孤立条目
    if !pinned_set.is_empty() {
        log::debug!(
            "apply_pin_order: 清理 {} 个孤立置顶条目",
            pinned_set.len()
        );
    }

    let mut pinned_map: HashMap<String, String> = pinned_list
        .into_iter()
        .map(|item| (crate::utils::database::stable_history_item_id(&item), item))
        .collect();
    let mut sorted_pinned = Vec::new();
    for p in pinned_items {
        if let Some(item) = pinned_map.remove(p) {
            sorted_pinned.push(item);
        }
    }

    history.extend(sorted_pinned);
    history.extend(unpinned_list);
}
