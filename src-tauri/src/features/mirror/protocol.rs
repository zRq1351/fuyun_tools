//! scrcpy 控制协议（v5.0.1）字节级编解码。
//!
//! 布局依据 scrcpy v5.0.1 源码：
//! - server/src/main/java/com/genymobile/scrcpy/control/ControlMessageReader.java
//! - app/src/control_msg.c（序列化参考实现）
//!
//! 所有多字节整数均为大端。

/// 注入按键
pub const MSG_INJECT_KEYCODE: u8 = 0;
/// 注入文本
pub const MSG_INJECT_TEXT: u8 = 1;
/// 注入触摸/鼠标
pub const MSG_INJECT_TOUCH_EVENT: u8 = 2;
/// 注入滚轮
pub const MSG_INJECT_SCROLL_EVENT: u8 = 3;
/// 返回键或点亮屏幕
pub const MSG_BACK_OR_SCREEN_ON: u8 = 4;
/// 设置剪贴板
pub const MSG_SET_CLIPBOARD: u8 = 9;
/// 旋转设备屏幕
pub const MSG_ROTATE_DEVICE: u8 = 11;

/// MotionEvent.ACTION_DOWN
pub const ACTION_DOWN: u8 = 0;
/// MotionEvent.ACTION_UP
pub const ACTION_UP: u8 = 1;
/// MotionEvent.ACTION_MOVE
pub const ACTION_MOVE: u8 = 2;

/// scrcpy 客户端为鼠标事件预留的 pointerId（server 据此区分 TOOL_TYPE_MOUSE/FINGER）。
/// 我们模拟手指，使用 0 号手指。
pub const POINTER_ID_FINGER0: i64 = 0;

fn put_i16(buf: &mut Vec<u8>, v: i16) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_i32(buf: &mut Vec<u8>, v: i32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_i64(buf: &mut Vec<u8>, v: i64) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    put_i32(buf, bytes.len() as i32);
    buf.extend_from_slice(bytes);
}

/// 压力浮点 → u16 定点：0xFFFF 表示 1.0，其余 value/65536（server 端 u16FixedPointToFloat）。
fn pressure_to_u16(pressure: f32) -> u16 {
    if pressure >= 1.0 {
        return 0xFFFF;
    }
    let v = (pressure * 65536.0).round() as i64;
    v.clamp(0, 0xFFFE) as u16
}

/// 滚轮格数 → i16 定点。
/// server 端解码：`i16FixedPointToFloat(v) * 16`，即实际格数 = v / 32768 * 16。
/// 编码：v = clamp(f / 16, -1, 1) * 32768（与 app/src/control_msg.c 一致，1.0 特判 0x7FFF）。
fn scroll_to_i16(cells: f32) -> i16 {
    let norm = (cells / 16.0).clamp(-1.0, 1.0);
    if norm >= 1.0 {
        return 0x7FFF;
    }
    (norm * 32768.0) as i16
}

fn put_position(buf: &mut Vec<u8>, x: i32, y: i32, screen_w: u16, screen_h: u16) {
    put_i32(buf, x);
    put_i32(buf, y);
    put_u16(buf, screen_w);
    put_u16(buf, screen_h);
}

/// 注入触摸事件（模拟手指）。
/// `action`: ACTION_DOWN/UP/MOVE；坐标为设备像素坐标，`screen_w/h` 为当前视频分辨率。
pub fn inject_touch(
    action: u8,
    x: i32,
    y: i32,
    screen_w: u16,
    screen_h: u16,
    pressure: f32,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32);
    buf.push(MSG_INJECT_TOUCH_EVENT);
    buf.push(action);
    put_i64(&mut buf, POINTER_ID_FINGER0);
    put_position(&mut buf, x, y, screen_w, screen_h);
    put_i16(&mut buf, pressure_to_u16(pressure) as i16);
    put_i32(&mut buf, 0); // actionButton
    put_i32(&mut buf, 0); // buttons（触摸事件必须为 0）
    buf
}

/// 注入滚轮事件。
/// `h_cells`/`v_cells`：滚动格数，正负号决定方向（范围 [-16, 16]）。
pub fn inject_scroll(
    x: i32,
    y: i32,
    screen_w: u16,
    screen_h: u16,
    h_cells: f32,
    v_cells: f32,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(24);
    buf.push(MSG_INJECT_SCROLL_EVENT);
    put_position(&mut buf, x, y, screen_w, screen_h);
    put_i16(&mut buf, scroll_to_i16(h_cells));
    put_i16(&mut buf, scroll_to_i16(v_cells));
    put_i32(&mut buf, 0); // buttons
    buf
}

/// 注入按键（Android keycode）。
pub fn inject_keycode(action: u8, keycode: i32, repeat: i32, meta_state: i32) -> Vec<u8> {
    let mut buf = vec![MSG_INJECT_KEYCODE, action];
    put_i32(&mut buf, keycode);
    put_i32(&mut buf, repeat);
    put_i32(&mut buf, meta_state);
    buf
}

/// 注入文本（server 端逐字符转 KeyEvent）。
pub fn inject_text(text: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5 + text.len());
    buf.push(MSG_INJECT_TEXT);
    put_string(&mut buf, text);
    buf
}

