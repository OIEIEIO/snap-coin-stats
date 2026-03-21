// =============================================================================
// src/api.rs
// snap-coin-stats/src/api.rs
// v0.4.0
// Axum HTTP handlers — globals, wallet intelligence, top receivers, history, visits.
// api_globals increments the page load counter on each call.
// GET /api/visits returns the total page load count.
// All DB reads go through RwLock read guard.
// =============================================================================

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

use crate::db::{self, AppState};

// ---------------------------------------------------------------------------
// UI index
// ---------------------------------------------------------------------------

pub async fn ui_index() -> impl IntoResponse {
    let html = include_str!("../static/stats.html");
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(html.to_owned())
        .unwrap()
}

// ---------------------------------------------------------------------------
// GET /api/globals?init=1
// Increments page load counter only on initial page load (init=1).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct GlobalsQuery {
    #[serde(default)]
    pub init: u8,
}

pub async fn api_globals(
    State(state): State<AppState>,
    Query(q): Query<GlobalsQuery>,
) -> Response {
    if q.init == 1 {
        state.page_loads.fetch_add(1, Ordering::Relaxed);
        let count = state.page_loads.load(Ordering::Relaxed);
        let _ = std::fs::write("./page_loads.txt", count.to_string());
    }

    let db = state.db.read().await;
    match db::global_metrics(&db) {
        Ok(g)  => Json(g).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/wallet_intelligence
// ---------------------------------------------------------------------------

pub async fn api_wallet_intelligence(State(state): State<AppState>) -> Response {
    let db = state.db.read().await;
    match db::wallet_intelligence(&db) {
        Ok(wi) => Json(wi).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/top_receivers?limit=N
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TopQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize { usize::MAX }

pub async fn api_top_receivers(
    State(state): State<AppState>,
    Query(q): Query<TopQuery>,
) -> Response {
    let db = state.db.read().await;
    match db::top_receivers(&db, q.limit) {
        Ok(rows) => Json(rows).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/history
// Returns the rolling history buffer as JSON.
// Each record has: timestamp, circulation, wallets, tx_count,
//                  outputs_total, outputs_unspent, balances{addr->atomic}
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct HistoryResponse {
    pub records: Vec<HistoryPoint>,
}

#[derive(Serialize)]
pub struct HistoryPoint {
    pub timestamp:       u64,
    pub circulation:     u64,
    pub wallets:         u64,
    pub tx_count:        u64,
    pub outputs_total:   u64,
    pub outputs_unspent: u64,
    pub balances:        std::collections::HashMap<String, u64>,
}

pub async fn api_history(State(state): State<AppState>) -> Response {
    let hist = state.history.read().await;
    let records: Vec<HistoryPoint> = hist.iter().map(|r| HistoryPoint {
        timestamp:       r.timestamp,
        circulation:     r.circulation,
        wallets:         r.wallets,
        tx_count:        r.tx_count,
        outputs_total:   r.outputs_total,
        outputs_unspent: r.outputs_unspent,
        balances:        r.balances.clone(),
    }).collect();
    Json(HistoryResponse { records }).into_response()
}

// ---------------------------------------------------------------------------
// GET /api/visits
// Returns total page load count.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct VisitsResponse {
    pub page_loads: u64,
}

pub async fn api_visits(State(state): State<AppState>) -> Response {
    let count = state.page_loads.load(Ordering::Relaxed);
    Json(VisitsResponse { page_loads: count }).into_response()
}

// =============================================================================
// src/api.rs
// snap-coin-stats/src/api.rs
// Created: 2026-03-21T00:00:00Z
// =============================================================================