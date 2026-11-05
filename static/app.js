import * as CM from '/vendor/codemirror.js';
import { createRedisUI } from '/redis.js';

// Rowveil — single-page UI (no build step).

// ---------------------------------------------------------------- helpers
const $ = (sel, root = document) => root.querySelector(sel);

function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  let value;
  for (const [k, v] of Object.entries(props || {})) {
    if (v == null || v === false) continue;
    if (k === 'class') el.className = v;
    else if (k === 'style' && typeof v === 'object') Object.assign(el.style, v);
    else if (k === 'html') el.innerHTML = v;
    else if (k === 'value') value = v;
    else if (k.startsWith('on') && typeof v === 'function') el.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k === 'checked' || k === 'disabled' || k === 'selected' || k === 'indeterminate') el[k] = !!v;
    else if (k === 'dataset') Object.assign(el.dataset, v);
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const kid of kids.flat(Infinity)) {
    if (kid == null || kid === false) continue;
    el.append(kid instanceof Node ? kid : String(kid));
  }
  if (value !== undefined) el.value = value;
  return el;
}

const ICONS = {
  table: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 10h18M3 15h18M9 10v10"/>',
  view: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  refresh: '<path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/>',
  filter: '<path d="M3 5h18l-7 8v6l-4 2v-8z"/>',
  columns: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16M15 4v16"/>',
  bookmark: '<path d="M6 3h12v18l-6-4.5L6 21z"/>',
  trash: '<path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14"/>',
  left: '<path d="m15 18-6-6 6-6"/>',
  right: '<path d="m9 18 6-6-6-6"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  play: '<path d="M7 4v16l13-8z"/>',
  db: '<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5"/><path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"/>',
  terminal: '<path d="m4 17 6-5-6-5M12 19h8"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  moon: '<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>',
  link: '<path d="M7 17 17 7M8 7h9v9"/>',
  key: '<circle cx="7.5" cy="15.5" r="4.5"/><path d="m10.7 12.3 9.3-9.3M16 7l3 3"/>',
  download: '<path d="M12 3v12M7 10l5 5 5-5M4 21h16"/>',
  structure: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/><path d="M6.5 10v4a3 3 0 0 0 3 3H14"/>',
  chevron: '<path d="m6 9 6 6 6-6"/>',
  expand: '<path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/>',
  lock: '<rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>',
  history: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5M12 7v5l3 2"/>',
  copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/>',
  undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h11a5 5 0 0 1 0 10h-3"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  eyeOff: '<path d="M10.6 5.1A10.4 10.4 0 0 1 12 5c6.5 0 10 7 10 7a17.7 17.7 0 0 1-2.2 3.2M6.6 6.6C3.8 8.4 2 12 2 12s3.5 7 10 7c1.9 0 3.6-.6 5-1.5"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/><path d="M3 3l18 18"/>',
  wand: '<path d="M15 4V2M15 16v-2M8 9h2M20 9h2M17.8 11.8 19 13M15 9h.01M17.8 6.2 19 5M3 21l9-9M12.2 6.2 11 5"/>',
  keyboard: '<rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  list: '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>',
  shield: '<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/>',
  home: '<path d="M3 11l9-7 9 7"/><path d="M5 10v10h14V10"/><path d="M10 20v-6h4v6"/>',
  edit: '<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M14 6l4 4"/>',
  external: '<path d="M14 4h6v6M20 4l-9 9M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5"/>',
  schema: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/><path d="M6.5 10v4a2 2 0 0 0 2 2H14"/>',
  palette: '<path d="M12 22a10 10 0 1 1 10-10c0 2.8-2.2 4-4 4h-2a2 2 0 0 0-1.5 3.3A1.6 1.6 0 0 1 12 22z"/><circle cx="7.5" cy="10.5" r="1"/><circle cx="10.5" cy="7" r="1"/><circle cx="15" cy="7.5" r="1"/>',
};

function icon(name, size = 15) {
  const span = document.createElement('span');
  span.className = 'icon';
  span.innerHTML = `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${ICONS[name] || ''}</svg>`;
  return span;
}

const store = {
  get(key, fallback) {
    try { const v = localStorage.getItem(key); return v == null ? fallback : JSON.parse(v); } catch { return fallback; }
  },
  set(key, value) {
    try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* storage unavailable */ }
    prefsChanged();
  },
};

// Desktop app: the page origin changes at each launch (a new port), so localStorage starts empty.
// Copy the rs.* keys to the server; it puts them back into the next page (see src/prefs.rs).
let prefsTimer = null;
function prefsChanged() {
  try { if (!state.info?.desktop) return; } catch { return; }
  clearTimeout(prefsTimer);
  prefsTimer = setTimeout(syncPrefs, 400);
}
function prefsSnapshot() {
  const items = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k.startsWith('rs.') && k !== 'rs.__rev') items[k] = localStorage.getItem(k);
    }
  } catch { /* storage unavailable */ }
  return items;
}
async function syncPrefs() {
  prefsTimer = null;
  try {
    const { rev } = await api('/prefs', { method: 'PUT', body: { items: prefsSnapshot() } });
    localStorage.setItem('rs.__rev', String(rev));
  } catch { /* the next change tries again */ }
}
// A change in the last moment before the window closes.
window.addEventListener('pagehide', () => {
  if (!prefsTimer) return;
  clearTimeout(prefsTimer);
  prefsTimer = null;
  fetch('/api/prefs', { method: 'PUT', keepalive: true, headers: { 'content-type': 'application/json' }, body: JSON.stringify({ items: prefsSnapshot() }) }).catch(() => {});
});

async function api(path, { method, body, signal } = {}) {
  let res;
  try {
    res = await fetch('/api' + path, {
      method: method || (body !== undefined ? 'POST' : 'GET'),
      headers: body !== undefined ? { 'content-type': 'application/json' } : {},
      body: body !== undefined ? JSON.stringify(body) : undefined,
      signal,
    });
  } catch (e) {
    if (e?.name === 'AbortError') throw e;
    throw new Error(state.info?.desktop ? 'The Rowveil engine stopped. Restart the app.' : `Cannot reach the Rowveil server at ${location.host}. It may have stopped: start it again and reload the page.`);
  }
  const text = await res.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch { data = { error: text }; }
  if (!res.ok) throw new Error((data && data.error) || `${res.status} ${res.statusText}`);
  return data;
}

function toast(message, kind = 'info', action = null) {
  const el = h('div', { class: `toast ${kind}` }, h('span', {}, message),
    action ? h('button', { class: 'toast-action', onclick: () => { action.run(); el.remove(); } }, action.label) : null);
  $('#toasts').append(el);
  const ttl = action ? 7000 : kind === 'error' ? 6000 : 2800;
  setTimeout(() => el.classList.add('hide'), ttl);
  setTimeout(() => el.remove(), ttl + 400);
}

const fmtNum = (n) => (n == null ? '?' : Number(n).toLocaleString('en-US'));

