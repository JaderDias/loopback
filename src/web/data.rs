use chrono::Datelike;
use chrono::{DateTime, Timelike, Utc};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::Mutex;

pub async fn group_by(history: Arc<Mutex<VecDeque<crate::model::Result>>>) -> String {
    let mut rows = vec![];
    let minutes = group_by_minute(history).await;
    for (minute, min, max, median) in minutes {
        let row = format!("[new Date({minute}), {min}, {max}, {median}]",);

        rows.push(row);
    }
    rows.join(",\n")
}

fn process_group(minute: &str, latencies: &mut Vec<u64>) -> (String, u64, u64, u64) {
    latencies.sort_unstable();
    let min = *latencies.first().unwrap();
    let max = *latencies.last().unwrap();
    let median = if latencies.len() % 2 == 0 {
        let mid = latencies.len() / 2;
        (latencies[mid - 1] + latencies[mid]) / 2
    } else {
        latencies[latencies.len() / 2]
    };

    latencies.clear();
    (minute.to_string(), min, max, median)
}

async fn group_by_minute(
    shared_results: Arc<Mutex<VecDeque<crate::model::Result>>>,
) -> Vec<(String, u64, u64, u64)> {
    let mut result_stats = Vec::new();
    let mut current_minute: Option<String> = None;
    let mut current_latencies = Vec::new();

    let results = shared_results.lock().await;
    for &(sent, latency_millis) in results.iter() {
        let duration_since_epoch = Duration::from_micros(sent as u64);
        let system_time = UNIX_EPOCH + duration_since_epoch;

        let sent: DateTime<Utc> = system_time.into();
        let minute_key = format!(
            "{}, {}, {}, {}, {}",
            sent.year(),
            sent.month0(),
            sent.day(),
            sent.hour(),
            sent.minute()
        );

        if let Some(ref current) = current_minute {
            if current != &minute_key {
                result_stats.push(process_group(current, &mut current_latencies));
                current_minute = Some(minute_key);
            }
        } else {
            current_minute = Some(minute_key);
        }

        current_latencies.push(latency_millis);
    }

    // Process the final group
    if let Some(minute) = current_minute {
        result_stats.push(process_group(&minute, &mut current_latencies));
    }

    result_stats
}
