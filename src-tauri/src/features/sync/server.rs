//! HTTP + WebSocket 服务：配对、内容推送、历史拉取、图片下载、实时推送频道。
//!
//! 所有内容接口均需令牌认证（`Authorization: Bearer <token>` 或 `?token=`）；
//! `/api/pair` 用一次性配对码换令牌；`/api/status` 免认证（仅暴露名称/配对状态）。

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tauri::AppHandle;

use super::pairing;
use super::{lock, HistoryItem, SyncMessage, SyncState, SYNC_PROTOCOL_VERSION};
use crate::core::app_state::AppState as SharedAppState;
use crate::core::app_state::ClipboardActivity;
use crate::sync::{lock_arc_mutex, Mutex};
use tauri_plugin_clipboard_manager::ClipboardExt;

/// axum 状态包（三个句柄打包传递）
#[derive(Clone)]
struct ServerState {
    sync: Arc<SyncState>,
    app: AppHandle,
    app_state: Arc<Mutex<SharedAppState>>,
}

type ApiResult<T> = Result<T, StatusCode>;

/// 记录最近一次请求来源 IP（排除本机回环）→ 同步状态暴露给无线投屏自动预填。
async fn record_peer_ip(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    if !addr.ip().is_loopback() {
        let ip = addr.ip().to_string();
        if lock(&state.sync.last_peer_ip).as_deref() != Some(ip.as_str()) {
            *lock(&state.sync.last_peer_ip) = Some(ip);
        }
        // 在线判定：任何来自手机的请求都刷新最近活跃时间
        *lock(&state.sync.last_peer_ms) = super::pairing::now_ms();
    }
    next.run(req).await
}

pub fn router(
    sync: Arc<SyncState>,
    app: AppHandle,
    app_state: Arc<Mutex<SharedAppState>>,
) -> Router {
    // 手机端的剪贴板图片（截图/照片）经 base64 后轻松超过 axum 默认 2MB 上限，
    // 会被直接拒成 413。放宽到 32MB：足以容纳 4K 截图/照片，同时防止恶意超大请求体。
    Router::new()
        .route("/api/pair", post(pair))
        .route("/api/status", get(status))
        .route("/api/clip", post(push_clip))
        .route("/api/history", get(history))
        .route("/api/image", get(image_file))
        .route("/api/ws", get(ws_handler))
        // 手机 App「投屏到电脑」远程入口（与剪贴板同步共用认证）
        .route("/api/mirror/start", post(mirror_start_route))
        .route("/api/mirror/authorize", post(mirror_authorize_route))
        .route("/api/mirror/status", get(mirror_status_route))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            ServerState {
                sync: Arc::clone(&sync),
                app: app.clone(),
                app_state: Arc::clone(&app_state),
            },
            record_peer_ip,
        ))
        .with_state(ServerState {
            sync,
            app,
            app_state,
        })
}

// ---------- 请求/响应结构 ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorAuthRequest {
    /// true = 允许电脑投屏；false = 撤销授权
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairRequest {
    code: String,
    /// 手机端上报的设备名（如 "Xiaomi 14"），用于设置页展示已配对设备
    #[serde(default)]
    device_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipRequest {
    #[serde(flatten)]
    kind: ClipKind,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ClipKind {
    Text { text: String },
    Image { image_base64: String },
}

#[derive(Deserialize)]
struct HistoryQuery {
    token: Option<String>,
    since: Option<i64>,
}

#[derive(Deserialize)]
struct ImageQuery {
    token: Option<String>,
    id: i64,
}

#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

// ---------- 认证辅助 ----------

fn request_token(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    let header_value = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    pairing::extract_token(header_value, query_token)
}

fn check_token(sync: &SyncState, token: &Option<String>) -> ApiResult<()> {
    let expected = lock(&sync.token);
    match (expected.as_deref(), token.as_deref()) {
        (Some(exp), Some(tok)) if pairing::constant_time_eq(exp, tok) => Ok(()),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

// ---------- 处理器 ----------

/// 配对：一次性配对码换设备令牌
async fn pair(
    State(s): State<ServerState>,
    Json(req): Json<PairRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let now = pairing::now_ms();
    let valid = {
        let code = lock(&s.sync.pairing_code);
        pairing::verify_pairing_code(code.as_ref(), &req.code, now)
    };
    if !valid {
        log::warn!("同步配对失败：配对码无效或已过期");
        return Err(StatusCode::FORBIDDEN);
    }
    let token = lock(&s.sync.token).clone();
    let token = token.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    // 配对成功即焚：防止同一码被第二台设备使用
    *lock(&s.sync.pairing_code) = None;
    // 记录设备名（手机端可能未上报，忽略即可）
    if let Some(name) = req
        .device_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        *lock(&s.sync.paired_device) = Some(name.to_string());
        if let Err(e) = super::save_paired_device(&s.app, name) {
            log::warn!("保存配对设备名称失败: {}", e);
        }
        log::info!("同步配对成功：新设备已接入（{}）", name);
    } else {
        log::info!("同步配对成功：新设备已接入");
    }
    Ok(Json(serde_json::json!({
        "token": token,
        "pcName": s.sync.pc_name,
        "protocolVersion": SYNC_PROTOCOL_VERSION,
    })))
}

/// 状态：名称 + 是否已配对（免认证，供手机端展示）
async fn status(State(s): State<ServerState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "pcName": s.sync.pc_name,
        "paired": lock(&s.sync.token).is_some(),
        "protocolVersion": SYNC_PROTOCOL_VERSION,
    }))
}

