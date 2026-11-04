Rowveil 0.4.2 adds **saved queries** and **column visibility for SQL results**, and fixes autocomplete, the tag filter and the Redis JSON editor.

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

- **Saved queries.** Keep SQL statements and Redis commands for later.
  - `Ctrl+S` or the **Save** button in the SQL console and in the Redis console.
  - A query belongs to one connection, or to all connections of the same dialect (for DEV / PROD copies of one database).
  - The **Saved queries** section in the sidebar opens a query; hover it to rename or delete it.
  - The queries are stored in `saved-queries.json` next to `connections.json`.
- **Columns for SQL results.** Hide columns of a result with the **Columns** button. Copy, CSV and JSON use the visible columns. The setting belongs to the SQL tab and to that set of result columns.
- **Fixes.**
  - Autocomplete: `Tab` accepts the selected suggestion (the first one by default), as in VS Code. Without suggestions, `Tab` indents.
  - Tag filter in the connection menu: a click shows only that tag (a second click clears the filter); `Ctrl+click` selects several tags; **All** shows all connections. Custom tags follow the preset tags.
  - Redis: the JSON editor of a key fills the full height of the window.
- **Updates.** Rowveil 0.4.0 and 0.4.1 offer this version through the **Update** button in the sidebar footer.

## Where your data is stored

| System | Folder |
|---|---|
| Windows | `%APPDATA%\rowveil\` |
| macOS | `~/Library/Application Support/rowveil/` |
| Linux | `~/.config/rowveil/` |

The folder holds `connections.json` with your saved connections and `saved-queries.json` with your saved queries. Passwords are stored as plain text inside the connection URLs. It also holds `window.json`, `rowveil.log` and the `webview/` cache.

## Notes

- **The builds are not code-signed yet.**
  - Windows SmartScreen: choose *More info → Run anyway*.
  - macOS: right-click the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/Rowveil.app`.
- **WebView2.** The Windows app needs the WebView2 runtime, which is preinstalled on Windows 10/11.
