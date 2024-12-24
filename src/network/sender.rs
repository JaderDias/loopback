use pnet::datalink;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

pub fn create_socket(interface_name: String) -> UdpSocket {
    // Find the specified network interface
    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(|iface| iface.name == interface_name)
        .expect("Interface not found");

    println!(
        "Using interface: {}, with IPs: {:?}",
        interface.name, interface.ips
    );

    // Bind the UDP socket to the interface's first IP
    let bind_addr = match interface.ips.first() {
        Some(ip) => ip.ip(),
        None => panic!("Interface has no associated IPs"),
    };

    UdpSocket::bind(SocketAddr::new(bind_addr, 0)).expect("Failed to bind UDP socket to interface")
}

pub fn send(socket: &UdpSocket, target_ip: &str, port: u16, min_payload: usize) {
    let target_ip: Ipv4Addr = target_ip.parse().expect("Invalid target IP");
    let target_addr = SocketAddr::new(target_ip.into(), port); // Target port is hardcoded for now
    let payload_size = min_payload;
    let payload = vec![0u8; payload_size];

    match socket.send_to(&payload, target_addr) {
        Ok(size) => println!("Sent {} bytes to {}", size, target_addr),
        Err(e) => eprintln!("Failed to send packet: {}", e),
    }
}
