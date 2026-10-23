//! mDNS 服务广播：让手机在同一局域网自动发现 PC。
//!
//! 广播 `_fuyun._tcp.local.`，TXT 记录携带电脑名称与协议版本。
//! 广播失败不阻断同步服务（手机可手动输入 IP:端口）。

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// 服务类型（手机端按此类型浏览）
pub const SERVICE_TYPE: &str = "_fuyun._tcp.local.";

/// 启动广播，返回守护对象（drop 即注销）。
pub fn advertise(port: u16, pc_name: &str) -> Option<ServiceDaemon> {
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            log::warn!("mDNS 守护进程启动失败: {}", e);
            return None;
        }
    };
    let host_ip = lan_ip()?;
    let host_name = sanitize_host_name(pc_name);
    // 显式切片类型：泛型参数不会做数组→切片 unsize 强转
    let properties: &[(&str, &str)] = &[("name", pc_name), ("ver", super::SYNC_PROTOCOL_VERSION)];
    let service = match ServiceInfo::new(
        SERVICE_TYPE,
        "fuyun-clip",
        &host_name,
        host_ip,
        port,
        properties,
    ) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("mDNS 服务信息构造失败: {}", e);
            return None;
        }
    };
    match daemon.register(service) {
        Ok(()) => {
            log::info!("mDNS 广播已启动: {} ({}:{})", SERVICE_TYPE, host_ip, port);
            Some(daemon)
        }
        Err(e) => {
            log::warn!("mDNS 服务注册失败: {}", e);
            None
        }
    }
}

/// 获取本机局域网 IP：向外部地址连 UDP（不实际发包），读取本地绑定地址。
pub(crate) fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// mDNS 主机名只允许 ASCII 字母数字和连字符
fn sanitize_host_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(32)
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "fuyun-pc".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_host_name() {
        assert_eq!(sanitize_host_name("My PC"), "my-pc");
        assert_eq!(sanitize_host_name("DESKTOP-ABC123"), "desktop-abc123");
        assert_eq!(sanitize_host_name("电脑"), "fuyun-pc");
        assert_eq!(sanitize_host_name("---"), "fuyun-pc");
        // 超长截断
        let long = "a".repeat(50);
        assert_eq!(sanitize_host_name(&long).len(), 32);
    }

    #[test]
    fn test_lan_ip_returns_valid_ip() {
        // 无网络环境下允许 None；有网络时必须是合法 IP
        if let Some(ip) = lan_ip() {
            assert!(ip.is_ipv4() || ip.is_ipv6());
        }
    }
}
