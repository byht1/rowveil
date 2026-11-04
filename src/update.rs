//! Updates: checks GitHub releases, downloads the build for this install, verifies its
//! SHA-256 and replaces the app after it exits.
//!
//! How an install is updated ([`Method`]):
//! - Windows, installed by the MSI: `msiexec /i … /passive` (one UAC prompt), then the app starts again.
//! - Windows, portable `rowveil.exe` in a writable folder: the file is swapped.
//! - macOS: the `.app` bundle is replaced from the dmg.
//! - Linux, portable tar.gz in a writable folder: the binaries are swapped.
//! - Anything else (a .deb in /usr/bin, the web server): a link to the release page.

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const FEED: &str = "https://api.github.com/repos/byht1/rowveil/releases/latest";
const CHECK_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Msi,
    Portable,
    App,
    Tarball,
    Manual,
    /// Web server / Docker: show the news only.
    None,
}

impl Method {
    fn asset(self) -> Option<&'static str> {
        match self {
            Method::Msi => Some("Rowveil-Setup-x64.msi"),
            Method::Portable => Some("rowveil.exe"),
            Method::App => Some("Rowveil-macOS-universal.dmg"),
            Method::Tarball => Some("rowveil-linux-x86_64.tar.gz"),
            Method::Manual | Method::None => None,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub name: String,
    pub notes: String,
    pub url: String,
    pub published_at: Option<String>,
    #[serde(skip)]
    assets: Vec<(String, String, u64)>,
}

#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// idle | downloading | ready | restarting | error
    pub phase: String,
    pub version: Option<String>,
    pub received: u64,
    pub total: u64,
    pub error: Option<String>,
}

struct Inner {
    checked: Option<(Instant, Release)>,
    progress: Progress,
    file: Option<PathBuf>,
}

pub struct Updater {
    http: reqwest::Client,
    feed: String,
    desktop: bool,
    data_dir: PathBuf,
    inner: Mutex<Inner>,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// `1.10.0` > `1.9.3`; a pre-release (`1.0.0-rc1`) is older than the release.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(v: &str) -> (Vec<u64>, bool) {
        let v = v.trim().trim_start_matches(['v', 'V']);
        let (main, pre) = v.split_once('-').map(|(m, p)| (m, !p.is_empty())).unwrap_or((v, false));
        (main.split('.').map(|x| x.parse().unwrap_or(0)).collect(), pre)
    }
    let (a, a_pre) = parse(candidate);
    let (b, b_pre) = parse(current);
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    !a_pre && b_pre
}

impl Updater {
    pub fn new(desktop: bool, data_dir: PathBuf) -> Arc<Updater> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("rowveil/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("http client");
        let feed = std::env::var("ROWVEIL_UPDATE_FEED").ok().filter(|f| !f.is_empty()).unwrap_or_else(|| FEED.to_string());
        if desktop {
            cleanup_leftovers();
        }
        Arc::new(Updater {
            http,
            feed,
            desktop,
            data_dir,
            inner: Mutex::new(Inner { checked: None, progress: Progress { phase: "idle".into(), ..Default::default() }, file: None }),
        })
    }

    pub fn method(&self) -> Method {
        if !self.desktop {
            return Method::None;
        }
        detect_method()
    }

    /// The latest release (cached for 30 minutes unless `force`).
    pub async fn check(&self, force: bool) -> Result<Value> {
        let release = {
            let inner = self.inner.lock().await;
            inner.checked.as_ref().filter(|(at, _)| !force && at.elapsed() < CHECK_TTL).map(|(_, r)| r.clone())
        };
        let release = match release {
            Some(r) => r,
            None => {
                let r = self.fetch_release().await?;
                self.inner.lock().await.checked = Some((Instant::now(), r.clone()));
                r
            }
        };
        let method = self.method();
        let available = is_newer(&release.version, current_version());
        let asset = method.asset().and_then(|a| release.assets.iter().find(|x| x.0 == a));
        Ok(serde_json::json!({
            "current": current_version(),
            "latest": release.version,
            "available": available,
            "release": release,
            "method": method,
            "canInstall": available && asset.is_some(),
            "size": asset.map(|a| a.2),
        }))
    }

