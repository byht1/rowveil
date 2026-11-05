Rowveil 0.4.4 fixes the desktop app: the theme and other UI settings now stay after a restart.

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

- **Fix: UI settings after a restart (desktop app).** Earlier, the desktop app lost the theme, the open tabs, the SQL history, the tag filter and the start page option at each launch. Now the app keeps them in `ui-prefs.json` in the data folder. Settings that were lost before do not come back: choose the theme once more after the update.
- **From 0.4.3.**
  - A start page with all connections, search and a tag filter. **All connections** in the connection menu goes back to it.
  - Drag the edge of a column header in SQL results to change the column width.
  - Windows: no terminal window flashes when Rowveil checks for updates.
- **Updates.** Rowveil 0.4.0 – 0.4.3 offer this version through the **Update** button in the sidebar footer.

## Where your data is stored

| System | Folder |
|---|---|
| Windows | `%APPDATA%\rowveil\` |
| macOS | `~/Library/Application Support/rowveil/` |
| Linux | `~/.config/rowveil/` |

The folder holds `connections.json` with your saved connections, `saved-queries.json` with your saved queries, and `ui-prefs.json` with the UI settings of the desktop app. Passwords are stored as plain text inside the connection URLs. It also holds `window.json`, `rowveil.log` and the `webview/` cache.

## Notes

- **The builds are not code-signed yet.**
  - Windows SmartScreen: choose *More info → Run anyway*.
  - macOS: right-click the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/Rowveil.app`.
- **WebView2.** The Windows app needs the WebView2 runtime, which is preinstalled on Windows 10/11.