function download(filename, text, mime) {
  if (state.info?.desktop) { saveWithDialog(filename, text); return; }
  const url = URL.createObjectURL(new Blob([text], { type: mime }));
  const a = h('a', { href: url, download: filename });
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** Desktop app: native "Save as" dialog, then a toast with "Show in folder". */
async function saveWithDialog(filename, text) {
  try {
    const r = await api('/desktop/save', { body: { filename, content: text } });
    if (r.cancelled) return;
    toast(`Saved: ${r.path}`, 'success', { label: 'Show in folder', run: () => api('/desktop/reveal', { body: { path: r.path } }).catch(() => {}) });
  } catch (e) { toast(e.message, 'error'); }
}

function toCSV(columns, rows) {
  const esc = (v) => {
    if (v == null) return '';
    const s = typeof v === 'object' ? JSON.stringify(v) : String(v);
    return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  return [columns.map(esc).join(','), ...rows.map((r) => r.map(esc).join(','))].join('\n');
}

function rowsToObjects(columns, rows) {
  return rows.map((r) => Object.fromEntries(columns.map((c, i) => [c, r[i]])));
}

function copyText(text) {
  navigator.clipboard?.writeText(text).then(() => toast('Copied to clipboard'), () => toast('Copy failed', 'error'));
}

// ---------------------------------------------------------------- state
const state = {
  info: null,
  connections: [],
  connId: null,
  schema: null,
  schemaName: null,
  search: '',
  tabs: [],
  activeTab: null,
  savedQueries: [],
  loadingSchema: false,
  schemaError: null,
};
let tabSeq = 1;
let sqlSeq = 1;

const conn = () => state.connections.find((c) => c.id === state.connId);
const isReadOnly = () => !!(state.schema && state.schema.readOnly);
const findTable = (schema, name) => state.schema?.tables.find((t) => t.schema === schema && t.name === name);
const activeTab = () => state.tabs.find((t) => t.id === state.activeTab);

const DIALECT_LABEL = { postgres: 'PostgreSQL', mysql: 'MySQL', sqlite: 'SQLite', bigquery: 'BigQuery', redis: 'Redis' };
const isRedis = () => state.schema?.dialect === 'redis';
// Dialects that quote identifiers with backticks and allow "double-quoted" strings and # comments.
const backtickSql = () => ['mysql', 'bigquery'].includes(state.schema?.dialect);
const schemaWord = (d) => ({ mysql: 'Database', bigquery: 'Dataset' }[d] || 'Schema');

// ---------------------------------------------------------------- persistence of tabs
function saveTabs() {
  if (!state.connId) return;
  const tabs = state.tabs.map((t) => {
    if (t.type === 'sql') return { type: 'sql', title: t.title, sql: t.sql, savedId: t.savedId || undefined, resHidden: t.resHidden?.names.length ? t.resHidden : undefined, resWidths: t.resWidths?.sig ? t.resWidths : undefined };
    if (t.type === 'schema') return { type: 'schema', section: t.section, sel: t.sel, schemaFilter: t.schemaFilter };
    if (t.type === 'rkey' || t.type === 'rcli') return R.saveTab(t);
    return { type: 'table', schema: t.schema, table: t.table, filters: t.filters, sort: t.sort, pageSize: t.pageSize, hidden: t.hidden, widths: t.widths, view: t.view };
  });
  const activeIdx = state.tabs.findIndex((t) => t.id === state.activeTab);
  store.set(`rs.tabs.${state.connId}`, { tabs, active: activeIdx, schemaName: state.schemaName });
}

function restoreTabs() {
  const saved = store.get(`rs.tabs.${state.connId}`, null);
  state.tabs = [];
  state.activeTab = null;
  if (!saved) return;
  if (saved.schemaName && state.schema.schemas.includes(saved.schemaName)) state.schemaName = saved.schemaName;
  for (const t of saved.tabs || []) {
    if (t.type === 'sql') state.tabs.push(Object.assign(newSqlTab(t.title, t.sql), { savedId: t.savedId || null, resHidden: t.resHidden?.sig ? t.resHidden : null, resWidths: t.resWidths?.sig ? t.resWidths : null }));
    else if (t.type === 'schema') state.tabs.push(newSchemaTab(t));
    else if (t.type === 'rkey' || t.type === 'rcli') { if (isRedis()) state.tabs.push(R.restoreTab(t)); }
    else if (findTable(t.schema, t.table)) state.tabs.push(newTableTab(t.schema, t.table, t));
  }
  const active = state.tabs[saved.active] || state.tabs[0];
  state.activeTab = active ? active.id : null;
}

function newTableTab(schema, table, opts = {}) {
  return {
    id: tabSeq++,
    type: 'table',
    schema,
    table,
    page: 0,
    pageSize: opts.pageSize || 50,
    filters: opts.filters || [],
    filterDraft: null,
    sort: opts.sort || [],
    hidden: opts.hidden || [],
    widths: opts.widths || {},
    view: opts.view || 'data',
    data: null,
    loading: false,
    error: null,
    pending: { updates: new Map(), inserts: [], deletes: new Map() },
    selected: new Set(),
    sel: null,
    drawer: null,
    popover: null,
  };
}

function newSqlTab(title, sql = '') {
  return { id: tabSeq++, type: 'sql', title: title || `SQL ${sqlSeq++}`, sql, results: null, error: null, running: false, activeResult: 0, duration: null, resHidden: null, resWidths: null };
}

// ---------------------------------------------------------------- boot
async function boot() {
  initSidebarResizer();
  document.addEventListener('keydown', globalKeys);
  try {
    state.info = await api('/info');
    state.connections = await api('/connections');
    await loadSavedQueries();
  } catch (e) {
    toast(e.message, 'error');
  }
  // The URL hash selects the connection, so each browser tab can show its own database.
  // Without a connection in the URL, the start page opens (or the last connection, when the user chose that).
  const last = store.get('rs.conn', null);
  const pick = state.connections.find((c) => c.id === hashConn())
    || (store.get('rs.openLast', false) ? state.connections.find((c) => c.id === last) : null);
  window.addEventListener('hashchange', () => {
    const id = hashConn();
    if (!id) { if (state.connId) goHome(); return; }
    if (id !== state.connId && state.connections.some((c) => c.id === id)) selectConnection(id);
  });
  if (pick) await selectConnection(pick.id);
  else render();
  setTimeout(() => checkForUpdates({ auto: true }), 3000);
  setInterval(() => checkForUpdates({ auto: true }), 6 * 3600 * 1000);
}

// ---------------------------------------------------------------- updates
const updateAvailable = () => !!(state.update?.available && state.update.latest !== store.get('rs.skipVersion', null));

async function checkForUpdates({ auto = false, force = false } = {}) {
  if (auto && !store.get('rs.autoUpdate', true)) return state.update;
  try {
    state.update = await api(`/update/check${force ? '?force=1' : ''}`);
  } catch (e) {
    if (!auto) throw e;
    return null;
  }
  renderSidebar();
  if (auto && updateAvailable() && store.get('rs.updateToasted', null) !== state.update.latest) {
    store.set('rs.updateToasted', state.update.latest);
    toast(`Rowveil ${state.update.latest} is available`, 'info', { label: 'Details', run: () => openUpdateModal() });
  }
  return state.update;
}

function openUpdateModal() {
  const body = h('div', { class: 'form update-modal' });
  const foot = h('div', { class: 'update-foot' });
  let timer = null;
  const close = openModal('Updates', body, foot, { wide: true, onClose: () => clearInterval(timer) });
  const auto = h('input', { type: 'checkbox', checked: store.get('rs.autoUpdate', true), onchange: (e) => store.set('rs.autoUpdate', e.target.checked) });

  const fmtMb = (n) => `${(n / 1048576).toFixed(1)} MB`;
  const draw = (u, progress = null, error = null) => {
    body.replaceChildren();
    foot.replaceChildren();
    const cur = state.info?.version;
    if (!u) {
      body.append(h('p', { class: error ? 'error-text' : 'muted' }, error || 'Checking for updates…'));
    } else if (!u.available) {
      body.append(h('div', { class: 'update-head' }, icon('check', 18), h('div', {}, h('b', {}, 'Rowveil is up to date'), h('div', { class: 'muted small' }, `Version ${cur}. Latest release: ${u.latest}.`))));
    } else {
      body.append(h('div', { class: 'update-head' }, icon('download', 18),
        h('div', {}, h('b', {}, `Rowveil ${u.latest} is available`), h('div', { class: 'muted small' }, `You have ${cur}.${u.size ? ` Download: ${fmtMb(u.size)}.` : ''}`))));
      if (u.release?.notes) body.append(h('pre', { class: 'update-notes' }, u.release.notes.trim()));
      if (progress && progress.phase === 'downloading') {
        const pct = progress.total ? Math.min(100, Math.round((progress.received / progress.total) * 100)) : 0;
        body.append(h('div', { class: 'update-progress' }, h('div', { style: { width: `${pct}%` } })),
          h('div', { class: 'muted small' }, `Downloading… ${fmtMb(progress.received)}${progress.total ? ` of ${fmtMb(progress.total)}` : ''}`));
      }
      if (progress?.phase === 'ready') {
        body.append(h('div', { class: 'form-status ok' }, u.method === 'msi'
          ? 'Downloaded and verified. Rowveil closes, Windows asks for permission once, installs the update and starts Rowveil again.'
          : 'Downloaded and verified. Rowveil restarts with the new version.'));
      }
      if (progress?.phase === 'restarting') body.append(h('div', { class: 'form-status' }, h('span', { class: 'spinner' }), ' Restarting…'));
      if (progress?.phase === 'error' || error) body.append(h('div', { class: 'form-status error' }, error || progress.error));
      if (u.method === 'none') {
        body.append(h('div', { class: 'hint' }, 'This is the web server. Update it like any other server: pull the new Docker image (', h('code', {}, 'docker compose pull && docker compose up -d'), ') or download the new binary.'));
      } else if (!u.canInstall) {
        body.append(h('div', { class: 'hint' }, 'This install cannot update itself (for example, a system package). Download the new version from the release page.'));
      }
    }
    body.append(h('label', { class: 'check' }, auto, 'Check for updates automatically'));

    const btns = [];
    if (u?.available) {
      btns.push(h('a', { class: 'btn ghost', href: u.release.url, target: '_blank', rel: 'noopener' }, icon('external', 13), 'Release page'));
      btns.push(h('button', { class: 'btn ghost', onclick: () => { store.set('rs.skipVersion', u.latest); close(); renderSidebar(); } }, 'Skip this version'));
      if (u.canInstall) {
        const phase = progress?.phase || 'idle';
        if (phase === 'ready') btns.push(h('button', { class: 'btn primary', onclick: () => restart() }, icon('refresh', 13), 'Restart and update'));
        else if (phase === 'downloading' || phase === 'restarting') btns.push(h('button', { class: 'btn primary', disabled: true }, h('span', { class: 'spinner' }), phase === 'restarting' ? 'Restarting…' : 'Downloading…'));
        else btns.push(h('button', { class: 'btn primary', onclick: () => download() }, icon('download', 13), phase === 'error' ? 'Try again' : 'Download and install'));
      }
    } else {
      btns.push(h('button', { class: 'btn', onclick: () => load(true) }, icon('refresh', 13), 'Check now'));
    }
    btns.push(h('button', { class: 'btn', onclick: () => close() }, 'Close'));
    foot.append(...btns);
  };

  const poll = () => {
    clearInterval(timer);
    timer = setInterval(async () => {
      try {
        const p = await api('/update/progress');
        draw(state.update, p);
        if (p.phase !== 'downloading') clearInterval(timer);
      } catch { clearInterval(timer); }
    }, 400);
  };
  const download = async () => {
    try { const p = await api('/update/download', { body: {} }); draw(state.update, p); poll(); } catch (e) { draw(state.update, null, e.message); }
  };
  const restart = async () => {
    draw(state.update, { phase: 'restarting' });
    try { await api('/update/restart', { body: {} }); } catch (e) { draw(state.update, null, e.message); }
  };
  const load = async (force) => {
    draw(null);
    try {
      const u = await checkForUpdates({ force });
      const p = await api('/update/progress').catch(() => null);
      draw(u, p && p.version === u.latest ? p : null);
      if (p?.phase === 'downloading') poll();
    } catch (e) { draw(null, null, e.message); }
  };
  load(false);
}

function hashConn() {
  return decodeURIComponent(location.hash.replace(/^#\/?/, ''));
}

function connUrl(id) {
  return `${location.pathname}#/${encodeURIComponent(id)}`;
}

// The start page: no connection is selected.
function goHome() {
  if (state.connId) saveTabs();
  state.connId = null;
  state.schema = null;
  state.schemaError = null;
  state.loadingSchema = false;
  state.tabs = [];
  state.activeTab = null;
  if (location.hash) history.replaceState(null, '', location.pathname + location.search);
  document.title = 'Rowveil';
  render();
}

async function selectConnection(id, refresh = false) {
  if (state.connId && state.connId !== id) saveTabs();
  state.connId = id;
  store.set('rs.conn', id);
  if (hashConn() !== id) history.replaceState(null, '', connUrl(id));
  const c = state.connections.find((x) => x.id === id);
  document.title = c ? `${c.name} — Rowveil` : 'Rowveil';
  state.schema = null;
  state.schemaError = null;
  state.loadingSchema = true;
  state.tabs = [];
  state.activeTab = null;
  render();
  try {
    state.schema = await api(`/connections/${id}/schema${refresh ? '?refresh=1' : ''}`);
    state.schemaName = state.schema.defaultSchema;
    if (!state.schema.schemas.includes(state.schemaName)) state.schemaName = state.schema.schemas[0] || '';
    if (isRedis()) R.onConnect();
    restoreTabs();
  } catch (e) {
    state.schemaError = e.message;
  }
  state.loadingSchema = false;
  render();
  const tab = activeTab();
  if (tab && tab.type === 'table' && !tab.data) loadRows(tab);
}

async function refreshSchema() {
  if (!state.connId) return;
  try {
    const keepSchema = state.schemaName;
    state.schema = await api(`/connections/${state.connId}/schema?refresh=1`);
    if (state.schema.schemas.includes(keepSchema)) state.schemaName = keepSchema;
    // Drop tabs whose tables vanished.
    state.tabs = state.tabs.filter((t) => t.type !== 'table' || findTable(t.schema, t.table));
    if (!activeTab()) state.activeTab = state.tabs[0]?.id ?? null;
    toast('Schema refreshed');
  } catch (e) {
    toast(e.message, 'error');
  }
  render();
}

/** Asks Windows (one UAC prompt) to make the SQLite file writable for the current user. */
async function grantWriteAccess(btn) {
  btn.disabled = true;
  btn.replaceChildren(h('span', { class: 'spinner' }), 'Waiting for Windows…');
  try {
    await api(`/connections/${state.connId}/grant-write`, { method: 'POST', body: {} });
    state.schema = await api(`/connections/${state.connId}/schema?refresh=1`);
    toast(state.schema.readOnly ? 'Access changed, but the file is still read-only' : 'Write access granted. You can edit the data now', state.schema.readOnly ? 'error' : 'success');
  } catch (e) {
    toast(e.message, 'error');
  }
  render();
}

function render() {
  const c = conn();
  document.body.classList.toggle('conn-prod', isProd(c));
  if (c) document.title = `${isProd(c) ? '[PROD] ' : ''}${c.name} — Rowveil`;
  renderSidebar();
  renderMain();
}

// ---------------------------------------------------------------- sidebar
function renderSidebar() {
  const sb = $('#sidebar');
  // Rebuilding replaces the inputs: keep the focused one (search fields) and its caret.
  const active = sb.contains(document.activeElement) && document.activeElement.matches('input:not([type="checkbox"])') ? document.activeElement : null;
  const restore = active && { cls: active.className, start: active.selectionStart, end: active.selectionEnd };
  sb.replaceChildren();
  if (restore) queueMicrotask(() => {
    const el = [...sb.querySelectorAll('input')].find((x) => x.className === restore.cls);
    if (el) { el.focus({ preventScroll: true }); try { el.setSelectionRange(restore.start, restore.end); } catch { /* not a text input */ } }
  });
  const c = conn();

  // Connection switcher
  const switcher = h('button', { class: 'conn-switch', onclick: (e) => openConnMenu(e.currentTarget) },
    h('span', { class: `dialect-dot ${c?.dialect || ''}` }),
    h('span', { class: 'conn-name' }, c ? c.name : 'No connection'),
    c ? tagChips(c, 1) : null,
    c && !c.tags?.length ? h('span', { class: 'conn-dialect' }, DIALECT_LABEL[c.dialect] || '') : null,
    icon('chevron', 14),
  );
  sb.append(h('div', { class: 'sb-head' }, switcher));

  if (!c) {
    sb.append(h('div', { class: 'sb-empty' }, state.connections.length ? 'Choose a connection on the start page.' : 'Add a database connection to start.'));
    sb.append(sidebarFooter());
    return;
  }

  if (state.loadingSchema) {
    sb.append(h('div', { class: 'sb-empty' }, h('span', { class: 'spinner' }), ' Loading schema…'));
    sb.append(sidebarFooter());
    return;
  }
  if (state.schemaError) {
    sb.append(h('div', { class: 'sb-empty error' }, state.schemaError,
      h('button', { class: 'btn sm', onclick: () => selectConnection(state.connId, true) }, 'Retry')));
    sb.append(sidebarFooter());
    return;
  }

  if (isRedis()) {
    R.renderSidebar(sb);
    sb.append(sidebarFooter());
    return;
  }

  const schemas = state.schema.schemas;
  if (schemas.length > 1) {
    const sel = h('select', {
      class: 'schema-select',
      value: state.schemaName,
      onchange: (e) => { state.schemaName = e.target.value; saveTabs(); renderSidebar(); },
    }, schemas.map((s) => h('option', { value: s }, s)));
    sb.append(h('div', { class: 'sb-row' }, h('label', { class: 'sb-label' }, schemaWord(state.schema.dialect)), sel));
  }

  const search = h('input', {
    class: 'sb-search', placeholder: 'Search tables…', value: state.search,
    oninput: (e) => { state.search = e.target.value; renderTableList(); },
  });
  sb.append(h('div', { class: 'sb-search-wrap' }, icon('search', 14), search));

  sb.append(h('button', { class: 'sb-item sql-item', onclick: () => openSqlTab() }, icon('terminal'), h('span', {}, 'SQL console'), h('span', { class: 'kbd' }, '+')));
  const schemaActive = activeTab()?.type === 'schema';
  sb.append(h('button', { class: `sb-item sql-item schema-item${schemaActive ? ' active' : ''}`, onclick: openSchemaTab }, icon('schema'), h('span', {}, 'Schema')));
  renderSavedSection(sb);

  sb.append(h('div', { class: 'sb-section' }, h('span', {}, 'Tables'),
    h('button', { class: 'icon-btn', title: 'Refresh schema', onclick: refreshSchema }, icon('refresh', 13))));
  sb.append(h('div', { id: 'table-list', class: 'table-list' }));
  renderTableList();
  sb.append(sidebarFooter());
}

function renderTableList() {
  const list = $('#table-list');
  if (!list) return;
  list.replaceChildren();
  const q = state.search.trim().toLowerCase();
  const tables = state.schema.tables.filter((t) => t.schema === state.schemaName && (!q || t.name.toLowerCase().includes(q)));
  if (!tables.length) {
    list.append(h('div', { class: 'sb-empty small' }, q ? 'No matches' : 'No tables in this schema'));
    return;
  }
  const active = activeTab();
  for (const t of tables) {
    const isActive = active && active.type === 'table' && active.schema === t.schema && active.table === t.name;
    list.append(h('button', {
      class: `sb-item${isActive ? ' active' : ''}`,
      title: `${t.schema}.${t.name}`,
      onclick: (e) => openTable(t.schema, t.name, { newTab: e.ctrlKey || e.metaKey }),
      onauxclick: (e) => { if (e.button === 1) openTable(t.schema, t.name, { newTab: true }); },
    },
    icon(t.kind === 'table' ? 'table' : 'view'),
    h('span', { class: 'sb-name' }, t.name),
    t.rowEstimate != null && t.rowEstimate >= 0 ? h('span', { class: 'sb-count' }, compact(t.rowEstimate)) : null,
    ));
  }
}

function compact(n) {
  if (n < 1000) return String(n);
  if (n < 1e6) return (n / 1e3).toFixed(n < 1e4 ? 1 : 0).replace(/\.0$/, '') + 'k';
  return (n / 1e6).toFixed(1).replace(/\.0$/, '') + 'M';
}

// Each theme is a set of CSS variables in style.css under :root[data-theme="<id>"].
// The swatch shows background, accent, keyword and string colors.
const THEMES = [
  { id: 'rowveil-light', name: 'Rowveil Light', swatch: ['#ffffff', '#4f46e5', '#8b5cf6', '#14b8a6'] },
  { id: 'rowveil', name: 'Rowveil Dark', swatch: ['#0b1020', '#6366f1', '#a78bfa', '#2dd4bf'] },
  { id: 'dark', name: 'Graphite', swatch: ['#0b0b0b', '#c5f74f', '#c792ea', '#c3e88d'] },
  { id: 'light', name: 'Paper', swatch: ['#ffffff', '#1f2937', '#7c3aed', '#15803d'] },
  { id: 'one-dark-pro', name: 'One Dark Pro', swatch: ['#282c34', '#61afef', '#c678dd', '#98c379'] },
  { id: 'dracula', name: 'Dracula', swatch: ['#282a36', '#bd93f9', '#ff79c6', '#f1fa8c'] },
  { id: 'nord', name: 'Nord', swatch: ['#2e3440', '#88c0d0', '#81a1c1', '#a3be8c'] },
  { id: 'github-light', name: 'GitHub Light', swatch: ['#ffffff', '#1f883d', '#cf222e', '#0a3069'] },
];

function setTheme(id) {
  document.documentElement.dataset.theme = id;
  store.set('rs.theme', id);
}

// Re-apply the saved theme in app.js too, and fall back to the default for an unknown value.
{
  // Theme ids of the app's earlier name map to the same themes.
  const legacy = { querynest: 'rowveil', 'querynest-light': 'rowveil-light', roostbase: 'rowveil', 'roostbase-light': 'rowveil-light' };
  // No saved choice: follow the system light / dark setting.
  const fallback = window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'rowveil-light' : 'rowveil';
  const raw = store.get('rs.theme', fallback);
  const saved = legacy[raw] || raw;
  document.documentElement.dataset.theme = THEMES.some((t) => t.id === saved) ? saved : fallback;
}

function openThemeMenu(anchor) {
  const current = document.documentElement.dataset.theme;
  const original = current;
  let chosen = false;
  const menu = openMenu(anchor, THEMES.map((t) => h('div', {
    class: `menu-item${t.id === current ? ' active' : ''}`,
    // Live preview on hover; the click keeps it.
    onmouseenter: () => { document.documentElement.dataset.theme = t.id; },
    onclick: () => { chosen = true; setTheme(t.id); closeMenu(); renderSidebar(); },
  },
  h('span', { class: 'swatch' }, t.swatch.map((c) => h('span', { style: { background: c } }))),
  h('span', { class: 'menu-text' }, t.name),
  t.id === current ? icon('check', 13) : null,
  )), 'theme-menu');
  menu.addEventListener('mouseleave', () => { if (!chosen) document.documentElement.dataset.theme = original; });
  // Reposition above the anchor: the button sits at the bottom of the sidebar.
  const r = anchor.getBoundingClientRect();
  menu.style.top = `${Math.max(8, r.top - menu.offsetHeight - 4)}px`;
}

function sidebarFooter() {
  const theme = THEMES.find((t) => t.id === document.documentElement.dataset.theme) || THEMES[0];
  return h('div', { class: 'sb-foot' },
    h('span', { class: 'brand' }, h('img', { class: 'brand-mark', src: '/brand/symbol.svg', alt: '' }), h('span', { class: 'brand-name' }, 'Rowveil'),
      h('button', { class: 'ver', title: 'Check for updates', onclick: () => openUpdateModal() }, state.info ? `v${state.info.version}` : ''),
      updateAvailable() ? h('button', { class: 'update-pill', title: `Rowveil ${state.update.latest} is available`, onclick: () => openUpdateModal() }, icon('download', 11), 'Update') : null),
    h('button', { class: 'btn ghost sm', title: 'Theme', onclick: (e) => openThemeMenu(e.currentTarget) },
      icon('palette', 14), h('span', { class: 'theme-name' }, theme.name)),
  );
}

// ---------------------------------------------------------------- connection tags
const TAG_PRESETS = ['LOCAL', 'DEV', 'TEST', 'STAGE', 'PROD'];
function tagTone(t) {
  const u = String(t).toUpperCase();
  if (/^(PROD|PRODUCTION|PRD|LIVE)$/.test(u)) return 'prod';
  if (/^(STAGE|STAGING|STG|PREPROD|PRE-PROD|UAT)$/.test(u)) return 'stage';
  if (/^(DEV|DEVELOP|DEVELOPMENT)$/.test(u)) return 'dev';
  if (/^(LOCAL|LOCALHOST)$/.test(u)) return 'local';
  if (/^(TEST|TESTING|QA|CI)$/.test(u)) return 'test';
  return 'other';
}
const tagChip = (t) => h('span', { class: `tag-chip tone-${tagTone(t)}` }, t);
const tagChips = (c, max = 8) => {
  const tags = c?.tags || [];
  if (!tags.length) return null;
  return h('span', { class: 'tag-list' }, tags.slice(0, max).map(tagChip), tags.length > max ? h('span', { class: 'tag-more' }, `+${tags.length - max}`) : null);
};
const isProd = (c) => (c?.tags || []).some((t) => tagTone(t) === 'prod');
const cleanTag = (s) => s.trim().replace(/[^\p{L}\p{N}\-_. ]/gu, '').slice(0, 24).trim().toUpperCase();

/** Preset toggles (LOCAL, DEV, …) plus free-form tags. */
function tagEditor(initial = []) {
  let tags = [...initial];
  // Redrawing detaches the input, which fires "blur": ignore that one.
  let drawing = false;
  const box = h('div', { class: 'tag-editor' });
  const input = h('input', {
    class: 'tag-input', placeholder: 'Add tag…', maxlength: 24,
    onkeydown: (e) => {
      if ((e.key === 'Enter' || e.key === ',') && input.value.trim()) { e.preventDefault(); e.stopPropagation(); add(input.value); input.value = ''; input.focus(); }
      else if (e.key === 'Backspace' && !input.value && tags.length) { tags.pop(); draw(); input.focus(); }
    },
    onblur: () => { if (!drawing && input.value.trim()) { add(input.value); input.value = ''; } },
  });
  const add = (raw) => {
    for (const part of raw.split(/[,|;]/)) {
      const t = cleanTag(part);
      if (t && !tags.includes(t) && tags.length < 8) tags.push(t);
    }
    draw();
  };
  const toggle = (t) => { tags = tags.includes(t) ? tags.filter((x) => x !== t) : [...tags, t]; draw(); };
  const draw = () => {
    drawing = true;
    const custom = tags.filter((t) => !TAG_PRESETS.includes(t));
    box.replaceChildren(
      h('div', { class: 'tag-presets' }, TAG_PRESETS.map((t) => h('button', {
        type: 'button', class: `tag-chip tone-${tagTone(t)} toggle${tags.includes(t) ? ' on' : ''}`, onclick: () => toggle(t),
      }, t))),
      h('div', { class: 'tag-custom' },
        custom.map((t) => h('span', { class: `tag-chip tone-${tagTone(t)}` }, t,
          h('button', { type: 'button', class: 'tag-x', title: 'Remove', onclick: () => toggle(t) }, '×'))),
        input),
    );
    drawing = false;
  };
  draw();
  return { dom: box, value: () => { if (input.value.trim()) { add(input.value); input.value = ''; } return tags; } };
}

function openEditConnection(c) {
  const env = c.source === 'env';
  const name = h('input', { class: 'input', value: c.name, disabled: env });
  const ro = h('input', { type: 'checkbox', checked: c.readOnly, disabled: env || state.info?.readOnly });
  const tags = tagEditor(c.tags || []);
  const status = h('div', { class: 'form-status' });
  const body = h('div', { class: 'form' },
    h('label', {}, 'Name', name),
    h('label', {}, 'Connection URL', h('div', { class: 'url-preview mono' }, c.url)),
    h('label', {}, 'Tags', tags.dom),
    env ? h('div', { class: 'hint' }, 'This connection comes from an environment variable: only its tags can change here. You can also set them with ', h('code', {}, `DATABASE_TAGS_${c.id.replace(/^env-/, '').toUpperCase()}`), '.') : null,
    h('label', { class: 'check' }, ro, 'Read-only (block data changes)'),
    status);
  const save = h('button', {
    class: 'btn primary', onclick: async () => {
      save.disabled = true;
      try {
        const patch = { tags: tags.value() };
        if (!env) { patch.name = name.value; patch.readOnly = ro.checked; }
        const updated = await api(`/connections/${c.id}`, { method: 'PATCH', body: patch });
        const i = state.connections.findIndex((x) => x.id === c.id);
        if (i >= 0) state.connections[i] = updated;
        close();
        if (c.id === state.connId && !env && updated.readOnly !== c.readOnly) selectConnection(c.id, true);
        else render();
        toast('Connection saved', 'success');
      } catch (e) { status.textContent = e.message; status.className = 'form-status error'; save.disabled = false; }
    },
  }, 'Save');
  const close = openModal('Edit connection', body, [h('button', { class: 'btn', onclick: () => close() }, 'Cancel'), save]);
}

function openConnMenu(anchor) {
  // Presets first (in their order), then custom tags A–Z.
  const rank = (t) => { const i = TAG_PRESETS.indexOf(t); return i < 0 ? TAG_PRESETS.length : i; };
  const allTags = [...new Set(state.connections.flatMap((c) => c.tags || []))].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
  const filter = new Set((store.get('rs.connTagFilter', []) || []).filter((t) => allTags.includes(t)));
  const visible = state.connections.filter((c) => !filter.size || (c.tags || []).some((t) => filter.has(t)));
  const apply = () => { store.set('rs.connTagFilter', [...filter]); closeMenu(); openConnMenu(anchor); };
  // A click shows only that tag (a second click clears the filter). Ctrl / Shift + click adds or removes it.
  const pick = (t, multi) => {
    if (multi) { if (filter.has(t)) filter.delete(t); else filter.add(t); }
    else if (filter.size === 1 && filter.has(t)) filter.clear();
    else { filter.clear(); filter.add(t); }
    apply();
  };
  const filterBar = allTags.length ? h('div', { class: 'tag-filter', onclick: (e) => e.stopPropagation() },
    h('span', { class: 'muted small' }, 'Tags'),
    h('button', { type: 'button', class: `tag-chip tone-other toggle${filter.size ? '' : ' on'}`, title: 'Show all connections', onclick: () => { filter.clear(); apply(); } }, 'All'),
    allTags.map((t) => h('button', {
      type: 'button', class: `tag-chip tone-${tagTone(t)} toggle${filter.has(t) ? ' on' : ''}`,
      title: `${filter.size === 1 && filter.has(t) ? 'Show all' : `Only ${t}`} · Ctrl+click to select several tags`,
      onclick: (e) => pick(t, e.ctrlKey || e.metaKey || e.shiftKey),
    }, t))) : null;
  const items = visible.map((c) =>
    h('div', {
      class: `menu-item${c.id === state.connId ? ' active' : ''}`,
      title: 'Click to switch · Ctrl+click or middle-click to open in a new browser tab',
      onclick: (e) => {
        closeMenu();
        if (e.ctrlKey || e.metaKey) window.open(connUrl(c.id), '_blank');
        else selectConnection(c.id);
      },
      onauxclick: (e) => { if (e.button === 1) { closeMenu(); window.open(connUrl(c.id), '_blank'); } },
    },
      h('span', { class: `dialect-dot ${c.dialect || ''}` }),
      h('span', { class: 'menu-text' }, h('b', {}, h('span', { class: 'menu-name' }, c.name), tagChips(c, 3)), h('small', {}, c.url)),
      h('a', {
        class: 'icon-btn', href: connUrl(c.id), target: '_blank', title: state.info?.desktop ? 'Open in a new window' : 'Open in a new browser tab',
        onclick: (e) => { e.stopPropagation(); closeMenu(); },
      }, icon('expand', 13)),
      c.readOnly ? h('span', { class: 'badge' }, 'read-only') : null,
      h('button', { class: 'icon-btn', title: 'Edit name and tags', onclick: (e) => { e.stopPropagation(); closeMenu(); openEditConnection(c); } }, icon('edit', 13)),
      c.source === 'user'
        ? h('button', {
          class: 'icon-btn danger', title: 'Remove connection',
          onclick: async (e) => {
            e.stopPropagation();
            if (!confirm(`Remove connection "${c.name}"?`)) return;
            try {
              await api(`/connections/${c.id}`, { method: 'DELETE' });
              state.connections = state.connections.filter((x) => x.id !== c.id);
              closeMenu();
              if (state.connId === c.id) goHome();
              else if (!state.connId) render();
              else renderSidebar();
            } catch (err) { toast(err.message, 'error'); }
          },
        }, icon('trash', 13))
        : h('span', { class: 'badge muted', title: 'Defined by environment variable' }, 'env'),
    ));
  if (filterBar) items.unshift(filterBar);
  if (state.connId) items.unshift(h('div', { class: 'menu-item', onclick: () => { closeMenu(); goHome(); } }, icon('home'), h('span', {}, 'All connections')));
  if (filter.size && !visible.length) items.push(h('div', { class: 'menu-item muted' }, 'No connections with these tags'));
  items.push(h('div', { class: 'menu-sep' }));
  items.push(h('div', { class: 'menu-item', onclick: () => { closeMenu(); openConnectionModal(); } }, icon('plus'), h('span', {}, 'Add connection')));
  if (state.connId) {
    items.push(h('div', { class: 'menu-item', onclick: () => { closeMenu(); selectConnection(state.connId, true); } }, icon('refresh'), h('span', {}, 'Reconnect & reload schema')));
  }
  openMenu(anchor, items, 'conn-menu');
}

// ---------------------------------------------------------------- menus & modals
let menuEl = null;
function openMenu(anchor, items, cls = '') {
  closeMenu();
  const r = anchor.getBoundingClientRect();
  menuEl = h('div', { class: `menu ${cls}` }, items);
  document.body.append(menuEl);
  const mw = menuEl.offsetWidth;
  const left = Math.min(r.left, window.innerWidth - mw - 8);
  menuEl.style.left = `${Math.max(8, left)}px`;
  menuEl.style.top = `${r.bottom + 4}px`;
  setTimeout(() => document.addEventListener('mousedown', menuOutside), 0);
  return menuEl;
}
function menuOutside(e) { if (menuEl && !menuEl.contains(e.target)) closeMenu(); }
function closeMenu() {
  document.removeEventListener('mousedown', menuOutside);
  menuEl?.remove();
  menuEl = null;
}

function openModal(title, body, footer, { wide = false, onClose = null } = {}) {
  const overlay = h('div', { class: 'overlay', onmousedown: (e) => { if (e.target === overlay) close(); } });
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    overlay.remove();
    document.removeEventListener('keydown', onKey);
    if (onClose) onClose();
  };
  const onKey = (e) => { if (e.key === 'Escape' && !e.defaultPrevented) close(); };
  document.addEventListener('keydown', onKey);
  overlay.append(h('div', { class: `modal${wide ? ' wide' : ''}` },
    h('div', { class: 'modal-head' }, h('h3', {}, title), h('button', { class: 'icon-btn', onclick: close }, icon('x'))),
    h('div', { class: 'modal-body' }, body),
    footer ? h('div', { class: 'modal-foot' }, footer) : null,
  ));
  document.body.append(overlay);
  return close;
}

// ---------------------------------------------------------------- connection form
const DB_TYPES = [
  { id: 'postgres', label: 'PostgreSQL', port: 5432, user: 'postgres', sslKey: 'sslmode', ssl: { disable: 'disable', require: 'require' } },
  { id: 'mysql', label: 'MySQL', port: 3306, user: 'root', sslKey: 'ssl-mode', ssl: { disable: 'disabled', require: 'required' } },
  { id: 'mariadb', label: 'MariaDB', port: 3306, user: 'root', sslKey: 'ssl-mode', ssl: { disable: 'disabled', require: 'required' } },
  { id: 'sqlite', label: 'SQLite' },
  { id: 'redis', label: 'Redis', port: 6379, user: 'default' },
  { id: 'bigquery', label: 'BigQuery' },
];
const dbType = (id) => DB_TYPES.find((t) => t.id === id) || DB_TYPES[0];

function safeDecode(s) {
  try { return decodeURIComponent(s); } catch { return s; }
}

// Builds a connection URL from separate fields. User, password and database are percent-encoded,
// so special characters (/ ? # @ : %) in a password do not break the URL.
function buildConnUrl(f, { maskPassword = false } = {}) {
  const t = dbType(f.type);
  if (t.id === 'bigquery') {
    const project = f.project.trim();
    if (!project) return '';
    const q = [];
    const add = (k, v) => { if (String(v).trim()) q.push(`${k}=${encodeURIComponent(String(v).trim())}`); };
    add('credentials', f.keyFile);
    add('location', f.location);
    add('maximumBytesBilled', f.maxBytes);
    add('endpoint', f.endpoint);
    const ds = f.dataset.trim();
    return `bigquery://${encodeURIComponent(project)}${ds ? '/' + encodeURIComponent(ds) : ''}${q.length ? '?' + q.join('&') : ''}`;
  }
  if (t.id === 'sqlite') {
    const p = f.file.trim().replace(/\\/g, '/').replace(/%/g, '%25').replace(/\?/g, '%3F').replace(/#/g, '%23');
    return p ? `sqlite://${p}` : '';
  }
  const host = f.host.trim();
  if (!host) return '';
  let auth = '';
  if (f.user || (t.id === 'redis' && f.password)) {
    auth = encodeURIComponent(f.user);
    if (f.password) auth += ':' + (maskPassword ? '••••' : encodeURIComponent(f.password));
    auth += '@';
  }
  const hostPart = host.includes(':') && !host.startsWith('[') ? `[${host}]` : host;
  const port = String(f.port).trim();
  const params = [];
  if (t.sslKey && f.ssl && f.ssl !== 'default') params.push(`${t.sslKey}=${t.ssl[f.ssl]}`);
  const extra = f.options.trim().replace(/^\?/, '');
  if (extra) params.push(extra);
  const query = params.length ? `?${params.join('&')}` : '';
  if (t.id === 'redis') {
    const scheme = f.ssl === 'require' ? 'rediss' : 'redis';
    const db = f.database.trim();
    return `${scheme}://${auth}${hostPart}${port ? ':' + port : ''}${db ? '/' + encodeURIComponent(db) : ''}${query}`;
  }
  return `${t.id}://${auth}${hostPart}${port ? ':' + port : ''}/${encodeURIComponent(f.database.trim())}${query}`;
}

// Splits a connection URL into fields. Tolerates raw (not encoded) passwords: the last "@" ends the credentials.
function parseConnUrl(raw) {
  const url = raw.trim();
  const f = { type: 'postgres', host: '', port: '', user: '', password: '', database: '', ssl: 'default', options: '', file: '', project: '', dataset: '', keyFile: '', location: '', maxBytes: '', endpoint: '' };
  const m = /^([a-z][a-z0-9+.-]*):(?:\/\/)?(.*)$/i.exec(url);
  if (!m) {
    if (url) { f.type = 'sqlite'; f.file = url; }
    return f;
  }
  const scheme = m[1].toLowerCase();
  let rest = m[2];
  if (scheme === 'sqlite' || scheme === 'file') {
    f.type = 'sqlite';
    f.file = safeDecode(rest.split('?')[0].replace(/^@/, '').replace(/^\/([a-z]:)/i, '$1'));
    return f;
  }
  if (scheme === 'bigquery' || scheme === 'bq') {
    f.type = 'bigquery';
    const [path, query = ''] = rest.split(/\?(.*)/s);
    const [project = '', dataset = ''] = path.split('/');
    f.project = safeDecode(project);
    f.dataset = safeDecode(dataset);
    for (const kv of query.split('&').filter(Boolean)) {
      const [k, v = ''] = kv.split('=');
      const val = safeDecode(v);
      const key = k.toLowerCase();
      if (key === 'credentials' || key === 'keyfile') f.keyFile = val;
      else if (key === 'location') f.location = val;
      else if (key === 'maximumbytesbilled') f.maxBytes = val;
      else if (key === 'endpoint') f.endpoint = val;
    }
    return f;
  }
  if (['redis', 'rediss', 'valkey', 'valkeys'].includes(scheme)) {
    f.type = 'redis';
    if (scheme.endsWith('s')) f.ssl = 'require';
  } else f.type = scheme === 'postgresql' ? 'postgres' : (DB_TYPES.some((t) => t.id === scheme) ? scheme : 'postgres');
  const t = dbType(f.type);
  const at = rest.lastIndexOf('@');
  if (at >= 0) {
    const creds = rest.slice(0, at);
    rest = rest.slice(at + 1);
    const colon = creds.indexOf(':');
    f.user = safeDecode(colon >= 0 ? creds.slice(0, colon) : creds);
    f.password = colon >= 0 ? safeDecode(creds.slice(colon + 1)) : '';
  }
  const [main, query = ''] = rest.split(/\?(.*)/s);
  const slash = main.indexOf('/');
  const hostPort = slash >= 0 ? main.slice(0, slash) : main;
  f.database = slash >= 0 ? safeDecode(main.slice(slash + 1)) : '';
  const v6 = /^\[([^\]]+)\](?::(\d*))?$/.exec(hostPort);
  if (v6) { f.host = v6[1]; f.port = v6[2] || ''; } else {
    const c = hostPort.lastIndexOf(':');
    f.host = c >= 0 ? hostPort.slice(0, c) : hostPort;
    f.port = c >= 0 ? hostPort.slice(c + 1) : '';
  }
  f.host = safeDecode(f.host);
  const opts = [];
  for (const kv of query.split('&').filter(Boolean)) {
    const [k, v = ''] = kv.split('=');
    if (t.sslKey && k.toLowerCase() === t.sslKey) {
      const val = v.toLowerCase();
      if (val === t.ssl.disable) f.ssl = 'disable';
      else if (val === t.ssl.require) f.ssl = 'require';
      else opts.push(kv);
    } else opts.push(kv);
  }
  f.options = opts.join('&');
  return f;
}

function openConnectionModal() {
  let mode = store.get('rs.connMode', 'fields');
  const name = h('input', { class: 'input', placeholder: 'My database' });
  const ro = h('input', { type: 'checkbox' });
  const tags = tagEditor([]);
  const status = h('div', { class: 'form-status' });
  const setStatus = (text, kind) => { status.textContent = text; status.className = `form-status ${kind || ''}`; };

  // ---- URL mode
  const url = h('input', { class: 'input mono', placeholder: 'postgres://user:password@localhost:5432/db' });
  const urlPane = h('div', { class: 'form' },
    h('label', {}, 'Connection URL', url),
    h('div', { class: 'hint' },
      h('div', {}, h('code', {}, 'postgres://user:pass@host:5432/db')),
      h('div', {}, h('code', {}, 'mysql://user:pass@host:3306/db'), ' (or ', h('code', {}, 'mariadb://'), ')'),
      h('div', {}, h('code', {}, 'sqlite://./data.db'), ' or a path to a ', h('code', {}, '.db'), ' file'),
      h('div', {}, h('code', {}, 'redis://:pass@host:6379/0'), ' (TLS: ', h('code', {}, 'rediss://'), ')'),
      h('div', {}, h('code', {}, 'bigquery://project/dataset?credentials=C:\\key.json'), ' (', h('code', {}, 'location'), ', ', h('code', {}, 'maximumBytesBilled'), ', ', h('code', {}, 'endpoint'), ')'),
      h('div', {}, 'Encode special characters in the password: ', h('code', {}, '/ → %2F'), ' ', h('code', {}, '# → %23'), ' ', h('code', {}, '@ → %40')),
    ),
  );

  // ---- Fields mode
  let type = 'postgres';
  const host = h('input', { class: 'input mono', placeholder: 'localhost', value: 'localhost' });
  const port = h('input', { class: 'input mono', inputmode: 'numeric', placeholder: '5432' });
  const user = h('input', { class: 'input mono', placeholder: 'postgres', autocomplete: 'off' });
  const password = h('input', { class: 'input mono', type: 'password', placeholder: 'password', autocomplete: 'new-password' });
  const database = h('input', { class: 'input mono', placeholder: 'postgres' });
  const ssl = h('select', { class: 'input' },
    h('option', { value: 'default' }, 'Default'),
    h('option', { value: 'disable' }, 'Disable'),
    h('option', { value: 'require' }, 'Require'),
  );
  const options = h('input', { class: 'input mono', placeholder: 'application_name=studio&connect_timeout=10' });
  const file = h('input', { class: 'input mono', placeholder: 'C:\\data\\app.db  or  ./data.db' });
  const project = h('input', { class: 'input mono', placeholder: 'my-gcp-project' });
  const dataset = h('input', { class: 'input mono', placeholder: 'analytics' });
  const keyFile = h('input', { class: 'input mono', placeholder: 'C:\\keys\\service-account.json' });
  const location = h('input', { class: 'input mono', placeholder: 'US, EU, europe-west1…' });
  const maxBytes = h('input', { class: 'input mono', inputmode: 'numeric', placeholder: '10000000000  (10 GB)' });
  const endpoint = h('input', { class: 'input mono', placeholder: 'http://localhost:9050' });
  const preview = h('div', { class: 'url-preview mono' });
  const eye = h('button', {
    class: 'icon-btn pw-toggle', type: 'button', title: 'Show password', onclick: () => {
      const show = password.type === 'password';
      password.type = show ? 'text' : 'password';
      eye.replaceChildren(icon(show ? 'eyeOff' : 'eye', 14));
      eye.title = show ? 'Hide password' : 'Show password';
    },
  }, icon('eye', 14));

  const fields = () => ({
    type, host: host.value, port: port.value, user: user.value, password: password.value,
    database: database.value, ssl: ssl.value, options: options.value, file: file.value,
    project: project.value, dataset: dataset.value, keyFile: keyFile.value, location: location.value, maxBytes: maxBytes.value, endpoint: endpoint.value,
  });
  const typeSeg = h('div', { class: 'seg seg-full' });
  const serverBox = h('div', { class: 'form' },
    h('div', { class: 'form-grid hp' }, h('label', {}, 'Host', host), h('label', {}, 'Port', port)),
    h('div', { class: 'form-grid two' },
      h('label', {}, 'User', user),
      h('label', {}, 'Password', h('div', { class: 'pw-wrap' }, password, eye)),
    ),
    h('div', { class: 'form-grid two' }, h('label', { class: 'db-label' }, h('span', {}, 'Database'), database), h('label', { class: 'ssl-label' }, h('span', {}, 'SSL'), ssl)),
    h('label', {}, h('span', {}, 'Extra parameters ', h('span', { class: 'opt' }, '(optional)')), options),
  );
  const sqliteBox = h('div', { class: 'form' },
    h('label', {}, 'Database file', file),
    h('div', { class: 'hint' }, h('div', {}, 'Full path to an existing ', h('code', {}, '.db'), ' / ', h('code', {}, '.sqlite'), ' file on the machine where Rowveil runs.')),
  );
  const opt = (text) => h('span', {}, text, ' ', h('span', { class: 'opt' }, '(optional)'));
  const bigqueryBox = h('div', { class: 'form' },
    h('div', { class: 'form-grid two' }, h('label', {}, 'Project ID', project), h('label', {}, opt('Default dataset'), dataset)),
    h('label', {}, opt('Service account key file'), keyFile),
    h('div', { class: 'hint' },
      h('div', {}, 'Without a key file Rowveil uses ', h('code', {}, 'GOOGLE_APPLICATION_CREDENTIALS'), ', then ', h('code', {}, 'gcloud auth application-default login'), ', then the metadata server on Google Cloud.'),
      h('div', {}, 'Browsing a table without filters is free. Filters, sorting and the console run query jobs that BigQuery bills.')),
    h('div', { class: 'form-grid two' }, h('label', {}, opt('Location'), location), h('label', {}, opt('Max bytes billed per query'), maxBytes)),
    h('label', {}, opt('API endpoint (emulator)'), endpoint),
  );
  const fieldsPane = h('div', { class: 'form' }, h('label', {}, 'Database type', typeSeg), serverBox, sqliteBox, bigqueryBox, h('label', {}, 'Connection URL', preview));

  const updatePreview = () => {
    const u = buildConnUrl(fields(), { maskPassword: true });
    preview.textContent = u || '—';
    preview.classList.toggle('empty', !u);
  };
  const setType = (id, { keepPort = false } = {}) => {
    const prev = dbType(type);
    type = id;
    const t = dbType(id);
    for (const b of typeSeg.children) b.classList.toggle('on', b.dataset.id === id);
    serverBox.hidden = id === 'sqlite' || id === 'bigquery';
    sqliteBox.hidden = id !== 'sqlite';
    bigqueryBox.hidden = id !== 'bigquery';
    const redis = id === 'redis';
    serverBox.querySelector('.db-label span').textContent = redis ? 'Database number' : 'Database';
    serverBox.querySelector('.ssl-label span').textContent = redis ? 'TLS' : 'SSL';
    ssl.options[2].textContent = redis ? 'Require (rediss://)' : 'Require';
    user.placeholder = redis ? 'default (optional)' : t.user || '';
    if (id !== 'sqlite' && id !== 'bigquery') {
      port.placeholder = String(t.port);
      if (!redis) user.placeholder = t.user;
      database.placeholder = id === 'postgres' ? 'postgres' : redis ? '0' : 'mydb';
      options.placeholder = redis ? 'protocol=resp3' : 'application_name=studio&connect_timeout=10';
      if (!keepPort && (!port.value || port.value === String(prev.port))) port.value = String(t.port);
    }
    updatePreview();
  };
  for (const t of DB_TYPES) {
    typeSeg.append(h('button', { type: 'button', dataset: { id: t.id }, onclick: () => setType(t.id) }, t.label));
  }
  for (const el of [host, port, user, password, database, options, file, project, dataset, keyFile, location, maxBytes, endpoint]) el.addEventListener('input', updatePreview);
  ssl.addEventListener('change', updatePreview);

  const fillFields = (u) => {
    const f = parseConnUrl(u);
    host.value = f.host; port.value = f.port; user.value = f.user; password.value = f.password;
    database.value = f.database; ssl.value = f.ssl; options.value = f.options; file.value = f.file;
    project.value = f.project; dataset.value = f.dataset; keyFile.value = f.keyFile; location.value = f.location; maxBytes.value = f.maxBytes; endpoint.value = f.endpoint;
    setType(f.type, { keepPort: true });
  };

  // ---- mode switch
  const modeSeg = h('div', { class: 'seg' });
  const modes = [['fields', 'Parameters'], ['url', 'URL']];
  const setMode = (m, { sync = true } = {}) => {
    if (sync && m !== mode) {
      if (m === 'url') { const u = buildConnUrl(fields()); if (u) url.value = u; } else if (url.value.trim()) fillFields(url.value);
    }
    mode = m;
    store.set('rs.connMode', m);
    for (const b of modeSeg.children) b.classList.toggle('on', b.dataset.id === m);
    fieldsPane.hidden = m !== 'fields';
    urlPane.hidden = m !== 'url';
    setStatus('');
    setTimeout(() => (m === 'url' ? url : type === 'sqlite' ? file : type === 'bigquery' ? project : host).focus(), 0);
  };
  for (const [id, label] of modes) modeSeg.append(h('button', { type: 'button', dataset: { id }, onclick: () => setMode(id) }, label));

  const currentUrl = () => (mode === 'url' ? url.value.trim() : buildConnUrl(fields()));
  const validate = () => {
    if (mode === 'url') return url.value.trim() ? null : 'Enter a connection URL';
    if (type === 'sqlite') return file.value.trim() ? null : 'Enter the path to the database file';
    if (type === 'bigquery') {
      if (!project.value.trim()) return 'Enter the Google Cloud project ID';
      if (maxBytes.value.trim() && !/^\d+$/.test(maxBytes.value.trim())) return 'Max bytes billed must be a whole number';
      return null;
    }
    if (type === 'redis' && database.value.trim() && !/^\d+$/.test(database.value.trim())) return 'Database number must be 0, 1, 2…';
    if (!host.value.trim()) return 'Enter the host';
    const p = port.value.trim();
    if (p && !(/^\d+$/.test(p) && +p >= 1 && +p <= 65535)) return 'Port must be a number from 1 to 65535';
    return null;
  };

  const body = h('div', { class: 'form' },
    h('div', { class: 'form-top' }, h('label', {}, 'Name', name), h('label', {}, 'Connect with', modeSeg)),
    fieldsPane,
    urlPane,
    h('label', {}, h('span', {}, 'Tags ', h('span', { class: 'opt' }, '(optional)')), tags.dom),
    h('label', { class: 'check' }, ro, 'Read-only (block data changes)'),
    status,
  );
  const test = h('button', {
    class: 'btn', onclick: async () => {
      const err = validate();
      if (err) { setStatus(err, 'error'); return; }
      setStatus('Testing…');
      try { const r = await api('/connections/test', { body: { url: currentUrl() } }); setStatus(`Connected (${DIALECT_LABEL[r.dialect]})`, 'ok'); } catch (e) { setStatus(e.message, 'error'); }
    },
  }, 'Test');
  const save = h('button', {
    class: 'btn primary', onclick: async () => {
      const err = validate();
      if (err) { setStatus(err, 'error'); return; }
      setStatus('Connecting…');
      save.disabled = true;
      try {
        const c = await api('/connections', { body: { name: name.value, url: currentUrl(), readOnly: ro.checked, tags: tags.value() } });
        state.connections.push(c);
        close();
        selectConnection(c.id);
      } catch (e) { setStatus(e.message, 'error'); save.disabled = false; }
    },
  }, 'Save & connect');
  const close = openModal('New connection', body, [test, save]);
  setType('postgres');
  setMode(mode, { sync: false });
  body.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target.tagName === 'INPUT') save.click(); });
}

