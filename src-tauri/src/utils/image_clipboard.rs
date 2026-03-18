use crate::sync::Mutex;
use crate::utils::image_store;
use crate::utils::utils_helpers::atomic_write_with_backup;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use image::imageops::FilterType;
use image::{DynamicImage, ImageEncoder, RgbaImage};
use lru::LruCache;
use parking_lot::Mutex as ParkingMutex;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::env;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::image::Image;
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::UINT;
#[cfg(target_os = "windows")]
use winapi::um::shellapi::DragQueryFileW;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, CF_HDROP};

const MAX_UI_HISTORY_ITEMS: usize = 30;
const IMAGE_FULL_RES_MEMORY_BUDGET_BYTES: usize = 256 * 1024 * 1024;
const IMAGE_FULL_RES_CACHE_KEEP_RECENT: usize = 6;
const IMAGE_FULL_RES_LRU_MAX_CAPACITY: usize = 4096;
const IMAGE_PERSIST_QUEUE_SIZE: usize = 6;
const IMAGE_PREVIEW_MAX_EDGE: u32 = 320;
const BITMAP_STORAGE_USE_LOSSLESS_WEBP: bool = true;
const IMAGE_PNG_BASE64_CACHE_CAPACITY: usize = 64;
const IMAGE_FILL_VERIFY_MODE_STRICT: u8 = 0;
const IMAGE_FILL_VERIFY_MODE_FAST: u8 = 1;
static IMAGE_FILL_VERIFY_MODE: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(IMAGE_FILL_VERIFY_MODE_STRICT);
static IMAGE_PNG_BASE64_CACHE: LazyLock<ParkingMutex<LruCache<String, (u64, String)>>> =
    LazyLock::new(|| {
        ParkingMutex::new(LruCache::new(
            NonZeroUsize::new(IMAGE_PNG_BASE64_CACHE_CAPACITY).unwrap_or(NonZeroUsize::MIN),
        ))
    });
pub type ClipboardImagePayload = (Vec<u8>, u32, u32, Option<(Vec<u8>, String)>);

fn lock_arc_mutex<'a, T>(mutex: &'a Arc<Mutex<T>>) -> crate::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(e) => match e {},
    }
}

pub fn set_image_fill_verify_mode(mode: &str) {
    let value = if mode == "fast" {
        IMAGE_FILL_VERIFY_MODE_FAST
    } else {
        IMAGE_FILL_VERIFY_MODE_STRICT
    };
    IMAGE_FILL_VERIFY_MODE.store(value, Ordering::SeqCst);
}

fn is_fast_fill_verify_mode() -> bool {
    IMAGE_FILL_VERIFY_MODE.load(Ordering::SeqCst) == IMAGE_FILL_VERIFY_MODE_FAST
}

pub fn is_fast_fill_verify_mode_enabled() -> bool {
    is_fast_fill_verify_mode()
}

#[derive(Clone)]
struct PendingImageData {
    rgba: Arc<Vec<u8>>,
    width: u32,
    height: u32,
}

struct PersistTask {
    item_id: String,
    image_path: String,
    rgba: Arc<Vec<u8>>,
    width: u32,
    height: u32,
    encoded_bytes: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageHistoryItem {
    pub id: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub preview_width: u32,
    #[serde(default)]
    pub preview_height: u32,
    #[serde(default)]
    pub preview_rgba_base64: String,
    pub image_path: String,
    #[serde(skip, default)]
    pub rgba_bytes: Vec<u8>,
    pub signature: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ImageHistoryPreviewItem {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub preview_width: u32,
    pub preview_height: u32,
    pub preview_rgba_base64: String,
    pub image_path: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImageHistoryPageItem {
    pub position: usize,
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub preview_width: u32,
    pub preview_height: u32,
    pub preview_rgba_base64: String,
    pub preview_png_base64: String,
    pub image_path: String,
    pub category: String,
    pub tags: Vec<String>,
    pub pinned: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImageHistoryPageData {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<ImageHistoryPageItem>,
    pub category_list: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ImageHistoryData {
    pub items: Vec<ImageHistoryItem>,
    #[serde(default)]
    pub categories: HashMap<String, String>,
    #[serde(default)]
    pub category_list: Vec<String>,
    #[serde(default)]
    pub image_tags: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub pinned_items: Vec<String>,
}

#[derive(Clone)]
pub struct ImageClipboardManager {
    history: Arc<Mutex<Vec<ImageHistoryItem>>>,
    signature_index: Arc<Mutex<HashMap<String, usize>>>,
    signature_index_dirty: Arc<AtomicBool>,
    categories: Arc<Mutex<HashMap<String, String>>>,
    category_list: Arc<Mutex<Vec<String>>>,
    image_tags: Arc<Mutex<HashMap<String, Vec<String>>>>,
    pinned_items: Arc<Mutex<Vec<String>>>,
    pending_images: Arc<Mutex<HashMap<String, PendingImageData>>>,
    full_res_lru: Arc<Mutex<LruCache<String, ()>>>,
    persist_tx: SyncSender<PersistTask>,
    max_items: usize,
    image_disk_limit_bytes: u64,
    grouped_items_protected_from_limit: bool,
}

impl ImageClipboardManager {
    pub fn new(
        max_items: usize,
        image_disk_limit_mb: u64,
        grouped_items_protected_from_limit: bool,
    ) -> Self {
        let history_data = load_image_history_data().unwrap_or_else(|e| {
            log::error!("加载图片历史记录失败: {}，使用空历史记录", e);
            ImageHistoryData::default()
        });
        let mut pinned_items = history_data.pinned_items.clone();
        normalize_pinned_items(&mut pinned_items, &history_data.items);

        let signature_index = build_signature_index(&history_data.items);
        let pending_images = Arc::new(Mutex::new(HashMap::new()));
        let (persist_tx, persist_rx) = sync_channel::<PersistTask>(IMAGE_PERSIST_QUEUE_SIZE);
        start_image_persist_worker(persist_rx, pending_images.clone());
        let full_res_lru_capacity = max_items
            .max(IMAGE_FULL_RES_CACHE_KEEP_RECENT)
            .min(IMAGE_FULL_RES_LRU_MAX_CAPACITY)
            .max(1);
        let manager = Self {
            history: Arc::new(Mutex::new(history_data.items)),
            signature_index: Arc::new(Mutex::new(signature_index)),
            signature_index_dirty: Arc::new(AtomicBool::new(false)),
            categories: Arc::new(Mutex::new(history_data.categories)),
            category_list: Arc::new(Mutex::new(history_data.category_list)),
            image_tags: Arc::new(Mutex::new(history_data.image_tags)),
            pinned_items: Arc::new(Mutex::new(pinned_items)),
            pending_images,
            full_res_lru: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(full_res_lru_capacity).unwrap_or(NonZeroUsize::MIN),
            ))),
            persist_tx,
            max_items,
            image_disk_limit_bytes: image_disk_limit_mb.saturating_mul(1024 * 1024),
            grouped_items_protected_from_limit,
        };
        manager.compact_history_after_load();
        if let Err(e) = image_store::init_image_store() {
            log::warn!("初始化图片 SQLite 存储失败: {}", e);
        }
        manager
    }

    pub fn get_history(&self) -> Vec<ImageHistoryItem> {
        lock_arc_mutex(&self.history).clone()
    }

    pub fn get_history_preview(&self) -> Vec<ImageHistoryPreviewItem> {
        let history = lock_arc_mutex(&self.history);
        history
            .iter()
            .take(MAX_UI_HISTORY_ITEMS)
            .map(|item| ImageHistoryPreviewItem {
                id: item.id.clone(),
                width: item.width,
                height: item.height,
                preview_width: item.preview_width,
                preview_height: item.preview_height,
                preview_rgba_base64: item.preview_rgba_base64.clone(),
                image_path: item.image_path.clone(),
            })
            .collect::<Vec<_>>()
    }

    pub fn get_history_preview_page(
        &self,
        offset: usize,
        limit: usize,
        category: Option<String>,
        keyword: Option<String>,
        pinned_only: bool,
        sort_order: Option<String>,
    ) -> ImageHistoryPageData {
        if let Ok(page) = image_store::load_history_page(
            offset,
            limit,
            category.clone(),
            keyword.clone(),
            pinned_only,
            sort_order.clone(),
        ) {
            return page;
        }
        self.get_history_preview_page_fallback(
            offset,
            limit,
            category,
            keyword,
            pinned_only,
            sort_order,
        )
    }

    pub async fn get_history_preview_page_async(
        &self,
        offset: usize,
        limit: usize,
        category: Option<String>,
        keyword: Option<String>,
        pinned_only: bool,
        sort_order: Option<String>,
    ) -> ImageHistoryPageData {
        if let Ok(page) = image_store::load_history_page_async(
            offset,
            limit,
            category.clone(),
            keyword.clone(),
            pinned_only,
            sort_order.clone(),
        )
            .await
        {
            return page;
        }
        self.get_history_preview_page_fallback(
            offset,
            limit,
            category,
            keyword,
            pinned_only,
            sort_order,
        )
    }

    fn get_history_preview_page_fallback(
        &self,
        offset: usize,
        limit: usize,
        category: Option<String>,
        keyword: Option<String>,
        pinned_only: bool,
        sort_order: Option<String>,
    ) -> ImageHistoryPageData {
        let history = lock_arc_mutex(&self.history).clone();
        let categories = lock_arc_mutex(&self.categories).clone();
        let image_tags = lock_arc_mutex(&self.image_tags).clone();
        let pinned_items = lock_arc_mutex(&self.pinned_items).clone();
        let category_list = lock_arc_mutex(&self.category_list).clone();

        let category_filter = category
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty() && v != "全部");
        let keyword_filter = keyword
            .map(|v| v.trim().to_lowercase())
            .filter(|v| !v.is_empty());
        let pinned_set: HashSet<String> = pinned_items.iter().cloned().collect();
        let unpinned_desc = matches!(sort_order.as_deref(), Some("desc") | Some("DESC"));

        let mut rows: Vec<ImageHistoryPageItem> = history
            .iter()
            .enumerate()
            .map(|(position, item)| {
                let category = categories
                    .get(&item.id)
                    .cloned()
                    .unwrap_or_else(|| "未分类".to_string());
                let tags = image_tags.get(&item.id).cloned().unwrap_or_default();
                let pinned = pinned_set.contains(&item.id);
                let preview_png_base64 = if item.preview_rgba_base64.is_empty()
                    || item.preview_width == 0
                    || item.preview_height == 0
                {
                    String::new()
                } else {
                    rgba_base64_to_png_base64(
                        &item.preview_rgba_base64,
                        item.preview_width,
                        item.preview_height,
                    )
                        .unwrap_or_default()
                };
                ImageHistoryPageItem {
                    position,
                    id: item.id.clone(),
                    width: item.width,
                    height: item.height,
                    preview_width: item.preview_width,
                    preview_height: item.preview_height,
                    preview_rgba_base64: item.preview_rgba_base64.clone(),
                    preview_png_base64,
                    image_path: item.image_path.clone(),
                    category,
                    tags,
                    pinned,
                }
            })
            .filter(|entry| {
                if pinned_only && !entry.pinned {
                    return false;
                }
                if let Some(filter) = category_filter.as_deref() {
                    if entry.category != filter {
                        return false;
                    }
                }
                if let Some(keyword) = keyword_filter.as_deref() {
                    let in_category = entry.category.to_lowercase().contains(keyword);
                    let in_tags = entry
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(keyword));
                    if !in_category && !in_tags {
                        return false;
                    }
                }
                true
            })
            .collect();

        rows.sort_by(|a, b| {
            let pin_diff = (b.pinned as i32) - (a.pinned as i32);
            if pin_diff != 0 {
                return pin_diff.cmp(&0);
            }
            if a.pinned && b.pinned {
                return a.position.cmp(&b.position);
            }
            if unpinned_desc {
                b.position.cmp(&a.position)
            } else {
                a.position.cmp(&b.position)
            }
        });

        let total = rows.len();
        let effective_limit = limit.clamp(1, 200);
        let items = rows
            .into_iter()
            .skip(offset)
            .take(effective_limit)
            .collect::<Vec<_>>();

        ImageHistoryPageData {
            total,
            offset,
            limit: effective_limit,
            items,
            category_list,
        }
    }

