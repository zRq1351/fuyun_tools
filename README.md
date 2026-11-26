<div align="center">

<img src="src-tauri/icons/icon.png" alt="fuyun_tools" width="112" />

# fuyun_tools

**一套快捷键，重塑桌面效率**

把剪贴板、AI 划词、截图 OCR、屏幕录制、手机投屏、应用启动与文档管理，收进同一套常驻系统托盘的工作流。

![Version](https://img.shields.io/badge/version-0.8.53-blue?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Windows_10%20%2F%2011-0078D6?style=flat-square&logo=windows)
![License](https://img.shields.io/badge/license-GPL--2.0-green?style=flat-square)
![Tauri](https://img.shields.io/badge/Tauri-2.x-FFC131?style=flat-square&logo=tauri)
![Vue](https://img.shields.io/badge/Vue-3.x-4FC08D?style=flat-square&logo=vue.js)
![Rust](https://img.shields.io/badge/Rust-native-CE422B?style=flat-square&logo=rust)

[English](README_EN.md) · [Releases](https://github.com/zRq1351/fuyun_tools/releases) · [Gitee 镜像](https://gitee.com/zrq1351/fuyun_tools)

</div>

---

## 产品定位

**fuyun_tools** 不是功能堆砌的工具箱，而是为 Windows 打造的**键盘优先效率中枢**：

| 能力 | 入口 | 一句话 |
|------|------|--------|
| 文字剪贴板 | <kbd>Ctrl+Shift+Z</kbd> | 多层去重 · 分类置顶 · AI 解释翻译 |
| 图片剪贴板 | <kbd>Ctrl+Shift+X</kbd> | 异步缩略图 · 配额保护 · 批量导入 |
| 截图 OCR | <kbd>Ctrl+Shift+S</kbd> | 区域 / 长截图 · 专业标注 · 双引擎离线 OCR |
| 屏幕录制 | <kbd>Alt+R</kbd> | WGC / WASAPI · 多显示器 · 悬浮胶囊 |
| 手机投屏 | 设置开启 | USB / WiFi · 反向控制 · 独立投屏 · 息屏投屏 |
| 应用启动 | <kbd>Alt+Q</kbd> | 全盘扫描 · 模糊搜索 · 自定义命令 |
| 文档管理 | <kbd>Ctrl+Shift+D</kbd> | 索引 / 仓库双模式 · FTS5 · 标签治理 |

> **设计原则**：功能按需启用，默认关闭；数据全本地；快捷键可定制；未启用的入口不会干扰系统。

---

## 核心体验

### 1 · 智能剪贴板

- **三层去重**：Bloom Filter → XXH3 索引 → 模糊版本检测，自动用完整文本替换碎片
- **组织力**：分类、置顶、容量保护策略，重要条目不被清理
- **键盘流**：方向键浏览、回车回填、`Ctrl+1~9` 快选；选中后 `T` / `E` 调用 AI
- **图片库**：异步缩略图、磁盘配额、全屏预览、拖拽批量导入；有本地路径时不下发冗余预览数据

### 2 · AI 划词

```
选中文本 → 悬浮工具栏 → 翻译 / 解释 / 自定义提示词 → 流式结果 → 一键回写
```

- 全局钩子 + 线性度 / 水平偏向 / 速度多因子启发，精准识别「划词」而非普通拖拽
- 支持 DeepSeek、通义千问、小米 Mimo 及任意 OpenAI 兼容接口
- API Key **加密存于本地 SQLite**，界面仅掩码展示；SSE 流式 + Markdown 渲染

### 3 · 截图与标注

一条链路：**选区 → 标注 → 长截图 → OCR → 贴图**

| 工具 | 说明 |
|------|------|
| 几何标注 | 矩形 · 圆形 · 箭头 · 编号 · 线段 |
| 信息强调 | 荧光笔 · 文字 · 画笔 |
| 遮挡与取样 | 马赛克 · 遮挡黑条 · 取色器（HEX/RGB） |
| 几何约束 | <kbd>Shift</kbd> 正方形 / 正圆 / 水平·垂直·45° |
| 裁剪 | 「裁剪到选区」将标注烘焙为新底图，继续编辑 |
| 撤销栈 | <kbd>Ctrl+Z</kbd> / <kbd>Ctrl+Y</kbd>，最多 50 步 |

**OCR 双引擎（均离线）**

| | Windows 原生 | PaddleOCR (MNN) |
|--|-------------|-----------------|
| 速度 | ~500ms | ~1000ms |
| 精度 | 80–85% | **95–98%** |
| 手写体 | — | ✓ |

### 4 · 专业录屏

- **采集**：WGC 窗口 / 显示器硬件编码 · FFmpeg 全屏区域 · WASAPI 系统 / 麦克风 / 进程音频
- **控制**：悬浮胶囊折叠 / 展开；Push-to-Talk（<kbd>Ctrl+Space</kbd>）
- **稳健**：黑屏看门狗 · Job Object 子进程回收 · 音频预校验 · AAC 回退

### 5 · 手机投屏

基于 scrcpy 协议实现的安卓设备投屏（详见[第三方组件](#第三方组件)），画面经本地 USB 或局域网传输，**不经过任何服务器**。

**连接方式**

| 方式 | 说明 |
|------|------|
| USB | 插线即用，延迟最低（约 40–80ms） |
| USB 一键切换无线 | 自动 `adb tcpip` + 读取手机 IP + 建立无线连接，拔线继续投 |
| 手动 IP / 无线调试 | 手动输入 IP 连接；Android 11+ 支持配对码无线调试 |
| 历史设备 | 多台手机一键重连，自动记忆 |

**画质模式（设置页可切换）**

| 档位 | 分辨率 | 帧率 | 码率 | 适用 |
|------|--------|------|------|------|
| 流畅 | 长边 1280 | 30fps | 6 Mbps | 弱网 / 省流量 |
| 均衡（默认） | 长边 1600 | 30fps | 8 Mbps | 延迟与画质平衡 |
| 高清 | 长边 1920 | 60fps | 16 Mbps | 高刷顺滑，USB 首选 |
| 原生 | 手机原始分辨率 | 60fps | 24 Mbps | 接近手机原生体验 |

**反向控制与投屏增强**

- **反向控制**：鼠标点击 / 滑动 / 滚轮、键盘与中文输入（IME 组合输入支持）、剪贴板双向同步
- **一键独立投屏**：工具栏一键将手机**当前前台应用**切入独立虚拟显示器——窗口里只看这个 App，手机物理屏幕可自由使用（Android 10+）
- **息屏投屏**：一键关闭手机显示面板，渲染与投屏继续，省电不中断；停止投屏自动点亮
- **滚轮翻页**：滚轮模拟触摸快滑手势，抖音 / 红果等短视频 App 直接吸附翻页；可随时切回平滑滚动
- **文件互传**：拖拽文件到投屏窗口即推送到手机并触发媒体扫描；一键导出手机最近文件 / 图片
- **会话韧性**：视频流 / 控制链路断开自动重建；USB 切无线、设备切换全程自动恢复，无需手动重连

### 6 · 启动器与文档中枢

- **启动器**：开始菜单全量扫描、PE 签名校验、内置命令闭环到各模块、分类网格与拖拽排序
- **文档管理**：索引模式零搬迁 / 仓库模式集中托管；FTS5 全文检索；**多选批量打标**、标签全局改名合并删除、按标签筛选与摘要高亮、可重建索引；桌面小部件拖拽即用

---

## 系统集成

<div align="center">

| 系统托盘 | 开机自启 | 全局快捷键 | 主题 | 国际化 | 自动更新 |
|:-------:|:-------:|:--------:|:----:|:-----:|:-------:|
| 右键菜单 | 可选 | 全功能可定制 | 浅色 / 深色 / 护眼 | 中 / 英 | 静默检测 + 设置红点 |

</div>

- **备份与恢复**：`.fytbk.zip` + SHA-256；手动 / 定时；合并或覆盖，失败自动回滚
- **诊断中心**：健康检查、一键修复；性能监控（CPU / 内存、启动与 IPC 耗时 Top、闲置窗口回收统计）
- **设置中心**：10 个配置页，右侧 450ms 防抖自动保存

---

## 快速开始

1. 从 [Releases](https://github.com/zRq1351/fuyun_tools/releases) 下载 `.exe` 安装
2. 托盘图标 → **设置**，启用需要的功能（默认全部关闭）
3. 使用划词 / 剪贴板 AI 时，先在 **AI 设置** 中配置 OpenAI 兼容接口并「连接测试」
4. 录屏首次启用会引导按需下载 FFmpeg（不打包进安装包，减小体积）
5. 手机投屏在 **设置 → 手机同步** 中开启：USB 插线即可投；无线建议 Android 11+（配对），独立投屏需 Android 10+

**系统要求**：Windows 10/11（64 位）· 建议 4GB+ 内存 · 500MB+ 磁盘

```bash
# 本地开发
cd src && npm install
npm run tauri:dev      # 热重载
npm run tauri:build    # 生产构建
cd src-tauri && cargo check
```

---

## 技术架构

| 层级 | 选型 |
|------|------|
| 桌面壳 | **Tauri 2.x** |
| 前端 | Vue 3 · Element Plus · Vite · 16 独立 WebView |
| 后端 | **Rust** · 多线程 / 异步任务 |
| 数据 | SQLite · WAL · FTS5 · 全本地 |
| AI | async-openai（OpenAI 兼容）· Key 本地加密 |
| 图像 | image · imageproc · OpenCV（可选长截图） |
| 音视频 | WASAPI · WGC / DXGI · FFmpeg（按需） |
| OCR | Windows Media OCR · PaddleOCR / MNN（离线） |
| 手机投屏 | scrcpy-server v5.0.1（内嵌，未修改）· Android platform-tools adb |

```text
fuyun_tools/
├── src/           # Vue 3 前端（剪贴板 / 截图 / 录屏 / 投屏 / 启动器 / 文档…）
├── src-tauri/     # Rust 后端（features · services · ui · utils）
└── docs/          # 许可证与第三方声明
```

---

## 安全与隐私

| | |
|--|--|
| API Key | 本地 SQLite 加密，界面掩码 |
| OCR / 录屏 / 投屏 / 数据 | **全本地**，零上传、零收集；投屏画面仅经 USB 或局域网点对点传输 |
| 源码 | **GPL-2.0** 开源可审计 |
| 功能默认策略 | **默认关闭**，按需启用，避免无谓后台常驻 |

---

## 常见问题

<details>
<summary><b>快捷键按了没反应？</b></summary>
多数功能默认关闭。先到设置打开对应功能开关，再确认快捷键无冲突；启动注册失败会自动弹出设置页。
</details>

<details>
<summary><b>为什么 Linux / macOS 没有划词？</b></summary>
划词依赖 Windows 全局钩子（<code>WH_MOUSE_LL</code> / <code>WH_KEYBOARD_LL</code>），其他平台尚未移植。
</details>

<details>
<summary><b>录屏为何提示下载 FFmpeg？</b></summary>
为控制安装包体积，FFmpeg 按需下载（GitHub / Gitee，地址可配置）。
</details>

<details>
<summary><b>API Key 是否安全？</b></summary>
加密保存在本地 SQLite 的 AI 提供商配置中，不会明文写入配置文件。
</details>

<details>
<summary><b>投屏有哪些系统版本要求？</b></summary>
USB / WiFi 基础投屏要求 Android 7+；「独立投屏」（虚拟显示器）要求 Android 10+；无线调试配对（免 USB 首连）要求 Android 11+。
</details>

<details>
<summary><b>投屏画面会经过第三方服务器吗？</b></summary>
不会。画面数据仅通过 USB 线或同一局域网点对点传输，应用不收集、不上传任何投屏内容。
</details>

<details>
<summary><b>如何迁移数据？</b></summary>
旧机「设置 → 备份与恢复 → 立即备份」导出 <code>.fytbk.zip</code>，新机恢复导入即可。
</details>

---

## 第三方组件

本项目基于优秀的开源项目构建，感谢以下作者与社区：

- **scrcpy**（Genymobile）— 手机投屏核心：内嵌其 **scrcpy-server v5.0.1**（未修改，按原样分发），投屏协议与设备端渲染均由其提供 · Apache License 2.0 · [项目主页](https://github.com/Genymobile/scrcpy)
- **Android platform-tools（adb）** — 设备连接与调试通道，随安装包分发 · Apache License 2.0 · [AOSP](https://developer.android.com/tools/adb)
- **FFmpeg** — 外部进程（GPL/LGPL），录屏启用时按需下载
- **PaddleOCR / MNN** — Apache 2.0
- **OpenCV** — 可选编译特性（Apache 2.0）

许可证全文与分发合规说明见 [`docs/THIRD_PARTY_NOTICES.md`](docs/THIRD_PARTY_NOTICES.md)、[`docs/scrcpy_LICENSE`](docs/scrcpy_LICENSE)。

---

<div align="center">

## 获取应用

[![GitHub](https://img.shields.io/badge/GitHub-Release-white?style=for-the-badge&logo=github)](https://github.com/zRq1351/fuyun_tools/releases)
[![Gitee](https://img.shields.io/badge/Gitee-国内镜像-white?style=for-the-badge)](https://gitee.com/zrq1351/fuyun_tools)
[![Bilibili](https://img.shields.io/badge/Bilibili-演示视频-white?style=for-the-badge&logo=bilibili)](https://www.bilibili.com/video/BV1bwBSBUE8k)

**GPL-2.0** · Windows 10 / 11 · 本地优先 · 键盘驱动

</div>
