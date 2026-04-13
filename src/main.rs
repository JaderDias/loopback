mod config;
mod model;
mod network;
mod persistence;
mod web;

use dotenvy::dotenv;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use model::Packet;
use web::PingSource;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = config::load();

    // Random-ish session ID: low 32 bits of the startup timestamp in microseconds.
    // Prevents stale in-flight packets from a previous run (which carry a different
    // session ID) from being matched against the new run's history.
    let session_id: u32 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u32;
    println!("Session ID: 0x{:08X}", session_id);

    let sent_counter = Arc::new(AtomicU64::new(0));

    // ── Loopback history ───────────────────────────────────────────────────────
    let history: Arc<Mutex<VecDeque<Packet>>> =
        Arc::new(Mutex::new(persistence::load(&config.data_file)));

    let loopback_mtu: Arc<Mutex<VecDeque<(u128, u32)>>> =
        Arc::new(Mutex::new(persistence::load_mtu(&config.loopback_mtu_file())));

    // ── ICMP ping histories ────────────────────────────────────────────────────
    let ping_sources: Vec<PingSource> = config
        .ping_targets
        .iter()
        .map(|target| PingSource {
            target: target.clone(),
            history: Arc::new(Mutex::new(persistence::load(
                &config.ping_data_file_for(target),
            ))),
            mtu_history: Arc::new(Mutex::new(persistence::load_mtu(
                &config.ping_mtu_file_for(target),
            ))),
        })
        .collect();

    // ── Listener ───────────────────────────────────────────────────────────────
    {
        let sent_counter = Arc::clone(&sent_counter);
        let history = Arc::clone(&history);
        tokio::spawn(async move {
            network::listener::start_listener(
                config.target_port,
                session_id,
                sent_counter,
                history,
            )
            .await;
        });
    }

    // ── Sender (discovers public IP first) ────────────────────────────────────
    {
        let sent_counter = Arc::clone(&sent_counter);
        let history = Arc::clone(&history);
        let loopback_mtu = Arc::clone(&loopback_mtu);
        let config = config.clone();
        tokio::spawn(async move {
            let public_ip = loop {
                match network::ip::discover(config.alternative_interface.as_deref()).await {
                    Some(ip) => break ip,
                    None => {
                        eprintln!("Could not determine public IP, retrying in 30s...");
                        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                    }
                }
            };
            let address = format!("{}:{}", public_ip, config.target_port);
            {
                let loopback_mtu = Arc::clone(&loopback_mtu);
                let addr = address.clone();
                let min_mtu = config.min_mtu;
                let max_mtu = config.max_mtu;
                let max_queue_size = config.max_queue_size;
                tokio::spawn(async move {
                    network::mtu::start_probing_udp(
                        "0.0.0.0:0".to_string(),
                        addr,
                        min_mtu,
                        max_mtu,
                        max_queue_size,
                        loopback_mtu,
                    )
                    .await;
                });
            }
            network::sender::start_sending(
                &config,
                public_ip,
                session_id,
                sent_counter,
                history,
            )
            .await;
        });
    }

    // ── ICMP pingers + MTU probers ────────────────────────────────────────────
    for src in &ping_sources {
        let target = src.target.clone();
        let history = Arc::clone(&src.history);
        let mtu_history = Arc::clone(&src.mtu_history);
        let interval_millis = config.interval_millis;
        let max_packet_size = config.max_packet_size as u32;
        let max_queue_size = config.max_queue_size;
        let min_mtu = config.min_mtu;
        let max_mtu = config.max_mtu;

        tokio::spawn(async move {
            network::pinger::start_pinging(
                target.clone(),
                interval_millis,
                max_packet_size,
                max_queue_size,
                history,
            )
            .await;
        });

        let target2 = src.target.clone();
        tokio::spawn(async move {
            network::mtu::start_probing_icmp(
                target2,
                min_mtu,
                max_mtu,
                max_queue_size,
                mtu_history,
            )
            .await;
        });
    }

    // ── Web server ────────────────────────────────────────────────────────────
    {
        let history = Arc::clone(&history);
        let loopback_mtu = Arc::clone(&loopback_mtu);
        let ping_sources = ping_sources.clone();
        let web_port = config.web_port;
        tokio::spawn(async move {
            web::serve(web_port, history, loopback_mtu, ping_sources).await;
        });
    }

    // ── Periodic save: packet histories ───────────────────────────────────────
    {
        let history = Arc::clone(&history);
        let path = config.data_file.clone();
        tokio::spawn(async move {
            persistence::start_periodic_save(path, history).await;
        });
    }
    {
        let loopback_mtu = Arc::clone(&loopback_mtu);
        let path = config.loopback_mtu_file();
        tokio::spawn(async move {
            persistence::start_periodic_save_mtu(path, loopback_mtu).await;
        });
    }
    for src in &ping_sources {
        let path = config.ping_data_file_for(&src.target);
        let history = Arc::clone(&src.history);
        tokio::spawn(async move {
            persistence::start_periodic_save(path, history).await;
        });

        let path = config.ping_mtu_file_for(&src.target);
        let mtu_history = Arc::clone(&src.mtu_history);
        tokio::spawn(async move {
            persistence::start_periodic_save_mtu(path, mtu_history).await;
        });
    }

    println!("Program is running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");
    println!("Shutting down...");
    println!("Total packets sent: {}", sent_counter.load(Ordering::Relaxed));

    // ── Final save ────────────────────────────────────────────────────────────
    persistence::save(&config.data_file, &*history.lock().await);
    persistence::save_mtu(&config.loopback_mtu_file(), &*loopback_mtu.lock().await);
    println!("Data saved to {}", config.data_file);

    for src in &ping_sources {
        let path = config.ping_data_file_for(&src.target);
        persistence::save(&path, &*src.history.lock().await);
        let mtu_path = config.ping_mtu_file_for(&src.target);
        persistence::save_mtu(&mtu_path, &*src.mtu_history.lock().await);
        println!("Ping data for {} saved", src.target);
    }
}
