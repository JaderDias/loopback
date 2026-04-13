use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use tokio::time;

fn now_micros() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
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
        let mtu =
            tokio::task::spawn_blocking(move || probe_udp_blocking(&bind, &addr, min_mtu, max_mtu))
                .await
                .unwrap_or(None);
        if let Some(mtu) = mtu {
            println!("UDP MTU probe: {} bytes", mtu);
            let mut q = history.lock().await;
            push_mtu(&mut q, mtu, max_queue_size);
        }
    }
}

/// Binary-search for the maximum fitting MTU in [min, max], stepping by 2 (even only).
/// `probe` returns Some(true)=fits, Some(false)=too big, None=abort.
fn binary_search_mtu(min: u32, max: u32, mut probe: impl FnMut(u32) -> Option<bool>) -> Option<u32> {
    let mut lo = (min + 1) & !1; // round up to even
    let mut hi = max & !1;       // round down to even
    let mut result = None;
    while lo <= hi {
        let mid = ((lo + hi) / 2) & !1;
        match probe(mid) {
            Some(true)  => { result = Some(mid); lo = mid + 2; }
            Some(false) => { hi = mid.saturating_sub(2); }
            None        => break,
        }
    }
    result
}

/// Try special well-known MTU values before falling back to binary search.
/// Order: 9000 (jumbo) → 1472 → 1512 → binary search in remaining range.
fn probe_mtu(min: u32, max: u32, mut probe: impl FnMut(u32) -> Option<bool>) -> Option<u32> {
    if matches!(probe(9000), Some(true)) {
        return Some(9000);
    }
    match probe(1472) {
        None        => return None,
        Some(false) => return binary_search_mtu(min, 1470, probe),
        Some(true)  => {}
    }
    let upper = max & !1;
    match probe(1512) {
        Some(true) => Some(1512),
        _          => binary_search_mtu(1474, upper.min(1510), &mut probe).or(Some(1472)),
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

    probe_mtu(min, max, |size| {
        let payload = vec![0u8; size as usize];
        let _ = socket.send_to(&payload, address);
        std::thread::sleep(Duration::from_millis(80));
        match socket.send_to(&payload, address) {
            Ok(_)                                              => Some(true),
            Err(e) if e.raw_os_error() == Some(libc::EMSGSIZE) => Some(false),
            Err(_)                                             => None,
        }
    })
}

// ── ICMP MTU (raw socket with IP_PMTUDISC_DO, binary search) ──────────────────
//
// We build raw ICMP echo requests ourselves so we can set IP_PMTUDISC_DO on the
// socket. The probe size is the total IP payload (ICMP header 8 B + data), so
// the on-wire IP packet size is probe_size + 20 (IPv4 header).
//
// Same two-send strategy as UDP: first send teaches the kernel's PMTU cache via
// the ICMP "fragmentation needed" reply from routers; the second send returns
// EMSGSIZE immediately if the size is confirmed too large.

pub async fn start_probing_icmp(
    target: String,
    min_mtu: u32,
    max_mtu: u32,
    max_queue_size: usize,
    history: Arc<Mutex<VecDeque<(u128, u32)>>>,
) {
    let ip: Ipv4Addr = match target.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4,
        Ok(_) => {
            eprintln!("ICMP MTU probe only supports IPv4 ({})", target);
            return;
        }
        Err(e) => {
            eprintln!("Invalid ICMP MTU probe target '{}': {}", target, e);
            return;
        }
    };

    let mut interval = time::interval(Duration::from_secs(60));
    let mut seq: u16 = 0;

    loop {
        interval.tick().await;

        let ip_copy = ip;
        let min = min_mtu;
        let max = max_mtu;
        let seq_base = seq;

        let result = tokio::task::spawn_blocking(move || {
            probe_icmp_blocking(ip_copy, min, max, seq_base)
        })
        .await
        .unwrap_or(None);

        // Advance seq past all probes used this round (3 special + ~13 binary = 20)
        seq = seq.wrapping_add(20);

        if let Some(mtu) = result {
            println!("ICMP MTU probe {}: {} bytes", target, mtu);
            let mut q = history.lock().await;
            push_mtu(&mut q, mtu, max_queue_size);
        }
    }
}

