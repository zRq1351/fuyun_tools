//! HTTP + WebSocket 服务：配对、内容推送、历史拉取、图片下载、实时推送频道。
//!
//! 所有内容接口均需令牌认证（`Authorization: Bearer <token>` 或 `?token=`）；
//! `/api/pair` 用一次性配对码换令牌；`/api/status` 免认证（仅暴露名称/配对状态）。

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tauri::AppHandle;

use super::pairing;
use super::{lock, HistoryItem, SyncMessage, SyncState, SYNC_PROTOCOL_VERSION};
use crate::core::app_state::AppState as SharedAppState;
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

pub fn router(
    sync: Arc<SyncState>,
    app: AppHandle,
    app_state: Arc<Mutex<SharedAppState>>,
) -> Router {
    Router::new()
        .route("/api/pair", post(pair))
        .route("/api/status", get(status))
        .route("/api/clip", post(push_clip))
        .route("/api/history", get(history))
        .route("/api/image", get(image_file))
        .route("/api/ws", get(ws_handler))
        .with_state(ServerState {
            sync,
            app,
            app_state,
        })
}

// ---------- 请求/响应结构 ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairRequest {
    code: String,
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
    log::info!("同步配对成功：新设备已接入");
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
        ClipKind::Image { image_base64 } => write_clipboard_image(&app, &image_base64),
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

/// 写文本到 PC 剪贴板。设置 `is_updating_clipboard` 守卫，
/// 避免本机剪贴板轮询器把同步内容当作新复制项再次入库/广播（回环）。
fn write_clipboard_text(
    app: &AppHandle,
    app_state: &Arc<Mutex<SharedAppState>>,
    text: &str,
) -> Result<(), String> {
    {
        let mut guard = lock_arc_mutex(app_state);
        guard.is_updating_clipboard = true;
    }
    let result = crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
        app.clipboard().write_text(text).map_err(|e| e.to_string())
    });
    {
        let mut guard = lock_arc_mutex(app_state);
        guard.is_updating_clipboard = false;
    }
    result
}

/// 写图片到 PC 剪贴板（base64 → 解码 → tauri Image → 复用现有写入路径）
fn write_clipboard_image(app: &AppHandle, image_base64: &str) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image_base64)
        .map_err(|e| format!("base64 解码失败: {}", e))?;
    let decoded = image::load_from_memory(&bytes).map_err(|e| format!("图片解码失败: {}", e))?;
    let rgba = decoded.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let tauri_image = tauri::image::Image::new_owned(rgba.into_raw(), width, height);
    crate::utils::image_clipboard::ImageClipboardManager::write_clipboard_image(app, &tauri_image)
}
