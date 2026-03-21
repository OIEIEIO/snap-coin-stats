// =============================================================================
// src/db.rs
// snap-coin-stats/src/db.rs
// v0.4.0
// Sled query helpers: globals, wallet intelligence, top receivers, history.
// AppState now holds RwLock<sled::Db>, history ring buffer, and page load counter.
// =============================================================================

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

use snap_coin::core::transaction::{TransactionId, TransactionOutput};

// ---------------------------------------------------------------------------
// History record — one entry per snapshot cycle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotRecord {
    pub timestamp:       u64,
    pub circulation:     u64,
    pub wallets:         u64,
    pub tx_count:        u64,
    pub outputs_total:   u64,
    pub outputs_unspent: u64,
    pub balances:        HashMap<String, u64>,
}

pub const HISTORY_MAX: usize = 288;

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub db:         Arc<RwLock<sled::Db>>,
    pub history:    Arc<RwLock<VecDeque<SnapshotRecord>>>,
    pub page_loads: Arc<std::sync::atomic::AtomicU64>,
}

// ---------------------------------------------------------------------------
// GlobalMetrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct GlobalMetrics {
    pub tx_entries_scanned:        u64,
    pub outputs_total:             u64,
    pub outputs_unspent:           u64,
    pub outputs_spent:             u64,
    pub wallets_with_unspent:      u64,
    pub utxo_total_unspent_atomic: u64,
}

pub fn global_metrics(db: &sled::Db) -> Result<GlobalMetrics> {
    let mut tx_entries_scanned:        u64 = 0;
    let mut outputs_total:             u64 = 0;
    let mut outputs_unspent:           u64 = 0;
    let mut outputs_spent:             u64 = 0;
    let mut utxo_total_unspent_atomic: u64 = 0;
    let mut receivers_with_unspent: HashSet<String> = HashSet::new();

    for item in db.iter() {
        let (k, v) = item?;
        let Ok(txid_str) = String::from_utf8(k.to_vec()) else { continue };
        let Some(_) = TransactionId::new_from_base36(&txid_str) else { continue };
        let Ok((outputs, _)) = bincode::decode_from_slice::<Vec<Option<TransactionOutput>>, _>(
            &v, bincode::config::standard(),
        ) else { continue };

        tx_entries_scanned += 1;
        outputs_total += outputs.len() as u64;

        for o in outputs.into_iter() {
            match o {
                Some(out) => {
                    outputs_unspent += 1;
                    utxo_total_unspent_atomic =
                        utxo_total_unspent_atomic.saturating_add(out.amount);
                    receivers_with_unspent.insert(out.receiver.dump_base36());
                }
                None => { outputs_spent += 1; }
            }
        }
    }

    Ok(GlobalMetrics {
        tx_entries_scanned,
        outputs_total,
        outputs_unspent,
        outputs_spent,
        wallets_with_unspent: receivers_with_unspent.len() as u64,
        utxo_total_unspent_atomic,
    })
}

// ---------------------------------------------------------------------------
// full_scan — globals + per-address balances in one pass
// ---------------------------------------------------------------------------

