# 05 - 项目运行方式

## 环境准备

在开始开发或构建 `fuyun_tools` 之前，请确保您的开发环境满足以下要求：

1. **Node.js** (推荐 v18+ 或 v20+)
2. **npm** (Node Package Manager)
3. **Rust 工具链** (最新稳定版 `rustc`, `cargo` 等，可通过 [rustup](https://rustup.rs/) 安装)
4. **C++ 构建工具**
   - **Windows**: Visual Studio 2022 C++ 构建工具 (安装 "Desktop development with C++" 工作负载) 和 Windows 10/11 SDK。
   - **Linux**: `build-essential`, `libwebkit2gtk-4.0-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev` 等 Tauri 依赖。
   - **macOS**: Xcode Command Line Tools (`xcode-select --install`)。

## 安装依赖

在项目根目录 `/workspace/src` 中安装前端依赖：

```bash
cd src
npm install
```

后端的 Rust 依赖会在首次运行 `cargo build` 或 `tauri dev` 时自动拉取。

## 开发模式 (Development Mode)

在开发过程中，您可以通过以下命令同时启动前端的热更新 (HMR) 服务器和后端的 Tauri 窗口：

```bash
cd src
npm run tauri:dev
```
*此命令相当于执行 `vite` 并使用 Tauri CLI 启动应用，应用会自动加载本地 `http://localhost:5173`。*

### 严格的 CSP 模式 (Strict CSP)
如果您需要测试在更严格的内容安全策略 (Content Security Policy) 下运行应用：
```bash
npm run tauri:dev:strict-csp
```

## 生产构建 (Production Build)

当开发完成，准备发布时，使用以下命令构建用于分发的独立安装包：

```bash
cd src
npm run tauri:build
```
此命令将：
1. 运行 `vite build` 编译前端资产，输出到 `src/dist` 目录。
2. 运行 `cargo build --release` 编译 Rust 后端代码。
3. 根据 `tauri.conf.json` 中配置的平台打包器（如 Windows 上的 NSIS/MSI，macOS 上的 DMG/App，Linux 上的 AppImage/DEB），生成安装程序。

构建完成后，安装包将位于 `/workspace/src-tauri/target/release/bundle/` 目录中。

## 检查与调试

1. **检查 Tauri 配置和后端代码**:
   ```bash
   cd src-tauri
   cargo check
   ```

2. **检查前端内联样式 (CSP 兼容性)**:
   ```bash
   cd src
   npm run check:csp-style
   ```

3. **前端预览 (不包含 Tauri 环境)**:
   如果您只想查看静态页面的渲染效果（注意：所有依赖 Tauri IPC 的功能将会失效报错）：
   ```bash
   cd src
   npm run dev
   ```
   并在浏览器中打开提供的本地地址（如 `http://localhost:5173/settings.html`）。

## 日志调试

正式环境默认不写入日志文件，但在开发环境中：
- 您可以通过托盘菜单选择 **“打开日志目录”** 查看详细的系统运行日志。
- 控制台也会实时输出 Tauri 后端的 `log::info`、`log::warn` 和 `log::error` 信息。
