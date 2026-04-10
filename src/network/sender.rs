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
    sent_counter: Arc<AtomicU64>,
    history: Arc<Mutex<VecDeque<Packet>>>,
) {
    let bind_addr = match &config.alternative_interface {
        Some(iface_name) => {
            let interfaces = datalink::interfaces();
            match interfaces.into_iter().find(|iface| &iface.name == iface_name) {
                Some(iface) => match iface.ips.first() {
                    Some(ip) => match ip.ip() {
                        IpAddr::V4(v4) => {
                            println!("Using interface: {} ({})", iface_name, v4);
                            format!("{}:0", v4)
                        }
                        _ => {
                            eprintln!(
                                "Interface '{}' has no IPv4 address (loopback sender disabled).",
                                iface_name
                            );
                            return;
                        }
                    },
                    None => {
                        eprintln!(
                            "Interface '{}' has no IPs (loopback sender disabled).",
                            iface_name
                        );
                        return;
                    }
                },
                None => {
                    eprintln!(
                        "Interface '{}' not found (loopback sender disabled). \
                         ICMP ping will continue as fallback.",
                        iface_name
                    );
                    return;
                }
            }
        }
        None => {
            println!("No ALTERNATIVE_INTERFACE set, binding loopback sender to 0.0.0.0");
            "0.0.0.0:0".to_string()
        }
    };

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
            .expect("Time went backwards")
            .as_micros();

        let counter = sent_counter.fetch_add(1, Ordering::Relaxed);

        {
            let mut queue = history.lock().await;
            if queue.len() >= config.max_queue_size {
                queue.pop_front();
            }
            queue.push_back(Packet::pending(timestamp, size));
        }

        let payload = build_payload(counter, timestamp, size);
        socket.send_to(&payload, &address).unwrap_or_else(|e| {
            eprintln!("Failed to send packet: {}", e);
            0
        });
    }
}

/// Payload layout (28 bytes header + padding):
///   [0..8]   counter   u64 big-endian
///   [8..24]  timestamp u128 big-endian
///   [24..28] size      u32 big-endian  ← new field for MTU tracking
///   [28..]   zero padding
fn build_payload(counter: u64, timestamp: u128, size: u32) -> Vec<u8> {
    let mut payload = vec![0u8; size as usize];
    let mut cursor = Cursor::new(&mut payload);
    cursor.write_u64::<BigEndian>(counter).unwrap();
    cursor.write_u128::<BigEndian>(timestamp).unwrap();
    cursor.write_u32::<BigEndian>(size).unwrap();
    payload
}
