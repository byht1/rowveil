<p align="center"><img src="static/brand/logo-horizontal.svg" width="520" alt="Rowveil — Explore / Query / Understand"></p>

# Rowveil

**English** · [Українська](README.uk.md)

Database explorer & query tool. A standalone desktop app to browse, edit and manage the structure of your databases.
The backend is Rust (axum + sqlx); the UI is vanilla JS embedded in the binary. No ORM or project config is needed.

Supports **PostgreSQL**, **MySQL / MariaDB**, **SQLite**, **Redis / Valkey** and **Google BigQuery**.

## Features

- Several connections: from environment variables and added in the UI (stored in `connections.json`). A start page lists them with search and a tag filter.
- Add a connection with separate fields (database type, host, port, user, password, database, SSL) or with a URL. Special characters in the password are encoded for you.
- Open each connection in its own window (in the browser version, in its own tab).
- Tags on connections: presets `LOCAL`, `DEV`, `TEST`, `STAGE`, `PROD` or your own (up to 8). Colored chips in the connection menu, a tag filter, and edit name / tags / read-only later. A `PROD` connection gets a red line over the work area and `[PROD]` in the window title.
- Tables and views, schema / database picker, search, row estimates.
- Grid: pagination, sorting (Shift+click sorts by several columns), resizable and hideable columns.
- Filters: `=, <>, >, >=, <, <=, like, ilike, not like, not ilike, in, not in, is null, is not null`.
- Edit cells, add and delete rows. Changes are staged and saved in one transaction (`Ctrl+S`).
- Type-aware editors: lists for enum and boolean, `NULL` and `default`.
- JSON (in JSON columns and in plain TEXT): CodeMirror editor with highlighting, line numbers and folding; Formatted / Raw, Collapse / Expand, validation, `Shift+Alt+F` to format. Formatting keeps numbers and key order, and compact JSON stays compact.
- Row panel: all fields, jump along a foreign key, reverse relations ("Referenced by").
- Structure tab: columns, indexes, foreign keys, relations.
- Schema view: Schemas, Tables, Views, Enums, Roles, Policies; Copy schema gives the DDL of the whole database (to the clipboard or to the SQL console).
  - Tables: rename, move to another schema, RLS; add / change / drop a column, constraint, index, policy; create / drop a table.
  - Views: edit the definition, rename, create, drop, refresh a materialized view.
  - Enums, roles (flags, connection limit, password, privileges) and RLS policies for PostgreSQL.
  - Every change shows its SQL and runs only after Apply. PostgreSQL and SQLite apply changes in one transaction; MySQL commits each DDL statement on its own.
  - Dialects: PostgreSQL has all sub-tabs; MySQL has Schemas (databases), Tables, Views; SQLite has Tables, Views (a column can only be renamed or dropped).
- SQL console on CodeMirror 6:
  - a ▶ button next to each statement runs only that statement; `Ctrl+Enter` runs the statement under the cursor or the selection; `Shift+Ctrl+Enter` runs everything;
  - Run menu: "Run all" and "Explain"; Prettify (`Shift+Alt+F`);
  - VS Code shortcuts: `Alt+↑/↓`, `Shift+Alt+↑/↓`, `Ctrl+D`, `Ctrl+Shift+L`, `Ctrl+/`, `Ctrl+Shift+K`, `Ctrl+L`, `Alt+Click`, `Ctrl+F` and more (the keyboard button in the toolbar lists them all);
  - highlighting, autocomplete (tables, columns, aliases, keywords, dialect functions; `Tab` or `Enter` accepts a suggestion), line numbers, folding, history;
  - saved queries (`Ctrl+S`): for one connection or for all connections of the dialect, listed in the sidebar;
  - results as a table or JSON, several result sets, resize and hide columns, copy, CSV / JSON export.
- CSV / JSON export (the current page or all filtered rows, up to 10,000).
- Themes: Rowveil Light and Rowveil Dark (follows the system by default), Graphite, Paper, One Dark Pro, Dracula, Nord, GitHub Light (button at the bottom of the sidebar, preview on hover).
- Read-only mode. Optional Basic Auth.

### Redis

- URL: `redis://[user:password@]host:6379/0`, TLS: `rediss://`; `valkey://` also works. The connection form has host, port, user (ACL, optional), password, database number and TLS.
- Sidebar: database picker (`db0…db15` with key counts), glob filter (`user:*`; plain text matches anywhere in the key), type filter, a tree grouped by `:` or a flat list. Keys load with `SCAN` in batches, so a large Redis does not block.
- View and edit by type: string (text or JSON editor), hash, list, set, sorted set, stream (entries, consumer groups), RedisJSON documents. Large collections load in pages (`HSCAN` / `SSCAN` / `LRANGE` / `ZRANGE`).
- TTL (change or `PERSIST`), rename (`RENAMENX`), delete (`UNLINK`), new keys of any type.
- A redis-cli style console: replies printed like redis-cli, `↑/↓` history, one command per line, saved commands (`Ctrl+S`). A read-only connection runs read commands only.

