use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SERVICES: &[(&str, &str, &str)] = &[
    ("api.ipify.org:80", "api.ipify.org", "GET / HTTP/1.0\r\nHost: api.ipify.org\r\n\r\n"),
    ("ifconfig.me:80", "ifconfig.me", "GET /ip HTTP/1.0\r\nHost: ifconfig.me\r\nUser-Agent: curl/7.0\r\n\r\n"),
    ("icanhazip.com:80", "icanhazip.com", "GET / HTTP/1.0\r\nHost: icanhazip.com\r\n\r\n"),
];

pub async fn discover() -> Option<String> {
    // Prefer gluetun's control server — it returns the VPN exit IP directly.
    if let Some(ip) = try_gluetun().await {
        println!("Public IP: {} (from gluetun)", ip);
        return Some(ip);
    }

    for (addr, host, request) in SERVICES {
        match try_service(addr, request).await {
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

/// Query gluetun's HTTP control server (default port 8000).
/// Response body is JSON: {"public_ip":"1.2.3.4", ...}
async fn try_gluetun() -> Option<String> {
    let request = "GET /v1/publicip/ip HTTP/1.0\r\nHost: 127.0.0.1:8000\r\n\r\n";
    let mut stream = TcpStream::connect("127.0.0.1:8000").await.ok()?;
    stream.write_all(request.as_bytes()).await.ok()?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.ok()?;

    let text = String::from_utf8_lossy(&response);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or(&text);

    // Extract "public_ip":"VALUE" from JSON without a JSON library
    let key = "\"public_ip\":\"";
    let start = body.find(key)? + key.len();
    let end = body[start..].find('"')? + start;
    let ip = body[start..end].trim().to_string();

    if ip.parse::<std::net::IpAddr>().is_ok() {
        Some(ip)
    } else {
        None
    }
}

async fn try_service(addr: &str, request: &str) -> Option<String> {
    let mut stream = TcpStream::connect(addr).await.ok()?;
    stream.write_all(request.as_bytes()).await.ok()?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.ok()?;

    let text = String::from_utf8_lossy(&response);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or(&text);
    let ip = body.trim().to_string();

    if ip.parse::<std::net::IpAddr>().is_ok() {
        Some(ip)
    } else {
        None
    }
}
