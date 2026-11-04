Rowveil 0.4.1 adds **tags for connections** and fixes **Redis key search**.

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

- **Connection tags.** Mark connections as `LOCAL`, `DEV`, `TEST`, `STAGE`, `PROD` or with your own tags (up to 8).
  - Colored tags in the connection menu, and a tag filter at its top.
  - Edit the name, tags and read-only flag of a saved connection (the ✎ button).
  - A `PROD` connection gets a red line over the work area and `[PROD]` in the window title.
  - Connections from environment variables: `DATABASE_TAGS_<NAME>=DEV,LOCAL`, or set the tags in the UI.
- **Redis key search fixes.**
  - The filter input keeps focus while keys load.
  - Typing a new filter cancels the previous search instead of being ignored.
  - On large databases the search continues by itself until it finds matches or reaches the end; each step scans more keys.
  - A filter that names a key exactly (for example `cron:config`) shows that key at once.
- **Updates.** Rowveil 0.4.0 offers this version through the **Update** button in the sidebar footer.

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
