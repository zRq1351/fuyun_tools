use crate::features::recording::state::RecordingRuntime;
use crate::sync::Mutex;
use crate::utils::clipboard::ClipboardManager;
use crate::utils::image_clipboard::{set_image_fill_verify_mode, ImageClipboardManager};
use crate::utils::utils_helpers::{load_settings, AppSettingsData};
use std::sync::Arc;

/// 托盘菜单项
#[derive(Clone)]
pub struct TrayMenuItems {
    pub autostart_item: tauri::menu::CheckMenuItem<tauri::Wry>,
}

#[derive(Clone, Default)]
pub struct ForegroundTargetSnapshot {
    pub title: String,
    pub pid: u32,
    pub hwnd: isize,
}

#[derive(Clone, Default)]
pub struct OverlayLifecycleRecord {
    pub label: String,
    pub action: String,
    pub focused: bool,
    pub occurred_at: i64,
}

/// 应用程序全局状态
pub struct AppState {
    pub clipboard_manager: Arc<Mutex<ClipboardManager>>,
    pub image_clipboard_manager: Arc<Mutex<ImageClipboardManager>>,
    pub is_visible: bool,
    pub is_image_visible: bool,
    pub image_history_dirty: bool,
    pub selected_index: usize,
    pub image_selected_index: usize,
    pub settings: AppSettingsData,
    pub is_updating_clipboard: bool,
    pub is_processing_selection: bool,
    pub is_selection_capture_active: bool,
    pub is_text_writeback_active: bool,
    pub is_image_writeback_active: bool,
    pub selection_guard_epoch: u64,
    pub text_fill_seq: u64,
    pub image_fill_seq: u64,
    pub ai_request_seq: u64,
    pub active_translation_op_id: u64,
    pub active_explanation_op_id: u64,
    pub active_custom_prompt_op_id: u64,
    pub last_external_foreground: Option<ForegroundTargetSnapshot>,
    pub active_overlay_window: Option<String>,
    pub last_overlay_lifecycle: Option<OverlayLifecycleRecord>,
    pub overlay_lifecycle_history: Vec<OverlayLifecycleRecord>,
    pub recording_runtime: Arc<Mutex<RecordingRuntime>>,
    pub tray_menu_items: Option<TrayMenuItems>,
}

impl Clone for AppState {
    /// 克隆状态（托盘菜单项不克隆）
    fn clone(&self) -> Self {
        Self {
            clipboard_manager: self.clipboard_manager.clone(),
            image_clipboard_manager: self.image_clipboard_manager.clone(),
            is_visible: self.is_visible,
            is_image_visible: self.is_image_visible,
            image_history_dirty: self.image_history_dirty,
            selected_index: self.selected_index,
            image_selected_index: self.image_selected_index,
            settings: self.settings.clone(),
            is_updating_clipboard: self.is_updating_clipboard,
            is_processing_selection: self.is_processing_selection,
            is_selection_capture_active: self.is_selection_capture_active,
            is_text_writeback_active: self.is_text_writeback_active,
            is_image_writeback_active: self.is_image_writeback_active,
            selection_guard_epoch: self.selection_guard_epoch,
            text_fill_seq: self.text_fill_seq,
            image_fill_seq: self.image_fill_seq,
            ai_request_seq: self.ai_request_seq,
            active_translation_op_id: self.active_translation_op_id,
            active_explanation_op_id: self.active_explanation_op_id,
            last_external_foreground: self.last_external_foreground.clone(),
            active_overlay_window: self.active_overlay_window.clone(),
            last_overlay_lifecycle: self.last_overlay_lifecycle.clone(),
            overlay_lifecycle_history: self.overlay_lifecycle_history.clone(),
            recording_runtime: self.recording_runtime.clone(),
            tray_menu_items: None,
        }
    }
}

impl Default for AppState {
    /// 默认状态初始化
    fn default() -> Self {
        let saved_settings = load_settings().unwrap_or_default();
        set_image_fill_verify_mode(&saved_settings.image_fill_verify_mode);

        Self {
            clipboard_manager: Arc::new(Mutex::new(ClipboardManager::new(
                saved_settings.text_max_items,
                saved_settings.grouped_items_protected_from_limit,
            ))),
            image_clipboard_manager: Arc::new(Mutex::new(ImageClipboardManager::new(
                saved_settings.image_max_items,
                saved_settings.image_disk_limit_mb,
                saved_settings.grouped_items_protected_from_limit,
            ))),
            is_visible: false,
            is_image_visible: false,
            image_history_dirty: true,
            selected_index: 0,
            image_selected_index: 0,
            settings: saved_settings,
            is_updating_clipboard: false,
            is_processing_selection: false,
            is_selection_capture_active: false,
            is_text_writeback_active: false,
            is_image_writeback_active: false,
            selection_guard_epoch: 0,
            text_fill_seq: 0,
            image_fill_seq: 0,
            ai_request_seq: 0,
            active_translation_op_id: 0,
            active_explanation_op_id: 0,
            active_custom_prompt_op_id: 0,
            last_external_foreground: None,
            active_overlay_window: None,
            last_overlay_lifecycle: None,
            overlay_lifecycle_history: Vec::new(),
            recording_runtime: Arc::new(Mutex::new(RecordingRuntime::default())),
            tray_menu_items: None,
        }
    }
}

/// 共享应用程序状态别名
pub type SharedAppState = AppState;
