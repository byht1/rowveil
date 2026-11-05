Rowveil 0.4.3 adds a **start page** and **column resizing for SQL results**, and removes the terminal window that flashed on Windows when Rowveil checked for updates.

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

- **Start page.** Rowveil now opens on a page with all your connections, so you choose the database yourself. Earlier, Rowveil opened the first connection in the list.
  - Search the connections by name, address, dialect or tag. `Enter` opens the first match.
  - Filter the connections by tag, as in the connection menu.
  - **All connections** in the connection menu goes back to the start page.
  - Select **Open the last connection on start** to skip the start page.
- **Column width for SQL results.** Drag the right edge of a column header to change the width, as in the table view. The width belongs to the SQL tab and to that set of result columns; it stays after a reload.
- **Fixes.**
  - Windows: a terminal window no longer flashes when you click **Update** in the sidebar footer. The same fix applies to **Grant write access**.
- **Updates.** Rowveil 0.4.0 – 0.4.2 offer this version through the **Update** button in the sidebar footer. In 0.4.2 and earlier, the terminal window can still flash once during this update.

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
