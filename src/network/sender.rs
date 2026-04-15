use byteorder::{BigEndian, WriteBytesExt};
use pnet::datalink;
use std::collections::VecDeque;
use std::io::Cursor;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::{self, Duration};

use crate::model::Packet;

pub async fn start_sending(
    config: &crate::config::Config,
    public_ip: String,
    session_id: u32,
    sent_counter: Arc<AtomicU64>,
    history: Arc<Mutex<VecDeque<Packet>>>,
) {
    // When ALTERNATIVE_INTERFACE is set, verify the VPN is up (port forwarding requires it),
    // but always egress via the default route (eth0 → internet → VPN server NAT-PMP →
    // back through the VPN tunnel → listener). Binding to the VPN IP would make packets
    // travel through the tunnel to the server, which does not hairpin them back.
    if let Some(iface_name) = &config.alternative_interface {
        loop {
            let interfaces = datalink::interfaces();
            match interfaces.into_iter().find(|iface| &iface.name == iface_name) {
                Some(iface) => {
                    let ip_str = iface
                        .ips
                        .iter()
                        .find_map(|ip| match ip.ip() {
                            IpAddr::V4(v4) => Some(v4.to_string()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "?".to_string());
                    println!("VPN interface {} ({}) is up; sender will egress via default route.", iface_name, ip_str);
                    break;
                }
                None => {
                    eprintln!(
                        "Interface '{}' not found, retrying in 30s...",
                        iface_name
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                }
            }
        }
    }
    let bind_addr = "0.0.0.0:0".to_string();

    use libc::{IP_MTU_DISCOVER, IP_PMTUDISC_DO};
    use std::net::UdpSocket;
    use std::os::unix::io::AsRawFd;

    let socket = match UdpSocket::bind(&bind_addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Cannot bind UDP sender socket on {} (loopback sender disabled): {}. \
                 ICMP ping will continue as fallback.",
                bind_addr, e
            );
            return;
        }
    };

    let fd = socket.as_raw_fd();
    unsafe {
        let optval: libc::c_int = IP_PMTUDISC_DO;
        let result = libc::setsockopt(
            fd,
            libc::IPPROTO_IP,
            IP_MTU_DISCOVER,
            &optval as *const _ as *const libc::c_void,
            std::mem::size_of_val(&optval) as libc::socklen_t,
        );
        if result != 0 {
            eprintln!(
                "Failed to set IP_MTU_DISCOVER: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    let address = format!("{}:{}", public_ip, config.target_port);
    let size = config.max_packet_size as u32;
    let mut interval = time::interval(Duration::from_millis(config.interval_millis));

    loop {
        interval.tick().await;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();

        // Increment counter and push to history atomically under the same lock,
        // so the listener can rely on: queue.len() == sent_counter at all times.
        let counter;
        {
            let mut queue = history.lock().await;
            if queue.len() >= config.max_queue_size {
                queue.pop_front();
            }
            counter = sent_counter.fetch_add(1, Ordering::Relaxed);
            queue.push_back(Packet::pending(timestamp, size));
        }

        let payload = build_payload(counter, timestamp, size, session_id);
        socket.send_to(&payload, &address).unwrap_or_else(|e| {
            eprintln!("Failed to send packet: {}", e);
            0
        });
    }
}

/// Payload layout (32 bytes header + padding):
///   [0..8]   counter    u64 big-endian
///   [8..24]  timestamp  u128 big-endian
///   [24..28] size       u32 big-endian
///   [28..32] session_id u32 big-endian  ← prevents stale packets from a prior run
///   [32..]   zero padding
fn build_payload(counter: u64, timestamp: u128, size: u32, session_id: u32) -> Vec<u8> {
    let mut payload = vec![0u8; size as usize];
    let mut cursor = Cursor::new(&mut payload);
    cursor.write_u64::<BigEndian>(counter).unwrap();
    cursor.write_u128::<BigEndian>(timestamp).unwrap();
    cursor.write_u32::<BigEndian>(size).unwrap();
    cursor.write_u32::<BigEndian>(session_id).unwrap();
    payload
}
