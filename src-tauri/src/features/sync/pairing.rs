//! 配对逻辑：6 位一次性配对码 + 设备令牌。
//!
//! 纯函数设计、无 IO，便于单测。配对码 5 分钟有效、单次使用；
//! 令牌（UUID）配对成功后发放，后续请求凭令牌认证。

use std::time::{SystemTime, UNIX_EPOCH};

/// 配对码有效期（毫秒）
pub const PAIRING_CODE_TTL_MS: i64 = 5 * 60 * 1000;

/// 配对码长度（位）
pub const PAIRING_CODE_DIGITS: usize = 6;

#[derive(Clone, Debug)]
pub struct PairingCode {
    pub code: String,
    pub expires_at_ms: i64,
}

impl PairingCode {
    pub fn is_valid(&self, now_ms: i64) -> bool {
        !self.code.is_empty() && now_ms < self.expires_at_ms
    }

    /// 剩余有效秒数（过期返回 0）
    pub fn remaining_secs(&self, now_ms: i64) -> i64 {
        ((self.expires_at_ms - now_ms) / 1000).max(0)
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 生成 6 位数字配对码。
/// 熵来源为 UUIDv4（内部走 getrandom），避免额外引入 rand 依赖。
pub fn generate_pairing_code() -> PairingCode {
    let uuid = uuid::Uuid::new_v4();
    let bytes = uuid.as_bytes();
    let mut n: u64 = 0;
    for (i, b) in bytes.iter().take(8).enumerate() {
        n |= (*b as u64) << (i * 8);
    }
    let modulus = 10u64.pow(PAIRING_CODE_DIGITS as u32);
    PairingCode {
        code: format!("{:0width$}", n % modulus, width = PAIRING_CODE_DIGITS),
        expires_at_ms: now_ms() + PAIRING_CODE_TTL_MS,
    }
}

/// 生成设备令牌（每个配对设备一个，可吊销）
pub fn generate_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 常量时间比较，防时序侧信道（局域网场景属纵深防御）
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 校验配对码：存在、未过期、内容匹配
pub fn verify_pairing_code(expected: Option<&PairingCode>, provided: &str, now_ms: i64) -> bool {
    match expected {
        Some(pc) if pc.is_valid(now_ms) => constant_time_eq(&pc.code, provided),
        _ => false,
    }
}

/// 从请求头或查询参数中提取令牌
pub fn extract_token(header_value: Option<&str>, query_value: Option<&str>) -> Option<String> {
    header_value
        .and_then(|h| h.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        .or_else(|| query_value.map(|s| s.trim().to_string()))
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairing_code_is_six_digits() {
        for _ in 0..50 {
            let pc = generate_pairing_code();
            assert_eq!(pc.code.len(), PAIRING_CODE_DIGITS, "配对码必须 6 位");
            assert!(
                pc.code.chars().all(|c| c.is_ascii_digit()),
                "配对码必须全数字: {}",
                pc.code
            );
        }
    }

    #[test]
    fn test_pairing_code_has_ttl() {
        let before = now_ms();
        let pc = generate_pairing_code();
        assert!(pc.is_valid(before));
        assert!(pc.remaining_secs(before) > 4 * 60);
        // 过期后失效
        let after_ttl = before + PAIRING_CODE_TTL_MS + 1;
        assert!(!pc.is_valid(after_ttl));
        assert_eq!(pc.remaining_secs(after_ttl), 0);
    }

    #[test]
    fn test_verify_pairing_code_success_and_failure() {
        let pc = generate_pairing_code();
        let now = now_ms();
        assert!(verify_pairing_code(Some(&pc), &pc.code, now));
        // 错误码
        let wrong = if pc.code == "000000" {
            "111111"
        } else {
            "000000"
        };
        assert!(!verify_pairing_code(Some(&pc), wrong, now));
        // 空码
        assert!(!verify_pairing_code(Some(&pc), "", now));
        // 无配对码时任何输入都失败
        assert!(!verify_pairing_code(None, "123456", now));
        // 过期
        assert!(!verify_pairing_code(
            Some(&pc),
            &pc.code,
            now + PAIRING_CODE_TTL_MS + 1
        ));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        // 长度不同直接 false（不泄露内容）
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn test_extract_token() {
        assert_eq!(
            extract_token(Some("Bearer tok-123"), None).as_deref(),
            Some("tok-123")
        );
        assert_eq!(
            extract_token(None, Some("tok-456")).as_deref(),
            Some("tok-456")
        );
        // 头优先
        assert_eq!(
            extract_token(Some("Bearer h"), Some("q")).as_deref(),
            Some("h")
        );
        // 畸形/空值
        assert_eq!(extract_token(Some("tok-789"), None), None);
        assert_eq!(extract_token(None, Some("  ")), None);
        assert_eq!(extract_token(None, None), None);
    }

    #[test]
    fn test_token_is_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert!(a.len() >= 32);
    }
}
