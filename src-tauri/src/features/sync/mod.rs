//! 局域网剪贴板同步：PC 内嵌服务 + 手机 App 直连，无云、无服务器成本。
//!
//! 架构：
//! 1. PC 启动同步服务 → mDNS 广播 `_fuyun._tcp`（手机自动发现）+ 生成 6 位配对码
//! 2. 手机输入配对码 → `POST /api/pair` 换取设备令牌
//! 3. 双向同步：
//!    - 手机 → PC：`POST /api/clip`（文本/图片）写入 PC 剪贴板（带 `SyncClipboardWrite` 活动守卫防回环）
//!    - PC → 手机：剪贴板新内容经 `broadcast_text` / `broadcast_image` 钩子推给 WS 连接
//!
//! 模块划分：pairing（纯配对逻辑）/ server（HTTP+WS）/ discovery（mDNS）/ commands（Tauri 命令）

pub mod commands;
pub mod discovery;
pub mod pairing;
pub mod server;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::{broadcast, oneshot};

use crate::core::app_state::AppState as SharedAppState;
use crate::sync::Mutex;

/// 锁定本模块的普通（非 Arc）Mutex 字段
pub(crate) fn lock<T>(m: &Mutex<T>) -> parking_lot::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|never| match never {})
}

/// 默认同步端口（被占用时向后尝试）
pub const SYNC_DEFAULT_PORT: u16 = 45817;
/// 端口尝试次数
const PORT_ATTEMPTS: u16 = 10;
/// 内存中保留的最近同步条数（手机断线重连后按 since 拉取）
const HISTORY_CAP: usize = 200;
/// WS 广播频道容量
const BROADCAST_CAPACITY: usize = 64;
/// 同步服务协议版本（手机端兼容性判断）
pub const SYNC_PROTOCOL_VERSION: &str = "1";

/// 一条同步消息（文本或图片）
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SyncMessage {
    Text {
        text: String,
    },
    Image {
        /// PC 侧图片文件绝对路径（手机经 /api/image 下载）
        path: String,
        width: u32,
        height: u32,
    },
}

/// 带序号的历史条目（序号即时间戳毫秒，手机按 since 增量拉取）
#[derive(Clone, Debug, Serialize)]
pub struct HistoryItem {
    pub id: i64,
    #[serde(flatten)]
    pub message: SyncMessage,
}

/// 同步服务共享状态（进程级单例）
pub struct SyncState {
    running: AtomicBool,
    port: Mutex<u16>,
    pairing_code: Mutex<Option<pairing::PairingCode>>,
    token: Mutex<Option<String>>,
    tx: broadcast::Sender<SyncMessage>,
    history: Mutex<Vec<HistoryItem>>,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    last_error: Mutex<Option<String>>,
    pc_name: String,
}

static SYNC: OnceLock<Arc<SyncState>> = OnceLock::new();

fn shared() -> &'static Arc<SyncState> {
    SYNC.get_or_init(|| {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
        Arc::new(SyncState {
            running: AtomicBool::new(false),
            port: Mutex::new(SYNC_DEFAULT_PORT),
            pairing_code: Mutex::new(None),
            token: Mutex::new(None),
            tx,
            history: Mutex::new(Vec::new()),
            shutdown_tx: Mutex::new(None),
            last_error: Mutex::new(None),
            pc_name: detect_pc_name(),
        })
    })
}

/// 电脑名称（优先 Windows 环境变量，回退固定值）
fn detect_pc_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "我的电脑".to_string())
}

/// 推入一条历史并广播（服务未启动时为 no-op，钩子调用零成本）
fn push_and_broadcast(msg: SyncMessage) {
    let state = shared();
    if !state.running.load(Ordering::SeqCst) {
        return;
    }
    let id = pairing::now_ms();
    {
        let mut history = lock(&state.history);
        history.insert(
            0,
            HistoryItem {
                id,
                message: msg.clone(),
            },
        );
        history.truncate(HISTORY_CAP);
    }
    let _ = state.tx.send(msg);
}

/// 钩子：新剪贴板文本（由 clipboard_manager 入库成功后调用）
pub fn broadcast_text(text: &str) {
    if text.is_empty() {
        return;
    }
    push_and_broadcast(SyncMessage::Text {
        text: text.to_string(),
    });
}

