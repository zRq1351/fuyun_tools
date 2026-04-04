# 02 - 主要模块职责

## 1. 前端模块（`src/pages`）

项目采用 MPA，每个页面对应独立窗口与入口 HTML。

- `settings`：设置中心，分为剪贴板、截图、录屏、划词、AI、关于（开发模式含开发者页）。
- `clipboard`：文字历史窗口，支持分页、搜索、分类、置顶、快捷回填、AI 快捷动作。
- `image_clipboard`：图片历史窗口，支持虚拟化渲染、分页预取、标签/分类、置顶、预览与回填。
- `image_preview`：大图预览窗口。
- `selection_toolbar`：划词后弹出的小工具栏。
- `result_display`：翻译/解释结果窗口（按类型区分 `result_translation` / `result_explanation`）。
- `screenshot`：全屏截图交互窗口。
- `pinned_image`：固定截图窗口（可多实例）。
- `ocr_text`：OCR 文本结果窗口。
- `recording_toolbar`：录屏胶囊窗口（紧凑/展开尺寸）。

## 2. 前端基础层

- `src/services/ipc.js`：统一封装 `invoke` 命令常量与服务方法。
- `src/pages/shared/`：跨页面共享交互逻辑（右键菜单状态、分类动作、窗口样式）。
- `src/utils/errorHandler.js`：前端错误统一处理入口。

## 3. 后端模块（`src-tauri/src`）

### 3.1 `core`

- `app_state.rs`：全局状态定义与初始化。
- `config.rs`：快捷键常量、AI provider 枚举、基础配置结构。
- `error.rs`：统一错误码与前端错误序列化。
- `logger.rs`：日志插件构建与策略。

### 3.2 `services`

- `clipboard_manager.rs`：文本剪贴板监听开关与历史推送。
- `image_clipboard_manager.rs`：图片监听、入队、去重、异步处理与增量事件推送。
- `ai_client.rs`：AI 客户端封装（OpenAI 兼容接口）。
- `ai_services.rs`：翻译/解释流式任务编排与窗口更新。
- `native_ocr.rs`：Windows OCR 调用。
- `clipboard_wakeup.rs` / `adaptive_poll.rs`：剪贴板唤醒与轮询策略支撑。

### 3.3 `features`

- `mouse_listener.rs`：Windows 低级键鼠 Hook，驱动划词触发逻辑。
- `text_selection.rs`：模拟 `Ctrl+C` 捕获选中文本并恢复原剪贴板。
- `screenshot/`：全屏/区域截图、窗口探测、截图状态控制。
- `recording/`：录屏运行时、音频设备、WASAPI 采集、ffmpeg 执行与回归自测。

### 3.4 `ui`

- `commands.rs`：主命令集合（剪贴板、分类、AI、截图、设置、窗口行为等）。
- `commands_recording.rs`：录屏专属命令（状态、设备、胶囊、ffmpeg 检查/下载）。
- `window_manager.rs`：窗口显示隐藏、定位、结果窗口布局、粘贴模拟输入。
- `tray_menu.rs`：托盘菜单构建与菜单动作绑定。

### 3.5 `utils`

- `settings_model.rs`：设置模型、默认值、迁移与校验、密钥存取。
- `database.rs`：SQLite 数据层。
- `clipboard.rs` / `image_clipboard.rs`：文本与图片管理核心实现。
- `image_process.rs` / `image_store.rs`：图片处理、存储、缓存与去重支撑。

## 4. 启动入口与装配

- `src-tauri/src/lib.rs` 是后端装配中心：
  - 初始化 `AppState`。
  - 注册全局快捷键与冲突提示事件。
  - 注册 Tauri 插件与全部 `invoke` 命令。
  - 启动文本/图片监听器、划词监听器和窗口初始化逻辑。
