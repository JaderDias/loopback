use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;

const PAYLOAD_SIZE: usize = 128; // Ensure this matches the sender's payload size

pub async fn start_listener(port: u16, received_counter: Arc<AtomicU64>) {
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

        // Increment received counter
        received_counter.fetch_add(1, Ordering::Relaxed);

        // Decode payload
        if size >= PAYLOAD_SIZE {
            let payload = &buf[..size];
            let mut cursor = Cursor::new(payload);

            let counter = cursor.read_u64::<BigEndian>().unwrap_or_default(); // Read the counter
            let timestamp = cursor.read_u128::<BigEndian>().unwrap_or_default(); // Read the timestamp

            // Calculate the time difference
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();
            let time_diff = now as i128 - timestamp as i128;

            // Print results
            println!(
                "Received packet from {}: Counter = {}, Timestamp = {}, Time Difference = {} ms",
                src, counter, timestamp, time_diff
            );
        } else {
            println!(
                "Received invalid or short packet from {}: {:?}",
                src,
                &buf[..size]
            );
        }
    }
}
