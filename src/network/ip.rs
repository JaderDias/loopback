use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SERVICES: &[(&str, &str, &str)] = &[
    ("api.ipify.org:80", "api.ipify.org", "GET / HTTP/1.0\r\nHost: api.ipify.org\r\n\r\n"),
    ("ifconfig.me:80", "ifconfig.me", "GET /ip HTTP/1.0\r\nHost: ifconfig.me\r\nUser-Agent: curl/7.0\r\n\r\n"),
    ("icanhazip.com:80", "icanhazip.com", "GET / HTTP/1.0\r\nHost: icanhazip.com\r\n\r\n"),
];

pub async fn discover() -> Option<String> {
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

async fn try_service(addr: &str, request: &str) -> Option<String> {
    let mut stream = TcpStream::connect(addr).await.ok()?;
    stream.write_all(request.as_bytes()).await.ok()?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.ok()?;

    let text = String::from_utf8_lossy(&response);
    // HTTP response body follows the blank line after headers
    let body = text.split("\r\n\r\n").nth(1).unwrap_or(&text);
    let ip = body.trim().to_string();

    if ip.parse::<std::net::IpAddr>().is_ok() {
        Some(ip)
    } else {
        None
    }
}
