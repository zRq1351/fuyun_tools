use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::image::Image;

const MAX_PREVIEW_WIDTH: u32 = 160;
const MAX_PREVIEW_HEIGHT: u32 = 90;
const MAX_UI_HISTORY_ITEMS: usize = 30;

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
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ImageHistoryData {
    pub items: Vec<ImageHistoryItem>,
    #[serde(default)]
    pub categories: HashMap<String, String>,
    #[serde(default)]
    pub category_list: Vec<String>,
}

pub struct ImageClipboardManager {
    history: Arc<Mutex<Vec<ImageHistoryItem>>>,
    categories: Arc<Mutex<HashMap<String, String>>>,
    category_list: Arc<Mutex<Vec<String>>>,
    save_pending: Arc<AtomicBool>,
    save_running: Arc<AtomicBool>,
    max_items: usize,
    grouped_items_protected_from_limit: bool,
}

impl ImageClipboardManager {
    pub fn new(max_items: usize, grouped_items_protected_from_limit: bool) -> Self {
        let history_data = load_image_history_data().unwrap_or_else(|e| {
            log::error!("加载图片历史记录失败: {}，使用空历史记录", e);
            ImageHistoryData::default()
        });

        Self {
            history: Arc::new(Mutex::new(history_data.items)),
            categories: Arc::new(Mutex::new(history_data.categories)),
            category_list: Arc::new(Mutex::new(history_data.category_list)),
            save_pending: Arc::new(AtomicBool::new(false)),
            save_running: Arc::new(AtomicBool::new(false)),
            max_items,
            grouped_items_protected_from_limit,
        }
    }

    pub fn get_history(&self) -> Vec<ImageHistoryItem> {
        self.history.lock().unwrap().clone()
    }

    pub fn get_history_preview(&self) -> Vec<ImageHistoryPreviewItem> {
        let mut changed = false;
        let result = {
            let mut history = self.history.lock().unwrap();
            history
                .iter_mut()
                .take(MAX_UI_HISTORY_ITEMS)
                .map(|item| {
                    let preview_invalid = item.preview_width == 0
                        || item.preview_height == 0
                        || item.preview_rgba_base64.is_empty()
                        || item.preview_width > MAX_PREVIEW_WIDTH
                        || item.preview_height > MAX_PREVIEW_HEIGHT;
                    if preview_invalid {
                        if item.rgba_bytes.is_empty() {
                            if let Ok(bytes) = read_image_blob(&item.image_path, item.width, item.height) {
                                item.rgba_bytes = bytes;
                            }
                        }
                        if !item.rgba_bytes.is_empty() {
                            let (preview_rgba, preview_width, preview_height) =
                                generate_preview_rgba(&item.rgba_bytes, item.width, item.height);
                            item.preview_width = preview_width;
                            item.preview_height = preview_height;
                            item.preview_rgba_base64 = BASE64_STANDARD.encode(&preview_rgba);
                            changed = true;
                        }
                    }
                    ImageHistoryPreviewItem {
                        id: item.id.clone(),
                        width: item.width,
                        height: item.height,
                        preview_width: item.preview_width,
                        preview_height: item.preview_height,
                        preview_rgba_base64: item.preview_rgba_base64.clone(),
                    }
                })
                .collect::<Vec<_>>()
        };
        if changed {
            self.schedule_async_save();
        }
        result
    }

    pub fn get_categories(&self) -> HashMap<String, String> {
        self.categories.lock().unwrap().clone()
    }

    pub fn get_category_list(&self) -> Vec<String> {
        self.category_list.lock().unwrap().clone()
    }

    pub fn set_max_items(&mut self, max_items: usize) {
        self.max_items = max_items;
        let mut history = self.history.lock().unwrap();
        if history.len() > max_items {
            let mut categories = self.categories.lock().unwrap();
            let overflow_paths =
                shrink_image_history_with_group_protection(
                    &mut history,
                    max_items,
                    &mut categories,
                    self.grouped_items_protected_from_limit,
                );
            cleanup_image_blob_files(overflow_paths);
            drop(history);
            self.schedule_async_save();
        }
    }

