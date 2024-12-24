use byteorder::{BigEndian, ReadBytesExt};
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

pub async fn start_listener(
    port: u16,
    sent_counter: Arc<AtomicU64>,
    history: Arc<Mutex<VecDeque<u64>>>,
) {
    let addr = format!("0.0.0.0:{}", port);
    let socket = UdpSocket::bind(addr)
        .await
        .expect("Failed to bind UDP socket");

    let mut buf = [0; 2048];
    loop {
        let (size, src) = socket
            .recv_from(&mut buf)
            .await
            .expect("Failed to receive packet");

        let payload = &buf[..size];
        let mut cursor = Cursor::new(payload);

        let counter = cursor.read_u64::<BigEndian>().unwrap_or_default();
        let age: usize = (sent_counter.load(Ordering::Relaxed) - counter)
            .try_into()
            .unwrap();
        let timestamp = cursor.read_u128::<BigEndian>().unwrap_or_default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        let time_diff = now as i128 - timestamp as i128;

        // Remove the packet from the unacknowledged queue
        {
            let mut queue = history.lock().await;
            let index = queue.len() - age;
            if let Some(latency) = queue.get_mut(index) {
                *latency = time_diff as u64;
            }
        }

        println!(
            "Received packet from {}: Counter = {}, Timestamp = {}, Time Difference = {} ms",
            src, counter, timestamp, time_diff
        );
    }
}
