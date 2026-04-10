use std::collections::VecDeque;
use std::net::{IpAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use surge_ping::{Client, Config, PingIdentifier, PingSequence, SurgeError};
use tokio::sync::Mutex;
use tokio::time;

fn now_micros() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_micros()
}

fn push_mtu(history: &mut VecDeque<(u128, u32)>, mtu: u32, max_queue_size: usize) {
    if history.len() >= max_queue_size {
        history.pop_front();
    }
    history.push_back((now_micros(), mtu));
}

// ── UDP loopback MTU (via EMSGSIZE binary search) ─────────────────────────────
//
// With IP_PMTUDISC_DO set, send_to() returns EMSGSIZE immediately once the
// kernel's PMTU cache knows the path MTU. We send a packet twice — the first
// send triggers an ICMP "fragmentation needed" from the router if oversized,
// and the kernel updates its cache; the second send then returns EMSGSIZE.

pub async fn start_probing_udp(
    bind_addr: String,
    address: String,
    min_mtu: u32,
    max_mtu: u32,
    max_queue_size: usize,
    history: Arc<Mutex<VecDeque<(u128, u32)>>>,
) {
    let mut interval = time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let bind = bind_addr.clone();
        let addr = address.clone();
        let mtu = tokio::task::spawn_blocking(move || probe_udp_blocking(&bind, &addr, min_mtu, max_mtu))
            .await
            .unwrap_or(None);
        if let Some(mtu) = mtu {
            println!("UDP MTU probe: {} bytes", mtu);
            let mut q = history.lock().await;
            push_mtu(&mut q, mtu, max_queue_size);
        }
    }
}

fn probe_udp_blocking(bind_addr: &str, address: &str, min: u32, max: u32) -> Option<u32> {
    use libc::{IP_MTU_DISCOVER, IP_PMTUDISC_DO};
    use std::os::unix::io::AsRawFd;

    let socket = UdpSocket::bind(bind_addr).ok()?;
    let fd = socket.as_raw_fd();
    unsafe {
        let optval: libc::c_int = IP_PMTUDISC_DO;
        libc::setsockopt(
            fd,
            libc::IPPROTO_IP,
            IP_MTU_DISCOVER,
            &optval as *const _ as *const libc::c_void,
            std::mem::size_of_val(&optval) as libc::socklen_t,
        );
    }

    let mut lo = min;
    let mut hi = max;
    let mut result = None;

    while lo <= hi {
        let mid = (lo + hi) / 2;
        let payload = vec![0u8; mid as usize];

        // First send: may go through and trigger ICMP frag-needed from a router
        let _ = socket.send_to(&payload, address);
        // Brief pause for the kernel to process any ICMP response
        std::thread::sleep(Duration::from_millis(80));
        // Second send: kernel now knows if this size exceeds the path MTU
        match socket.send_to(&payload, address) {
            Ok(_) => {
                result = Some(mid);
                lo = mid + 1;
            }
            Err(e) if e.raw_os_error() == Some(libc::EMSGSIZE) => {
                hi = mid - 1;
            }
            Err(_) => break,
        }
    }

    result
}

// ── ICMP MTU (vary payload sizes, report max successful) ──────────────────────
//
// Note: without DF bit control through surge-ping's API, the OS may fragment
// oversized packets rather than dropping them, so this measures effective
// throughput size rather than strict path MTU.

pub async fn start_probing_icmp(
    target: String,
    min_mtu: u32,
    max_mtu: u32,
    max_queue_size: usize,
    history: Arc<Mutex<VecDeque<(u128, u32)>>>,
) {
    let ip: IpAddr = match target.parse() {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("Invalid MTU probe target '{}': {}", target, e);
            return;
        }
    };

    let client = match Client::new(&Config::default()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Cannot create ICMP MTU probe socket for {}: {}", target, e);
            return;
        }
    };

    // Use a distinct PingIdentifier so probes don't collide with the regular pinger
    let mut pinger = client.pinger(ip, PingIdentifier(0xFFFE)).await;
    pinger.timeout(Duration::from_millis(500));

    let mut interval = time::interval(Duration::from_secs(60));
    let mut seq: u16 = 0;

    loop {
        interval.tick().await;

        let mtu = probe_icmp_binary(&mut pinger, min_mtu, max_mtu, &mut seq).await;
        if let Some(mtu) = mtu {
            println!("ICMP MTU probe {}: {} bytes", target, mtu);
            let mut q = history.lock().await;
            push_mtu(&mut q, mtu, max_queue_size);
        }
    }
}

async fn probe_icmp_binary(
    pinger: &mut surge_ping::Pinger,
    min: u32,
    max: u32,
    seq: &mut u16,
) -> Option<u32> {
    let mut lo = min;
    let mut hi = max;
    let mut result = None;

    while lo <= hi {
        let mid = (lo + hi) / 2;
        let payload = vec![0u8; mid as usize];
        *seq = seq.wrapping_add(1);
        match pinger.ping(PingSequence(*seq), &payload).await {
            Ok(_) => {
                result = Some(mid);
                lo = mid + 1;
            }
            Err(SurgeError::Timeout { .. }) => {
                hi = mid - 1;
            }
            Err(_) => {
                hi = mid - 1;
            }
        }
    }

    result
}
