# Fuyun Tools - Code Wiki

欢迎来到 **fuyun_tools** 的代码知识库 (Code Wiki)！本 Wiki 旨在帮助开发者快速了解本项目的整体架构、模块职责、核心类与运行方式。

## 项目简介

**fuyun_tools** 是一款常驻系统托盘的桌面效率工具，聚焦于：
1. **剪贴板管理**：高效管理文字和图片剪贴板历史，支持搜索、分类和快捷回填。
2. **AI 划词助手**：在 Windows 上进行划词翻译与解释，支持流式输出与一键回填。
3. **截图与 OCR**：在 Windows 上进行快捷截图与图片 OCR 文字提取。

本项目采用 **AI 全流程开发模式**，并且拥有深度优化的系统级交互体验（热键、鼠标监听、原生系统剪贴板对接等）。

## 整体架构 (Overall Architecture)

本项目采用了典型的 **Tauri 2 客户端架构**，分为前端 (Frontend) 与后端 (Backend) 两部分，通过 Tauri 的 IPC (Inter-Process Communication) 进行通信。

### 1. 前端 (UI Layer)
- **技术栈**：Vue 3 + Element Plus + Vite
- **架构模式**：**多页应用 (MPA)**。为了实现多个独立的独立窗口（如剪贴板主窗口、设置窗口、划词工具栏窗口、OCR 结果窗口等），前端使用 Vite 配置了多个入口 HTML 文件。
- **通信方式**：通过 `@tauri-apps/api/core` 中的 `invoke` 调用后端 Rust 提供的方法，并监听后端触发的事件（Event）。

### 2. 后端 (Core Layer)
- **技术栈**：Rust + Tauri 2 + SQLite (sqlx)
- **核心职责**：
  - **系统级集成**：常驻托盘 (Tray)、全局快捷键 (Global Shortcuts)、系统凭据管理器存取 (keyring)、开机自启。
  - **硬件与输入监听**：通过 `enigo` 和 `rdev` 模拟键盘鼠标输入（实现一键回填）、监听鼠标划词动作触发 AI 助手。
  - **原生功能**：剪贴板读写监听 (Clipboard)、原生 OCR 调用 (Windows Media OCR)、截图截屏 (screenshots)。
  - **业务逻辑与持久化**：管理 SQLite 本地数据库，保存剪贴板历史记录、用户配置、AI 接口配置等。
  - **网络请求**：通过 `async-openai` 库对接多种 AI 大模型（DeepSeek、通义千问、Mimo 等）。

## 目录导航

请查看以下文档深入了解项目细节：

- [02 - 主要模块职责](./02_Modules.md)
- [03 - 关键类与函数说明](./03_Classes_and_Functions.md)
- [04 - 依赖关系](./04_Dependencies.md)
- [05 - 项目运行方式](./05_Run_and_Build.md)
