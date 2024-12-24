mod config;
mod network;

use dotenvy::dotenv;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = config::load();

    tokio::spawn(async move {
        network::listener::start_listener(config.listen_port).await;
    });

    let socket = network::sender::create_socket(config.alternative_interface);
//    loop {
        network::sender::send(
            &socket,
            &config.public_ip_address,
            config.listen_port,
            config.min_payload_size,
        );
//    }

    println!("Program is running. Press Ctrl+C to stop.");

    // Wait for a termination signal (e.g., Ctrl+C)
    tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

 
}
