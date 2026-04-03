# 04 - 依赖关系

**fuyun_tools** 建立在诸多成熟的开源库之上。

## 1. 前端依赖 (`package.json`)

前端是一个标准的基于 Node.js 环境的 Vue 3 SPA/MPA 项目：

### 核心框架
- **`vue`** (`^3.4.0`): 构建界面的前端框架。
- **`vite`** (`^5.0.0`): 极速前端构建工具。

### UI 与样式
- **`element-plus`** (`^2.5.0`): 基于 Vue 3 的组件库（提供输入框、按钮、下拉菜单、设置面板等）。
- **`@element-plus/icons-vue`** (`^2.3.0`): Element Plus 官方图标库。
- **`lucide-vue-next`** (`^0.577.0`): 现代化的图标库，为应用提供美观统一的 SVG 图标。
- **`sass`** (`^1.70.0`): 样式预处理器，支持复杂的嵌套和变量。

### Tauri 官方前端插件
- **`@tauri-apps/api`** (`^2.10.1`): 核心 API，包含事件、窗口管理、文件系统等。
- **`@tauri-apps/plugin-dialog`**: 原生对话框（打开/保存文件、提示框）。
- **`@tauri-apps/plugin-opener`**: 默认浏览器或应用打开 URL。
- **`@tauri-apps/plugin-process`**: 系统进程相关操作。
- **`@tauri-apps/plugin-updater`**: 自动更新前端模块。

### 工具库
- **`marked`** (`^17.0.1`): 极速的 Markdown 解析器，用于将 AI 返回的 Markdown 解释或翻译内容渲染为 HTML。

## 2. 后端依赖 (`Cargo.toml`)

后端是用 Rust 编写的 Tauri Core，涉及大量的系统级操作。

### 核心框架
- **`tauri`** (`version = "2"`): 构建轻量级、跨平台桌面应用的框架。包含了 `protocol-asset`, `tray-icon` 等特性。
- **`tauri-plugin-*`**: 一系列 Tauri 官方及社区插件，如 `global-shortcut` (全局快捷键)、`autostart` (开机自启)、`log` (日志)、`clipboard-manager` (剪贴板读写)、`updater` (自动更新) 等。

### 序列化与日志
- **`serde`** / **`serde_json`**: 高性能数据序列化/反序列化库，用于配置解析及与前端 IPC 的 JSON 交互。
- **`log`**: 标准日志门面。

### AI 与网络
- **`async-openai`** (`0.34.0`): 异步的 OpenAI API 客户端，用于对接 DeepSeek、通义千问等兼容模型。
- **`futures-util`**: 异步任务的扩展工具，常用于流式 (Streaming) 处理。

### 安全与加密
- **`keyring`** (`3.6.3`): 跨平台凭据存储，专门用于将 API Key 存入 Windows 凭据管理器，防止明文泄露。

### 图像与多媒体
- **`image`** (`0.25.10`): 处理剪贴板或截图获取到的图像数据（如缩放、格式转换 png/jpeg/webp）。
- **`screenshots`** (`0.8.10`): 跨平台屏幕截图库，用于捕获显示器画面。

### 数据库与缓存
- **`sqlx`** (`0.8.6`): 纯异步、强类型的 SQL 框架，使用 `sqlite` 特性。
- **`lru`**: 内存缓存淘汰算法，用于快速读取最近使用过的历史或图片。
- **`bloom`**: 布隆过滤器，用于在大规模历史中快速判断重复（如判断刚复制的一段长文本是否已存在）。
- **`xxhash-rust`**: 极速非加密哈希算法，用于生成文本或图片的特征哈希值以做去重。

### 系统交互 (底层)
- **`rdev`** (`0.5.3`): 全局鼠标/键盘钩子，监听 Windows 系统中用户的划词、点击等底层操作。
- **`enigo`** (`0.6.1`): 跨平台的鼠标键盘模拟库，用于自动粘贴（模拟 `Ctrl+V`）。
- **`winapi`** / **`windows`**: Windows 独有，包含 `winuser` (窗口管理)、`Media_Ocr` (调用 Windows 10/11 免费本地 OCR 引擎) 等大量底层 API。
