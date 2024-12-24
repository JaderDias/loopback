use byteorder::{BigEndian, WriteBytesExt}; // Add this crate for binary data serialization
use pnet::datalink;
use pnet::packet::ipv4::{Ipv4Flags, MutableIpv4Packet};
use pnet::packet::udp::{MutableUdpPacket, UdpPacket};
use pnet::packet::MutablePacket;
use pnet::packet::Packet;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{self, Duration};

pub async fn start_sending(
    interface_name: String,
    target_ip: String,
    port: u16,
    min_payload: usize,
    max_payload_size: usize,
    interval_millis: u64,
    sent_counter: Arc<AtomicU64>,
) {
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

    // Use the first available IP address on the interface
    let src_ip = match interface.ips.first() {
        Some(ip) => match ip.ip() {
            IpAddr::V4(v4) => v4,
            _ => panic!("Only IPv4 is supported"),
        },
        None => panic!("Interface has no associated IPs"),
    };

    let target_ip: Ipv4Addr = target_ip.parse().expect("Invalid target IP");
    let buffer_size: usize = 65535;

    // Create a raw socket
    let (mut tx, _) = pnet::transport::transport_channel(
        buffer_size,
        pnet::transport::TransportChannelType::Layer3(pnet::packet::ip::IpNextHeaderProtocols::Udp),
    )
    .expect("Failed to create transport channel");

    let mut interval = time::interval(Duration::from_millis(interval_millis));

    let buffer_length = MutableIpv4Packet::minimum_packet_size()
        + MutableUdpPacket::minimum_packet_size()
        + PAYLOAD_SIZE;

    dbg!(&buffer_length);

    loop {
        interval.tick().await;

        // Generate payload content
        let counter = sent_counter.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();

        let mut buffer = vec![0u8; buffer_length];

        let mut ip_packet = MutableIpv4Packet::new(&mut buffer).unwrap();

        // Set IPv4 fields
        ip_packet.set_version(4);
        ip_packet.set_header_length(5);
        ip_packet.set_total_length(buffer_length as u16);
        ip_packet.set_ttl(64);
        ip_packet.set_next_level_protocol(pnet::packet::ip::IpNextHeaderProtocols::Udp);
        ip_packet.set_source(src_ip);
        ip_packet.set_destination(target_ip);
        ip_packet.set_flags(Ipv4Flags::DontFragment);

        // Compute checksums
        ip_packet.set_checksum(pnet::packet::ipv4::checksum(&ip_packet.to_immutable()));

        let payload = create_binary_payload(counter, timestamp);

        let mut udp_packet = MutableUdpPacket::new(ip_packet.payload_mut()).unwrap();

        udp_packet.set_source(12345); // Arbitrary source port
        udp_packet.set_destination(port); // Set the target port
        udp_packet.set_length((MutableUdpPacket::minimum_packet_size() + payload.len()) as u16);
        udp_packet.set_payload(&payload);

        // Calculate and set the UDP checksum
        udp_packet.set_checksum(pnet::packet::udp::ipv4_checksum(
            &udp_packet.to_immutable(),
            &src_ip,
            &target_ip,
        ));

        // Send the entire IPv4 packet
        match tx.send_to(ip_packet, IpAddr::V4(target_ip)) {
            Ok(_) => println!(
                "Sent packet {} with {} bytes to {}:{}",
                counter, max_payload_size, target_ip, port
            ),
            Err(e) => eprintln!("Failed to send packet: {}", e),
        }
    }
}

const PAYLOAD_SIZE: usize = 192; // Fixed size for the payload

fn create_binary_payload(counter: u64, timestamp: u128) -> Vec<u8> {
    let mut payload = vec![0u8; PAYLOAD_SIZE]; // Initialize a buffer with zero padding

    {
        // Write counter and timestamp into the buffer
        let mut cursor = Cursor::new(&mut payload);
        cursor.write_u64::<BigEndian>(counter).unwrap(); // Write the counter as a 64-bit unsigned integer
        cursor.write_u128::<BigEndian>(timestamp).unwrap(); // Write the timestamp as a 128-bit unsigned integer
    }

    payload // Return the binary payload
}