    pub fn add_category(&self, category: String) -> Result<(), String> {
        {
            let mut list = self.category_list.lock().unwrap();
            let normalized = category.trim().to_string();
            if !normalized.is_empty()
                && normalized != "未分类"
                && normalized != "全部"
                && !list.contains(&normalized)
            {
                list.push(normalized);
            }
        }
        self.schedule_async_save();
        Ok(())
    }

    pub fn remove_category(&self, category: String) -> Result<(), String> {
        {
            let mut categories = self.categories.lock().unwrap();
            let mut category_list = self.category_list.lock().unwrap();
            category_list.retain(|c| c != &category);
            categories.retain(|_, v| v != &category);
        }
        self.schedule_async_save();
        Ok(())
    }

    pub fn set_category(&self, item_id: String, category: String) -> Result<(), String> {
        {
            let mut categories = self.categories.lock().unwrap();
            let mut category_list = self.category_list.lock().unwrap();
            let normalized = category.trim().to_string();
            if normalized.is_empty() || normalized == "未分类" || normalized == "全部" {
                categories.remove(&item_id);
            } else {
                categories.insert(item_id, normalized.clone());
                if !category_list.contains(&normalized) {
                    category_list.push(normalized);
                }
            }
        }
        self.schedule_async_save();
        Ok(())
    }

    pub fn add_rgba_image(&self, rgba: Vec<u8>, width: u32, height: u32) {
        let signature = compute_signature(&rgba, width, height);
        let id = generate_item_id(&signature);
        let (preview_rgba, preview_width, preview_height) = generate_preview_rgba(&rgba, width, height);
        let image_path = match persist_image_blob(&id, &rgba) {
            Ok(path) => path,
            Err(e) => {
                log::error!("保存图片二进制失败: {}", e);
                return;
            }
        };
        let item = ImageHistoryItem {
            id,
            width,
            height,
            preview_width,
            preview_height,
            preview_rgba_base64: BASE64_STANDARD.encode(&preview_rgba),
            image_path,
            rgba_bytes: rgba,
            signature: signature.clone(),
        };

        {
            let mut history = self.history.lock().unwrap();
            let mut categories = self.categories.lock().unwrap();
            let mut removed_paths = Vec::new();
            let mut removed_ids = Vec::new();
            history.retain(|entry| {
                if entry.signature == signature {
                    removed_paths.push(entry.image_path.clone());
                    removed_ids.push(entry.id.clone());
                    false
                } else {
                    true
                }
            });
            for removed_id in removed_ids {
                categories.remove(&removed_id);
            }
            history.insert(0, item);
            let overflow_paths =
                shrink_image_history_with_group_protection(
                    &mut history,
                    self.max_items,
                    &mut categories,
                    self.grouped_items_protected_from_limit,
                );
            removed_paths.extend(overflow_paths);
            cleanup_image_blob_files(removed_paths);
        }

        self.schedule_async_save();
    }

    pub fn remove_from_history(&self, index: usize) -> Result<(String, String, String), String> {
        let (removed_id, removed_path, removed_signature) = {
            let mut history = self.history.lock().unwrap();
            if index >= history.len() {
                return Err("索引超出范围".to_string());
            }
            let removed = history.remove(index);
            (removed.id, removed.image_path, removed.signature)
        };

        {
            let mut categories = self.categories.lock().unwrap();
            categories.remove(&removed_id);
        }

        cleanup_image_blob_files(vec![removed_path.clone()]);
        self.schedule_async_save();
        Ok((removed_id, removed_path, removed_signature))
    }

    pub fn promote_to_top(&self, index: usize) -> Result<(), String> {
        let mut history = self.history.lock().unwrap();
        if index >= history.len() {
            return Err("索引超出范围".to_string());
        }
        if index == 0 {
            return Ok(());
        }
        let moved = history.remove(index);
        history.insert(0, moved);
        self.schedule_async_save();
        Ok(())
    }