// ---------------------------------------------------------------- main area / tabs
function renderMain() {
  const main = $('#main');
  main.replaceChildren();
  if (!conn()) {
    main.append(state.connections.length ? homePage() : welcome());
    return;
  }
  main.append(renderTabBar());
  const tab = activeTab();
  const content = h('div', { class: 'tab-content', id: 'tab-content' });
  main.append(content);
  if (!tab && isRedis() && state.schema.redis) { R.renderEmpty(content); return; }
  if (!tab) {
    content.append(h('div', { class: 'empty-main' },
      icon('db', 40),
      h('h2', {}, conn().name),
      h('p', {}, state.schema ? `${state.schema.tables.length} tables and views · ${DIALECT_LABEL[state.schema.dialect]}` : ''),
      h('p', { class: 'muted' }, 'Select a table in the sidebar or open the SQL console.'),
      h('button', { class: 'btn', onclick: () => openSqlTab() }, icon('terminal'), 'Open SQL console'),
    ));
    return;
  }
  renderTabContent(tab, content);
}

function renderTabContent(tab, content) {
  if (tab.type === 'table') renderTableView(tab, content);
  else if (tab.type === 'schema') renderSchemaView(tab, content);
  else if (tab.type === 'rkey') R.renderKey(tab, content);
  else if (tab.type === 'rcli') R.renderConsole(tab, content);
  else renderSqlView(tab, content);
}

function welcome() {
  return h('div', { class: 'empty-main' },
    h('img', { src: '/brand/symbol.svg', width: 96, height: 96, alt: '' }),
    h('h2', { class: 'welcome-title' }, 'Rowveil'),
    h('p', { class: 'welcome-tag' }, 'Database explorer & query tool'),
    h('p', { class: 'muted' }, 'Browse and edit PostgreSQL, MySQL, SQLite, Redis and BigQuery.'),
    h('button', { class: 'btn primary', onclick: openConnectionModal }, icon('plus'), 'Add connection'),
  );
}

// The start page: all connections as cards, with search and the tag filter (shared with the connection menu).
function homePage() {
  const rank = (t) => { const i = TAG_PRESETS.indexOf(t); return i < 0 ? TAG_PRESETS.length : i; };
  const allTags = [...new Set(state.connections.flatMap((c) => c.tags || []))].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
  const filter = new Set((store.get('rs.connTagFilter', []) || []).filter((t) => allTags.includes(t)));
  const last = store.get('rs.conn', null);
  const grid = h('div', { class: 'home-grid' });
  const search = h('input', { class: 'home-search', type: 'search', placeholder: 'Search connections…', autofocus: true });
  const open = (c, e) => {
    if (e && (e.ctrlKey || e.metaKey || e.button === 1)) window.open(connUrl(c.id), '_blank');
    else selectConnection(c.id);
  };
  const matches = () => {
    const q = search.value.trim().toLowerCase();
    return state.connections.filter((c) => (!filter.size || (c.tags || []).some((t) => filter.has(t)))
      && (!q || `${c.name} ${c.url} ${DIALECT_LABEL[c.dialect] || ''} ${(c.tags || []).join(' ')}`.toLowerCase().includes(q)));
  };
  const card = (c) => h('div', {
    class: `home-card${isProd(c) ? ' prod' : ''}`, tabindex: '0', role: 'button',
    title: 'Click to open · Ctrl+click or middle-click to open in a new browser tab',
    onclick: (e) => open(c, e),
    onauxclick: (e) => { if (e.button === 1) open(c, e); },
    onkeydown: (e) => { if (e.key === 'Enter') open(c, e); },
  },
    h('div', { class: 'home-card-head' },
      h('span', { class: `dialect-dot ${c.dialect || ''}` }),
      h('span', { class: 'home-card-name' }, c.name),
      h('button', { class: 'icon-btn', title: 'Edit name and tags', onclick: (e) => { e.stopPropagation(); openEditConnection(c); } }, icon('edit', 13))),
    h('div', { class: 'home-card-meta' },
      h('span', { class: 'muted small' }, DIALECT_LABEL[c.dialect] || c.dialect),
      c.readOnly ? h('span', { class: 'badge' }, 'read-only') : null,
      c.id === last ? h('span', { class: 'badge muted' }, 'last used') : null,
      tagChips(c, 4)),
    h('div', { class: 'home-card-url mono' }, c.url));
  const draw = () => {
    const list = matches();
    grid.replaceChildren(...list.map(card),
      h('button', { class: 'home-card home-add', onclick: openConnectionModal }, icon('plus', 18), h('span', {}, 'Add connection')));
    if (!list.length) grid.prepend(h('div', { class: 'muted home-none' }, 'No connections match.'));
  };
  search.addEventListener('input', draw);
  search.addEventListener('keydown', (e) => { if (e.key === 'Enter') { const c = matches()[0]; if (c) open(c, e); } });
  const chips = allTags.length ? h('div', { class: 'home-tags' },
    h('button', { type: 'button', class: `tag-chip tone-other toggle${filter.size ? '' : ' on'}`, title: 'Show all connections', onclick: () => { filter.clear(); store.set('rs.connTagFilter', []); render(); } }, 'All'),
    allTags.map((t) => h('button', {
      type: 'button', class: `tag-chip tone-${tagTone(t)} toggle${filter.has(t) ? ' on' : ''}`,
      title: `${filter.size === 1 && filter.has(t) ? 'Show all' : `Only ${t}`} · Ctrl+click to select several tags`,
      onclick: (e) => {
        if (e.ctrlKey || e.metaKey || e.shiftKey) { if (filter.has(t)) filter.delete(t); else filter.add(t); }
        else if (filter.size === 1 && filter.has(t)) filter.clear();
        else { filter.clear(); filter.add(t); }
        store.set('rs.connTagFilter', [...filter]);
        render();
      },
    }, t))) : null;
  const openLast = h('input', { type: 'checkbox', checked: store.get('rs.openLast', false), onchange: (e) => store.set('rs.openLast', e.target.checked) });
  draw();
  queueMicrotask(() => search.focus());
  return h('div', { class: 'home' },
    h('div', { class: 'home-inner' },
      h('div', { class: 'home-brand' },
        h('img', { src: '/brand/symbol.svg', width: 48, height: 48, alt: '' }),
        h('div', {}, h('h1', { class: 'welcome-title' }, 'Rowveil'), h('p', { class: 'welcome-tag' }, 'Choose a connection'))),
      h('div', { class: 'home-bar' }, search, chips),
      grid,
      h('label', { class: 'home-opt muted small' }, openLast, 'Open the last connection on start')));
}

function tabTitle(t) {
  if (t.type === 'rkey' || t.type === 'rcli') return R.tabTitle(t);
  return t.type === 'sql' ? t.title : t.type === 'schema' ? 'Schema' : t.table;
}

function renderTabBar() {
  const bar = h('div', { class: 'tabbar' });
  for (const t of state.tabs) {
    const dirty = t.type === 'table' && pendingCount(t) > 0;
    bar.append(h('div', {
      class: `tab${t.id === state.activeTab ? ' active' : ''}`,
      title: t.type === 'table' ? `${t.schema}.${t.table}` : t.type === 'rkey' ? `db${t.db}: ${t.key}` : tabTitle(t),
      onclick: () => activate(t.id),
      onauxclick: (e) => { if (e.button === 1) closeTab(t.id); },
    },
    icon(t.type === 'rkey' || t.type === 'rcli' ? R.tabIcon(t) : t.type === 'sql' ? (findSaved(t.savedId) ? 'bookmark' : 'terminal') : t.type === 'schema' ? 'schema' : (findTable(t.schema, t.table)?.kind === 'table' ? 'table' : 'view'), 13),
    h('span', { class: 'tab-name' }, tabTitle(t)),
    dirty ? h('span', { class: 'dirty-dot' }) : null,
    h('button', { class: 'tab-close', onclick: (e) => { e.stopPropagation(); closeTab(t.id); } }, icon('x', 12)),
    ));
  }
  bar.append(isRedis()
    ? h('button', { class: 'icon-btn tab-add', title: 'New Redis console', onclick: () => R.openConsole() }, icon('plus', 14))
    : h('button', { class: 'icon-btn tab-add', title: 'New SQL console', onclick: () => openSqlTab() }, icon('plus', 14)));
  return bar;
}

function activate(id) {
  state.activeTab = id;
  saveTabs();
  closeMenu();
  render();
  const tab = activeTab();
  if (tab && tab.type === 'table' && !tab.data && !tab.loading) loadRows(tab);
}

function closeTab(id) {
  const idx = state.tabs.findIndex((t) => t.id === id);
  if (idx < 0) return;
  const t = state.tabs[idx];
  if (t.type === 'table' && pendingCount(t) > 0 && !confirm('Discard unsaved changes?')) return;
  if (t.type === 'sql') t.cm?.destroy();
  state.tabs.splice(idx, 1);
  if (state.activeTab === id) state.activeTab = (state.tabs[idx] || state.tabs[idx - 1])?.id ?? null;
  activate(state.activeTab);
}

function openTable(schema, table, { newTab = false, filters = null } = {}) {
  let tab = !newTab && state.tabs.find((t) => t.type === 'table' && t.schema === schema && t.table === table);
  if (tab && filters) {
    if (pendingCount(tab) > 0) tab = null; // keep unsaved work; open a new tab instead
    else { tab.filters = filters; tab.filterDraft = null; tab.page = 0; tab.data = null; }
  }
  if (!tab) {
    const current = activeTab();
    tab = newTableTab(schema, table, filters ? { filters } : {});
    // Replace a clean current table tab (preview behaviour), otherwise append.
    if (!newTab && current && current.type === 'table' && pendingCount(current) === 0 && !filters) {
      const i = state.tabs.indexOf(current);
      state.tabs[i] = tab;
    } else {
      state.tabs.push(tab);
    }
  }
  if (schema !== state.schemaName && state.schema.schemas.includes(schema)) state.schemaName = schema;
  activate(tab.id);
}

function openSqlTab(sql = '') {
  const t = newSqlTab(null, sql);
  state.tabs.push(t);
  activate(t.id);
  setTimeout(() => t.cm?.focus(), 0);
}

function globalKeys(e) {
  const tab = activeTab();
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    // The SQL editor and the Redis console handle Ctrl+S themselves (defaultPrevented).
    if (e.defaultPrevented) return;
    e.preventDefault();
    if (tab && tab.type === 'table' && pendingCount(tab) > 0) saveChanges(tab);
    else if (tab?.type === 'sql') saveSqlTab(tab);
  }
}

// ---------------------------------------------------------------- table view
const OPS = ['=', '<>', '>', '>=', '<', '<=', 'like', 'not like', 'ilike', 'not ilike', 'in', 'not in', 'is null', 'is not null'];
const NO_VALUE_OPS = new Set(['is null', 'is not null']);

function pendingCount(tab) {
  let n = tab.pending.inserts.length + tab.pending.deletes.size;
  for (const u of tab.pending.updates.values()) n += Object.keys(u.values).length;
  return n;
}

function pkOf(meta, row, columns) {
  const pk = {};
  for (const k of meta.primaryKey) pk[k] = row[columns.indexOf(k)];
  return pk;
}
const pkKey = (pk) => JSON.stringify(pk);

function editBlockReason(meta) {
  if (isReadOnly()) return state.schema.readOnlyReason || 'Connection is read-only';
  if (meta.kind !== 'table') return `This is a ${meta.kind.replace('_', ' ')}`;
  if (!meta.primaryKey.length) return 'Table has no primary key';
  return null;
}

async function loadRows(tab, { keepSelection = false } = {}) {
  const meta = findTable(tab.schema, tab.table);
  if (!meta) return;
  tab.loading = true;
  tab.error = null;
  if (tab.id === state.activeTab) refreshView(tab);
  try {
    const res = await api(`/connections/${state.connId}/rows`, {
      body: {
        schema: tab.schema,
        table: tab.table,
        filters: activeFilters(tab),
        sort: tab.sort,
        limit: tab.pageSize,
        offset: tab.page * tab.pageSize,
      },
    });
    tab.data = res;
    if (!keepSelection) { tab.selected.clear(); tab.sel = null; }
  } catch (e) {
    tab.error = e.message;
  }
  tab.loading = false;
  if (tab.id === state.activeTab) refreshView(tab);
}

function activeFilters(tab) {
  return tab.filters.filter((f) => f.column && f.op && (NO_VALUE_OPS.has(f.op) || f.value !== ''));
}

/** Re-render the active table view and keep the grid scroll position. */
function refreshView(tab) {
  const content = $('#tab-content');
  if (!content || tab.id !== state.activeTab) return;
  const wrap = $('.grid-wrap', content);
  const scroll = wrap ? [wrap.scrollLeft, wrap.scrollTop] : null;
  const hadFocus = wrap && wrap.contains(document.activeElement);
  const drawerScroll = $('.drawer-body', content)?.scrollTop;
  content.replaceChildren();
  renderTabContent(tab, content);
  const nw = $('.grid-wrap', content);
  if (nw && scroll) { nw.scrollLeft = scroll[0]; nw.scrollTop = scroll[1]; }
  if (nw && hadFocus) nw.focus({ preventScroll: true });
  const db = $('.drawer-body', content);
  if (db && drawerScroll) db.scrollTop = drawerScroll;
  // Keep the tab bar dirty markers in sync.
  const bar = $('.tabbar');
  if (bar) bar.replaceWith(renderTabBar());
}

function renderTableView(tab, root) {
  const meta = findTable(tab.schema, tab.table);
  if (!meta) { root.append(h('div', { class: 'empty-main' }, 'Table not found')); return; }
  root.append(renderToolbar(tab, meta));
  if (tab.filterOpen && tab.view === 'data') root.append(renderFilterPanel(tab, meta));
  const body = h('div', { class: 'table-body' });
  root.append(body);
  if (tab.view === 'structure') {
    body.append(renderStructure(meta));
    return;
  }
  const gridArea = h('div', { class: 'grid-area' });
  body.append(gridArea);
  if (tab.error) gridArea.append(h('div', { class: 'grid-error' }, icon('x'), tab.error));
  if (tab.data) gridArea.append(renderGrid(tab, meta));
  else if (tab.loading) gridArea.append(h('div', { class: 'grid-loading' }, h('span', { class: 'spinner' }), 'Loading…'));
  if (tab.loading && tab.data) gridArea.append(h('div', { class: 'loading-bar' }));
  if (tab.drawer) body.append(renderDrawer(tab, meta));
  root.append(renderFooter(tab, meta));
}

function renderToolbar(tab, meta) {
  const reason = editBlockReason(meta);
  const canInsert = !isReadOnly() && meta.kind === 'table';
  const nFilters = activeFilters(tab).length;
  const nSel = tab.selected.size;
  return h('div', { class: 'toolbar' },
    h('div', { class: 'tb-group' },
      h('div', { class: 'tb-title' }, icon(meta.kind === 'table' ? 'table' : 'view'), h('span', {}, meta.name),
        reason ? h('span', { class: 'badge muted', title: reason }, icon('lock', 11), 'read-only') : null,
        state.schema?.canGrantWrite ? h('button', {
          class: 'btn sm grant-btn', title: 'The file is write-protected for your Windows user. Windows asks for permission once (UAC); after that, editing works without administrator rights.',
          onclick: (e) => grantWriteAccess(e.currentTarget),
        }, icon('key', 13), 'Grant write access') : null),
      h('div', { class: 'seg' },
        h('button', { class: tab.view === 'data' ? 'on' : '', onclick: () => { tab.view = 'data'; saveTabs(); refreshView(tab); } }, 'Data'),
        h('button', { class: tab.view === 'structure' ? 'on' : '', onclick: () => { tab.view = 'structure'; saveTabs(); refreshView(tab); } }, 'Structure'),
      ),
    ),
    tab.view === 'data' ? h('div', { class: 'tb-group' },
      h('button', { class: `btn ghost${nFilters ? ' active' : ''}`, onclick: () => { tab.filterOpen = !tab.filterOpen; if (tab.filterOpen && !tab.filterDraft) tab.filterDraft = draftFrom(tab, meta); refreshView(tab); } },
        icon('filter'), 'Filters', nFilters ? h('span', { class: 'count' }, nFilters) : null),
      h('button', { class: 'btn ghost', onclick: (e) => openColumnsMenu(e.currentTarget, tab, meta) }, icon('columns'), 'Columns',
        tab.hidden.length ? h('span', { class: 'count' }, `${meta.columns.length - tab.hidden.length}/${meta.columns.length}`) : null),
      h('button', { class: 'btn ghost', title: 'Refresh (reload rows)', onclick: () => loadRows(tab, { keepSelection: true }) }, icon('refresh')),
      h('button', { class: 'btn ghost', title: 'Export', onclick: (e) => openExportMenu(e.currentTarget, tab, meta) }, icon('download'), 'Export'),
      nSel && !reason ? h('button', { class: 'btn danger-ghost', onclick: () => markDeleteSelected(tab, meta) }, icon('trash'), `Delete ${nSel}`) : null,
      canInsert ? h('button', { class: 'btn primary', onclick: () => addRecord(tab, meta) }, icon('plus'), 'Add record') : null,
    ) : null,
  );
}

function draftFrom(tab, meta) {
  const d = tab.filters.map((f) => ({ ...f }));
  if (!d.length) d.push({ column: meta.columns[0]?.name || '', op: '=', value: '' });
  return d;
}

function renderFilterPanel(tab, meta) {
  const draft = tab.filterDraft || (tab.filterDraft = draftFrom(tab, meta));
  const apply = () => {
    tab.filters = draft.filter((f) => f.column).map((f) => ({ ...f }));
    tab.page = 0;
    saveTabs();
    loadRows(tab);
  };
  const rows = draft.map((f, i) => {
    const valueInput = NO_VALUE_OPS.has(f.op) ? h('span', { class: 'filter-novalue' }) : h('input', {
      class: 'input sm mono', value: f.value ?? '', placeholder: f.op.includes('in') && !f.op.includes('like') ? 'a, b, c' : f.op.includes('like') ? '%text%' : 'value',
      oninput: (e) => { f.value = e.target.value; },
      onkeydown: (e) => { if (e.key === 'Enter') apply(); },
    });
    return h('div', { class: 'filter-row' },
      h('span', { class: 'filter-join' }, i === 0 ? 'where' : 'and'),
      h('select', { class: 'input sm', value: f.column, onchange: (e) => { f.column = e.target.value; } },
        meta.columns.map((c) => h('option', { value: c.name }, c.name))),
      h('select', { class: 'input sm', value: f.op, onchange: (e) => { f.op = e.target.value; refreshView(tab); } },
        OPS.map((o) => h('option', { value: o }, o))),
      valueInput,
      h('button', { class: 'icon-btn', title: 'Remove filter', onclick: () => { draft.splice(i, 1); refreshView(tab); } }, icon('x', 13)),
    );
  });
  return h('div', { class: 'filter-panel' },
    rows.length ? rows : h('div', { class: 'muted small' }, 'No filters'),
    h('div', { class: 'filter-actions' },
      h('button', { class: 'btn ghost sm', onclick: () => { draft.push({ column: meta.columns[0]?.name || '', op: '=', value: '' }); refreshView(tab); } }, icon('plus', 13), 'Add filter'),
      h('span', { class: 'spacer' }),
      h('button', { class: 'btn ghost sm', onclick: () => { tab.filterDraft = []; tab.filters = []; tab.page = 0; saveTabs(); loadRows(tab); } }, 'Clear'),
      h('button', { class: 'btn primary sm', onclick: apply }, 'Apply'),
    ),
  );
}

function openColumnsMenu(anchor, tab, meta) {
  columnsMenu(anchor, meta.columns.map((c) => ({ name: c.name, type: c.dataType })), () => tab.hidden,
    (hidden) => { tab.hidden = hidden; saveTabs(); refreshView(tab); });
}

/**
 * The "Columns" menu: a checkbox per column, "Show all" and "Hide all" (keeps the first column).
 * `getHidden()` returns the hidden names; `setHidden(names)` stores them and redraws.
 */
function columnsMenu(anchor, cols, getHidden, setHidden) {
  const names = [...new Set(cols.map((c) => c.name))];
  const items = [
    h('div', { class: 'menu-row' },
      h('button', { class: 'btn ghost sm', onclick: () => { closeMenu(); setHidden([]); } }, 'Show all'),
      h('button', { class: 'btn ghost sm', onclick: () => { closeMenu(); setHidden(names.slice(1)); } }, 'Hide all')),
    ...names.map((name) => h('label', { class: 'menu-item check' },
      h('input', {
        type: 'checkbox', checked: !getHidden().includes(name),
        onchange: (e) => setHidden(e.target.checked ? getHidden().filter((x) => x !== name) : [...getHidden(), name]),
      }),
      h('span', { class: 'menu-text' }, name), h('small', { class: 'muted' }, cols.find((c) => c.name === name)?.type || ''))),
  ];
  openMenu(anchor, items, 'scroll-menu');
}

function openExportMenu(anchor, tab, meta) {
  const exportData = async (format, all) => {
    closeMenu();
    let columns = tab.data?.columns || [];
    let rows = tab.data?.rows || [];
    if (all) {
      try {
        const res = await api(`/connections/${state.connId}/rows`, {
          body: { schema: tab.schema, table: tab.table, filters: activeFilters(tab), sort: tab.sort, limit: 10000, offset: 0, withCount: false },
        });
        columns = res.columns; rows = res.rows;
      } catch (e) { toast(e.message, 'error'); return; }
    }
    if (format === 'csv') download(`${meta.name}.csv`, toCSV(columns, rows), 'text/csv');
    else download(`${meta.name}.json`, JSON.stringify(rowsToObjects(columns, rows), null, 2), 'application/json');
  };
  openMenu(anchor, [
    h('div', { class: 'menu-item', onclick: () => exportData('csv', false) }, 'Current page → CSV'),
    h('div', { class: 'menu-item', onclick: () => exportData('json', false) }, 'Current page → JSON'),
    h('div', { class: 'menu-sep' }),
    h('div', { class: 'menu-item', onclick: () => exportData('csv', true) }, 'All filtered rows (max 10k) → CSV'),
    h('div', { class: 'menu-item', onclick: () => exportData('json', true) }, 'All filtered rows (max 10k) → JSON'),
  ]);
}

function renderFooter(tab, meta) {
  const total = tab.data?.total;
  const count = tab.data?.rows.length || 0;
  const from = count ? tab.page * tab.pageSize + 1 : 0;
  const to = tab.page * tab.pageSize + count;
  const lastPage = total != null ? Math.max(0, Math.ceil(total / tab.pageSize) - 1) : tab.page + (count === tab.pageSize ? 1 : 0);
  const n = pendingCount(tab);
  const go = (p) => { tab.page = Math.max(0, Math.min(lastPage, p)); loadRows(tab); };
  return h('div', { class: 'footer' },
    n ? h('div', { class: 'pending-bar' },
      h('span', { class: 'pending-text' }, h('span', { class: 'dirty-dot' }), `${n} unsaved change${n === 1 ? '' : 's'}`),
      h('button', { class: 'btn ghost sm', onclick: () => discardChanges(tab) }, icon('undo', 13), 'Discard'),
      h('button', { class: 'btn primary sm', onclick: () => saveChanges(tab) }, icon('check', 13), 'Save changes', h('span', { class: 'kbd' }, 'Ctrl S')),
    ) : h('div', { class: 'muted small' }, tab.view === 'structure' ? '' : `${meta.columns.length} columns`,
      state.schema?.dialect === 'bigquery' && tab.view !== 'structure'
        ? h('span', { class: 'bq-note' }, (tab.filters.length || tab.sort.length || meta.kind !== 'table')
          ? ' · filtered view: runs a query job (billed)' : ' · table preview: no query, free')
        : null),
    tab.view === 'data' ? h('div', { class: 'pager' },
      h('select', {
        class: 'input sm', value: String(tab.pageSize), title: 'Rows per page',
        onchange: (e) => { tab.pageSize = Number(e.target.value); tab.page = 0; saveTabs(); loadRows(tab); },
      }, [25, 50, 100, 250, 500, 1000].map((s) => h('option', { value: String(s) }, `${s} / page`))),
      h('span', { class: 'pager-text' }, count ? `${fmtNum(from)}–${fmtNum(to)} of ${fmtNum(total)}` : `0 of ${fmtNum(total ?? 0)}`),
      h('button', { class: 'icon-btn', disabled: tab.page === 0, onclick: () => go(tab.page - 1), title: 'Previous page' }, icon('left')),
      h('button', { class: 'icon-btn', disabled: tab.page >= lastPage, onclick: () => go(tab.page + 1), title: 'Next page' }, icon('right')),
    ) : null,
  );
}

