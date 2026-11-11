Rowveil 0.4.6 makes the **Redis key list** work on large databases: it scans keys in parts, with a **Next** button.

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

- **Redis: keys load in parts.** One pass scans 10,000 keys, in requests of 1,000 keys. This stops "timed out" errors on a large Redis.
  - Below the list: "~10,000 of 1,234,567 keys scanned" and a **Next 10,000** button that scans the next part.
  - A search (`user:*`, `*order*`) works the same way: one pass, then **Next**.
  - Change the number in **Edit name and tags → Keys per scan** (100 – 1,000,000) for a connection you added.
- **Updates.** Rowveil 0.4.0 – 0.4.5 offer this version through the **Update** button in the sidebar footer.

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