### BigQuery

- URL: `bigquery://PROJECT[/DATASET]?credentials=PATH&location=EU&maximumBytesBilled=N&endpoint=URL`.
- Credentials, in order: `credentials=` (a service account key or a `gcloud auth application-default login` file), `GOOGLE_APPLICATION_CREDENTIALS`, the gcloud ADC file, the metadata server on Google Cloud. `endpoint=` points to an emulator (no credentials).
- Datasets take the place of schemas; tables, views, materialized views; `STRUCT` and `ARRAY` show as JSON, `TIMESTAMP` in UTC.
- Browsing a table without filters or sorting reads it with `tabledata.list`: no query job and no cost. Filters, sorting and the console run query jobs; the result header shows bytes processed / billed. `Explain` in the console does a dry run and shows how many bytes the query will process.
- `maximumBytesBilled` caps the cost of each query.
- Rows can be edited in tables with a primary key (`NOT ENFORCED`); otherwise use DML in the console. The Schema view shows the DDL of tables and views, and Copy schema gives the DDL of the whole project; change the schema in the console.

## Install

Builds are on the [Releases](https://github.com/byht1/rowveil/releases/latest) page and on the [website](https://byht1.github.io/rowveil/#downloads).

| System | File | How to install |
|---|---|---|
| Windows 10/11 | `Rowveil-Setup-x64.msi` | Installer: folder, shortcuts, "Open with" for `.db` files, a page that shows where data is kept |
| Windows | `rowveil.exe` | Portable: one file, no install |
| macOS 11+ | `Rowveil-macOS-universal.dmg` | Drag Rowveil to Applications. First start: right-click → Open |
| Debian / Ubuntu | `rowveil-linux-amd64.deb` | `sudo apt install ./rowveil-linux-amd64.deb` |
| Other Linux | `rowveil-linux-x86_64.tar.gz` | Needs WebKitGTK 4.1 and GTK 3 (see `README.txt` in the archive) |

Where data is kept (`connections.json`: connections, passwords in URLs as plain text; `window.json`; `rowveil.log`; `webview/`):

| System | Folder |
|---|---|
| Windows | `%APPDATA%\rowveil\` |
| macOS | `~/Library/Application Support/rowveil/` |
| Linux | `~/.config/rowveil/` |

Uninstalling the app keeps this folder.

## Updates

The desktop app checks for new versions on start and every 6 hours. When a new version is out, an **Update** button appears in the sidebar footer:

1. Click **Update** to see the release notes.
2. Click **Download and install**. The file downloads in the background, and Rowveil checks its SHA-256 against `SHA256SUMS.txt` from the release.
3. Click **Restart and update**. Rowveil closes, installs the update and starts again.

| Install | How it updates |
|---|---|
| Windows, MSI | `msiexec /passive`: Windows asks for permission once (UAC). Your feature choices are kept |
| Windows, portable `rowveil.exe` | The file is replaced in place |
| macOS | `Rowveil.app` is replaced from the dmg |
| Linux, tar.gz | `rowveil` and `rowveil-server` are replaced in place |
| Linux `.deb`; `rowveil-server` / Docker | A notice and a link to the release only |

Turn off automatic checks in the updates window (click the version number in the footer). Updates do not touch your data or connections.

## Desktop app

- Its own window with the Rowveil icon, no console and no browser. Engine: WebView2 (Windows), WKWebView (macOS), WebKitGTK (Linux).
- Open each connection in its own window. The main window remembers its size and position.
- CSV / JSON export opens the system "Save as" dialog, then offers "Show in folder".
- Pass a database as an argument: `rowveil C:\data\app.db` or `rowveil postgres://user:pass@host/db`.
- The server inside the app listens on `127.0.0.1` on a random port and accepts requests only from its own windows (a secret token per launch).

### Build from source

```bash
cargo build --release                       # Windows / macOS
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev && cargo build --release   # Linux
```

Packages: `packaging/windows/build-msi.ps1` (WiX 5), `packaging/macos/build-dmg.sh`, `packaging/linux/package.sh`. A `v*` tag runs `.github/workflows/release.yml`: it builds all three systems, tests the installers and publishes the release.

## Browser version and Docker

`rowveil-server` serves the same UI to a browser. Use it for Docker and for access from other machines.

```bash
./target/release/rowveil-server --url postgres://user:pass@localhost:5432/db
# opens http://127.0.0.1:4983
```

```bash
docker build -t rowveil .
docker run -p 4983:4983 -v rowveil-data:/data   -e DATABASE_URL=postgres://user:pass@host.docker.internal:5432/db   --add-host host.docker.internal:host-gateway   rowveil
```

Demo with a PostgreSQL test database:

```bash
docker compose up -d --build
# open http://localhost:4983
```

## Testbed

PostgreSQL, MySQL, SQLite, Redis (Redis Stack with the JSON module) and a BigQuery emulator, with data for every feature (JSON in text columns, big numbers, enums, roles, RLS, awkward names, 5,000 rows and more):

```bash
docker compose -f examples/testbed/docker-compose.yml up -d --build
# open http://localhost:4984
```

Details and connection URLs: [examples/testbed/README.md](examples/testbed/README.md) (in Ukrainian).

## Configuration

| Variable / flag | Description | Default |
|---|---|---|
| `DATABASE_URL` / `--url` | Main connection (desktop: an argument) | — |
| `DATABASE_URL_<NAME>` | More connections (named `<name>`) | — |
| `DATABASE_TAGS_<NAME>` | Tags of that connection: `DEV,LOCAL` (`DATABASE_TAGS` for `DATABASE_URL`). Tags changed in the UI are kept in `env-connection-tags.json` | — |
| `STUDIO_HOST` / `--host` | Address (`rowveil-server` only) | `127.0.0.1` (Docker: `0.0.0.0`) |
| `STUDIO_PORT` / `--port` | Port (`rowveil-server` only) | `4983` |
| `STUDIO_DATA_DIR` / `--data-dir` | Folder for `connections.json` | `%APPDATA%\rowveil` (Docker: `/data`) |
| `STUDIO_NO_OPEN` / `--no-open` | Do not open a browser (`rowveil-server` only) | `false` |
| `STUDIO_READ_ONLY` / `--read-only` | Block data changes | `false` |
| `STUDIO_PASSWORD` / `--password` | Turn on Basic Auth | — |
| `STUDIO_USER` / `--user` | Basic Auth user | `admin` |

URL formats:

```
postgres://user:pass@host:5432/db
mysql://user:pass@host:3306/db      (or mariadb://)
sqlite://./data.db                  (or a path to a .db / .sqlite file)
redis://:pass@host:6379/0           (TLS: rediss://, also valkey://)
bigquery://project/dataset?credentials=C:\key.json&location=EU
```

## Limitations

- A SQLite file that the current user cannot write opens read-only. On Windows the table panel shows "Grant write access": Windows asks for permission once (UAC), and `icacls` gives your user Modify rights on that file only. After that you can edit the file without admin rights, in other programs too. On Linux / macOS change the file owner or mode (`chown` / `chmod`).
- Only tables with a primary key can be edited. Tables without a PK and views open read-only.
- In read-only mode the SQL console runs only queries that read data (`SELECT`, `WITH`, `SHOW`, `EXPLAIN`, `PRAGMA`). The check is keyword-based; for full protection use a database user with read-only rights.
- The console returns up to 5,000 rows per result set.
- Redis Cluster and Sentinel are not supported. Binary Redis values are read-only in the editors (change them in the console).

## Development

```bash
cargo test
cargo run -- examples/testbed/testbed.db                                   # desktop
cargo run --bin rowveil-server -- --url sqlite://examples/testbed/testbed.db  # browser
```

Layout: `src/lib.rs` is the server and API, `src/main.rs` is `rowveil-server`, `src/bin/desktop.rs` is the desktop window (tao + wry), `src/kv.rs` is Redis, `src/bigquery.rs` is BigQuery, `src/update.rs` is the updater, `build.rs` adds the icon and `.exe` metadata. Without the desktop (Linux / Docker): `cargo build --no-default-features --bin rowveil-server`.

`ROWVEIL_UPDATE_FEED` points the updater at another releases feed (for tests).

Brand: `assets/brand/` (colors, guide), `assets/rowveil.ico`, `static/brand/`.

In a debug build the files in `static/` are read from disk, so UI changes show after a page reload.

`static/vendor/codemirror.js` is a prebuilt bundle of CodeMirror 6 and sql-formatter; `cargo build` does not need Node.js. To rebuild the bundle:

```bash
cd ui-vendor
npm install
npm run build
```

Test data: `examples/testbed/` (see above) and short seeds in `examples/*-seed.sql`.

## License

[MIT](LICENSE)
