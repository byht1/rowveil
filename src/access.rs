//! Write access to SQLite files that are write-protected for the current OS user.
//!
//! On Windows a file under e.g. `C:\ProgramData` is often readable but not writable for
//! `BUILTIN\Users`. [`grant_write`] asks Windows once (UAC prompt) to give the current user
//! "Modify" on exactly the files that block writes. The change is persistent, so the prompt
//! does not come back.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Paths that must become writable before SQLite can change `db`:
/// the database file, existing `-wal` / `-shm` / `-journal` files, and the directory
/// (SQLite creates its journal next to the database).
pub fn write_blockers(db: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let writable = |p: &Path| std::fs::OpenOptions::new().write(true).open(p).is_ok();
    if !writable(db) {
        out.push(db.to_path_buf());
    }
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut s = db.as_os_str().to_owned();
        s.push(suffix);
        let p = PathBuf::from(s);
        if p.is_file() && !writable(&p) {
            out.push(p);
        }
    }
    if let Some(dir) = db.parent() {
        if !dir_allows_new_files(dir) {
            out.push(dir.to_path_buf());
        }
    }
    out
}

fn dir_allows_new_files(dir: &Path) -> bool {
    let probe = dir.join(format!(".querynest-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(f) => {
            drop(f);
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(e) => e.kind() == std::io::ErrorKind::AlreadyExists,
    }
}

/// Whether this build can ask the OS for write access.
pub const CAN_GRANT: bool = cfg!(windows);

/// Gives the current user write access to `paths`. Shows one UAC prompt on Windows.
#[cfg(windows)]
pub async fn grant_write(paths: &[PathBuf]) -> Result<()> {
    use tokio::process::Command;

    if paths.is_empty() {
        return Ok(());
    }
    // Resolve the SID in the non-elevated process: the elevated process can run under a
    // different admin account, and the grant must go to the user who runs QueryNest.
    // Full path: a `whoami` from Git Bash / MSYS on PATH does not understand these flags.
    let system32 = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
    let out = Command::new(system32.join("whoami.exe"))
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .await
        .context("whoami failed")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let sid = text
        .trim()
        .rsplit(',')
        .next()
        .map(|s| s.trim_matches('"').to_string())
        .filter(|s| s.starts_with("S-1-"))
        .context("cannot determine the current user SID")?;

    let mut script = String::from("@echo off\r\n");
    for p in paths {
        let path = p.display().to_string().replace('%', "%%");
        // Directory: only "create files" (WD). File: Modify (M).
        let rights = if p.is_dir() { "(WD)" } else { "(M)" };
        script.push_str(&format!("icacls \"{path}\" /grant *{sid}:{rights} /C /Q >nul || exit /b 1\r\n"));
    }
    script.push_str("exit /b 0\r\n");
    let script_path = std::env::temp_dir().join(format!("querynest-grant-{}.cmd", uuid::Uuid::new_v4()));
    tokio::fs::write(&script_path, script).await.context("cannot write the helper script")?;

    let ps = format!(
        "try {{ $p = Start-Process -FilePath \"$env:SystemRoot\\System32\\cmd.exe\" -ArgumentList '/c','\"{}\"' \
         -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}",
        script_path.display().to_string().replace('\'', "''")
    );
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .status()
        .await
        .context("cannot start PowerShell");
    let _ = tokio::fs::remove_file(&script_path).await;
    match status?.code() {
        Some(0) => Ok(()),
        Some(1223) => bail!("The Windows permission request was cancelled"),
        Some(code) => bail!("icacls failed (exit code {code})"),
        None => bail!("icacls was terminated"),
    }
}

#[cfg(not(windows))]
pub async fn grant_write(_paths: &[PathBuf]) -> Result<()> {
    bail!("Granting access from the UI is supported on Windows only. Change the file owner or mode (chown / chmod) instead")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_file_has_no_blockers() {
        let dir = std::env::temp_dir().join(format!("rs-access-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("ok.db");
        std::fs::write(&db, b"").unwrap();
        assert!(write_blockers(&db).is_empty());
        let mut perms = std::fs::metadata(&db).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&db, perms.clone()).unwrap();
        assert_eq!(write_blockers(&db), vec![db.clone()]);
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&db, perms).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
