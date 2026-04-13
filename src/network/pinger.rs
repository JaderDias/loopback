use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use surge_ping::{Client, Config, PingIdentifier, PingSequence, SurgeError};
use tokio::sync::Mutex;
use tokio::time::{self, Duration};

use crate::model::{Packet, MAX_LATENCY_MICROS};

pub async fn start_pinging(
    target: String,
    interval_millis: u64,
    max_packet_size: u32,
    max_queue_size: usize,
    history: Arc<Mutex<VecDeque<Packet>>>,
) {
    let ip: IpAddr = match target.parse() {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("Invalid ping target '{}': {}", target, e);
            return;
        }
    };

    let client = match Client::new(&Config::default()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Cannot create ICMP socket (ping disabled): {}. \
                 To enable, grant CAP_NET_RAW or set /proc/sys/net/ipv4/ping_group_range.",
                e
            );
            return;
        }
    };

    let mut pinger = client
        .pinger(ip, PingIdentifier(std::process::id() as u16))
        .await;
    pinger.timeout(Duration::from_secs(1));

    println!("Pinging {} every {}ms", target, interval_millis);

    let payload = vec![0u8; max_packet_size as usize];
    let mut interval = time::interval(Duration::from_millis(interval_millis));
    let mut seq: u16 = 0;

    loop {
        interval.tick().await;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();

        let latency = match pinger.ping(PingSequence(seq), &payload).await {
            Ok((_packet, duration)) => duration.as_micros() as u64,
            Err(SurgeError::Timeout { .. }) => MAX_LATENCY_MICROS,
            Err(e) => {
                eprintln!("Ping error to {}: {}", target, e);
                MAX_LATENCY_MICROS
            }
        };

        {
            let mut queue = history.lock().await;
            if queue.len() >= max_queue_size {
                queue.pop_front();
            }
            queue.push_back(Packet {
                timestamp,
                latency,
                size: max_packet_size,
                reordered: false, // ICMP is sequential — reorder can't occur
                duplicate: false,
            });
        }

        seq = seq.wrapping_add(1);
    }
}
