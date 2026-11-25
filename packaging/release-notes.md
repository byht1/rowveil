Rowveil 0.5.2 lets you **copy results in the Redis console**. It also has the fixes of 0.5.1 and everything from 0.5.0, the **new desktop app**.

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

## What's new in 0.5.2

- **Redis console: copy a result.** In 0.5.x, the text of a result could not be selected. Each command now has a copy button next to its duration. Right-click a command for **Copy result**, **Copy command** or **Copy command and result**.
- **Updated UI library** (gpui-kit 0.11). No visible changes are expected.

## Fixed in 0.5.1

- **SQL autocomplete: Tab accepts the suggestion again.** In 0.5.0, Tab inserted an indent and closed the list. Now Tab inserts the selected suggestion, as Enter does. Without the list, Tab indents as before.
- **JSON: Collapse and Expand are back.** 0.5.0 did not have them. In the cell dialog and for a Redis JSON value, Collapse folds every block inside the top-level value, so the top-level keys stay visible. Expand opens all blocks.

## New in 0.5.0

The desktop app is drawn on the GPU with [GPUI](https://www.gpui.rs/) instead of a web view: it starts faster and uses much less memory. The features and the look stay the same.

- **A native desktop app.** One process instead of a web view with its helper processes. On Windows: the window is ready in about 0.3 s (0.4.7: about 0.5 s), and the app uses about 100 MB of memory (0.4.7: about 430 MB).
- **The same features**: tables with filters, sorting, editing and export; the SQL console; the Schema tab; Redis; BigQuery; saved queries; the 8 themes.
- **No WebView2 / WebKitGTK.** On Linux the app needs Vulkan instead (see the table above).
- **Updates.** Rowveil 0.4.x and 0.5.x offer this version through the **Update** button in the sidebar footer.

## Before you update from 0.4.x

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
