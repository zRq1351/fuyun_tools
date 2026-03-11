use crate::utils::clipboard::ClipboardManager;
use crate::utils::image_clipboard::ImageClipboardManager;
use crate::utils::utils_helpers::{load_settings, AppSettingsData};
use std::sync::{Arc, Mutex};

/// 托盘菜单项
#[derive(Clone)]
pub struct TrayMenuItems {
    pub autostart_item: tauri::menu::CheckMenuItem<tauri::Wry>,
}

/// 应用程序全局状态
pub struct AppState {
    pub clipboard_manager: Arc<Mutex<ClipboardManager>>,
    pub image_clipboard_manager: Arc<Mutex<ImageClipboardManager>>,
    pub is_visible: bool,
    pub is_image_visible: bool,
    pub selected_index: usize,
    pub image_selected_index: usize,
    pub settings: AppSettingsData,
    pub is_updating_clipboard: bool,
    pub is_processing_selection: bool,
    pub text_fill_seq: u64,
    pub image_fill_seq: u64,
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
            selected_index: self.selected_index,
            image_selected_index: self.image_selected_index,
            settings: self.settings.clone(),
            is_updating_clipboard: self.is_updating_clipboard,
            is_processing_selection: self.is_processing_selection,
            text_fill_seq: self.text_fill_seq,
            image_fill_seq: self.image_fill_seq,
            tray_menu_items: None,
        }
    }
}

impl Default for AppState {
    /// 默认状态初始化
    fn default() -> Self {
        let saved_settings = load_settings().unwrap_or_default();

        Self {
            clipboard_manager: Arc::new(Mutex::new(ClipboardManager::new(
                saved_settings.max_items,
                saved_settings.grouped_items_protected_from_limit,
            ))),
            image_clipboard_manager: Arc::new(Mutex::new(ImageClipboardManager::new(
                saved_settings.max_items,
                saved_settings.grouped_items_protected_from_limit,
            ))),
            is_visible: false,
            is_image_visible: false,
            selected_index: 0,
            image_selected_index: 0,
            settings: saved_settings,
            is_updating_clipboard: false,
            is_processing_selection: false,
            text_fill_seq: 0,
            image_fill_seq: 0,
            tray_menu_items: None,
        }
    }
}

/// 共享应用程序状态别名
pub type SharedAppState = AppState;
