# 04 - 依赖关系

本文按“功能用途”说明当前主要依赖，版本以仓库 `package.json` 与 `Cargo.toml` 为准。

## 1. 前端依赖（`src/package.json`）

### 1.1 核心框架

- `vue`：前端响应式框架。
- `vite`：开发服务器与构建工具。
- `@vitejs/plugin-vue`：Vite 的 Vue 编译支持。

### 1.2 UI 与交互

- `element-plus`：设置页、列表、弹窗等基础组件。
- `@element-plus/icons-vue`：Element Plus 图标组件。
- `lucide-vue-next`：补充图标集（如置顶图标）。
- `sass`：样式预处理支持。

### 1.3 Tauri 前端 SDK 与插件

- `@tauri-apps/api`：窗口、事件、调用 `invoke` 等基础能力。
- `@tauri-apps/plugin-dialog`：文件选择/保存等原生对话框能力。
- `@tauri-apps/plugin-opener`：打开外部链接。
- `@tauri-apps/plugin-process`：进程能力桥接。
- `@tauri-apps/plugin-updater`：更新能力前端接入。
- `@tauri-apps/cli`：Tauri 命令行工具（开发/构建脚本依赖）。

### 1.4 业务工具库

- `marked`：AI 结果中的 Markdown 渲染。

## 2. 后端依赖（`src-tauri/Cargo.toml`）

### 2.1 Tauri 核心与插件

- `tauri`（启用 `protocol-asset`、`tray-icon`）。
- 插件：
  - `tauri-plugin-global-shortcut`
  - `tauri-plugin-autostart`
  - `tauri-plugin-log`
  - `tauri-plugin-opener`
  - `tauri-plugin-clipboard-manager`
  - `tauri-plugin-notification`
  - `tauri-plugin-dialog`
  - `tauri-plugin-updater`
  - `tauri-plugin-positioner`

### 2.2 AI 与网络

- `async-openai`：OpenAI 兼容接口客户端。
- `futures-util`：流式异步处理辅助。
- `reqwest`：ffmpeg 下载与 HTTP 请求。

### 2.3 数据与序列化

- `serde` / `serde_json`：配置与 IPC 序列化。
- `sqlx`（`sqlite` + `runtime-tokio-rustls`）：本地数据库访问。

### 2.4 图像、缓存与去重

- `image`：图片解码与转换。
- `screenshots`：屏幕捕获。
- `lru`：缓存策略。
- `bloom`：快速重复判断辅助。
- `xxhash-rust`：高性能 hash。

### 2.5 系统交互与安全

- `enigo`：模拟键盘输入（回填/复制链路）。
- `keyring`：系统凭据存储 API Key。
- `regex`：文本匹配与过滤。
- `parking_lot`：并发锁实现。

### 2.6 平台相关依赖

- Windows 专用：
  - `winapi`
  - `windows`
  - `cpal`
  - `hound`
- 条件编译的 `enigo`：
  - Windows/macOS 使用默认配置。
  - Linux 使用 `x11rb` 特性。

## 3. 依赖与功能映射

- 剪贴板监听与回填：`tauri-plugin-clipboard-manager` + `enigo`。
- 划词监听：Windows Hook（`winapi`）+ 文本过滤（`regex`）。
- 截图/OCR：`screenshots` + `windows` OCR API。
- 录屏：ffmpeg 进程编排 + `cpal`/`hound` WASAPI 音频链路 + `reqwest` 下载能力。
- AI：`async-openai` 流式输出 + 前端 `marked` 渲染。