    async fn fetch_release(&self) -> Result<Release> {
        let r = self
            .http
            .get(&self.feed)
            .header("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .context("Cannot reach GitHub to check for updates")?;
        if !r.status().is_success() {
            bail!("GitHub answered {} while checking for updates", r.status());
        }
        let v: Value = r.json().await.context("Unexpected answer from GitHub")?;
        let tag = v["tag_name"].as_str().ok_or_else(|| anyhow!("The latest release has no tag"))?;
        let assets = v["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| {
                Some((a["name"].as_str()?.to_string(), a["browser_download_url"].as_str()?.to_string(), a["size"].as_u64().unwrap_or(0)))
            })
            .collect();
        Ok(Release {
            version: tag.trim_start_matches(['v', 'V']).to_string(),
            name: v["name"].as_str().unwrap_or(tag).to_string(),
            notes: v["body"].as_str().unwrap_or("").to_string(),
            url: v["html_url"].as_str().unwrap_or("https://github.com/byht1/rowveil/releases").to_string(),
            published_at: v["published_at"].as_str().map(str::to_string),
            assets,
        })
    }

    pub async fn progress(&self) -> Progress {
        self.inner.lock().await.progress.clone()
    }

    /// Starts the download in the background. Poll [`Updater::progress`].
    pub async fn download(self: &Arc<Self>) -> Result<()> {
        if !self.desktop {
            bail!("Updates are installed by the desktop app only");
        }
        let method = self.method();
        let asset_name = method.asset().ok_or_else(|| anyhow!("This install cannot update itself. Download the new version from the release page"))?;
        let release = match self.inner.lock().await.checked.clone() {
            Some((_, r)) => r,
            None => self.fetch_release().await?,
        };
        if !is_newer(&release.version, current_version()) {
            bail!("Rowveil {} is the latest version", current_version());
        }
        let (_, url, size) = release.assets.iter().find(|a| a.0 == asset_name).cloned().ok_or_else(|| anyhow!("The release has no {asset_name}"))?;
        let sums_url = release.assets.iter().find(|a| a.0 == "SHA256SUMS.txt").map(|a| a.1.clone());
        {
            let mut inner = self.inner.lock().await;
            if inner.progress.phase == "downloading" {
                return Ok(());
            }
            inner.progress = Progress { phase: "downloading".into(), version: Some(release.version.clone()), received: 0, total: size, error: None };
            inner.file = None;
        }
        let me = self.clone();
        let version = release.version.clone();
        tokio::spawn(async move {
            let result = me.fetch_asset(asset_name, &url, sums_url.as_deref(), &version).await;
            let mut inner = me.inner.lock().await;
            match result {
                Ok(path) => {
                    inner.progress.phase = "ready".into();
                    inner.file = Some(path);
                }
                Err(e) => {
                    tracing::warn!("update download failed: {e:#}");
                    inner.progress.phase = "error".into();
                    inner.progress.error = Some(format!("{e:#}"));
                }
            }
        });
        Ok(())
    }

    async fn fetch_asset(&self, name: &str, url: &str, sums_url: Option<&str>, version: &str) -> Result<PathBuf> {
        let expected = match sums_url {
            Some(u) => {
                let text = self.http.get(u).timeout(Duration::from_secs(30)).send().await?.error_for_status()?.text().await?;
                text.lines()
                    .filter_map(|l| l.split_once(char::is_whitespace))
                    .find(|(_, n)| n.trim().trim_start_matches('*') == name)
                    .map(|(h, _)| h.trim().to_ascii_lowercase())
            }
            None => None,
        };
        let expected = expected.ok_or_else(|| anyhow!("The release has no checksum for {name}; the update was not installed"))?;
        let dir = update_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{version}-{name}"));
        let mut resp = self.http.get(url).timeout(Duration::from_secs(1800)).send().await?.error_for_status().context("Download failed")?;
        let mut file = tokio::fs::File::create(&path).await.with_context(|| format!("Cannot write {}", path.display()))?;
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        use tokio::io::AsyncWriteExt;
        while let Some(chunk) = resp.chunk().await.context("Download interrupted")? {
            ctx.update(&chunk);
            file.write_all(&chunk).await?;
            self.inner.lock().await.progress.received += chunk.len() as u64;
        }
        file.flush().await?;
        drop(file);
        let actual: String = ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
        if actual != expected {
            let _ = std::fs::remove_file(&path);
            bail!("Checksum mismatch for {name}: the download is damaged. Try again");
        }
        Ok(path)
    }

