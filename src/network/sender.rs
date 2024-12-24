use pnet::datalink;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{self, Duration};

pub async fn start_sending(
    interface_name: String,
    target_ip: String,
    port: u16,
    min_payload: usize,
    interval_millis: u64,
    sent_counter: Arc<AtomicUsize>,
) {
    // Find the specified network interface
    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(|iface| iface.name == interface_name)
        .expect("Interface not found");

    println!(
        "Using interface: {}, with IPs: {:?}",
        interface.name, interface.ips
    );

    // Bind the UDP socket to the interface's first IP
    let bind_addr = match interface.ips.first() {
        Some(ip) => ip.ip(),
        None => panic!("Interface has no associated IPs"),
    };

    let socket = UdpSocket::bind(SocketAddr::new(bind_addr, 0))
        .expect("Failed to bind UDP socket to interface");

    println!(
        "Bound UDP socket to {}. Sending packets to {}...",
        bind_addr, target_ip
    );

    let target_ip: Ipv4Addr = target_ip.parse().expect("Invalid target IP");
    let target_addr = SocketAddr::new(target_ip.into(), port);

    let mut interval = time::interval(Duration::from_millis(interval_millis));

    loop {
        interval.tick().await;

        // Generate packet content
        let counter = sent_counter.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        let payload = format!("Counter: {}, Timestamp: {}", counter, timestamp);

        match socket.send_to(payload.as_bytes(), target_addr) {
            Ok(size) => println!("Sent {} bytes to {}", size, target_addr),
            Err(e) => eprintln!("Failed to send packet: {}", e),
        }
    }
}
