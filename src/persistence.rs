use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time;

use crate::model::Packet;

// Magic headers — first byte 0xFF is safe: valid u128 timestamps always start with 0x00
const PACKET_MAGIC: [u8; 4] = [0xFF, b'L', b'B', 1];
const MTU_MAGIC: [u8; 4] = [0xFF, b'M', b'T', 1];

const THIRTY_DAYS_MICROS: u128 = 30 * 24 * 60 * 60 * 1_000_000;

fn cutoff_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
        .saturating_sub(THIRTY_DAYS_MICROS)
}

// ── Packet history ────────────────────────────────────────────────────────────

pub fn load(path: &str) -> VecDeque<Packet> {
    let path = Path::new(path);
    if !path.exists() {
        return VecDeque::new();
    }

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open {}: {}", path.display(), e);
            return VecDeque::new();
        }
    };

    let mut reader = BufReader::new(file);
    let mut magic = [0u8; 4];

    let records = match reader.read_exact(&mut magic) {
        Ok(()) if magic == PACKET_MAGIC => load_packets_new(&mut reader),
        Ok(()) if magic[0] == 0x00 => {
            // Old format: seek back and read (u128, u64) records
            if reader.seek(SeekFrom::Start(0)).is_err() {
                return VecDeque::new();
            }
            load_packets_old(&mut reader)
        }
        _ => VecDeque::new(),
    };

    println!("Loaded {} records from {}", records.len(), path.display());
    records
}

fn load_packets_new(reader: &mut BufReader<File>) -> VecDeque<Packet> {
    let cutoff = cutoff_micros();
    let mut records = VecDeque::new();
    loop {
        let timestamp = match reader.read_u128::<BigEndian>() {
            Ok(v) => v,
            Err(_) => break,
        };
        let latency = match reader.read_u64::<BigEndian>() {
            Ok(v) => v,
            Err(_) => break,
        };
        let size = match reader.read_u32::<BigEndian>() {
            Ok(v) => v,
            Err(_) => break,
        };
        let flags = match reader.read_u8() {
            Ok(v) => v,
            Err(_) => break,
        };
        if timestamp >= cutoff {
            records.push_back(Packet {
                timestamp,
                latency,
                size,
                reordered: flags & 0b01 != 0,
                duplicate: flags & 0b10 != 0,
            });
        }
    }
    records
}

fn load_packets_old(reader: &mut BufReader<File>) -> VecDeque<Packet> {
    let cutoff = cutoff_micros();
    let mut records = VecDeque::new();
    loop {
        let timestamp = match reader.read_u128::<BigEndian>() {
            Ok(v) => v,
            Err(_) => break,
        };
        let latency = match reader.read_u64::<BigEndian>() {
            Ok(v) => v,
            Err(_) => break,
        };
        if timestamp >= cutoff {
            records.push_back(Packet {
                timestamp,
                latency,
                size: 0, // unknown in old format
                reordered: false,
                duplicate: false,
            });
        }
    }
    records
}

pub fn save(path: &str, history: &VecDeque<Packet>) {
    let tmp = format!("{}.tmp", path);
    let result = (|| -> std::io::Result<()> {
        let file = File::create(&tmp)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&PACKET_MAGIC)?;
        for p in history {
            writer.write_u128::<BigEndian>(p.timestamp)?;
            writer.write_u64::<BigEndian>(p.latency)?;
            writer.write_u32::<BigEndian>(p.size)?;
            let flags = (p.reordered as u8) | ((p.duplicate as u8) << 1);
            writer.write_u8(flags)?;
        }
        Ok(())
    })();
    commit(result, &tmp, path);
}

pub async fn start_periodic_save(path: String, history: Arc<Mutex<VecDeque<Packet>>>) {
    let mut interval = time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let mut queue = history.lock().await;
        let cutoff = cutoff_micros();
        let before = queue.len();
        queue.retain(|p| p.timestamp >= cutoff);
        let removed = before - queue.len();
        if removed > 0 {
            println!("Removed {} records older than 30 days from {}", removed, path);
        }
        save(&path, &queue);
    }
}

// ── MTU history ───────────────────────────────────────────────────────────────

pub fn load_mtu(path: &str) -> VecDeque<(u128, u32)> {
    let path = Path::new(path);
    if !path.exists() {
        return VecDeque::new();
    }
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open {}: {}", path.display(), e);
            return VecDeque::new();
        }
    };
    let mut reader = BufReader::new(file);
    let mut magic = [0u8; 4];
    if reader.read_exact(&mut magic).is_err() || magic != MTU_MAGIC {
        return VecDeque::new();
    }
    let cutoff = cutoff_micros();
    let mut records = VecDeque::new();
    loop {
        let ts = match reader.read_u128::<BigEndian>() {
            Ok(v) => v,
            Err(_) => break,
        };
        let mtu = match reader.read_u32::<BigEndian>() {
            Ok(v) => v,
            Err(_) => break,
        };
        if ts >= cutoff {
            records.push_back((ts, mtu));
        }
    }
    println!("Loaded {} MTU records from {}", records.len(), path.display());
    records
}

pub fn save_mtu(path: &str, history: &VecDeque<(u128, u32)>) {
    let tmp = format!("{}.tmp", path);
    let result = (|| -> std::io::Result<()> {
        let file = File::create(&tmp)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&MTU_MAGIC)?;
        for &(ts, mtu) in history {
            writer.write_u128::<BigEndian>(ts)?;
            writer.write_u32::<BigEndian>(mtu)?;
        }
        Ok(())
    })();
    commit(result, &tmp, path);
}

pub async fn start_periodic_save_mtu(path: String, history: Arc<Mutex<VecDeque<(u128, u32)>>>) {
    let mut interval = time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let mut queue = history.lock().await;
        let cutoff = cutoff_micros();
        queue.retain(|&(ts, _)| ts >= cutoff);
        save_mtu(&path, &queue);
    }
}

// ── Shared helper ─────────────────────────────────────────────────────────────

fn commit(result: std::io::Result<()>, tmp: &str, dest: &str) {
    match result {
        Ok(()) => {
            if let Err(e) = fs::rename(tmp, dest) {
                eprintln!("Failed to rename {} → {}: {}", tmp, dest, e);
            }
        }
        Err(e) => {
            eprintln!("Failed to write {}: {}", dest, e);
            let _ = fs::remove_file(tmp);
        }
    }
}