    /// Prepares the swap, starts a helper that waits for this process to exit, installs and
    /// starts the new version. The caller must exit the process soon after.
    pub async fn restart(&self) -> Result<()> {
        let file = {
            let inner = self.inner.lock().await;
            if inner.progress.phase != "ready" {
                bail!("The update is not downloaded yet");
            }
            inner.file.clone().ok_or_else(|| anyhow!("The update file is missing"))?
        };
        let log = self.data_dir.join("update.log");
        install(self.method(), &file, &log)?;
        self.inner.lock().await.progress.phase = "restarting".into();
        Ok(())
    }
}

fn update_dir() -> PathBuf {
    std::env::temp_dir().join("rowveil-update")
}

/// Removes the previous binary (Windows keeps it as `rowveil.exe.old` until the next start)
/// and old downloads.
fn cleanup_leftovers() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("exe.old"));
        let _ = std::fs::remove_file(exe.with_file_name(".rowveil.old"));
    }
    let _ = std::fs::remove_dir_all(update_dir());
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".rowveil-write-test-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
fn detect_method() -> Method {
    let Ok(exe) = std::env::current_exe() else { return Method::Manual };
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    // The MSI records its folder: HKLM\Software\Rowveil InstallDir.
    let msi_dir = std::process::Command::new(r"C:\Windows\System32\reg.exe")
        .args(["query", r"HKLM\Software\Rowveil", "/v", "InstallDir"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().find_map(|l| l.split("REG_SZ").nth(1).map(|v| PathBuf::from(v.trim()))));
    let same = |a: &Path, b: &Path| {
        let n = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_ascii_lowercase();
        n(a) == n(b)
    };
    if msi_dir.as_deref().is_some_and(|m| same(m, &dir)) {
        return Method::Msi;
    }
    if dir_writable(&dir) {
        return Method::Portable;
    }
    // Program Files without the registry value: an MSI install as well.
    Method::Msi
}

#[cfg(target_os = "macos")]
fn detect_method() -> Method {
    match app_bundle() {
        Some(app) if app.parent().is_some_and(dir_writable) => Method::App,
        _ => Method::Manual,
    }
}

#[cfg(target_os = "macos")]
fn app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn detect_method() -> Method {
    let Ok(exe) = std::env::current_exe() else { return Method::Manual };
    if !cfg!(target_arch = "x86_64") || exe.starts_with("/usr") {
        return Method::Manual;
    }
    match exe.parent() {
        Some(dir) if dir_writable(dir) => Method::Tarball,
        _ => Method::Manual,
    }
}

#[cfg(windows)]
fn install(method: Method, file: &Path, log: &Path) -> Result<()> {
    let exe = std::env::current_exe()?;
    let pid = std::process::id();
    let q = |p: &Path| p.display().to_string().replace('\'', "''");
    let body = match method {
        Method::Msi => format!(
            "$p = Start-Process -FilePath \"$env:SystemRoot\\System32\\msiexec.exe\" -ArgumentList '/i \"{msi}\" /passive /norestart' -Wait -PassThru\n\
             Add-Content -LiteralPath '{log}' \"$(Get-Date -Format s) msiexec exit code $($p.ExitCode)\"\n",
            msi = q(file),
            log = q(log)
        ),
        Method::Portable => {
            // A running exe can be renamed, so swap now; the helper only restarts the app.
            let old = exe.with_extension("exe.old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&exe, &old).context("Cannot move the running rowveil.exe aside")?;
            if let Err(e) = std::fs::copy(file, &exe) {
                let _ = std::fs::rename(&old, &exe);
                return Err(e).context("Cannot write the new rowveil.exe");
            }
            String::new()
        }
        _ => bail!("Unsupported update method"),
    };
    // The app starts again with the same arguments (for example a SQLite file).
    let args: Vec<String> = std::env::args().skip(1).map(|a| format!("\"{}\"", a.replace('"', "\\\""))).collect();
    let start = if args.is_empty() {
        format!("Start-Process -FilePath '{}'", q(&exe))
    } else {
        format!("Start-Process -FilePath '{}' -ArgumentList '{}'", q(&exe), args.join(" ").replace('\'', "''"))
    };
    let script = format!(
        "$ErrorActionPreference = 'Continue'\n\
         Add-Content -LiteralPath '{log}' \"$(Get-Date -Format s) updater started ({method:?})\"\n\
         Wait-Process -Id {pid} -Timeout 60 -ErrorAction SilentlyContinue\n\
         {body}\
         try {{ {start} -ErrorAction Stop }} catch {{ Add-Content -LiteralPath '{log}' \"$(Get-Date -Format s) restart failed: $_\" }}\n",
        log = q(log)
    );
    let path = update_dir().join("update.ps1");
    std::fs::create_dir_all(update_dir())?;
    // A BOM makes Windows PowerShell read the script as UTF-8 (paths may contain non-ASCII).
    std::fs::write(&path, format!("\u{feff}{script}"))?;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    // Leave the app's job object (if any), so the helper survives when the app exits.
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let spawn = |flags: u32| {
        std::process::Command::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    let base = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    spawn(base | CREATE_BREAKAWAY_FROM_JOB).or_else(|_| spawn(base)).context("Cannot start the updater")?;
    Ok(())
}

#[cfg(unix)]
fn install(method: Method, file: &Path, log: &Path) -> Result<()> {
    let pid = std::process::id();
    let q = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', r"'\''"));
    let wait = format!("while kill -0 {pid} 2>/dev/null; do sleep 0.3; done\n");
    let script = match method {
        #[cfg(target_os = "macos")]
        Method::App => {
            let app = app_bundle().ok_or_else(|| anyhow!("Rowveil does not run from an .app bundle"))?;
            let new = app.with_extension("app.new");
            format!(
                "{wait}M=$(mktemp -d)\n\
                 if hdiutil attach -nobrowse -readonly -mountpoint \"$M\" {dmg} >/dev/null; then\n\
                   SRC=$(ls -d \"$M\"/*.app | head -1)\n\
                   rm -rf {new} && ditto \"$SRC\" {new} && rm -rf {app} && mv {new} {app} || echo \"$(date) replace failed\" >> {log}\n\
                   hdiutil detach \"$M\" -quiet\n\
                 else echo \"$(date) cannot mount the dmg\" >> {log}; fi\n\
                 open {app}\n",
                dmg = q(file),
                new = q(&new),
                app = q(&app),
                log = q(log)
            )
        }
        Method::Tarball => {
            let exe = std::env::current_exe()?;
            let dir = exe.parent().ok_or_else(|| anyhow!("No folder for the binary"))?.to_path_buf();
            let tmp = update_dir().join("unpacked");
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&tmp)?;
            let ok = std::process::Command::new("tar").arg("-xzf").arg(file).arg("-C").arg(&tmp).status()?.success();
            if !ok {
                bail!("Cannot unpack the update");
            }
            // Swap each binary with a rename in the same folder (the running file stays valid).
            for name in ["rowveil", "rowveil-server"] {
                let Some(src) = std::fs::read_dir(&tmp)?
                    .flatten()
                    .map(|e| e.path().join(name))
                    .find(|p| p.is_file())
                else {
                    continue;
                };
                let dst = dir.join(name);
                if name != "rowveil" && !dst.exists() {
                    continue;
                }
                let staged = dir.join(format!(".{name}.new"));
                std::fs::copy(&src, &staged)?;
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
                std::fs::rename(&staged, &dst)?;
            }
            let args: Vec<String> = std::env::args().skip(1).map(|a| q(Path::new(&a))).collect();
            format!("{wait}exec {exe} {args}\n", exe = q(&exe), args = args.join(" "))
        }
        _ => bail!("Unsupported update method"),
    };
    let path = update_dir().join("update.sh");
    std::fs::create_dir_all(update_dir())?;
    std::fs::write(&path, script)?;
    std::process::Command::new("/bin/sh")
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Cannot start the updater")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.4.0", "0.3.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "1.0.0-rc1"));
        assert!(!is_newer("1.0.0-rc1", "1.0.0"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(is_newer("0.3.1", "0.3"));
    }

    #[test]
    fn assets_per_method() {
        assert_eq!(Method::Msi.asset(), Some("Rowveil-Setup-x64.msi"));
        assert_eq!(Method::None.asset(), None);
    }
}
