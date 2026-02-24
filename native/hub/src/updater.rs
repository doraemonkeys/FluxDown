//! Auto-update module: version check via website API proxy, download update
//! package, and launch installation.
//!
//! Platform strategies:
//!   Windows setup  (.exe)         → Inno Setup silent install
//!   Windows portable (.zip)       → bat script: wait, extract, copy, restart
//!   Linux AppImage (.AppImage)    → sh script: wait, mv replace, chmod, restart
//!   Linux deb (.deb)              → pkexec dpkg -i  (GUI password dialog)
//!   Linux arch (.pkg.tar.zst)     → pkexec pacman -U (GUI password dialog)
//!   Linux portable (.tar.gz)      → sh script: wait, tar extract, copy, restart
//!
//! All HTTP requests go through the website API (`/api/release`, `/api/download/:fn`)
//! so that GITHUB_TOKEN stays server-side — the client never touches GitHub directly.

#[cfg(target_os = "windows")]
use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use rinf::RustSignal;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use crate::signals::{UpdateCheckResult, UpdateDownloadProgress};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const UPDATE_API_BASE: &str = "https://fluxdown.zerx.dev";

#[cfg(target_os = "windows")]
const PORTABLE_MARKER: &str = "portable";

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("semver error: {0}")]
    Semver(String),
    #[error("{0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// API response types (matching website /api/release)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ReleaseInfo {
    version: String,
    published_at: String,
    assets: ReleaseAssets,
}

#[derive(Deserialize)]
struct ReleaseAssets {
    // Windows (unused on Linux but must be present for serde deserialization)
    #[allow(dead_code)]
    setup: Option<AssetInfo>,
    #[allow(dead_code)]
    portable: Option<AssetInfo>,
    #[allow(dead_code)]
    setup_arm64: Option<AssetInfo>,
    #[allow(dead_code)]
    portable_arm64: Option<AssetInfo>,
    // Linux (unused on Windows but must be present for serde deserialization)
    #[allow(dead_code)]
    linux_appimage: Option<AssetInfo>,
    #[allow(dead_code)]
    linux_deb: Option<AssetInfo>,
    #[allow(dead_code)]
    linux_arch: Option<AssetInfo>,
    #[allow(dead_code)]
    linux_tarball: Option<AssetInfo>,
}

#[derive(Deserialize)]
struct AssetInfo {
    #[allow(dead_code)]
    name: String,
    size: i64,
    download_url: String,
}

// ---------------------------------------------------------------------------
// Environment detection
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn is_portable() -> bool {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        return dir.join(PORTABLE_MARKER).exists();
    }
    false
}

#[cfg(target_os = "windows")]
fn is_arm64() -> bool {
    std::env::consts::ARCH == "aarch64"
}

/// Linux installation type detected at runtime.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
enum LinuxInstallType {
    /// Running as an AppImage ($APPIMAGE env is set by the AppImage runtime).
    AppImage,
    /// Installed via .deb package to /opt/fluxdown/ (dpkg can locate the exe).
    Deb,
    /// Installed via .pkg.tar.zst to /opt/fluxdown/ (pacman can locate the exe).
    Arch,
    /// Extracted tar.gz in any user-writable directory.
    Portable,
}

/// Detect how FluxDown was installed on this Linux system.
#[cfg(target_os = "linux")]
fn detect_linux_install_type() -> LinuxInstallType {
    // 1. AppImage: the AppImage runtime always sets $APPIMAGE to the path of
    //    the squashfs image being executed.
    if std::env::var("APPIMAGE").is_ok() {
        return LinuxInstallType::AppImage;
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return LinuxInstallType::Portable,
    };
    let exe_str = exe.to_str().unwrap_or("");

    // 2. System package: both deb and arch install to /opt/fluxdown/.
    if exe_str.starts_with("/opt/fluxdown") {
        // Try dpkg first (Debian/Ubuntu).
        let dpkg_found = std::process::Command::new("dpkg")
            .args(["-S", exe_str])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if dpkg_found {
            return LinuxInstallType::Deb;
        }

        // Try pacman (Arch Linux).
        let pacman_found = std::process::Command::new("pacman")
            .args(["-Qo", exe_str])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if pacman_found {
            return LinuxInstallType::Arch;
        }
    }

    // 3. Fallback: portable tar.gz extracted to a user directory.
    LinuxInstallType::Portable
}

