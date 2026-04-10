# 05 - 项目运行与构建

## 1. 环境准备

建议在 Windows 环境开发（当前主功能链路以 Windows 为准）。

必备工具：

1. Node.js（建议 LTS，18+）。
2. npm。
3. Rust 工具链（`rustup` + stable）。
4. Visual Studio C++ Build Tools + Windows SDK（用于 Tauri/Rust 原生依赖编译）。

## 2. 安装依赖

在仓库根目录执行：

```bash
cd src
npm install
```

Rust 依赖会在首次执行 `tauri dev` / `cargo check` / `cargo build` 时自动拉取。

## 3. 常用开发命令

在 `src` 目录执行：

```bash
npm run dev
```

- 仅启动 Vite 前端开发服务器（不包含 Tauri 容器）。

```bash
npm run tauri:dev
```

- 启动完整桌面应用开发模式（前端 HMR + Rust 后端）。


## 4. 构建命令

在 `src` 目录执行：

```bash
npm run build
```

- 仅构建前端静态资源，输出 `src/dist`。

```bash
npm run tauri:build
```

- 构建 Tauri 发布包（包含前端构建 + Rust release 编译 + 打包）。


产物目录（Windows）通常位于：

- `src-tauri/target/release/bundle/`

## 5. 检查与排障

### 5.1 后端静态检查

```bash
cd src-tauri
cargo check
```

### 5.2 前端 CSP 检查

```bash
cd src
npm run check:csp-style
```

### 5.3 仅前端页面调试

```bash
cd src
npm run dev
```

然后访问如 `http://localhost:5173/settings.html`。  
注意：直接浏览器模式下，依赖 Tauri IPC 的功能会报错。

## 6. 录屏功能额外说明

- 录屏启用前会检查 `ffmpeg.exe` 是否存在。
- 缺失时会触发按需下载流程（下载地址来自设置项 `recording_ffmpeg_download_url`）。
- 录屏输出目录默认可配置，若未配置则使用程序目录下 `recordings`。
