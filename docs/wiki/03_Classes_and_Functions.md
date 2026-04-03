# 03 - 关键类与函数说明

本页面列出了项目中最为核心的几个类和函数，它们构成了整个系统的骨架。

## 1. 后端 (Rust) 关键类

### `AppState` (位于 `core/app_state.rs`)
**功能**：管理全局应用程序状态，是 Tauri 后端中生命周期最长的对象。
**属性**：
- `settings`: 保存从配置文件加载的用户设置（如快捷键、是否启用剪贴板、AI提供商信息等）。
- `is_visible` / `is_image_visible`: 标识主窗口或图片窗口当前是否在屏幕上可见。
- `is_ocr_active`: 标识当前系统是否正在进行 OCR 任务。

### `ClipboardManager` (位于 `utils/clipboard.rs` / `services/clipboard_manager.rs`)
**功能**：全局单例，处理系统剪贴板文字的监听、记录和去重。
**关键方法**：
- `check_clipboard_and_store`: 轮询或监听系统剪贴板事件，比较新旧文本的 Hash，如果为新内容则保存至 SQLite 数据库并发出事件 `clipboard-update` 通知前端。
- `remove_item` / `clear_history`: 清理过期或主动删除的剪贴板历史。

### `ImageClipboardManager` (位于 `services/image_clipboard_manager.rs`)
**功能**：专门针对图片的剪贴板管理类。由于图片数据量大，它包含了一套缩略图生成、大图文件系统持久化与 LRU 缓存策略。
**关键方法**：
- `store_image`: 将新图片转换为缩略图（Base64 或文件路径）和大图，并生成特征 Hash 避免重复。
- `emit_image_history_payload`: 批量读取图片数据推送到前端渲染列表。

### `AIClient` (位于 `services/ai_client.rs`)
**功能**：对 `async-openai` 的封装，处理所有发往大语言模型的请求。
**关键方法**：
- `stream_chat_completion`: 发送请求，建立流式连接，将 AI 吐出的 Token 通过 Tauri Window 的 `emit` 发送到前端（如 `result_display` 窗口）。

## 2. 后端 (Rust) 关键函数

### `commands::copy_and_paste_text(text: String, ...)` (位于 `ui/commands.rs`)
**功能**：实现“快捷回填”功能。当前端点击某条历史记录时：
1. 隐藏当前剪贴板窗口并将焦点交还给前一个应用程序。
2. 将指定的 `text` 写入系统剪贴板。
3. 使用 `enigo` 库模拟 `Ctrl+V` (Windows/Linux) 或 `Cmd+V` (macOS) 键盘事件，将内容粘贴到光标处。

### `start_text_selection_listener` (位于 `lib.rs` / `features/mouse_listener.rs`)
**功能**：Windows 独占功能。通过底层全局鼠标 Hook（`rdev` 库），监听用户的鼠标左键按下、拖拽与松开。
1. 若判断为有效选词（如双击、三击或拖动一段距离），则触发系统复制动作（`Ctrl+C`）。
2. 获取复制到的文本，如果有效，则在光标附近弹出 `selection_toolbar` 窗口供用户进行“翻译”或“解释”。

### `open_screenshot_editor` (位于 `features/screenshot/capture.rs`)
**功能**：触发全屏截图并打开裁剪工具。
1. 调用 `screenshots` 库捕获当前所有屏幕的图像。
2. 创建或显示 `screenshot` 全屏无边框窗口。
3. 将截图以 Base64 或本地文件形式传给前端，前端进行框选渲染。

## 3. 前端 (Vue 3) 关键组件

### `ClipboardList.vue` (位于 `src/pages/clipboard/components/`)
**功能**：文字剪贴板的展示列表。
**特性**：
- 使用了虚拟列表或分页加载以优化长历史记录的渲染。
- 绑定了键盘事件监听（方向键移动高亮项，回车键触发回填）。

### `useClipboardHistory.js` (位于 `src/pages/clipboard/composables/`)
**功能**：Vue 3 组合式 API (Composable)，封装了与后端的 IPC 通信逻辑。
- 包含了 `fetchHistory`、`removeItem`、`pasteItem` 等方法，使 UI 组件与底层 Tauri Command 解耦。

### `useAIProvider.js` (位于 `src/pages/settings/composables/`)
**功能**：管理 AI 提供商配置的响应式逻辑，支持添加自定义提供商、测试连接（调用后端的 `get_ai_settings` 或发起测试请求）并将配置持久化保存。
