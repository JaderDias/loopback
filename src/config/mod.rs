use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub alternative_interface: Option<String>,
    pub data_file: String,
    pub interval_millis: u64,
    pub max_mtu: u32,
    pub max_packet_size: usize,
    pub max_queue_size: usize,
    pub min_mtu: u32,
    pub ping_data_file: String,
    pub ping_targets: Vec<String>,
    pub target_port: u16,
    pub web_port: u16,
}

impl Config {
    /// Derive a per-target packet history path.
    pub fn ping_data_file_for(&self, target: &str) -> String {
        let base = self
            .ping_data_file
            .strip_suffix(".bin")
            .unwrap_or(&self.ping_data_file);
        format!("{}_{}.bin", base, target)
    }

    /// Derive the UDP loopback MTU history path from the main data file.
    pub fn loopback_mtu_file(&self) -> String {
        let base = self
            .data_file
            .strip_suffix(".bin")
            .unwrap_or(&self.data_file);
        format!("{}_mtu.bin", base)
    }

    /// Derive a per-target ICMP MTU history path.
    pub fn ping_mtu_file_for(&self, target: &str) -> String {
        let base = self
            .ping_data_file
            .strip_suffix(".bin")
            .unwrap_or(&self.ping_data_file);
        format!("{}_{}_mtu.bin", base, target)
    }
}

pub fn load() -> Config {
    let ping_targets = env::var("PING_TARGET")
        .unwrap_or_else(|_| "1.1.1.1".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Config {
        data_file: env::var("DATA_FILE")
            .unwrap_or_else(|_| "/var/lib/loopback/data.bin".to_string()),
        ping_data_file: env::var("PING_DATA_FILE")
            .unwrap_or_else(|_| "/var/lib/loopback/ping_data.bin".to_string()),
        ping_targets,
        alternative_interface: Some(
            env::var("ALTERNATIVE_INTERFACE").unwrap_or_else(|_| "wgproton".to_string()),
        )
        .filter(|s| !s.is_empty()),
        min_mtu: env::var("MIN_MTU")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(576),
        max_mtu: env::var("MAX_MTU")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1512),
        max_packet_size: env::var("MAX_PACKET_SIZE")
            .expect("MAX_PACKET_SIZE must be set")
            .parse()
            .expect("MAX_PACKET_SIZE must be a number"),
        max_queue_size: env::var("MAX_QUEUE_SIZE")
            .expect("MAX_QUEUE_SIZE must be set")
            .parse()
            .expect("MAX_QUEUE_SIZE must be a number"),
        interval_millis: env::var("INTERVAL_MILLIS")
            .expect("INTERVAL_MILLIS must be set")
            .parse()
            .expect("INTERVAL_MILLIS must be a number"),
        target_port: env::var("TARGET_PORT")
            .expect("TARGET_PORT must be set")
            .parse()
            .expect("TARGET_PORT must be a number"),
        web_port: env::var("WEB_PORT")
            .expect("WEB_PORT must be set")
            .parse()
            .expect("WEB_PORT must be a number"),
    }
}