// ---------------------------------------------------------------- grid
const DEFAULT = Symbol('default');

function defaultWidth(c) {
  const byKind = { number: 110, bigint: 140, boolean: 100, uuid: 300, date: 210, json: 260, array: 200, binary: 200, enum: 140 };
  return Math.max(byKind[c.kind] || 200, Math.min(320, c.name.length * 8 + 70));
}

function displayValue(v, c) {
  if (v === DEFAULT) return { text: 'default', cls: 'default' };
  if (v === null || v === undefined) return { text: 'NULL', cls: 'null' };
  if (c && c.kind === 'boolean' && (v === 0 || v === 1)) v = v === 1;
  if (typeof v === 'boolean') return { text: String(v), cls: v ? 'bool-true' : 'bool-false' };
  if (typeof v === 'object') return { text: JSON.stringify(v), cls: 'json' };
  if (typeof v === 'number') return { text: String(v), cls: 'num' };
  const s = String(v);
  return { text: s.length > 500 ? s.slice(0, 500) + '…' : s, cls: (c && c.kind === 'json') || looksLikeJsonText(s) ? 'json' : '' };
}

function stringValue(v) {
  if (v === null || v === undefined || v === DEFAULT) return '';
  return typeof v === 'object' ? JSON.stringify(v) : String(v);
}

const same = (a, b) => (a === null || b === null ? a === b : stringValue(a) === stringValue(b));

/** Build the list of visible rows: pending inserts first, then fetched rows. */
function gridRows(tab, meta) {
  const cols = tab.data.columns;
  const out = tab.pending.inserts.map((ins) => ({ type: 'new', key: `new:${ins.id}`, ins }));
  tab.data.rows.forEach((row, idx) => {
    const key = meta.primaryKey.length ? pkKey(pkOf(meta, row, cols)) : `idx:${idx}`;
    out.push({ type: 'row', key, idx, row });
  });
  return out;
}

function cellValue(tab, r, colName) {
  if (r.type === 'new') return colName in r.ins.values ? r.ins.values[colName] : DEFAULT;
  const upd = tab.pending.updates.get(r.key);
  if (upd && colName in upd.values) return upd.values[colName];
  return r.row[tab.data.columns.indexOf(colName)];
}

function isModified(tab, r, colName) {
  if (r.type === 'new') return colName in r.ins.values;
  const upd = tab.pending.updates.get(r.key);
  return !!(upd && colName in upd.values);
}

function renderGrid(tab, meta) {
  const visible = meta.columns.filter((c) => !tab.hidden.includes(c.name));
  const rows = gridRows(tab, meta);
  const reason = editBlockReason(meta);
  const editable = (r, c) => !c.generated && (r.type === 'new' ? !isReadOnly() : !reason) && !(r.type === 'row' && tab.pending.deletes.has(r.key));

  const colgroup = h('colgroup', {}, h('col', { style: { width: '64px' } }),
    visible.map((c) => h('col', { style: { width: `${tab.widths[c.name] || defaultWidth(c)}px` } })));

  const allSelected = rows.length > 0 && rows.every((r) => tab.selected.has(r.key));
  const head = h('tr', {},
    h('th', { class: 'rownum' }, h('input', {
      type: 'checkbox', checked: allSelected, title: 'Select all',
      onchange: (e) => { rows.forEach((r) => e.target.checked ? tab.selected.add(r.key) : tab.selected.delete(r.key)); refreshView(tab); },
    })),
    visible.map((c) => {
      const s = tab.sort.find((x) => x.column === c.name);
      const th = h('th', {
        class: s ? 'sorted' : '',
        title: `${c.name} ${c.dataType}${c.nullable ? '' : ' not null'}${c.default ? ` default ${c.default}` : ''}`,
        onclick: (e) => {
          if (e.target.classList.contains('resizer')) return;
          toggleSort(tab, c.name, e.shiftKey);
        },
      },
      h('div', { class: 'th-inner' },
        c.primary ? h('span', { class: 'pk', title: 'Primary key' }, icon('key', 11)) : null,
        h('span', { class: 'th-name' }, c.name),
        c.references ? h('span', { class: 'fk', title: `→ ${c.references.table}.${c.references.column}` }, icon('link', 11)) : null,
        h('span', { class: 'th-type' }, c.dataType),
        s ? h('span', { class: 'sort-ind' }, s.desc ? '↓' : '↑', tab.sort.length > 1 ? tab.sort.indexOf(s) + 1 : '') : null,
      ),
      h('div', { class: 'resizer', onmousedown: (e) => startResize(e, tab.widths, c.name) }));
      return th;
    }),
  );

  const tbody = h('tbody');
  rows.forEach((r, ri) => {
    const deleted = r.type === 'row' && tab.pending.deletes.has(r.key);
    const tr = h('tr', { class: `${r.type === 'new' ? 'new' : ''}${deleted ? ' deleted' : ''}${tab.selected.has(r.key) ? ' selected' : ''}`, dataset: { ri: String(ri) } });
    const rowNo = r.type === 'new' ? '+' : String(tab.page * tab.pageSize + r.idx + 1);
    tr.append(h('td', { class: 'rownum' },
      h('input', {
        type: 'checkbox', checked: tab.selected.has(r.key),
        onchange: (e) => { e.target.checked ? tab.selected.add(r.key) : tab.selected.delete(r.key); refreshView(tab); },
      }),
      h('span', { class: 'rn' }, rowNo),
      h('button', { class: 'row-open', title: 'Open row', onclick: () => { tab.drawer = { key: r.key }; refreshView(tab); } }, icon('expand', 12)),
    ));
    visible.forEach((c, ci) => {
      const v = cellValue(tab, r, c.name);
      const d = displayValue(v, c);
      const selected = tab.sel && tab.sel.ri === ri && tab.sel.ci === ci;
      const td = h('td', {
        class: `cell ${d.cls}${isModified(tab, r, c.name) ? ' modified' : ''}${selected ? ' sel' : ''}${editable(r, c) ? '' : ' ro'}`,
        dataset: { ci: String(ci) },
      }, h('span', { class: 'cell-text' }, d.text));
      if (c.references && v !== null && v !== DEFAULT && v !== undefined) {
        td.append(h('button', {
          class: 'fk-jump', title: `Open ${c.references.table} where ${c.references.column} = ${stringValue(v)}`,
          onclick: (e) => { e.stopPropagation(); openTable(c.references.schema, c.references.table, { filters: [{ column: c.references.column, op: '=', value: stringValue(v) }] }); },
        }, icon('link', 12)));
      }
      tr.append(td);
    });
    tbody.append(tr);
  });

  const table = h('table', { class: 'grid' }, colgroup, h('thead', {}, head), tbody);
  const wrap = h('div', { class: 'grid-wrap', tabindex: '0' }, table);
  if (!rows.length) wrap.append(h('div', { class: 'grid-empty' }, activeFilters(tab).length ? 'No rows match the filters' : 'This table is empty'));

  const cellAt = (target) => {
    const td = target.closest('td.cell');
    if (!td) return null;
    const ri = Number(td.parentElement.dataset.ri);
    const ci = Number(td.dataset.ci);
    return { td, ri, ci, r: rows[ri], c: visible[ci] };
  };
  const select = (ri, ci) => {
    tab.sel = { ri, ci };
    wrap.querySelectorAll('td.sel').forEach((x) => x.classList.remove('sel'));
    const td = tbody.children[ri]?.children[ci + 1];
    if (td) { td.classList.add('sel'); td.scrollIntoView({ block: 'nearest', inline: 'nearest' }); }
  };
  wrap.addEventListener('mousedown', (e) => {
    const hit = cellAt(e.target);
    if (hit) select(hit.ri, hit.ci);
  });
  wrap.addEventListener('dblclick', (e) => {
    const hit = cellAt(e.target);
    if (!hit) return;
    if (editable(hit.r, hit.c)) openEditor(tab, meta, hit.r, hit.c, hit.td);
    else openViewer(hit.c, cellValue(tab, hit.r, hit.c.name), hit.td);
  });
  wrap.addEventListener('keydown', (e) => {
    if (!tab.sel || e.target !== wrap) return;
    let { ri, ci } = tab.sel;
    const r = rows[ri];
    const c = visible[ci];
    if (!r || !c) return;
    if (e.key === 'ArrowDown') ri = Math.min(rows.length - 1, ri + 1);
    else if (e.key === 'ArrowUp') ri = Math.max(0, ri - 1);
    else if (e.key === 'ArrowRight' || (e.key === 'Tab' && !e.shiftKey)) ci = Math.min(visible.length - 1, ci + 1);
    else if (e.key === 'ArrowLeft' || (e.key === 'Tab' && e.shiftKey)) ci = Math.max(0, ci - 1);
    else if (e.key === 'Enter' || e.key === 'F2') {
      e.preventDefault();
      const td = tbody.children[ri]?.children[ci + 1];
      if (editable(r, c)) openEditor(tab, meta, r, c, td); else openViewer(c, cellValue(tab, r, c.name), td);
      return;
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
      copyText(stringValue(cellValue(tab, r, c.name)));
      return;
    } else if (e.key === 'Escape') {
      tab.sel = null;
      wrap.querySelectorAll('td.sel').forEach((x) => x.classList.remove('sel'));
      return;
    } else return;
    e.preventDefault();
    select(ri, ci);
  });
  return wrap;
}

function toggleSort(tab, column, additive) {
  const existing = tab.sort.find((s) => s.column === column);
  if (!additive) tab.sort = tab.sort.filter((s) => s.column === column);
  if (!existing) tab.sort.push({ column, desc: false });
  else if (!existing.desc) existing.desc = true;
  else tab.sort = tab.sort.filter((s) => s.column !== column);
  tab.page = 0;
  saveTabs();
  loadRows(tab, { keepSelection: true });
}

// widths: { columnName: px }; the new width is stored there and the tabs are saved on mouse up.
function startResize(e, widths, name) {
  e.preventDefault();
  e.stopPropagation();
  const th = e.target.parentElement;
  const table = th.closest('table');
  const idx = [...th.parentElement.children].indexOf(th);
  const col = table.querySelector('colgroup').children[idx];
  const startX = e.clientX;
  const startW = th.getBoundingClientRect().width;
  document.body.classList.add('resizing');
  const move = (ev) => { const w = Math.max(60, startW + ev.clientX - startX); col.style.width = `${w}px`; widths[name] = Math.round(w); };
  const up = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up); document.body.classList.remove('resizing'); saveTabs(); };
  document.addEventListener('mousemove', move);
  document.addEventListener('mouseup', up);
}

// ---------------------------------------------------------------- editing
let popEl = null;
function closePopover() { popEl?.remove(); popEl = null; document.removeEventListener('mousedown', popOutside, true); }
function popOutside(e) { if (popEl && !popEl.contains(e.target)) closePopover(); }

function placePopover(el, anchor) {
  document.body.append(el);
  const r = anchor.getBoundingClientRect();
  const w = el.offsetWidth;
  const hgt = el.offsetHeight;
  let left = r.left;
  let top = r.top;
  if (left + w > window.innerWidth - 8) left = window.innerWidth - w - 8;
  if (top + hgt > window.innerHeight - 8) top = Math.max(8, window.innerHeight - hgt - 8);
  el.style.left = `${Math.max(8, left)}px`;
  el.style.top = `${top}px`;
  popEl = el;
  setTimeout(() => document.addEventListener('mousedown', popOutside, true), 0);
}

// ---------------------------------------------------------------- JSON in cells
/** Cheap check for the grid: the text looks like a JSON object or array. */
function looksLikeJsonText(s) {
  if (typeof s !== 'string' || s.length < 2) return false;
  const a = s.trimStart()[0];
  const b = s.trimEnd().slice(-1);
  return (a === '{' && b === '}') || (a === '[' && b === ']');
}

/** The value is JSON: a json column, or text that parses as a JSON object / array. */
function isJsonValue(v, c) {
  if (v === null || v === undefined || v === DEFAULT) return false;
  if (c && c.kind === 'json') return true;
  if (typeof v === 'object') return true;
  if (!looksLikeJsonText(v)) return false;
  try { JSON.parse(v); return true; } catch { return false; }
}

/**
 * Re-indents JSON text without parsing numbers, so big integers, number formatting and key
 * order stay exactly as stored. indent = 0 gives the compact form.
 */
function reformatJson(text, indent = 2) {
  let out = '';
  let depth = 0;
  let inStr = false;
  const nl = () => (indent ? '\n' + ' '.repeat(depth * indent) : '');
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (inStr) {
      out += ch;
      if (ch === '\\') { out += text[++i] ?? ''; continue; }
      if (ch === '"') inStr = false;
      continue;
    }
    if (ch === '"') { inStr = true; out += ch; continue; }
    if (ch === ' ' || ch === '\n' || ch === '\r' || ch === '\t') continue;
    if (ch === '{' || ch === '[') {
      let j = i + 1;
      while (/\s/.test(text[j] || '')) j++;
      if (text[j] === (ch === '{' ? '}' : ']')) { out += ch + text[j]; i = j; continue; }
      depth++;
      out += ch + nl();
    } else if (ch === '}' || ch === ']') {
      depth = Math.max(0, depth - 1);
      out += nl() + ch;
    } else if (ch === ',') out += ',' + nl();
    else if (ch === ':') out += indent ? ': ' : ':';
    else out += ch;
  }
  return out;
}

const jsonHighlight = CM.HighlightStyle.define([
  { tag: CM.tags.propertyName, class: 't-id' },
  { tag: CM.tags.string, class: 't-str' },
  { tag: CM.tags.number, class: 't-num' },
  { tag: [CM.tags.bool, CM.tags.null], class: 't-const' },
  { tag: [CM.tags.separator, CM.tags.squareBracket, CM.tags.brace, CM.tags.punctuation], class: 't-punct' },
]);

function setEditorText(view, text) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

function jsonError(text) {
  try { JSON.parse(text); return null; } catch (e) { return e.message; }
}

/** A small CodeMirror JSON editor (or viewer, when readOnly). */
function jsonEditor(text, { readOnly = false, onSubmit = null, onCancel = null, onChange = null } = {}) {
  const keys = [];
  if (onSubmit) keys.push({ key: 'Mod-Enter', run: () => { onSubmit(); return true; } });
  if (onCancel) keys.push({ key: 'Escape', run: (v) => { if (CM.searchPanelOpen(v.state)) { CM.closeSearchPanel(v); return true; } onCancel(); return true; } });
  if (!readOnly) keys.push({ key: 'Shift-Alt-f', run: (v) => { setEditorText(v, reformatJson(v.state.doc.toString(), 2)); return true; }, preventDefault: true });
  return new CM.EditorView({
    doc: text,
    extensions: [
      CM.lineNumbers(),
      CM.foldGutter({ openText: '▾', closedText: '▸' }),
      CM.highlightActiveLineGutter(),
      CM.history(),
      CM.drawSelection(),
      CM.bracketMatching(),
      CM.closeBrackets(),
      CM.highlightSelectionMatches(),
      CM.EditorView.lineWrapping,
      CM.EditorState.readOnly.of(readOnly),
      CM.Prec.highest(CM.keymap.of(keys)),
      CM.keymap.of([...CM.closeBracketsKeymap, ...CM.defaultKeymap, ...CM.searchKeymap, ...CM.historyKeymap, ...CM.foldKeymap, CM.indentWithTab]),
      CM.json(),
      CM.syntaxHighlighting(jsonHighlight),
      CM.EditorView.updateListener.of((u) => { if (u.docChanged && onChange) onChange(); }),
    ],
  });
}

/** Popover body for a JSON value: Formatted / Raw switch, fold controls, validity, editor. */
function jsonPane(raw, { readOnly, onSubmit, onCancel }) {
  const compact = !raw.includes('\n');
  let mode = store.get('rs.jsonMode', 'formatted');
  const status = h('span', { class: 'json-status' });
  const check = () => {
    const err = jsonError(view.state.doc.toString());
    status.textContent = err ? `Invalid JSON: ${err}` : 'Valid JSON';
    status.className = `json-status ${err ? 'bad' : 'ok'}`;
  };
  const view = jsonEditor(mode === 'formatted' ? reformatJson(raw, 2) : raw, { readOnly, onSubmit, onCancel, onChange: () => check() });
  const seg = h('div', { class: 'seg' });
  const setMode = (m) => {
    mode = m;
    store.set('rs.jsonMode', m);
    for (const b of seg.children) b.classList.toggle('on', b.dataset.m === m);
    const cur = view.state.doc.toString();
    const next = m === 'formatted' ? reformatJson(cur, 2) : (readOnly ? raw : reformatJson(cur, 0));
    // A read-only view still accepts programmatic changes.
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
  };
  seg.append(
    h('button', { dataset: { m: 'formatted' }, class: mode === 'formatted' ? 'on' : '', onclick: () => setMode('formatted') }, 'Formatted'),
    h('button', { dataset: { m: 'raw' }, class: mode === 'raw' ? 'on' : '', onclick: () => setMode('raw') }, 'Raw'),
  );
  // Collapse: fold every nested object / array, keep the top-level keys visible.
  const fold = (collapse) => {
    CM.unfoldAll(view);
    if (collapse) {
      const { state } = view;
      const effects = [];
      for (let n = 2; n <= state.doc.lines; n++) {
        const line = state.doc.line(n);
        if (!/^ {2}\S/.test(line.text)) continue;
        const range = CM.foldable(state, line.from, line.to);
        if (range) effects.push(CM.foldEffect.of(range));
      }
      if (effects.length) view.dispatch({ effects });
      else CM.foldAll(view);
    }
    view.focus();
  };
  const bar = h('div', { class: 'json-bar' }, seg,
    h('button', { class: 'btn ghost sm', title: 'Collapse all (Ctrl+Alt+[)', onclick: () => fold(true) }, 'Collapse'),
    h('button', { class: 'btn ghost sm', title: 'Expand all (Ctrl+Alt+])', onclick: () => fold(false) }, 'Expand'),
    h('span', { class: 'spacer' }), status);
  check();
  // The value to store keeps the original layout: compact JSON stays compact.
  const value = () => {
    const t = view.state.doc.toString();
    if (jsonError(t)) return t;
    return compact ? reformatJson(t, 0) : t;
  };
  return { dom: h('div', { class: 'json-pane' }, bar, h('div', { class: 'json-editor' }, view.dom)), view, value };
}

function editorText(v, c) {
  if (v === null || v === undefined || v === DEFAULT) return '';
  if (isJsonValue(v, c)) return reformatJson(typeof v === 'string' ? v : JSON.stringify(v), 2);
  return stringValue(v);
}

function openEditor(tab, meta, r, c, anchor) {
  closePopover();
  const current = cellValue(tab, r, c.name);
  const commit = (value) => {
    closePopover();
    setCell(tab, meta, r, c.name, value);
    $('.grid-wrap')?.focus({ preventScroll: true });
  };
  const cancel = () => { closePopover(); $('.grid-wrap')?.focus({ preventScroll: true }); };
  const head = h('div', { class: 'pop-head' }, h('b', {}, c.name), h('span', { class: 'muted' }, c.dataType));
  const actions = [];
  if (c.nullable) actions.push(h('button', { class: 'btn ghost sm', onclick: () => commit(null) }, 'Set NULL'));
  if (r.type === 'new' && (c.default != null || c.auto || c.nullable)) actions.push(h('button', { class: 'btn ghost sm', onclick: () => commit(DEFAULT) }, 'Use default'));

  let field;
  let getValue;
  let jsonMode = false;
  const options = c.kind === 'boolean' ? ['true', 'false'] : c.kind === 'enum' && c.enumValues ? c.enumValues : null;
  if (options) {
    const cur = current === DEFAULT || current == null ? '' : (c.kind === 'boolean' ? String(current === true || current === 1 || current === 't' || current === 'true') : String(current));
    field = h('div', { class: 'opt-list' }, options.map((o) => h('button', {
      class: `opt${o === cur ? ' on' : ''}`,
      onclick: () => commit(c.kind === 'boolean' ? o === 'true' : o),
    }, o)));
    getValue = null;
  } else if (isJsonValue(current, c) || (c.kind === 'json' && (current == null || current === DEFAULT))) {
    const raw = current == null || current === DEFAULT ? '' : (typeof current === 'string' ? current : JSON.stringify(current));
    const pane = jsonPane(raw, { readOnly: false, onSubmit: () => commit(pane.value()), onCancel: () => cancel() });
    field = pane.dom;
    field.focus = () => pane.view.focus();
    getValue = pane.value;
    jsonMode = true;
  } else {
    const multiline = ['json', 'text', 'array'].includes(c.kind);
    field = h(multiline ? 'textarea' : 'input', {
      class: `input mono${multiline ? ' area' : ''}`, value: editorText(current, c), spellcheck: 'false',
      rows: c.kind === 'json' ? 10 : 4,
    });
    getValue = () => {
      const s = field.value;
      if (c.kind === 'json') { try { return JSON.stringify(JSON.parse(s)); } catch { return s; } }
      return s;
    };
    field.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); cancel(); }
      else if (e.key === 'Enter' && (!multiline || e.ctrlKey || e.metaKey || (!e.shiftKey && c.kind !== 'json'))) { e.preventDefault(); commit(getValue()); }
    });
  }
  const el = h('div', { class: `popover editor${options ? '' : ' wide'}${jsonMode ? ' json' : ''}` },
    head, field,
    h('div', { class: 'pop-actions' }, actions, h('span', { class: 'spacer' }),
      h('button', { class: 'btn ghost sm', onclick: cancel }, 'Cancel'),
      getValue ? h('button', { class: 'btn primary sm', onclick: () => commit(getValue()) }, 'Apply') : null),
    getValue ? h('div', { class: 'pop-hint' }, jsonMode ? 'Ctrl+Enter to apply · Shift+Alt+F to format · Esc to cancel' : (c.kind === 'json' ? 'Ctrl+Enter to apply · Esc to cancel' : 'Enter to apply · Shift+Enter for new line · Esc to cancel')) : null,
  );
  el.addEventListener('keydown', (e) => { if (e.key === 'Escape' && !e.defaultPrevented) cancel(); });
  placePopover(el, anchor);
  if (field.focus) { field.focus(); if (field.select && c.kind !== 'json') field.select(); }
}

function openViewer(c, v, anchor) {
  closePopover();
  const json = isJsonValue(v, c);
  const raw = typeof v === 'object' && v !== null ? JSON.stringify(v) : stringValue(v);
  const pane = json ? jsonPane(raw, { readOnly: true, onCancel: closePopover }) : null;
  const el = h('div', { class: `popover editor wide${json ? ' json' : ''}` },
    h('div', { class: 'pop-head' }, h('b', {}, c.name), h('span', { class: 'muted' }, c.dataType || c.type || ''),
      json && c.kind !== 'json' ? h('span', { class: 'badge muted' }, 'json in text') : null, h('span', { class: 'badge muted' }, 'read-only')),
    pane ? pane.dom : h('textarea', { class: 'input mono area', readonly: true, value: v === null ? 'NULL' : raw, rows: 8 }),
    h('div', { class: 'pop-actions' }, h('span', { class: 'spacer' }),
      h('button', { class: 'btn ghost sm', onclick: () => copyText(pane ? pane.view.state.doc.toString() : raw) }, icon('copy', 13), 'Copy'),
      h('button', { class: 'btn ghost sm', onclick: closePopover }, 'Close')),
  );
  el.addEventListener('keydown', (e) => { if (e.key === 'Escape' && !e.defaultPrevented) closePopover(); });
  placePopover(el, anchor);
}

function setCell(tab, meta, r, colName, value) {
  if (r.type === 'new') {
    if (value === DEFAULT) delete r.ins.values[colName]; else r.ins.values[colName] = value;
  } else {
    const original = r.row[tab.data.columns.indexOf(colName)];
    let upd = tab.pending.updates.get(r.key);
    if (same(original, value)) {
      if (upd) { delete upd.values[colName]; if (!Object.keys(upd.values).length) tab.pending.updates.delete(r.key); }
    } else {
      if (!upd) { upd = { pk: pkOf(meta, r.row, tab.data.columns), values: {} }; tab.pending.updates.set(r.key, upd); }
      upd.values[colName] = value;
    }
  }
  refreshView(tab);
}

let insertSeq = 1;
function addRecord(tab, meta) {
  if (tab.view !== 'data') tab.view = 'data';
  const ins = { id: insertSeq++, values: {} };
  tab.pending.inserts.unshift(ins);
  tab.sel = { ri: 0, ci: 0 };
  refreshView(tab);
  // Open the first editable column without a default.
  const visible = meta.columns.filter((c) => !tab.hidden.includes(c.name));
  const ci = visible.findIndex((c) => !c.auto && !c.generated);
  if (ci >= 0) {
    const td = $('.grid tbody tr')?.children[ci + 1];
    if (td) { tab.sel = { ri: 0, ci }; td.classList.add('sel'); openEditor(tab, meta, { type: 'new', key: `new:${ins.id}`, ins }, visible[ci], td); }
  }
}

function markDeleteSelected(tab, meta) {
  const rows = gridRows(tab, meta);
  for (const r of rows) {
    if (!tab.selected.has(r.key)) continue;
    if (r.type === 'new') tab.pending.inserts = tab.pending.inserts.filter((x) => x !== r.ins);
    else if (tab.pending.deletes.has(r.key)) tab.pending.deletes.delete(r.key);
    else tab.pending.deletes.set(r.key, pkOf(meta, r.row, tab.data.columns));
  }
  tab.selected.clear();
  refreshView(tab);
}

function discardChanges(tab) {
  tab.pending = { updates: new Map(), inserts: [], deletes: new Map() };
  refreshView(tab);
}

async function saveChanges(tab) {
  const changes = [];
  for (const ins of [...tab.pending.inserts].reverse()) changes.push({ type: 'insert', values: ins.values });
  for (const [key, u] of tab.pending.updates) if (!tab.pending.deletes.has(key)) changes.push({ type: 'update', pk: u.pk, values: u.values });
  for (const pk of tab.pending.deletes.values()) changes.push({ type: 'delete', pk });
  if (!changes.length) return;
  try {
    const res = await api(`/connections/${state.connId}/apply`, { body: { schema: tab.schema, table: tab.table, changes } });
    tab.pending = { updates: new Map(), inserts: [], deletes: new Map() };
    toast(`Saved ${changes.length} change${changes.length === 1 ? '' : 's'} (${res.rowsAffected} row${res.rowsAffected === 1 ? '' : 's'} affected)`, 'success');
    await loadRows(tab);
  } catch (e) {
    toast(e.message, 'error');
  }
}

// ---------------------------------------------------------------- row drawer
function renderDrawer(tab, meta) {
  const rows = tab.data ? gridRows(tab, meta) : [];
  const r = rows.find((x) => x.key === tab.drawer.key);
  const close = () => { tab.drawer = null; refreshView(tab); };
  if (!r) { tab.drawer = null; return null; }
  const reason = editBlockReason(meta);
  const editable = (c) => !c.generated && (r.type === 'new' ? !isReadOnly() : !reason) && !tab.pending.deletes.has(r.key);
  const ri = rows.indexOf(r);
  const nav = (d) => { const n = rows[ri + d]; if (n) { tab.drawer = { key: n.key }; refreshView(tab); } };

  const fields = meta.columns.map((c) => {
    const v = cellValue(tab, r, c.name);
    const d = displayValue(v, c);
    const valueEl = h('div', {
      class: `field-value ${d.cls}${isModified(tab, r, c.name) ? ' modified' : ''}${editable(c) ? ' editable' : ''}`,
      onclick: (e) => editable(c) ? openEditor(tab, meta, r, c, e.currentTarget) : openViewer(c, v, e.currentTarget),
    }, isJsonValue(v, c) ? h('pre', { html: highlightJson(editorText(v, c)) }) : d.text);
    return h('div', { class: 'field' },
      h('div', { class: 'field-label' },
        c.primary ? icon('key', 11) : null,
        h('span', {}, c.name), h('span', { class: 'th-type' }, c.dataType),
        c.references && v !== null && v !== DEFAULT ? h('button', {
          class: 'link-btn', onclick: () => openTable(c.references.schema, c.references.table, { filters: [{ column: c.references.column, op: '=', value: stringValue(v) }] }),
        }, `→ ${c.references.table}`, icon('link', 11)) : null),
      valueEl,
    );
  });

  // Reverse relations: tables whose foreign keys point at this table.
  const reverse = [];
  if (r.type === 'row') {
    for (const t of state.schema.tables) {
      for (const fk of t.foreignKeys) {
        if (fk.refTable === meta.name && fk.refSchema === meta.schema && fk.columns.length === 1) {
          const v = cellValue(tab, r, fk.refColumns[0]);
          if (v === null || v === undefined) continue;
          reverse.push(h('button', {
            class: 'rel-btn',
            onclick: () => openTable(t.schema, t.name, { filters: [{ column: fk.columns[0], op: '=', value: stringValue(v) }], newTab: true }),
          }, icon('table', 12), h('span', {}, `${t.name}`), h('span', { class: 'muted' }, `.${fk.columns[0]}`)));
        }
      }
    }
  }

  return h('div', { class: 'drawer' },
    h('div', { class: 'drawer-head' },
      h('b', {}, r.type === 'new' ? 'New record' : `Row ${tab.page * tab.pageSize + r.idx + 1}`),
      h('span', { class: 'spacer' }),
      h('button', { class: 'icon-btn', disabled: ri <= 0, onclick: () => nav(-1), title: 'Previous row' }, icon('left')),
      h('button', { class: 'icon-btn', disabled: ri >= rows.length - 1, onclick: () => nav(1), title: 'Next row' }, icon('right')),
      h('button', { class: 'icon-btn', title: 'Copy as JSON', onclick: () => copyText(JSON.stringify(Object.fromEntries(meta.columns.map((c) => [c.name, cellValue(tab, r, c.name) === DEFAULT ? undefined : cellValue(tab, r, c.name)])), null, 2)) }, icon('copy')),
      h('button', { class: 'icon-btn', onclick: close, title: 'Close' }, icon('x')),
    ),
    h('div', { class: 'drawer-body' },
      fields,
      reverse.length ? h('div', { class: 'field' }, h('div', { class: 'field-label' }, h('span', {}, 'Referenced by')), h('div', { class: 'rel-list' }, reverse)) : null,
    ),
  );
}

