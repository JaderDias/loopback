use chrono::{DateTime, Datelike, Timelike, Utc};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::model::{Packet, MAX_LATENCY_MICROS};

struct MinuteBucket {
    ts_js: String,
    latencies: Vec<u64>, // only received (non-lost) packets
    total: u32,
    lost: u32,
    reordered: u32,
    duplicated: u32,
}

fn minute_key(dt: &DateTime<Utc>) -> String {
    format!(
        "{},{},{},{},{}",
        dt.year(),
        dt.month0(),
        dt.day(),
        dt.hour(),
        dt.minute()
    )
}

fn js_date(dt: &DateTime<Utc>) -> String {
    format!(
        "new Date({},{},{},{},{})",
        dt.year(),
        dt.month0(),
        dt.day(),
        dt.hour(),
        dt.minute()
    )
}

async fn buckets_for(history: Arc<Mutex<VecDeque<Packet>>>) -> Vec<MinuteBucket> {
    let queue = history.lock().await;
    let mut result: Vec<MinuteBucket> = Vec::new();
    let mut current_key = String::new();

    for p in queue.iter() {
        let system_time = UNIX_EPOCH + Duration::from_micros(p.timestamp as u64);
        let dt: DateTime<Utc> = system_time.into();
        let key = minute_key(&dt);

        if key != current_key {
            result.push(MinuteBucket {
                ts_js: js_date(&dt),
                latencies: Vec::new(),
                total: 0,
                lost: 0,
                reordered: 0,
                duplicated: 0,
            });
            current_key = key;
        }

        let bucket = result.last_mut().unwrap();
        bucket.total += 1;
        if p.duplicate {
            bucket.duplicated += 1;
        } else if p.is_lost() {
            bucket.lost += 1;
        } else {
            bucket.latencies.push(p.latency);
            if p.reordered {
                bucket.reordered += 1;
            }
        }
    }

    result
}

fn median(v: &mut Vec<u64>) -> u64 {
    if v.is_empty() {
        return MAX_LATENCY_MICROS;
    }
    v.sort_unstable();
    let mid = v.len() / 2;
    if v.len() % 2 == 0 {
        (v[mid - 1] + v[mid]) / 2
    } else {
        v[mid]
    }
}

/// JS rows for the latency chart: [Date, min, max, median]
pub async fn latency_rows(history: Arc<Mutex<VecDeque<Packet>>>) -> String {
    let buckets = buckets_for(history).await;
    let mut rows = Vec::new();
    for mut b in buckets {
        if b.latencies.is_empty() {
            continue;
        }
        b.latencies.sort_unstable();
        let min = *b.latencies.first().unwrap();
        let max = *b.latencies.last().unwrap();
        let med = median(&mut b.latencies);
        rows.push(format!("[{},{},{},{}]", b.ts_js, min, max, med));
    }
    rows.join(",\n")
}

/// JS rows for the quality chart: [Date, loss_pct, reorders, duplicates]
pub async fn quality_rows(history: Arc<Mutex<VecDeque<Packet>>>) -> String {
    let buckets = buckets_for(history).await;
    let mut rows = Vec::new();
    for b in buckets {
        if b.total == 0 {
            continue;
        }
        let loss_pct = (b.lost as f64 / b.total as f64) * 100.0;
        rows.push(format!(
            "[{},{:.2},{},{}]",
            b.ts_js, loss_pct, b.reordered, b.duplicated
        ));
    }
    rows.join(",\n")
}

/// JS rows for an MTU chart: [Date, mtu]
pub async fn mtu_rows(history: Arc<Mutex<VecDeque<(u128, u32)>>>) -> String {
    let queue = history.lock().await;
    let mut rows = Vec::new();
    for &(ts, mtu) in queue.iter() {
        let system_time = UNIX_EPOCH + Duration::from_micros(ts as u64);
        let dt: DateTime<Utc> = system_time.into();
        rows.push(format!("[{},{}]", js_date(&dt), mtu));
    }
    rows.join(",\n")
}