fn select_asset(assets: &ReleaseAssets) -> Option<&AssetInfo> {
    #[cfg(target_os = "windows")]
    {
        match (is_portable(), is_arm64()) {
            (true, true) => assets.portable_arm64.as_ref(),
            (true, false) => assets.portable.as_ref(),
            (false, true) => assets.setup_arm64.as_ref(),
            (false, false) => assets.setup.as_ref(),
        }
    }

    #[cfg(target_os = "linux")]
    {
        match detect_linux_install_type() {
            LinuxInstallType::AppImage => assets.linux_appimage.as_ref(),
            LinuxInstallType::Deb => assets.linux_deb.as_ref(),
            LinuxInstallType::Arch => assets.linux_arch.as_ref(),
            LinuxInstallType::Portable => assets.linux_tarball.as_ref(),
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// Simple semver comparison (major.minor.patch only)
// ---------------------------------------------------------------------------

fn parse_semver(s: &str) -> Result<(u64, u64, u64), UpdateError> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return Err(UpdateError::Semver(format!("invalid version: {s}")));
    }
    let major = parts[0]
        .parse::<u64>()
        .map_err(|_| UpdateError::Semver(format!("invalid major: {}", parts[0])))?;
    let minor = parts[1]
        .parse::<u64>()
        .map_err(|_| UpdateError::Semver(format!("invalid minor: {}", parts[1])))?;
    let patch = parts[2]
        .parse::<u64>()
        .map_err(|_| UpdateError::Semver(format!("invalid patch: {}", parts[2])))?;
    Ok((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> Result<bool, UpdateError> {
    let (lmaj, lmin, lpat) = parse_semver(latest)?;
    let (cmaj, cmin, cpat) = parse_semver(current)?;
    Ok((lmaj, lmin, lpat) > (cmaj, cmin, cpat))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Check for updates by querying the website API proxy.
/// Sends `UpdateCheckResult` signal back to Dart.
pub async fn check(current_version: &str) {
    let result = check_inner(current_version).await;
    match result {
        Ok(()) => {} // signal already sent inside check_inner
        Err(e) => {
            UpdateCheckResult {
                has_update: false,
                latest_version: String::new(),
                current_version: current_version.to_string(),
                download_url: String::new(),
                file_size: 0,
                published_at: String::new(),
                error_message: e.to_string(),
            }
            .send_signal_to_dart();
        }
    }
}

async fn check_inner(current_version: &str) -> Result<(), UpdateError> {
    let client = Client::new();
    let url = format!("{UPDATE_API_BASE}/api/release");

    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(15))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(UpdateError::Other(format!(
            "API returned status {}",
            resp.status()
        )));
    }

    let release: ReleaseInfo = resp.json().await?;
    let has_update = is_newer(&release.version, current_version).unwrap_or(false);

    let (download_url, file_size) = match select_asset(&release.assets) {
        Some(asset) => {
            let full_url = if asset.download_url.starts_with('/') {
                format!("{UPDATE_API_BASE}{}", asset.download_url)
            } else {
                asset.download_url.clone()
            };
            (full_url, asset.size)
        }
        None => (String::new(), 0),
    };

    UpdateCheckResult {
        has_update,
        latest_version: release.version,
        current_version: current_version.to_string(),
        download_url,
        file_size,
        published_at: release.published_at,
        error_message: String::new(),
    }
    .send_signal_to_dart();

    Ok(())
}

/// Download the update installer to a temp directory.
/// Sends periodic `UpdateDownloadProgress` signals to Dart.
pub async fn download(url: &str, version: &str) {
    let result = download_inner(url, version).await;
    if let Err(e) = result {
        UpdateDownloadProgress {
            version: version.to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            speed: 0,
            status: 2, // error
            installer_path: String::new(),
            error_message: e.to_string(),
        }
        .send_signal_to_dart();
    }
}

async fn download_inner(url: &str, version: &str) -> Result<(), UpdateError> {
    let client = Client::new();

    let resp = client
        .get(url)
        .timeout(Duration::from_secs(600)) // 10 min max for large installer
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(UpdateError::Other(format!(
            "Download returned status {}",
            resp.status()
        )));
    }

    let total_bytes = resp.content_length().unwrap_or(0) as i64;
    let file_name = url
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("FluxDown-update");
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join(file_name);

    let mut file = tokio::fs::File::create(&file_path).await?;
    let mut stream = resp.bytes_stream();

    let mut downloaded: i64 = 0;
    let mut last_report = std::time::Instant::now();
    let mut last_downloaded_for_speed: i64 = 0;
    let mut last_speed_time = std::time::Instant::now();
    let report_interval = Duration::from_millis(200);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as i64;

        let now = std::time::Instant::now();
        if now.duration_since(last_report) >= report_interval {
            let elapsed_secs = now.duration_since(last_speed_time).as_secs_f64();
            let speed = if elapsed_secs > 0.0 {
                ((downloaded - last_downloaded_for_speed) as f64 / elapsed_secs) as i64
            } else {
                0
            };
            last_downloaded_for_speed = downloaded;
            last_speed_time = now;

            UpdateDownloadProgress {
                version: version.to_string(),
                downloaded_bytes: downloaded,
                total_bytes,
                speed,
                status: 0, // downloading
                installer_path: String::new(),
                error_message: String::new(),
            }
            .send_signal_to_dart();

            last_report = now;
        }
    }

    file.flush().await?;
    drop(file);

    let installer_path = file_path.to_string_lossy().to_string();

    // Send completion signal
    UpdateDownloadProgress {
        version: version.to_string(),
        downloaded_bytes: downloaded,
        total_bytes,
        speed: 0,
        status: 1, // completed
        installer_path,
        error_message: String::new(),
    }
    .send_signal_to_dart();

    Ok(())
}

/// Install a downloaded update package and restart the application.
///
/// On success the function does not return — it exits the process.
/// On failure it returns an error so the caller can report it to the UI.
pub fn install(installer_path: &str) -> Result<(), UpdateError> {
    #[cfg(target_os = "windows")]
    {
        let path = Path::new(installer_path);
        let is_zip = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));

        if is_zip {
            install_portable(installer_path)
        } else {
            install_setup(installer_path)
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Dispatch based on file extension / suffix.
        if installer_path.ends_with(".AppImage") {
            install_appimage(installer_path)
        } else if installer_path.ends_with(".deb") {
            install_deb(installer_path)
        } else if installer_path.ends_with(".pkg.tar.zst") {
            install_arch(installer_path)
        } else {
            // .tar.gz portable fallback
            install_portable_tarball(installer_path)
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = installer_path;
        Err(UpdateError::Other(
            "Auto-update install is not supported on this platform".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Windows installers
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn install_setup(installer_path: &str) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    std::process::Command::new(installer_path)
        .args(["/SILENT", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(UpdateError::Io)?;

    std::thread::sleep(Duration::from_millis(500));
    std::process::exit(0);
}

/// Portable upgrade: write a bat script that waits for the app to close,
/// extracts the zip over the app directory via PowerShell, then restarts.
#[cfg(target_os = "windows")]
fn install_portable(zip_path: &str) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let exe = std::env::current_exe().map_err(UpdateError::Io)?;
    let app_dir = exe
        .parent()
        .ok_or_else(|| UpdateError::Other("cannot determine app directory".to_string()))?;
    let exe_name = exe
        .file_name()
        .ok_or_else(|| UpdateError::Other("cannot determine exe name".to_string()))?
        .to_string_lossy();

    let script = format!(
        r#"@echo off
chcp 65001 >nul 2>&1
set "ZIP={zip}"
set "DIR={dir}"
set "EXE={exe}"
:loop
timeout /t 1 /nobreak >nul
tasklist /fi "imagename eq %EXE%" 2>nul | find /i "%EXE%" >nul && goto loop
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$tmp = Join-Path $env:TEMP ('fluxdown_upd_' + (Get-Random));" ^
  "Expand-Archive -LiteralPath '%ZIP%' -DestinationPath $tmp -Force;" ^
  "$items = @(Get-ChildItem $tmp);" ^
  "if ($items.Count -eq 1 -and $items[0].PSIsContainer) {{ $src = $items[0].FullName }} else {{ $src = $tmp }};" ^
  "Copy-Item -Path (Join-Path $src '*') -Destination '%DIR%' -Recurse -Force;" ^
  "Remove-Item $tmp -Recurse -Force"
del "%ZIP%" 2>nul
start "" "%DIR%\%EXE%"
(goto) 2>nul & del "%~f0"
"#,
        zip = zip_path,
        dir = app_dir.to_string_lossy(),
        exe = exe_name,
    );

    let script_path = std::env::temp_dir().join("fluxdown_update.bat");
    std::fs::write(&script_path, &script).map_err(UpdateError::Io)?;

    std::process::Command::new("cmd")
        .args(["/c", &script_path.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(UpdateError::Io)?;

    std::thread::sleep(Duration::from_millis(500));
    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// Linux installers
// ---------------------------------------------------------------------------

/// AppImage self-update: write a shell script that waits for this process to
/// exit, then atomically replaces the old AppImage with the new one and
/// relaunches it.  No root privileges required.
#[cfg(target_os = "linux")]
fn install_appimage(new_appimage_path: &str) -> Result<(), UpdateError> {
    // $APPIMAGE is set by the AppImage runtime to the absolute path of the
    // currently-running squashfs image.
    let current_appimage = std::env::var("APPIMAGE").map_err(|_| {
        UpdateError::Other(
            "$APPIMAGE not set; cannot determine the current AppImage path".to_string(),
        )
    })?;

    let pid = std::process::id();

    let script = format!(
        r#"#!/bin/sh
NEW="{new}"
OLD="{old}"
PID={pid}
# Wait for the app process to exit.
while kill -0 "$PID" 2>/dev/null; do
    sleep 1
done
# Replace the AppImage atomically.
chmod +x "$NEW"
mv -f "$NEW" "$OLD"
# Relaunch the updated AppImage detached from this shell.
nohup "$OLD" >/dev/null 2>&1 &
# Self-delete this script.
rm -- "$0"
"#,
        new = new_appimage_path,
        old = current_appimage,
        pid = pid,
    );

    write_and_run_sh_script(&script, "fluxdown_update_appimage.sh")?;

    std::thread::sleep(Duration::from_millis(500));
    std::process::exit(0);
}

/// Deb package update via pkexec: triggers a native GUI password dialog on
/// GNOME/KDE/XFCE through the Polkit authentication agent.
#[cfg(target_os = "linux")]
fn install_deb(deb_path: &str) -> Result<(), UpdateError> {
    check_pkexec_available()?;

    let status = std::process::Command::new("pkexec")
        .args(["dpkg", "-i", deb_path])
        .status()
        .map_err(UpdateError::Io)?;

    if !status.success() {
        // User cancelled pkexec or dpkg failed.
        return Err(UpdateError::Other(format!(
            "dpkg install failed (exit code {}). \
             If you cancelled the password prompt, click Install & Restart to try again.",
            status.code().unwrap_or(-1)
        )));
    }

    // dpkg replaced the binary in-place; restart the new version.
    restart_app()
}

/// Arch package update via pkexec + pacman: triggers a native GUI password
/// dialog through the Polkit authentication agent.
#[cfg(target_os = "linux")]
fn install_arch(pkg_path: &str) -> Result<(), UpdateError> {
    check_pkexec_available()?;

    let status = std::process::Command::new("pkexec")
        .args(["pacman", "-U", "--noconfirm", pkg_path])
        .status()
        .map_err(UpdateError::Io)?;

    if !status.success() {
        return Err(UpdateError::Other(format!(
            "pacman install failed (exit code {}). \
             If you cancelled the password prompt, click Install & Restart to try again.",
            status.code().unwrap_or(-1)
        )));
    }

    restart_app()
}

/// Portable tar.gz self-update: write a shell script that waits for this
/// process to exit, extracts the new archive over the app directory, then
/// relaunches the app.  No root privileges required.
#[cfg(target_os = "linux")]
fn install_portable_tarball(tarball_path: &str) -> Result<(), UpdateError> {
    let exe = std::env::current_exe().map_err(UpdateError::Io)?;
    let app_dir = exe
        .parent()
        .ok_or_else(|| UpdateError::Other("cannot determine app directory".to_string()))?;
    let exe_name = exe
        .file_name()
        .ok_or_else(|| UpdateError::Other("cannot determine exe name".to_string()))?
        .to_string_lossy()
        .into_owned();

    let pid = std::process::id();

    let script = format!(
        r#"#!/bin/sh
TAR="{tar}"
DIR="{dir}"
EXE="{exe}"
PID={pid}
# Wait for the app process to exit.
while kill -0 "$PID" 2>/dev/null; do
    sleep 1
done
# Extract the tarball into a temp directory.
TMP=$(mktemp -d)
tar xzf "$TAR" -C "$TMP"
# Handle single top-level directory (e.g. FluxDown-x.y.z-linux-x64/).
COUNT=$(ls "$TMP" | wc -l)
FIRST=$(ls "$TMP" | head -n 1)
if [ "$COUNT" -eq 1 ] && [ -d "$TMP/$FIRST" ]; then
    SRC="$TMP/$FIRST"
else
    SRC="$TMP"
fi
# Overwrite the app directory.
cp -a "$SRC/." "$DIR/"
# Cleanup.
rm -rf "$TMP"
rm -f "$TAR"
# Relaunch the updated binary detached from this shell.
nohup "$DIR/$EXE" >/dev/null 2>&1 &
# Self-delete this script.
rm -- "$0"
"#,
        tar = tarball_path,
        dir = app_dir.to_string_lossy(),
        exe = exe_name,
        pid = pid,
    );

    write_and_run_sh_script(&script, "fluxdown_update_portable.sh")?;

    std::thread::sleep(Duration::from_millis(500));
    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// Linux helpers
// ---------------------------------------------------------------------------

/// Write `content` to a temp shell script, make it executable, and spawn it
/// detached.  Returns before the script does anything (the script waits for
/// our PID to exit).
#[cfg(target_os = "linux")]
fn write_and_run_sh_script(content: &str, name: &str) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;

    let script_path = std::env::temp_dir().join(name);
    std::fs::write(&script_path, content).map_err(UpdateError::Io)?;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
        .map_err(UpdateError::Io)?;

    std::process::Command::new("sh")
        .arg(&script_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(UpdateError::Io)?;

    Ok(())
}

/// Verify that `pkexec` is available on PATH before attempting a privileged
/// install.  Returns a clear error if it is not found.
#[cfg(target_os = "linux")]
fn check_pkexec_available() -> Result<(), UpdateError> {
    let found = std::process::Command::new("pkexec")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if found {
        Ok(())
    } else {
        Err(UpdateError::Other(
            "pkexec not found. Please install the update manually using your package manager."
                .to_string(),
        ))
    }
}

/// Spawn the current executable path and exit, effectively restarting the app.
#[cfg(target_os = "linux")]
fn restart_app() -> Result<(), UpdateError> {
    let exe = std::env::current_exe().map_err(UpdateError::Io)?;
    std::process::Command::new(&exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(UpdateError::Io)?;
    std::process::exit(0);
}