    pub fn get_categories(&self) -> HashMap<String, String> {
        lock_arc_mutex(&self.categories).clone()
    }

    pub fn get_category_list(&self) -> Vec<String> {
        lock_arc_mutex(&self.category_list).clone()
    }

    pub fn get_image_tags(&self) -> HashMap<String, Vec<String>> {
        lock_arc_mutex(&self.image_tags).clone()
    }

    pub fn get_pinned_items(&self) -> Vec<String> {
        lock_arc_mutex(&self.pinned_items).clone()
    }

    pub fn set_max_items(&mut self, max_items: usize) {
        self.max_items = max_items;
        let mut history = lock_arc_mutex(&self.history);
        if history.len() > max_items {
            let before_ids = history.iter().map(|item| item.id.clone()).collect::<HashSet<_>>();
            let mut categories = lock_arc_mutex(&self.categories);
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            let overflow_paths =
                shrink_image_history_with_group_protection(
                    &mut history,
                    max_items,
                    &mut categories,
                    self.grouped_items_protected_from_limit,
                );
            cleanup_image_blob_files_async(overflow_paths);
            normalize_pinned_items(&mut pinned_items, &history);
            apply_pin_order(&mut history, &pinned_items);
            self.prune_pending_images_by_history(&history);
            self.prune_full_res_cache_access_by_history(&history);
            let items_for_store = history.iter().map(compact_item_for_persist).collect::<Vec<_>>();
            let after_ids = history.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
            let after_id_set = after_ids.iter().cloned().collect::<HashSet<_>>();
            let removed_ids = before_ids
                .into_iter()
                .filter(|id| !after_id_set.contains(id))
                .collect::<Vec<_>>();
            let pinned_for_store = pinned_items.clone();
            drop(history);
            self.signature_index_dirty.store(true, Ordering::SeqCst);
            for (position, item) in items_for_store.iter().enumerate() {
                if let Err(e) = image_store::upsert_item(item, position) {
                    log::error!("同步图片项失败: {}", e);
                }
            }
            for removed_id in removed_ids {
                if let Err(e) = image_store::delete_item(&removed_id) {
                    log::error!("删除图片项失败: {}", e);
                }
                if let Err(e) = image_store::delete_category(&removed_id) {
                    log::error!("删除分类失败: {}", e);
                }
                if let Err(e) = image_store::delete_tags_for_item(&removed_id) {
                    log::error!("删除标签失败: {}", e);
                }
            }
            if let Err(e) = image_store::sync_pinned_order(&pinned_for_store) {
                log::error!("同步置顶列表失败: {}", e);
            }
            if let Err(e) = image_store::sync_item_positions(&after_ids) {
                log::error!("同步图片位置失败: {}", e);
            }
        }
    }

    pub fn add_category(&self, category: String) -> Result<(), String> {
        {
            let mut list = lock_arc_mutex(&self.category_list);
            let normalized = category.trim().to_string();
            if !normalized.is_empty()
                && normalized != "未分类"
                && normalized != "全部"
                && !list.contains(&normalized)
            {
                list.push(normalized);
            }
        }
        let list = lock_arc_mutex(&self.category_list).clone();
        if let Err(e) = image_store::sync_category_list_order(&list) {
            log::error!("同步分类列表失败: {}", e);
        }
        Ok(())
    }

    pub async fn add_category_async(&self, category: String) -> Result<(), String> {
        {
            let mut list = lock_arc_mutex(&self.category_list);
            let normalized = category.trim().to_string();
            if !normalized.is_empty()
                && normalized != "未分类"
                && normalized != "全部"
                && !list.contains(&normalized)
            {
                list.push(normalized);
            }
        }
        let list = lock_arc_mutex(&self.category_list).clone();
        if let Err(e) = image_store::sync_category_list_order_async(&list).await {
            log::error!("同步分类列表失败: {}", e);
        }
        Ok(())
    }

    pub fn remove_category(&self, category: String) -> Result<(), String> {
        {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
        }
        if let Err(e) = image_store::delete_categories_by_category(&category) {
            log::error!("删除分类失败: {}", e);
        }
        let list = lock_arc_mutex(&self.category_list).clone();
        if let Err(e) = image_store::sync_category_list_order(&list) {
            log::error!("同步分类列表失败: {}", e);
        }
        Ok(())
    }

    pub async fn remove_category_async(&self, category: String) -> Result<(), String> {
        {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
        }
        if let Err(e) = image_store::delete_categories_by_category_async(&category).await {
            log::error!("删除分类失败: {}", e);
        }
        let list = lock_arc_mutex(&self.category_list).clone();
        if let Err(e) = image_store::sync_category_list_order_async(&list).await {
            log::error!("同步分类列表失败: {}", e);
        }
        Ok(())
    }

    pub fn set_category(&self, item_id: String, category: String) -> Result<(), String> {
        let mut added_to_category_list = false;
        let normalized = category.trim().to_string();
        {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            if normalized.is_empty() || normalized == "未分类" || normalized == "全部" {
                categories.remove(&item_id);
            } else {
                categories.insert(item_id.clone(), normalized.clone());
                if !category_list.contains(&normalized) {
                    category_list.push(normalized.clone());
                    added_to_category_list = true;
                }
            }
        }
        if normalized.is_empty() || normalized == "未分类" || normalized == "全部" {
            if let Err(e) = image_store::delete_category(&item_id) {
                log::error!("删除分类失败: {}", e);
            }
        } else if let Err(e) = image_store::upsert_category(&item_id, &normalized) {
            log::error!("写入分类失败: {}", e);
        }
        if added_to_category_list {
            let list = lock_arc_mutex(&self.category_list).clone();
            if let Err(e) = image_store::sync_category_list_order(&list) {
                log::error!("同步分类列表失败: {}", e);
            }
        }
        Ok(())
    }

    pub async fn set_category_async(&self, item_id: String, category: String) -> Result<(), String> {
        let mut added_to_category_list = false;
        let normalized = category.trim().to_string();
        {
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            if normalized.is_empty() || normalized == "未分类" || normalized == "全部" {
                categories.remove(&item_id);
            } else {
                categories.insert(item_id.clone(), normalized.clone());
                if !category_list.contains(&normalized) {
                    category_list.push(normalized.clone());
                    added_to_category_list = true;
                }
            }
        }
        if normalized.is_empty() || normalized == "未分类" || normalized == "全部" {
            if let Err(e) = image_store::delete_category_async(&item_id).await {
                log::error!("删除分类失败: {}", e);
            }
        } else if let Err(e) = image_store::upsert_category_async(&item_id, &normalized).await {
            log::error!("写入分类失败: {}", e);
        }
        if added_to_category_list {
            let list = lock_arc_mutex(&self.category_list).clone();
            if let Err(e) = image_store::sync_category_list_order_async(&list).await {
                log::error!("同步分类列表失败: {}", e);
            }
        }
        Ok(())
    }

    pub fn set_tags(&self, item_id: String, tags: Vec<String>) -> Result<(), String> {
        let exists = {
            let history = lock_arc_mutex(&self.history);
            history.iter().any(|item| item.id == item_id)
        };
        if !exists {
            return Err("目标图片不存在".to_string());
        }
        let normalized = normalize_tags(tags);
        {
            let mut image_tags = lock_arc_mutex(&self.image_tags);
            if normalized.is_empty() {
                image_tags.remove(&item_id);
            } else {
                image_tags.insert(item_id.clone(), normalized.clone());
            }
        }
        if normalized.is_empty() {
            if let Err(e) = image_store::delete_tags_for_item(&item_id) {
                log::error!("删除标签失败: {}", e);
            }
        } else if let Err(e) = image_store::sync_tags_for_item(&item_id, &normalized) {
            log::error!("同步标签失败: {}", e);
        }
        Ok(())
    }