/// 返回键或点亮屏幕（action: ACTION_DOWN/UP）。
pub fn back_or_screen_on(action: u8) -> Vec<u8> {
    vec![MSG_BACK_OR_SCREEN_ON, action]
}

/// 设置设备剪贴板（可同时粘贴）。
pub fn set_clipboard(sequence: i64, paste: bool, text: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(14 + text.len());
    buf.push(MSG_SET_CLIPBOARD);
    put_i64(&mut buf, sequence);
    buf.push(if paste { 1 } else { 0 });
    put_string(&mut buf, text);
    buf
}

/// 旋转设备屏幕。
pub fn rotate_device() -> Vec<u8> {
    vec![MSG_ROTATE_DEVICE]
}

/// 视频帧头（12 字节）解析结果。
pub enum VideoHeader {
    /// session 重置（设备旋转/分辨率变化）：新宽高 + 是否客户端缩放触发
    Session { width: u32, height: u32 },
    /// 媒体包：pts（微秒）、是否配置包（SPS/PPS）、是否关键帧、负载长度
    Media { pts: i64, config: bool, key_frame: bool, size: u32 },
}

/// 解析 12 字节帧头。首字节 MSB=1 为 session 包（旋转），否则为媒体包。
pub fn parse_video_header(header: &[u8; 12]) -> VideoHeader {
    let pts_and_flags = i64::from_be_bytes(header[0..8].try_into().unwrap());
    let size = u32::from_be_bytes(header[8..12].try_into().unwrap());
    const FLAG_SESSION: i64 = 1 << 63;
    const FLAG_CONFIG: i64 = 1 << 62;
    const FLAG_KEY_FRAME: i64 = 1 << 61;

    if pts_and_flags & FLAG_SESSION != 0 {
        // session 包：byte0 低 1 位是 isClientResize，byte4~7 宽，byte8~11 高
        let width = u32::from_be_bytes(header[4..8].try_into().unwrap());
        let height = u32::from_be_bytes(header[8..12].try_into().unwrap());
        VideoHeader::Session { width, height }
    } else {
        let pts = pts_and_flags & !(FLAG_SESSION | FLAG_CONFIG | FLAG_KEY_FRAME);
        VideoHeader::Media {
            pts,
            config: pts_and_flags & FLAG_CONFIG != 0,
            key_frame: pts_and_flags & FLAG_KEY_FRAME != 0,
            size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_touch_layout() {
        let buf = inject_touch(ACTION_DOWN, 100, 200, 718, 1600, 1.0);
        assert_eq!(buf[0], MSG_INJECT_TOUCH_EVENT);
        assert_eq!(buf[1], ACTION_DOWN);
        assert_eq!(&buf[2..10], &0i64.to_be_bytes()); // pointerId
        assert_eq!(&buf[10..14], &100i32.to_be_bytes());
        assert_eq!(&buf[14..18], &200i32.to_be_bytes());
        assert_eq!(&buf[18..20], &718u16.to_be_bytes());
        assert_eq!(&buf[20..22], &1600u16.to_be_bytes());
        assert_eq!(&buf[22..24], &0xFFFFu16.to_be_bytes()); // pressure 1.0
        assert_eq!(buf.len(), 32);
    }

    #[test]
    fn test_scroll_one_notch() {
        let buf = inject_scroll(0, 0, 718, 1600, 0.0, -1.0);
        assert_eq!(buf[0], MSG_INJECT_SCROLL_EVENT);
        // 布局：type(1) + x(4)+y(4)+w(2)+h(2) + hscroll(2) + vscroll(2) + buttons(4) = 21
        assert_eq!(&buf[13..15], &0i16.to_be_bytes()); // hscroll
        assert_eq!(&buf[15..17], &(-2048i16).to_be_bytes()); // vscroll = -1 格
        assert_eq!(&buf[17..21], &0i32.to_be_bytes()); // buttons
        assert_eq!(buf.len(), 21);
    }

    #[test]
    fn test_parse_session_header() {
        let mut header = [0u8; 12];
        header[0] = 0x80; // session flag
        header[4..8].copy_from_slice(&718u32.to_be_bytes());
        header[8..12].copy_from_slice(&1600u32.to_be_bytes());
        match parse_video_header(&header) {
            VideoHeader::Session { width, height } => {
                assert_eq!((width, height), (718, 1600));
            }
            _ => panic!("expected session"),
        }
    }

    #[test]
    fn test_parse_media_header() {
        let mut header = [0u8; 12];
        // key frame + pts=1000
        let pts_and_flags: i64 = (1 << 61) | 1000;
        header[0..8].copy_from_slice(&pts_and_flags.to_be_bytes());
        header[8..12].copy_from_slice(&55812u32.to_be_bytes());
        match parse_video_header(&header) {
            VideoHeader::Media { pts, config, key_frame, size } => {
                assert_eq!((pts, config, key_frame, size), (1000, false, true, 55812));
            }
            _ => panic!("expected media"),
        }
    }

    #[test]
    fn test_inject_text_layout() {
        let buf = inject_text("hi");
        assert_eq!(buf[0], MSG_INJECT_TEXT);
        assert_eq!(&buf[1..5], &2i32.to_be_bytes());
        assert_eq!(&buf[5..], b"hi");
    }
}
