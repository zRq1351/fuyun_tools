use crate::utils::clipboard::ClipboardManager;
use crate::utils::utils_helpers::{load_settings, AppSettingsData};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct TrayMenuItems {
    pub autostart_item: tauri::menu::CheckMenuItem<tauri::Wry>,
}

pub struct AppState {
    pub clipboard_manager: Arc<Mutex<ClipboardManager>>,
    pub is_visible: bool,
    pub selected_index: usize,
    pub settings: AppSettingsData,
    pub is_updating_clipboard: bool,
    pub is_processing_selection: bool,
    pub tray_menu_items: Option<TrayMenuItems>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            clipboard_manager: self.clipboard_manager.clone(),
            is_visible: self.is_visible,
            selected_index: self.selected_index,
            settings: self.settings.clone(),
            is_updating_clipboard: self.is_updating_clipboard,
            is_processing_selection: self.is_processing_selection,
            tray_menu_items: None,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        let saved_settings = load_settings().unwrap_or_default();

        Self {
            clipboard_manager: Arc::new(Mutex::new(ClipboardManager::new(
                saved_settings.max_items,
            ))),
            is_visible: false,
            selected_index: 0,
            settings: saved_settings,
            is_updating_clipboard: false,
            is_processing_selection: false,
            tray_menu_items: None,
        }
    }
}

pub type SharedAppState = AppState;