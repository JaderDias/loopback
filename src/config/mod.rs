use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub alternative_interface: String,
    pub interval_millis: u64,
    pub max_packet_size: usize,
    pub max_queue_size: usize,
    pub public_ip_address: String,
    pub target_port: u16,
    pub web_port: u16,
}

pub fn load() -> Config {
    Config {
        public_ip_address: env::var("PUBLIC_IP_ADDRESS").expect("PUBLIC_IP_ADDRESS must be set"),
        alternative_interface: env::var("ALTERNATIVE_INTERFACE")
            .expect("ALTERNATIVE_INTERFACE must be set"),
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