    pub async fn set_tags_async(&self, item_id: String, tags: Vec<String>) -> Result<(), String> {
        let exists = {
            let history = lock_arc_mutex(&self.history);
            history.iter().any(|item| item.id == item_id)
        };
        if !exists {
            return Err("目标图片不存在".to_string());
        }
        let normalized = normalize_tags(tags);
        {
            let mut image_tags = lock_arc_mutex(&self.image_tags);
            if normalized.is_empty() {
                image_tags.remove(&item_id);
            } else {
                image_tags.insert(item_id.clone(), normalized.clone());
            }
        }
        if normalized.is_empty() {
            if let Err(e) = image_store::delete_tags_for_item_async(&item_id).await {
                log::error!("删除标签失败: {}", e);
            }
        } else if let Err(e) = image_store::sync_tags_for_item_async(&item_id, &normalized).await {
            log::error!("同步标签失败: {}", e);
        }
        Ok(())
    }

    pub fn add_rgba_image(&self, rgba: Vec<u8>, width: u32, height: u32) {
        self.add_image_with_source_blob(rgba, width, height, None);
    }

    pub fn add_rgba_image_with_source_blob(
        &self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        source_blob: Option<(Vec<u8>, String)>,
    ) {
        self.add_image_with_source_blob(rgba, width, height, source_blob);
    }

    fn add_image_with_source_blob(
        &self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        source_blob: Option<(Vec<u8>, String)>,
    ) {
        let signature = compute_signature(&rgba, width, height);
        let id = generate_item_id(&signature);
        let (preview_width, preview_height, preview_rgba_base64) =
            build_preview_from_rgba(&rgba, width, height);
        let blob_ext = source_blob.as_ref().map(|(_, ext)| ext.as_str()).unwrap_or(
            if BITMAP_STORAGE_USE_LOSSLESS_WEBP {
                "webp"
            } else {
                "png"
            },
        );
        let image_path = image_blob_path(&id, blob_ext).to_string_lossy().to_string();
        let item = ImageHistoryItem {
            id: id.clone(),
            width,
            height,
            preview_width,
            preview_height,
            preview_rgba_base64,
            image_path: image_path.clone(),
            rgba_bytes: Vec::new(),
            signature: signature.clone(),
        };
        let removed_ids_after_insert = {
            let mut history = lock_arc_mutex(&self.history);
            let mut categories = lock_arc_mutex(&self.categories);
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            let mut signature_index = lock_arc_mutex(&self.signature_index);
            let before_ids = history.iter().map(|item| item.id.clone()).collect::<HashSet<_>>();
            if self.signature_index_dirty.load(Ordering::SeqCst) || signature_index.len() != history.len()
            {
                *signature_index = build_signature_index(&history);
                self.signature_index_dirty.store(false, Ordering::SeqCst);
            }
            if let Some(&existing_index) = signature_index.get(&signature) {
                if existing_index < history.len() {
                    if existing_index != 0 {
                        let moved_signature = history[existing_index].signature.clone();
                        let moved = history.remove(existing_index);
                        history.insert(0, moved);
                        signature_index_move_to_front(
                            &mut signature_index,
                            &moved_signature,
                            existing_index,
                        );
                    }
                    let items_for_store = history.iter().map(compact_item_for_persist).collect::<Vec<_>>();
                    let item_ids = history.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
                    drop(signature_index);
                    drop(pinned_items);
                    drop(categories);
                    drop(history);
                    for (position, item) in items_for_store.iter().enumerate() {
                        if let Err(e) = image_store::upsert_item(item, position) {
                            log::error!("同步图片项失败: {}", e);
                        }
                    }
                    if let Err(e) = image_store::sync_item_positions(&item_ids) {
                        log::error!("同步图片位置失败: {}", e);
                    }
                    return;
                }
            }
            history.insert(0, item);
            let overflow_paths =
                shrink_image_history_with_group_protection(
                    &mut history,
                    self.max_items,
                    &mut categories,
                    self.grouped_items_protected_from_limit,
                );
            cleanup_image_blob_files_async(overflow_paths);
            normalize_pinned_items(&mut pinned_items, &history);
            apply_pin_order(&mut history, &pinned_items);
            let disk_removed_paths = shrink_image_history_by_disk_limit(
                &mut history,
                &mut categories,
                &mut pinned_items,
                self.image_disk_limit_bytes,
            );
            cleanup_image_blob_files_async(disk_removed_paths);
            *signature_index = build_signature_index(&history);
            self.signature_index_dirty.store(false, Ordering::SeqCst);
            self.enforce_full_res_cache_budget_lru(&mut history);
            self.prune_pending_images_by_history(&history);
            self.prune_full_res_cache_access_by_history(&history);
            let after_id_set = history.iter().map(|item| item.id.clone()).collect::<HashSet<_>>();
            before_ids
                .into_iter()
                .filter(|id| !after_id_set.contains(id))
                .collect::<Vec<_>>()
        };

        let rgba_arc = Arc::new(rgba);
        {
            let mut pending = lock_arc_mutex(&self.pending_images);
            pending.insert(
                id.clone(),
                PendingImageData {
                    rgba: rgba_arc.clone(),
                    width,
                    height,
                },
            );
        }
        let task = PersistTask {
            item_id: id,
            image_path,
            rgba: rgba_arc,
            width,
            height,
            encoded_bytes: source_blob.map(|(bytes, _)| bytes),
        };
        match self.persist_tx.try_send(task) {
            Ok(_) => {}
            Err(TrySendError::Full(task)) => {
                if let Err(send_err) = self.persist_tx.send(task) {
                    let failed_task = send_err.0;
                    log::error!("图片持久化队列阻塞后发送失败: {}", failed_task.item_id);
                    let mut pending = lock_arc_mutex(&self.pending_images);
                    pending.remove(&failed_task.item_id);
                }
            }
            Err(TrySendError::Disconnected(task)) => {
                log::error!("图片持久化队列不可用: {}", task.item_id);
                let mut pending = lock_arc_mutex(&self.pending_images);
                pending.remove(&task.item_id);
            }
        }
        let (items_snapshot, item_ids_snapshot, pinned_snapshot) = {
            let history = lock_arc_mutex(&self.history);
            let pinned = lock_arc_mutex(&self.pinned_items);
            let item_ids = history.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
            (
                history.iter().map(compact_item_for_persist).collect::<Vec<_>>(),
                item_ids,
                pinned.clone(),
            )
        };
        for (position, item) in items_snapshot.iter().enumerate() {
            if let Err(e) = image_store::upsert_item(item, position) {
                log::error!("同步图片项失败: {}", e);
            }
        }
        for removed_id in removed_ids_after_insert {
            if let Err(e) = image_store::delete_category(&removed_id) {
                log::error!("删除分类失败: {}", e);
            }
            if let Err(e) = image_store::delete_tags_for_item(&removed_id) {
                log::error!("删除标签失败: {}", e);
            }
            if let Err(e) = image_store::delete_item(&removed_id) {
                log::error!("删除图片项失败: {}", e);
            }
        }
        if let Err(e) = image_store::sync_pinned_order(&pinned_snapshot) {
            log::error!("同步置顶列表失败: {}", e);
        }
        if let Err(e) = image_store::sync_item_positions(&item_ids_snapshot) {
            log::error!("同步图片位置失败: {}", e);
        }
    }

    pub fn import_local_image_paths(&self, paths: Vec<String>) -> Result<usize, String> {
        let mut imported = 0usize;
        let mut last_error = String::new();
        for raw_path in paths {
            let path = raw_path.trim();
            if path.is_empty() {
                continue;
            }
            match read_local_image_for_import(path) {
                Ok((rgba, width, height, source_bytes, source_ext)) => {
                    self.add_image_with_source_blob(
                        rgba,
                        width,
                        height,
                        Some((source_bytes, source_ext)),
                    );
                    imported += 1;
                }
                Err(e) => {
                    last_error = format!("{}: {}", path, e);
                }
            }
        }
        if imported == 0 {
            if last_error.is_empty() {
                Err("未导入任何图片".to_string())
            } else {
                Err(last_error)
            }
        } else {
            Ok(imported)
        }
    }

    pub async fn import_local_image_paths_async(&self, paths: Vec<String>) -> Result<usize, String> {
        let manager = self.clone();
        tauri::async_runtime::spawn_blocking(move || manager.import_local_image_paths(paths))
            .await
            .map_err(|e| format!("导入图片任务执行失败: {}", e))?
    }

    pub fn remove_from_history(&self, index: usize) -> Result<(String, String, String), String> {
        let (removed_id, removed_path, removed_signature, item_ids_after_remove) = {
            let mut history = lock_arc_mutex(&self.history);
            let mut signature_index = lock_arc_mutex(&self.signature_index);
            if index >= history.len() {
                return Err("索引超出范围".to_string());
            }
            let removed = history.remove(index);
            signature_index_remove(
                &mut signature_index,
                &removed.signature,
                index,
            );
            let ids = history.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
            (removed.id, removed.image_path, removed.signature, ids)
        };

        let pinned_snapshot = {
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            pinned_items.retain(|id| id != &removed_id);
            pinned_items.clone()
        };
        {
            let mut categories = lock_arc_mutex(&self.categories);
            categories.remove(&removed_id);
            let mut image_tags = lock_arc_mutex(&self.image_tags);
            image_tags.remove(&removed_id);
            let mut pending_images = lock_arc_mutex(&self.pending_images);
            pending_images.remove(&removed_id);
        }

        cleanup_image_blob_files_async(vec![removed_path.clone()]);
        if let Err(e) = image_store::delete_item(&removed_id) {
            log::error!("删除图片项失败: {}", e);
        }
        if let Err(e) = image_store::delete_category(&removed_id) {
            log::error!("删除分类失败: {}", e);
        }
        if let Err(e) = image_store::delete_tags_for_item(&removed_id) {
            log::error!("删除标签失败: {}", e);
        }
        if let Err(e) = image_store::sync_pinned_order(&pinned_snapshot) {
            log::error!("同步置顶列表失败: {}", e);
        }
        if let Err(e) = image_store::sync_item_positions(&item_ids_after_remove) {
            log::error!("同步图片位置失败: {}", e);
        }
        Ok((removed_id, removed_path, removed_signature))
    }

    pub fn remove_from_history_by_id(
        &self,
        item_id: &str,
    ) -> Result<(String, String, String), String> {
        let index = {
            let history = lock_arc_mutex(&self.history);
            history
                .iter()
                .position(|item| item.id == item_id)
                .ok_or_else(|| "目标图片不存在".to_string())?
        };
        self.remove_from_history(index)
    }