// ---------------------------------------------------------------- structure view
function renderStructure(meta) {
  const referencedBy = [];
  for (const t of state.schema.tables) for (const fk of t.foreignKeys) if (fk.refTable === meta.name && fk.refSchema === meta.schema) referencedBy.push({ t, fk });
  const section = (title, head, rows) => h('div', { class: 'struct-section' },
    h('h4', {}, title, h('span', { class: 'muted' }, ` ${rows.length}`)),
    rows.length ? h('table', { class: 'struct' }, h('thead', {}, h('tr', {}, head.map((x) => h('th', {}, x)))), h('tbody', {}, rows))
      : h('div', { class: 'muted small' }, 'None'));
  return h('div', { class: 'structure' },
    section('Columns', ['', 'Name', 'Type', 'Nullable', 'Default', 'References'], meta.columns.map((c) => h('tr', {},
      h('td', { class: 'flags' }, c.primary ? h('span', { class: 'pk', title: 'Primary key' }, icon('key', 12)) : null, c.auto ? h('span', { class: 'badge muted' }, c.generated ? 'generated' : 'auto') : null),
      h('td', { class: 'mono' }, c.name),
      h('td', { class: 'mono type' }, c.dataType, c.enumValues ? h('div', { class: 'muted small' }, c.enumValues.join(' | ')) : null),
      h('td', {}, c.nullable ? 'yes' : h('b', {}, 'not null')),
      h('td', { class: 'mono muted' }, c.default ?? ''),
      h('td', { class: 'mono' }, c.references ? h('button', { class: 'link-btn', onclick: () => openTable(c.references.schema, c.references.table) }, `${c.references.table}.${c.references.column}`) : ''),
    ))),
    section('Indexes', ['Name', 'Columns', 'Unique', 'Definition'], meta.indexes.map((i) => h('tr', {},
      h('td', { class: 'mono' }, i.name), h('td', { class: 'mono' }, i.columns.join(', ')), h('td', {}, i.unique ? 'yes' : ''), h('td', { class: 'mono muted small' }, i.definition || '')))),
    section('Foreign keys', ['Name', 'Columns', 'References'], meta.foreignKeys.map((f) => h('tr', {},
      h('td', { class: 'mono' }, f.name), h('td', { class: 'mono' }, f.columns.join(', ')),
      h('td', { class: 'mono' }, h('button', { class: 'link-btn', onclick: () => openTable(f.refSchema, f.refTable) }, `${f.refTable}(${f.refColumns.join(', ')})`))))),
    section('Referenced by', ['Table', 'Columns', 'Target'], referencedBy.map(({ t, fk }) => h('tr', {},
      h('td', { class: 'mono' }, h('button', { class: 'link-btn', onclick: () => openTable(t.schema, t.name) }, t.name)),
      h('td', { class: 'mono' }, fk.columns.join(', ')), h('td', { class: 'mono' }, fk.refColumns.join(', '))))),
  );
}

// ---------------------------------------------------------------- SQL console
const SQL_KEYWORDS = ('select from where and or not in is null like ilike between as on join left right inner outer full cross group by order having limit offset '
  + 'insert into values update set delete create table view index drop alter add column primary key foreign references unique default distinct union all '
  + 'except intersect case when then else end returning with recursive exists asc desc nulls first last true false begin commit rollback transaction '
  + 'explain analyze show pragma truncate cascade restrict if replace using natural lateral window over partition rows range count sum avg min max coalesce cast '
  + 'conflict do nothing constraint check schema database grant revoke to of only similar any some array filter within materialized refresh concurrently '
  + 'vacuum auto_increment engine charset collate unsigned temporary temp trigger function procedure returns language declare return each row for execute '
  + 'comment rename owner type extension lock share nowait skip locked fetch next generated always identity stored virtual without zone at local interval '
  + 'describe use ignore duplicate preceding following unbounded current groups exclude ties sequence start increment savepoint release isolation level '
  + 'read write committed serializable deferrable initially deferred immediate match partial simple action no enable disable after before instead').split(' ');

const SQL_TYPES = new Set(('int integer smallint bigint serial bigserial smallserial real double precision float numeric decimal money boolean bool char '
  + 'character varchar varying nchar nvarchar text citext bytea blob clob date time timestamp timestamptz timetz interval uuid json jsonb xml inet cidr macaddr '
  + 'point line polygon tinyint mediumint tinytext mediumtext longtext tinyblob mediumblob longblob varbinary binary datetime year enum bit varbit tsvector '
  + 'tsquery oid regclass int2 int4 int8 float4 float8 hstore geometry geography').split(' '));
const SQL_CONSTANTS = new Set('null true false unknown current_date current_time current_timestamp localtime localtimestamp current_user session_user'.split(' '));
const SQL_KW_SET = new Set(SQL_KEYWORDS);
// keywords that stay keywords even when followed by "("
const SQL_KW_PAREN = new Set('in exists values using over filter within any some all array as on not and or into table check primary key unique references foreign conflict when then else'.split(' '));

function escapeHtml(s) { return s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])); }

function makeSqlTokenRe(mysql) {
  return new RegExp([
    String.raw`(?<com>--[^\n]*|\/\*[\s\S]*?(?:\*\/|$)${mysql ? String.raw`|#[^\n]*` : ''})`,
    String.raw`(?<dollar>\$(?<tag>[A-Za-z_]\w*)?\$[\s\S]*?(?:\$\k<tag>\$|$))`,
    mysql
      ? String.raw`(?<str>(?:[NnBbXx](?='))?'(?:[^'\]|''|\.)*(?:'|$)|"(?:[^"\]|""|\.)*(?:"|$))`
      : String.raw`(?<str>[Ee]'(?:[^'\]|''|\.)*(?:'|$)|(?:[NnBbXx](?='))?'(?:[^']|'')*(?:'|$))`,
    mysql ? String.raw`(?<qid>\x60(?:[^\x60]|\x60\x60)*(?:\x60|$))` : String.raw`(?<qid>"(?:[^"]|"")*(?:"|$)|\x60[^\x60]*(?:\x60|$))`,
    String.raw`(?<num>\b0x[0-9a-fA-F]+\b|(?:\b\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?\b)`,
    String.raw`(?<param>\$\d+|\?(?![|&])|(?<!:):[A-Za-z_]\w*|@@?[A-Za-z_][\w.]*)`,
    String.raw`(?<word>[A-Za-z_][\w$]*)`,
    String.raw`(?<op>::|->>|->|#>>|#>|@>|<@|\?\||\?&|<=|>=|<>|!=|\|\||&&|[=<>+\-*/%~^!|&#@])`,
    String.raw`(?<punct>[(),;.\[\]{}])`,
  ].join('|'), 'g');
}
const SQL_TOKEN_RE = { std: makeSqlTokenRe(false), mysql: makeSqlTokenRe(true) };
// type names that take a length/precision argument: varchar(255) stays a type, date(x) is a function call
const SQL_PARAM_TYPES = new Set('varchar char character numeric decimal float bit varbit varbinary binary nvarchar nchar timestamp time interval enum int tinyint smallint mediumint bigint double'.split(' '));

/** Tokenizes SQL into colored ranges: [{ from, to, cls }]. */
function sqlTokens(src) {
  const out = [];
  const re = backtickSql() ? SQL_TOKEN_RE.mysql : SQL_TOKEN_RE.std;
  for (const m of src.matchAll(re)) {
    const tok = m[0];
    if (!tok) continue;
    const from = m.index;
    const to = from + tok.length;
    const g = m.groups;
    let cls = null;
    if (g.com) cls = 't-com';
    else if (g.dollar || g.str) cls = 't-str';
    else if (g.qid) cls = 't-id';
    else if (g.num) cls = 't-num';
    else if (g.param) cls = 't-param';
    else if (g.op) cls = 't-op';
    else if (g.punct) cls = 't-punct';
    else {
      const w = tok.toLowerCase();
      const prev = src.slice(Math.max(0, from - 40), from).trimEnd();
      const afterDot = prev.endsWith('.') && !prev.endsWith('..');
      const callNext = /^\s*\(/.test(src.slice(to, to + 40));
      if (afterDot) cls = null; // table.column — plain identifier
      else if (SQL_CONSTANTS.has(w)) cls = 't-const';
      else if (SQL_TYPES.has(w) && (!callNext || SQL_PARAM_TYPES.has(w))) cls = 't-type';
      else if (callNext && !SQL_KW_PAREN.has(w) && !/\b(?:into|table|references|from|join|update|view|index|on)$/i.test(prev)) cls = 't-fn';
      else if (SQL_KW_SET.has(w)) cls = 't-kw';
    }
    if (cls) out.push({ from, to, cls });
  }
  return out;
}

// ---------------------------------------------------------------- SQL autocomplete
const AC_PHRASES = ['order by', 'group by', 'left join', 'right join', 'inner join', 'full join', 'cross join', 'insert into', 'delete from', 'primary key', 'foreign key', 'is not null', 'is null', 'not in', 'not exists', 'union all', 'on conflict', 'do nothing'];
const AC_FUNCTIONS = {
  common: 'count sum avg min max coalesce nullif lower upper length trim substring replace round abs cast concat greatest least row_number rank dense_rank lag lead'.split(' '),
  postgres: 'now date_trunc to_char to_timestamp to_date extract age json_agg jsonb_agg array_agg string_agg json_build_object jsonb_build_object jsonb_array_elements jsonb_each jsonb_set gen_random_uuid generate_series unnest array_length regexp_replace split_part position initcap left right lpad rpad md5 random ceil floor date_part current_date current_timestamp'.split(' '),
  mysql: 'now curdate curtime date_format str_to_date date_add date_sub datediff timestampdiff unix_timestamp from_unixtime ifnull if group_concat json_extract json_unquote json_object json_array json_arrayagg uuid left right lpad rpad locate instr md5 rand ceil floor year month day'.split(' '),
  sqlite: 'ifnull iif datetime date time strftime julianday unixepoch json json_extract json_object json_array json_group_array group_concat typeof random printf instr hex quote total ltrim rtrim'.split(' '),
  bigquery: 'current_date current_timestamp current_datetime date_trunc timestamp_trunc datetime_trunc date_diff timestamp_diff date_add date_sub timestamp_add format_date format_timestamp parse_date parse_timestamp extract safe_cast safe_divide ifnull if countif count_if approx_count_distinct array_agg array_length array_to_string string_agg split unnest struct generate_array generate_date_array json_value json_query json_extract json_extract_scalar to_json_string parse_json regexp_contains regexp_extract regexp_replace starts_with ends_with farm_fingerprint md5 sha256 any_value logical_and logical_or st_geogpoint st_distance',
};
const AC_TABLE_CONTEXT = new Set(['from', 'join', 'into', 'update', 'table', 'describe']);
const AC_CLAUSE_KW = /\b(select|from|join|where|on|set|into|update|table|by|having|values|returning|using|describe)\b/gi;
const AC_NOT_ALIAS = new Set('where on join left right inner outer full cross natural group order limit offset set values using having union except intersect returning window as'.split(' '));
const IDENT = String.raw`(?:"[^"]+"|\`[^\`]+\`|\[[^\]]+\]|[A-Za-z_][\w$]*)`;

const unquoteIdent = (s) => (/^["`[]/.test(s) ? s.slice(1, -1) : s);

function acStatementAt(text, pos) {
  // the statement under the caret, with string and comment contents blanked out
  const clean = text.replace(/--[^\n]*|\/\*[\s\S]*?(?:\*\/|$)|'(?:[^']|'')*'?/g, (m) => ' '.repeat(m.length));
  const start = clean.lastIndexOf(';', pos - 1) + 1;
  let end = clean.indexOf(';', pos);
  if (end < 0) end = clean.length;
  return { stmt: clean.slice(start, end), offset: pos - start };
}

function acResolveTable(ref) {
  const tables = state.schema?.tables || [];
  const parts = ref.match(new RegExp(IDENT, 'g'))?.map(unquoteIdent) || [];
  if (!parts.length) return null;
  const name = parts[parts.length - 1].toLowerCase();
  if (parts.length > 1) {
    const sc = parts[parts.length - 2].toLowerCase();
    return tables.find((t) => t.schema.toLowerCase() === sc && t.name.toLowerCase() === name) || null;
  }
  const order = [state.schema.defaultSchema, state.schemaName];
  for (const sc of order) {
    const t = tables.find((x) => x.schema === sc && x.name.toLowerCase() === name);
    if (t) return t;
  }
  return tables.find((t) => t.name.toLowerCase() === name) || null;
}

// alias (lowercase) -> table, for every table referenced in the statement
function acReferencedTables(stmt) {
  const re = new RegExp(String.raw`(?:\b(?:from|join|update|into)\b|,)\s+(${IDENT}(?:\s*\.\s*${IDENT})?)(?:\s+(?:as\s+)?(${IDENT}))?`, 'gi');
  const map = new Map();
  for (const m of stmt.matchAll(re)) {
    const t = acResolveTable(m[1]);
    if (!t) continue;
    map.set(t.name.toLowerCase(), t);
    const alias = m[2] && unquoteIdent(m[2]);
    if (alias && !AC_NOT_ALIAS.has(alias.toLowerCase())) map.set(alias.toLowerCase(), t);
  }
  return map;
}

function acQuote(name) {
  const dialect = state.schema?.dialect;
  const simple = dialect === 'postgres' ? /^[a-z_][a-z0-9_$]*$/ : /^[A-Za-z_][\w$]*$/;
  if (simple.test(name) && !SQL_KEYWORDS.includes(name.toLowerCase())) return name;
  if (dialect === 'bigquery') return `\`${name.replace(/[\\`]/g, '\\$&')}\``;
  return dialect === 'mysql' ? `\`${name.replace(/`/g, '``')}\`` : `"${name.replace(/"/g, '""')}"`;
}

function acTableText(t) {
  const sameSchema = t.schema === state.schema.defaultSchema || state.schema.dialect === 'sqlite';
  return sameSchema ? acQuote(t.name) : `${acQuote(t.schema)}.${acQuote(t.name)}`;
}

// Returns { from, to, prefix, items } for the caret position, or null.
function acSuggest(text, pos, forced) {
  if (!state.schema) return null;
  const before = text.slice(0, pos);
  const word = /[A-Za-z_][\w$]*$/.exec(before)?.[0] || '';
  const from = pos - word.length;
  const { stmt, offset } = acStatementAt(text, pos);
  const head = stmt.slice(0, offset - word.length);
  if (/'(?:[^']|'')*$/.test(text.slice(text.lastIndexOf('\n', pos - 1) + 1, pos).replace(/'(?:[^']|'')*'/g, ''))) return null; // inside a string
  if (/--[^\n]*$/.test(before.slice(before.lastIndexOf('\n') + 1))) return null; // inside a comment
  const qual = new RegExp(String.raw`(${IDENT})\s*\.\s*$`).exec(head);
  if (!word && !qual && !forced) return null;

  const refs = acReferencedTables(stmt);
  const items = [];
  const dialect = state.schema.dialect;
  const tables = state.schema.tables;

  if (qual) {
    const q = unquoteIdent(qual[1]).toLowerCase();
    const t = refs.get(q) || acResolveTable(qual[1]);
    if (t) for (const c of t.columns) items.push({ label: c.name, insert: acQuote(c.name), kind: 'column', detail: c.dataType, pk: c.primary });
    for (const x of tables.filter((x) => x.schema.toLowerCase() === q)) {
      items.push({ label: x.name, insert: acQuote(x.name), kind: x.kind === 'table' ? 'table' : 'view', detail: x.kind.replace('_', ' ') });
    }
  } else {
    const lastKw = [...head.matchAll(AC_CLAUSE_KW)].pop()?.[1].toLowerCase();
    const tableCtx = AC_TABLE_CONTEXT.has(lastKw) && !/\b(?:on|where)\b|\(\s*$/i.test(head.slice(head.toLowerCase().lastIndexOf(lastKw)));
    const seen = new Set();
    if (!tableCtx) {
      for (const [alias, t] of refs) {
        if (seen.has(t)) continue;
        seen.add(t);
        for (const c of t.columns) items.push({ label: c.name, insert: acQuote(c.name), kind: 'column', detail: `${c.dataType} · ${t.name}`, pk: c.primary, rank: 0 });
        if (alias !== t.name.toLowerCase()) items.push({ label: alias, insert: alias, kind: 'alias', detail: t.name, rank: 1 });
      }
    }
    const visible = tables.filter((t) => t.schema === state.schemaName || t.schema === state.schema.defaultSchema);
    for (const t of new Set(visible)) {
      items.push({ label: t.name, insert: acTableText(t), kind: t.kind === 'table' ? 'table' : 'view', detail: t.schema !== state.schema.defaultSchema ? t.schema : t.kind.replace('_', ' '), rank: tableCtx ? 0 : 2 });
    }
    if (dialect !== 'sqlite') {
      for (const s of state.schema.schemas) items.push({ label: s, insert: acQuote(s), kind: 'schema', detail: schemaWord(dialect).toLowerCase(), rank: tableCtx ? 1 : 4 });
    }
    if (!tableCtx || !word) {
      const upper = /^[A-Z]/.test(word);
      const kwCase = (k) => (upper ? k.toUpperCase() : k);
      const fns = new Set([...AC_FUNCTIONS.common, ...(AC_FUNCTIONS[dialect] || [])]);
      for (const k of [...SQL_KEYWORDS, ...AC_PHRASES]) if (!fns.has(k)) items.push({ label: kwCase(k), insert: kwCase(k), kind: 'keyword', rank: 3 });
      for (const f of fns) {
        items.push({ label: kwCase(f), insert: kwCase(f), kind: 'function', detail: 'function', fn: true, rank: 3 });
      }
    }
  }

  const p = word.toLowerCase();
  const scored = [];
  const dedupe = new Set();
  for (const it of items) {
    const l = it.label.toLowerCase();
    let s;
    if (!p) s = 0;
    else if (l.startsWith(p)) s = 0;
    else if (l.includes('_' + p) || l.includes(' ' + p)) s = 1;
    else if (l.includes(p)) s = 2;
    else continue;
    if (p && l === p) continue; // already typed in full
    const key = `${it.kind}:${l}:${it.detail || ''}`;
    if (dedupe.has(key)) continue;
    dedupe.add(key);
    scored.push({ ...it, score: s * 10 + (it.rank || 0), idx: scored.length });
  }
  scored.sort((a, b) => a.score - b.score || (p ? a.label.length - b.label.length || a.label.localeCompare(b.label) : a.idx - b.idx));
  if (!scored.length) return null;
  return { from, to: pos, prefix: word, items: scored.slice(0, 80) };
}

// ---------------------------------------------------------------- SQL editor (CodeMirror 6)
/** Splits SQL text into statements. Skips strings, quoted identifiers, comments and dollar-quoted bodies. */
function splitSqlStatements(text) {
  const mysql = backtickSql();
  const out = [];
  const n = text.length;
  let start = -1;
  let i = 0;
  while (i < n) {
    const c = text[i];
    const d = text[i + 1];
    if ((c === '-' && d === '-') || (mysql && c === '#')) { const e = text.indexOf('\n', i); i = e < 0 ? n : e; continue; }
    if (c === '/' && d === '*') { const e = text.indexOf('*/', i + 2); i = e < 0 ? n : e + 2; continue; }
    if (c === ';') { if (start >= 0) out.push({ from: start, to: i + 1 }); start = -1; i++; continue; }
    if (start < 0 && !/\s/.test(c)) start = i;
    if (c === "'" || c === '"' || c === '`') {
      i++;
      while (i < n) {
        if (mysql && text[i] === '\\') { i += 2; continue; }
        if (text[i] === c) { if (text[i + 1] === c) { i += 2; continue; } break; }
        i++;
      }
      i++;
      continue;
    }
    if (c === '$' && !mysql) {
      const m = /^\$(?:[A-Za-z_]\w*)?\$/.exec(text.slice(i, i + 64));
      if (m) { const e = text.indexOf(m[0], i + m[0].length); i = e < 0 ? n : e + m[0].length; continue; }
    }
    i++;
  }
  if (start >= 0) out.push({ from: start, to: n });
  return out.map((s) => {
    const raw = text.slice(s.from, s.to);
    return { from: s.from, to: s.from + raw.trimEnd().length, text: raw.trim() };
  });
}

/** The statement under the cursor; on a blank line, the nearest statement above it. */
function statementAt(text, pos) {
  const list = splitSqlStatements(text);
  return list.find((s) => pos >= s.from && pos <= s.to)
    || [...list].reverse().find((s) => s.to <= pos)
    || list[0]
    || null;
}

const MARK_CACHE = new Map();
const markFor = (cls) => {
  if (!MARK_CACHE.has(cls)) MARK_CACHE.set(cls, CM.Decoration.mark({ class: cls }));
  return MARK_CACHE.get(cls);
};

/** Colors tokens with the same tokenizer and theme variables as the rest of the app. */
const sqlHighlighter = CM.ViewPlugin.fromClass(class {
  constructor(view) { this.decorations = this.build(view); }
  update(u) { if (u.docChanged || u.viewportChanged) this.decorations = this.build(u.view); }
  build(view) {
    const text = view.state.doc.toString();
    const ranges = [];
    for (const t of sqlTokens(text)) ranges.push(markFor(t.cls).range(t.from, t.to));
    return CM.Decoration.set(ranges, true);
  }
}, { decorations: (v) => v.decorations });

/** Briefly marks the statement that was sent to the server. */
const flashEffect = CM.StateEffect.define();
const flashField = CM.StateField.define({
  create: () => CM.Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) if (e.is(flashEffect)) deco = e.value ? CM.Decoration.set([CM.Decoration.mark({ class: 'cm-ran' }).range(e.value.from, e.value.to)]) : CM.Decoration.none;
    return deco;
  },
  provide: (f) => CM.EditorView.decorations.from(f),
});
function flashRange(view, range) {
  if (!range || range.to <= range.from) return;
  view.dispatch({ effects: flashEffect.of(range) });
  setTimeout(() => { try { view.dispatch({ effects: flashEffect.of(null) }); } catch { /* view destroyed */ } }, 700);
}

class RunMarker extends CM.GutterMarker {
  toDOM() {
    const el = document.createElement('span');
    el.className = 'cm-run-btn';
    el.title = 'Run this statement';
    el.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15a1 1 0 0 0 1.5.86l12.5-7.5a1 1 0 0 0 0-1.72L8.5 3.64A1 1 0 0 0 7 4.5z"/></svg>';
    return el;
  }
}
const RUN_MARKER = new RunMarker();

function runGutter(tab) {
  return CM.gutter({
    class: 'cm-run-gutter',
    markers: (view) => {
      const doc = view.state.doc;
      const seen = new Set();
      const ranges = [];
      for (const s of splitSqlStatements(doc.toString())) {
        const line = doc.lineAt(s.from);
        if (seen.has(line.from)) continue;
        seen.add(line.from);
        ranges.push(RUN_MARKER.range(line.from));
      }
      return CM.RangeSet.of(ranges, true);
    },
    initialSpacer: () => RUN_MARKER,
    domEventHandlers: {
      mousedown(view, line, e) {
        if (!e.target.closest('.cm-run-btn')) return false;
        const text = view.state.doc.toString();
        const stmts = splitSqlStatements(text).filter((s) => s.from >= line.from && s.from <= line.to);
        if (!stmts.length) return false;
        const range = { from: stmts[0].from, to: stmts[stmts.length - 1].to };
        runSql(tab, range);
        return true;
      },
    },
  });
}

const AC_CM_TYPE = { column: 'property', table: 'class', view: 'interface', schema: 'namespace', alias: 'variable', keyword: 'keyword', function: 'function' };

function sqlCompletionSource(ctx) {
  const res = acSuggest(ctx.state.doc.toString(), ctx.pos, ctx.explicit);
  if (!res) return null;
  return {
    from: res.from,
    to: res.to,
    filter: false,
    options: res.items.map((it, i) => ({
      label: it.label,
      detail: it.detail ? `${it.pk ? 'PK · ' : ''}${it.detail}` : undefined,
      type: AC_CM_TYPE[it.kind],
      boost: -i,
      apply: it.fn
        ? (view, _c, from, to) => {
          const hasParen = view.state.sliceDoc(to, to + 1) === '(';
          view.dispatch({ changes: { from, to, insert: it.insert + (hasParen ? '' : '()') }, selection: { anchor: from + it.insert.length + 1 } });
        }
        : it.insert,
    })),
  };
}

function cmSqlDialect() {
  return { postgres: CM.PostgreSQL, mysql: CM.MySQL, sqlite: CM.SQLite, bigquery: CM.MySQL }[state.schema?.dialect] || CM.StandardSQL;
}

function prettifySql(view) {
  const dialect = { postgres: CM.postgresql, mysql: CM.mysql, sqlite: CM.sqlite, bigquery: CM.bigquery }[state.schema?.dialect] || CM.standardSql;
  const sel = view.state.selection.main;
  const from = sel.empty ? 0 : sel.from;
  const to = sel.empty ? view.state.doc.length : sel.to;
  const src = view.state.sliceDoc(from, to);
  try {
    const out = CM.formatDialect(src, { dialect, keywordCase: 'upper', tabWidth: 2, linesBetweenQueries: 1 });
    if (out !== src) view.dispatch({ changes: { from, to, insert: out } });
  } catch (e) {
    toast(`Prettify failed: ${e.message.split('\n')[0]}`, 'error');
  }
  return true;
}

const SQL_SHORTCUTS = [
  ['Run statement under cursor / selection', 'Ctrl+Enter'],
  ['Run all', 'Shift+Ctrl+Enter'],
  ['Prettify (selection or all)', 'Shift+Alt+F'],
  ['Suggestions / accept suggestion', 'Ctrl+Space / Tab, Enter'],
  ['Save query', 'Ctrl+S'],
  ['Toggle line comment', 'Ctrl+/'],
  ['Move line up / down', 'Alt+↑ / Alt+↓'],
  ['Copy line up / down', 'Shift+Alt+↑ / Shift+Alt+↓'],
  ['Delete line', 'Ctrl+Shift+K'],
  ['Select line', 'Ctrl+L'],
  ['Select next occurrence', 'Ctrl+D'],
  ['Select all occurrences', 'Ctrl+Shift+L'],
  ['Add cursor above / below', 'Ctrl+Alt+↑ / Ctrl+Alt+↓'],
  ['Add cursor', 'Alt+Click'],
  ['Column (box) selection', 'Shift+Alt+Drag'],
  ['Indent / outdent', 'Tab / Shift+Tab  ·  Ctrl+] / Ctrl+['],
  ['Find / replace', 'Ctrl+F'],
  ['Go to line', 'Ctrl+Alt+G'],
  ['Jump to matching bracket', 'Ctrl+Shift+\\'],
  ['Fold / unfold block', 'Ctrl+Shift+[ / Ctrl+Shift+]'],
  ['Undo / redo', 'Ctrl+Z / Ctrl+Shift+Z, Ctrl+Y'],
];

