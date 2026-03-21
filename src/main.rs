// =============================================================================
// src/main.rs
// snap-coin-stats/src/main.rs
// v0.4.0
// Entry point: open snapshot sled DB, build Axum router, serve.
// Background task: rsync every 5 min, reload DB, run full_scan, push to history.
// Page load counter loaded from ./page_loads.txt on startup, persisted on each hit.
// =============================================================================

mod api;
mod db;

use anyhow::{anyhow, Context, Result};
use axum::{routing::get, Router};
use std::{collections::VecDeque, net::SocketAddr, sync::Arc};
use std::sync::atomic::AtomicU64;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

pub use db::AppState;

// ---------------------------------------------------------------------------
// Rsync helper
// ---------------------------------------------------------------------------

async fn run_rsync(src: &str, dst: &str) -> bool {
    let tmp = format!("{}.tmp", dst);
    match tokio::process::Command::new("rsync")
        .args(["-a", src, &tmp])
        .status()
        .await
    {
        Ok(s) if s.success() => {
            match tokio::fs::rename(&tmp, dst).await {
                Ok(_)  => { println!("[snapshot] rsync done -> {dst}"); true }
                Err(e) => { eprintln!("[snapshot] rename failed: {e}"); false }
            }
        }
        Ok(s)  => { eprintln!("[snapshot] rsync status: {s}"); false }
        Err(e) => { eprintln!("[snapshot] rsync error: {e}"); false }
    }
}

// ---------------------------------------------------------------------------
// Background reload + history loop
// ---------------------------------------------------------------------------

async fn reload_loop(
    state:    AppState,
    src:      String,
    dst:      String,
    db_path:  String,
    interval: u64,
) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;

        // 1. rsync
        if !run_rsync(&src, &dst).await {
            eprintln!("[reload] rsync failed, skipping reload");
            continue;
        }

        // 2. reopen DB
        match sled::open(&db_path) {
            Ok(new_db) => {
                // 3. full scan for history record
                match db::full_scan(&new_db) {
                    Ok(record) => {
                        let mut hist = state.history.write().await;
                        if hist.len() >= db::HISTORY_MAX { hist.pop_front(); }
                        hist.push_back(record);
                        println!("[reload] history now {} records", hist.len());
                    }
                    Err(e) => eprintln!("[reload] full_scan error: {e}"),
                }
                // 4. swap DB handle
                *state.db.write().await = new_db;
                println!("[reload] DB reloaded");
            }
            Err(e) => eprintln!("[reload] sled open failed: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let db_path   = std::env::var("SNAP_STATS_DB")
        .unwrap_or_else(|_| "./snapshot".to_string());
    let bind_addr = std::env::var("SNAP_STATS_BIND")
        .unwrap_or_else(|_| "127.0.0.1:5338".to_string());
    let snap_src  = std::env::var("SNAP_STATS_SRC")
        .unwrap_or_else(|_| {
            "/home/jorge/snap-coin-node/node-mainnet/blockchain/blockchain/db".to_string()
        });
    let snap_dst  = std::env::var("SNAP_STATS_DST")
        .unwrap_or_else(|_| "./snapshot/db".to_string());
    let interval  = std::env::var("SNAP_STATS_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300u64); // 5 minutes

    std::fs::create_dir_all("./snapshot")?;

    // Load persisted page load count
    let saved_loads: u64 = std::fs::read_to_string("./page_loads.txt")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    println!("[visits] page loads restored: {saved_loads}");

    // Initial rsync
    let snap_tmp = format!("{}.tmp", snap_dst);
    println!("[snapshot] initial sync: {snap_src} -> {snap_tmp}");
    let status = std::process::Command::new("rsync")
        .args(["-a", &snap_src, &snap_tmp])
        .status()
        .with_context(|| "rsync not found")?;
    if !status.success() {
        eprintln!("[snapshot] initial rsync failed — using existing snapshot if present");
    } else {
        std::fs::rename(&snap_tmp, &snap_dst)
            .with_context(|| format!("rename {snap_tmp} -> {snap_dst} failed"))?;
        println!("[snapshot] initial sync done");
    }

    // Open DB
    println!("opening snapshot db at: {db_path}");
    let database = sled::open(&db_path)
        .with_context(|| format!("failed to open sled snapshot at {db_path}"))?;

    // Initial full scan -> seed history
    let mut history = VecDeque::new();
    match db::full_scan(&database) {
        Ok(record) => {
            println!("[snapshot] initial scan: {} wallets, {} atomic circulation",
                record.wallets, record.circulation);
            history.push_back(record);
        }
        Err(e) => eprintln!("[snapshot] initial scan error: {e}"),
    }

    let state = AppState {
        db:         Arc::new(RwLock::new(database)),
        history:    Arc::new(RwLock::new(history)),
        page_loads: Arc::new(AtomicU64::new(saved_loads)),
    };

    // Spawn reload loop
    tokio::spawn(reload_loop(
        state.clone(),
        snap_src,
        snap_dst,
        db_path,
        interval,
    ));

    let app = Router::new()
        .route("/",                        get(api::ui_index))
        .nest_service("/static",           ServeDir::new("static"))
        .route("/api/globals",             get(api::api_globals))
        .route("/api/wallet_intelligence", get(api::api_wallet_intelligence))
        .route("/api/top_receivers",       get(api::api_top_receivers))
        .route("/api/history",             get(api::api_history))
        .route("/api/visits",              get(api::api_visits))
        .with_state(state);

    let addr: SocketAddr = bind_addr.parse()
        .map_err(|e| anyhow!("invalid bind address: {e}"))?;

    println!("snap-coin-stats serving on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

// =============================================================================
// src/main.rs
// snap-coin-stats/src/main.rs
// Created: 2026-03-21T00:00:00Z
// Version: v0.4.0
// =============================================================================