    pub fn promote_to_top(&self, index: usize) -> Result<(), String> {
        let mut history = lock_arc_mutex(&self.history);
        let mut signature_index = lock_arc_mutex(&self.signature_index);
        if index >= history.len() {
            return Err("索引超出范围".to_string());
        }
        if index == 0 {
            return Ok(());
        }
        let moved_signature = history[index].signature.clone();
        let moved = history.remove(index);
        history.insert(0, moved);
        signature_index_move_to_front(&mut signature_index, &moved_signature, index);
        let item_ids = history.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
        drop(history);
        if let Err(e) = image_store::sync_item_positions(&item_ids) {
            log::error!("同步图片位置失败: {}", e);
        }
        Ok(())
    }

    pub fn promote_to_top_by_id(&self, item_id: &str) -> Result<(), String> {
        let index = {
            let history = lock_arc_mutex(&self.history);
            history
                .iter()
                .position(|item| item.id == item_id)
                .ok_or_else(|| "目标图片不存在".to_string())?
        };
        self.promote_to_top(index)
    }

    pub fn promote_to_top_in_memory_by_id(&self, item_id: &str) -> Result<(), String> {
        let mut history = lock_arc_mutex(&self.history);
        let mut signature_index = lock_arc_mutex(&self.signature_index);
        let index = history
            .iter()
            .position(|item| item.id == item_id)
            .ok_or_else(|| "目标图片不存在".to_string())?;
        if index == 0 {
            return Ok(());
        }
        let moved_signature = history[index].signature.clone();
        let moved = history.remove(index);
        history.insert(0, moved);
        signature_index_move_to_front(&mut signature_index, &moved_signature, index);
        Ok(())
    }

    pub fn set_pinned(&self, item_id: String, pinned: bool) -> Result<(), String> {
        let mut history = lock_arc_mutex(&self.history);
        if !history.iter().any(|item| item.id == item_id) {
            return Err("目标图片不存在".to_string());
        }
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        if pinned {
            if !pinned_items.iter().any(|id| id == &item_id) {
                pinned_items.insert(0, item_id.clone());
            }
        } else {
            pinned_items.retain(|id| id != &item_id);
        }
        normalize_pinned_items(&mut pinned_items, &history);
        apply_pin_order(&mut history, &pinned_items);
        self.signature_index_dirty.store(true, Ordering::SeqCst);
        let item_ids = history.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
        let pinned_snapshot = pinned_items.clone();
        drop(pinned_items);
        drop(history);
        if let Err(e) = image_store::sync_item_positions(&item_ids) {
            log::error!("同步图片位置失败: {}", e);
        }
        if let Err(e) = image_store::sync_pinned_order(&pinned_snapshot) {
            log::error!("同步置顶列表失败: {}", e);
        }
        Ok(())
    }

    pub async fn set_pinned_async(&self, item_id: String, pinned: bool) -> Result<(), String> {
        let (item_ids, pinned_snapshot) = {
            let mut history = lock_arc_mutex(&self.history);
            if !history.iter().any(|item| item.id == item_id) {
                return Err("目标图片不存在".to_string());
            }
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            if pinned {
                if !pinned_items.iter().any(|id| id == &item_id) {
                    pinned_items.insert(0, item_id.clone());
                }
            } else {
                pinned_items.retain(|id| id != &item_id);
            }
            normalize_pinned_items(&mut pinned_items, &history);
            apply_pin_order(&mut history, &pinned_items);
            self.signature_index_dirty.store(true, Ordering::SeqCst);
            (
                history.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
                pinned_items.clone(),
            )
        };
        if let Err(e) = image_store::sync_item_positions_async(&item_ids).await {
            log::error!("同步图片位置失败: {}", e);
        }
        if let Err(e) = image_store::sync_pinned_order_async(&pinned_snapshot).await {
            log::error!("同步置顶列表失败: {}", e);
        }
        Ok(())
    }

    pub fn clear_history_by_mode(&self, mode: &str) -> Result<usize, String> {
        let mut removed_paths: Vec<String> = Vec::new();
        let mut removed_ids: Vec<String> = Vec::new();
        let removed_count = {
            let mut history = lock_arc_mutex(&self.history);
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            let mut image_tags = lock_arc_mutex(&self.image_tags);
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            let mut pending_images = lock_arc_mutex(&self.pending_images);
            let before = history.len();
            match mode {
                "all" => {
                    removed_paths.extend(history.iter().map(|item| item.image_path.clone()));
                    for item in history.iter() {
                        pending_images.remove(&item.id);
                        removed_ids.push(item.id.clone());
                    }
                    history.clear();
                    categories.clear();
                    category_list.clear();
                    image_tags.clear();
                    pinned_items.clear();
                }
                "untagged_or_unclassified" | "untagged_unclassified_unpinned" => {
                    let tagged_ids: HashSet<String> = image_tags.keys().cloned().collect();
                    let classified_ids: HashSet<String> = categories.keys().cloned().collect();
                    let pinned_ids: HashSet<String> = pinned_items.iter().cloned().collect();
                    let mut kept = Vec::with_capacity(history.len());
                    for item in history.drain(..) {
                        let is_tagged = tagged_ids.contains(&item.id);
                        let is_classified = classified_ids.contains(&item.id);
                        let is_pinned = pinned_ids.contains(&item.id);
                        if is_tagged || is_classified || is_pinned {
                            kept.push(item);
                        } else {
                            pending_images.remove(&item.id);
                            removed_paths.push(item.image_path.clone());
                            removed_ids.push(item.id.clone());
                        }
                    }
                    *history = kept;
                    let valid_ids: HashSet<String> = history.iter().map(|item| item.id.clone()).collect();
                    categories.retain(|item_id, _| valid_ids.contains(item_id));
                    image_tags.retain(|item_id, _| valid_ids.contains(item_id));
                    normalize_pinned_items(&mut pinned_items, &history);
                    apply_pin_order(&mut history, &pinned_items);
                }
                _ => return Err("不支持的清理模式".to_string()),
            }
            before.saturating_sub(history.len())
        };
        cleanup_image_blob_files_async(removed_paths);
        self.signature_index_dirty.store(true, Ordering::SeqCst);
        let (items_snapshot, item_ids_snapshot, category_list_snapshot, tags_snapshot, pinned_snapshot) = {
            let history = lock_arc_mutex(&self.history);
            let category_list = lock_arc_mutex(&self.category_list);
            let tags = lock_arc_mutex(&self.image_tags);
            let pinned = lock_arc_mutex(&self.pinned_items);
            (
                history.iter().map(compact_item_for_persist).collect::<Vec<_>>(),
                history.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
                category_list.clone(),
                tags.clone(),
                pinned.clone(),
            )
        };
        for (position, item) in items_snapshot.iter().enumerate() {
            if let Err(e) = image_store::upsert_item(item, position) {
                log::error!("同步图片项失败: {}", e);
            }
        }
        for removed_id in removed_ids {
            if let Err(e) = image_store::delete_item(&removed_id) {
                log::error!("删除图片项失败: {}", e);
            }
            if let Err(e) = image_store::delete_category(&removed_id) {
                log::error!("删除分类失败: {}", e);
            }
            if let Err(e) = image_store::delete_tags_for_item(&removed_id) {
                log::error!("删除标签失败: {}", e);
            }
        }
        for (item_id, tags) in &tags_snapshot {
            if let Err(e) = image_store::sync_tags_for_item(item_id, tags) {
                log::error!("同步标签失败: {}", e);
            }
        }
        if let Err(e) = image_store::sync_pinned_order(&pinned_snapshot) {
            log::error!("同步置顶列表失败: {}", e);
        }
        if let Err(e) = image_store::sync_item_positions(&item_ids_snapshot) {
            log::error!("同步图片位置失败: {}", e);
        }
        if let Err(e) = image_store::sync_category_list_order(&category_list_snapshot) {
            log::error!("同步分类列表失败: {}", e);
        }
        Ok(removed_count)
    }

    pub async fn clear_history_by_mode_async(&self, mode: &str) -> Result<usize, String> {
        let mut removed_paths: Vec<String> = Vec::new();
        let mut removed_ids: Vec<String> = Vec::new();
        let removed_count = {
            let mut history = lock_arc_mutex(&self.history);
            let mut categories = lock_arc_mutex(&self.categories);
            let mut category_list = lock_arc_mutex(&self.category_list);
            let mut image_tags = lock_arc_mutex(&self.image_tags);
            let mut pinned_items = lock_arc_mutex(&self.pinned_items);
            let mut pending_images = lock_arc_mutex(&self.pending_images);
            let before = history.len();
            match mode {
                "all" => {
                    removed_paths.extend(history.iter().map(|item| item.image_path.clone()));
                    for item in history.iter() {
                        pending_images.remove(&item.id);
                        removed_ids.push(item.id.clone());
                    }
                    history.clear();
                    categories.clear();
                    category_list.clear();
                    image_tags.clear();
                    pinned_items.clear();
                }
                "untagged_or_unclassified" | "untagged_unclassified_unpinned" => {
                    let tagged_ids: HashSet<String> = image_tags.keys().cloned().collect();
                    let classified_ids: HashSet<String> = categories.keys().cloned().collect();
                    let pinned_ids: HashSet<String> = pinned_items.iter().cloned().collect();
                    let mut kept = Vec::with_capacity(history.len());
                    for item in history.drain(..) {
                        let is_tagged = tagged_ids.contains(&item.id);
                        let is_classified = classified_ids.contains(&item.id);
                        let is_pinned = pinned_ids.contains(&item.id);
                        if is_tagged || is_classified || is_pinned {
                            kept.push(item);
                        } else {
                            pending_images.remove(&item.id);
                            removed_paths.push(item.image_path.clone());
                            removed_ids.push(item.id.clone());
                        }
                    }
                    *history = kept;
                    let valid_ids: HashSet<String> = history.iter().map(|item| item.id.clone()).collect();
                    categories.retain(|item_id, _| valid_ids.contains(item_id));
                    image_tags.retain(|item_id, _| valid_ids.contains(item_id));
                    normalize_pinned_items(&mut pinned_items, &history);
                    apply_pin_order(&mut history, &pinned_items);
                }
                _ => return Err("不支持的清理模式".to_string()),
            }
            before.saturating_sub(history.len())
        };
        cleanup_image_blob_files_async(removed_paths);
        self.signature_index_dirty.store(true, Ordering::SeqCst);
        let (items_snapshot, item_ids_snapshot, category_list_snapshot, tags_snapshot, pinned_snapshot) = {
            let history = lock_arc_mutex(&self.history);
            let category_list = lock_arc_mutex(&self.category_list);
            let tags = lock_arc_mutex(&self.image_tags);
            let pinned = lock_arc_mutex(&self.pinned_items);
            (
                history.iter().map(compact_item_for_persist).collect::<Vec<_>>(),
                history.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
                category_list.clone(),
                tags.clone(),
                pinned.clone(),
            )
        };
        for (position, item) in items_snapshot.iter().enumerate() {
            if let Err(e) = image_store::upsert_item_async(item, position).await {
                log::error!("同步图片项失败: {}", e);
            }
        }
        for removed_id in removed_ids {
            if let Err(e) = image_store::delete_item_async(&removed_id).await {
                log::error!("删除图片项失败: {}", e);
            }
            if let Err(e) = image_store::delete_category_async(&removed_id).await {
                log::error!("删除分类失败: {}", e);
            }
            if let Err(e) = image_store::delete_tags_for_item_async(&removed_id).await {
                log::error!("删除标签失败: {}", e);
            }
        }
        for (item_id, tags) in &tags_snapshot {
            if let Err(e) = image_store::sync_tags_for_item_async(item_id, tags).await {
                log::error!("同步标签失败: {}", e);
            }
        }
        if let Err(e) = image_store::sync_pinned_order_async(&pinned_snapshot).await {
            log::error!("同步置顶列表失败: {}", e);
        }
        if let Err(e) = image_store::sync_item_positions_async(&item_ids_snapshot).await {
            log::error!("同步图片位置失败: {}", e);
        }
        if let Err(e) = image_store::sync_category_list_order_async(&category_list_snapshot).await {
            log::error!("同步分类列表失败: {}", e);
        }
        Ok(removed_count)
    }

