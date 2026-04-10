use byteorder::{BigEndian, ReadBytesExt};
use std::collections::{HashSet, VecDeque};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

use crate::model::Packet;

// Keep a sliding window of recently seen sequence numbers for duplicate detection.
const SEEN_WINDOW: usize = 10_000;

pub async fn start_listener(
    port: u16,
    sent_counter: Arc<AtomicU64>,
    history: Arc<Mutex<VecDeque<Packet>>>,
) {
    let addr = format!("0.0.0.0:{}", port);
    let socket = match UdpSocket::bind(&addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Cannot bind UDP socket on {} (loopback disabled): {}. \
                 ICMP ping will continue as fallback.",
                addr, e
            );
            return;
        }
    };

    println!("Listening for loopback packets on {}", addr);

    let mut buf = [0u8; 2048];
    // Sliding window for duplicate / reorder detection
    let mut seen_seqs: VecDeque<u64> = VecDeque::with_capacity(SEEN_WINDOW + 1);
    let mut seen_set: HashSet<u64> = HashSet::with_capacity(SEEN_WINDOW + 1);
    let mut max_seen_seq: u64 = 0;
    let mut first_packet = true;

    loop {
        let (size, _) = match socket.recv_from(&mut buf).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Loopback recv error: {}", e);
                continue;
            }
        };

        let payload = &buf[..size];
        let mut cursor = Cursor::new(payload);

        let counter = cursor.read_u64::<BigEndian>().unwrap_or_default();
        let timestamp = cursor.read_u128::<BigEndian>().unwrap_or_default();
        // Size field added in new payload format (offset 24, 4 bytes)
        let recv_size = cursor.read_u32::<BigEndian>().unwrap_or(0);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_micros();
        let latency = (now as i128 - timestamp as i128).max(0) as u64;

        // Duplicate detection
        let is_duplicate = seen_set.contains(&counter);

        // Reorder detection: a packet is reordered if it arrives after a higher-numbered
        // packet has already been received (and it's not a duplicate)
        let is_reordered = !is_duplicate && !first_packet && counter < max_seen_seq;

        if !is_duplicate {
            if first_packet || counter > max_seen_seq {
                max_seen_seq = counter;
                first_packet = false;
            }
            // Maintain bounded sliding window
            seen_seqs.push_back(counter);
            seen_set.insert(counter);
            if seen_seqs.len() > SEEN_WINDOW {
                if let Some(old) = seen_seqs.pop_front() {
                    seen_set.remove(&old);
                }
            }
        }

        // Locate this packet in the history queue by age
        let sent = sent_counter.load(Ordering::Relaxed);
        let age = sent.saturating_sub(counter) as usize;

        let mut queue = history.lock().await;
        if let Some(index) = queue.len().checked_sub(age) {
            if let Some(packet) = queue.get_mut(index) {
                if is_duplicate {
                    packet.duplicate = true;
                } else {
                    packet.latency = latency;
                    packet.reordered = is_reordered;
                    if recv_size > 0 {
                        packet.size = recv_size;
                    }
                }
            }
        }
    }
}