function openShortcutsModal() {
  openModal('Keyboard shortcuts', h('table', { class: 'shortcuts' },
    h('tbody', {}, SQL_SHORTCUTS.map(([what, keys]) => h('tr', {}, h('td', {}, what), h('td', {}, keys.split(/(\s\/\s|\s·\s|,\s)/).map((part) => (/^(\s\/\s|\s·\s|,\s)$/.test(part) ? h('span', { class: 'muted' }, part) : part.split('+').map((k, i) => [i ? '+' : '', h('kbd', {}, k)])))))))));
}

function sqlEditorFor(tab) {
  if (tab.cm) return tab.cm;
  const runKeys = CM.Prec.highest(CM.keymap.of([
    { key: 'Mod-Enter', run: () => { runSql(tab, 'current'); return true; } },
    { key: 'Shift-Mod-Enter', run: () => { runSql(tab, 'all'); return true; } },
    { key: 'Shift-Alt-f', run: prettifySql, preventDefault: true },
    { key: 'Mod-l', run: CM.selectLine, preventDefault: true },
    { key: 'Mod-s', run: () => { saveSqlTab(tab); return true; }, preventDefault: true },
    // Tab accepts the selected suggestion (the first one by default), as in VS Code.
    // Without an open list, Tab falls through to indentWithTab.
    { key: 'Tab', run: CM.acceptCompletion },
  ]));
  tab.cm = new CM.EditorView({
    doc: tab.sql,
    extensions: [
      CM.lineNumbers(),
      runGutter(tab),
      CM.foldGutter({ openText: '▾', closedText: '▸' }),
      CM.highlightActiveLineGutter(),
      CM.highlightSpecialChars(),
      CM.history(),
      CM.drawSelection(),
      CM.dropCursor(),
      CM.EditorState.allowMultipleSelections.of(true),
      CM.EditorView.clickAddsSelectionRange.of((e) => e.altKey),
      CM.rectangularSelection({ eventFilter: (e) => e.shiftKey && e.altKey }),
      CM.indentOnInput(),
      CM.indentUnit.of('  '),
      CM.bracketMatching(),
      CM.closeBrackets(),
      CM.autocompletion({ override: [sqlCompletionSource], icons: true, maxRenderedOptions: 80, interactionDelay: 0 }),
      CM.highlightActiveLine(),
      CM.highlightSelectionMatches(),
      runKeys,
      CM.keymap.of([...CM.closeBracketsKeymap, ...CM.defaultKeymap, ...CM.searchKeymap, ...CM.historyKeymap, ...CM.foldKeymap, ...CM.completionKeymap, CM.indentWithTab]),
      CM.sql({ dialect: cmSqlDialect() }),
      sqlHighlighter,
      flashField,
      CM.placeholder('SELECT * FROM …   — Ctrl+Enter runs the statement under the cursor, ▶ in the gutter runs one statement'),
      CM.EditorView.updateListener.of((u) => {
        if (!u.docChanged) return;
        tab.sql = u.state.doc.toString();
        saveTabsDebounced();
      }),
    ],
  });
  return tab.cm;
}

function renderSqlView(tab, root) {
  const view = sqlEditorFor(tab);
  const history = store.get(`rs.history.${state.connId}`, []);
  const runBtn = h('button', { class: 'btn primary split-main', disabled: tab.running, title: 'Run the statement under the cursor (or the selection)', onclick: () => runSql(tab, 'current') },
    tab.running ? h('span', { class: 'spinner' }) : icon('play', 13), 'Run', h('span', { class: 'kbd' }, 'Ctrl+Enter'));
  const runMore = h('button', {
    class: 'btn primary split-more', disabled: tab.running, title: 'More run options',
    onclick: (e) => openMenu(e.currentTarget, [
      h('div', { class: 'menu-item', onclick: () => { closeMenu(); runSql(tab, 'all'); } }, icon('play', 13), h('span', { class: 'menu-text' }, 'Run all'), h('span', { class: 'kbd' }, 'Shift+Ctrl+Enter')),
      h('div', { class: 'menu-item', onclick: () => { closeMenu(); runSql(tab, 'explain'); } }, icon('structure', 13), h('span', { class: 'menu-text' }, 'Explain'), h('span', { class: 'kbd' }, 'statement')),
    ]),
  }, icon('chevron', 13));
  root.append(h('div', { class: 'toolbar' },
    h('div', { class: 'tb-group' },
      h('div', { class: 'tb-title' }, icon('terminal'), h('span', {
        contenteditable: 'true', class: 'tab-title-edit', spellcheck: 'false',
        onblur: (e) => { tab.title = e.target.textContent.trim() || tab.title; saveTabs(); const bar = $('.tabbar'); if (bar) bar.replaceWith(renderTabBar()); },
        onkeydown: (e) => { if (e.key === 'Enter') { e.preventDefault(); e.target.blur(); } },
      }, tab.title)),
      h('button', { class: 'btn', title: 'Format SQL (selection or all)', onclick: () => { prettifySql(view); view.focus(); } }, icon('wand', 14), 'Prettify', h('span', { class: 'kbd' }, 'Shift+Alt+F')),
      h('div', { class: 'split-btn' }, runBtn, runMore),
    ),
    h('div', { class: 'tb-group' },
      saveQueryButtons(tab),
      h('button', { class: 'btn ghost', disabled: !history.length, onclick: (e) => openHistoryMenu(e.currentTarget, tab) }, icon('history'), 'History'),
      h('button', { class: 'icon-btn', title: 'Keyboard shortcuts', onclick: openShortcutsModal }, icon('keyboard', 16)),
    ),
  ));
  const editor = h('div', { class: 'sql-editor' }, view.dom);
  root.append(h('div', { class: 'sql-split' }, editor, renderSqlResults(tab)));
  view.requestMeasure();
}

let saveTimer = null;
function saveTabsDebounced() { clearTimeout(saveTimer); saveTimer = setTimeout(saveTabs, 400); }

/** "Save" (Ctrl+S) for a SQL tab; a linked tab also gets "Save as new". */
function saveQueryButtons(tab) {
  const linked = findSaved(tab.savedId);
  const main = h('button', { class: 'btn ghost', title: linked ? `Update the saved query "${linked.name}"` : 'Save the query for later', onclick: () => saveSqlTab(tab) },
    icon('bookmark', 14), 'Save', h('span', { class: 'kbd' }, 'Ctrl+S'));
  if (!linked) return main;
  return h('div', { class: 'split-btn' }, main,
    h('button', {
      class: 'btn ghost split-more-plain', title: 'More',
      onclick: (e) => openMenu(e.currentTarget, [
        h('div', { class: 'menu-item', onclick: () => { closeMenu(); saveSqlTab(tab, { asNew: true }); } }, icon('plus', 13), h('span', { class: 'menu-text' }, 'Save as new query…')),
        h('div', { class: 'menu-item', onclick: () => { closeMenu(); renameSavedQuery(linked); } }, icon('edit', 13), h('span', { class: 'menu-text' }, 'Rename or change scope…')),
      ]),
    }, icon('chevron', 13)));
}

function openHistoryMenu(anchor, tab) {
  const history = store.get(`rs.history.${state.connId}`, []);
  openMenu(anchor, history.map((q) => h('div', {
    class: 'menu-item mono hist', title: q,
    onclick: () => {
      closeMenu();
      const view = tab.cm;
      if (view) {
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: q }, selection: { anchor: q.length } });
        view.focus();
      } else { tab.sql = q; saveTabs(); refreshView(tab); }
    },
  }, q.length > 120 ? q.slice(0, 120) + '…' : q)), 'scroll-menu wide-menu');
}

/**
 * mode: 'current' (selection or statement under the cursor), 'all', 'explain', or a {from, to} range.
 */
async function runSql(tab, mode = 'current') {
  if (tab.running) return;
  const view = tab.cm;
  const text = view ? view.state.doc.toString() : tab.sql;
  const sel = view?.state.selection.main;
  let range = null;
  if (typeof mode === 'object') range = mode;
  else if (mode === 'all') range = { from: 0, to: text.length };
  else if (sel && !sel.empty) range = { from: sel.from, to: sel.to };
  else {
    const st = statementAt(text, sel ? sel.head : 0);
    if (st) range = { from: st.from, to: st.to };
  }
  let sql = range ? text.slice(range.from, range.to).trim() : '';
  if (!sql) return;
  if (mode === 'explain' && !/^\s*explain\b/i.test(sql)) {
    sql = `${state.schema?.dialect === 'sqlite' ? 'EXPLAIN QUERY PLAN' : 'EXPLAIN'} ${sql}`;
  }
  if (view && range) flashRange(view, range);
  tab.running = true;
  tab.error = null;
  refreshView(tab);
  try {
    const res = await api(`/connections/${state.connId}/sql`, { body: { sql } });
    tab.results = res.results;
    tab.duration = res.durationMs;
    tab.activeResult = Math.max(0, res.results.findLastIndex((r) => r.columns.length > 0));
    const history = store.get(`rs.history.${state.connId}`, []).filter((q) => q !== sql);
    history.unshift(sql);
    store.set(`rs.history.${state.connId}`, history.slice(0, 50));
    if (/\b(create|drop|alter|rename)\b/i.test(sql)) refreshSchemaSilently();
  } catch (e) {
    tab.error = e.message;
    tab.results = null;
  }
  tab.running = false;
  refreshView(tab);
  setTimeout(() => tab.cm?.focus(), 0);
}

async function refreshSchemaSilently() {
  try { state.schema = await api(`/connections/${state.connId}/schema?refresh=1`); renderSidebar(); } catch { /* ignore */ }
}

function renderSqlResults(tab) {
  const box = h('div', { class: 'sql-results' });
  if (tab.error) { box.append(h('div', { class: 'grid-error' }, icon('x'), h('pre', {}, tab.error))); return box; }
  if (!tab.results) { box.append(h('div', { class: 'grid-empty' }, tab.running ? 'Running…' : 'Run a query to see results')); return box; }
  const results = tab.results;
  if (!results.length) { box.append(h('div', { class: 'grid-empty' }, 'Query executed')); return box; }
  const res = results[Math.min(tab.activeResult, results.length - 1)];
  // Hidden columns belong to this tab and this exact set of result columns (not to table settings).
  // The same query run again keeps them; a result with other columns shows all columns.
  // Copy and export use the visible columns.
  const sig = res.columns.map((c) => c.name).join('\u0000');
  const hidden = tab.resHidden?.sig === sig ? tab.resHidden.names : [];
  const vis = res.columns.map((c, i) => i).filter((i) => !hidden.includes(res.columns[i].name));
  const names = vis.map((i) => res.columns[i].name);
  const visRows = () => (vis.length === res.columns.length ? res.rows : res.rows.map((r) => vis.map((i) => r[i])));
  const asJson = () => JSON.stringify(rowsToObjects(names, visRows()), null, 2);
  const nHidden = res.columns.length - vis.length;
  const mode = tab.resultView || 'table';
  const setMode = (m) => { tab.resultView = m; refreshView(tab); };
  const head = h('div', { class: 'res-head' },
    res.columns.length ? h('div', { class: 'seg' },
      h('button', { class: mode === 'table' ? 'on' : '', title: 'Table', onclick: () => setMode('table') }, icon('table', 13)),
      h('button', { class: mode === 'json' ? 'on' : '', title: 'JSON', onclick: () => setMode('json') }, '{ }'),
    ) : null,
    results.length > 1 ? h('div', { class: 'seg' }, results.map((r, i) => h('button', {
      class: i === tab.activeResult ? 'on' : '', onclick: () => { tab.activeResult = i; refreshView(tab); },
    }, `Result ${i + 1}`))) : null,
    h('span', { class: 'muted small' },
      res.columns.length ? `${fmtNum(res.rows.length)} row${res.rows.length === 1 ? '' : 's'}${res.truncated ? ' (truncated)' : ''}` : `${fmtNum(res.rowsAffected)} row${res.rowsAffected === 1 ? '' : 's'} affected`,
      tab.duration != null ? ` · ${tab.duration.toFixed(1)} ms` : '',
      res.info ? ` · ${res.info}` : ''),
    h('span', { class: 'spacer' }),
    res.columns.length > 1 ? h('button', {
      class: 'btn ghost sm', title: 'Show or hide columns',
      onclick: (e) => columnsMenu(e.currentTarget, res.columns.map((c) => ({ name: c.name, type: c.type.toLowerCase() })), () => (tab.resHidden?.sig === sig ? tab.resHidden.names : []),
        (list) => { tab.resHidden = { sig, names: list }; saveTabs(); refreshView(tab); }),
    }, icon('columns', 13), 'Columns', nHidden ? h('span', { class: 'count' }, `${vis.length}/${res.columns.length}`) : null) : null,
    res.columns.length ? h('button', { class: 'btn ghost sm', title: nHidden ? 'Copy all rows as JSON (visible columns)' : 'Copy all rows as JSON', onclick: () => { copyText(asJson()); toast('Copied'); } }, icon('copy', 13), 'Copy') : null,
    res.columns.length ? h('button', { class: 'btn ghost sm', onclick: () => download('result.csv', toCSV(names, visRows()), 'text/csv') }, icon('download', 13), 'CSV') : null,
    res.columns.length ? h('button', { class: 'btn ghost sm', onclick: () => download('result.json', asJson(), 'application/json') }, icon('download', 13), 'JSON') : null,
  );
  box.append(head);
  if (res.columns.length) box.append(mode === 'json' ? h('pre', { class: 'res-json mono', html: highlightJson(asJson()) }) : staticGrid(res, vis, resWidths(tab, sig)));
  return box;
}

function highlightJson(src) {
  return escapeHtml(src).replace(/("(?:[^"\\]|\\.)*")(\s*:)?|\b(true|false)\b|\bnull\b|-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b/g, (m, str, colon, bool) => {
    if (str) return colon ? `<span class="t-id">${str}</span>${colon}` : `<span class="t-str">${str}</span>`;
    if (bool) return `<span class="t-const">${m}</span>`;
    if (m === 'null') return `<span class="t-com">${m}</span>`;
    return `<span class="t-num">${m}</span>`;
  });
}

/** A read-only grid of a SQL result. `vis` lists the indexes of the visible columns. */
// Column widths of a SQL result belong to the tab and to this exact set of result columns (like hidden columns).
function resWidths(tab, sig) {
  if (tab.resWidths?.sig !== sig) tab.resWidths = { sig, w: {} };
  return tab.resWidths.w;
}

// widths: optional { columnName: px }; when given, the columns can be resized with the mouse.
function staticGrid(res, vis = res.columns.map((c, i) => i), widths = null) {
  const cols = res.columns;
  const shown = vis.map((i) => cols[i]);
  const table = h('table', { class: 'grid' },
    h('colgroup', {}, h('col', { style: { width: '56px' } }), shown.map((c) => h('col', { style: { width: `${widths?.[c.name] || defaultWidth({ kind: c.kind, name: c.name })}px` } }))),
    h('thead', {}, h('tr', {}, h('th', { class: 'rownum' }, '#'), shown.map((c) => h('th', { title: `${c.name} ${c.type}` },
      h('div', { class: 'th-inner' }, h('span', { class: 'th-name' }, c.name), h('span', { class: 'th-type' }, c.type.toLowerCase())),
      widths ? h('div', { class: 'resizer', onmousedown: (e) => startResize(e, widths, c.name) }) : null)))),
    h('tbody', {}, res.rows.map((row, i) => h('tr', {}, h('td', { class: 'rownum' }, h('span', { class: 'rn' }, String(i + 1))),
      vis.map((ci) => { const d = displayValue(row[ci], cols[ci]); return h('td', { class: `cell ro ${d.cls}`, dataset: { ci: String(ci) } }, h('span', { class: 'cell-text' }, d.text)); })))),
  );
  const wrap = h('div', { class: 'grid-wrap', tabindex: '0' }, table);
  wrap.addEventListener('mousedown', (e) => {
    const td = e.target.closest('td.cell');
    wrap.querySelectorAll('td.sel').forEach((x) => x.classList.remove('sel'));
    if (td) td.classList.add('sel');
  });
  wrap.addEventListener('dblclick', (e) => {
    const td = e.target.closest('td.cell');
    if (!td) return;
    const ri = [...td.parentElement.parentElement.children].indexOf(td.parentElement);
    const ci = Number(td.dataset.ci);
    openViewer({ name: cols[ci].name, kind: cols[ci].kind, type: cols[ci].type }, res.rows[ri][ci], td);
  });
  wrap.addEventListener('keydown', (e) => {
    const td = wrap.querySelector('td.sel');
    if (td && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
      const ri = [...td.parentElement.parentElement.children].indexOf(td.parentElement);
      copyText(stringValue(res.rows[ri][Number(td.dataset.ci)]));
    }
  });
  return wrap;
}

// ---------------------------------------------------------------- Schema view (schema management)
const SCHEMA_SECTIONS = [
  { id: 'schemas', label: 'Schemas', icon: 'folder', feature: 'schemas' },
  { id: 'tables', label: 'Tables', icon: 'table' },
  { id: 'views', label: 'Views', icon: 'view' },
  { id: 'enums', label: 'Enums', icon: 'list', feature: 'enums' },
  { id: 'roles', label: 'Roles', icon: 'user', feature: 'roles' },
  { id: 'policies', label: 'Policies', icon: 'shield', feature: 'policies' },
];

const COLUMN_TYPES = {
  postgres: ['integer', 'bigint', 'smallint', 'serial', 'bigserial', 'numeric(12,2)', 'real', 'double precision', 'boolean', 'text', 'varchar(255)', 'char(1)', 'uuid', 'date', 'time', 'timestamp', 'timestamptz', 'interval', 'json', 'jsonb', 'bytea', 'inet', 'text[]', 'integer[]'],
  mysql: ['int', 'bigint', 'smallint', 'tinyint', 'tinyint(1)', 'decimal(12,2)', 'float', 'double', 'varchar(255)', 'char(36)', 'text', 'mediumtext', 'longtext', 'json', 'date', 'datetime', 'timestamp', 'time', 'year', 'blob', "enum('a','b')"],
  sqlite: ['INTEGER', 'REAL', 'TEXT', 'BLOB', 'NUMERIC', 'BOOLEAN', 'DATETIME', 'JSON'],
};
const FK_ACTIONS = ['NO ACTION', 'RESTRICT', 'CASCADE', 'SET NULL', 'SET DEFAULT'];
const PG_PRIVILEGES = ['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'TRUNCATE', 'REFERENCES', 'TRIGGER'];

function newSchemaTab(opts = {}) {
  return { id: tabSeq++, type: 'schema', section: opts.section || 'tables', sel: opts.sel || null, filter: '', schemaFilter: opts.schemaFilter || null, catalog: null, detail: null, loading: false, error: null };
}

function openSchemaTab() {
  let tab = state.tabs.find((t) => t.type === 'schema');
  if (!tab) { tab = newSchemaTab(); state.tabs.push(tab); }
  activate(tab.id);
}

const sqlDialect = () => state.schema?.dialect || 'postgres';
const qi = (name) => (sqlDialect() === 'bigquery' ? `\`${String(name).replace(/[\\`]/g, '\\$&')}\``
  : sqlDialect() === 'mysql' ? `\`${String(name).replace(/`/g, '``')}\`` : `"${String(name).replace(/"/g, '""')}"`);
const qref = (schema, name) => (sqlDialect() === 'sqlite' || !schema ? qi(name) : `${qi(schema)}.${qi(name)}`);
const lit = (v) => `'${String(v).replace(/'/g, "''")}'`;
const colList = (cols) => cols.map(qi).join(', ');
const splitCols = (s) => s.split(',').map((x) => x.trim()).filter(Boolean);

/** Highlighted SQL as HTML (same tokenizer and colors as the console). */
function sqlHtml(src) {
  let out = '';
  let last = 0;
  for (const t of sqlTokens(src)) {
    out += escapeHtml(src.slice(last, t.from)) + `<span class="${t.cls}">${escapeHtml(src.slice(t.from, t.to))}</span>`;
    last = t.to;
  }
  return out + escapeHtml(src.slice(last));
}
const sqlBlock = (src, cls = '') => h('pre', { class: `sql-line ${cls}`, html: sqlHtml(src) });

async function loadCatalog(tab, { keepDetail = false } = {}) {
  tab.loading = true;
  tab.error = null;
  try {
    tab.catalog = await api(`/connections/${state.connId}/ddl/catalog`);
    if (!tab.schemaFilter || !tab.catalog.schemas.includes(tab.schemaFilter)) {
      tab.schemaFilter = tab.catalog.schemas.includes(state.schemaName) ? state.schemaName : tab.catalog.defaultSchema;
    }
    const sec = SCHEMA_SECTIONS.find((s) => s.id === tab.section);
    if (sec?.feature && !tab.catalog.features[sec.feature]) tab.section = 'tables';
  } catch (e) {
    tab.error = e.message;
  }
  tab.loading = false;
  if (!keepDetail) tab.detail = null;
  refreshView(tab);
  if (tab.sel) loadDetail(tab);
}

async function loadDetail(tab) {
  const sel = tab.sel;
  if (!sel) { tab.detail = null; refreshView(tab); return; }
  const enc = encodeURIComponent;
  const url = {
    tables: `/ddl/table?schema=${enc(sel.schema || '')}&name=${enc(sel.name)}`,
    views: `/ddl/view?schema=${enc(sel.schema || '')}&name=${enc(sel.name)}`,
    roles: `/ddl/role?name=${enc(sel.name)}`,
  }[tab.section];
  if (!url) { tab.detail = { ok: true }; refreshView(tab); return; }
  try {
    const detail = await api(`/connections/${state.connId}${url}`);
    if (tab.sel === sel) { tab.detail = detail; tab.detailError = null; }
  } catch (e) {
    if (tab.sel === sel) { tab.detail = null; tab.detailError = e.message; }
  }
  refreshView(tab);
}

function selectItem(tab, sel) {
  tab.sel = sel;
  tab.detail = null;
  tab.detailError = null;
  saveTabs();
  refreshView(tab);
  loadDetail(tab);
}

function schemaReadOnly(tab) { return !!tab.catalog?.readOnlyReason; }

// ---- apply flow
/**
 * Shows the SQL and runs it after confirmation. statements: string[] or {sql, display}[].
 * Returns true when applied.
 */
function applyDdl(tab, title, statements, { danger = false, after = null, note = null } = {}) {
  const items = statements.filter(Boolean).map((s) => (typeof s === 'string' ? { sql: s, display: s } : s));
  if (!items.length) { toast('Nothing changed'); return Promise.resolve(false); }
  return new Promise((resolve) => {
    const status = h('div', { class: 'form-status' });
    const body = h('div', { class: 'form' },
      note ? h('div', { class: `ddl-note${danger ? ' danger' : ''}` }, note) : null,
      h('div', { class: 'ddl-preview' }, items.map((i) => sqlBlock(i.display))),
      sqlDialect() === 'mysql' && items.length > 1 ? h('div', { class: 'hint' }, h('div', {}, 'MySQL commits each DDL statement separately: a failure stops at that statement.')) : null,
      status);
    const apply = h('button', {
      class: `btn ${danger ? 'danger' : 'primary'}`, onclick: async () => {
        apply.disabled = true;
        status.className = 'form-status';
        status.textContent = 'Applying…';
        try {
          await api(`/connections/${state.connId}/ddl`, { body: { statements: items.map((i) => i.sql) } });
          applied = true;
          close();
          toast('Schema updated', 'success');
          if (after) after();
          await refreshSchemaSilently();
          state.tabs = state.tabs.filter((t) => t.type !== 'table' || findTable(t.schema, t.table));
          renderMainBar();
          await loadCatalog(tab, { keepDetail: true });
          resolve(true);
        } catch (e) {
          status.className = 'form-status error';
          status.textContent = e.message;
          apply.disabled = false;
        }
      },
    }, danger ? 'Apply (destructive)' : 'Apply');
    let applied = false;
    const close = openModal(title, body, [h('button', { class: 'btn', onclick: () => close() }, 'Cancel'), apply], { wide: true, onClose: () => { if (!applied) resolve(false); } });
  });
}

/** Re-render only the tab bar (tabs may have been removed). */
function renderMainBar() {
  const bar = $('.tabbar');
  if (bar) bar.replaceWith(renderTabBar());
}

/**
 * A form modal with live SQL preview. fields: DOM; build(): statements or throws Error(message).
 */
function ddlForm(tab, title, fields, build, opts = {}) {
  const preview = h('div', { class: 'ddl-preview' });
  const status = h('div', { class: 'form-status' });
  const update = () => {
    try {
      const st = build().filter(Boolean).map((s) => (typeof s === 'string' ? { sql: s, display: s } : s));
      preview.replaceChildren(...(st.length ? st.map((s) => sqlBlock(s.display)) : [h('div', { class: 'muted small' }, 'Fill in the fields to see the SQL')]));
      status.textContent = '';
      return st;
    } catch (e) {
      preview.replaceChildren(h('div', { class: 'muted small' }, e.message));
      return null;
    }
  };
  const body = h('div', { class: 'form ddl-form' }, fields, h('label', {}, 'SQL'), preview, status);
  body.addEventListener('input', update);
  body.addEventListener('change', update);
  const apply = h('button', {
    class: 'btn primary', onclick: async () => {
      const st = update();
      if (!st || !st.length) { status.className = 'form-status error'; status.textContent = 'Nothing to apply'; return; }
      apply.disabled = true;
      status.className = 'form-status';
      status.textContent = 'Applying…';
      try {
        await api(`/connections/${state.connId}/ddl`, { body: { statements: st.map((s) => s.sql) } });
        close();
        toast('Schema updated', 'success');
        if (opts.after) opts.after();
        await refreshSchemaSilently();
        state.tabs = state.tabs.filter((t) => t.type !== 'table' || findTable(t.schema, t.table));
        renderMainBar();
        await loadCatalog(tab, { keepDetail: true });
      } catch (e) {
        status.className = 'form-status error';
        status.textContent = e.message;
        apply.disabled = false;
      }
    },
  }, opts.applyLabel || 'Apply');
  const close = openModal(title, body, [h('button', { class: 'btn', onclick: () => close() }, 'Cancel'), apply], { wide: true });
  setTimeout(() => { update(); body.querySelector('input,textarea,select')?.focus(); }, 0);
  return close;
}

// small form controls
const fInput = (label, value = '', attrs = {}) => {
  const el = h('input', { class: `input${attrs.mono === false ? '' : ' mono'}`, value, ...attrs });
  return { el, wrap: h('label', {}, label, el) };
};
const fSelect = (label, options, value) => {
  const el = h('select', { class: 'input' }, options.map((o) => (Array.isArray(o) ? h('option', { value: o[0], selected: o[0] === value }, o[1]) : h('option', { value: o, selected: o === value }, o))));
  if (value != null) el.value = value;
  return { el, wrap: h('label', {}, label, el) };
};
const fCheck = (label, checked = false) => {
  const el = h('input', { type: 'checkbox', checked });
  return { el, wrap: h('label', { class: 'check' }, el, label) };
};
const fArea = (label, value = '', rows = 3) => {
  const el = h('textarea', { class: 'input mono area', rows, spellcheck: 'false', value });
  return { el, wrap: h('label', {}, label, el) };
};
function typeInput(value = '') {
  const id = `types-${Math.random().toString(36).slice(2)}`;
  const el = h('input', { class: 'input mono', value, list: id, placeholder: COLUMN_TYPES[sqlDialect()][0] });
  return { el, list: h('datalist', { id }, COLUMN_TYPES[sqlDialect()].map((t) => h('option', { value: t }))) };
}
const req = (v, what) => { if (!String(v).trim()) throw new Error(`Enter ${what}`); return String(v).trim(); };

// ---- view
function renderSchemaView(tab, root) {
  if (!tab.catalog && !tab.loading && !tab.error) { loadCatalog(tab); }
  const cat = tab.catalog;
  const sections = SCHEMA_SECTIONS.filter((s) => !s.feature || cat?.features[s.feature]);
  const bar = h('div', { class: 'toolbar schema-bar' },
    h('div', { class: 'tb-group' },
      h('div', { class: 'seg seg-lg' }, sections.map((s) => h('button', {
        class: tab.section === s.id ? 'on' : '',
        onclick: () => { tab.section = s.id; tab.sel = null; tab.detail = null; tab.filter = ''; saveTabs(); refreshView(tab); },
      }, icon(s.icon, 13), s.label))),
    ),
    h('div', { class: 'tb-group' },
      cat?.readOnlyReason ? h('span', { class: 'badge muted', title: cat.readOnlyReason }, icon('lock', 11), 'read-only') : null,
      h('div', { class: 'split-btn' },
        h('button', { class: 'btn', title: 'Copy the DDL of the whole database', onclick: () => copySchema(false) }, icon('copy', 14), 'Copy schema'),
        h('button', { class: 'btn split-more-plain', title: 'Open the DDL in a SQL console', onclick: () => copySchema(true) }, icon('external', 13))),
    ));
  root.append(bar);
  const body = h('div', { class: 'schema-body' });
  root.append(body);
  if (tab.error) { body.append(h('div', { class: 'grid-error' }, icon('x'), h('pre', {}, tab.error))); return; }
  if (!cat) { body.append(h('div', { class: 'grid-empty' }, 'Loading…')); return; }
  body.append(renderSchemaList(tab), h('div', { class: 'schema-detail' }, renderSchemaDetail(tab)));
}

