// Rowveil — Redis UI: key browser, typed value editors, command console.
// app.js passes its helpers in `createRedisUI(ctx)`; this module owns no global state.

const TYPES = {
  string: { label: 'STR', name: 'String' },
  hash: { label: 'HASH', name: 'Hash' },
  list: { label: 'LIST', name: 'List' },
  set: { label: 'SET', name: 'Set' },
  zset: { label: 'ZSET', name: 'Sorted set' },
  stream: { label: 'STRM', name: 'Stream' },
  'ReJSON-RL': { label: 'JSON', name: 'JSON' },
  json: { label: 'JSON', name: 'JSON' },
};
const typeInfo = (t) => TYPES[t] || { label: String(t || '?').slice(0, 4).toUpperCase(), name: t };
const typeClass = (t) => `rtype ${t === 'ReJSON-RL' ? 'json' : (TYPES[t] ? t : 'other')}`;

const PAGE = 200;

export function createRedisUI(ctx) {
  const { h, icon, api, toast, state, store, openModal, openMenu, closeMenu, copyText, fmtNum, compact,
    jsonPane, looksLikeJsonText, reformatJson } = ctx;

  // ------------------------------------------------------------ state
  const rs = () => state.redis;
  const cid = () => state.connId;
  const readOnly = () => !!state.schema?.readOnly;

  function onConnect() {
    const saved = store.get(`rs.redis.${cid()}`, {});
    state.redis = {
      db: saved.db ?? state.schema.redis.defaultDb ?? 0,
      pattern: saved.pattern || '',
      type: saved.type || '',
      tree: saved.tree ?? true,
      keys: [],
      cursor: '0',
      done: false,
      loading: false,
      error: null,
      open: new Set(saved.open || []),
      collapsed: new Set(),
    };
    scan(true);
  }

  function persist() {
    const r = rs();
    if (!r) return;
    store.set(`rs.redis.${cid()}`, { db: r.db, pattern: r.pattern, type: r.type, tree: r.tree, open: [...r.open].slice(0, 200) });
  }

  const keyCount = (db) => state.schema?.redis?.keyspace?.find((k) => k.db === db)?.keys ?? 0;

  async function refreshOverview() {
    try {
      const o = await api(`/connections/${cid()}/redis/overview`);
      state.schema.redis = o;
    } catch { /* keep the old numbers */ }
  }

  // ------------------------------------------------------------ scanning
  function globPattern(text) {
    const t = text.trim();
    if (!t) return '*';
    return /[*?[]/.test(t) ? t : `*${t}*`;
  }

  async function scan(reset = false) {
    const r = rs();
    if (!r || r.loading) return;
    if (reset) { r.keys = []; r.cursor = '0'; r.done = false; r.error = null; r.collapsed.clear(); }
    r.loading = true;
    ctx.renderSidebar();
    try {
      const res = await api(`/connections/${cid()}/redis/scan`, {
        body: { db: r.db, pattern: globPattern(r.pattern), cursor: r.cursor, count: 500, type: r.type || null },
      });
      const seen = new Set(r.keys.map((k) => k.key));
      for (const k of res.keys) if (!seen.has(k.key)) r.keys.push(k);
      r.keys.sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
      r.cursor = res.cursor;
      r.done = res.cursor === '0';
    } catch (e) {
      r.error = e.message;
    }
    r.loading = false;
    ctx.renderSidebar();
  }

  async function reloadKeys() {
    await refreshOverview();
    await scan(true);
  }

  // ------------------------------------------------------------ sidebar
  function renderSidebar(sb) {
    const r = rs();
    const o = state.schema.redis;
    const dbSel = h('select', {
      class: 'schema-select', value: String(r.db), title: 'Database',
      onchange: (e) => { r.db = Number(e.target.value); persist(); scan(true); },
    }, Array.from({ length: Math.max(o.databases || 16, r.db + 1) }, (_, i) =>
      h('option', { value: String(i) }, `db${i}${keyCount(i) ? ` · ${fmtNum(keyCount(i))} keys` : ''}`)));
    sb.append(h('div', { class: 'sb-row' }, h('label', { class: 'sb-label' }, 'Database'), dbSel));

    let timer = null;
    const search = h('input', {
      class: 'sb-search', placeholder: 'Filter keys: user:*', value: r.pattern, title: 'Glob pattern (* ? [abc]). Plain text matches anywhere in the key',
      oninput: (e) => { r.pattern = e.target.value; clearTimeout(timer); timer = setTimeout(() => { persist(); scan(true); }, 300); },
      onkeydown: (e) => { if (e.key === 'Enter') { clearTimeout(timer); persist(); scan(true); } },
    });
    const typeSel = h('select', {
      class: 'r-type-select', value: r.type, title: 'Key type',
      onchange: (e) => { r.type = e.target.value; persist(); scan(true); },
    }, h('option', { value: '' }, 'All'), ['string', 'hash', 'list', 'set', 'zset', 'stream', 'ReJSON-RL'].map((t) => h('option', { value: t }, typeInfo(t).name)));
    sb.append(h('div', { class: 'sb-search-wrap r-search' }, icon('search', 14), search, typeSel));

    const consoleActive = ctx.activeTab()?.type === 'rcli';
    sb.append(h('button', { class: `sb-item sql-item${consoleActive ? ' active' : ''}`, onclick: () => openConsole() }, icon('terminal'), h('span', {}, 'Redis console'), h('span', { class: 'kbd' }, '+')));
    if (!readOnly()) sb.append(h('button', { class: 'sb-item sql-item', onclick: () => createKeyForm() }, icon('plus'), h('span', {}, 'Add key')));

    const total = keyCount(r.db);
    sb.append(h('div', { class: 'sb-section' },
      h('span', {}, 'Keys', h('span', { class: 'r-count' }, r.pattern || r.type ? `${fmtNum(r.keys.length)}${r.done ? '' : '+'} found` : fmtNum(total))),
      h('span', { class: 'r-section-actions' },
        h('button', { class: `icon-btn${r.tree ? ' on' : ''}`, title: r.tree ? 'Show as a list' : 'Group by ":"', onclick: () => { r.tree = !r.tree; persist(); renderKeyList(); ctx.renderSidebar(); } }, icon(r.tree ? 'folder' : 'list', 13)),
        h('button', { class: 'icon-btn', title: 'Reload keys', onclick: reloadKeys }, icon('refresh', 13)),
      )));
    sb.append(h('div', { id: 'table-list', class: 'table-list r-keys' }));
    renderKeyList();
  }

  function keyButton(k, label, depth) {
    const active = ctx.activeTab();
    const isActive = active?.type === 'rkey' && active.key === k.key && active.db === rs().db;
    return h('button', {
      class: `sb-item r-key${isActive ? ' active' : ''}`, title: k.key, style: { paddingLeft: `${8 + depth * 12}px` },
      onclick: (e) => openKey(k.key, { newTab: e.ctrlKey || e.metaKey }),
      onauxclick: (e) => { if (e.button === 1) openKey(k.key, { newTab: true }); },
    },
    h('span', { class: typeClass(k.type) }, typeInfo(k.type).label),
    h('span', { class: 'sb-name' }, label),
    k.ttl > 0 ? h('span', { class: 'r-ttl-dot', title: `Expires in ${fmtTtl(k.ttl)}` }, icon('history', 11)) : null,
    );
  }

  function renderKeyList() {
    const list = document.getElementById('table-list');
    const r = rs();
    if (!list || !r) return;
    list.replaceChildren();
    if (r.error) { list.append(h('div', { class: 'sb-empty small error' }, r.error)); return; }
    if (!r.keys.length && r.loading) { list.append(h('div', { class: 'sb-empty small' }, h('span', { class: 'spinner' }), ' Scanning…')); return; }
    if (!r.keys.length && r.done) { list.append(h('div', { class: 'sb-empty small' }, r.pattern || r.type ? 'No matching keys' : 'This database is empty')); }
    if (r.tree) renderTree(list, r);
    else for (const k of r.keys) list.append(keyButton(k, k.key, 0));
    if (!r.done) {
      list.append(h('button', { class: 'btn ghost sm r-more', disabled: r.loading, onclick: () => scan(false) },
        r.loading ? h('span', { class: 'spinner' }) : null, r.loading ? 'Scanning…' : 'Load more keys'));
    }
  }

  // Groups keys by ":" into folders. A folder with one key shows the key directly.
  function renderTree(list, r) {
    const root = { dirs: new Map(), keys: [], count: 0 };
    for (const k of r.keys) {
      const parts = k.key.split(':');
      let node = root;
      node.count++;
      for (let i = 0; i < parts.length - 1; i++) {
        const name = parts.slice(0, i + 1).join(':') + ':';
        if (!node.dirs.has(name)) node.dirs.set(name, { name, label: parts[i], dirs: new Map(), keys: [], count: 0 });
        node = node.dirs.get(name);
        node.count++;
      }
      node.keys.push({ k, label: parts[parts.length - 1] || k.key });
    }
    // While a filter is active, folders start open (r.collapsed holds the ones the user closed).
    const filtering = !!r.pattern.trim();
    const isOpen = (name) => (filtering ? !r.collapsed.has(name) : r.open.has(name));
    const toggle = (name) => {
      const set = filtering ? r.collapsed : r.open;
      if (set.has(name)) set.delete(name); else set.add(name);
      persist();
      renderKeyList();
    };
    const walk = (node, depth) => {
      // Folders and keys in one alphabetical list.
      const entries = [
        ...[...node.dirs.values()].map((d) => ({ sort: d.label, d })),
        ...node.keys.map((x) => ({ sort: x.label, x })),
      ].sort((a, b) => (a.sort < b.sort ? -1 : a.sort > b.sort ? 1 : 0));
      for (const e of entries) {
        if (e.x) { list.append(keyButton(e.x.k, e.x.label, depth)); continue; }
        const d = e.d;
        if (d.count === 1) {
          const only = findOnly(d);
          list.append(keyButton(only, only.key.slice(node.name?.length || 0), depth));
          continue;
        }
        const open = isOpen(d.name);
        list.append(h('button', {
          class: 'sb-item r-folder', style: { paddingLeft: `${8 + depth * 12}px` }, title: `${d.name}*`,
          onclick: () => toggle(d.name),
        },
        h('span', { class: `r-caret${open ? ' open' : ''}` }, icon('right', 12)),
        icon('folder', 13),
        h('span', { class: 'sb-name' }, d.label || '(empty)'),
        h('span', { class: 'sb-count' }, compact(d.count))));
        if (open) walk(d, depth + 1);
      }
    };
    walk(root, 0);
  }

  function findOnly(d) {
    if (d.keys.length) return d.keys[0].k;
    return findOnly(d.dirs.values().next().value);
  }

  // ------------------------------------------------------------ tabs
  function newKeyTab(key, db) {
    return { id: ctx.nextTabId(), type: 'rkey', key, db, data: null, loading: false, error: null, filter: '', desc: false, dirty: false };
  }
  function newConsoleTab(title) {
    return { id: ctx.nextTabId(), type: 'rcli', title: title || `Console ${ctx.nextConsoleNo()}`, log: [], running: false };
  }

  function openKey(key, { newTab = false } = {}) {
    const r = rs();
    let tab = !newTab && state.tabs.find((t) => t.type === 'rkey' && t.key === key && t.db === r.db);
    if (!tab) {
      tab = newKeyTab(key, r.db);
      const current = ctx.activeTab();
      if (!newTab && current?.type === 'rkey' && !current.dirty) state.tabs[state.tabs.indexOf(current)] = tab;
      else state.tabs.push(tab);
    }
    ctx.activate(tab.id);
  }

  function openConsole() {
    const t = newConsoleTab();
    state.tabs.push(t);
    ctx.activate(t.id);
  }

  function saveTab(t) {
    if (t.type === 'rkey') return { type: 'rkey', key: t.key, db: t.db };
    if (t.type === 'rcli') return { type: 'rcli', title: t.title };
    return null;
  }
  function restoreTab(t) {
    if (t.type === 'rkey') return newKeyTab(t.key, t.db ?? 0);
    if (t.type === 'rcli') return newConsoleTab(t.title);
    return null;
  }
  function tabTitle(t) { return t.type === 'rkey' ? t.key : t.title; }
  function tabIcon(t) { return t.type === 'rkey' ? 'key' : 'terminal'; }

  // ------------------------------------------------------------ key view
  async function load(tab, { more = false } = {}) {
    if (tab.loading) return;
    tab.loading = true;
    if (!more) tab.error = null;
    rerender(tab);
    try {
      const prev = more ? tab.data : null;
      const body = { db: tab.db, key: tab.key, limit: PAGE, filter: tab.filter ? globPattern(tab.filter) : '', desc: tab.desc };
      if (more && prev) {
        if (prev.type === 'list' || prev.type === 'zset') body.offset = prev.items.length;
        else body.cursor = prev.cursor;
      }
      const d = await api(`/connections/${cid()}/redis/key`, { body });
      if (more && prev) { d.items = prev.items.concat(d.items || []); }
      tab.data = d;
      tab.dirty = false;
    } catch (e) {
      tab.error = e.message;
      if (!more) tab.data = null;
    }
    tab.loading = false;
    rerender(tab);
  }

  function rerender(tab) {
    if (ctx.activeTab() === tab) ctx.renderMain();
  }

  function fmtTtl(ms) {
    if (ms == null || ms < 0) return 'No expiry';
    const s = Math.round(ms / 1000);
    if (s < 60) return `${s}s`;
    if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
    if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
    return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
  }

  const fmtBytes = (n) => {
    if (n == null) return '—';
    const u = ['B', 'KB', 'MB', 'GB'];
    let v = n; let i = 0;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return i ? `${v.toFixed(1)} ${u[i]}` : `${n} B`;
  };

  async function edit(tab, op, args = {}, { reload = true, quiet = false } = {}) {
    try {
      const r = await api(`/connections/${cid()}/redis/edit`, { body: { db: tab.db, key: tab.key, op, args } });
      if (!quiet) toast('Saved', 'success');
      if (reload) await load(tab);
      return r;
    } catch (e) {
      toast(e.message, 'error');
      throw e;
    }
  }

  function renderKey(tab, root) {
    if (!tab.data && !tab.loading && !tab.error) load(tab);
    const d = tab.data;
    if (tab.error && !d) {
      root.append(h('div', { class: 'empty-main' }, icon('key', 36), h('h2', {}, tab.key), h('p', { class: 'error-text' }, tab.error),
        h('button', { class: 'btn', onclick: () => { forgetKey(tab); } }, 'Close tab')));
      return;
    }
    if (!d) { root.append(h('div', { class: 'empty-main' }, h('span', { class: 'spinner' }))); return; }
    const ro = readOnly();
    const head = h('div', { class: 'r-head' },
      h('span', { class: typeClass(d.type) }, typeInfo(d.type).label),
      h('span', { class: 'r-key-name mono', title: d.key }, d.key),
      h('button', { class: 'icon-btn', title: 'Copy key name', onclick: () => copyText(d.key) }, icon('copy', 13)),
      ro ? null : h('button', { class: 'icon-btn', title: 'Rename', onclick: () => renameForm(tab) }, icon('edit', 13)),
      h('span', { class: 'spacer' }),
      h('button', { class: `r-chip${d.ttl > 0 ? ' warn' : ''}`, title: ro ? 'Time to live' : 'Change the time to live', disabled: ro, onclick: () => ttlForm(tab) }, icon('history', 12), `TTL: ${fmtTtl(d.ttl)}`),
      d.length != null ? h('span', { class: 'r-chip', title: 'Length' }, lengthLabel(d)) : null,
      h('span', { class: 'r-chip', title: 'MEMORY USAGE' }, fmtBytes(d.memory)),
      d.encoding ? h('span', { class: 'r-chip', title: 'OBJECT ENCODING' }, d.encoding) : null,
      h('button', { class: 'icon-btn', title: 'Reload', onclick: () => load(tab) }, tab.loading ? h('span', { class: 'spinner' }) : icon('refresh', 14)),
      ro ? null : h('button', { class: 'icon-btn danger', title: 'Delete key', onclick: () => deleteKey(tab) }, icon('trash', 14)),
    );
    root.append(h('div', { class: 'r-view' }, head, valueView(tab, d, ro)));
  }

  function lengthLabel(d) {
    const n = fmtNum(d.length);
    switch (d.type) {
      case 'string': return `${n} bytes`;
      case 'hash': return `${n} fields`;
      case 'list': case 'set': case 'zset': return `${n} items`;
      case 'stream': return `${n} entries`;
      default: return n;
    }
  }

  function valueView(tab, d, ro) {
    if (d.unsupported) {
      return h('div', { class: 'empty-main' }, h('p', { class: 'muted' }, d.unsupported),
        h('button', { class: 'btn', onclick: () => openConsoleWith(`TYPE ${quoteArg(d.key)}`) }, icon('terminal'), 'Open console'));
    }
    switch (d.type) {
      case 'string': return stringView(tab, d, ro);
      case 'json': return jsonView(tab, d, ro);
      case 'hash': return collectionView(tab, d, ro, {
        cols: ['Field', 'Value'],
        rows: d.items.map(([f, v]) => ({ cells: [f, v], f, v })),
        filter: true,
        onAdd: () => hashForm(tab),
        onEdit: (row) => hashForm(tab, row.f, row.v),
        onDelete: (row) => edit(tab, 'hdel', { field: row.f }),
        copy: (row) => row.v,
      });
      case 'list': return collectionView(tab, d, ro, {
        cols: ['Index', 'Value'],
        rows: d.items.map((v, i) => ({ cells: [String(d.offset + i), v], i: d.offset + i, v })),
        actions: [h('button', { class: 'btn sm', onclick: () => valueForm(tab, 'Push to the head', '', (v) => call(tab, 'lpush', { value: v })) }, icon('plus', 13), 'Head'),
          h('button', { class: 'btn sm', onclick: () => valueForm(tab, 'Push to the tail', '', (v) => call(tab, 'rpush', { value: v })) }, icon('plus', 13), 'Tail')],
        onEdit: (row) => valueForm(tab, `Element ${row.i}`, row.v, (v) => call(tab, 'lset', { index: String(row.i), value: v })),
        onDelete: (row) => edit(tab, 'lrem_index', { index: String(row.i), value: row.v }),
        more: d.items.length < d.length,
        copy: (row) => row.v,
      });
      case 'set': return collectionView(tab, d, ro, {
        cols: ['Member'],
        rows: d.items.map((m) => ({ cells: [m], v: m })),
        filter: true,
        onAdd: () => valueForm(tab, 'Add member', '', (v) => call(tab, 'sadd', { value: v })),
        onEdit: (row) => valueForm(tab, 'Edit member', row.v, (v) => call(tab, 'sadd', { value: v, oldValue: row.v })),
        onDelete: (row) => edit(tab, 'srem', { value: row.v }),
        copy: (row) => row.v,
      });
      case 'zset': return collectionView(tab, d, ro, {
        cols: ['#', 'Member', 'Score'],
        rows: d.items.map(([m, s], i) => ({ cells: [String(d.offset + i + 1), m, s], v: m, s })),
        actions: [h('button', { class: 'btn sm', title: 'Sort order', onclick: () => { tab.desc = !tab.desc; load(tab); } }, tab.desc ? 'Highest score first' : 'Lowest score first')],
        onAdd: () => zsetForm(tab),
        onEdit: (row) => zsetForm(tab, row.v, row.s),
        onDelete: (row) => edit(tab, 'zrem', { value: row.v }),
        more: d.items.length < d.length,
        copy: (row) => row.v,
      });
      case 'stream': return streamView(tab, d, ro);
      default: return h('div', {}, JSON.stringify(d));
    }
  }

  // ---- string
  function stringView(tab, d, ro) {
    const editable = !ro && !d.binary && !d.truncated;
    const note = d.binary ? 'Binary value: shown with \\xHH escapes, read-only here. Use the console to change it.'
      : d.truncated ? `Showing the first ${fmtNum(d.value.length)} of ${fmtNum(d.length)} bytes. Use the console to change it.` : null;
    if (looksLikeJsonText(d.value) && !d.binary) {
      return jsonView(tab, d, !editable, 'set');
    }
    const area = h('textarea', { class: 'r-text mono', spellcheck: 'false', readonly: !editable, value: d.value });
    const save = h('button', { class: 'btn primary sm', disabled: true, onclick: () => edit(tab, 'set', { value: area.value }) }, icon('check', 13), 'Save', h('span', { class: 'kbd' }, 'Ctrl S'));
    const reset = h('button', { class: 'btn ghost sm', disabled: true, onclick: () => { area.value = d.value; changed(); } }, icon('undo', 13), 'Discard');
    const changed = () => { const dirty = area.value !== d.value; save.disabled = !dirty; reset.disabled = !dirty; tab.dirty = dirty; };
    area.addEventListener('input', changed);
    area.addEventListener('keydown', (e) => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') { e.preventDefault(); if (!save.disabled) save.click(); } });
    return h('div', { class: 'r-body' },
      note ? h('div', { class: 'r-note' }, note) : null,
      area,
      editable ? h('div', { class: 'r-actions' }, h('span', { class: 'spacer' }), reset, save) : null);
  }

  // ---- JSON (ReJSON documents and JSON strings)
  function jsonView(tab, d, ro, op = 'json_set') {
    const pane = jsonPane(d.value, {
      readOnly: ro,
      onSubmit: ro ? null : () => submit(),
      onCancel: null,
    });
    const submit = () => {
      const v = pane.value();
      edit(tab, op, { value: v });
    };
    return h('div', { class: 'r-body' },
      h('div', { class: 'r-json' }, pane.dom),
      ro ? null : h('div', { class: 'r-actions' },
        h('span', { class: 'muted small' }, op === 'json_set' ? 'JSON.SET key $ value' : 'SET key value KEEPTTL'),
        h('span', { class: 'spacer' }),
        h('button', { class: 'btn primary sm', onclick: submit }, icon('check', 13), 'Save', h('span', { class: 'kbd' }, 'Ctrl Enter'))));
  }

  // ---- hash / list / set / zset
  function collectionView(tab, d, ro, opts) {
    const bar = h('div', { class: 'r-toolbar' });
    if (opts.filter) {
      let timer = null;
      bar.append(h('div', { class: 'sb-search-wrap r-filter' }, icon('search', 13), h('input', {
        class: 'sb-search', placeholder: d.type === 'hash' ? 'Filter fields (glob)' : 'Filter members (glob)', value: tab.filter,
        oninput: (e) => { tab.filter = e.target.value; clearTimeout(timer); timer = setTimeout(() => load(tab), 300); },
      })));
    }
    for (const a of opts.actions || []) if (!ro || a.title === 'Sort order') bar.append(a);
    bar.append(h('span', { class: 'spacer' }));
    bar.append(h('span', { class: 'muted small' }, `${fmtNum(opts.rows.length)} of ${fmtNum(d.length)} shown`));
    if (!ro && opts.onAdd) bar.append(h('button', { class: 'btn primary sm', onclick: opts.onAdd }, icon('plus', 13), 'Add'));

    const table = h('table', { class: 'grid r-grid' },
      h('thead', {}, h('tr', {}, opts.cols.map((c) => h('th', { class: `c-${c.toLowerCase().replace('#', 'n')}` }, h('div', { class: 'th-inner' }, h('span', { class: 'th-name' }, c)))), h('th', { class: 'r-act-col' }))),
      h('tbody', {}, opts.rows.map((row) => h('tr', { ondblclick: () => { if (!ro && opts.onEdit) opts.onEdit(row); } },
        row.cells.map((c, i) => h('td', { class: `cell${i === row.cells.length - 1 && opts.cols.length > 1 ? ' r-val' : ''}`, title: c.length > 200 ? `${c.slice(0, 200)}…` : c }, h('span', { class: 'cell-text' }, c))),
        h('td', { class: 'r-act-col' },
          h('button', { class: 'icon-btn', title: 'Copy value', onclick: () => copyText(opts.copy(row)) }, icon('copy', 13)),
          !ro && opts.onEdit ? h('button', { class: 'icon-btn', title: 'Edit', onclick: () => opts.onEdit(row) }, icon('edit', 13)) : null,
          !ro && opts.onDelete ? h('button', { class: 'icon-btn danger', title: 'Delete', onclick: () => opts.onDelete(row).catch(() => {}) }, icon('trash', 13)) : null,
        ))),
      ),
    );
    const more = opts.more ?? (d.cursor && d.cursor !== '0');
    return h('div', { class: 'r-body' }, bar,
      h('div', { class: 'grid-wrap r-grid-wrap' }, table,
        opts.rows.length ? null : h('div', { class: 'sb-empty small' }, tab.filter ? 'No matches' : 'Empty'),
        more ? h('button', { class: 'btn ghost sm r-more', disabled: tab.loading, onclick: () => load(tab, { more: true }) }, tab.loading ? 'Loading…' : `Load ${PAGE} more`) : null));
  }

  // ---- stream
  function streamView(tab, d, ro) {
    const bar = h('div', { class: 'r-toolbar' },
      d.groups?.length ? h('span', { class: 'muted small' }, `Consumer groups: ${d.groups.map((g) => `${g.name} (pending ${g.pending ?? 0}, lag ${g.lag ?? '?'})`).join(', ')}`) : h('span', { class: 'muted small' }, 'Newest entries first'),
      h('span', { class: 'spacer' }),
      ro ? null : h('button', { class: 'btn primary sm', onclick: () => streamForm(tab) }, icon('plus', 13), 'Add entry'));
    const rows = d.items.map((e) => {
      const ms = Number(e.id.split('-')[0]);
      const when = Number.isFinite(ms) ? new Date(ms).toISOString().replace('T', ' ').replace('Z', '') : '';
      const fields = JSON.stringify(e.fields);
      return h('tr', {},
        h('td', { class: 'cell mono' }, h('span', { class: 'cell-text' }, e.id)),
        h('td', { class: 'cell muted' }, h('span', { class: 'cell-text' }, when)),
        h('td', { class: 'cell r-val', title: fields }, h('span', { class: 'cell-text' }, Object.entries(e.fields).map(([k, v]) => [h('b', { class: 'r-field' }, k), ' ', v, '   ']))),
        h('td', { class: 'r-act-col' },
          h('button', { class: 'icon-btn', title: 'Copy as JSON', onclick: () => copyText(fields) }, icon('copy', 13)),
          ro ? null : h('button', { class: 'icon-btn danger', title: 'Delete entry', onclick: () => edit(tab, 'xdel', { id: e.id }).catch(() => {}) }, icon('trash', 13))));
    });
    return h('div', { class: 'r-body' }, bar,
      h('div', { class: 'grid-wrap r-grid-wrap' },
        h('table', { class: 'grid r-grid r-stream' },
          h('thead', {}, h('tr', {}, ['ID', 'Time (UTC)', 'Fields'].map((c) => h('th', {}, h('div', { class: 'th-inner' }, h('span', { class: 'th-name' }, c)))), h('th', { class: 'r-act-col' }))),
          h('tbody', {}, rows)),
        d.cursor && d.cursor !== '0' ? h('button', { class: 'btn ghost sm r-more', onclick: () => load(tab, { more: true }) }, `Load ${PAGE} older`) : null));
  }

  // ------------------------------------------------------------ forms
  function valueInput(value = '') {
    return h('textarea', { class: 'input mono r-input', rows: Math.min(14, Math.max(3, String(value).split('\n').length + 1)), spellcheck: 'false', value });
  }

  function formModal(title, fields, onSave, { wide = false, saveLabel = 'Save' } = {}) {
    const status = h('div', { class: 'form-status' });
    const body = h('div', { class: 'form' }, fields, status);
    const save = h('button', {
      class: 'btn primary', onclick: async () => {
        save.disabled = true;
        status.textContent = '';
        try { await onSave(); close(); } catch (e) { status.textContent = e.message; status.className = 'form-status error'; save.disabled = false; }
      },
    }, saveLabel);
    const close = openModal(title, body, [h('button', { class: 'btn', onclick: () => close() }, 'Cancel'), save], { wide });
    body.addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) save.click(); });
    setTimeout(() => body.querySelector('input, textarea')?.focus(), 0);
    return close;
  }

  // Edits through the API; errors go to the form, so `edit` is not used here.
  async function call(tab, op, args) {
    await api(`/connections/${cid()}/redis/edit`, { body: { db: tab.db, key: tab.key, op, args } });
    toast('Saved', 'success');
    await load(tab);
  }

  function valueForm(tab, title, value, save) {
    const v = valueInput(value);
    formModal(title, [h('label', {}, 'Value', v)], () => save(v.value).then(() => {}), { wide: true });
  }

  function hashForm(tab, field = null, value = '') {
    const f = h('input', { class: 'input mono', value: field ?? '' });
    const v = valueInput(value);
    formModal(field == null ? 'Add field' : 'Edit field', [h('label', {}, 'Field', f), h('label', {}, 'Value', v)], async () => {
      if (!f.value) throw new Error('Enter the field name');
      await call(tab, 'hset', field == null ? { field: f.value, value: v.value } : { oldField: field, field: f.value, value: v.value });
    }, { wide: true });
  }

  function zsetForm(tab, member = null, score = '') {
    const m = valueInput(member ?? '');
    const s = h('input', { class: 'input mono', value: score, placeholder: '0', inputmode: 'decimal' });
    formModal(member == null ? 'Add member' : 'Edit member', [h('label', {}, 'Score', s), h('label', {}, 'Member', m)], async () => {
      if (!s.value.trim()) throw new Error('Enter the score');
      await call(tab, 'zadd', member == null ? { value: m.value, score: s.value } : { value: m.value, score: s.value, oldValue: member });
    }, { wide: true });
  }

  function fieldRows(initial = [['', '']]) {
    const box = h('div', { class: 'r-fields' });
    const add = (k = '', v = '') => {
      const row = h('div', { class: 'r-field-row' },
        h('input', { class: 'input mono', placeholder: 'field', value: k }),
        h('input', { class: 'input mono', placeholder: 'value', value: v }),
        h('button', { class: 'icon-btn', type: 'button', title: 'Remove', onclick: () => { if (box.children.length > 1) row.remove(); } }, icon('x', 13)));
      box.append(row);
    };
    for (const [k, v] of initial) add(k, v);
    const read = () => [...box.children].map((r) => [r.children[0].value, r.children[1].value]).filter(([k]) => k);
    return { dom: h('div', {}, box, h('button', { class: 'btn ghost sm', type: 'button', onclick: () => add() }, icon('plus', 13), 'Field')), read };
  }

  function streamForm(tab) {
    const id = h('input', { class: 'input mono', value: '*', title: '* lets Redis generate the ID' });
    const rows = fieldRows();
    formModal('Add stream entry', [h('label', {}, 'ID', id), h('label', {}, 'Fields', rows.dom)], async () => {
      const fields = rows.read();
      if (!fields.length) throw new Error('Add at least one field');
      await call(tab, 'xadd', { id: id.value.trim() || '*', fields });
    }, { wide: true });
  }

  function ttlForm(tab) {
    const cur = tab.data?.ttl > 0 ? String(Math.ceil(tab.data.ttl / 1000)) : '';
    const t = h('input', { class: 'input mono', value: cur, placeholder: 'No expiry', inputmode: 'numeric' });
    formModal('Time to live', [h('label', {}, 'Seconds', t), h('div', { class: 'hint' }, 'Leave empty to remove the expiry (PERSIST).')], async () => {
      await call(tab, 'expire', { ttl: t.value.trim() });
      updateListedKey(tab.key, { ttl: t.value.trim() ? Number(t.value) * 1000 : -1 });
    });
  }

  function renameForm(tab) {
    const n = h('input', { class: 'input mono', value: tab.key });
    formModal('Rename key', [h('label', {}, 'New name', n), h('div', { class: 'hint' }, 'RENAMENX: fails when a key with this name exists.')], async () => {
      const to = n.value;
      if (to === tab.key) return;
      await api(`/connections/${cid()}/redis/edit`, { body: { db: tab.db, key: tab.key, op: 'rename', args: { newKey: to } } });
      const k = rs().keys.find((x) => x.key === tab.key);
      if (k) { k.key = to; rs().keys.sort((a, b) => (a.key < b.key ? -1 : 1)); }
      tab.key = to;
      ctx.saveTabs();
      toast('Renamed', 'success');
      await load(tab);
      ctx.renderSidebar();
    });
  }

  async function deleteKey(tab) {
    if (!confirm(`Delete key "${tab.key}"?`)) return;
    try {
      await api(`/connections/${cid()}/redis/edit`, { body: { db: tab.db, key: tab.key, op: 'delete' } });
      toast('Key deleted', 'success');
      forgetKey(tab);
      refreshOverview().then(() => ctx.renderSidebar());
    } catch (e) { toast(e.message, 'error'); }
  }

  function forgetKey(tab) {
    const r = rs();
    r.keys = r.keys.filter((k) => !(k.key === tab.key && tab.db === r.db));
    ctx.closeTab(tab.id);
  }

  function updateListedKey(key, patch) {
    const k = rs().keys.find((x) => x.key === key);
    if (k) Object.assign(k, patch);
    ctx.renderSidebar();
  }

  function createKeyForm() {
    const r = rs();
    let type = 'string';
    const key = h('input', { class: 'input mono', placeholder: 'user:42' });
    const ttl = h('input', { class: 'input mono', placeholder: 'No expiry', inputmode: 'numeric' });
    const typeSeg = h('div', { class: 'seg seg-full' });
    const body = h('div', { class: 'form' });
    const modules = state.schema.redis.modules || [];
    const types = ['string', 'hash', 'list', 'set', 'zset', 'stream'].concat(modules.some((m) => /json/i.test(m)) ? ['json'] : []);
    let inputs = {};
    const fill = () => {
      body.replaceChildren();
      inputs = {};
      if (type === 'hash') { inputs.field = h('input', { class: 'input mono', placeholder: 'field' }); body.append(h('label', {}, 'First field', inputs.field)); }
      if (type === 'zset') { inputs.score = h('input', { class: 'input mono', placeholder: '0', value: '0' }); body.append(h('label', {}, 'Score', inputs.score)); }
      if (type === 'stream') { inputs.fields = fieldRows(); body.append(h('label', {}, 'First entry', inputs.fields.dom)); return; }
      inputs.value = valueInput(type === 'json' ? '{\n  \n}' : '');
      const label = { string: 'Value', hash: 'Value', list: 'First element', set: 'First member', zset: 'Member', json: 'JSON document' }[type];
      body.append(h('label', {}, label, inputs.value));
    };
    for (const t of types) typeSeg.append(h('button', { type: 'button', class: t === type ? 'on' : '', onclick: (e) => { type = t; for (const b of typeSeg.children) b.classList.toggle('on', b === e.currentTarget); fill(); } }, typeInfo(t).name));
    fill();
    formModal(`Add key to db${r.db}`, [
      h('div', { class: 'form-grid two' }, h('label', {}, 'Key', key), h('label', {}, h('span', {}, 'TTL, seconds ', h('span', { class: 'opt' }, '(optional)')), ttl)),
      h('label', {}, 'Type', typeSeg), body,
    ], async () => {
      if (!key.value) throw new Error('Enter the key name');
      const args = { type, ttl: ttl.value.trim() };
      if (inputs.value) args.value = inputs.value.value;
      if (inputs.field) { if (!inputs.field.value) throw new Error('Enter the field name'); args.field = inputs.field.value; }
      if (inputs.score) args.score = inputs.score.value;
      if (inputs.fields) args.fields = inputs.fields.read();
      await api(`/connections/${cid()}/redis/edit`, { body: { db: r.db, key: key.value, op: 'create', args } });
      toast('Key created', 'success');
      await refreshOverview();
      await scan(true);
      openKey(key.value);
    }, { wide: true, saveLabel: 'Create' });
  }

  // ------------------------------------------------------------ console
  function quoteArg(s) {
    return /^[^\s"'\\]+$/.test(s) ? s : `"${s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n')}"`;
  }

  function openConsoleWith(cmd) {
    const t = newConsoleTab();
    t.draft = cmd;
    state.tabs.push(t);
    ctx.activate(t.id);
  }

  // Prints a reply like redis-cli.
  function formatReply(r, indent = '') {
    switch (r.t) {
      case 'nil': return '(nil)';
      case 'int': return `(integer) ${r.v}`;
      case 'double': return `(double) ${r.v}`;
      case 'bool': return r.v ? '(true)' : '(false)';
      case 'status': return r.v;
      case 'error': return `(error) ${r.v}`;
      case 'string': return JSON.stringify(r.v);
      case 'array': case 'set': {
        if (!r.v.length) return r.t === 'set' ? '(empty set)' : '(empty array)';
        const w = String(r.v.length).length;
        return r.v.map((x, i) => {
          const prefix = `${String(i + 1).padStart(w)}) `;
          const inner = formatReply(x, indent + ' '.repeat(prefix.length));
          return (i ? indent : '') + prefix + inner;
        }).join('\n');
      }
      case 'map': {
        if (!r.v.length) return '(empty hash)';
        const w = String(r.v.length).length;
        return r.v.map(([k, v], i) => {
          const prefix = `${String(i + 1).padStart(w)}# `;
          return (i ? indent : '') + prefix + formatReply(k) + ' => ' + formatReply(v, indent + ' '.repeat(prefix.length + 4));
        }).join('\n');
      }
      default: return JSON.stringify(r.v);
    }
  }

  const HISTORY_KEY = 'rs.redisHistory';

  function renderConsole(tab, root) {
    const r = rs();
    const out = h('div', { class: 'r-log mono' });
    const draw = () => {
      out.replaceChildren(...tab.log.map((e) => h('div', { class: 'r-entry' },
        h('div', { class: 'r-cmd' }, h('span', { class: 'r-prompt' }, `db${e.db}>`), ' ', e.cmd),
        e.pending ? h('div', { class: 'r-out muted' }, h('span', { class: 'spinner' })) : h('pre', { class: `r-out${e.error ? ' error' : ''}${e.reply?.t === 'error' ? ' error' : ''}` }, e.error ? `(error) ${e.error}` : formatReply(e.reply)),
        e.ms != null ? h('div', { class: 'r-ms' }, `${e.ms.toFixed(1)} ms`) : null)));
      if (!tab.log.length) {
        out.append(h('div', { class: 'r-help muted' },
          h('div', {}, 'Type a Redis command and press Enter. Shift+Enter adds a line: each line runs as one command.'),
          h('div', {}, 'Examples: ', ['SCAN 0 MATCH user:* COUNT 100', 'HGETALL user:1', 'ZREVRANGE leaderboard 0 9 WITHSCORES', 'INFO memory'].map((x) =>
            h('button', { class: 'r-example mono', onclick: () => { input.value = x; input.focus(); } }, x))),
          readOnly() ? h('div', {}, 'The connection is read-only: only read commands run.') : null));
      }
      out.scrollTop = out.scrollHeight;
    };
    const history = store.get(HISTORY_KEY, []);
    let hpos = history.length;
    const input = h('textarea', { class: 'r-input-line mono', rows: 1, spellcheck: 'false', placeholder: 'GET key', value: tab.draft || '' });
    const grow = () => { input.style.height = 'auto'; input.style.height = `${Math.min(200, input.scrollHeight)}px`; };
    input.addEventListener('input', () => { tab.draft = input.value; grow(); });
    const run = async () => {
      const lines = input.value.split('\n').map((x) => x.trim()).filter(Boolean);
      if (!lines.length || tab.running) return;
      input.value = '';
      tab.draft = '';
      grow();
      tab.running = true;
      for (const cmd of lines) {
        if (/^(clear|cls)$/i.test(cmd)) { tab.log = []; draw(); continue; }
        const last = history[history.length - 1];
        if (last !== cmd) { history.push(cmd); if (history.length > 300) history.shift(); store.set(HISTORY_KEY, history); }
        hpos = history.length;
        const e = { cmd, db: r.db, pending: true };
        tab.log.push(e);
        if (tab.log.length > 500) tab.log.shift();
        draw();
        try {
          const res = await api(`/connections/${cid()}/redis/command`, { body: { db: r.db, command: cmd } });
          e.reply = res.reply;
          e.ms = res.durationMs;
        } catch (err) { e.error = err.message; }
        e.pending = false;
        draw();
      }
      tab.running = false;
      // Commands may have changed keys: refresh the counts quietly.
      refreshOverview().then(() => ctx.renderSidebar());
    };
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); run(); return; }
      const atTop = !input.value.slice(0, input.selectionStart).includes('\n');
      const atBottom = !input.value.slice(input.selectionEnd).includes('\n');
      if (e.key === 'ArrowUp' && atTop && history.length) {
        e.preventDefault(); hpos = Math.max(0, hpos - 1); input.value = history[hpos] || ''; grow();
      } else if (e.key === 'ArrowDown' && atBottom && hpos < history.length) {
        e.preventDefault(); hpos = Math.min(history.length, hpos + 1); input.value = history[hpos] || ''; grow();
      } else if (e.key === 'l' && e.ctrlKey) {
        e.preventDefault(); tab.log = []; draw();
      }
    });
    root.append(h('div', { class: 'r-console' },
      h('div', { class: 'r-toolbar' },
        h('span', { class: 'muted small' }, `Database db${r.db} · ↑/↓ history · Ctrl+L clear`),
        h('span', { class: 'spacer' }),
        h('button', { class: 'btn ghost sm', onclick: () => { tab.log = []; draw(); input.focus(); } }, 'Clear')),
      out,
      h('div', { class: 'r-input-wrap' }, h('span', { class: 'r-prompt mono' }, `db${r.db}>`), input,
        h('button', { class: 'btn primary sm', onclick: run }, icon('play', 12), 'Run'))));
    draw();
    grow();
    setTimeout(() => input.focus(), 0);
  }

  // ------------------------------------------------------------ overview
  function renderEmpty(root) {
    const o = state.schema.redis;
    const s = o.server || {};
    const up = Number(s.uptime_in_seconds || 0);
    const rows = [
      ['Version', s.valkey_version ? `Valkey ${s.valkey_version}` : s.redis_version ? `Redis ${s.redis_version}` : '—'],
      ['Mode', [s.redis_mode, s.role].filter(Boolean).join(' · ') || '—'],
      ['Memory', s.used_memory_human ? `${s.used_memory_human}${s.maxmemory_human && s.maxmemory_human !== '0B' ? ` of ${s.maxmemory_human}` : ''}` : '—'],
      ['Clients', s.connected_clients || '—'],
      ['Uptime', up ? fmtTtl(up * 1000) : '—'],
      ['Modules', (o.modules || []).join(', ') || 'none'],
    ];
    root.append(h('div', { class: 'empty-main' },
      icon('db', 40),
      h('h2', {}, ctx.conn().name),
      h('table', { class: 'r-info' }, rows.map(([k, v]) => h('tr', {}, h('th', {}, k), h('td', {}, v))),
        h('tr', {}, h('th', {}, 'Keys'), h('td', {}, (o.keyspace || []).map((k) => `db${k.db}: ${fmtNum(k.keys)}`).join(' · ') || 'none'))),
      h('p', { class: 'muted' }, 'Select a key in the sidebar or open the console.'),
      h('div', { class: 'r-actions' },
        h('button', { class: 'btn', onclick: () => openConsole() }, icon('terminal'), 'Open console'),
        readOnly() ? null : h('button', { class: 'btn', onclick: () => createKeyForm() }, icon('plus'), 'Add key'))));
  }

  return { onConnect, renderSidebar, renderKey, renderConsole, renderEmpty, saveTab, restoreTab, tabTitle, tabIcon, openConsole, load };
}