pub fn full_scan(db: &sled::Db) -> Result<SnapshotRecord> {
    let mut tx_count:        u64 = 0;
    let mut outputs_total:   u64 = 0;
    let mut outputs_unspent: u64 = 0;
    let mut circulation:     u64 = 0;
    let mut balances: HashMap<String, u64> = HashMap::new();

    for item in db.iter() {
        let (k, v) = item?;
        let Ok(txid_str) = String::from_utf8(k.to_vec()) else { continue };
        let Some(_) = TransactionId::new_from_base36(&txid_str) else { continue };
        let Ok((outputs, _)) = bincode::decode_from_slice::<Vec<Option<TransactionOutput>>, _>(
            &v, bincode::config::standard(),
        ) else { continue };

        tx_count      += 1;
        outputs_total += outputs.len() as u64;

        for o in outputs.into_iter() {
            if let Some(out) = o {
                outputs_unspent += 1;
                circulation = circulation.saturating_add(out.amount);
                let entry = balances.entry(out.receiver.dump_base36()).or_insert(0);
                *entry = entry.saturating_add(out.amount);
            }
        }
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(SnapshotRecord {
        timestamp,
        circulation,
        wallets: balances.len() as u64,
        tx_count,
        outputs_total,
        outputs_unspent,
        balances,
    })
}

// ---------------------------------------------------------------------------
// WalletIntelligence
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct AddrAccum {
    tx_count:                u64,
    unspent_count:           u64,
    spent_count:             u64,
    total_unspent_atomic:    u64,
    largest_output:          u64,
    baseline_unspent_atomic: u64,
    baseline_set:            bool,
    first_seen_txid:         String,
    last_seen_txid:          String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletIntelligence {
    pub most_active_address:        String,
    pub most_active_tx_count:       u64,
    pub most_fragmented_address:    String,
    pub most_fragmented_unspent:    u64,
    pub fastest_growing_address:    String,
    pub fastest_growing_delta:      i64,
    pub highest_spend_rate_address: String,
    pub highest_spend_rate_pct:     f64,
    pub highest_spend_rate_spent:   u64,
    pub highest_spend_rate_total:   u64,
    pub largest_output_amount:      u64,
    pub largest_output_txid:        String,
    pub largest_output_receiver:    String,
}

pub fn wallet_intelligence(db: &sled::Db) -> Result<WalletIntelligence> {
    let mut map: HashMap<String, AddrAccum> = HashMap::new();
    let mut largest_output_amount:   u64    = 0;
    let mut largest_output_txid:     String = String::new();
    let mut largest_output_receiver: String = String::new();

    for item in db.iter() {
        let (k, v) = item?;
        let Ok(txid_str) = String::from_utf8(k.to_vec()) else { continue };
        let Some(_) = TransactionId::new_from_base36(&txid_str) else { continue };
        let Ok((outputs, _)) = bincode::decode_from_slice::<Vec<Option<TransactionOutput>>, _>(
            &v, bincode::config::standard(),
        ) else { continue };

        for o in &outputs {
            if let Some(out) = o {
                let addr = out.receiver.dump_base36();
                let acc  = map.entry(addr.clone()).or_default();
                acc.tx_count      += 1;
                acc.unspent_count += 1;
                acc.total_unspent_atomic =
                    acc.total_unspent_atomic.saturating_add(out.amount);
                if out.amount > acc.largest_output { acc.largest_output = out.amount; }
                if acc.first_seen_txid.is_empty() { acc.first_seen_txid = txid_str.clone(); }
                acc.last_seen_txid = txid_str.clone();
                if !acc.baseline_set {
                    acc.baseline_unspent_atomic =
                        acc.baseline_unspent_atomic.saturating_add(out.amount);
                }
                if out.amount > largest_output_amount {
                    largest_output_amount   = out.amount;
                    largest_output_txid     = txid_str.clone();
                    largest_output_receiver = addr;
                }
            }
        }
        for o in &outputs {
            if let Some(out) = o {
                if let Some(acc) = map.get_mut(&out.receiver.dump_base36()) {
                    acc.baseline_set = true;
                }
            }
        }
    }

    let most_active = map.iter().max_by_key(|(_, a)| a.tx_count)
        .ok_or_else(|| anyhow!("no data"))?;
    let most_fragmented = map.iter().max_by_key(|(_, a)| a.unspent_count)
        .ok_or_else(|| anyhow!("no data"))?;
    let fastest_growing = map.iter()
        .max_by_key(|(_, a)| a.total_unspent_atomic as i64 - a.baseline_unspent_atomic as i64)
        .ok_or_else(|| anyhow!("no data"))?;
    let highest_spend_rate = map.iter()
        .filter(|(_, a)| (a.unspent_count + a.spent_count) > 0)
        .max_by(|(_, a), (_, b)| {
            let ra = a.spent_count as f64 / (a.unspent_count + a.spent_count) as f64;
            let rb = b.spent_count as f64 / (b.unspent_count + b.spent_count) as f64;
            ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| anyhow!("no data"))?;

    let hsr       = highest_spend_rate.1;
    let hsr_total = hsr.unspent_count + hsr.spent_count;
    let hsr_pct   = if hsr_total > 0 { hsr.spent_count as f64 / hsr_total as f64 * 100.0 } else { 0.0 };
    let fg_delta  = fastest_growing.1.total_unspent_atomic as i64 - fastest_growing.1.baseline_unspent_atomic as i64;

    Ok(WalletIntelligence {
        most_active_address:        most_active.0.clone(),
        most_active_tx_count:       most_active.1.tx_count,
        most_fragmented_address:    most_fragmented.0.clone(),
        most_fragmented_unspent:    most_fragmented.1.unspent_count,
        fastest_growing_address:    fastest_growing.0.clone(),
        fastest_growing_delta:      fg_delta,
        highest_spend_rate_address: highest_spend_rate.0.clone(),
        highest_spend_rate_pct:     hsr_pct,
        highest_spend_rate_spent:   hsr.spent_count,
        highest_spend_rate_total:   hsr_total,
        largest_output_amount,
        largest_output_txid,
        largest_output_receiver,
    })
}

// ---------------------------------------------------------------------------
// TopReceivers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TopReceiverRow {
    pub receiver_base36: String,
    pub total_unspent:   u64,
}

pub fn top_receivers(db: &sled::Db, limit: usize) -> Result<Vec<TopReceiverRow>> {
    let mut map: HashMap<String, u64> = HashMap::new();
    for item in db.iter() {
        let (_k, v) = item?;
        let Ok((outputs, _)) = bincode::decode_from_slice::<Vec<Option<TransactionOutput>>, _>(
            &v, bincode::config::standard(),
        ) else { continue };
        for o in outputs.into_iter().flatten() {
            let entry = map.entry(o.receiver.dump_base36()).or_insert(0);
            *entry = entry.saturating_add(o.amount);
        }
    }
    let mut rows: Vec<TopReceiverRow> = map
        .into_iter()
        .map(|(receiver_base36, total_unspent)| TopReceiverRow { receiver_base36, total_unspent })
        .collect();
    rows.sort_by(|a, b| b.total_unspent.cmp(&a.total_unspent));
    rows.truncate(limit);
    Ok(rows)
}

// =============================================================================
// src/db.rs
// snap-coin-stats/src/db.rs
// Created: 2026-03-21T00:00:00Z
// =============================================================================