/// Probe sizes represent total IP packet size (IP header + ICMP header + data).
/// ICMP data length = probe_size - 20 (IP hdr) - 8 (ICMP hdr), minimum 0.
fn probe_icmp_blocking(ip: Ipv4Addr, min: u32, max: u32, seq_base: u16) -> Option<u32> {
    use socket2::{Domain, Protocol, Socket, Type};
    use std::mem::MaybeUninit;
    use std::os::unix::io::AsRawFd;

    let socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))
        .map_err(|e| eprintln!("ICMP MTU probe: cannot create raw socket (missing CAP_NET_RAW?): {}", e))
        .ok()?;
    socket.set_read_timeout(Some(Duration::from_millis(500))).ok()?;

    // Set DF bit — the whole point of this implementation
    let fd = socket.as_raw_fd();
    unsafe {
        let optval: libc::c_int = libc::IP_PMTUDISC_DO;
        libc::setsockopt(
            fd,
            libc::IPPROTO_IP,
            libc::IP_MTU_DISCOVER,
            &optval as *const _ as *const libc::c_void,
            std::mem::size_of_val(&optval) as libc::socklen_t,
        );
    }

    let dest = socket2::SockAddr::from(SocketAddrV4::new(ip, 0));
    const IDENT: u16 = 0xF00D;
    let mut seq = seq_base;
    let mut buf = [MaybeUninit::uninit(); 2048];

    probe_mtu(min, max, |size| {
        seq = seq.wrapping_add(1);
        let packet = build_icmp_echo(IDENT, seq, size);
        let _ = socket.send_to(&packet, &dest);
        std::thread::sleep(Duration::from_millis(80));
        match socket.send_to(&packet, &dest) {
            Err(e) if e.raw_os_error() == Some(libc::EMSGSIZE) => return Some(false),
            Err(_) => return None,
            Ok(_) => {}
        }
        // Timeout treated as too big (PMTU drop without EMSGSIZE yet, or loss)
        if wait_for_reply(&socket, &mut buf, IDENT, seq) { Some(true) } else { Some(false) }
    })
}

/// Build an ICMP echo request. Total on-wire IP payload = 8 (ICMP hdr) + data.
/// We size data so that IP packet = probe_size: data = probe_size - 20 - 8.
fn build_icmp_echo(ident: u16, seq: u16, probe_ip_size: u32) -> Vec<u8> {
    let data_len = (probe_ip_size as usize).saturating_sub(20 + 8);
    let total = 8 + data_len;
    let mut pkt = vec![0u8; total];
    pkt[0] = 8; // type: echo request
    pkt[1] = 0; // code
    // checksum at [2..4] — fill after
    pkt[4] = (ident >> 8) as u8;
    pkt[5] = ident as u8;
    pkt[6] = (seq >> 8) as u8;
    pkt[7] = seq as u8;
    let csum = icmp_checksum(&pkt);
    pkt[2] = (csum >> 8) as u8;
    pkt[3] = csum as u8;
    pkt
}

fn icmp_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Read from the raw socket until we see an echo reply matching our ident+seq,
/// or until the read timeout fires.
fn wait_for_reply(
    socket: &socket2::Socket,
    buf: &mut [std::mem::MaybeUninit<u8>; 2048],
    ident: u16,
    seq: u16,
) -> bool {
    loop {
        match socket.recv_from(buf) {
            Ok((n, _)) => {
                // Safety: recv_from initialises the first n bytes
                let data: &[u8] =
                    unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, n) };
                // RAW socket on Linux: data includes the 20-byte IPv4 header
                if data.len() < 28 {
                    continue;
                }
                let icmp = &data[20..];
                // type=0 (echo reply), code=0, ident and seq match
                if icmp[0] == 0
                    && icmp[1] == 0
                    && u16::from_be_bytes([icmp[4], icmp[5]]) == ident
                    && u16::from_be_bytes([icmp[6], icmp[7]]) == seq
                {
                    return true;
                }
                // Not our reply; keep reading until timeout
            }
            Err(_) => return false, // timeout or error
        }
    }
}
