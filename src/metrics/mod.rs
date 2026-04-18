use prost::Message as _;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::model::Packet;

pub struct PingSource {
    pub target: String,
    pub history: Arc<Mutex<VecDeque<Packet>>>,
    pub mtu_history: Arc<Mutex<VecDeque<(u128, u32)>>>,
}

impl Clone for PingSource {
    fn clone(&self) -> Self {
        PingSource {
            target: self.target.clone(),
            history: Arc::clone(&self.history),
            mtu_history: Arc::clone(&self.mtu_history),
        }
    }
}

// ── Prometheus remote_write protobuf types ────────────────────────────────────

#[derive(Clone, PartialEq, prost::Message)]
struct WriteRequest {
    #[prost(message, repeated, tag = "1")]
    timeseries: Vec<TimeSeries>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct TimeSeries {
    #[prost(message, repeated, tag = "1")]
    labels: Vec<Label>,
    #[prost(message, repeated, tag = "2")]
    samples: Vec<Sample>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct Label {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(string, tag = "2")]
    value: String,
}

#[derive(Clone, PartialEq, prost::Message)]
struct Sample {
    #[prost(double, tag = "1")]
    value: f64,
    #[prost(int64, tag = "2")]
    timestamp: i64,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn make_ts(name: &str, extra_labels: &[(&str, &str)], value: f64, ts_ms: i64) -> TimeSeries {
    let mut labels = vec![Label {
        name: "__name__".into(),
        value: name.into(),
    }];
    for &(k, v) in extra_labels {
        labels.push(Label { name: k.into(), value: v.into() });
    }
    // Prometheus requires labels sorted by name.
    labels.sort_by(|a, b| a.name.cmp(&b.name));
    TimeSeries {
        labels,
        samples: vec![Sample { value, timestamp: ts_ms }],
    }
}

// ── Stats computation ─────────────────────────────────────────────────────────

struct Stats {
    sent: u64,
    received: u64,
    lost: u64,
    reordered: u64,
    duplicated: u64,
    rtt_min: Option<u64>,
    rtt_max: Option<u64>,
    rtt_median: Option<u64>,
}

fn compute_stats(queue: &VecDeque<Packet>) -> Stats {
    let now_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u128;
    let cutoff = now_us.saturating_sub(60 * 1_000_000); // RTT window: last 60 s

    let mut sent = 0u64;
    let mut received = 0u64;
    let mut lost = 0u64;
    let mut reordered = 0u64;
    let mut duplicated = 0u64;
    let mut recent_rtts: Vec<u64> = Vec::new();

    for p in queue {
        sent += 1;
        if p.duplicate {
            duplicated += 1;
        } else if p.is_lost() {
            lost += 1;
        } else {
            received += 1;
            if p.reordered {
                reordered += 1;
            }
            if p.timestamp >= cutoff {
                recent_rtts.push(p.latency);
            }
        }
    }

    let (rtt_min, rtt_max, rtt_median) = if recent_rtts.is_empty() {
        (None, None, None)
    } else {
        recent_rtts.sort_unstable();
        let n = recent_rtts.len();
        let median = if n % 2 == 0 {
            (recent_rtts[n / 2 - 1] + recent_rtts[n / 2]) / 2
        } else {
            recent_rtts[n / 2]
        };
        (
            Some(*recent_rtts.first().unwrap()),
            Some(*recent_rtts.last().unwrap()),
            Some(median),
        )
    };

    Stats { sent, received, lost, reordered, duplicated, rtt_min, rtt_max, rtt_median }
}

fn push_stats(
    series: &mut Vec<TimeSeries>,
    prefix: &str,
    extra: &[(&str, &str)],
    s: &Stats,
    ts_ms: i64,
) {
    series.push(make_ts(&format!("{prefix}_packets_sent_total"), extra, s.sent as f64, ts_ms));
    series.push(make_ts(
        &format!("{prefix}_packets_received_total"),
        extra,
        s.received as f64,
        ts_ms,
    ));
    series.push(make_ts(&format!("{prefix}_packets_lost_total"), extra, s.lost as f64, ts_ms));
    series.push(make_ts(
        &format!("{prefix}_packets_reordered_total"),
        extra,
        s.reordered as f64,
        ts_ms,
    ));
    series.push(make_ts(
        &format!("{prefix}_packets_duplicated_total"),
        extra,
        s.duplicated as f64,
        ts_ms,
    ));
    if let Some(v) = s.rtt_min {
        series.push(make_ts(&format!("{prefix}_rtt_min_microseconds"), extra, v as f64, ts_ms));
    }
    if let Some(v) = s.rtt_max {
        series.push(make_ts(&format!("{prefix}_rtt_max_microseconds"), extra, v as f64, ts_ms));
    }
    if let Some(v) = s.rtt_median {
        series.push(make_ts(
            &format!("{prefix}_rtt_median_microseconds"),
            extra,
            v as f64,
            ts_ms,
        ));
    }
}

// ── Push loop ─────────────────────────────────────────────────────────────────

pub async fn start_push_loop(
    mimir_url: String,
    history: Arc<Mutex<VecDeque<Packet>>>,
    loopback_mtu: Arc<Mutex<VecDeque<(u128, u32)>>>,
    ping_sources: Vec<PingSource>,
) {
    let client = reqwest::Client::new();
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.tick().await; // discard immediate first tick; wait a full interval

    loop {
        interval.tick().await;

        let ts_ms = now_ms();
        let mut series: Vec<TimeSeries> = Vec::new();

        // Loopback packet metrics
        {
            let q = history.lock().await;
            let stats = compute_stats(&q);
            push_stats(&mut series, "loopback", &[], &stats, ts_ms);
        }
        {
            let q = loopback_mtu.lock().await;
            if let Some(&(_, mtu)) = q.back() {
                series.push(make_ts("loopback_mtu_bytes", &[], mtu as f64, ts_ms));
            }
        }

        // Per-target ping metrics
        for src in &ping_sources {
            let target = src.target.as_str();
            let extra = &[("target", target)];
            {
                let q = src.history.lock().await;
                let stats = compute_stats(&q);
                push_stats(&mut series, "ping", extra, &stats, ts_ms);
            }
            {
                let q = src.mtu_history.lock().await;
                if let Some(&(_, mtu)) = q.back() {
                    series.push(make_ts("ping_mtu_bytes", extra, mtu as f64, ts_ms));
                }
            }
        }

        // Encode protobuf → snappy → HTTP POST
        let proto = WriteRequest { timeseries: series }.encode_to_vec();
        let body = match snap::raw::Encoder::new().compress_vec(&proto) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Mimir: snappy compression failed: {e}");
                continue;
            }
        };

        match client
            .post(&mimir_url)
            .header("Content-Type", "application/x-protobuf")
            .header("X-Prometheus-Remote-Write-Version", "0.1.0")
            .header("X-Scope-OrgID", "anonymous")
            .body(body)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {}
            Ok(r) => eprintln!("Mimir push failed: HTTP {}", r.status()),
            Err(e) => eprintln!("Mimir push error: {e}"),
        }
    }
}