    pub fn get_image_by_index(&self, index: usize) -> Result<Image<'static>, String> {
        let (bytes, width, height) = {
            let mut history = self.history.lock().unwrap();
            let item = history
                .get_mut(index)
                .ok_or_else(|| format!("索引 {} 超出范围", index))?;
            if item.rgba_bytes.is_empty() {
                item.rgba_bytes =
                    read_image_blob(&item.image_path, item.width, item.height)?;
            }
            (item.rgba_bytes.clone(), item.width, item.height)
        };
        Ok(Image::new_owned(bytes, width, height))
    }

    pub fn warmup_image_by_index(&self, index: usize) -> Result<(), String> {
        let mut history = self.history.lock().unwrap();
        let item = history
            .get_mut(index)
            .ok_or_else(|| format!("索引 {} 超出范围", index))?;
        if item.rgba_bytes.is_empty() {
            item.rgba_bytes = read_image_blob(&item.image_path, item.width, item.height)?;
        }
        Ok(())
    }

    pub fn get_preview_window_payload_by_index(&self, index: usize) -> Result<(String, u32, u32), String> {
        let mut history = self.history.lock().unwrap();
        let item = history
            .get_mut(index)
            .ok_or_else(|| format!("索引 {} 超出范围", index))?;
        if item.rgba_bytes.is_empty() {
            item.rgba_bytes = read_image_blob(&item.image_path, item.width, item.height)?;
        }
        Ok((BASE64_STANDARD.encode(&item.rgba_bytes), item.width, item.height))
    }

    pub fn read_clipboard_images_rgba(
        app_handle: &tauri::AppHandle,
    ) -> Result<Vec<(Vec<u8>, u32, u32)>, String> {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        let retry_delays = [12u64, 18, 26, 36, 48, 62, 78, 96];
        for (attempt, delay_ms) in retry_delays.iter().enumerate() {
            match app_handle.clipboard().read_image() {
                Ok(image) => {
                    let width = image.width();
                    let height = image.height();
                    let rgba = image.rgba().to_vec();
                    if !rgba.is_empty() && width > 0 && height > 0 {
                        return Ok(vec![(rgba, width, height)]);
                    }
                }
                Err(_) => {}
            }
            if attempt < retry_delays.len() - 1 {
                std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
            }
        }
        if let Ok(text) = app_handle.clipboard().read_text() {
            if let Some((rgba, width, height)) = parse_image_from_text_payload(&text) {
                return Ok(vec![(rgba, width, height)]);
            }
            if let Some(path) = parse_local_image_path_from_text(&text) {
                if let Ok((rgba, width, height)) = read_local_image_rgba(&path) {
                    return Ok(vec![(rgba, width, height)]);
                }
            }
            if text_contains_remote_image_url(&text) {
                return Err("检测到网页图片链接，但剪贴板中没有位图数据。请在网页中使用“复制图片”而不是“复制图片地址”".to_string());
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
        let data = self.snapshot();
        save_image_history_data_with_retry(&data, 3)
    }

    pub fn set_grouped_items_protected_from_limit(&mut self, enabled: bool) {
        self.grouped_items_protected_from_limit = enabled;
        let mut history = self.history.lock().unwrap();
        let mut categories = self.categories.lock().unwrap();
        let removed_paths = shrink_image_history_with_group_protection(
            &mut history,
            self.max_items,
            &mut categories,
            self.grouped_items_protected_from_limit,
        );
        cleanup_image_blob_files(removed_paths);
    }

    fn snapshot(&self) -> ImageHistoryData {
        let history = {
            let history = self.history.lock().unwrap();
            history.iter().map(compact_item_for_persist).collect::<Vec<_>>()
        };
        let categories = self.categories.lock().unwrap().clone();
        let category_list = self.category_list.lock().unwrap().clone();
        ImageHistoryData {
            items: history,
            categories,
            category_list,
        }
    }

    fn schedule_async_save(&self) {
        self.save_pending.store(true, Ordering::SeqCst);
        if self
            .save_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let history_arc = self.history.clone();
        let categories_arc = self.categories.clone();
        let category_list_arc = self.category_list.clone();
        let save_pending = self.save_pending.clone();
        let save_running = self.save_running.clone();

        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(280));
            if !save_pending.swap(false, Ordering::SeqCst) {
                save_running.store(false, Ordering::SeqCst);
                if save_pending.load(Ordering::SeqCst)
                    && save_running
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    continue;
                }
                break;
            }

            let data = snapshot_from_arcs(&history_arc, &categories_arc, &category_list_arc);
            if let Err(e) = save_image_history_data_with_retry(&data, 3) {
                log::error!("异步保存图片历史失败: {}", e);
            }
        });
    }
}

