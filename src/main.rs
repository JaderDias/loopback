mod config;
mod network;

use dotenvy::dotenv;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = config::load();

    // Global counters
    let sent_counter = Arc::new(AtomicU64::new(0));
    let received_counter = Arc::new(AtomicU64::new(0));

    // Spawn the network listener
    let received_counter_clone = Arc::clone(&received_counter);
    tokio::spawn(async move {
        network::listener::start_listener(config.listen_port, received_counter_clone).await;
    });

    // Spawn the network sender
    let sent_counter_clone = Arc::clone(&sent_counter);
    tokio::spawn(async move {
        network::sender::start_sending(
            config.alternative_interface,
            config.public_ip_address,
            config.listen_port,
            config.min_packet_size,
            config.max_packet_size,
            config.interval_millis,
            sent_counter_clone,
        )
        .await;
    });

    println!("Program is running. Press Ctrl+C to stop.");

    // Wait for a termination signal (e.g., Ctrl+C)
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");

    println!("Shutting down...");
    println!(
        "Total packets sent: {}",
        sent_counter.load(Ordering::Relaxed)
    );
    println!(
        "Total packets received: {}",
        received_counter.load(Ordering::Relaxed)
    );
}
