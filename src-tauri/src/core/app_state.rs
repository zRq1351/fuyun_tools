use crate::features::recording::state::RecordingRuntime;
use crate::sync::Mutex;
use crate::utils::clipboard::ClipboardManager;
use crate::utils::image_clipboard::{set_image_fill_verify_mode, ImageClipboardManager};
use crate::utils::utils_helpers::{load_settings, AppSettingsData};
use std::collections::VecDeque;
use std::sync::Arc;

/// 本应用写入剪贴板的序列号指纹队列容量
const SELF_WRITE_SEQ_QUEUE_CAPACITY: usize = 16;

/// 托盘菜单项
#[derive(Clone)]
pub struct TrayMenuItems {
    pub autostart_item: tauri::menu::CheckMenuItem<tauri::Wry>,
}

/// 剪贴板互斥活动类型。
///
/// 这些活动可能短暂叠加进行（例如划词捕获期间用户点击历史项触发回填），
/// 因此用"进行中活动集合"表达，而不是 5 个独立布尔 + 手动 recompute。
/// 两个对外谓词 [`AppState::is_updating_clipboard`] / [`AppState::is_processing_selection`]
/// 由集合实时推导，消除"改了标志忘了同步派生值"一类竞态。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipboardActivity {
    /// 划词捕获会话进行中（Ctrl+C 模拟 / 选区读取窗口期）
    SelectionCapture,
    /// 文本回填进行中（删除 + 重写剪贴板的窗口期）
    TextWriteback,
    /// 图片回填进行中（同上）
    ImageWriteback,
    /// 手机同步写入本机剪贴板（防回环守卫）
    SyncClipboardWrite,
    /// 剪贴板维护操作（删除历史后重写剪贴板等）
    ClipboardMaintenance,
}

/// 进行中剪贴板活动位集（当前 5 种活动，u8 位图足够）
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct ClipboardActivitySet(u8);

impl ClipboardActivitySet {
    fn bit(activity: ClipboardActivity) -> u8 {
        1 << (activity as u8)
    }

    pub fn insert(&mut self, activity: ClipboardActivity) {
        self.0 |= Self::bit(activity);
    }

    pub fn remove(&mut self, activity: ClipboardActivity) {
        self.0 &= !Self::bit(activity);
    }

    pub fn contains(&self, activity: ClipboardActivity) -> bool {
        self.0 & Self::bit(activity) != 0
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// 除指定活动外是否还有其他活动在进行
    fn has_any_except(&self, activity: ClipboardActivity) -> bool {
        self.0 & !Self::bit(activity) != 0
    }

    /// 原 `is_updating_clipboard` 语义：剪贴板正被本应用写入/维护，
    /// 文本与图片轮询器应静默跳过本次读取，防止回环入库。
    pub fn is_updating_clipboard(&self) -> bool {
        self.contains(ClipboardActivity::TextWriteback)
            || self.contains(ClipboardActivity::ImageWriteback)
            || self.contains(ClipboardActivity::SyncClipboardWrite)
            || self.contains(ClipboardActivity::ClipboardMaintenance)
    }

    /// 原 `is_processing_selection` 语义：划词捕获或回填/维护流程进行中，
    /// 轮询器需结合"手动复制"标志决定是否放行。
    pub fn is_processing_selection(&self) -> bool {
        self.has_any_except(ClipboardActivity::SyncClipboardWrite)
    }
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
    /// 进行中的剪贴板互斥活动集合（派生谓词见 [`AppState::is_updating_clipboard`] 等）
    pub clipboard_activities: ClipboardActivitySet,
    /// 本应用最近写入剪贴板的序列号指纹（防自捕获回声，见
    /// [`AppState::record_self_clipboard_write`]）
    pub self_write_clipboard_seqs: VecDeque<u32>,
    pub text_fill_seq: u64,
    pub image_fill_seq: u64,
    pub ai_request_seq: u64,
    pub active_translation_op_id: u64,
    pub active_explanation_op_id: u64,
    pub active_custom_prompt_op_id: u64,
    pub last_external_foreground: Option<ForegroundTargetSnapshot>,
    pub active_overlay_window: Option<String>,
    pub last_overlay_lifecycle: Option<OverlayLifecycleRecord>,
    pub overlay_lifecycle_history: VecDeque<OverlayLifecycleRecord>,
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
            clipboard_activities: self.clipboard_activities,
            self_write_clipboard_seqs: self.self_write_clipboard_seqs.clone(),
            text_fill_seq: self.text_fill_seq,
            image_fill_seq: self.image_fill_seq,
            ai_request_seq: self.ai_request_seq,
            active_translation_op_id: self.active_translation_op_id,
            active_explanation_op_id: self.active_explanation_op_id,
            active_custom_prompt_op_id: self.active_custom_prompt_op_id,
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
            clipboard_activities: ClipboardActivitySet::default(),
            self_write_clipboard_seqs: VecDeque::new(),
            text_fill_seq: 0,
            image_fill_seq: 0,
            ai_request_seq: 0,
            active_translation_op_id: 0,
            active_explanation_op_id: 0,
            active_custom_prompt_op_id: 0,
            last_external_foreground: None,
            active_overlay_window: None,
            last_overlay_lifecycle: None,
            overlay_lifecycle_history: VecDeque::new(),
            recording_runtime: Arc::new(Mutex::new(RecordingRuntime::default())),
            tray_menu_items: None,
        }
    }
}