/// 图片广播抑制计数。
///
/// 手机推来的图片要进本机图片历史，而 `ImageClipboardManager::add_rgba_image_with_source_blob`
/// 内部会调用 [`broadcast_image`] 把新图片推给对端——若不抑制，刚收到的图片会被原样推回手机。
static IMAGE_BROADCAST_SUPPRESS: AtomicU32 = AtomicU32::new(0);

/// RAII 守卫：作用域内抑制图片广播，panic 也能恢复计数
pub struct ImageBroadcastSuppressGuard;

impl ImageBroadcastSuppressGuard {
    pub fn new() -> Self {
        IMAGE_BROADCAST_SUPPRESS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Default for ImageBroadcastSuppressGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ImageBroadcastSuppressGuard {
    fn drop(&mut self) {
        IMAGE_BROADCAST_SUPPRESS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 钩子：新剪贴板图片（由 image_clipboard 入库成功后调用）
pub fn broadcast_image(path: &str, width: u32, height: u32) {
    if path.is_empty() {
        return;
    }
    if IMAGE_BROADCAST_SUPPRESS.load(Ordering::SeqCst) > 0 {
        log::debug!("图片广播已被抑制（来自手机同步，不回传对端）");
        return;
    }
    push_and_broadcast(SyncMessage::Image {
        path: path.to_string(),
        width,
        height,
    });
}

/// 启动同步服务。返回状态快照；已在运行则直接返回当前状态。
pub fn start(app: &AppHandle, app_state: Arc<Mutex<SharedAppState>>) -> Result<SyncStatus, String> {
    let state = shared();
    if state.running.load(Ordering::SeqCst) {
        return Ok(status_snapshot(state));
    }

    // 1. 绑定端口（默认端口被占用则向后尝试）
    let listener = {
        let mut last_err = None;
        let mut bound = None;
        for offset in 0..PORT_ATTEMPTS {
            let port = SYNC_DEFAULT_PORT + offset;
            match std::net::TcpListener::bind(("0.0.0.0", port)) {
                Ok(l) => {
                    bound = Some((l, port));
                    break;
                }
                Err(e) => last_err = Some(e),
            }
        }
        match bound {
            Some(v) => v,
            None => {
                return Err(format!(
                    "无法绑定同步端口 {}-{}: {}",
                    SYNC_DEFAULT_PORT,
                    SYNC_DEFAULT_PORT + PORT_ATTEMPTS - 1,
                    last_err
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "未知错误".to_string())
                ))
            }
        }
    };
    let (std_listener, port) = listener;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;
    *lock(&state.port) = port;

    // 2. 令牌：从磁盘恢复或新建
    let token = load_or_create_token(app)?;
    *lock(&state.token) = Some(token);

    // 3. 生成配对码
    *lock(&state.pairing_code) = Some(pairing::generate_pairing_code());
    *lock(&state.last_error) = None;

    // 4. mDNS 广播（失败不阻断服务，手机仍可手动输 IP）
    let mdns = discovery::advertise(port, &state.pc_name);
    if mdns.is_none() {
        log::warn!("mDNS 广播启动失败，手机可通过手动输入 IP 连接");
    }

    // 5. 启动 HTTP/WS 服务
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let tokio_listener =
        tokio::net::TcpListener::from_std(std_listener).map_err(|e| e.to_string())?;
    let router = server::router(Arc::clone(state), app.clone(), app_state);
    tauri::async_runtime::spawn(async move {
        let server = axum::serve(tokio_listener, router).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = server.await {
            log::error!("同步服务退出: {}", e);
        }
        // mdns 守护对象随本 future 结束而释放，自动注销广播
        drop(mdns);
    });
    *lock(&state.shutdown_tx) = Some(shutdown_tx);

    state.running.store(true, Ordering::SeqCst);
    // 记录"启用中"标记：应用下次启动时据此自动恢复服务（stop 时删除）
    if let Err(e) = set_enabled_flag(app, true) {
        log::warn!("写入同步启用标记失败（下次启动不会自动恢复）: {}", e);
    }
    log::info!("同步服务已启动: port={}, name={}", port, state.pc_name);
    Ok(status_snapshot(state))
}

/// 停止同步服务
pub fn stop(app: &AppHandle) {
    let state = shared();
    if !state.running.load(Ordering::SeqCst) {
        return;
    }
    if let Some(tx) = lock(&state.shutdown_tx).take() {
        let _ = tx.send(());
    }
    *lock(&state.pairing_code) = None;
    state.running.store(false, Ordering::SeqCst);
    // 用户主动停止 → 清除启用标记，下次启动不再自动恢复
    if let Err(e) = set_enabled_flag(app, false) {
        log::warn!("清除同步启用标记失败: {}", e);
    }
    log::info!("同步服务已停止");
}

/// 应用启动时自动恢复同步服务（仅当上次退出前处于启用状态）。
///
/// 此前服务只在前端设置页 mounted 时恢复——用户不打开设置页就等于没开，
/// 是"电脑端看起来开了、手机却连不上"的常见诱因。
/// `start()` 内部 `TcpListener::from_std` 需要 Tokio reactor，故在 async 任务中执行。
pub fn auto_start_if_enabled(app: &AppHandle, app_state: Arc<Mutex<SharedAppState>>) {
    let app = app.clone();
    let enabled = enabled_flag_path(&app).map(|p| p.exists()).unwrap_or(false);
    if !enabled {
        log::info!("同步服务上次为关闭状态，跳过自动恢复");
        return;
    }
    tauri::async_runtime::spawn(async move {
        match start(&app, app_state) {
            Ok(s) => log::info!("同步服务已随应用启动自动恢复: port={}", s.port),
            Err(e) => log::error!("同步服务自动恢复失败: {}", e),
        }
    });
}

/// 吊销当前令牌并生成新配对码（手机需重新配对）。
/// 吊销后立即轮换新令牌并落盘：否则 `pair` 取不到令牌恒返回 500，
/// 且服务运行中 `start()` 会早退，用户将无法重新配对。
pub fn unpair(app: &AppHandle) -> Result<(), String> {
    let state = shared();
    let path = token_file_path(app)?;
    let new_token = pairing::generate_token();
    std::fs::write(&path, &new_token).map_err(|e| format!("保存同步令牌失败: {}", e))?;
    *lock(&state.token) = Some(new_token);
    *lock(&state.pairing_code) = Some(pairing::generate_pairing_code());
    log::info!("同步设备已吊销，令牌已轮换，等待重新配对");
    Ok(())
}

/// 重新生成配对码
pub fn regenerate_pairing_code() -> String {
    let state = shared();
    let code = pairing::generate_pairing_code();
    *lock(&state.pairing_code) = Some(pairing::PairingCode {
        code: code.code.clone(),
        expires_at_ms: code.expires_at_ms,
    });
    code.code
}

/// 状态快照（给 FE 面板和命令层）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub running: bool,
    pub port: u16,
    /// 本机局域网 IP（手机手动连接用）
    pub local_ip: Option<String>,
    pub pc_name: String,
    pub paired: bool,
    pub pairing_code: Option<String>,
    pub pairing_expires_in_secs: i64,
    pub protocol_version: &'static str,
    pub last_error: Option<String>,
}

pub fn status() -> SyncStatus {
    status_snapshot(shared())
}

fn status_snapshot(state: &SyncState) -> SyncStatus {
    let now = pairing::now_ms();
    let running = state.running.load(Ordering::SeqCst);
    // 运行中且配对码缺失/过期 → 自动续期。
    // 面板每 2s 轮询本快照，保证用户看到的码始终可用——此前码只在启动时生成
    // 一次，5 分钟过期后配对恒 403（2026-10-26 06:15 日志证实为手机配对失败主因）。
    let mut code = lock(&state.pairing_code);
    if running && code.as_ref().is_none_or(|c| !c.is_valid(now)) {
        *code = Some(pairing::generate_pairing_code());
    }
    SyncStatus {
        running,
        port: *lock(&state.port),
        local_ip: discovery::lan_ip().map(|ip| ip.to_string()),
        pc_name: state.pc_name.clone(),
        paired: lock(&state.token).is_some(),
        pairing_code: code.as_ref().map(|c| c.code.clone()),
        pairing_expires_in_secs: code.as_ref().map(|c| c.remaining_secs(now)).unwrap_or(0),
        protocol_version: SYNC_PROTOCOL_VERSION,
        last_error: lock(&state.last_error).clone(),
    }
}

// ---------- 令牌与启用标记持久化 ----------

fn app_data_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建应用数据目录失败: {}", e))?;
    Ok(dir)
}

fn token_file_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app_data_dir(app)?.join("sync_token.txt"))
}

