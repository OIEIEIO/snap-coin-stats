// =============================================================================
// static/stats.js
// snap-coin-stats/static/stats.js
// v0.6.0
// - Removed: Biggest Spender, Top Miner, Gini Coefficient chips + logic
// - Kept: New Wallets, Biggest Gainer, New Circulation
// =============================================================================

const ATOMIC     = 100_000_000;
const REFRESH_MS = 30 * 1000;
const EXPLORER   = 'https://explorer.snap-coin.net';

const PIE_COLORS = [
  '#f0a832','#e05555','#4caf82','#5b8fff','#c55cf0',
  '#f07832','#32d4f0','#f0d832','#a8e06a','#f05580',
  '#7af0c8','#f0a0c8','#8080f0','#f0c060','#60f0a0',
  '#e08040','#40c0e0','#e040a0','#80e040','#a040e0',
];

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

function fmtSnap(n) {
  if (typeof n !== 'number') return '—';
  return (n / ATOMIC).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

function fmtSnapCirculation(n) {
  if (typeof n !== 'number') return '—';
  return (n / ATOMIC).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) + ' SNAP';
}

function fmtSnapDelta(n) {
  if (typeof n !== 'number') return '—';
  const v = n / ATOMIC;
  const sign = v >= 0 ? '+' : '';
  return sign + v.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

function fmtNum(n) {
  if (typeof n !== 'number') return '—';
  return n.toLocaleString('en-US');
}

function truncAddr(s, len = 16) {
  if (!s) return '—';
  return s.length > len ? s.slice(0, len) + '…' : s;
}

// ---------------------------------------------------------------------------
// Balance delta indicator — ▲ green / ▼ red / ● dim
// ---------------------------------------------------------------------------

function deltaIndicator(curr, prev) {
  if (!prev) return '<span class="delta-none">●</span>';
  const delta = (curr || 0) - (prev || 0);
  if (delta > 0) return '<span class="delta-up">▲</span>';
  if (delta < 0) return '<span class="delta-dn">▼</span>';
  return '<span class="delta-none">●</span>';
}

// ---------------------------------------------------------------------------
// Dual-axis global line chart
// Circulation on left axis (amber), wallets+txids on right axis (green/blue)
// ---------------------------------------------------------------------------

function renderGlobalChart(records) {
  const wrap = document.getElementById('globalChart');
  if (!wrap) return;
  if (!records || records.length < 2) {
    wrap.innerHTML = '<div class="chart-empty">accumulating data…</div>';
    return;
  }

  const W    = wrap.clientWidth || 600;
  const H    = 160;
  const PAD  = { top: 14, right: 8, bottom: 32, left: 8 };
  const cw   = W - PAD.left - PAD.right;
  const ch   = H - PAD.top  - PAD.bottom;

  const series = [
    { key: 'circulation', label: 'Circulation', color: '#f0a832', divisor: ATOMIC,
      fmt: v => v.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) },
    { key: 'wallets',     label: 'Wallets',     color: '#4caf82', divisor: 1,
      fmt: v => Math.round(v).toLocaleString() },
    { key: 'tx_count',    label: 'TxIDs',       color: '#5b8fff', divisor: 1,
      fmt: v => Math.round(v).toLocaleString() },
  ];

  let svg = `<svg width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" xmlns="http://www.w3.org/2000/svg">`;

  // Grid lines
  for (let i = 0; i <= 4; i++) {
    const y = PAD.top + (ch / 4) * i;
    svg += `<line x1="${PAD.left}" y1="${y}" x2="${W - PAD.right}" y2="${y}"
      stroke="#2a2d35" stroke-width="1"/>`;
  }

  // Each series normalised independently to its own 0-100% range
  series.forEach(s => {
    const vals = records.map(r => (r[s.key] || 0) / s.divisor);
    const min  = Math.min(...vals);
    const max  = Math.max(...vals);
    const rng  = Math.max(max - min, max * 0.001, 1);

    const pts = vals.map((v, i) => {
      const x = PAD.left + (i / (vals.length - 1)) * cw;
      const y = PAD.top  + ch - ((v - min) / rng) * ch;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(' ');

    const lastPt  = pts.split(' ').slice(-1)[0].split(',');
    const [lx, ly] = lastPt;
    const lastVal  = vals[vals.length - 1];

    svg += `<polyline points="${pts}" fill="none" stroke="${s.color}" stroke-width="1.5" opacity="0.85"/>`;
    svg += `<circle cx="${lx}" cy="${ly}" r="3" fill="${s.color}"/>`;
    svg += `<text x="${(parseFloat(lx) + 6).toFixed(1)}" y="${(parseFloat(ly) + 3).toFixed(1)}"
      font-family="IBM Plex Mono,monospace" font-size="8" fill="${s.color}">${s.fmt(lastVal)}</text>`;
  });

  // Time range
  const fmt = d => new Date(d * 1000).toLocaleTimeString('en-US',
    { hour: '2-digit', minute: '2-digit', hour12: false });
  svg += `<text x="${PAD.left}" y="${H - 16}" font-family="IBM Plex Mono,monospace"
    font-size="9" fill="#7a8090">${fmt(records[0].timestamp)}</text>`;
  svg += `<text x="${W / 2}" y="${H - 16}" font-family="IBM Plex Mono,monospace"
    font-size="9" fill="#7a8090" text-anchor="middle">${records.length} snapshots</text>`;
  svg += `<text x="${W - PAD.right}" y="${H - 16}" font-family="IBM Plex Mono,monospace"
    font-size="9" fill="#7a8090" text-anchor="end">${fmt(records[records.length - 1].timestamp)}</text>`;

  // Legend
  let lx = PAD.left;
  series.forEach(s => {
    svg += `<rect x="${lx}" y="${PAD.top}" width="8" height="8" rx="2" fill="${s.color}" opacity="0.85"/>`;
    svg += `<text x="${lx + 11}" y="${PAD.top + 8}" font-family="IBM Plex Mono,monospace"
      font-size="9" fill="#7a8090">${s.label}</text>`;
    lx += 82;
  });

  svg += '</svg>';
  wrap.innerHTML = svg;
}

// ---------------------------------------------------------------------------
// Data store
// ---------------------------------------------------------------------------

let _history = [];

// ---------------------------------------------------------------------------
// Load globals
// ---------------------------------------------------------------------------

async function loadGlobals(init = false) {
  try {
    const url = init ? '/api/globals?init=1' : '/api/globals';
    const r = await fetch(url);
    const g = await r.json();
    document.getElementById('gWallets').textContent     = fmtNum(g.wallets_with_unspent);
    document.getElementById('gTxids').textContent       = fmtNum(g.tx_entries_scanned);
    document.getElementById('gOutputs').textContent     = fmtNum(g.outputs_total);
    document.getElementById('gUnspent').textContent     = fmtNum(g.outputs_unspent);
    document.getElementById('gCirculation').textContent = fmtSnapCirculation(g.utxo_total_unspent_atomic);
  } catch (e) { console.error('globals error', e); }
}

// ---------------------------------------------------------------------------
// Intel chips — new wallets, biggest gainer, top sender, new circulation
// ---------------------------------------------------------------------------

function computeIntel(history) {
  if (!history || history.length < 2) return null;
  const prev = history[history.length - 2];
  const curr = history[history.length - 1];

  const prevAddrs  = new Set(Object.keys(prev.balances));
  const currAddrs  = new Set(Object.keys(curr.balances));
  const newWallets = [...currAddrs].filter(a => !prevAddrs.has(a)).length;

  const allAddrs = new Set([...prevAddrs, ...currAddrs]);
  let gainerAddr = '', gainerDelta = 0;
  let senderAddr = '', senderDelta = 0;
  for (const addr of allAddrs) {
    const delta = (curr.balances[addr] || 0) - (prev.balances[addr] || 0);
    if (delta > gainerDelta) { gainerDelta = delta; gainerAddr = addr; }
    if (delta < senderDelta) { senderDelta = delta; senderAddr = addr; }
  }

  const newCirc = curr.circulation - prev.circulation;

  return { newWallets, gainerAddr, gainerDelta, senderAddr, senderDelta, newCirc };
}

function renderIntelChips(intel) {
  const loading = document.getElementById('intelLoading');
  const chips   = document.getElementById('intelChips');

  if (!intel) {
    loading.style.display = 'flex';
    chips.style.display   = 'none';
    return;
  }

  document.getElementById('ic-newwallets-val').textContent = '+' + fmtNum(intel.newWallets);

  const gainerEl = document.getElementById('ic-gainer-val');
  gainerEl.textContent = fmtSnapDelta(intel.gainerDelta);
  gainerEl.className   = 'ichip-val green';
  document.getElementById('ic-gainer-sub').textContent = truncAddr(intel.gainerAddr);

  const senderEl = document.getElementById('ic-sender-val');
  senderEl.textContent = fmtSnapDelta(intel.senderDelta);
  senderEl.className   = intel.senderDelta < 0 ? 'ichip-val red' : 'ichip-val';
  document.getElementById('ic-sender-sub').textContent = truncAddr(intel.senderAddr);

  const circEl = document.getElementById('ic-newcirc-val');
  circEl.textContent = fmtSnapDelta(intel.newCirc);
  circEl.className   = intel.newCirc >= 0 ? 'ichip-val green' : 'ichip-val red';

  loading.style.display = 'none';
  chips.style.display   = 'flex';
}

// ---------------------------------------------------------------------------
// Load history
// ---------------------------------------------------------------------------

async function loadHistory() {
  try {
    const r    = await fetch('/api/history');
    const data = await r.json();
    _history   = data.records || [];

    document.getElementById('snapCounter').textContent = 'snapshot #' + _history.length;

    if (_history.length > 0) {
      const last = _history[_history.length - 1];
      document.getElementById('snapshotAge').textContent =
        new Date(last.timestamp * 1000).toUTCString().replace('GMT', 'UTC');
    }

    renderGlobalChart(_history);
    renderIntelChips(computeIntel(_history));
    return _history;
  } catch (e) {
    console.error('history error', e);
    return [];
  }
}

// ---------------------------------------------------------------------------
// Load top receivers
// ---------------------------------------------------------------------------

async function loadTopReceivers() {
  try {
    const r    = await fetch('/api/top_receivers');
    const rows = await r.json();
    if (!rows || !rows.length) return;
    const total = rows.reduce((s, r) => s + r.total_unspent, 0);
    document.getElementById('recvMeta').textContent    = 'total ' + fmtSnap(total);
    document.getElementById('recvPieMeta').textContent = rows.length + ' receivers shown';
    renderReceiverList(rows, total, _history);
    renderPie(rows, total);
  } catch (e) { console.error('top receivers error', e); }
}

// ---------------------------------------------------------------------------
// Receiver list — arrow indicator, no sparkline
// ---------------------------------------------------------------------------

function renderReceiverList(rows, total, history) {
  const card = document.getElementById('recvListCard');
  card.querySelectorAll('.recv-row').forEach(el => el.remove());

  const prev = history.length >= 2 ? history[history.length - 2] : null;
  const curr = history.length >= 1 ? history[history.length - 1] : null;

  rows.forEach((row, i) => {
    const pct   = total > 0 ? (row.total_unspent / total * 100) : 0;
    const color = PIE_COLORS[i % PIE_COLORS.length];
    const href  = `${EXPLORER}/wallet/${row.receiver_base36}`;

    const currBal = curr ? (curr.balances[row.receiver_base36] || 0) : null;
    const prevBal = prev ? (prev.balances[row.receiver_base36] || 0) : null;
    const arrow   = deltaIndicator(currBal, prevBal);

    const el = document.createElement('div');
    el.className = 'recv-row';
    el.innerHTML = `
      <span class="recv-rank">#${i + 1}</span>
      <a class="recv-addr" href="${href}" target="_blank" rel="noopener">${row.receiver_base36}</a>
      <span class="recv-pct">${pct.toFixed(2)}% ${arrow}</span>
      <span class="recv-amount">${fmtSnap(row.total_unspent)}</span>
      <div class="recv-bar-wrap" style="grid-column:1/-1">
        <div class="recv-bar" style="width:${pct.toFixed(2)}%;background:${color}"></div>
      </div>
    `;
    card.appendChild(el);
  });
}

// ---------------------------------------------------------------------------
// Pie chart
// ---------------------------------------------------------------------------

function renderPie(rows, total) {
  const wrap = document.getElementById('pieWrap');
  wrap.innerHTML = '';

  const size  = 300;
  const cx    = size / 2;
  const cy    = size / 2;
  const r     = 120;
  const inner = 72;

  let svg = `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">`;
  svg += `<text x="${cx}" y="${cy - 8}" text-anchor="middle"
    font-family="IBM Plex Mono,monospace" font-size="28" font-weight="600"
    fill="#eef0f4">${rows.length}</text>`;
  svg += `<text x="${cx}" y="${cy + 14}" text-anchor="middle"
    font-family="IBM Plex Mono,monospace" font-size="9" fill="#7a8090"
    letter-spacing="0.08em">RECEIVERS</text>`;

  let startAngle = -Math.PI / 2;
  rows.forEach((row, i) => {
    const frac  = total > 0 ? row.total_unspent / total : 0;
    const angle = frac * 2 * Math.PI;
    const end   = startAngle + angle;
    const large = angle > Math.PI ? 1 : 0;
    const color = PIE_COLORS[i % PIE_COLORS.length];
    const x1  = cx + r * Math.cos(startAngle), y1 = cy + r * Math.sin(startAngle);
    const x2  = cx + r * Math.cos(end),        y2 = cy + r * Math.sin(end);
    const xi1 = cx + inner * Math.cos(startAngle), yi1 = cy + inner * Math.sin(startAngle);
    const xi2 = cx + inner * Math.cos(end),        yi2 = cy + inner * Math.sin(end);
    svg += `<path d="M ${xi1} ${yi1} L ${x1} ${y1} A ${r} ${r} 0 ${large} 1 ${x2} ${y2} L ${xi2} ${yi2} A ${inner} ${inner} 0 ${large} 0 ${xi1} ${yi1} Z"
      fill="${color}" opacity="0.88" stroke="var(--bg2)" stroke-width="1"/>`;
    startAngle = end;
  });
  svg += '</svg>';

  let legend = '<div class="pie-legend">';
  rows.slice(0, 18).forEach((row, i) => {
    const pct   = total > 0 ? (row.total_unspent / total * 100).toFixed(1) : '0.0';
    const color = PIE_COLORS[i % PIE_COLORS.length];
    legend += `<div class="pie-legend-row">
      <span class="pie-legend-rank">#${i + 1}</span>
      <span class="pie-legend-pct">${pct}%</span>
      <div class="pie-legend-dot" style="background:${color}"></div>
      <span class="pie-legend-addr">${truncAddr(row.receiver_base36, 22)}</span>
    </div>`;
  });
  legend += '</div>';

  wrap.innerHTML = `<div class="pie-inner">${legend}${svg}</div>`;
}

// ---------------------------------------------------------------------------
// Load visits
// ---------------------------------------------------------------------------

async function loadVisits() {
  try {
    const r    = await fetch('/api/visits');
    const data = await r.json();
    document.getElementById('footerPageLoads').textContent = fmtNum(data.page_loads);
  } catch (e) { console.error('visits error', e); }
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

async function refresh(init = false) {
  await loadHistory();
  await Promise.all([loadGlobals(init), loadTopReceivers(), loadVisits()]);
}

document.addEventListener('DOMContentLoaded', () => {
  refresh(true);
  setInterval(refresh, REFRESH_MS);
});

// =============================================================================
// static/stats.js
// snap-coin-stats/static/stats.js
// Created: 2026-03-21T00:00:00Z
// =============================================================================