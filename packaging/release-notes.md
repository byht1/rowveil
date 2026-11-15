Rowveil 0.5.0 has a **new desktop app**. It is drawn on the GPU with [GPUI](https://www.gpui.rs/) instead of a web view: it starts faster and uses much less memory. The features and the look stay the same.

## Downloads

| System | File | Notes |
|---|---|---|
| **Windows 10/11** | **`Rowveil-Setup-x64.msi`** | Installer. It shows where the program and your data are stored. |
| Windows | `rowveil.exe` | Portable: a single file, no installation. |
| **macOS 11+** | **`Rowveil-macOS-universal.dmg`** | Apple Silicon and Intel. |
| **Linux (Debian/Ubuntu)** | **`rowveil-linux-amd64.deb`** | `sudo apt install ./rowveil-linux-amd64.deb` |
| Linux (other) | `rowveil-linux-x86_64.tar.gz` | Needs Vulkan, xkbcommon and fontconfig; see README.txt inside. |
| Server | `rowveil-server-*` | The web UI for a browser or a server (Windows, macOS, Linux). |
| | `SHA256SUMS.txt` | Checksums. |

## What's new

- **A native desktop app.** One process instead of a web view with its helper processes. On Windows: the window is ready in about 0.3 s (0.4.7: about 0.5 s), and the app uses about 100 MB of memory (0.4.7: about 430 MB).
- **The same features**: tables with filters, sorting, editing and export; the SQL console; the Schema tab; Redis; BigQuery; saved queries; the 8 themes.
- **No WebView2 / WebKitGTK.** On Linux the app needs Vulkan instead (see the table above).
- **Updates.** Rowveil 0.4.x offers this version through the **Update** button in the sidebar footer.

## Before you update

- Your connections and saved queries stay. The desktop app starts with the default theme and no open tabs: its settings are now in `desktop-prefs.json`.
- The cell editor opens as a dialog in the middle of the window (0.4.x: next to the cell).
- One window per app: "Open in a new window" is not available yet.
- The browser version (`rowveil-server`, Docker) has not changed.

## Where your data is stored

| System | Folder |
|---|---|
| Windows | `%APPDATA%\rowveil\` |
| macOS | `~/Library/Application Support/rowveil/` |
| Linux | `~/.config/rowveil/` |

The folder holds `connections.json` with your saved connections, `saved-queries.json` with your saved queries, and `desktop-prefs.json` with the settings of the desktop app. Passwords are stored as plain text inside the connection URLs.

## Notes

- **The builds are not code-signed yet.**
  - Windows SmartScreen: choose *More info → Run anyway*.
  - macOS: right-click the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/Rowveil.app`.
