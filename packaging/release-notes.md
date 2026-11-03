QueryNest is now available for **Windows**, **macOS** and **Linux**. Windows also gets a proper installer.

## Downloads

| System | File | Notes |
|---|---|---|
| **Windows 10/11** | **`QueryNest-Setup-x64.msi`** | Installer. It shows where the program and your data are stored. |
| Windows | `querynest.exe` | Portable: a single file, no installation. |
| **macOS 11+** | **`QueryNest-macOS-universal.dmg`** | Apple Silicon and Intel. |
| **Linux (Debian/Ubuntu)** | **`querynest-linux-amd64.deb`** | `sudo apt install ./querynest-linux-amd64.deb` |
| Linux (other) | `querynest-linux-x86_64.tar.gz` | Needs WebKitGTK 4.1 and GTK 3; see README.txt inside. |
| Server | `querynest-server-*` | The web UI for a browser or a server (Windows, macOS, Linux). |
| | `SHA256SUMS.txt` | Checksums. |

## What's new

- **Windows installer (MSI).**
  - Choose the folder and the components: desktop shortcut, "Open with" for `.db`/`.sqlite` files, and the optional web server.
  - A dedicated page shows where QueryNest keeps its files. The program goes to `C:\Program Files\QueryNest\`, and your data goes to `%APPDATA%\querynest\`.
  - The Finish page can launch the app.
  - Uninstalling keeps your saved connections.
- **macOS app.** It ships as a universal `.app` in a `.dmg`, with a native menu: Cmd+C/V/Z and Quit work as expected.
- **Linux app.** A `.deb` package adds a desktop entry and icons. A portable `.tar.gz` is also available.
- **Native "Save as" dialog** for exports on all three systems.

## Where your data is stored

| System | Folder |
|---|---|
| Windows | `%APPDATA%\querynest\` |
| macOS | `~/Library/Application Support/querynest/` |
| Linux | `~/.config/querynest/` |

The folder holds `connections.json` with your saved connections. Passwords are stored as plain text inside the connection URLs. It also holds `window.json`, `querynest.log` and the `webview/` cache.

## Notes

- **The builds are not code-signed yet.**
  - Windows SmartScreen: choose *More info → Run anyway*.
  - macOS: right-click the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/QueryNest.app`.
- **WebView2.** The Windows app needs the WebView2 runtime, which is preinstalled on Windows 10/11.
