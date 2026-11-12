**Rowveil Next** is an alpha of the new desktop UI, built with [GPUI](https://www.gpui.rs/) instead of a web view. It is a separate program: it does not replace or update the stable Rowveil.

## Why

- Starts in about 0.3 s to a drawn window (stable: about 0.5 s).
- One process and about 100 MB of memory (stable: 7 processes with WebView2, about 430 MB).

## Download

`rowveil-next-windows-x86_64.exe`: a single file, no installation (Windows 10/11). Updates come from later `next-v*` pre-releases (footer → version).

## What works

- The start page, adding and editing connections, tags (the same `connections.json` as the stable app).
- PostgreSQL, MySQL/MariaDB, SQLite, BigQuery: table data with filters, sorting, columns, paging, export, editing (add, change, delete rows), row details, structure.
- SQL tabs: highlighting, suggestions, Run / Run all / Explain, Prettify, history, saved queries, several results, CSV / JSON.
- Schema tab: tables, views, enums, roles, policies; DDL with a preview.
- Redis: key tree, all value types, TTL, rename, delete, console.
- The 8 themes of the stable app, the same look and icons.

## Not yet

- The cell editor opens as a dialog, not next to the cell.
- macOS and Linux builds.

Data editing, DDL and Redis changes are new code: try them on test databases first. The builds are not code-signed: in Windows SmartScreen choose *More info → Run anyway*.
