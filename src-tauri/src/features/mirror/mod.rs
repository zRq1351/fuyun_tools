//! 手机投屏（内嵌 scrcpy 内核，QtScrcpy 模式）。
//!
//! 形态：
//! - 随包分发 `adb.exe` 与 `scrcpy-server`（Apache-2.0，见 docs/scrcpy_LICENSE）
//! - Rust 侧为轻客户端：adb 反向隧道 + 解析视频流 + 转发控制指令
//! - 投屏窗口用 WebView2 WebCodecs 硬解渲染，控制事件经本地 WS 中继回传设备
//!
//! 模块划分：
//! - `protocol`：scrcpy v5 协议字节级编解码
//! - `adb`：adb 命令封装
//! - `session`：会话生命周期（握手/收流/控制通道）
//! - `relay`：本地 WS 中继（viewer 窗口 ↔ 会话）
//! - `window`：投屏窗口创建与显示
//! - `commands`：Tauri 命令（mirror_start/stop/status）

pub mod adb;
pub mod commands;
pub mod protocol;
pub mod relay;
pub mod session;
pub mod window;

/// 随包 scrcpy-server 版本（server 与客户端版本必须严格一致）
pub const SCRCPY_VERSION: &str = session::SCRCPY_VERSION;

/// 应用退出时的清理入口。
pub fn shutdown(app: &tauri::AppHandle) {
    commands::shutdown(app);
}