/// 手机 → PC：写入 PC 剪贴板
async fn push_clip(
    State(s): State<ServerState>,
    headers: HeaderMap,
    Json(req): Json<ClipRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let token = request_token(&headers, None);
    check_token(&s.sync, &token)?;

    // 剪贴板写入是阻塞操作，放 blocking 池避免卡住 async worker
    let app = s.app.clone();
    let app_state = s.app_state.clone();
    let result = tokio::task::spawn_blocking(move || match req.kind {
        ClipKind::Text { text } => write_clipboard_text(&app, &app_state, &text),
        ClipKind::Image { image_base64 } => write_clipboard_image(&app, &app_state, &image_base64),
    })
    .await
    .map_err(|e| {
        log::error!("同步写剪贴板任务失败: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    result.map_err(|e| {
        log::error!("同步写入剪贴板失败: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// 手机 → PC：设置投屏授权开关（扫码授权后可分别勾选同步 / 投屏）。
async fn mirror_authorize_route(
    State(s): State<ServerState>,
    headers: HeaderMap,
    Json(req): Json<MirrorAuthRequest>,
) -> Json<serde_json::Value> {
    let token = request_token(&headers, None);
    if check_token(&s.sync, &token).is_err() {
        return Json(serde_json::json!({ "ok": false, "error": "认证失败，请重新配对" }));
    }
    *lock(&s.sync.mirror_authorized) = req.enabled;
    if let Err(e) = super::save_mirror_authorized(&s.app, req.enabled) {
        log::warn!("保存投屏授权状态失败: {}", e);
    }
    log::info!("投屏授权已{}", if req.enabled { "开启" } else { "撤销" });
    Json(serde_json::json!({ "ok": true, "authorized": req.enabled }))
}

/// 手机 → PC：远程开启投屏（手机 App「投屏到电脑」入口）。
/// **不要求配对**：剪贴板同步与投屏互不干扰，未配对手机也能触发投屏；
/// 已配对时沿用令牌认证仅用于日志区分。统一返回 200 + ok 标记。
async fn mirror_start_route(
    State(s): State<ServerState>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let token = request_token(&headers, None);
    let token_ok = check_token(&s.sync, &token).is_ok();
    // 投屏需要显式授权：已配对令牌 或 手机端已打开「投屏」开关
    if !token_ok && !*lock(&s.sync.mirror_authorized) {
        log::warn!("[sync] 未授权设备请求开启投屏，已拒绝");
        return Json(
            serde_json::json!({ "ok": false, "error": "手机未授权投屏，请先在 App 中扫码授权" }),
        );
    }
    log::info!(
        "[sync] 请求开启投屏（{}）",
        if token_ok {
            "已配对手机"
        } else {
            "仅投屏授权"
        }
    );
    match crate::features::mirror::commands::mirror_start(s.app.clone(), None, None).await {
        Ok(info) => Json(serde_json::json!({
            "ok": true,
            "deviceName": info.get("deviceName"),
            "serial": info.get("serial"),
        })),
        Err(e) => {
            log::warn!("[sync] 手机远程开启投屏失败: {}", e);
            Json(serde_json::json!({ "ok": false, "error": e }))
        }
    }
}

/// 手机 → PC：查询投屏会话状态（App 主页/发现页展示用）。**不要求配对**。
async fn mirror_status_route(
    State(s): State<ServerState>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let token = request_token(&headers, None);
    let _ = check_token(&s.sync, &token); // 令牌仅用于区分来源，不阻断
    match crate::features::mirror::commands::mirror_status(s.app.clone()).await {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "authorized".to_string(),
                    serde_json::Value::Bool(*lock(&s.sync.mirror_authorized)),
                );
            }
            Json(v)
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })),
    }
}

/// 历史增量拉取（手机重连后补漏）
async fn history(
    State(s): State<ServerState>,
    headers: HeaderMap,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<Vec<HistoryItem>>> {
    let token = request_token(&headers, q.token.as_deref());
    check_token(&s.sync, &token)?;
    let since = q.since.unwrap_or(0);
    let items: Vec<HistoryItem> = {
        let history = lock(&s.sync.history);
        history
            .iter()
            .filter(|item| item.id > since)
            .cloned()
            .collect()
    };
    Ok(Json(items))
}

/// 按历史条目 id 下载图片（路径由服务端持有，防路径穿越）
async fn image_file(
    State(s): State<ServerState>,
    headers: HeaderMap,
    Query(q): Query<ImageQuery>,
) -> ApiResult<Response> {
    let token = request_token(&headers, q.token.as_deref());
    check_token(&s.sync, &token)?;
    let path = {
        let history = lock(&s.sync.history);
        history
            .iter()
            .find(|item| item.id == q.id)
            .and_then(|item| {
                if let SyncMessage::Image { path, .. } = &item.message {
                    Some(path.clone())
                } else {
                    None
                }
            })
    };
    let path = path.ok_or(StatusCode::NOT_FOUND)?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        log::warn!("读取同步图片失败: {} ({})", path, e);
        StatusCode::NOT_FOUND
    })?;
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
}

