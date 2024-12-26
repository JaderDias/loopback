use byteorder::{BigEndian, WriteBytesExt}; // Add this crate for binary data serialization
use pnet::datalink;
use std::collections::VecDeque;
use std::io::Cursor;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::{self, Duration};

pub async fn start_sending(
    config: &crate::config::Config,
    sent_counter: Arc<AtomicU64>,
    history: Arc<Mutex<VecDeque<crate::model::Result>>>,
) {
    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(|iface| iface.name == config.alternative_interface)
        .expect("Interface not found");

    println!(
        "Using interface: {}, with IPs: {:?}",
        interface.name, interface.ips
    );

    // Use the first available IP address on the interface
    let src_ip = match interface.ips.first() {
        Some(ip) => match ip.ip() {
            IpAddr::V4(v4) => v4,
            _ => panic!("Only IPv4 is supported"),
        },
        None => panic!("Interface has no associated IPs"),
    };

    let mut interval = time::interval(Duration::from_millis(config.interval_millis));
    const MAX_LATENCY_MICROS: u64 = 1_000_000;

    use libc::{IP_MTU_DISCOVER, IP_PMTUDISC_DO};
    use std::net::UdpSocket;
    use std::os::unix::io::AsRawFd;

    let src = format!("{src_ip}:0");
    let socket = UdpSocket::bind(&src).expect("failed to bind");
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

    let address = format!("{}:{}", config.public_ip_address, config.target_port);

    loop {
        interval.tick().await;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_micros();
        {
            // Add the packet to the unacknowledged queue
            let mut queue = history.lock().await;
            if queue.len() >= config.max_queue_size {
                queue.pop_front(); // Discard the oldest entry if the queue is full
            }

            queue.push_back((timestamp, MAX_LATENCY_MICROS));
        }

        let counter = sent_counter.fetch_add(1, Ordering::Relaxed);

        let payload = get_payload(counter, timestamp, config.max_packet_size);
        socket.send_to(&payload, &address).unwrap_or_else(|e| {
            eprintln!("Failed to send packet: {}", e);
            0
        });
    }
}

fn get_payload(counter: u64, timestamp: u128, size: usize) -> Vec<u8> {
    let mut payload = vec![0u8; size]; // Initialize a buffer with zero padding

    // Write counter and timestamp into the buffer
    let mut cursor = Cursor::new(&mut payload);
    cursor.write_u64::<BigEndian>(counter).unwrap(); // Write the counter as a 64-bit unsigned integer
    cursor.write_u128::<BigEndian>(timestamp).unwrap(); // Write the timestamp as a 128-bit unsigned integer

    payload // Return the binary payload
}