impl Drop for ImageClipboardManager {
    fn drop(&mut self) {
        if let Err(e) = self.save_history_on_exit() {
            log::error!("程序退出时保存图片历史记录失败: {}", e);
        }
    }
}

fn snapshot_from_arcs(
    history_arc: &Arc<Mutex<Vec<ImageHistoryItem>>>,
    categories_arc: &Arc<Mutex<HashMap<String, String>>>,
    category_list_arc: &Arc<Mutex<Vec<String>>>,
) -> ImageHistoryData {
    let history = {
        let history = history_arc.lock().unwrap();
        history.iter().map(compact_item_for_persist).collect::<Vec<_>>()
    };
    let categories = categories_arc.lock().unwrap().clone();
    let category_list = category_list_arc.lock().unwrap().clone();
    ImageHistoryData {
        items: history,
        categories,
        category_list,
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

pub(crate) fn compute_signature(rgba: &[u8], width: u32, height: u32) -> String {
    let mut hasher = DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    rgba.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn generate_item_id(signature: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("img_{}_{}", millis, signature)
}

fn get_image_history_file_path() -> PathBuf {
    let mut history_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    history_dir.pop();
    history_dir.push("image_history.json");
    history_dir
}

fn get_image_blobs_dir() -> PathBuf {
    let mut dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    dir.pop();
    dir.push("image_history_blobs");
    dir
}

fn image_blob_path(item_id: &str) -> PathBuf {
    let mut path = get_image_blobs_dir();
    path.push(format!("{}.rgba", item_id));
    path
}

fn persist_image_blob(item_id: &str, rgba: &[u8]) -> Result<String, String> {
    let dir = get_image_blobs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建图片存储目录失败: {}", e))?;
    let path = image_blob_path(item_id);
    std::fs::write(&path, rgba).map_err(|e| format!("写入图片数据失败: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

fn read_image_blob(path: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取图片二进制失败: {}", e))?;
    if bytes.is_empty() {
        return Err("图片数据为空".to_string());
    }
    let expected = width as usize * height as usize * 4;
    if bytes.len() != expected {
        return Err(format!("图片数据长度异常: 期望 {} 实际 {}", expected, bytes.len()));
    }
    Ok(bytes)
}

fn cleanup_image_blob_files(paths: Vec<String>) {
    for path in paths {
        if path.trim().is_empty() {
            continue;
        }
        let _ = std::fs::remove_file(path);
    }
}

fn load_image_history_data() -> Result<ImageHistoryData, String> {
    let history_path = get_image_history_file_path();
    if !history_path.exists() {
        return Ok(ImageHistoryData::default());
    }
    let contents =
        std::fs::read_to_string(&history_path).map_err(|e| format!("读取图片历史文件失败: {}", e))?;
    let mut data = serde_json::from_str::<ImageHistoryData>(&contents)
        .map_err(|e| format!("解析图片历史文件失败: {}", e))?;
    let mut changed = false;
    for item in &mut data.items {
        if item.image_path.trim().is_empty() {
            changed = true;
            continue;
        }
        let preview_invalid = item.preview_width == 0
            || item.preview_height == 0
            || item.preview_rgba_base64.is_empty()
            || item.preview_width > MAX_PREVIEW_WIDTH
            || item.preview_height > MAX_PREVIEW_HEIGHT;
        if preview_invalid {
            if let Ok(full_rgba) = read_image_blob(&item.image_path, item.width, item.height) {
                let (preview_rgba, preview_width, preview_height) =
                    generate_preview_rgba(&full_rgba, item.width, item.height);
                item.preview_width = preview_width;
                item.preview_height = preview_height;
                item.preview_rgba_base64 = BASE64_STANDARD.encode(&preview_rgba);
                changed = true;
            }
        }
    }
    data.items.retain(|item| {
        !item.image_path.trim().is_empty() && Path::new(&item.image_path).exists()
    });
    if changed {
        let _ = save_image_history_data_with_retry(&data, 2);
    }
    Ok(data)
}

fn save_image_history_data_with_retry(data: &ImageHistoryData, max_retries: u32) -> Result<(), String> {
    let history_path = get_image_history_file_path();
    let json = serde_json::to_string_pretty(data).map_err(|e| format!("序列化图片历史失败: {}", e))?;
    for i in 0..max_retries {
        match std::fs::write(&history_path, &json) {
            Ok(_) => return Ok(()),
            Err(e) => {
                if i == max_retries - 1 {
                    return Err(format!("写入图片历史文件失败: {}", e));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    Ok(())
}

fn generate_preview_rgba(rgba: &[u8], width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    if width == 0 || height == 0 || rgba.is_empty() {
        return (Vec::new(), 0, 0);
    }

    let scale_w = width as f32 / MAX_PREVIEW_WIDTH as f32;
    let scale_h = height as f32 / MAX_PREVIEW_HEIGHT as f32;
    let scale = scale_w.max(scale_h).max(1.0);

    let target_width = ((width as f32 / scale).round() as u32).max(1);
    let target_height = ((height as f32 / scale).round() as u32).max(1);

    if target_width == width && target_height == height {
        return (rgba.to_vec(), width, height);
    }

    let mut out = vec![0u8; (target_width * target_height * 4) as usize];
    for ty in 0..target_height {
        for tx in 0..target_width {
            let sx = ((tx as f32 * width as f32) / target_width as f32).floor() as u32;
            let sy = ((ty as f32 * height as f32) / target_height as f32).floor() as u32;
            let src_x = sx.min(width - 1);
            let src_y = sy.min(height - 1);
            let src_idx = ((src_y * width + src_x) * 4) as usize;
            let dst_idx = ((ty * target_width + tx) * 4) as usize;
            out[dst_idx..dst_idx + 4].copy_from_slice(&rgba[src_idx..src_idx + 4]);
            if out[dst_idx + 3] == 0 {
                out[dst_idx + 3] = 255;
            }
        }
    }
    (out, target_width, target_height)
}

fn parse_local_image_path_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    let normalized = if let Some(rest) = trimmed.strip_prefix("file:///") {
        rest.replace('/', "\\").replace("%20", " ")
    } else {
        trimmed.to_string()
    };
    if !looks_like_image_file_path(&normalized) {
        return None;
    }
    let path = Path::new(&normalized);
    if !path.exists() || !path.is_file() {
        return None;
    }
    Some(normalized)
}

fn parse_image_from_text_payload(text: &str) -> Option<(Vec<u8>, u32, u32)> {
    if let Some((rgba, width, height)) = parse_data_url_image(text) {
        return Some((rgba, width, height));
    }
    if let Some(src) = extract_img_src_from_html(text) {
        if let Some((rgba, width, height)) = parse_data_url_image(&src) {
            return Some((rgba, width, height));
        }
        if let Some(path) = parse_local_image_path_from_text(&src) {
            if let Ok((rgba, width, height)) = read_local_image_rgba(&path) {
                return Some((rgba, width, height));
            }
        }
    }
    None
}

fn parse_data_url_image(text: &str) -> Option<(Vec<u8>, u32, u32)> {
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
    let dyn_img = ::image::load_from_memory(&bytes).ok()?;
    let rgba8 = dyn_img.to_rgba8();
    let (width, height) = rgba8.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some((rgba8.into_raw(), width, height))
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

fn read_local_image_rgba(path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    let dyn_img = ::image::open(path).map_err(|e| format!("读取本地图片失败: {}", e))?;
    let rgba8 = dyn_img.to_rgba8();
    let (width, height) = rgba8.dimensions();
    let rgba = rgba8.into_raw();
    if rgba.is_empty() || width == 0 || height == 0 {
        return Err("本地图片为空".to_string());
    }
    Ok((rgba, width, height))
}
