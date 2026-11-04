Rowveil 0.4.0 adds **Redis** and **Google BigQuery**, and the desktop app now **updates itself**.

## Downloads

| System | File | Notes |
|---|---|---|
| **Windows 10/11** | **`Rowveil-Setup-x64.msi`** | Installer. It shows where the program and your data are stored. |
| Windows | `rowveil.exe` | Portable: a single file, no installation. |
| **macOS 11+** | **`Rowveil-macOS-universal.dmg`** | Apple Silicon and Intel. |
| **Linux (Debian/Ubuntu)** | **`rowveil-linux-amd64.deb`** | `sudo apt install ./rowveil-linux-amd64.deb` |
| Linux (other) | `rowveil-linux-x86_64.tar.gz` | Needs WebKitGTK 4.1 and GTK 3; see README.txt inside. |
| Server | `rowveil-server-*` | The web UI for a browser or a server (Windows, macOS, Linux). |
| | `SHA256SUMS.txt` | Checksums. |

## What's new

- **Automatic updates.** The desktop app checks GitHub for a new version on start and every 6 hours. Click **Update** in the sidebar footer, then **Download and install** and **Restart and update**.
  - The download is checked against `SHA256SUMS.txt`.
  - MSI installs update with `msiexec` (one UAC prompt); the portable `.exe`, the macOS app and the Linux `.tar.gz` replace themselves. A `.deb` install shows a link to the release.
  - Turn off automatic checks in the updates window (click the version number).
  - **Version 0.3.0 has no updater: install 0.4.0 once by hand.** Later versions update themselves.
- **Redis and Valkey** (`redis://`, `rediss://`, `valkey://`).
  - Key browser: database picker with key counts, glob filter, type filter, a tree grouped by `:`; keys load in `SCAN` batches.
  - Editors for strings (text or JSON), hashes, lists, sets, sorted sets, streams (with consumer groups) and RedisJSON documents.
  - TTL, rename, delete, new keys of any type.
  - A redis-cli style console with history. Read-only connections run read commands only.
- **Google BigQuery** (`bigquery://PROJECT/DATASET?credentials=key.json`).
  - Credentials: a service account key, `GOOGLE_APPLICATION_CREDENTIALS`, `gcloud auth application-default login` or the Google Cloud metadata server.
  - Datasets, tables, views; `STRUCT` / `ARRAY` as JSON.
  - Browsing a table without filters is free (no query job). Filtered views and the console show bytes processed and billed; `Explain` does a dry run. `maximumBytesBilled` caps each query.
  - The Schema view shows the DDL; Copy schema exports the whole project.
- **README in English** (Ukrainian: `README.uk.md`).

## Where your data is stored

| System | Folder |
|---|---|
| Windows | `%APPDATA%\rowveil\` |
| macOS | `~/Library/Application Support/rowveil/` |
| Linux | `~/.config/rowveil/` |

The folder holds `connections.json` with your saved connections. Passwords are stored as plain text inside the connection URLs. It also holds `window.json`, `rowveil.log` and the `webview/` cache.

## Notes

- **The builds are not code-signed yet.**
  - Windows SmartScreen: choose *More info → Run anyway*.
  - macOS: right-click the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/Rowveil.app`.
- **WebView2.** The Windows app needs the WebView2 runtime, which is preinstalled on Windows 10/11.
