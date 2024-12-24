use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::UdpSocket;

pub async fn start_listener(port: u16, received_counter: Arc<AtomicUsize>) {
    let addr = format!("0.0.0.0:{}", port);
    let socket = UdpSocket::bind(addr)
        .await
        .expect("Failed to bind UDP socket");

    println!("Listening for packets on port {}", port);

    let mut buf = [0; 2048];
    loop {
        let (size, src) = socket
            .recv_from(&mut buf)
            .await
            .expect("Failed to receive packet");
        received_counter.fetch_add(1, Ordering::Relaxed);
        println!("Received {} bytes from {}: {:?}", size, src, &buf[..size]);
    }
}