async function copySchema(openInConsole) {
  try {
    const { sql } = await api(`/connections/${state.connId}/ddl/dump`);
    if (openInConsole) {
      const t = newSqlTab('Schema DDL', sql);
      state.tabs.push(t);
      activate(t.id);
    } else {
      copyText(sql);
      toast('Schema DDL copied', 'success');
    }
  } catch (e) { toast(e.message, 'error'); }
}

function listItems(tab) {
  const cat = tab.catalog;
  const inSchema = (x) => !cat.features.schemas || x.schema === tab.schemaFilter;
  switch (tab.section) {
    case 'schemas': return cat.schemas.map((s) => ({ key: s, label: s, icon: 'folder', sel: { name: s } }));
    case 'tables': return cat.tables.filter(inSchema).map((t) => ({ key: `${t.schema}.${t.name}`, label: t.name, icon: 'table', badge: t.rls ? 'RLS' : null, sel: { schema: t.schema, name: t.name } }));
    case 'views': return cat.views.filter(inSchema).map((v) => ({ key: `${v.schema}.${v.name}`, label: v.name, icon: 'view', badge: v.materialized ? 'MAT' : null, sel: { schema: v.schema, name: v.name } }));
    case 'enums': return cat.enums.filter(inSchema).map((e) => ({ key: `${e.schema}.${e.name}`, label: e.name, icon: 'list', badge: String(e.values.length), sel: { schema: e.schema, name: e.name } }));
    case 'roles': return cat.roles.map((r) => ({ key: r.name, label: r.name, icon: 'user', system: r.system, sel: { name: r.name } }));
    case 'policies': return cat.policies.filter(inSchema).map((p) => ({ key: `${p.schema}.${p.table}.${p.name}`, label: p.name, sub: p.table, icon: 'shield', sel: { schema: p.schema, table: p.table, name: p.name } }));
    default: return [];
  }
}

function renderSchemaList(tab) {
  const cat = tab.catalog;
  const ro = schemaReadOnly(tab);
  const q = tab.filter.toLowerCase();
  const items = listItems(tab).filter((i) => !q || i.label.toLowerCase().includes(q) || (i.sub || '').toLowerCase().includes(q));
  const list = h('div', { class: 'schema-items' });
  let systemShown = false;
  for (const it of items) {
    if (it.system && !systemShown) { list.append(h('div', { class: 'menu-sep' })); systemShown = true; }
    const active = tab.sel && it.key === selKey(tab);
    list.append(h('button', { class: `sb-item${active ? ' active' : ''}`, onclick: () => selectItem(tab, it.sel) },
      icon(it.icon, 14), h('span', { class: 'sb-name' }, it.label), it.sub ? h('span', { class: 'sb-sub' }, it.sub) : null,
      it.badge ? h('span', { class: 'sb-count' }, it.badge) : null));
  }
  if (!items.length) list.append(h('div', { class: 'sb-empty' }, q ? 'Nothing found' : `No ${tab.section} in ${cat.features.schemas && tab.section !== 'schemas' && tab.section !== 'roles' ? `schema ${tab.schemaFilter}` : 'database'}`));
  const search = h('input', {
    class: 'sb-search', placeholder: 'Search…', value: tab.filter,
    oninput: (e) => { tab.filter = e.target.value; const l = $('.schema-items'); if (l) l.replaceWith(renderSchemaList(tab).querySelector('.schema-items')); },
  });
  const schemaPick = cat.features.schemas && !['schemas', 'roles'].includes(tab.section)
    ? h('select', { class: 'input sm', onchange: (e) => { tab.schemaFilter = e.target.value; tab.sel = null; tab.detail = null; saveTabs(); refreshView(tab); } },
      cat.schemas.map((s) => h('option', { value: s, selected: s === tab.schemaFilter }, s)))
    : null;
  return h('div', { class: 'schema-list' },
    schemaPick ? h('div', { class: 'sb-row' }, h('label', { class: 'sb-label' }, schemaWord(cat.dialect)), schemaPick) : null,
    h('div', { class: 'schema-list-head' },
      h('div', { class: 'sb-search-wrap' }, icon('search', 14), search),
      h('button', { class: 'icon-btn', title: 'Refresh', onclick: () => loadCatalog(tab, { keepDetail: true }) }, icon('refresh', 14)),
      ro ? null : h('button', { class: 'icon-btn', title: 'Create', onclick: () => createObject(tab) }, icon('plus', 14))),
    list);
}

function selKey(tab) {
  const s = tab.sel;
  if (!s) return null;
  if (tab.section === 'policies') return `${s.schema}.${s.table}.${s.name}`;
  if (s.schema != null && ['tables', 'views', 'enums'].includes(tab.section)) return `${s.schema}.${s.name}`;
  return s.name;
}

function renderSchemaDetail(tab) {
  if (!tab.sel) return h('div', { class: 'grid-empty' }, 'Select an item on the left');
  if (tab.detailError) return h('div', { class: 'grid-error' }, icon('x'), h('pre', {}, tab.detailError));
  switch (tab.section) {
    case 'schemas': return schemaCard(tab);
    case 'tables': return tab.detail ? tableCard(tab, tab.detail) : h('div', { class: 'grid-empty' }, 'Loading…');
    case 'views': return tab.detail ? viewCard(tab, tab.detail) : h('div', { class: 'grid-empty' }, 'Loading…');
    case 'enums': return enumCard(tab);
    case 'roles': return tab.detail ? roleCard(tab, tab.detail) : h('div', { class: 'grid-empty' }, 'Loading…');
    case 'policies': return policyCard(tab);
    default: return null;
  }
}

/** An editable name field: Enter or blur proposes a rename. */
function nameField(label, value, ro, onRename) {
  const el = h('input', { class: 'input mono', value, readonly: ro });
  // Enter and the following change/blur both commit: propose each new name only once.
  let proposed = value;
  const go = () => {
    const v = el.value.trim();
    if (!v || v === value || v === proposed) return;
    proposed = v;
    onRename(v, () => { el.value = value; proposed = value; });
  };
  el.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); go(); } if (e.key === 'Escape') el.value = value; });
  el.addEventListener('change', go);
  return h('label', { class: 'card-field' }, h('span', {}, label), el);
}

function section(title, addLabel, onAdd, children, empty = 'None') {
  return h('div', { class: 'card-section' },
    h('div', { class: 'card-section-head' }, h('span', {}, title), onAdd ? h('button', { class: 'link-btn', onclick: onAdd }, addLabel) : null),
    children.length ? h('div', { class: 'def-list' }, children) : h('div', { class: 'def-empty' }, empty));
}

function defRow(sql, actions = [], cls = '') {
  return h('div', { class: `def-row ${cls}` }, sqlBlock(sql), actions.length ? h('div', { class: 'def-actions' }, actions) : null);
}
const actBtn = (ic, title, onclick, cls = '') => h('button', { class: `icon-btn ${cls}`, title, onclick }, icon(ic, 13));

// ---- schemas
function schemaCard(tab) {
  const cat = tab.catalog;
  const ro = schemaReadOnly(tab);
  const name = tab.sel.name;
  const d = sqlDialect();
  const counts = [['tables', cat.tables], ['views', cat.views], ['enums', cat.enums]].map(([k, arr]) => `${arr.filter((x) => x.schema === name).length} ${k}`);
  return h('div', { class: 'card' },
    h('div', { class: 'card-fields' },
      nameField(d === 'mysql' ? 'Database name' : 'Schema name', name, ro || !cat.features.renameSchema,
        (v, revert) => applyDdl(tab, 'Rename schema', [`ALTER SCHEMA ${qi(name)} RENAME TO ${qi(v)}`], { after: () => { tab.sel = { name: v }; if (tab.schemaFilter === name) tab.schemaFilter = v; } }).then((ok) => ok || revert()))),
    h('p', { class: 'muted small' }, counts.filter((c) => !c.endsWith(' enums') || cat.features.enums).join(' · ')),
    ro ? null : h('div', { class: 'card-actions' },
      h('button', { class: 'btn danger-ghost', onclick: () => {
        const cascade = fCheck('CASCADE (also drop all objects in it)');
        ddlForm(tab, d === 'mysql' ? 'Drop database' : 'Drop schema', h('div', { class: 'form' }, h('div', { class: 'ddl-note danger' }, `This permanently deletes ${d === 'mysql' ? 'the database' : 'the schema'} "${name}".`), d === 'mysql' ? null : cascade.wrap),
          () => [d === 'mysql' ? `DROP DATABASE ${qi(name)}` : `DROP SCHEMA ${qi(name)}${cascade.el.checked ? ' CASCADE' : ''}`],
          { applyLabel: 'Drop', after: () => { tab.sel = null; tab.detail = null; } });
      } }, icon('trash', 13), d === 'mysql' ? 'Drop database' : 'Drop schema')));
}

// ---- tables
function tableCard(tab, t) {
  const cat = tab.catalog;
  const f = cat.features;
  const ro = schemaReadOnly(tab);
  const d = sqlDialect();
  const ref = qref(t.schema, t.name);
  const reload = (sel) => () => { if (sel) tab.sel = sel; };

  const fields = h('div', { class: 'card-fields' },
    nameField('Table name', t.name, ro, (v, revert) => applyDdl(tab, 'Rename table',
      [d === 'mysql' ? `RENAME TABLE ${ref} TO ${qref(t.schema, v)}` : `ALTER TABLE ${ref} RENAME TO ${qi(v)}`],
      { after: reload({ schema: t.schema, name: v }) }).then((ok) => ok || revert())),
    f.schemas ? h('label', { class: 'card-field' }, h('span', {}, d === 'mysql' ? 'Database' : 'Schema'),
      h('select', {
        class: 'input', disabled: ro, onchange: (e) => {
          const to = e.target.value;
          applyDdl(tab, 'Move table', [d === 'mysql' ? `RENAME TABLE ${ref} TO ${qref(to, t.name)}` : `ALTER TABLE ${ref} SET SCHEMA ${qi(to)}`],
            { after: () => { tab.sel = { schema: to, name: t.name }; tab.schemaFilter = to; } }).then((ok) => { if (!ok) e.target.value = t.schema; });
        },
      }, cat.schemas.map((s) => h('option', { value: s, selected: s === t.schema }, s)))) : null,
    f.rls ? h('label', { class: 'card-field toggle-field' }, icon('lock', 13), h('span', {}, 'Row Level Security'),
      h('input', {
        type: 'checkbox', class: 'switch', checked: t.rls, disabled: ro,
        onchange: (e) => applyDdl(tab, `${e.target.checked ? 'Enable' : 'Disable'} RLS`, [`ALTER TABLE ${ref} ${e.target.checked ? 'ENABLE' : 'DISABLE'} ROW LEVEL SECURITY`])
          .then((ok) => { if (!ok) e.target.checked = !e.target.checked; }),
      })) : null,
  );

  const cols = t.columns.map((c) => defRow(c.definition, ro ? [] : [
    actBtn('edit', 'Edit column', () => columnForm(tab, t, c)),
    actBtn('trash', 'Drop column', () => applyDdl(tab, 'Drop column', [`ALTER TABLE ${ref} DROP COLUMN ${qi(c.name)}`], { danger: true, note: `Column "${c.name}" and its data will be deleted.` }), 'danger-icon'),
  ]));
  const cons = t.constraints.map((c) => defRow(c.definition, ro || !f.constraints ? [] : [
    actBtn('trash', 'Drop constraint', () => applyDdl(tab, 'Drop constraint', [dropConstraintSql(ref, c)], { danger: true }), 'danger-icon'),
  ]));
  const idx = t.indexes.filter((i) => !i.constraint).map((i) => defRow(i.definition, ro ? [] : [
    actBtn('trash', 'Drop index', () => applyDdl(tab, 'Drop index', [dropIndexSql(t, i.name)], { danger: true }), 'danger-icon'),
  ]));
  const pols = t.policies.map((p) => defRow(p.definition, ro ? [] : [
    actBtn('edit', 'Edit policy', () => policyForm(tab, p)),
    actBtn('trash', 'Drop policy', () => applyDdl(tab, 'Drop policy', [`DROP POLICY ${qi(p.name)} ON ${ref}`], { danger: true }), 'danger-icon'),
  ]));

  return h('div', { class: 'card' },
    fields,
    section('Columns', 'Add column', ro ? null : () => columnForm(tab, t, null), cols),
    section('Constraints', 'Add constraint', ro || !f.constraints ? null : () => constraintForm(tab, t), cons),
    section('Indexes', 'Add index', ro ? null : () => indexForm(tab, t), idx),
    f.policies ? section('Policies', 'Add policy', ro ? null : () => policyForm(tab, null, t), pols) : null,
    t.triggers.length ? section('Triggers', '', null, t.triggers.map((x) => defRow(x))) : null,
    h('details', { class: 'card-ddl' }, h('summary', {}, 'DDL'),
      h('div', { class: 'def-row' }, sqlBlock(t.ddl.trim()), h('div', { class: 'def-actions' }, actBtn('copy', 'Copy', () => { copyText(t.ddl); toast('Copied'); })))),
    ro ? null : h('div', { class: 'card-actions' },
      h('button', { class: 'btn ghost', onclick: () => openTable(t.schema, t.name) }, icon('table', 13), 'Open data'),
      h('span', { class: 'spacer' }),
      h('button', { class: 'btn danger-ghost', onclick: () => applyDdl(tab, 'Drop table', [`DROP TABLE ${ref}`], { danger: true, note: `Table "${t.name}" and all its rows will be deleted.`, after: () => { tab.sel = null; tab.detail = null; } }) }, icon('trash', 13), 'Drop table')),
  );
}

function dropConstraintSql(ref, c) {
  const d = sqlDialect();
  if (d !== 'mysql') return `ALTER TABLE ${ref} DROP CONSTRAINT ${qi(c.name)}`;
  if (c.kind === 'primary') return `ALTER TABLE ${ref} DROP PRIMARY KEY`;
  if (c.kind === 'foreign') return `ALTER TABLE ${ref} DROP FOREIGN KEY ${qi(c.name)}`;
  if (c.kind === 'check') return `ALTER TABLE ${ref} DROP CHECK ${qi(c.name)}`;
  return `ALTER TABLE ${ref} DROP INDEX ${qi(c.name)}`;
}

function dropIndexSql(t, name) {
  const d = sqlDialect();
  if (d === 'mysql') return `DROP INDEX ${qi(name)} ON ${qref(t.schema, t.name)}`;
  return `DROP INDEX ${qref(t.schema, name)}`;
}

function columnForm(tab, t, col) {
  const d = sqlDialect();
  const ref = qref(t.schema, t.name);
  const editing = !!col;
  const liteEdit = editing && d === 'sqlite';
  const name = fInput('Name', col?.name || '');
  const type = typeInput(col?.type || '');
  const notNull = fCheck('NOT NULL', col?.notNull || false);
  const def = fInput('Default (SQL expression)', col?.default ?? '', { placeholder: "e.g. 0, now(), 'text'" });
  const pk = fCheck('PRIMARY KEY');
  const unique = fCheck('UNIQUE');
  const auto = fCheck(d === 'postgres' ? 'Identity (GENERATED BY DEFAULT AS IDENTITY)' : 'AUTO_INCREMENT', col?.autoIncrement || false);
  for (const x of [type.el, notNull.el, def.el, pk.el, unique.el, auto.el]) x.disabled = liteEdit;
  const fields = h('div', { class: 'form' },
    h('div', { class: 'form-grid two' }, name.wrap, h('label', {}, 'Type', type.el, type.list)),
    def.wrap,
    h('div', { class: 'check-row' }, notNull.wrap, editing ? null : pk.wrap, editing ? null : unique.wrap, d === 'sqlite' || (editing && d === 'postgres') ? null : auto.wrap),
    liteEdit ? h('div', { class: 'hint' }, h('div', {}, 'SQLite can only rename a column. Changing its type or constraints needs a table rebuild.')) : null,
    d === 'sqlite' && !editing ? h('div', { class: 'hint' }, h('div', {}, 'SQLite: a NOT NULL column needs a default; PRIMARY KEY / UNIQUE cannot be added to an existing table.')) : null);
  const build = () => {
    const n = req(name.el.value, 'the column name');
    if (liteEdit) return n !== col.name ? [`ALTER TABLE ${ref} RENAME COLUMN ${qi(col.name)} TO ${qi(n)}`] : [];
    const ty = req(type.el.value, 'the type');
    const dv = def.el.value.trim();
    if (!editing) {
      let s = `ALTER TABLE ${ref} ADD COLUMN ${qi(n)} ${ty}`;
      if (auto.el.checked && d === 'postgres') s += ' GENERATED BY DEFAULT AS IDENTITY';
      if (notNull.el.checked) s += ' NOT NULL';
      if (dv) s += ` DEFAULT ${dv}`;
      if (auto.el.checked && d === 'mysql') s += ' AUTO_INCREMENT';
      if (pk.el.checked && d !== 'sqlite') s += ' PRIMARY KEY';
      else if (unique.el.checked && d !== 'sqlite') s += ' UNIQUE';
      return [s];
    }
    const out = [];
    if (n !== col.name) out.push(`ALTER TABLE ${ref} RENAME COLUMN ${qi(col.name)} TO ${qi(n)}`);
    if (d === 'postgres') {
      if (ty !== col.type) out.push(`ALTER TABLE ${ref} ALTER COLUMN ${qi(n)} TYPE ${ty} USING ${qi(n)}::${ty}`);
      if (notNull.el.checked !== col.notNull) out.push(`ALTER TABLE ${ref} ALTER COLUMN ${qi(n)} ${notNull.el.checked ? 'SET' : 'DROP'} NOT NULL`);
      if (dv !== (col.default ?? '')) out.push(dv ? `ALTER TABLE ${ref} ALTER COLUMN ${qi(n)} SET DEFAULT ${dv}` : `ALTER TABLE ${ref} ALTER COLUMN ${qi(n)} DROP DEFAULT`);
    } else {
      const changed = ty !== col.type || notNull.el.checked !== col.notNull || dv !== (col.default ?? '') || auto.el.checked !== col.autoIncrement;
      if (changed) {
        let s = `ALTER TABLE ${ref} MODIFY COLUMN ${qi(n)} ${ty}${notNull.el.checked ? ' NOT NULL' : ' NULL'}`;
        if (dv) s += ` DEFAULT ${/^[\d.-]+$|^(CURRENT_TIMESTAMP|NULL)/i.test(dv) || /^'.*'$/.test(dv) || /^\(.*\)$/.test(dv) ? dv : lit(dv)}`;
        if (auto.el.checked) s += ' AUTO_INCREMENT';
        out.push(s);
      }
    }
    return out;
  };
  ddlForm(tab, editing ? `Edit column ${col.name}` : `Add column to ${t.name}`, fields, build);
}

function constraintForm(tab, t) {
  const d = sqlDialect();
  const ref = qref(t.schema, t.name);
  const cat = tab.catalog;
  const kind = fSelect('Type', [['foreign', 'FOREIGN KEY'], ['unique', 'UNIQUE'], ['primary', 'PRIMARY KEY'], ['check', 'CHECK']], 'foreign');
  const name = fInput('Name (optional)', '', { placeholder: `${t.name}_..._fkey` });
  const cols = fInput('Columns (comma-separated)', '', { placeholder: t.columns.map((c) => c.name).slice(0, 2).join(', ') });
  const colChips = h('div', { class: 'chips' }, t.columns.map((c) => h('button', { type: 'button', class: 'chip', onclick: () => { cols.el.value = [...splitCols(cols.el.value), c.name].join(', '); cols.el.dispatchEvent(new Event('input', { bubbles: true })); } }, c.name)));
  const refTable = fSelect('References table', cat.tables.map((x) => [`${x.schema}.${x.name}`, d === 'sqlite' ? x.name : `${x.schema}.${x.name}`]), `${t.schema}.${t.name}`);
  const refCols = fInput('Referenced columns', 'id');
  const onDelete = fSelect('ON DELETE', FK_ACTIONS, 'NO ACTION');
  const onUpdate = fSelect('ON UPDATE', FK_ACTIONS, 'NO ACTION');
  const check = fInput('Check expression', '', { placeholder: 'price >= 0' });
  const fkBox = h('div', { class: 'form' }, h('div', { class: 'form-grid two' }, refTable.wrap, refCols.wrap), h('div', { class: 'form-grid two' }, onDelete.wrap, onUpdate.wrap));
  const colBox = h('div', { class: 'form' }, cols.wrap, colChips);
  const sync = () => { const k = kind.el.value; fkBox.hidden = k !== 'foreign'; colBox.hidden = k === 'check'; check.wrap.hidden = k !== 'check'; };
  kind.el.addEventListener('change', sync);
  const fields = h('div', { class: 'form' }, h('div', { class: 'form-grid two' }, kind.wrap, name.wrap), colBox, fkBox, check.wrap);
  sync();
  const build = () => {
    const k = kind.el.value;
    const n = name.el.value.trim();
    const head = `ALTER TABLE ${ref} ADD ${n ? `CONSTRAINT ${qi(n)} ` : ''}`;
    if (k === 'check') return [`${head}CHECK (${req(check.el.value, 'the check expression')})`];
    const c = splitCols(req(cols.el.value, 'the columns'));
    if (k === 'primary') return [`${head}PRIMARY KEY (${colList(c)})`];
    if (k === 'unique') return [`${head}UNIQUE (${colList(c)})`];
    const [rs, ...rest] = refTable.el.value.split('.');
    let s = `${head}FOREIGN KEY (${colList(c)}) REFERENCES ${qref(rs, rest.join('.'))} (${colList(splitCols(req(refCols.el.value, 'the referenced columns')))})`;
    if (onDelete.el.value !== 'NO ACTION') s += ` ON DELETE ${onDelete.el.value}`;
    if (onUpdate.el.value !== 'NO ACTION') s += ` ON UPDATE ${onUpdate.el.value}`;
    return [s];
  };
  ddlForm(tab, `Add constraint to ${t.name}`, fields, build);
}

function indexForm(tab, t) {
  const d = sqlDialect();
  const name = fInput('Name', `${t.name}_idx`);
  const cols = fInput('Columns / expressions (comma-separated)', '', { placeholder: 'email, created_at DESC' });
  const colChips = h('div', { class: 'chips' }, t.columns.map((c) => h('button', { type: 'button', class: 'chip', onclick: () => { cols.el.value = [...splitCols(cols.el.value), c.name].join(', '); cols.el.dispatchEvent(new Event('input', { bubbles: true })); } }, c.name)));
  const unique = fCheck('UNIQUE');
  const method = fSelect('Method', ['btree', 'hash', 'gin', 'gist', 'brin', 'spgist'], 'btree');
  const where = fInput('Partial index WHERE (optional)', '', { placeholder: 'deleted_at IS NULL' });
  const fields = h('div', { class: 'form' },
    h('div', { class: 'form-grid two' }, name.wrap, d === 'postgres' ? method.wrap : h('span')),
    cols.wrap, colChips,
    d === 'mysql' ? null : where.wrap,
    unique.wrap,
    h('div', { class: 'hint' }, h('div', {}, 'Plain column names are quoted; anything else (', h('code', {}, 'lower(email)'), ', ', h('code', {}, 'col DESC'), ') is used as written.')));
  const build = () => {
    const parts = splitCols(req(cols.el.value, 'the columns')).map((c) => (/^[A-Za-z_][\w$]*$/.test(c) ? qi(c) : c));
    let s = `CREATE ${unique.el.checked ? 'UNIQUE ' : ''}INDEX ${qi(req(name.el.value, 'the index name'))} ON ${qref(t.schema, t.name)}`;
    if (d === 'postgres' && method.el.value !== 'btree') s += ` USING ${method.el.value}`;
    s += ` (${parts.join(', ')})`;
    if (d !== 'mysql' && where.el.value.trim()) s += ` WHERE ${where.el.value.trim()}`;
    return [s];
  };
  ddlForm(tab, `Add index to ${t.name}`, fields, build);
}

function createTableForm(tab) {
  const d = sqlDialect();
  const cat = tab.catalog;
  const name = fInput('Table name', '');
  const schema = fSelect(d === 'mysql' ? 'Database' : 'Schema', cat.schemas, tab.schemaFilter);
  const rows = h('div', { class: 'col-rows' });
  const addRow = (c = {}) => {
    const n = h('input', { class: 'input mono', value: c.name || '', placeholder: 'column' });
    const ty = typeInput(c.type || '');
    const nn = h('input', { type: 'checkbox', checked: !!c.notNull, title: 'NOT NULL' });
    const pk = h('input', { type: 'checkbox', checked: !!c.pk, title: 'PRIMARY KEY' });
    const df = h('input', { class: 'input mono', value: c.default || '', placeholder: 'default' });
    const row = h('div', { class: 'col-row' }, n, ty.el, ty.list, df, h('label', { class: 'mini-check' }, nn, 'NN'), h('label', { class: 'mini-check' }, pk, 'PK'),
      h('button', { type: 'button', class: 'icon-btn', title: 'Remove', onclick: () => { row.remove(); rows.dispatchEvent(new Event('input', { bubbles: true })); } }, icon('x', 12)));
    row.get = () => ({ name: n.value.trim(), type: ty.el.value.trim(), notNull: nn.checked, pk: pk.checked, default: df.value.trim() });
    rows.append(row);
  };
  const idType = { postgres: 'integer', mysql: 'int', sqlite: 'INTEGER' }[d];
  addRow({ name: 'id', type: idType, pk: true, notNull: true });
  addRow({ name: 'name', type: { postgres: 'text', mysql: 'varchar(255)', sqlite: 'TEXT' }[d], notNull: true });
  const fields = h('div', { class: 'form' },
    h('div', { class: 'form-grid two' }, name.wrap, cat.features.schemas ? schema.wrap : h('span')),
    h('label', {}, 'Columns'), rows,
    h('button', { type: 'button', class: 'btn ghost sm', onclick: () => { addRow(); } }, icon('plus', 12), 'Add column'));
  const build = () => {
    const n = req(name.el.value, 'the table name');
    const cols = [...rows.children].map((r) => r.get()).filter((c) => c.name);
    if (!cols.length) throw new Error('Add at least one column');
    const pks = cols.filter((c) => c.pk);
    const lines = cols.map((c) => {
      let s = `${qi(c.name)} ${c.type || idType}`;
      if (pks.length === 1 && c.pk) {
        if (d === 'postgres' && /^(integer|int|bigint|smallint)$/i.test(c.type)) s += ' GENERATED BY DEFAULT AS IDENTITY';
        s += ' PRIMARY KEY';
        if (d === 'mysql' && /int/i.test(c.type)) s += ' AUTO_INCREMENT';
      } else if (c.notNull) s += ' NOT NULL';
      if (c.default) s += ` DEFAULT ${c.default}`;
      return s;
    });
    if (pks.length > 1) lines.push(`PRIMARY KEY (${colList(pks.map((c) => c.name))})`);
    return [`CREATE TABLE ${qref(schema.el.value, n)} (\n  ${lines.join(',\n  ')}\n)`];
  };
  ddlForm(tab, 'Create table', fields, build, { applyLabel: 'Create', after: () => { tab.sel = { schema: cat.features.schemas ? schema.el.value : 'main', name: name.el.value.trim() }; } });
}

