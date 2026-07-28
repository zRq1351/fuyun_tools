<div align="center">

<img src="src-tauri/icons/icon.png" alt="fuyun_tools" width="96" />

# fuyun_tools

**一套快捷键，搞定日常效率**

![Version](https://img.shields.io/badge/version-0.8.22-blue?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Windows_10/11-0078D6?style=flat-square&logo=windows)
![License](https://img.shields.io/badge/license-GPL--2.0-green?style=flat-square)
![Rust](https://img.shields.io/badge/Tauri-2.x-FFC131?style=flat-square&logo=tauri)
![Vue](https://img.shields.io/badge/Vue-3.x-4FC08D?style=flat-square&logo=vue.js)

[English](README_EN.md)

</div>

---

**fuyun_tools** 是常驻 Windows 系统托盘的桌面效率工具，将剪贴板管理、AI 划词、截图 OCR、屏幕录制、应用启动器、文档管理整合为一套快捷键驱动的工作流。

<div align="center">

| <kbd>Ctrl+Shift+Z</kbd><br>📋 文字剪贴板 | <kbd>Ctrl+Shift+X</kbd><br>🖼️ 图片剪贴板 | <kbd>Ctrl+Shift+S</kbd><br>✂️ 截图 OCR | <kbd>Alt+R</kbd><br>🎥 屏幕录制 | <kbd>Alt+Q</kbd><br>🔍 应用启动器 | <kbd>Ctrl+Shift+D</kbd><br>📁 文档管理 |
|-------------------------------------|--------------------------------------|--------------------------------------|-----------------------------|------------------------------|------------------------------------|

</div>

---

## 🚀 快速上手

### 系统要求

| 项目   | 要求                  |
|------|---------------------|
| 操作系统 | Windows 10/11（64 位） |
| 内存   | 建议 4GB 以上           |
| 磁盘   | 至少 500MB 可用空间       |

### 安装

1. 从 [GitHub Release](https://github.com/zRq1351/fuyun_tools/releases) 下载 `.exe` 安装包
2. 双击运行，按提示完成安装
3. 首次启动后图标出现在系统托盘，配置文件自动生成

### 三步开始使用

**第一步：配置 AI 服务**

进入「设置 → AI 设置」，选择提供商（DeepSeek / 通义千问 / 小米 Mimo / 自定义 OpenAI 兼容接口），填写 API
地址、模型名称和密钥，点击「连接测试」验证。

**第二步：体验划词**

在任意应用中选中文本，悬浮工具栏自动弹出，点击「翻译」或「解释」即可看到 AI 流式输出。

**第三步：探索其他功能**

| 操作       | 快捷键            |
|----------|----------------|
| 打开文字剪贴板  | `Ctrl+Shift+Z` |
| 打开图片剪贴板  | `Ctrl+Shift+X` |
| 截图 & OCR | `Ctrl+Shift+S` |
| 屏幕录制     | `Alt+R`        |
| 应用启动器    | `Alt+Q`        |
| 文档管理     | `Ctrl+Shift+D` |

---

## 📋 剪贴板管理

> 自动监听剪贴板变更，为文字和图片提供独立的历史记录与回填能力。

### 文字剪贴板

| 特性      | 实现                                    |
|---------|---------------------------------------|
| 多层去重    | Bloom filter → XXH3 哈希索引 → 模糊版本检测     |
| 智能替换    | 先复制片段再复制全文，自动替换为完整版本                  |
| 分类 & 置顶 | 自定义标签，拖拽归类；重要条目永久保留，不受容量清理影响          |
| 键盘操作    | `←` `→` 浏览、`Enter` 回填、`Ctrl+1~9` 快速选择 |
| AI 集成   | 选中条目按 `T` 翻译、`E` 解释                   |
| 容量保护    | 开启「仅限制未分组项」后，已分类和置顶条目不受数量上限影响         |

### 图片剪贴板

| 特性    | 实现               |
|-------|------------------|
| 异步缩略图 | 后台生成，长列表流畅滚动     |
| 磁盘配额  | 默认 2GB 上限，自动清理旧图 |
| 全屏预览  | 支持缩放与平移          |
| 批量导入  | 拖放或选择本地图片批量导入    |

---

## 🔤 AI 划词

> 在任意应用中选中文本，悬浮工具栏自动弹出 —— 无需离开当前窗口。

```
选中文本 → 工具栏浮现 → 翻译 / 解释 / 自定义提示词 → 流式结果展示 → 一键回写
```

### 检测机制

通过 `WH_MOUSE_LL` + `WH_KEYBOARD_LL` 全局钩子，结合 **线性度**（R² > 0.9）、**水平偏向**、**移动速度**
多因子启发式评分，精准区分「选择文字」与「普通点击拖拽」，并自动过滤误触发。

### 工具栏按钮

|         | 翻译 | 解释 | 复制 |      自定义       |
|---------|:--:|:--:|:--:|:--------------:|
| **默认**  | ✓  | ✓  | ✓  |       —        |
| **自定义** | —  | —  | —  | 可添加任意 AI 提示词按钮 |

### AI 提供商

- **内置**：DeepSeek · 通义千问 · 小米 Mimo
- **扩展**：支持任意 OpenAI 兼容接口
- **安全**：API Key 存储于 Windows 凭据管理器，永不明文落盘
- **流式**：SSE 实时推送，Markdown 渲染

---

## ✂️ 截图 & OCR

> 一条快捷键打通 区域截图 → 标注 → 长截图 → OCR 文字提取 → 贴图 全链路。

### 截图能力

| 功能    | 细节                                                       |
|-------|----------------------------------------------------------|
| 区域选择  | 拖拽选区，或点击自动检测窗口边界                                         |
| 长截图   | 滚动捕捉 + OpenCV 模板匹配/相位相关拼接（最大 20000px）                    |
| 标注工具  | 矩形 · 圆形 · 箭头 · 文字 · 画笔 · 马赛克 · 取色器                       |
| 撤销/重做 | `Ctrl+Z` / `Ctrl+Y`，最多 50 步                              |
| 贴图    | 截图可钉在屏幕最上层，右键触发 OCR，双击关闭                                 |
| 取色器   | 11×11px 区域放大 12 倍，像素级取色，`Shift` 切换 HEX/RGB，`Ctrl+C` 复制色值 |

### OCR 引擎

<table>
<tr>
<th></th>
<th>Windows 原生 OCR</th>
<th>PaddleOCR (MNN)</th>
</tr>
<tr>
<td><b>耗时</b></td>
<td>~500ms</td>
<td>~1000ms</td>
</tr>
<tr>
<td><b>精度</b></td>
<td>80-85%</td>
<td>95-98% ✨</td>
</tr>
<tr>
<td><b>联网</b></td>
<td>离线</td>
<td>离线</td>
</tr>
<tr>
<td><b>手写体</b></td>
<td>—</td>
<td>✓</td>
</tr>
<tr>
<td><b>预处理</b></td>
<td>Lanczos3 缩放 + 自适应二值化</td>
<td>MNN 推理</td>
</tr>
</table>

---

## 🎥 屏幕录制

> WASAPI 原生音频采集 + WGC 硬件加速 + FFmpeg 编码，悬浮胶囊一键控制。

### 采集矩阵

| 对象      | 技术方案                                        |
|---------|---------------------------------------------|
| 窗口      | **WGC**（Windows Graphics Capture）→ 硬件 H.264 |
| 全屏 / 区域 | **FFmpeg gdigrab** → `libx264 veryfast`     |
| 系统音频    | **WASAPI** 回环 → AAC 128kbps                 |
| 麦克风     | **WASAPI** 输入 → WAV                         |
| 进程音频    | 按应用独立回环采集                                   |

### 悬浮胶囊

```
┌──────────────────────────────────────┐
│ 🔴 00:12:35  ┃  ⏸  ⏹  ┃  ⚙  🎤  ✕  │
└──────────────────────────────────────┘
```

- **折叠态** — 38px 圆角悬浮条，红色脉冲动画指示录制中
- **展开态** — 完整面板：目标选择、音频设备、FPS/码率参数
- **麦克风 Push-to-Talk** — 录制中按住 `Ctrl+Space` 开麦，松开静音，适合临时解说

### 可靠性

| 机制         | 作用                     |
|------------|------------------------|
| 黑屏看门狗      | 4 秒无画面自动停止录制           |
| Job Object | 确保 FFmpeg 子进程随主进程退出    |
| 音频预验证      | 合并前 FFmpeg 解码检测，过滤损坏文件 |
| AAC 回退     | 流复制失败自动切换重编码           |

---

## 🔍 应用启动器

> `Alt+Q` 唤起，模糊搜索，回车即启动。

- 扫描 **开始菜单** 所有 `.lnk` 快捷方式，按文件夹自动归类
- **PE 签名验证** 区分系统应用与第三方应用
- 内置命令：`:settings` `:clipboard` `:screenshot` `:record`
- 自定义命令：运行程序 · 打开窗口 · 复制文本
- 分类网格视图 + **SortableJS 拖拽排序**
- 一键启动分类下全部应用

---

## 📁 文档管理

> 文件索引 / 仓库双模式，FTS5 全文检索，标签分类管理。支持桌面小组件常驻使用。

| 特性    | 说明                                     |
|-------|----------------------------------------|
| 索引模式  | 仅记录路径，文件留在原位                           |
| 仓库模式  | 物理移动文件到统一管理目录                          |
| 全文检索  | SQLite FTS5，覆盖标题 / 内容 / 标签 / 笔记        |
| 文件图标  | 自动提取系统关联程序图标（最高 256px），懒加载缓存，零等待      |
| 桌面小部件 | 常驻桌面悬浮窗，拖拽即导入，右键管理，紧凑贴边，独立于主窗口使用     |
| 拖拽导入  | 文件或文件夹拖入即导入，自动弹出 索引/搬迁 模式选择            |
| 导入撤销  | 支持按批次回退                                |
| 孤儿检测  | 自动发现管理目录中未被记录的文件                       |

---

## ⚙️ 系统集成

<div align="center">

| 🖥️ 系统托盘 | 🚀 开机自启 | ⌨️ 全局快捷键 | 🎨 主题切换  | 🌐 国际化 | 🔄 自动更新 |
|:--------:|:-------:|:--------:|:--------:|:------:|:-------:|
|   右键菜单   |   可选    |  全部可自定义  | 浅色/深色/护眼 | 中/英双语  |  内置检测   |

</div>

---

## 🧰 托盘菜单

右键点击系统托盘图标可访问：

| 菜单项  | 功能        |
|------|-----------|
| 开机自启 | 切换是否随系统启动 |
| 清除记录 | 清空剪贴板历史   |
| 设置   | 打开设置窗口    |
| 退出   | 退出应用      |

> 开发版本额外提供「清除日志」和「打开日志目录」选项。

---

## ⚙️ 设置页

设置窗口左侧提供 10 个标签页，右侧内容实时自动保存（450ms 防抖）：

| 标签页   | 配置内容                   |
|-------|------------------------|
| 剪贴板   | 快捷键、条目上限、容量保护策略        |
| 截图    | 截图快捷键、OCR 引擎选择         |
| 录屏    | 录屏快捷键、音频设备、FFmpeg 管理   |
| 划词    | 划词触发方式、自定义提示词、搜索引擎     |
| 启动器   | 启动器快捷键、视图模式            |
| 文档管理  | 文档管理快捷键、功能开关、桌面小部件开关       |
| AI 设置 | AI 提供商、API Key、模型、连接测试 |
| 备份与恢复 | 手动备份、自动备份计划、恢复数据       |
| 诊断    | 系统健康检查、一键修复            |
| 关于    | 版本信息、检查更新、项目链接         |

---

## 💾 备份与恢复

|        | 备份                        | 恢复            |
|--------|---------------------------|---------------|
| **方式** | 手动导出 / 定时自动（日/周/月）        | 选择备份包恢复       |
| **格式** | `.fytbk.zip` + SHA-256 校验 | 合并（仅新增）或覆盖    |
| **安全** | —                         | 自动创建回滚点，失败可回退 |
| **内容** | 剪贴板历史 · 图片历史 · 分类 · 设置    | 同左            |

---

## 🏗️ 技术架构

| 层    | 技术选型                                |
|:-----|:------------------------------------|
| 桌面框架 | **Tauri 2.x**                       |
| 前端   | Vue 3 · Element Plus · Vite         |
| 后端   | Rust                                |
| 数据库  | SQLite · WAL · FTS5                 |
| AI   | async-openai（OpenAI 兼容）             |
| 图像   | image · imageproc · OpenCV（可选）      |
| 音频   | WASAPI（cpal / wasapi）               |
| 录屏   | WGC · DXGI · FFmpeg                 |
| OCR  | Windows Media OCR · PaddleOCR / MNN |
| 安全   | Windows Credential Manager          |

```bash
fuyun_tools/
├── src/                     # Vue 3 前端（16 个独立 Webview 窗口）
│   ├── pages/
│   │   ├── clipboard/       # 文字剪贴板
│   │   ├── image_clipboard/ # 图片剪贴板
│   │   ├── selection_toolbar/  # AI 划词工具栏
│   │   ├── result_display/  # AI 结果展示
│   │   ├── screenshot/      # 截图编辑器
│   │   ├── longshot_toolbar/   # 长截图控制
│   │   ├── recording_toolbar/  # 录屏胶囊
│   │   ├── launcher/        # 应用启动器
│   │   ├── document_manager/   # 文档管理
│   │   ├── document_manager_widget/  # 文档管理桌面小部件
│   │   ├── settings/        # 设置
│   │   └── ...
│   └── services/            # IPC 通信层
├── src-tauri/               # Rust 后端
│   └── src/
│       ├── core/            # 应用状态 · 配置 · 错误处理
│       ├── features/        # 系统级功能（钩子 · 录屏 · 截图 · 选词）
│       ├── services/        # 业务逻辑（剪贴板 · AI · OCR · 启动器）
│       ├── ui/              # 命令路由 · 窗口管理 · 托盘菜单
│       └── utils/           # 数据库 · 备份 · 设置模型
└── docs/                    # 许可证与第三方声明
```

---

## ❓ 常见问题

<details>
<summary><b>为什么 Linux / macOS 没有划词功能？</b></summary>

当前版本的划词链路基于 Windows 全局钩子（`WH_MOUSE_LL` / `WH_KEYBOARD_LL`），Linux/macOS 暂不支持。

</details>

<details>
<summary><b>如何删除自定义 AI 提供商？</b></summary>

在 AI 提供商下拉框中，自定义项右侧点击 `✕` 即可删除。内置提供商不可删除。

</details>

<details>
<summary><b>启用录屏时为什么提示下载 FFmpeg？</b></summary>

为减小安装包体积，FFmpeg 不内置分发。首次启用录屏时自动检测，缺失则引导按需下载（可从 GitHub / Gitee 获取，下载地址可自定义）。

</details>

<details>
<summary><b>录制时如何快速开关麦克风？</b></summary>

两种方式：① 点击录屏胶囊上的麦克风图标；② 使用 `Ctrl+Space`（按住说话，松开静音）。快捷键可在设置中自定义。

</details>

<details>
<summary><b>剪贴板历史会丢失吗？</b></summary>

不会。文字/图片历史均持久化到本地 SQLite 数据库，重启后完整保留。重要条目建议置顶或分类，配合「容量保护策略」可永久保留。此外支持定时自动备份，可随时恢复。

</details>

<details>
<summary><b>API Key 安全吗？</b></summary>

非常安全。API Key 存储在 Windows 凭据管理器（keyring）中加密保存，不会明文写入任何配置文件，界面仅显示掩码。

</details>

<details>
<summary><b>快捷键冲突了怎么办？</b></summary>

所有全局快捷键均可在「设置」中自定义。如果启动时检测到快捷键注册失败，会自动弹出设置窗口并提示冲突信息。

</details>

<details>
<summary><b>如何迁移数据到新电脑？</b></summary>

在旧电脑上使用「设置 → 备份与恢复 → 立即备份」导出 `.fytbk.zip` 文件，在新电脑上通过「恢复数据」导入即可。

</details>

---

## 🛠️ 本地开发

<details>
<summary><b>环境要求</b></summary>

- **Node.js** 18+
- **Rust** 工具链（`rustup`）
- **Windows 10/11 SDK**
- （可选）**OpenCV** — 启用长截图编译特性

</details>

```bash
# 安装前端依赖
cd src && npm install

# Tauri 开发模式（热重载）
npm run tauri:dev

# 生产构建
npm run tauri:build

# 仅检查 Rust 编译
cd src-tauri && cargo check
```

---

## 🔒 安全与隐私

| 方面         | 策略                     |
|------------|------------------------|
| 🔑 API Key | Windows 凭据管理器加密，界面掩码显示 |
| 📷 OCR     | 完全离线，不上传图片             |
| 💾 数据      | 全本地存储，零数据收集            |
| 📖 代码      | GPL 开源，可审计             |

---

## 📄 第三方许可

- **FFmpeg** — 外部进程调用（GPL/LGPL），首次启用录屏时按需下载
- **PaddleOCR** — PP-OCRv5 模型（Apache 2.0）
- **MNN** — 推理引擎（Apache 2.0）
- **OpenCV** — 可选编译特性（Apache 2.0）

详见 [`docs/THIRD_PARTY_NOTICES.md`](docs/THIRD_PARTY_NOTICES.md)

---

<div align="center">

## 📥 下载

| [![GitHub](https://img.shields.io/badge/GitHub-Release-blue?style=for-the-badge&logo=github)](https://github.com/zRq1351/fuyun_tools/releases) | [![Gitee](https://img.shields.io/badge/Gitee-国内镜像-C71D23?style=for-the-badge)](https://gitee.com/zrq1351/fuyun_tools) |
|------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------|

**GPL-2.0** · [演示视频](https://www.bilibili.com/video/BV1bwBSBUE8k)

</div>
