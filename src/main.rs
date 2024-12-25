mod config;
mod model;
mod network;
mod web;

use dotenvy::dotenv;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = config::load();

    // Global counters
    let sent_counter = Arc::new(AtomicU64::new(0));
    let history = Arc::new(Mutex::new(VecDeque::new()));

    // Spawn the network listener
    {
        let sent_counter_clone = Arc::clone(&sent_counter);
        let history_clone = Arc::clone(&history);
        tokio::spawn(async move {
            network::listener::start_listener(
                config.target_port,
                sent_counter_clone,
                history_clone,
            )
            .await;
        });
    }

    // Spawn the network sender
    {
        let sent_counter_clone = Arc::clone(&sent_counter);
        let history_clone = Arc::clone(&history);
        tokio::spawn(async move {
            network::sender::start_sending(
                config.alternative_interface,
                config.public_ip_address,
                config.target_port,
                config.min_packet_size,
                config.max_packet_size,
                config.interval_millis,
                sent_counter_clone,
                history_clone,
            )
            .await;
        });
    }

    let history_clone = Arc::clone(&history);
    tokio::spawn(async move {
        web::serve(config.web_port, history_clone).await;
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
}