// ---- views
function viewCard(tab, v) {
  const ro = schemaReadOnly(tab);
  const d = sqlDialect();
  const ref = qref(v.schema, v.name);
  const kw = v.materialized ? 'MATERIALIZED VIEW' : 'VIEW';
  const editor = jsonlessSqlEditor(v.definition, ro);
  const save = () => {
    const def = editor.state.doc.toString().trim().replace(/;\s*$/, '');
    if (def === v.definition.trim()) { toast('Nothing changed'); return; }
    const stmts = v.materialized || d === 'sqlite'
      ? [`DROP ${kw} ${ref}`, `CREATE ${kw} ${ref} AS\n${def}`]
      : [`CREATE OR REPLACE VIEW ${ref} AS\n${def}`];
    applyDdl(tab, 'Update view', stmts, { note: d === 'mysql' ? null : (v.materialized ? 'A materialized view is dropped and created again; its data is recomputed.' : null) });
  };
  return h('div', { class: 'card' },
    h('div', { class: 'card-fields' },
      nameField(v.materialized ? 'Materialized view name' : 'View name', v.name, ro, (n, revert) => applyDdl(tab, 'Rename view',
        d === 'mysql' ? [`RENAME TABLE ${ref} TO ${qref(v.schema, n)}`]
          : d === 'sqlite' ? [`DROP VIEW ${ref}`, `CREATE VIEW ${qi(n)} AS\n${v.definition}`]
            : [`ALTER ${kw} ${ref} RENAME TO ${qi(n)}`],
        { after: () => { tab.sel = { schema: v.schema, name: n }; } }).then((ok) => ok || revert()))),
    h('div', { class: 'card-section' }, h('div', { class: 'card-section-head' }, h('span', {}, 'Definition'),
      ro ? null : h('button', { class: 'link-btn', onclick: save }, 'Save'),
      ro || !v.materialized ? null : h('button', { class: 'link-btn', onclick: () => applyDdl(tab, 'Refresh materialized view', [`REFRESH MATERIALIZED VIEW ${ref}`]) }, 'Refresh data')),
    h('div', { class: 'view-editor json-editor' }, editor.dom)),
    h('details', { class: 'card-ddl' }, h('summary', {}, 'DDL'), h('div', { class: 'def-row' }, sqlBlock(v.ddl.trim()), h('div', { class: 'def-actions' }, actBtn('copy', 'Copy', () => { copyText(v.ddl); toast('Copied'); })))),
    ro ? null : h('div', { class: 'card-actions' },
      h('button', { class: 'btn ghost', onclick: () => openTable(v.schema, v.name) }, icon('table', 13), 'Open data'),
      h('span', { class: 'spacer' }),
      h('button', { class: 'btn danger-ghost', onclick: () => applyDdl(tab, 'Drop view', [`DROP ${kw} ${ref}`], { danger: true, after: () => { tab.sel = null; tab.detail = null; } }) }, icon('trash', 13), 'Drop view')));
}

/** A CodeMirror SQL editor for view definitions (no run gutter). */
function jsonlessSqlEditor(text, readOnly) {
  return new CM.EditorView({
    doc: text,
    extensions: [
      CM.lineNumbers(), CM.history(), CM.drawSelection(), CM.bracketMatching(), CM.closeBrackets(), CM.highlightActiveLine(),
      CM.EditorView.lineWrapping, CM.EditorState.readOnly.of(readOnly),
      CM.Prec.highest(CM.keymap.of([{ key: 'Tab', run: CM.acceptCompletion }])),
      CM.keymap.of([...CM.closeBracketsKeymap, ...CM.defaultKeymap, ...CM.searchKeymap, ...CM.historyKeymap, ...CM.completionKeymap, CM.indentWithTab]),
      CM.sql({ dialect: cmSqlDialect() }), sqlHighlighter,
      CM.autocompletion({ override: [sqlCompletionSource], icons: true, interactionDelay: 0 }),
    ],
  });
}

function createViewForm(tab) {
  const cat = tab.catalog;
  const d = sqlDialect();
  const name = fInput('View name', '');
  const schema = fSelect(d === 'mysql' ? 'Database' : 'Schema', cat.schemas, tab.schemaFilter);
  const mat = fCheck('Materialized');
  const editor = jsonlessSqlEditor('SELECT ', false);
  const fields = h('div', { class: 'form' },
    h('div', { class: 'form-grid two' }, name.wrap, cat.features.schemas ? schema.wrap : h('span')),
    cat.features.materializedViews ? mat.wrap : null,
    h('label', {}, 'Definition'), h('div', { class: 'view-editor json-editor' }, editor.dom));
  editor.dispatch({ effects: CM.StateEffect.appendConfig.of(CM.EditorView.updateListener.of((u) => { if (u.docChanged) fields.dispatchEvent(new Event('input', { bubbles: true })); })) });
  const build = () => {
    const def = req(editor.state.doc.toString().replace(/;\s*$/, ''), 'the definition');
    return [`CREATE ${mat.el.checked ? 'MATERIALIZED ' : ''}VIEW ${qref(schema.el.value, req(name.el.value, 'the view name'))} AS\n${def}`];
  };
  ddlForm(tab, 'Create view', fields, build, { applyLabel: 'Create', after: () => { tab.sel = { schema: cat.features.schemas ? schema.el.value : 'main', name: name.el.value.trim() }; } });
}

// ---- enums (PostgreSQL)
function enumCard(tab) {
  const e = tab.catalog.enums.find((x) => x.schema === tab.sel.schema && x.name === tab.sel.name);
  if (!e) return h('div', { class: 'grid-empty' }, 'Enum not found');
  const ro = schemaReadOnly(tab);
  const ref = qref(e.schema, e.name);
  const vals = e.values.map((v) => defRow(lit(v), ro ? [] : [
    actBtn('edit', 'Rename value', () => {
      const to = fInput('New value', v);
      ddlForm(tab, `Rename value ${v}`, to.wrap, () => [`ALTER TYPE ${ref} RENAME VALUE ${lit(v)} TO ${lit(req(to.el.value, 'the value'))}`]);
    }),
  ]));
  return h('div', { class: 'card' },
    h('div', { class: 'card-fields' },
      nameField('Enum name', e.name, ro, (n, revert) => applyDdl(tab, 'Rename enum', [`ALTER TYPE ${ref} RENAME TO ${qi(n)}`], { after: () => { tab.sel = { schema: e.schema, name: n }; } }).then((ok) => ok || revert()))),
    defRow(`CREATE TYPE ${ref} AS ENUM (${e.values.map(lit).join(', ')});`),
    section('Values', 'Add value', ro ? null : () => {
      const v = fInput('Value', '');
      const pos = fSelect('Position', [['end', 'At the end'], ...e.values.flatMap((x) => [[`before:${x}`, `Before ${x}`], [`after:${x}`, `After ${x}`]])], 'end');
      ddlForm(tab, `Add value to ${e.name}`, h('div', { class: 'form' }, v.wrap, pos.wrap), () => {
        let s = `ALTER TYPE ${ref} ADD VALUE ${lit(req(v.el.value, 'the value'))}`;
        if (pos.el.value !== 'end') { const [k, ...x] = pos.el.value.split(':'); s += ` ${k.toUpperCase()} ${lit(x.join(':'))}`; }
        return [s];
      });
    }, vals),
    h('div', { class: 'hint' }, h('div', {}, 'PostgreSQL cannot remove a value from an enum.')),
    ro ? null : h('div', { class: 'card-actions' }, h('span', { class: 'spacer' }),
      h('button', { class: 'btn danger-ghost', onclick: () => applyDdl(tab, 'Drop enum', [`DROP TYPE ${ref}`], { danger: true, note: 'Fails while a column still uses this type.', after: () => { tab.sel = null; } }) }, icon('trash', 13), 'Drop enum')));
}

function createEnumForm(tab) {
  const cat = tab.catalog;
  const name = fInput('Enum name', '');
  const schema = fSelect('Schema', cat.schemas, tab.schemaFilter);
  const values = fArea('Values (one per line)', 'active\ninactive', 4);
  ddlForm(tab, 'Create enum', h('div', { class: 'form' }, h('div', { class: 'form-grid two' }, name.wrap, schema.wrap), values.wrap), () => {
    const vals = values.el.value.split('\n').map((x) => x.trim()).filter(Boolean);
    if (!vals.length) throw new Error('Enter at least one value');
    return [`CREATE TYPE ${qref(schema.el.value, req(name.el.value, 'the enum name'))} AS ENUM (${vals.map(lit).join(', ')})`];
  }, { applyLabel: 'Create', after: () => { tab.sel = { schema: schema.el.value, name: name.el.value.trim() }; } });
}

// ---- roles (PostgreSQL)
const ROLE_FLAGS = [
  ['superuser', 'Superuser', 'SUPERUSER', 'Allow the role to bypass all permission checks and access all objects in the database'],
  ['createDb', 'Create databases', 'CREATEDB', 'Allow the role to create new databases'],
  ['createRole', 'Create roles', 'CREATEROLE', 'Allow the role to create new roles'],
  ['inherit', 'Inherit', 'INHERIT', 'Allow the role to inherit privileges from roles it is a member of'],
  ['canLogin', 'Can login', 'LOGIN', 'Allow the role to log in to the database'],
  ['bypassRls', 'Bypass RLS', 'BYPASSRLS', 'Allow the role to bypass row-level security policies'],
  ['replication', 'Replication', 'REPLICATION', 'Allow the role to initiate replication connections'],
];

function roleCard(tab, r) {
  const ro = schemaReadOnly(tab);
  const name = qi(r.name);
  const flags = ROLE_FLAGS.map(([key, label, kw, desc]) => h('div', { class: 'flag-row' },
    h('label', { class: 'toggle-field' }, h('span', {}, label), h('input', {
      type: 'checkbox', class: 'switch', checked: r[key], disabled: ro,
      onchange: (e) => applyDdl(tab, `${label}: ${e.target.checked ? 'on' : 'off'}`, [`ALTER ROLE ${name} ${e.target.checked ? '' : 'NO'}${kw}`]).then((ok) => { if (!ok) e.target.checked = !e.target.checked; }),
    })),
    h('div', { class: 'muted small' }, desc)));
  const limit = h('input', { class: 'input mono', placeholder: 'Connection limit', value: r.connectionLimit >= 0 ? String(r.connectionLimit) : '', readonly: ro, inputmode: 'numeric' });
  limit.addEventListener('change', () => {
    const v = limit.value.trim() === '' ? -1 : Number(limit.value);
    if (!Number.isInteger(v)) { toast('Connection limit must be a whole number', 'error'); return; }
    applyDdl(tab, 'Connection limit', [`ALTER ROLE ${name} CONNECTION LIMIT ${v}`]);
  });
  const until = h('input', { class: 'input mono', type: 'datetime-local', value: r.validUntil ? r.validUntil.slice(0, 16).replace(' ', 'T') : '', readonly: ro, title: 'Valid until' });
  until.addEventListener('change', () => applyDdl(tab, 'Valid until', [`ALTER ROLE ${name} VALID UNTIL ${until.value ? lit(until.value.replace('T', ' ')) : "'infinity'"}`]));
  const privs = r.privileges.map((g) => defRow(`GRANT ${g.privileges.join(', ')} ON ${qref(g.schema, g.table)} TO ${name};`, ro ? [] : [
    actBtn('trash', 'Revoke all', () => applyDdl(tab, 'Revoke privileges', [`REVOKE ALL ON ${qref(g.schema, g.table)} FROM ${name}`], { danger: true }), 'danger-icon'),
  ]));
  return h('div', { class: 'card' },
    h('div', { class: 'card-fields' },
      nameField('Role name', r.name, ro, (n, revert) => applyDdl(tab, 'Rename role', [`ALTER ROLE ${name} RENAME TO ${qi(n)}`], { after: () => { tab.sel = { name: n }; } }).then((ok) => ok || revert())),
      ro ? null : h('button', { class: 'btn danger', onclick: () => {
        const p1 = fInput('New password', '', { type: 'password', autocomplete: 'new-password' });
        const p2 = fInput('Repeat password', '', { type: 'password', autocomplete: 'new-password' });
        ddlForm(tab, `Reset password for ${r.name}`, h('div', { class: 'form' }, p1.wrap, p2.wrap), () => {
          const p = req(p1.el.value, 'the password');
          if (p !== p2.el.value) throw new Error('The passwords do not match');
          return [{ sql: `ALTER ROLE ${name} PASSWORD ${lit(p)}`, display: `ALTER ROLE ${name} PASSWORD '••••••••'` }];
        });
      } }, 'Reset password')),
    defRow(r.definition),
    h('div', { class: 'flags' }, flags),
    h('div', { class: 'form-grid two role-limits' }, limit, until),
    r.memberOf.length ? h('p', { class: 'muted small' }, `Member of: ${r.memberOf.join(', ')}`) : null,
    section('Privileges', 'Add privilege', ro ? null : () => privilegeForm(tab, r), privs),
    ro ? null : h('div', { class: 'card-actions' }, h('span', { class: 'spacer' }),
      h('button', { class: 'btn danger-ghost', onclick: () => applyDdl(tab, 'Drop role', [`DROP ROLE ${name}`], { danger: true, note: 'Fails while the role owns objects or has privileges.', after: () => { tab.sel = null; tab.detail = null; } }) }, icon('trash', 13), 'Drop role')));
}

function privilegeForm(tab, r) {
  const cat = tab.catalog;
  const table = fSelect('Table', [['*', 'All tables in schema'], ...cat.tables.map((t) => [`${t.schema}.${t.name}`, `${t.schema}.${t.name}`])], cat.tables[0] ? `${cat.tables[0].schema}.${cat.tables[0].name}` : '*');
  const schema = fSelect('Schema (for "All tables")', cat.schemas, tab.schemaFilter);
  const checks = PG_PRIVILEGES.map((p) => ({ p, ...fCheck(p, p === 'SELECT') }));
  const fields = h('div', { class: 'form' }, h('div', { class: 'form-grid two' }, table.wrap, schema.wrap), h('div', { class: 'check-row' }, checks.map((c) => c.wrap)));
  ddlForm(tab, `Grant privileges to ${r.name}`, fields, () => {
    const ps = checks.filter((c) => c.el.checked).map((c) => c.p);
    if (!ps.length) throw new Error('Select at least one privilege');
    const target = table.el.value === '*' ? `ALL TABLES IN SCHEMA ${qi(schema.el.value)}` : (() => { const [s, ...n] = table.el.value.split('.'); return `TABLE ${qref(s, n.join('.'))}`; })();
    return [`GRANT ${ps.join(', ')} ON ${target} TO ${qi(r.name)}`];
  });
}

function createRoleForm(tab) {
  const name = fInput('Role name', '');
  const login = fCheck('Can login', true);
  const pass = fInput('Password (optional)', '', { type: 'password', autocomplete: 'new-password' });
  ddlForm(tab, 'Create role', h('div', { class: 'form' }, name.wrap, login.wrap, pass.wrap), () => {
    const n = req(name.el.value, 'the role name');
    const base = `CREATE ROLE ${qi(n)}${login.el.checked ? ' LOGIN' : ''}`;
    return [pass.el.value ? { sql: `${base} PASSWORD ${lit(pass.el.value)}`, display: `${base} PASSWORD '••••••••'` } : base];
  }, { applyLabel: 'Create', after: () => { tab.sel = { name: name.el.value.trim() }; } });
}

// ---- policies (PostgreSQL)
function policyCard(tab) {
  const s = tab.sel;
  const p = tab.catalog.policies.find((x) => x.schema === s.schema && x.table === s.table && x.name === s.name);
  if (!p) return h('div', { class: 'grid-empty' }, 'Policy not found');
  const ro = schemaReadOnly(tab);
  return h('div', { class: 'card' },
    h('div', { class: 'card-fields' },
      h('label', { class: 'card-field' }, h('span', {}, 'Policy'), h('input', { class: 'input mono', value: p.name, readonly: true })),
      h('label', { class: 'card-field' }, h('span', {}, 'Table'), h('input', { class: 'input mono', value: `${p.schema}.${p.table}`, readonly: true }))),
    defRow(p.definition),
    h('div', { class: 'policy-facts' },
      h('div', {}, h('span', { class: 'muted' }, 'Command '), p.command),
      h('div', {}, h('span', { class: 'muted' }, 'Type '), p.permissive ? 'PERMISSIVE' : 'RESTRICTIVE'),
      h('div', {}, h('span', { class: 'muted' }, 'Roles '), p.roles.join(', ') || 'public')),
    ro ? null : h('div', { class: 'card-actions' },
      h('button', { class: 'btn', onclick: () => policyForm(tab, p) }, icon('edit', 13), 'Edit'),
      h('button', { class: 'btn ghost', onclick: () => { tab.section = 'tables'; selectItem(tab, { schema: p.schema, name: p.table }); } }, icon('table', 13), 'Open table'),
      h('span', { class: 'spacer' }),
      h('button', { class: 'btn danger-ghost', onclick: () => applyDdl(tab, 'Drop policy', [`DROP POLICY ${qi(p.name)} ON ${qref(p.schema, p.table)}`], { danger: true, after: () => { tab.sel = null; } }) }, icon('trash', 13), 'Drop policy')));
}

function policyForm(tab, p, table = null) {
  const cat = tab.catalog;
  const name = fInput('Name', p?.name || '');
  const tbl = fSelect('Table', cat.tables.map((t) => [`${t.schema}.${t.name}`, `${t.schema}.${t.name}`]), p ? `${p.schema}.${p.table}` : table ? `${table.schema}.${table.name}` : undefined);
  if (p || table) tbl.el.disabled = true;
  const cmd = fSelect('Command', ['ALL', 'SELECT', 'INSERT', 'UPDATE', 'DELETE'], p?.command || 'ALL');
  const kind = fSelect('Type', [['PERMISSIVE', 'PERMISSIVE'], ['RESTRICTIVE', 'RESTRICTIVE']], p && !p.permissive ? 'RESTRICTIVE' : 'PERMISSIVE');
  const roleNames = cat.roles.filter((r) => !r.system).map((r) => r.name);
  const roles = fInput('Roles (comma-separated, empty = public)', (p?.roles || []).filter((r) => r !== 'public').join(', '), { list: 'policy-roles' });
  const using = fArea('USING (rows the role can see / change)', p?.using || '', 2);
  const check = fArea('WITH CHECK (rows the role can write)', p?.withCheck || '', 2);
  const fields = h('div', { class: 'form' },
    h('div', { class: 'form-grid two' }, name.wrap, tbl.wrap),
    h('div', { class: 'form-grid two' }, cmd.wrap, kind.wrap),
    roles.wrap, h('datalist', { id: 'policy-roles' }, roleNames.map((r) => h('option', { value: r }))),
    using.wrap, check.wrap);
  ddlForm(tab, p ? `Edit policy ${p.name}` : 'Create policy', fields, () => {
    const [s, ...n] = tbl.el.value.split('.');
    const ref = qref(s, n.join('.'));
    const rl = splitCols(roles.el.value);
    let sql = `CREATE POLICY ${qi(req(name.el.value, 'the policy name'))} ON ${ref} AS ${kind.el.value} FOR ${cmd.el.value} TO ${rl.length ? rl.map((r) => (r === 'public' ? r : qi(r))).join(', ') : 'public'}`;
    if (using.el.value.trim()) sql += ` USING (${using.el.value.trim()})`;
    if (check.el.value.trim()) sql += ` WITH CHECK (${check.el.value.trim()})`;
    return p ? [`DROP POLICY ${qi(p.name)} ON ${qref(p.schema, p.table)}`, sql] : [sql];
  }, { applyLabel: p ? 'Save' : 'Create', after: () => { if (tab.section === 'policies') { const [s, ...n] = tbl.el.value.split('.'); tab.sel = { schema: s, table: n.join('.'), name: name.el.value.trim() }; } } });
}

// ---- create (+) per section
function createObject(tab) {
  const d = sqlDialect();
  switch (tab.section) {
    case 'schemas': {
      const name = fInput(d === 'mysql' ? 'Database name' : 'Schema name', '');
      ddlForm(tab, d === 'mysql' ? 'Create database' : 'Create schema', name.wrap, () => [`CREATE ${d === 'mysql' ? 'DATABASE' : 'SCHEMA'} ${qi(req(name.el.value, 'the name'))}`],
        { applyLabel: 'Create', after: () => { tab.sel = { name: name.el.value.trim() }; } });
      break;
    }
    case 'tables': createTableForm(tab); break;
    case 'views': createViewForm(tab); break;
    case 'enums': createEnumForm(tab); break;
    case 'roles': createRoleForm(tab); break;
    case 'policies': policyForm(tab, null); break;
    default: break;
  }
}

// ---------------------------------------------------------------- layout
function initSidebarResizer() {
  const saved = store.get('rs.sidebarWidth', 248);
  document.documentElement.style.setProperty('--sidebar-w', `${saved}px`);
  $('#sidebar-resizer').addEventListener('mousedown', (e) => {
    e.preventDefault();
    document.body.classList.add('resizing');
    const move = (ev) => {
      const w = Math.max(180, Math.min(480, ev.clientX));
      document.documentElement.style.setProperty('--sidebar-w', `${w}px`);
      store.set('rs.sidebarWidth', w);
    };
    const up = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up); document.body.classList.remove('resizing'); };
    document.addEventListener('mousemove', move);
    document.addEventListener('mouseup', up);
  });
}

window.addEventListener('beforeunload', (e) => {
  saveTabs();
  if (state.tabs.some((t) => t.type === 'table' && pendingCount(t) > 0)) { e.preventDefault(); e.returnValue = ''; }
});

// Redis has its own sidebar and views (static/redis.js).
// ---------------------------------------------------------------- saved queries
// SQL statements and Redis commands kept on the server (saved-queries.json). A query belongs to
// one connection, or to all connections with the same dialect (for DEV / PROD copies of one database).

const queryKind = () => (isRedis() ? 'redis' : 'sql');

async function loadSavedQueries() {
  try { state.savedQueries = await api('/queries'); } catch { state.savedQueries = []; }
}

/** Saved queries for the current connection, sorted by name. */
function visibleQueries() {
  const d = state.schema?.dialect;
  if (!d) return [];
  return state.savedQueries
    .filter((q) => q.kind === queryKind() && (q.connectionId ? q.connectionId === state.connId : q.dialect === d))
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
}

const findSaved = (id) => (id ? state.savedQueries.find((q) => q.id === id) : null);

/** A default name: the first non-comment line of the query, shortened. */
function suggestQueryName(text) {
  const line = text.split('\n').map((l) => l.trim()).find((l) => l && !l.startsWith('--') && !l.startsWith('#')) || 'Query';
  return line.length > 60 ? line.slice(0, 57) + '…' : line;
}

/**
 * Opens the dialog that creates a saved query (`text` given) or edits the name and scope of
 * `existing`. Resolves with the saved query, or null when the user cancels.
 */
function openSaveQueryModal({ text = null, name = '', existing = null }) {
  return new Promise((resolve) => {
    const d = state.schema.dialect;
    const nameF = fInput('Name', existing ? existing.name : name, { mono: false, placeholder: 'Active users' });
    const shared = fCheck(`Show for all ${DIALECT_LABEL[d] || d} connections`, existing ? !existing.connectionId : false);
    let done = false;
    const finish = (v) => { if (!done) { done = true; resolve(v); } };
    const submit = async () => {
      const body = { name: nameF.el.value, connectionId: shared.el.checked ? '' : state.connId };
      try {
        const q = existing
          ? await api(`/queries/${existing.id}`, { method: 'PATCH', body: { ...body, ...(text != null ? { text } : {}) } })
          : await api('/queries', { body: { ...body, kind: queryKind(), text, dialect: d } });
        state.savedQueries = [...state.savedQueries.filter((x) => x.id !== q.id), q];
        finish(q);
        close();
        renderSidebar();
      } catch (e) { toast(e.message, 'error'); nameF.el.focus(); }
    };
    nameF.el.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); submit(); } });
    const body = h('div', { class: 'form' }, nameF.wrap, shared.wrap,
      h('div', { class: 'muted small' }, 'Off: only this connection shows the query.'),
      text != null && !existing ? h('pre', { class: 'saved-preview mono' }, text.length > 600 ? text.slice(0, 600) + '…' : text) : null);
    const close = openModal(existing ? 'Edit saved query' : 'Save query', body, [
      h('button', { class: 'btn ghost', onclick: () => close() }, 'Cancel'),
      h('button', { class: 'btn primary', onclick: submit }, icon('check', 13), existing ? 'Save' : 'Save query'),
    ], { onClose: () => finish(null) });
    setTimeout(() => { nameF.el.focus(); nameF.el.select(); }, 0);
  });
}

async function deleteSavedQuery(q) {
  if (!confirm(`Delete the saved query "${q.name}"?`)) return;
  try {
    await api(`/queries/${q.id}`, { method: 'DELETE' });
    state.savedQueries = state.savedQueries.filter((x) => x.id !== q.id);
    for (const t of state.tabs) if (t.savedId === q.id) t.savedId = null;
    saveTabs();
    renderSidebar();
    if (activeTab()?.type === 'sql') renderMain();
    toast('Query deleted');
  } catch (e) { toast(e.message, 'error'); }
}

/** Saves the SQL tab: updates its saved query, or asks for a name (always when `asNew`). */
async function saveSqlTab(tab, { asNew = false } = {}) {
  const text = tab.cm ? tab.cm.state.doc.toString() : tab.sql;
  if (!text.trim()) { toast('The editor is empty', 'error'); return; }
  const linked = !asNew && findSaved(tab.savedId);
  if (linked) {
    try {
      const q = await api(`/queries/${linked.id}`, { method: 'PATCH', body: { text } });
      state.savedQueries = state.savedQueries.map((x) => (x.id === q.id ? q : x));
      toast(`Saved "${q.name}"`, 'success');
      renderSidebar();
    } catch (e) { toast(e.message, 'error'); }
    return;
  }
  const q = await openSaveQueryModal({ text, name: /^SQL \d+$/.test(tab.title) ? suggestQueryName(text) : tab.title });
  if (!q) return;
  tab.savedId = q.id;
  tab.title = q.name;
  saveTabs();
  toast(`Saved "${q.name}"`, 'success');
  if (tab.id === state.activeTab) renderMain();
  renderSidebar();
}

/** Opens a saved query: SQL in its own tab (reused when open), Redis in the console. */
function openSavedQuery(q) {
  if (q.kind === 'redis') { R.insertCommand(q.text); return; }
  let tab = state.tabs.find((t) => t.type === 'sql' && t.savedId === q.id);
  if (!tab) {
    tab = newSqlTab(q.name, q.text);
    tab.savedId = q.id;
    state.tabs.push(tab);
  }
  activate(tab.id);
}

function renameSavedQuery(q) {
  openSaveQueryModal({ existing: q }).then((x) => {
    if (!x) return;
    for (const t of state.tabs) if (t.type === 'sql' && t.savedId === x.id) t.title = x.name;
    saveTabs();
    renderMain();
  });
}

/** The "Saved queries" sidebar section. Hidden when there are none. */
function renderSavedSection(sb) {
  const list = visibleQueries();
  if (!list.length) return;
  const collapsed = store.get('rs.savedCollapsed', false);
  const active = activeTab();
  sb.append(h('button', {
    class: 'sb-section sb-section-toggle', title: collapsed ? 'Show saved queries' : 'Hide saved queries',
    onclick: () => { store.set('rs.savedCollapsed', !collapsed); renderSidebar(); },
  }, h('span', {}, icon(collapsed ? 'right' : 'chevron', 11), 'Saved queries', h('span', { class: 'r-count' }, String(list.length)))));
  if (collapsed) return;
  const d = DIALECT_LABEL[state.schema.dialect] || state.schema.dialect;
  sb.append(h('div', { class: 'saved-list' }, list.map((q) => h('div', {
    class: `sb-item saved-item${active?.savedId === q.id ? ' active' : ''}`, role: 'button', tabindex: '0',
    title: `${q.text.length > 400 ? q.text.slice(0, 400) + '…' : q.text}\n\n${q.connectionId ? 'This connection only' : `All ${d} connections`}`,
    onclick: () => openSavedQuery(q),
    onkeydown: (e) => { if (e.key === 'Enter') openSavedQuery(q); },
  },
  icon('bookmark', 14),
  h('span', { class: 'sb-name' }, q.name),
  q.connectionId ? null : h('span', { class: 'saved-shared', title: `Shared with all ${d} connections` }, icon('db', 11)),
  h('span', { class: 'saved-actions' },
    h('button', { class: 'icon-btn', title: 'Rename or change scope', onclick: (e) => { e.stopPropagation(); renameSavedQuery(q); } }, icon('edit', 12)),
    h('button', { class: 'icon-btn', title: 'Delete', onclick: (e) => { e.stopPropagation(); deleteSavedQuery(q); } }, icon('trash', 12))),
  ))));
}

const R = createRedisUI({
  h, icon, api, toast, state, store, openModal, openMenu, closeMenu, copyText, fmtNum, compact,
  jsonPane, looksLikeJsonText, reformatJson,
  conn, activeTab, activate, closeTab, saveTabs, renderSidebar, renderMain,
  renderSavedSection, openSaveQueryModal, suggestQueryName, visibleQueries, openSavedQuery,
  nextTabId: () => tabSeq++, nextConsoleNo: () => sqlSeq++,
});

boot();
