//! 投屏本地中继：viewer 窗口（WebView）↔ 投屏会话之间的桥。
//!
//! 仅监听 127.0.0.1 随机端口，无鉴权面（会话生命周期与投屏窗口一致）。
//! - server → viewer：Binary = [flags][h264 负载]；Text = JSON 事件
//! - viewer → server：Text = JSON 控制指令（解析后编码成 scrcpy 协议字节）

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use tokio::sync::broadcast;

use super::protocol;

#[derive(Clone)]
pub enum RelayFrame {
    /// [flags][payload]
    Binary(Vec<u8>),
    /// JSON 事件文本
    Text(String),
}

#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<RelayFrame>,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self { tx }
    }

    pub fn send_binary(&self, envelope: Vec<u8>) {
        let _ = self.tx.send(RelayFrame::Binary(envelope));
    }

    pub fn send_text(&self, json: String) {
        let _ = self.tx.send(RelayFrame::Text(json));
    }
}

/// viewer 控制指令（JSON）→ scrcpy 协议字节。返回 None 表示指令无效。
fn encode_control(text: &str) -> Option<Vec<u8>> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let kind = v.get("kind")?.as_str()?;
    let pos = || -> Option<(i32, i32, u16, u16)> {
        Some((
            v.get("x")?.as_i64()? as i32,
            v.get("y")?.as_i64()? as i32,
            v.get("w")?.as_i64()? as u16,
            v.get("h")?.as_i64()? as u16,
        ))
    };
    match kind {
        "touch" => {
            let (x, y, w, h) = pos()?;
            let action = v.get("action")?.as_i64()? as u8;
            let pressure = v.get("pressure").and_then(|p| p.as_f64()).unwrap_or(1.0) as f32;
            Some(protocol::inject_touch(action, x, y, w, h, pressure))
        }
        "scroll" => {
            let (x, y, w, h) = pos()?;
            let dx = v.get("dx").and_then(|d| d.as_f64()).unwrap_or(0.0) as f32;
            let dy = v.get("dy").and_then(|d| d.as_f64()).unwrap_or(0.0) as f32;
            Some(protocol::inject_scroll(x, y, w, h, dx, dy))
        }
        "keycode" => {
            let action = v.get("action")?.as_i64()? as u8;
            let keycode = v.get("keycode")?.as_i64()? as i32;
            let repeat = v.get("repeat").and_then(|r| r.as_i64()).unwrap_or(0) as i32;
            let meta = v.get("metaState").and_then(|m| m.as_i64()).unwrap_or(0) as i32;
            Some(protocol::inject_keycode(action, keycode, repeat, meta))
        }
        "text" => Some(protocol::inject_text(v.get("text")?.as_str()?)),
        "back-or-screen-on" => {
            let action = v.get("action")?.as_i64()? as u8;
            Some(protocol::back_or_screen_on(action))
        }
        "set-clipboard" => {
            let sequence = v.get("sequence").and_then(|s| s.as_i64()).unwrap_or(0);
            let paste = v.get("paste").and_then(|p| p.as_bool()).unwrap_or(false);
            Some(protocol::set_clipboard(
                sequence,
                paste,
                v.get("text")?.as_str()?,
            ))
        }
        "rotate" => Some(protocol::rotate_device()),
        _ => None,
    }
}

/// 启动本地中继服务，返回监听端口。
pub async fn serve(hub: Hub, control_tx: std::sync::mpsc::Sender<Vec<u8>>) -> Result<u16, String> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("绑定中继端口失败: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    let app = Router::new()
        .route("/mirror", get(ws_handler))
        .with_state((hub, control_tx));
    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::warn!("投屏中继服务退出: {}", e);
        }
    });
    Ok(port)
}

async fn ws_handler(
    State((hub, control_tx)): State<(Hub, std::sync::mpsc::Sender<Vec<u8>>)>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, hub, control_tx))
}

async fn handle_socket(
    mut socket: WebSocket,
    hub: Hub,
    control_tx: std::sync::mpsc::Sender<Vec<u8>>,
) {
    let mut rx = hub.tx.subscribe();
    loop {
        tokio::select! {
            frame = rx.recv() => match frame {
                Ok(RelayFrame::Binary(data)) => {
                    if socket.send(Message::Binary(data.into())).await.is_err() {
                        break;
                    }
                }
                Ok(RelayFrame::Text(text)) => {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if let Some(bytes) = encode_control(&text) {
                        let _ = control_tx.send(bytes);
                    } else {
                        log::warn!("忽略无法解析的投屏控制指令");
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
        }
    }
}
