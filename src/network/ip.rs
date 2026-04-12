use pnet::datalink;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpSocket;

const SERVICES: &[(&str, &str, &str)] = &[
    ("api.ipify.org:80", "api.ipify.org", "GET / HTTP/1.0\r\nHost: api.ipify.org\r\n\r\n"),
    ("ifconfig.me:80", "ifconfig.me", "GET /ip HTTP/1.0\r\nHost: ifconfig.me\r\nUser-Agent: curl/7.0\r\n\r\n"),
    ("icanhazip.com:80", "icanhazip.com", "GET / HTTP/1.0\r\nHost: icanhazip.com\r\n\r\n"),
];

/// Resolve the first IPv4 address of a named network interface.
fn interface_ipv4(name: &str) -> Option<Ipv4Addr> {
    datalink::interfaces()
        .into_iter()
        .find(|i| i.name == name)?
        .ips
        .into_iter()
        .find_map(|ip| match ip.ip() {
            IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
            _ => None,
        })
}

/// Discover our public IP, binding outbound connections to `iface` when given
/// so that the result reflects the VPN exit IP rather than the ISP IP.
pub async fn discover(iface: Option<&str>) -> Option<String> {
    let bind_ip = iface.and_then(interface_ipv4);

    for (addr, host, request) in SERVICES {
        match try_service(addr, request, bind_ip).await {
            Some(ip) => {
                println!("Public IP: {} (from {})", ip, host);
                return Some(ip);
            }
            None => eprintln!("Failed to get public IP from {}, trying next...", host),
        }
    }
    eprintln!("Could not discover public IP from any service.");
    None
}

async fn try_service(addr: &str, request: &str, bind_ip: Option<Ipv4Addr>) -> Option<String> {
    let remote: SocketAddr = tokio::net::lookup_host(addr).await.ok()?.next()?;

    let socket = TcpSocket::new_v4().ok()?;
    if let Some(ip) = bind_ip {
        socket.bind(SocketAddr::new(IpAddr::V4(ip), 0)).ok()?;
    }
    let mut stream = socket.connect(remote).await.ok()?;
    stream.write_all(request.as_bytes()).await.ok()?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.ok()?;

    let text = String::from_utf8_lossy(&response);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or(&text);
    let ip = body.trim().to_string();

    if ip.parse::<IpAddr>().is_ok() {
        Some(ip)
    } else {
        None
    }
}
