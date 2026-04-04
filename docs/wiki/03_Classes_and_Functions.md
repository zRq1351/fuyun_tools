# 03 - 关键类与函数说明

本页聚焦“高频改动/排障最常用”的结构体与函数入口，便于快速定位代码。

## 1. 核心结构体与状态

### `AppState`（`core/app_state.rs`）

- 全局共享状态根对象。
- 关键字段：
  - `settings`：运行期设置快照。
  - `clipboard_manager` / `image_clipboard_manager`：文本与图片管理器。
  - `is_visible` / `is_image_visible`：窗口可见状态。
  - `text_fill_seq` / `image_fill_seq`：回填链路序列控制，防止过期请求生效。
  - `recording_runtime`：录屏运行时状态。

### `AppSettingsData`（`utils/settings_model.rs`）

- 设置数据模型，负责默认值、校验与迁移。
- 覆盖功能开关、快捷键、AI provider 配置、录屏参数、提示词模板、ffmpeg 下载地址。
- API Key 不明文落盘，使用 keyring 存储。

### `RecordingRuntime` / `RecordingRuntimeState`（`features/recording/state.rs` / `types.rs`）

- 记录录屏阶段（idle/starting/recording/paused/stopping/error）。
- 持有会话 ID、输出路径、码率/FPS、暂停累计时长、自动停止标记等。

## 2. 关键后端服务函数

### 启动与命令装配

- `run()`（`lib.rs`）：
  - 初始化全局状态与窗口行为。
  - 注册快捷键与冲突提示。
  - 注册全部 Tauri `invoke` 命令与插件。
  - 启动剪贴板与划词监听。

### 剪贴板链路

- `set_clipboard_listener_enabled()`（`services/clipboard_manager.rs`）：
  - 控制文本监听线程启停。
- `set_image_clipboard_listener_enabled()`（`services/image_clipboard_manager.rs`）：
  - 控制图片监听线程启停与队列消费。
- `emit_image_history_payload()`（`services/image_clipboard_manager.rs`）：
  - 统一发送图片历史全量 payload。
- `select_and_fill()` / `select_and_fill_image_by_id()`（`ui/commands.rs`）：
  - 触发文本/图片回填主流程（隐藏窗口 -> 写剪贴板 -> 模拟粘贴）。
- `copy_and_paste_text()`（`ui/commands.rs`）：
  - 结果窗口一键回写，内置短时去重与粘贴重试。

### 划词与 AI 链路

- `set_selection_listener_enabled()`（`features/mouse_listener.rs`）：
  - 划词监听总开关，控制 Hook 与工具栏显隐。
- `get_selected_text_with_app()`（`features/text_selection.rs`）：
  - `Ctrl+C` 捕获选区文本并恢复原剪贴板。
- `stream_translate_text()` / `stream_explain_text()`（`services/ai_services.rs`）：
  - 执行流式 AI 请求并实时推送结果窗口。

### 截图链路

- `open_screenshot_editor()`（`ui/commands.rs`）：
  - 捕获全屏并初始化截图窗口启动数据。
- `capture_region()` / `save_screenshot()` / `pin_screenshot_on_screen()`（`ui/commands.rs`）：
  - 区域截取、保存、固定图片窗口操作。
- `recognize_image_ocr()`（`ui/commands.rs`）：
  - OCR 识别入口。

### 录屏链路

- `check_recording_ffmpeg()` / `download_recording_ffmpeg()`（`ui/commands_recording.rs`）：
  - ffmpeg 检查与按需下载，带进度事件。
- `start_recording()` / `pause_recording()` / `resume_recording()` / `stop_recording()`（`features/recording/recorder_service.rs`）：
  - 录屏状态机主入口。
- `toggle_recording_from_shortcut()`（`ui/commands_recording.rs`）：
  - 快捷键唤起胶囊（紧凑模式）。

## 3. 关键前端模块函数

### `src/services/ipc.js`

- 前后端命令协议单点定义。
- `ClipboardService` / `ImageClipboardService` / `AISettingsService` / `RecordingService` 统一封装调用。

### `useClipboardHistory()`（`pages/clipboard/composables/useClipboardHistory.js`）

- 文本历史分页加载、筛选、排序、置顶和本地增量合并逻辑。
- `applyPayloadSnapshot()` 支撑后端全量 payload 快速落地。

### `image_clipboard/App.vue`

- 图片历史虚拟化渲染、滚动预加载、预热与异步预览缓存。
- 监听 `show-image-window`、`image-history-payload-updated`、`image-history-item-added` 等事件做增量刷新。

### `settings/App.vue` + `useAIProvider.js`

- 设置页面自动保存（差量字段上传）。
- 录屏启用前自动触发 ffmpeg 检查/下载流程。
- AI provider 切换、连接测试、自定义 provider 删除管理。