impl AppState {
    /// 原 `is_updating_clipboard` 字段的派生谓词（由活动集合实时推导）
    pub fn is_updating_clipboard(&self) -> bool {
        self.clipboard_activities.is_updating_clipboard()
    }

    /// 原 `is_processing_selection` 字段的派生谓词（由活动集合实时推导）
    pub fn is_processing_selection(&self) -> bool {
        self.clipboard_activities.is_processing_selection()
    }

    /// 本应用写入剪贴板后调用：记录当前剪贴板序列号指纹。
    /// 轮询器读回内容前先经 [`AppState::is_self_clipboard_write`] 比对，
    /// 命中即丢弃——解决"活动位只挡写入瞬间，押后事件在位清除后被
    /// 消费导致自捕获回声"的问题（回填/维护/同步写入/划词恢复的内容
    /// 被当新复制项入库并广播）。
    pub fn record_self_clipboard_write(&mut self) {
        let seq = clipboard_sequence_number();
        if seq == 0 {
            // 非 Windows 平台或获取失败：无指纹可用
            return;
        }
        let queue = &mut self.self_write_clipboard_seqs;
        if queue.contains(&seq) {
            return;
        }
        if queue.len() >= SELF_WRITE_SEQ_QUEUE_CAPACITY {
            queue.pop_front();
        }
        queue.push_back(seq);
    }

    /// 当前剪贴板序列号是否为本应用写入。
    /// 注意：只做包含判断、不消费指纹——文本与图片两个轮询器都会收到
    /// 同一剪贴板事件，消费语义会导致先到者吞掉指纹、后者误入库。
    /// 序列号全局单调递增，陈旧指纹不可能与用户后续复制的新序列号相同。
    pub fn is_self_clipboard_write(&self) -> bool {
        let seq = clipboard_sequence_number();
        seq != 0 && self.self_write_clipboard_seqs.contains(&seq)
    }
}

#[cfg(target_os = "windows")]
fn clipboard_sequence_number() -> u32 {
    // SAFETY: GetClipboardSequenceNumber 无参数、无副作用限制，仅读取全局序列号
    unsafe { winapi::um::winuser::GetClipboardSequenceNumber() }
}

#[cfg(not(target_os = "windows"))]
fn clipboard_sequence_number() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_set_yields_both_predicates_false() {
        let set = ClipboardActivitySet::default();
        assert!(!set.is_updating_clipboard());
        assert!(!set.is_processing_selection());
    }

    #[test]
    fn derived_predicates_match_legacy_formulas() {
        let mut set = ClipboardActivitySet::default();

        // 划词捕获：仅 processing 为真
        set.insert(ClipboardActivity::SelectionCapture);
        assert!(set.is_processing_selection());
        assert!(!set.is_updating_clipboard());

        // 文本回填：两者均为真
        set.insert(ClipboardActivity::TextWriteback);
        assert!(set.is_processing_selection());
        assert!(set.is_updating_clipboard());

        // 同步写入：仅 updating 为真（与旧派生公式一致）
        set.insert(ClipboardActivity::SyncClipboardWrite);
        assert!(set.is_processing_selection());
        assert!(set.is_updating_clipboard());

        // 逐一清理后归零
        set.remove(ClipboardActivity::SyncClipboardWrite);
        set.remove(ClipboardActivity::TextWriteback);
        set.remove(ClipboardActivity::SelectionCapture);
        assert!(!set.is_processing_selection());
        assert!(!set.is_updating_clipboard());
    }

    #[test]
    fn insert_remove_are_idempotent() {
        let mut set = ClipboardActivitySet::default();
        set.insert(ClipboardActivity::ClipboardMaintenance);
        set.insert(ClipboardActivity::ClipboardMaintenance);
        assert!(set.contains(ClipboardActivity::ClipboardMaintenance));
        set.remove(ClipboardActivity::ClipboardMaintenance);
        set.remove(ClipboardActivity::ClipboardMaintenance);
        assert!(set.is_empty());
    }

    #[test]
    fn sync_write_only_sets_updating_not_processing() {
        // 关键例外回归：仅同步写入进行中时，updating 为真、processing 为假
        // （轮询器静默跳过，而非走"手动复制"放行分支）
        let mut set = ClipboardActivitySet::default();
        set.insert(ClipboardActivity::SyncClipboardWrite);
        assert!(set.is_updating_clipboard());
        assert!(!set.is_processing_selection());
    }
}
