use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub public_ip_address: String,
    pub alternative_interface: String,
    pub max_payload_size: usize,
    pub min_payload_size: usize,
    pub interval_millis: u64,
    pub listen_port: u16,
}

pub fn load() -> Config {
    Config {
        public_ip_address: env::var("PUBLIC_IP_ADDRESS").expect("PUBLIC_IP_ADDRESS must be set"),
        alternative_interface: env::var("ALTERNATIVE_INTERFACE")
            .expect("ALTERNATIVE_INTERFACE must be set"),
        max_payload_size: env::var("MAX_PAYLOAD_SIZE")
            .expect("MAX_PAYLOAD_SIZE must be set")
            .parse()
            .expect("MAX_PAYLOAD_SIZE must be a number"),
        min_payload_size: env::var("MIN_PAYLOAD_SIZE")
            .expect("MIN_PAYLOAD_SIZE must be set")
            .parse()
            .expect("MIN_PAYLOAD_SIZE must be a number"),
        interval_millis: env::var("INTERVAL_MILLIS")
            .expect("INTERVAL_MILLIS must be set")
            .parse()
            .expect("INTERVAL_MILLIS must be a number"),
        listen_port: env::var("LISTEN_PORT")
            .expect("LISTEN_PORT must be set")
            .parse()
            .expect("LISTEN_PORT must be a number"),
    }
}