    pub fn get_image_by_index(&self, index: usize) -> Result<Image<'static>, String> {
        let (bytes, width, height) = {
            let mut history = lock_arc_mutex(&self.history);
            let item = history
                .get_mut(index)
                .ok_or_else(|| format!("索引 {} 超出范围", index))?;
            if item.rgba_bytes.is_empty() {
                item.rgba_bytes = self.read_item_rgba(item)?;
            }
            self.mark_image_accessed(&item.id);
            let bytes = item.rgba_bytes.clone();
            let width = item.width;
            let height = item.height;
            self.enforce_full_res_cache_budget_lru(&mut history);
            (bytes, width, height)
        };
        Ok(Image::new_owned(bytes, width, height))
    }

    pub fn get_image_by_index_for_fill(&self, index: usize) -> Result<Image<'static>, String> {
        let (bytes, width, height) = {
            let mut history = lock_arc_mutex(&self.history);
            let item = history
                .get_mut(index)
                .ok_or_else(|| format!("索引 {} 超出范围", index))?;
            if item.rgba_bytes.is_empty() {
                item.rgba_bytes = self.read_item_rgba(item)?;
            }
            self.mark_image_accessed(&item.id);
            let bytes = item.rgba_bytes.clone();
            let width = item.width;
            let height = item.height;
            self.enforce_full_res_cache_budget_lru(&mut history);
            (bytes, width, height)
        };
        Ok(Image::new_owned(bytes, width, height))
    }

    pub fn get_image_by_id_for_fill(&self, item_id: &str) -> Result<Image<'static>, String> {
        let index = {
            let history = lock_arc_mutex(&self.history);
            history
                .iter()
                .position(|item| item.id == item_id)
                .ok_or_else(|| "目标图片不存在".to_string())?
        };
        self.get_image_by_index_for_fill(index)
    }

    pub fn warmup_image_by_index(&self, index: usize) -> Result<(), String> {
        let mut history = lock_arc_mutex(&self.history);
        let item = history
            .get_mut(index)
            .ok_or_else(|| format!("索引 {} 超出范围", index))?;
        if item.rgba_bytes.is_empty() {
            item.rgba_bytes = self.read_item_rgba(item)?;
        }
        self.mark_image_accessed(&item.id);
        self.enforce_full_res_cache_budget_lru(&mut history);
        Ok(())
    }

    pub fn warmup_image_by_id(&self, item_id: &str) -> Result<(), String> {
        let index = {
            let history = lock_arc_mutex(&self.history);
            history
                .iter()
                .position(|item| item.id == item_id)
                .ok_or_else(|| "目标图片不存在".to_string())?
        };
        self.warmup_image_by_index(index)
    }

    pub fn get_preview_window_payload_by_index(&self, index: usize) -> Result<String, String> {
        let mut history = lock_arc_mutex(&self.history);
        let item = history
            .get_mut(index)
            .ok_or_else(|| format!("索引 {} 超出范围", index))?;
        let payload = self.read_item_png_base64(item)?;
        self.enforce_full_res_cache_budget_lru(&mut history);
        Ok(payload)
    }

    pub fn get_preview_image_path_by_index(&self, index: usize) -> Result<String, String> {
        let history = lock_arc_mutex(&self.history);
        let item = history
            .get(index)
            .ok_or_else(|| format!("索引 {} 超出范围", index))?;
        Ok(item.image_path.clone())
    }

    pub fn get_preview_image_path_by_id(&self, item_id: &str) -> Result<String, String> {
        let history = lock_arc_mutex(&self.history);
        let item = history
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| "目标图片不存在".to_string())?;
        Ok(item.image_path.clone())
    }

    pub fn get_preview_lowres_payload_by_index(&self, index: usize) -> Result<Option<String>, String> {
        let history = lock_arc_mutex(&self.history);
        let item = history
            .get(index)
            .ok_or_else(|| format!("索引 {} 超出范围", index))?;
        if item.preview_rgba_base64.is_empty() || item.preview_width == 0 || item.preview_height == 0 {
            return Ok(None);
        }
        rgba_base64_to_png_base64(
            &item.preview_rgba_base64,
            item.preview_width,
            item.preview_height,
        )
        .map(Some)
    }

    pub fn read_clipboard_images_rgba(
        app_handle: &tauri::AppHandle,
    ) -> Result<Vec<ClipboardImagePayload>, String> {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        let retry_delays = [12u64, 18, 26, 36, 48, 62, 78, 96];
        for (attempt, delay_ms) in retry_delays.iter().enumerate() {
            let read_result = crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
                app_handle.clipboard().read_image()
            });
            match read_result {
                Ok(image) => {
                    let width = image.width();
                    let height = image.height();
                    let rgba = image.rgba().to_vec();
                    if !rgba.is_empty() && width > 0 && height > 0 {
                        return Ok(vec![(rgba, width, height, None)]);
                    }
                }
                Err(_) => {}
            }
            if attempt < retry_delays.len() - 1 {
                std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
            }
        }
        if let Ok(text) = crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
            app_handle.clipboard().read_text()
        }) {
            if let Some(payload) = parse_image_from_text_payload(&text) {
                return Ok(vec![payload]);
            }
            if let Some(path) = parse_local_image_path_from_text(&text) {
                if let Ok((rgba, width, height, source_bytes, source_ext)) =
                    read_local_image_for_import(&path)
                {
                    return Ok(vec![(rgba, width, height, Some((source_bytes, source_ext)))]);
                }
            }
            if text_contains_remote_image_url(&text) {
                return Err("检测到网页图片链接，但剪贴板中没有位图数据。请在网页中使用“复制图片”而不是“复制图片地址”".to_string());
            }
        }
        #[cfg(target_os = "windows")]
        {
            let images = crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
                read_images_from_windows_file_clipboard()
            });
            if !images.is_empty() {
                return Ok(images);
            }
        }
        Err("当前剪贴板不是位图格式，可能是文件对象/路径/网页元素".to_string())
    }

    pub fn write_clipboard_image(
        app_handle: &tauri::AppHandle,
        image: &Image<'_>,
    ) -> Result<(), String> {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        let mut last_error = String::new();
        let retry_delays = [8u64, 12, 18, 26, 36, 50, 70, 95, 125];
        for (attempt, delay_ms) in retry_delays.iter().enumerate() {
            match app_handle.clipboard().write_image(image) {
                Ok(_) => {
                    if is_fast_fill_verify_mode() {
                        return Ok(());
                    }
                    let verify_delays = [10u64, 18, 28, 42];
                    let mut verified = false;
                    for (verify_index, verify_delay) in verify_delays.iter().enumerate() {
                        if let Ok(read_back) = app_handle.clipboard().read_image() {
                            if read_back.width() > 0 && read_back.height() > 0 && !read_back.rgba().is_empty() {
                                verified = true;
                                break;
                            }
                        }
                        if verify_index < verify_delays.len() - 1 {
                            std::thread::sleep(std::time::Duration::from_millis(*verify_delay));
                        }
                    }
                    if verified {
                        return Ok(());
                    }
                    last_error = "写入后校验失败：剪贴板位图尚未稳定".to_string();
                }
                Err(e) => {
                    last_error = e.to_string();
                }
            }
            if attempt < retry_delays.len() - 1 {
                std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
            }
        }
        Err(format!("写入剪贴板图片失败: {}", last_error))
    }

    pub fn save_history_on_exit(&self) -> Result<(), String> {
        Ok(())
    }

    pub fn set_grouped_items_protected_from_limit(&mut self, enabled: bool) {
        self.grouped_items_protected_from_limit = enabled;
        let mut history = lock_arc_mutex(&self.history);
        let mut categories = lock_arc_mutex(&self.categories);
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        let removed_paths = shrink_image_history_with_group_protection(
            &mut history,
            self.max_items,
            &mut categories,
            self.grouped_items_protected_from_limit,
        );
        cleanup_image_blob_files_async(removed_paths);
        normalize_pinned_items(&mut pinned_items, &history);
        apply_pin_order(&mut history, &pinned_items);
        let disk_removed_paths = shrink_image_history_by_disk_limit(
            &mut history,
            &mut categories,
            &mut pinned_items,
            self.image_disk_limit_bytes,
        );
        cleanup_image_blob_files_async(disk_removed_paths);
        self.prune_pending_images_by_history(&history);
        self.prune_full_res_cache_access_by_history(&history);
        self.signature_index_dirty.store(true, Ordering::SeqCst);
    }

    pub fn set_disk_limit_mb(&mut self, image_disk_limit_mb: u64) {
        self.image_disk_limit_bytes = image_disk_limit_mb.saturating_mul(1024 * 1024);
        let mut history = lock_arc_mutex(&self.history);
        let mut categories = lock_arc_mutex(&self.categories);
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        let disk_removed_paths = shrink_image_history_by_disk_limit(
            &mut history,
            &mut categories,
            &mut pinned_items,
            self.image_disk_limit_bytes,
        );
        cleanup_image_blob_files_async(disk_removed_paths);
        self.prune_pending_images_by_history(&history);
        self.prune_full_res_cache_access_by_history(&history);
        self.signature_index_dirty.store(true, Ordering::SeqCst);
        let items_snapshot = history.iter().map(compact_item_for_persist).collect::<Vec<_>>();
        let item_ids_snapshot = history.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
        let pinned_snapshot = pinned_items.clone();
        drop(pinned_items);
        drop(categories);
        drop(history);
        for (position, item) in items_snapshot.iter().enumerate() {
            if let Err(e) = image_store::upsert_item(item, position) {
                log::error!("同步图片项失败: {}", e);
            }
        }
        if let Err(e) = image_store::sync_pinned_order(&pinned_snapshot) {
            log::error!("同步置顶列表失败: {}", e);
        }
        if let Err(e) = image_store::sync_item_positions(&item_ids_snapshot) {
            log::error!("同步图片位置失败: {}", e);
        }
    }

    pub fn get_storage_metrics(&self) -> ImageStorageMetrics {
        let history = lock_arc_mutex(&self.history);
        let pinned_items = lock_arc_mutex(&self.pinned_items);
        let memory_bytes = history
            .iter()
            .fold(0u64, |acc, item| acc.saturating_add(item.rgba_bytes.len() as u64));
        let disk_bytes = history
            .iter()
            .fold(0u64, |acc, item| acc.saturating_add(file_size_bytes(&item.image_path)));
        ImageStorageMetrics {
            memory_bytes,
            memory_budget_bytes: IMAGE_FULL_RES_MEMORY_BUDGET_BYTES as u64,
            disk_bytes,
            disk_limit_bytes: self.image_disk_limit_bytes,
            item_count: history.len() as u64,
            pinned_count: pinned_items.len() as u64,
        }
    }

    fn read_item_rgba(&self, item: &ImageHistoryItem) -> Result<Vec<u8>, String> {
        if let Ok(bytes) = read_image_blob(&item.image_path, item.width, item.height) {
            return Ok(bytes);
        }
        let pending = lock_arc_mutex(&self.pending_images);
        if let Some(data) = pending.get(&item.id) {
            if data.width == item.width && data.height == item.height {
                return Ok(data.rgba.as_ref().clone());
            }
        }
        read_image_blob(&item.image_path, item.width, item.height)
    }

    fn read_item_png_base64(&self, item: &ImageHistoryItem) -> Result<String, String> {
        if let Ok(payload) = read_image_png_base64(&item.image_path) {
            return Ok(payload);
        }
        let pending = lock_arc_mutex(&self.pending_images);
        if let Some(data) = pending.get(&item.id) {
            if data.width == item.width && data.height == item.height {
                return rgba_to_png_base64(data.rgba.as_ref(), item.width, item.height);
            }
        }
        read_image_png_base64(&item.image_path)
    }

    fn prune_pending_images_by_history(&self, history: &[ImageHistoryItem]) {
        let valid_ids: HashSet<String> = history.iter().map(|item| item.id.clone()).collect();
        let mut pending = lock_arc_mutex(&self.pending_images);
        pending.retain(|id, _| valid_ids.contains(id));
    }

    fn mark_image_accessed(&self, item_id: &str) {
        let mut lru = lock_arc_mutex(&self.full_res_lru);
        lru.put(item_id.to_string(), ());
    }

    fn prune_full_res_cache_access_by_history(&self, history: &[ImageHistoryItem]) {
        let valid_ids = history
            .iter()
            .filter(|item| !item.rgba_bytes.is_empty())
            .map(|item| item.id.clone())
            .collect::<HashSet<_>>();
        let mut lru = lock_arc_mutex(&self.full_res_lru);
        let mut kept = Vec::new();
        while let Some((id, _)) = lru.pop_lru() {
            if valid_ids.contains(&id) {
                kept.push(id);
            }
        }
        for id in kept {
            lru.put(id, ());
        }
    }

    fn enforce_full_res_cache_budget_lru(&self, history: &mut Vec<ImageHistoryItem>) {
        let mut total = history
            .iter()
            .fold(0usize, |acc, item| acc.saturating_add(item.rgba_bytes.len()));
        self.prune_full_res_cache_access_by_history(history);
        if total <= IMAGE_FULL_RES_MEMORY_BUDGET_BYTES {
            return;
        }

        let protected_count = IMAGE_FULL_RES_CACHE_KEEP_RECENT.min(history.len());
        let protected_ids = history
            .iter()
            .take(protected_count)
            .map(|item| item.id.clone())
            .collect::<HashSet<_>>();
        let index_by_id = history
            .iter()
            .enumerate()
            .map(|(idx, item)| (item.id.clone(), idx))
            .collect::<HashMap<_, _>>();
        let mut lru = lock_arc_mutex(&self.full_res_lru);
        let mut deferred_protected = Vec::new();

        while total > IMAGE_FULL_RES_MEMORY_BUDGET_BYTES {
            let Some((id, _)) = lru.pop_lru() else {
                break;
            };
            if protected_ids.contains(&id) {
                deferred_protected.push(id);
                continue;
            }
            let Some(&idx) = index_by_id.get(&id) else {
                continue;
            };
            let len = history[idx].rgba_bytes.len();
            if len == 0 {
                continue;
            }
            history[idx].rgba_bytes.clear();
            total = total.saturating_sub(len);
        }
        for id in deferred_protected {
            lru.put(id, ());
        }

        while total > IMAGE_FULL_RES_MEMORY_BUDGET_BYTES {
            let Some((id, _)) = lru.pop_lru() else {
                break;
            };
            let Some(&idx) = index_by_id.get(&id) else {
                continue;
            };
            let len = history[idx].rgba_bytes.len();
            if len == 0 {
                continue;
            }
            history[idx].rgba_bytes.clear();
            total = total.saturating_sub(len);
        }
    }

    fn compact_history_after_load(&self) {
        let mut history = lock_arc_mutex(&self.history);
        let mut categories = lock_arc_mutex(&self.categories);
        let mut pinned_items = lock_arc_mutex(&self.pinned_items);
        let removed_paths = shrink_image_history_with_group_protection(
            &mut history,
            self.max_items,
            &mut categories,
            self.grouped_items_protected_from_limit,
        );
        let disk_removed_paths = shrink_image_history_by_disk_limit(
            &mut history,
            &mut categories,
            &mut pinned_items,
            self.image_disk_limit_bytes,
        );
        self.enforce_full_res_cache_budget_lru(&mut history);
        cleanup_image_blob_files_async(removed_paths);
        cleanup_image_blob_files_async(disk_removed_paths);
        self.prune_pending_images_by_history(&history);
        self.prune_full_res_cache_access_by_history(&history);
        self.signature_index_dirty.store(true, Ordering::SeqCst);
        let items_snapshot = history.iter().map(compact_item_for_persist).collect::<Vec<_>>();
        let item_ids_snapshot = history.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
        let pinned_snapshot = pinned_items.clone();
        drop(pinned_items);
        drop(categories);
        drop(history);
        for (position, item) in items_snapshot.iter().enumerate() {
            if let Err(e) = image_store::upsert_item(item, position) {
                log::error!("同步图片项失败: {}", e);
            }
        }
        if let Err(e) = image_store::sync_pinned_order(&pinned_snapshot) {
            log::error!("同步置顶列表失败: {}", e);
        }
        if let Err(e) = image_store::sync_item_positions(&item_ids_snapshot) {
            log::error!("同步图片位置失败: {}", e);
        }
    }
}

