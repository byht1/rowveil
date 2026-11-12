# Rowveil Next: migration checklist

The feature inventory of the stable web UI (static/app.js, static/redis.js). Every item must work
in Rowveil Next before it replaces the stable app. `{c}` = `/connections/{id}`; all paths have the
`/api` prefix (Next calls the same router in-process). Tick an item when it works in Next. `[x]` works, `[~]` partly (see the note), `[ ]` not yet.

## Global / infra
- [x] Boot: connections list; open the last connection when "open last" is on
- [x] Prefs (Next: next-prefs.json): theme, openLast, last connection, tabs per connection, history
- [x] Downloads: native "Save as" dialog, toast with "Show in folder"
- [x] PROD tag: red accent and `[PROD]` window title prefix
- [x] Sidebar resizer 180–480 px (default 248), saved
- [x] Warn on close when table tabs have unsaved changes

## Start page / connections
- [x] Welcome screen when there are no connections (Add connection)
- [x] Cards: dialect dot, name, dialect, read-only badge, "last used" badge, up to 4 tags, masked URL; search on name/URL/dialect/tags; Enter opens the first match; edit button per card; "Add connection" card; "Open the last connection on start" checkbox
- [x] Tag filter chips (All + tags; presets first, then A–Z); click = only that tag, second click clears; Ctrl/Shift = several; saved; shared with the connection menu
- [x] Sidebar connection switcher: All connections, tag filter, list (read-only badge, edit, delete for user connections, "env" badge), Add connection, "Reconnect & reload schema" (`GET {c}/schema?refresh=1`)
- [x] Delete connection: confirm → `DELETE /connections/{id}`
- [x] New connection dialog, Fields / URL modes (saved); switching converts both ways
  - Types: PostgreSQL (5432, postgres, sslmode disable/require), MySQL/MariaDB (3306, root, ssl-mode disabled/required), SQLite, Redis (6379, db number, TLS → rediss://), BigQuery
  - Server fields: Host, Port, User, Password (show/hide), Database, SSL Default/Disable/Require, Extra parameters
  - SQLite: file path. BigQuery: Project ID, Default dataset, Service account key file (`credentials=`), Location, Max bytes billed (`maximumBytesBilled`), API endpoint
  - Live URL preview (password masked); percent-encoding; IPv6 brackets
  - Validation: URL; SQLite file; BigQuery project + numeric max bytes; Redis db number; host; port 1–65535
  - Name, Tags, Read-only; Enter saves; Test `POST /connections/test {url}` → "Connected (X)"; Save & connect `POST /connections {name,url,readOnly,tags}`
- [x] Edit connection: Name, URL (read-only), Tags, Read-only, Redis "Keys per scan"; env connections: tags only; `PATCH /connections/{id}`; reconnect if readOnly/scanLimit changed
- [x] Tag editor: presets LOCAL/DEV/TEST/STAGE/PROD + free tags (Enter/comma adds, Backspace removes), uppercase, max 24 chars, max 8; tone colors
- [x] Schema load error state with Retry

## Sidebar (SQL dialects)
- [x] Schema / Database / Dataset picker (when more than one), saved
- [x] Table search
- [x] "SQL console" item, "Schema" item, Saved queries section, "Tables" header with Refresh (drops tabs of vanished tables, toast)
- [x] Table/view list: icon by kind, compact row estimate; click opens (Ctrl = new tab), middle-click new tab
- [x] Table context menu: New SQL `SELECT  FROM t` (cursor after SELECT, completion), Open in a new tab, Copy name, Copy schema.name
- [x] Footer: version (updates dialog), Update pill, theme button

## Tabs
- [x] Kinds: table, sql, schema (single), rkey, rcli
- [x] Tab bar: icon (bookmark for linked saved query), title, dirty dot, close; middle-click closes; "+" new SQL (Redis: console)
- [x] Close with pending changes: confirm; activate the neighbour
- [x] Preview tab: opening a table replaces the current clean table tab (unless new tab / filters)
- [x] Persistence per connection: tabs, active, schemaName (table: filters, sort, pageSize, hidden, widths, view; sql: title, sql, savedId, resHidden, resWidths; schema: section, sel, filter; rkey: key, db); drop tabs of missing tables
- [x] Empty state: connection name, table count, dialect, "Open SQL console"

## Table data view
- [x] Toolbar: title, read-only badge with reason (connection read-only, view, no primary key), "Grant write access", Data/Structure switch
- [x] Rows `POST {c}/rows {schema,table,filters,sort,limit,offset}` → `{columns,rows,total}`
- [x] Paging: 25/50/100/250/500/1000 (default 50), "a–b of total", prev/next
- [x] Sort: header click asc → desc → off; Shift+click multi-sort with index numbers
- [x] Filters panel (count badge): column + op + value rows; Enter/Apply; Clear; add/remove. Ops `= <> > >= < <= like, not like, ilike, not ilike, in, not in, is null, is not null`
- [x] Columns menu: checkbox per column with type; Show all; Hide all (keeps first); `visible/total` badge; saved
- [x] Column widths: drag (min 60), default by kind, saved
- [x] Refresh (keeps the selection)
- [x] Export: page → CSV/JSON; all filtered rows (max 10k, `limit:10000, withCount:false`) → CSV/JSON
- [~] Header: PK icon, FK icon (tooltip target), type, sort indicator; tooltip "name type not null default x"; right-click "Copy column name"  — Next: PK/FK icons and sort; no type line in the header
- [x] Cells: NULL, default, booleans, numbers, JSON, strings cut at 500; modified / selected / read-only styles
- [~] Row selection: checkboxes + select all; "Delete N"  — Next: Delete row / Undo delete per row (no checkbox column, no select-all)
- [~] Keyboard: arrows, Tab/Shift+Tab, Enter/F2 edit (or view), Ctrl+C copies, Esc clears; double-click edits/views  — Next: arrows, Enter/F2, Ctrl+C; Tab moves focus
- [x] FK jump button in a cell: opens the referenced table filtered `{column: ref, op:'=', value}`
- [x] Cell editor: boolean/enum buttons; JSON pane; textarea for text/json/array, input otherwise; Set NULL; Use default (new rows); Enter applies, Shift+Enter newline, Ctrl+Enter (JSON), Esc
- [x] Value viewer (read-only cells, SQL results): JSON pane or text, "json in text" badge, Copy
- [~] JSON pane: line numbers, folding, Formatted/Raw (saved), Collapse/Expand, valid/invalid status, Shift+Alt+F formats; keep big numbers and key order; compact stays compact  — Next: highlighting, line numbers, folding, Raw/Formatted, valid status; no Collapse/Expand buttons
- [x] Add record (row at top marked "+", editor on first editable column)
- [x] Staged changes: updates by PK (equal to original removes), inserts, deletes (struck through, toggle); footer count, Discard, Save changes (Ctrl+S)
- [x] Apply `POST {c}/apply {schema,table,changes:[insert{values}|update{pk,values}|delete{pk}]}` → toast, reload
- [x] Row drawer: all fields, FK links, "Referenced by" (reverse FKs, new tab with filter), prev/next, Copy as JSON
- [x] Structure sub-view: Columns (PK, auto/generated, type, enum values, nullable, default, references), Indexes, Foreign keys, Referenced by
- [x] BigQuery footer note: filtered view runs a billed query job; table preview is free

## SQL tab
- [~] Editor: highlighting (backticks for mysql/bigquery); run marker in the gutter; flash of the statement sent; multi-cursor  — Next: highlighting; no run marker in the gutter, no flash
- [x] Run current (selection, or statement under/above the cursor), Run all, Explain (`EXPLAIN` / `EXPLAIN QUERY PLAN`); `POST {c}/sql {sql}` → `{results, durationMs}`
- [x] After DDL-looking statements, reload the schema silently
- [x] Results: Table/JSON switch; "Result N" picker (default: last with columns); rows "(truncated)", ms, info; Columns menu; Copy as JSON; CSV/JSON download; resizable columns; double-click viewer; Ctrl+C; right-click copy column name
- [x] Autocomplete: columns of referenced tables + aliases, `alias.`/`schema.`, tables, schemas, keywords, dialect functions (insert `()`); ranking; Tab/Enter accept; Ctrl+Space
- [x] Prettify (selection or all, keywords uppercase, dialect-aware), Shift+Alt+F
- [x] Editable tab title
- [x] History: last 50 distinct per connection; History menu
- [x] Shortcuts dialog
- [x] SQL text autosaves into the tab

## Saved queries (SQL and Redis)
- [x] `GET /queries`; `POST /queries {name, connectionId ('' = shared), kind:'sql'|'redis', text, dialect}`; `PATCH /queries/{id}`; `DELETE /queries/{id}` (confirm)
- [x] Ctrl+S: update linked query or ask name (suggested from first line) + "Show for all <dialect> connections"; Save as new; Rename or change scope
- [x] Sidebar section (collapsible, sorted, shared icon, rename/delete, tooltip); click opens (reuses tab; Redis → console)

## Schema tab (DDL)
- [x] Sections Schemas, Tables, Views, Enums, Roles, Policies (by `catalog.features`)
- [x] `GET {c}/ddl/catalog`; details `GET {c}/ddl/table|view|role`
- [x] Apply DDL `POST {c}/ddl {statements}` with a preview dialog (danger; MySQL non-transactional hint); then refresh schema, drop vanished tabs, reload catalog
- [x] Copy schema / open in console: `GET {c}/ddl/dump` → `{sql}`
- [x] List: schema picker, search, refresh, + create, badges (RLS, MAT, enum count), system roles after a separator
- [x] Schema card: rename, counts, drop (CASCADE; DROP DATABASE on MySQL)
- [x] Table card: rename, move schema, RLS; columns add/edit/drop; constraints add (FK/UNIQUE/PK/CHECK, ON DELETE/UPDATE)/drop; indexes add (method, partial, unique)/drop; policies; triggers list; DDL + Copy; Open data; Drop
- [x] Create table form (name, schema, columns: name, type, default, NN, PK; single integer PK → identity / AUTO_INCREMENT)
- [x] View card: rename, edit definition + Save, refresh materialized, DDL, Open data, Drop; create view
- [x] Enum card (PG): rename, rename value, add value (end / before / after), drop; create
- [x] Role card (PG): rename, password, 7 flags, connection limit, valid until, privileges grant/revoke, drop; create
- [x] Policy card (PG): facts, edit, open table, drop; form (name, table, command, permissive, roles, USING, WITH CHECK)

## Read-only / grant-write
- [x] Read-only from `schema.readOnly`/`readOnlyReason`: no Add/Delete, read-only cells, lock badge; Schema tab hides edit controls
- [x] "Grant write access" (SQLite on Windows, `canGrantWrite`): `POST {c}/grant-write {}` then schema refresh

## BigQuery
- [x] Connection fields and URL; "Dataset" label; backtick quoting; BigQuery functions in autocomplete; billing note

## Redis
- [x] State per connection: db, pattern, type, tree, open folders
- [x] Overview `GET {c}/redis/overview`
- [x] Sidebar: DB select with counts, glob filter (plain text → `*text*`, debounce, Enter), type select, Redis console, Add key, saved queries, Keys counter, tree/list toggle, reload
- [x] Scan `POST {c}/redis/scan {db, pattern, cursor, count≤1000, type, single:true, exact}`; scanLimit per pass, "Next N"; plain text without `*` scans all ("Continue the search"); cancel stale scans; "~X of Y keys scanned"; exact match first
- [x] Tree by ":" with counts; single-key folder shows the key; open state saved; type badge + TTL dot; click / Ctrl / middle-click
- [x] Key tab `POST {c}/redis/key {db,key,limit:200,filter,desc,offset|cursor}`; "Load 200 more"
- [x] Header: type, key, copy, rename, TTL chip, length, memory, encoding, reload, delete
- [x] Edits `POST {c}/redis/edit {db,key,op,args}`: string set (Ctrl+S, binary/truncated read-only, JSON pane for JSON text); json_set; hash hset/hdel + filter; list lpush/rpush/lset/lrem_index; set sadd/srem + filter; zset zadd/zrem + sort; stream xadd/xdel, groups, load older; double-click edits; "N of M shown"
- [x] TTL `expire {ttl|''}`; Rename `rename {newKey}`; Delete (confirm)
- [x] Create key form (type-specific first value, TTL; json when the module is loaded)
- [x] Unsupported type: message + "Open console"
- [x] Console `POST {c}/redis/command {db, command}` → redis-cli style output; Enter runs, Shift+Enter newline; history (300); Ctrl+L / clear; Ctrl+S save; saved commands; examples; 500-entry log
- [x] Empty overview: version, mode, memory, clients, uptime, modules, keys per db

## Settings
- [x] Themes: light/dark following the system by default; more color themes later

## Updates
- [x] Next has its own update channel (pre-releases `next-v*`); check, download, restart

## Keyboard
- [x] Ctrl+S (table changes / SQL save); SQL: Ctrl+Enter, Shift+Ctrl+Enter, Shift+Alt+F, Ctrl+S, Tab; grid keys; dialogs Esc / Enter; Redis console keys

## Toasts / dialogs
- [x] Toasts info/success/error with an action; confirm dialogs for destructive actions; copy to clipboard toast

## Look (parity with static/style.css)
Checked with paired screenshots of the stable UI and Next on the same test data (dark and light themes).
- [x] Theme tokens: the 8 CSS themes, plus the raw CSS variables (`--dim`, `--accent-soft`, …) as a palette (`themes/gen_themes.py` → `src/palette.json`)
- [x] Fonts: 14 px base, Cascadia Code for data, SQL and URLs; 6 / 10 px radii
- [x] Icons: the stable line icons (`assets/gen_icons.py` → `src/icons.rs`, served as `rv/<name>.svg`), the brand symbol in the sidebar footer
- [x] SQL highlighting colors in the editor (the `highlight` section of each theme) and in read-only SQL (`src/sqlhl.rs`, the same token classes as `sqlTokens`)
- [x] Sidebar: connection switcher, schema select, search with icon, SQL console / Schema rows, `TABLES` section, counts, footer (brand, version, theme)
- [x] Tabs: icons, accent top line, close on hover, `+`; the tab bar shows with no tabs
- [x] Table: toolbar (title + read-only badge, Data / Structure segment, Filters / Columns / Refresh / Export, Add record), filter panel, grid (row numbers, name + type header, key / link icons, monospace cells, value colors, sort marks), footer (columns count or pending changes, page size, range, arrows)
- [x] Row details: the right side panel (`.drawer`) instead of a dialog
- [x] Structure view: the stable tables (Columns, Indexes, Foreign keys, Referenced by)
- [x] SQL tab: toolbar (Prettify, split Run button with kbd hints, Save, History, shortcuts), result bar (Table / JSON segment, rows · ms, Columns / Copy / CSV / JSON)
- [x] Schema tab: segment with icons, Copy schema split button, 24 px grid background, list panel, cards with fields, sections, highlighted definition rows with hover actions, collapsible DDL
- [x] Redis: database select, type filter in the search, solid type badges, folder tree, key header chips, console log and input
- [x] Dialogs: `.modal` surface; connection form layout (Name + Connect with, Database type segment, SSL select, URL box, dashed tag presets); DDL forms (check rows, SQL preview box, notes); cell editor (Formatted / Raw, Valid JSON, enum list)
- [~] Cell editor opens as a dialog in the middle (stable: a popover at the cell)
- [~] Redis hash / list grids show row numbers and no per-row action icons (stable: copy / edit / delete icons on each row)