/// 启用标记文件路径（存在 = 上次退出前同步处于启用状态）
fn enabled_flag_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app_data_dir(app)?.join("sync_enabled.flag"))
}

/// 写入/删除启用标记（best-effort，失败只记日志不阻断服务启停）
fn set_enabled_flag(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let path = enabled_flag_path(app)?;
    if enabled {
        std::fs::write(&path, b"1").map_err(|e| format!("写入同步启用标记失败: {}", e))
    } else if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("删除同步启用标记失败: {}", e))
    } else {
        Ok(())
    }
}

fn load_or_create_token(app: &AppHandle) -> Result<String, String> {
    let path = token_file_path(app)?;
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            log::info!("同步令牌已从磁盘恢复");
            return Ok(trimmed.to_string());
        }
    }
    let token = pairing::generate_token();
    std::fs::write(&path, &token).map_err(|e| format!("保存同步令牌失败: {}", e))?;
    log::info!("同步令牌已生成并保存");
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造独立 SyncState（不影响全局 shared()）
    fn test_state() -> Arc<SyncState> {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
        Arc::new(SyncState {
            running: AtomicBool::new(false),
            port: Mutex::new(SYNC_DEFAULT_PORT),
            pairing_code: Mutex::new(None),
            token: Mutex::new(None),
            tx,
            history: Mutex::new(Vec::new()),
            shutdown_tx: Mutex::new(None),
            last_error: Mutex::new(None),
            pc_name: "test-pc".to_string(),
        })
    }

    /// 回归测试（2026-10-26 闪退事故）：`start_sync` 原为同步 Tauri 命令，
    /// 在主线程调用 `tokio::net::TcpListener::from_std` 时因无 Tokio reactor
    /// 而 panic（"there is no reactor running"），全局 panic 钩子记录后进程
    /// 直接退出——用户点「启动同步服务」即闪退。
    /// 修复：命令改为 async，在 Tauri async runtime（tokio）上执行。
    /// 本测试锁定核心约束：from_std 在 runtime 上下文内可用。
    #[test]
    fn from_std_listener_works_inside_tauri_async_runtime() {
        tauri::async_runtime::block_on(async {
            let std_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            std_listener.set_nonblocking(true).expect("set_nonblocking");
            let _tokio_listener =
                tokio::net::TcpListener::from_std(std_listener).expect("from_std in runtime ctx");
        });
    }

    /// 回归测试（2026-10-26 配对失败事故）：配对码只在服务启动时生成一次，
    /// 5 分钟过期后配对恒 403——日志证实手机已到达 /api/pair 但配对失败。
    /// 修复：status_snapshot 在运行中检测到码缺失/过期时自动续期。
    #[test]
    fn status_snapshot_renews_expired_pairing_code_while_running() {
        let state = test_state();
        let now = pairing::now_ms();

        // 未运行：不生成码
        let snap = status_snapshot(&state);
        assert!(!snap.running);
        assert!(snap.pairing_code.is_none());

        // 运行中且码缺失 → 快照生成新码
        state.running.store(true, Ordering::SeqCst);
        let snap = status_snapshot(&state);
        assert!(snap.pairing_code.is_some());
        assert!(snap.pairing_expires_in_secs > 0);

        // 运行中且码已过期 → 快照自动续期（旧码被替换）
        *lock(&state.pairing_code) = Some(pairing::PairingCode {
            code: "000000".to_string(),
            expires_at_ms: now - 1,
        });
        let snap = status_snapshot(&state);
        assert_ne!(snap.pairing_code.as_deref(), Some("000000"));
        assert!(snap.pairing_expires_in_secs > 0);

        // 运行中且码有效 → 不刷新（保持同一码，避免用户正在输入时换码）
        let fresh = pairing::generate_pairing_code();
        let fresh_code = fresh.code.clone();
        *lock(&state.pairing_code) = Some(fresh);
        let snap = status_snapshot(&state);
        assert_eq!(snap.pairing_code.as_deref(), Some(fresh_code.as_str()));
    }
}