#[derive(Clone, Serialize)]
pub struct ImageStorageMetrics {
    pub memory_bytes: u64,
    pub memory_budget_bytes: u64,
    pub disk_bytes: u64,
    pub disk_limit_bytes: u64,
    pub item_count: u64,
    pub pinned_count: u64,
}

impl Drop for ImageClipboardManager {
    fn drop(&mut self) {
        if let Err(e) = self.save_history_on_exit() {
            log::error!("程序退出时保存图片历史记录失败: {}", e);
        }
    }
}

fn compact_item_for_persist(item: &ImageHistoryItem) -> ImageHistoryItem {
    ImageHistoryItem {
        id: item.id.clone(),
        width: item.width,
        height: item.height,
        preview_width: item.preview_width,
        preview_height: item.preview_height,
        preview_rgba_base64: item.preview_rgba_base64.clone(),
        image_path: item.image_path.clone(),
        rgba_bytes: Vec::new(),
        signature: item.signature.clone(),
    }
}

fn build_signature_index(history: &[ImageHistoryItem]) -> HashMap<String, usize> {
    history
        .iter()
        .enumerate()
        .map(|(idx, item)| (item.signature.clone(), idx))
        .collect()
}

fn signature_index_move_to_front(
    signature_index: &mut HashMap<String, usize>,
    moved_signature: &str,
    from_index: usize,
) {
    if from_index == 0 {
        return;
    }
    for index in signature_index.values_mut() {
        if *index < from_index {
            *index += 1;
        }
    }
    if let Some(existing) = signature_index.get_mut(moved_signature) {
        *existing = 0;
    } else {
        signature_index.insert(moved_signature.to_string(), 0);
    }
}

fn signature_index_remove(
    signature_index: &mut HashMap<String, usize>,
    removed_signature: &str,
    removed_index: usize,
) {
    signature_index.remove(removed_signature);
    for index in signature_index.values_mut() {
        if *index > removed_index {
            *index -= 1;
        }
    }
}

