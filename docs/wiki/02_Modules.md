# 02 - 主要模块职责

## 1. 前端模块 (Frontend Modules)
项目前端采用了多页应用架构（Multiple Page Application），通过 Vite (`vite.config.js`) 的 `rollupOptions.input` 划分了多个入口 HTML 文件：

- **`src/pages/settings`**: 应用设置界面，包括常规、AI 设置、剪贴板上限、截图、划词配置。
- **`src/pages/clipboard`**: 文字剪贴板历史窗口，支持历史查询、搜索、分类、快捷回填（支持双击、方向键选择、回车）。
- **`src/pages/image_clipboard`**: 图片剪贴板历史窗口，负责以缩略图形式展示图片并支持一键粘贴和分类。
- **`src/pages/image_preview`**: 图片预览窗口，展示剪贴板内选中大图的详细视图。
- **`src/pages/selection_toolbar`**: AI 划词工具栏，当用户在系统其他地方划词（选中文本）后自动弹出的小悬浮窗。
- **`src/pages/result_display`**: 划词结果（如翻译、解释）流式输出展示窗口。
- **`src/pages/screenshot`**: 截图窗口，提供屏幕快照编辑器。
- **`src/pages/pinned_image`**: 置顶图片窗口。
- **`src/pages/ocr_text`**: OCR 文本展示窗口，展示图片文字识别结果。

## 2. 后端模块 (Backend Modules)
后端的 Rust 代码按业务结构存放在 `src-tauri/src/` 目录下：

### 2.1 `core` 模块
- 包含应用运行生命周期的基础配置：
  - **`app_state.rs`**: 定义并管理全局状态 (`AppState`)，包含当前是否在截屏、各个窗口的可见性等标志。
  - **`config.rs`**: 加载和序列化用户配置文件，如热键配置、AI 提供商模型选择。
  - **`error.rs`**: 定义统一的应用错误类型 `AppError` 及序列化为前端可见的字符串。
  - **`logger.rs`**: 系统日志初始化与写入策略。

### 2.2 `services` 模块
- 封装系统级或后台常驻的服务与业务逻辑：
  - **`clipboard_manager.rs` & `image_clipboard_manager.rs`**: 负责监听系统剪贴板（文字、图片）变化，并在变化时存入本地 SQLite 或内存 LRU Cache，并向前端派发 `emit` 事件。
  - **`ai_client.rs` & `ai_services.rs`**: 对接各类大语言模型（兼容 OpenAI API），处理流式 (Streaming) 文本翻译与解释，通过 Tauri Emitter 推送至前端。
  - **`native_ocr.rs`**: 调用系统底层（如 Windows.Media.Ocr）进行图片转文字。

### 2.3 `features` 模块
- 提供独立的高级功能模块：
  - **`mouse_listener.rs` & `text_selection.rs`**: 监听鼠标双击、三击、拖拽选中，检测到选中事件后调用系统快捷键复制内容，并在光标处弹出划词工具栏。
  - **`screenshot/`**: 包含屏幕捕捉、活动窗口识别等底层截屏 API。

### 2.4 `ui` 模块
- 负责 Tauri UI 与 IPC 指令绑定：
  - **`commands.rs`**: 暴露所有给前端调用的 Tauri Commands，如 `get_clipboard_history`、`copy_and_paste_text`、`open_settings_window` 等。
  - **`window_manager.rs`**: 负责所有 Tauri `WebviewWindow` 的创建、显示、隐藏、置顶及位置计算。
  - **`tray_menu.rs`**: 管理系统托盘图标、菜单及其响应逻辑。

### 2.5 `utils` 模块
- 通用工具类：
  - **`database.rs`**: SQLite 连接池与建表逻辑、记录的增删改查。
  - **`image_clipboard.rs` / `image_process.rs` / `image_store.rs`**: 处理图片缩放、存储到本地文件系统、计算特征 Hash、特征匹配去重等逻辑。