/// WS 实时推送频道
async fn ws_handler(
    State(s): State<ServerState>,
    headers: HeaderMap,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    let token = request_token(&headers, q.token.as_deref());
    check_token(&s.sync, &token)?;
    let rx = s.sync.tx.subscribe();
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, rx)))
}

async fn handle_socket(
    mut socket: WebSocket,
    mut rx: tokio::sync::broadcast::Receiver<SyncMessage>,
) {
    log::info!("同步 WS 连接已建立");
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(m) => {
                    let json = serde_json::to_string(&m).unwrap_or_default();
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("WS 推送滞后 {} 条，手机端应走 /api/history 补拉", n);
                }
                Err(_) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Ping(p))) => {
                    if socket.send(Message::Pong(p)).await.is_err() {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    log::info!("同步 WS 连接已断开");
}

// ---------- 剪贴板写入（带防回环守卫） ----------

/// RAII 防回环守卫：标记"同步写入剪贴板"活动，
/// 让本机剪贴板轮询器在写入窗口期内跳过（避免把同步内容当作新复制项再次入库/广播）。
/// panic 时也能可靠撤销，不会把守卫泄漏成永久 true。
struct SyncClipboardWriteGuard<'a>(&'a Arc<Mutex<SharedAppState>>);

impl Drop for SyncClipboardWriteGuard<'_> {
    fn drop(&mut self) {
        let mut guard = lock_arc_mutex(self.0);
        guard
            .clipboard_activities
            .remove(ClipboardActivity::SyncClipboardWrite);
    }
}

/// 写文本到 PC 剪贴板。设置 `SyncClipboardWrite` 活动守卫避免回环，
/// 并记录自写入指纹供轮询器回读时丢弃（活动位只挡写入瞬间，
/// 押后事件仍会在守卫撤销后被消费，指纹比对才是最终防线）。
///
/// 指纹丢弃的同时必须显式入库：用户期望手机发来的文字出现在电脑端的
/// 文字剪贴板卡片里（可信手再取用），只是不能再广播回手机。
fn write_clipboard_text(
    app: &AppHandle,
    app_state: &Arc<Mutex<SharedAppState>>,
    text: &str,
) -> Result<(), String> {
    {
        let mut guard = lock_arc_mutex(app_state);
        guard
            .clipboard_activities
            .insert(ClipboardActivity::SyncClipboardWrite);
    }
    let _write_guard = SyncClipboardWriteGuard(app_state);
    crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
        app.clipboard().write_text(text).map_err(|e| e.to_string())
    })?;
    lock_arc_mutex(app_state).record_self_clipboard_write();
    crate::services::clipboard_manager::add_synced_to_clipboard_history(
        app,
        text.to_string(),
        app_state.clone(),
    );
    Ok(())
}

/// 写图片到 PC 剪贴板（base64 → 解码 → tauri Image → 复用现有写入路径）。
/// 与文本写入一致：SyncClipboardWrite 活动守卫 + 自写入指纹，防止
/// 图片轮询器把手机推来的图片重复入库并广播回手机；
/// 同时显式把图片补进本机图片历史，让用户在电脑端卡片里也能看到。
fn write_clipboard_image(
    app: &AppHandle,
    app_state: &Arc<Mutex<SharedAppState>>,
    image_base64: &str,
) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image_base64)
        .map_err(|e| format!("base64 解码失败: {}", e))?;
    let decoded = image::load_from_memory(&bytes).map_err(|e| format!("图片解码失败: {}", e))?;
    let rgba = decoded.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();
    // 剪贴板写入会消费 Image，历史入库还需要同一份像素，故先克隆一份
    let tauri_image = tauri::image::Image::new_owned(raw.clone(), width, height);
    {
        let mut guard = lock_arc_mutex(app_state);
        guard
            .clipboard_activities
            .insert(ClipboardActivity::SyncClipboardWrite);
    }
    let _write_guard = SyncClipboardWriteGuard(app_state);
    crate::utils::image_clipboard::ImageClipboardManager::write_clipboard_image(app, &tauri_image)?;
    lock_arc_mutex(app_state).record_self_clipboard_write();

    // 图片轮询器会因自写入指纹丢弃这次变化，这里显式补登图片历史卡片。
    // 入库过程内部会触发 broadcast_image 钩子，必须抑制，否则刚收到的图片会被推回手机。
    let manager_arc = { lock_arc_mutex(app_state).image_clipboard_manager.clone() };
    {
        let _no_echo = crate::features::sync::ImageBroadcastSuppressGuard::new();
        lock_arc_mutex(&manager_arc).add_rgba_image_with_source_blob(raw, width, height, None);
    }
    crate::services::image_clipboard_manager::emit_image_history_payload(app, app_state.clone());
    Ok(())
}