fn shrink_image_history_with_group_protection(
    history: &mut Vec<ImageHistoryItem>,
    max_items: usize,
    categories: &mut HashMap<String, String>,
    grouped_items_protected_from_limit: bool,
) -> Vec<String> {
    if !grouped_items_protected_from_limit {
        if history.len() > max_items {
            let removed = history.split_off(max_items);
            return removed
                .into_iter()
                .map(|entry| {
                    categories.remove(&entry.id);
                    entry.image_path
                })
                .collect::<Vec<_>>();
        }
        return Vec::new();
    }
    let mut removed_paths = Vec::new();
    while history.len() > max_items {
        if let Some(pos) = history
            .iter()
            .rposition(|entry| !categories.contains_key(&entry.id))
        {
            let removed = history.remove(pos);
            categories.remove(&removed.id);
            removed_paths.push(removed.image_path);
        } else {
            break;
        }
    }
    removed_paths
}

fn shrink_image_history_by_disk_limit(
    history: &mut Vec<ImageHistoryItem>,
    categories: &mut HashMap<String, String>,
    pinned_items: &mut Vec<String>,
    disk_limit_bytes: u64,
) -> Vec<String> {
    if disk_limit_bytes == 0 {
        return Vec::new();
    }
    let mut removed_paths = Vec::new();
    let mut total = history
        .iter()
        .fold(0u64, |acc, item| acc.saturating_add(file_size_bytes(&item.image_path)));
    while total > disk_limit_bytes {
        let removable_pos = history
            .iter()
            .rposition(|item| !pinned_items.iter().any(|id| id == &item.id));
        let Some(pos) = removable_pos else {
            break;
        };
        let removed = history.remove(pos);
        let file_size = file_size_bytes(&removed.image_path);
        total = total.saturating_sub(file_size);
        categories.remove(&removed.id);
        pinned_items.retain(|id| id != &removed.id);
        removed_paths.push(removed.image_path);
    }
    removed_paths
}

fn file_size_bytes(path: &str) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

pub(crate) fn compute_signature(rgba: &[u8], width: u32, height: u32) -> String {
    let mut hasher = DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    rgba.len().hash(&mut hasher);
    if !rgba.is_empty() {
        let sample_points = 96usize;
        let step = (rgba.len() / sample_points).max(1);
        let mut idx = 0usize;
        while idx < rgba.len() {
            rgba[idx].hash(&mut hasher);
            idx += step;
        }
        rgba[rgba.len() - 1].hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

fn generate_item_id(signature: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("img_{}_{}", millis, signature)
}

fn get_image_blobs_dir() -> PathBuf {
    let mut dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    dir.pop();
    dir.push("image_history_blobs");
    dir
}

fn image_blob_path(item_id: &str, ext: &str) -> PathBuf {
    let mut path = get_image_blobs_dir();
    let suffix = ext.trim().trim_start_matches('.').to_lowercase();
    let final_ext = if suffix.is_empty() { "png".to_string() } else { suffix };
    path.push(format!("{}.{}", item_id, final_ext));
    path
}

fn start_image_persist_worker(
    persist_rx: std::sync::mpsc::Receiver<PersistTask>,
    pending_images: Arc<Mutex<HashMap<String, PendingImageData>>>,
) {
    std::thread::spawn(move || {
        while let Ok(task) = persist_rx.recv() {
            let persist_result = if let Some(encoded_bytes) = task.encoded_bytes.as_ref() {
                atomic_write_with_backup(Path::new(&task.image_path), encoded_bytes)
                    .map_err(|e| format!("写入图片数据失败: {}", e))
            } else {
                persist_generated_image_to_path(&task.image_path, &task.rgba, task.width, task.height)
            };
            if let Err(e) = persist_result {
                log::error!("异步落盘图片失败: {}", e);
            }
            let mut pending = lock_arc_mutex(&pending_images);
            pending.remove(&task.item_id);
        }
    });
}

fn persist_generated_image_to_path(path: &str, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    let dir = get_image_blobs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建图片存储目录失败: {}", e))?;
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "png".to_string());
    let encoded = if ext == "webp" {
        rgba_to_lossless_webp_bytes(rgba, width, height)?
    } else {
        rgba_to_png_bytes_for_storage(rgba, width, height)?
    };
    atomic_write_with_backup(Path::new(path), &encoded).map_err(|e| format!("写入图片数据失败: {}", e))
}

fn rgba_to_lossless_webp_bytes(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut encoded = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
        .write_image(rgba, width, height, image::ColorType::Rgba8.into())
        .map_err(|e| format!("编码存储WebP失败: {}", e))?;
    Ok(encoded)
}

fn rgba_to_png_bytes_for_storage(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut encoded,
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(rgba, width, height, image::ColorType::Rgba8.into())
    .map_err(|e| format!("编码存储PNG失败: {}", e))?;
    Ok(encoded)
}

fn rgba_to_png_bytes(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut encoded,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::NoFilter,
    )
    .write_image(rgba, width, height, image::ColorType::Rgba8.into())
    .map_err(|e| format!("编码PNG失败: {}", e))?;
    Ok(encoded)
}

fn rgba_to_png_base64(rgba: &[u8], width: u32, height: u32) -> Result<String, String> {
    rgba_to_png_bytes(rgba, width, height).map(|encoded| BASE64_STANDARD.encode(encoded))
}

fn read_image_blob(path: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取图片二进制失败: {}", e))?;
    if bytes.is_empty() {
        return Err("图片数据为空".to_string());
    }
    let decoded = image::load_from_memory(&bytes).map_err(|e| format!("解码图片失败: {}", e))?;
    let rgba = decoded.to_rgba8();
    if rgba.width() != width || rgba.height() != height {
        return Err(format!(
            "图片尺寸异常: 期望 {}x{} 实际 {}x{}",
            width,
            height,
            rgba.width(),
            rgba.height()
        ));
    }
    Ok(rgba.into_raw())
}

fn read_image_png_base64(path: &str) -> Result<String, String> {
    let stamp = read_file_change_stamp(path).unwrap_or(0);
    if let Some((cached_stamp, cached_payload)) =
        IMAGE_PNG_BASE64_CACHE.lock().get(path).cloned()
    {
        if cached_stamp == stamp {
            return Ok(cached_payload);
        }
    }
    let bytes = std::fs::read(path).map_err(|e| format!("读取图片二进制失败: {}", e))?;
    if bytes.is_empty() {
        return Err("图片数据为空".to_string());
    }
    let payload = BASE64_STANDARD.encode(&bytes);
    IMAGE_PNG_BASE64_CACHE
        .lock()
        .put(path.to_string(), (stamp, payload.clone()));
    Ok(payload)
}

fn read_file_change_stamp(path: &str) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    let len = metadata.len();
    let modified = metadata.modified().ok()?;
    let modified_ms = modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Some(len ^ modified_ms.rotate_left(13))
}

pub(crate) fn rgba_base64_to_png_base64(rgba_base64: &str, width: u32, height: u32) -> Result<String, String> {
    let rgba = BASE64_STANDARD
        .decode(rgba_base64)
        .map_err(|e| format!("解析预览图数据失败: {}", e))?;
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(format!("预览图像素长度异常: 期望 {} 实际 {}", expected, rgba.len()));
    }
    rgba_to_png_base64(&rgba, width, height)
}

fn cleanup_image_blob_files(paths: Vec<String>) {
    for path in paths {
        if path.trim().is_empty() {
            continue;
        }
        let _ = std::fs::remove_file(path);
    }
}

fn cleanup_image_blob_files_async(paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    std::thread::spawn(move || cleanup_image_blob_files(paths));
}

fn load_image_history_data() -> Result<ImageHistoryData, String> {
    image_store::init_image_store()?;
    let mut data = image_store::load_all_data()?;
    let mut changed = false;
    let original_item_ids = data.items.iter().map(|item| item.id.clone()).collect::<HashSet<_>>();
    let previous_len = data.items.len();
    data.items.retain(|item| {
        let image_path = item.image_path.trim();
        !image_path.is_empty()
            && looks_like_image_file_path(image_path)
            && Path::new(image_path).exists()
    });
    if data.items.len() != previous_len {
        changed = true;
    }
    let current_item_ids = data.items.iter().map(|item| item.id.clone()).collect::<HashSet<_>>();
    let removed_item_ids = original_item_ids
        .into_iter()
        .filter(|id| !current_item_ids.contains(id))
        .collect::<Vec<_>>();
    let orphan_paths = collect_orphan_blob_paths(&data);
    if !orphan_paths.is_empty() {
        cleanup_image_blob_files(orphan_paths);
        changed = true;
    }
    let valid_ids: HashSet<String> = data.items.iter().map(|item| item.id.clone()).collect();
    let old_categories_len = data.categories.len();
    data.categories.retain(|item_id, _| valid_ids.contains(item_id));
    if data.categories.len() != old_categories_len {
        changed = true;
    }
    let old_tags_len = data.image_tags.len();
    data.image_tags.retain(|item_id, tags| {
        if !valid_ids.contains(item_id) {
            return false;
        }
        let normalized = normalize_tags(tags.clone());
        *tags = normalized;
        !tags.is_empty()
    });
    if data.image_tags.len() != old_tags_len {
        changed = true;
    }
    let old_pinned_len = data.pinned_items.len();
    normalize_pinned_items(&mut data.pinned_items, &data.items);
    if data.pinned_items.len() != old_pinned_len {
        changed = true;
    }
    let mut items_for_reorder = data.items.clone();
    apply_pin_order(&mut items_for_reorder, &data.pinned_items);
    if items_for_reorder
        .iter()
        .zip(data.items.iter())
        .any(|(left, right)| left.id != right.id)
    {
        data.items = items_for_reorder;
        changed = true;
    }
    if changed {
        for (position, item) in data.items.iter().enumerate() {
            let _ = image_store::upsert_item(item, position);
        }
        for removed_id in removed_item_ids {
            let _ = image_store::delete_item(&removed_id);
            let _ = image_store::delete_category(&removed_id);
            let _ = image_store::delete_tags_for_item(&removed_id);
        }
        for (item_id, category) in &data.categories {
            let _ = image_store::upsert_category(item_id, category);
        }
        let _ = image_store::sync_category_list_order(&data.category_list);
        for (item_id, tags) in &data.image_tags {
            let _ = image_store::sync_tags_for_item(item_id, tags);
        }
        let _ = image_store::sync_pinned_order(&data.pinned_items);
    }
    Ok(data)
}

