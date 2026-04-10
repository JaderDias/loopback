pub const MAX_LATENCY_MICROS: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct Packet {
    pub timestamp: u128, // microseconds since epoch (when sent)
    pub latency: u64,    // round-trip in microseconds; MAX_LATENCY_MICROS = lost/pending
    pub size: u32,       // payload size in bytes
    pub reordered: bool, // arrived after a packet with a higher sequence number
    pub duplicate: bool, // a copy of this packet was already received
}

impl Packet {
    pub fn pending(timestamp: u128, size: u32) -> Self {
        Self {
            timestamp,
            latency: MAX_LATENCY_MICROS,
            size,
            reordered: false,
            duplicate: false,
        }
    }

    pub fn is_lost(&self) -> bool {
        self.latency >= MAX_LATENCY_MICROS
    }
}