fn collect_orphan_blob_paths(data: &ImageHistoryData) -> Vec<String> {
    let blobs_dir = get_image_blobs_dir();
    if !blobs_dir.exists() {
        return Vec::new();
    }

    let referenced: HashSet<String> = data
        .items
        .iter()
        .map(|item| item.image_path.clone())
        .collect();

    let mut orphans = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&blobs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let normalized = path.to_string_lossy().to_string();
            if !referenced.contains(&normalized) {
                orphans.push(normalized);
            }
        }
    }
    orphans
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for raw in tags {
        let tag = raw.trim().to_string();
        if tag.is_empty() {
            continue;
        }
        if seen.insert(tag.clone()) {
            normalized.push(tag);
        }
    }
    normalized
}

fn normalize_pinned_items(pinned_items: &mut Vec<String>, history: &[ImageHistoryItem]) {
    let existing: HashSet<String> = history.iter().map(|item| item.id.clone()).collect();
    let mut seen = HashSet::new();
    pinned_items.retain(|id| existing.contains(id) && seen.insert(id.clone()));
}

fn apply_pin_order(history: &mut Vec<ImageHistoryItem>, pinned_items: &[String]) {
    if pinned_items.is_empty() || history.is_empty() {
        return;
    }
    let mut ordered = Vec::with_capacity(history.len());
    for pinned in pinned_items {
        if let Some(pos) = history.iter().position(|item| item.id == *pinned) {
            ordered.push(history.remove(pos));
        }
    }
    ordered.append(history);
    *history = ordered;
}

fn parse_local_image_path_from_text(text: &str) -> Option<String> {
    for candidate in collect_text_path_candidates(text) {
        if let Some(path) = normalize_local_image_path_candidate(&candidate) {
            return Some(path);
        }
    }
    None
}

fn parse_image_from_text_payload(text: &str) -> Option<(Vec<u8>, u32, u32, Option<(Vec<u8>, String)>)> {
    if let Some(payload) = parse_data_url_image(text) {
        return Some(payload);
    }
    if let Some(src) = extract_img_src_from_html(text) {
        if let Some(payload) = parse_data_url_image(&src) {
            return Some(payload);
        }
        if let Some(path) = parse_local_image_path_from_text(&src) {
            if let Ok((rgba, width, height, source_bytes, source_ext)) = read_local_image_for_import(&path) {
                return Some((rgba, width, height, Some((source_bytes, source_ext))));
            }
        }
    }
    None
}

fn parse_data_url_image(text: &str) -> Option<(Vec<u8>, u32, u32, Option<(Vec<u8>, String)>)> {
    let trimmed = text.trim();
    let data_url = if trimmed.starts_with("data:image/") {
        trimmed
    } else if let Some(start) = trimmed.find("data:image/") {
        let candidate = &trimmed[start..];
        let end = candidate
            .find(|c: char| c == '"' || c == '\'' || c == ')' || c.is_whitespace())
            .unwrap_or(candidate.len());
        &candidate[..end]
    } else {
        return None;
    };

    let comma_pos = data_url.find(',')?;
    let (meta, data) = data_url.split_at(comma_pos);
    let payload = data.get(1..)?;
    let bytes = if meta.contains(";base64") {
        BASE64_STANDARD.decode(payload).ok()?
    } else {
        return None;
    };
    let source_ext = parse_image_ext_from_data_url_meta(meta).unwrap_or_else(|| "png".to_string());
    let dyn_img = ::image::load_from_memory(&bytes).ok()?;
    let rgba8 = dyn_img.to_rgba8();
    let (width, height) = rgba8.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some((rgba8.into_raw(), width, height, Some((bytes, source_ext))))
}

fn parse_image_ext_from_data_url_meta(meta: &str) -> Option<String> {
    let normalized = meta.trim().to_lowercase();
    let prefix = "data:image/";
    if !normalized.starts_with(prefix) {
        return None;
    }
    let body = &normalized[prefix.len()..];
    let ext = body.split(';').next().unwrap_or("png").trim();
    if ext.is_empty() {
        return None;
    }
    Some(match ext {
        "jpeg" => "jpg".to_string(),
        "svg+xml" => "png".to_string(),
        other => other.to_string(),
    })
}

fn extract_img_src_from_html(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let img_pos = lower.find("<img")?;
    let src_pos_rel = lower[img_pos..].find("src=")?;
    let src_start = img_pos + src_pos_rel + 4;
    let bytes = text.as_bytes();
    let quote = *bytes.get(src_start)? as char;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value_start = src_start + 1;
    let value_rel_end = text.get(value_start..)?.find(quote)?;
    let value_end = value_start + value_rel_end;
    Some(text[value_start..value_end].to_string())
}

fn text_contains_remote_image_url(text: &str) -> bool {
    let lower = text.to_lowercase();
    (lower.contains("http://") || lower.contains("https://"))
        && (lower.contains(".png")
            || lower.contains(".jpg")
            || lower.contains(".jpeg")
            || lower.contains(".webp")
            || lower.contains(".gif")
            || lower.contains("<img"))
}

fn looks_like_image_file_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".bmp")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
}

fn read_local_image_for_import(path: &str) -> Result<(Vec<u8>, u32, u32, Vec<u8>, String), String> {
    let source_bytes = std::fs::read(path).map_err(|e| format!("读取本地图片文件失败: {}", e))?;
    let dyn_img = image::load_from_memory(&source_bytes).map_err(|e| format!("读取本地图片失败: {}", e))?;
    let rgba8 = dyn_img.to_rgba8();
    let (width, height) = rgba8.dimensions();
    let rgba = rgba8.into_raw();
    if rgba.is_empty() || width == 0 || height == 0 {
        return Err("本地图片为空".to_string());
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "png".to_string());
    Ok((rgba, width, height, source_bytes, ext))
}

fn build_preview_from_rgba(rgba: &[u8], width: u32, height: u32) -> (u32, u32, String) {
    if width == 0 || height == 0 {
        return (0, 0, String::new());
    }
    let Some(image) = RgbaImage::from_raw(width, height, rgba.to_vec()) else {
        return (0, 0, String::new());
    };
    let mut dynamic = DynamicImage::ImageRgba8(image);
    if width > IMAGE_PREVIEW_MAX_EDGE || height > IMAGE_PREVIEW_MAX_EDGE {
        dynamic = dynamic.resize(IMAGE_PREVIEW_MAX_EDGE, IMAGE_PREVIEW_MAX_EDGE, FilterType::Triangle);
    }
    let preview = dynamic.to_rgba8();
    let preview_width = preview.width();
    let preview_height = preview.height();
    if preview_width == 0 || preview_height == 0 {
        return (0, 0, String::new());
    }
    let payload = BASE64_STANDARD.encode(preview.into_raw());
    (preview_width, preview_height, payload)
}

fn collect_text_path_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let whole = text.trim();
    if !whole.is_empty() {
        candidates.push(whole.to_string());
    }
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            candidates.push(trimmed.to_string());
        }
    }
    candidates
}

fn normalize_local_image_path_candidate(raw: &str) -> Option<String> {
    let mut candidate = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('<')
        .trim_matches('>')
        .to_string();
    if candidate.is_empty() || candidate.starts_with('#') {
        return None;
    }
    if candidate.to_ascii_lowercase().starts_with("file://") {
        candidate = file_uri_to_local_path(&candidate)?;
    }
    candidate = percent_decode_path(&candidate);
    if !looks_like_image_file_path(&candidate) {
        return None;
    }
    let path = Path::new(&candidate);
    if !path.exists() || !path.is_file() {
        return None;
    }
    Some(candidate)
}

fn file_uri_to_local_path(uri: &str) -> Option<String> {
    if !uri.to_ascii_lowercase().starts_with("file://") {
        return None;
    }
    let mut rest = uri.get(7..)?.to_string();
    if rest.is_empty() {
        return None;
    }
    if rest.to_ascii_lowercase().starts_with("localhost/") {
        rest = rest.get("localhost".len()..)?.to_string();
    }
    #[cfg(target_os = "windows")]
    {
        if rest.starts_with('/') && rest.len() >= 3 {
            let bytes = rest.as_bytes();
            if bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
                rest = rest[1..].to_string();
            }
        }
        return Some(rest.replace('/', "\\"));
    }
    #[cfg(not(target_os = "windows"))]
    {
        if rest.starts_with('/') {
            Some(rest)
        } else {
            Some(format!("/{}", rest))
        }
    }
}

fn percent_decode_path(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h * 16 + l) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn read_images_from_windows_file_clipboard() -> Vec<ClipboardImagePayload> {
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP as UINT) == 0 {
            return Vec::new();
        }
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Vec::new();
        }
        let handle = GetClipboardData(CF_HDROP as UINT);
        if handle.is_null() {
            CloseClipboard();
            return Vec::new();
        }
        let count = DragQueryFileW(handle as *mut _, 0xFFFFFFFF, std::ptr::null_mut(), 0);
        if count == 0 {
            CloseClipboard();
            return Vec::new();
        }
        let mut result: Vec<ClipboardImagePayload> = Vec::new();
        let mut index = 0;
        while index < count {
            let len = DragQueryFileW(handle as *mut _, index, std::ptr::null_mut(), 0);
            if len > 0 {
                let mut buf = vec![0u16; (len + 1) as usize];
                let written = DragQueryFileW(handle as *mut _, index, buf.as_mut_ptr(), len + 1);
                if written > 0 {
                    let path = String::from_utf16_lossy(&buf[..written as usize]);
                    if looks_like_image_file_path(&path) {
                        if let Ok((rgba, width, height, source_bytes, source_ext)) =
                            read_local_image_for_import(&path)
                        {
                            result.push((rgba, width, height, Some((source_bytes, source_ext))));
                        }
                    }
                }
            }
            index += 1;
        }
        CloseClipboard();
        result
    }
}
