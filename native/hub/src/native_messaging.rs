//! Named Pipe server for browser extension communication via Native Messaging.
//!
//! Architecture:
//!   - FluxDown main process creates a Named Pipe server at `\\.\pipe\fluxdown`.
//!   - The NMH relay binary (`fluxdown_nmh.exe`) connects to this pipe.
//!   - Messages use a 4-byte LE length prefix + JSON payload.
//!
//! Message protocol:
//!   - `{"action":"ping","msg_id":N}`     → `{"success":true,"message":"pong","msg_id":N}`
//!   - `{"action":"download","msg_id":N, ...}` → `{"success":true,"message":"download accepted","msg_id":N}`

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Named Pipe path for the NMH relay to connect to.
#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\fluxdown";

/// Maximum message size: 1 MB.
const MAX_MESSAGE_SIZE: u32 = 1024 * 1024;

// ---------------------------------------------------------------------------
// Message types matching the browser extension protocol
// ---------------------------------------------------------------------------

/// 浏览器原始请求体（form POST / XHR raw body 等）。
///
/// 当用户在 form-submit 触发的下载中点击下载按钮时，浏览器实际发起的是
/// POST 请求并携带表单数据；扩展通过 `webRequest.onBeforeRequest` 抓到 method
/// 与 body 后透传到此字段。Rust 端按 `kind` 重建请求体。
///
/// 协议字段：
/// - `formData`：来自 `requestBody.formData`，Rust 端用 `reqwest::form()` 编码为
///   `application/x-www-form-urlencoded`
/// - `urlencoded`：扩展端已序列化好的 url-encoded 字符串（直接作为 body 发送）
/// - `raw`：base64 编码的二进制 body（XHR / fetch 直接发送 ArrayBuffer 的场景）
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RequestBody {
    FormData {
        fields: std::collections::HashMap<String, Vec<String>>,
    },
    Urlencoded {
        raw: String,
    },
    Raw {
        #[serde(rename = "bytesB64")]
        bytes_b64: String,
        #[serde(rename = "contentType", default)]
        content_type: Option<String>,
    },
}

/// Download request payload from the browser extension.
#[derive(Debug, Clone, Deserialize)]
pub struct DownloadRequest {
    pub url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub referrer: String,
    #[serde(default)]
    pub cookies: String,
    /// 浏览器请求中捕获的额外 HTTP 头（如 Authorization）。
    /// 由 Rust 下载引擎在发起请求时附加到请求头中。
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
    /// 文件大小提示（字节）。
    ///   - `>0` = 已知大小，跳过 probe
    ///   - `-1` = 大小未知但确认是下载资源（webRequest 嗅探），跳过 probe
    ///   - `0` / `None` = 正常 probe
    #[serde(rename = "fileSize")]
    #[serde(default)]
    pub file_size: Option<i64>,
    #[serde(rename = "mimeType")]
    #[serde(default)]
    pub mime_type: Option<String>,
    /// 浏览器原始请求方法（"GET" / "POST" / ...）。
    /// 缺省 = "GET"。POST/PUT/PATCH 类请求由 `body` 携带请求体。
    /// 这是修复 uupdump.net 等 form-POST 触发下载场景的关键字段——
    /// 没有它，FluxDown 会用 GET 重发拿到 HTML 页面而非真实文件。
    #[serde(default)]
    pub method: Option<String>,
    /// 浏览器原始请求体（仅在非 GET 时有意义）。
    #[serde(default)]
    pub body: Option<RequestBody>,
}

/// Incoming pipe message with action routing.
#[derive(Debug, Deserialize)]
struct PipeMessage {
    action: String,
    #[serde(default)]
    msg_id: u64,
    #[serde(flatten)]
    payload: serde_json::Value,
}

/// JSON response sent back via the pipe.
#[derive(Debug, Serialize)]
struct PipeResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    msg_id: u64,
}

// ---------------------------------------------------------------------------
// Named Pipe server (Windows)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod server {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ServerOptions;
    use tokio::sync::mpsc;

    use super::{DownloadRequest, MAX_MESSAGE_SIZE, PIPE_NAME, PipeMessage, PipeResponse};
    use crate::logger::log_info;

    /// Read a 4-byte LE length-prefixed message from the pipe.
    async fn read_framed_message(
        pipe: &mut tokio::net::windows::named_pipe::NamedPipeServer,
    ) -> Result<Vec<u8>, std::io::Error> {
        let mut len_buf = [0u8; 4];
        pipe.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf);
        if len == 0 || len > MAX_MESSAGE_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid message length: {}", len),
            ));
        }
        let mut buf = vec![0u8; len as usize];
        pipe.read_exact(&mut buf).await?;
        Ok(buf)
    }

    /// Write a 4-byte LE length-prefixed message to the pipe.
    async fn write_framed_message(
        pipe: &mut tokio::net::windows::named_pipe::NamedPipeServer,
        data: &[u8],
    ) -> Result<(), std::io::Error> {
        let len = data.len() as u32;
        pipe.write_all(&len.to_le_bytes()).await?;
        pipe.write_all(data).await?;
        pipe.flush().await?;
        Ok(())
    }

    /// Handle a single pipe client connection.
    async fn handle_pipe_client(
        mut pipe: tokio::net::windows::named_pipe::NamedPipeServer,
        tx: mpsc::Sender<DownloadRequest>,
    ) {
        loop {
            let raw = match read_framed_message(&mut pipe).await {
                Ok(data) => data,
                Err(e) => {
                    log_info!("[nmh-pipe] read error: {}", e);
                    break;
                }
            };

            let msg: PipeMessage = match serde_json::from_slice(&raw) {
                Ok(m) => m,
                Err(e) => {
                    log_info!("[nmh-pipe] JSON parse error: {}", e);
                    let resp = PipeResponse {
                        success: false,
                        message: Some(format!("invalid JSON: {}", e)),
                        msg_id: 0,
                    };
                    if let Ok(json) = serde_json::to_vec(&resp)
                        && write_framed_message(&mut pipe, &json).await.is_err()
                    {
                        break;
                    }
                    continue;
                }
            };

            let response = match msg.action.as_str() {
                "ping" => {
                    log_info!("[nmh-pipe] ping (msg_id={})", msg.msg_id);
                    PipeResponse {
                        success: true,
                        message: Some("pong".to_string()),
                        msg_id: msg.msg_id,
                    }
                }
                "download" => match serde_json::from_value::<DownloadRequest>(msg.payload) {
                    Ok(download_req) => {
                        log_info!(
                            "[nmh-pipe] download request (msg_id={}): url={}",
                            msg.msg_id,
                            download_req.url
                        );
                        let _ = tx.send(download_req).await;
                        PipeResponse {
                            success: true,
                            message: Some("download accepted".to_string()),
                            msg_id: msg.msg_id,
                        }
                    }
                    Err(e) => {
                        log_info!(
                            "[nmh-pipe] download parse error (msg_id={}): {}",
                            msg.msg_id,
                            e
                        );
                        PipeResponse {
                            success: false,
                            message: Some(format!("invalid download payload: {}", e)),
                            msg_id: msg.msg_id,
                        }
                    }
                },
                other => {
                    log_info!(
                        "[nmh-pipe] unknown action '{}' (msg_id={})",
                        other,
                        msg.msg_id
                    );
                    PipeResponse {
                        success: false,
                        message: Some(format!("unknown action: {}", other)),
                        msg_id: msg.msg_id,
                    }
                }
            };

            if let Ok(json) = serde_json::to_vec(&response)
                && write_framed_message(&mut pipe, &json).await.is_err()
            {
                break;
            }
        }
    }

    /// Spawn the Named Pipe server, feeding download requests into `tx`.
    /// The receiving end is owned by `download_actor` and shared with the
    /// local HTTP takeover server so both transports converge on one channel.
    pub fn spawn_listener_with(tx: mpsc::Sender<DownloadRequest>) {
        tokio::spawn(async move {
            log_info!("[nmh-pipe] starting Named Pipe server at {}", PIPE_NAME);

            // Create the first server instance before entering the loop.
            let mut server = match ServerOptions::new()
                .first_pipe_instance(true)
                .create(PIPE_NAME)
            {
                Ok(s) => s,
                Err(e) => {
                    log_info!("[nmh-pipe] failed to create pipe server: {}", e);
                    return;
                }
            };

            loop {
                // Wait for a client to connect.
                if let Err(e) = server.connect().await {
                    log_info!("[nmh-pipe] connect error: {}", e);
                    // Brief pause before retrying.
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                }

                log_info!("[nmh-pipe] client connected");

                // Create the next server instance to accept the next client
                // while we handle the current one.
                let next_server = match ServerOptions::new().create(PIPE_NAME) {
                    Ok(s) => s,
                    Err(e) => {
                        log_info!("[nmh-pipe] failed to create next pipe instance: {}", e);
                        // Can't accept more clients, but handle the current one.
                        let tx_clone = tx.clone();
                        tokio::spawn(handle_pipe_client(server, tx_clone));
                        // Exit the accept loop — single client mode until restart.
                        break;
                    }
                };

                // Hand off the connected server to a task.
                let connected = std::mem::replace(&mut server, next_server);
                let tx_clone = tx.clone();
                tokio::spawn(handle_pipe_client(connected, tx_clone));
            }
        });
    }
}

// Non-Windows: Unix Domain Socket server.
#[cfg(not(windows))]
mod server {
    use std::os::unix::fs::PermissionsExt;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;
    use tokio::sync::mpsc;

    use super::{DownloadRequest, MAX_MESSAGE_SIZE, PipeMessage, PipeResponse};
    use crate::logger::log_info;

    /// Returns the current user's home directory.
    ///
    /// Prefers `$HOME` but falls back to the passwd database via `getpwuid_r`
    /// so that the correct path is returned even when the process is launched
    /// by a system service (launchd on macOS) that may not set `$HOME`.
    #[cfg(target_os = "macos")]
    fn home_dir() -> Option<std::path::PathBuf> {
        if let Ok(h) = std::env::var("HOME") {
            if !h.is_empty() {
                return Some(std::path::PathBuf::from(h));
            }
        }
        use std::ffi::CStr;
        let uid = unsafe { libc::getuid() };
        let buf_size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
        let buf_size = if buf_size > 0 {
            buf_size as usize
        } else {
            1024
        };
        let mut buf = vec![0i8; buf_size];
        let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let ret = unsafe {
            libc::getpwuid_r(
                uid,
                pwd.as_mut_ptr(),
                buf.as_mut_ptr(),
                buf_size,
                &mut result,
            )
        };
        if ret == 0 && !result.is_null() {
            let pwd = unsafe { pwd.assume_init() };
            if !pwd.pw_dir.is_null() {
                let cstr = unsafe { CStr::from_ptr(pwd.pw_dir) };
                if let Ok(s) = cstr.to_str() {
                    if !s.is_empty() {
                        return Some(std::path::PathBuf::from(s));
                    }
                }
            }
        }
        None
    }

    /// Returns the Unix socket path for the NMH relay to connect to.
    ///
    /// - macOS: `~/Library/Application Support/fluxdown/fluxdown.sock`
    ///   (avoids /tmp sandbox isolation and $TMPDIR per-app randomisation;
    ///    uses getpwuid fallback so launchd-launched NMH also finds it)
    /// - Linux:  `~/.local/share/fluxdown/fluxdown.sock`
    ///   (avoids $XDG_RUNTIME_DIR sandbox remapping inside Flatpak/Snap;
    ///    ~/.local/share/ is bind-mounted into the sandbox so both the host
    ///    app and the browser-spawned NMH see the same path)
    /// - Other Unix: `$XDG_RUNTIME_DIR/fluxdown.sock` → `/tmp/fluxdown.sock`
    pub fn socket_path() -> std::path::PathBuf {
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = home_dir() {
                let dir = home
                    .join("Library")
                    .join("Application Support")
                    .join("fluxdown");
                let _ = std::fs::create_dir_all(&dir);
                return dir.join("fluxdown.sock");
            }
        }
        // Linux: use ~/.local/share/fluxdown/fluxdown.sock
        // This path is accessible from both the host (app process) and Flatpak/Snap
        // sandboxes (which bind-mount ~/.local/share/ into the sandbox), unlike
        // $XDG_RUNTIME_DIR which gets remapped to a sandbox-private path inside
        // Flatpak, causing the app and NMH to see different socket paths.
        #[cfg(target_os = "linux")]
        {
            if let Ok(home) = std::env::var("HOME") {
                if !home.is_empty() {
                    let dir = std::path::Path::new(&home)
                        .join(".local")
                        .join("share")
                        .join("fluxdown");
                    let _ = std::fs::create_dir_all(&dir);
                    return dir.join("fluxdown.sock");
                }
            }
        }
        // Fallback for any other Unix-like OS
        if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
            std::path::Path::new(&dir).join("fluxdown.sock")
        } else {
            std::path::Path::new("/tmp").join("fluxdown.sock")
        }
    }

    async fn read_framed_message(
        stream: &mut tokio::net::UnixStream,
    ) -> Result<Vec<u8>, std::io::Error> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf);
        if len == 0 || len > MAX_MESSAGE_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid message length: {}", len),
            ));
        }
        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf).await?;
        Ok(buf)
    }

    async fn write_framed_message(
        stream: &mut tokio::net::UnixStream,
        data: &[u8],
    ) -> Result<(), std::io::Error> {
        let len = data.len() as u32;
        stream.write_all(&len.to_le_bytes()).await?;
        stream.write_all(data).await?;
        stream.flush().await?;
        Ok(())
    }

    async fn handle_client(mut stream: tokio::net::UnixStream, tx: mpsc::Sender<DownloadRequest>) {
        loop {
            let raw = match read_framed_message(&mut stream).await {
                Ok(data) => data,
                Err(e) => {
                    log_info!("[nmh-uds] read error: {}", e);
                    break;
                }
            };

            let msg: PipeMessage = match serde_json::from_slice(&raw) {
                Ok(m) => m,
                Err(e) => {
                    log_info!("[nmh-uds] JSON parse error: {}", e);
                    let resp = PipeResponse {
                        success: false,
                        message: Some(format!("invalid JSON: {}", e)),
                        msg_id: 0,
                    };
                    if let Ok(json) = serde_json::to_vec(&resp)
                        && write_framed_message(&mut stream, &json).await.is_err()
                    {
                        break;
                    }
                    continue;
                }
            };

            let response = match msg.action.as_str() {
                "ping" => {
                    log_info!("[nmh-uds] ping (msg_id={})", msg.msg_id);
                    PipeResponse {
                        success: true,
                        message: Some("pong".to_string()),
                        msg_id: msg.msg_id,
                    }
                }
                "download" => match serde_json::from_value::<DownloadRequest>(msg.payload) {
                    Ok(download_req) => {
                        log_info!(
                            "[nmh-uds] download request (msg_id={}): url={}",
                            msg.msg_id,
                            download_req.url
                        );
                        let _ = tx.send(download_req).await;
                        PipeResponse {
                            success: true,
                            message: Some("download accepted".to_string()),
                            msg_id: msg.msg_id,
                        }
                    }
                    Err(e) => {
                        log_info!(
                            "[nmh-uds] download parse error (msg_id={}): {}",
                            msg.msg_id,
                            e
                        );
                        PipeResponse {
                            success: false,
                            message: Some(format!("invalid download payload: {}", e)),
                            msg_id: msg.msg_id,
                        }
                    }
                },
                other => {
                    log_info!(
                        "[nmh-uds] unknown action '{}' (msg_id={})",
                        other,
                        msg.msg_id
                    );
                    PipeResponse {
                        success: false,
                        message: Some(format!("unknown action: {}", other)),
                        msg_id: msg.msg_id,
                    }
                }
            };

            if let Ok(json) = serde_json::to_vec(&response)
                && write_framed_message(&mut stream, &json).await.is_err()
            {
                break;
            }
        }
    }

    pub fn spawn_listener_with(tx: mpsc::Sender<DownloadRequest>) {
        let sock_path = socket_path();

        tokio::spawn(async move {
            // Remove stale socket file left by a previous run.
            let _ = std::fs::remove_file(&sock_path);

            let listener = match UnixListener::bind(&sock_path) {
                Ok(l) => {
                    log_info!(
                        "[nmh-uds] Unix socket server started at {}",
                        sock_path.display()
                    );
                    l
                }
                Err(e) => {
                    log_info!("[nmh-uds] failed to bind Unix socket: {}", e);
                    return;
                }
            };

            // Restrict socket to owner-only so other local users cannot connect.
            if let Err(e) =
                std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600))
            {
                log_info!("[nmh-uds] failed to set socket permissions: {}", e);
            }

            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        log_info!("[nmh-uds] client connected");
                        let tx_clone = tx.clone();
                        tokio::spawn(handle_client(stream, tx_clone));
                    }
                    Err(e) => {
                        log_info!("[nmh-uds] accept error: {}", e);
                    }
                }
            }
        });
    }
}

/// Spawn the Native Messaging listener, feeding requests into the provided
/// sender. Used by `download_actor` so that both the NMH transport and the
/// local HTTP takeover server can push `DownloadRequest`s into one shared
/// channel — the actor's `native_msg_rx` branch then handles both uniformly.
pub fn spawn_native_messaging_listener_with(tx: mpsc::Sender<DownloadRequest>) {
    server::spawn_listener_with(tx);
}

/// Spawn the Native Messaging listener and return a fresh receiver.
///
/// Convenience wrapper for callers that don't need to share the channel.
/// Ping requests are handled internally (immediate pong response).
#[allow(dead_code)]
pub fn spawn_native_messaging_listener() -> mpsc::Receiver<DownloadRequest> {
    let (tx, rx) = mpsc::channel::<DownloadRequest>(64);
    server::spawn_listener_with(tx);
    rx
}

#[cfg(test)]
mod tests {
    use super::DownloadRequest;

    #[test]
    fn deserialize_download_request_with_headers() {
        let json = r#"{
            "url": "https://example.com/file.zip",
            "filename": "file.zip",
            "referrer": "https://example.com/",
            "cookies": "session=abc123",
            "headers": {
                "Authorization": "Bearer token123",
                "X-Custom": "value"
            },
            "fileSize": 1024,
            "mimeType": "application/zip"
        }"#;
        let req: DownloadRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.url, "https://example.com/file.zip");
        assert_eq!(req.filename, "file.zip");
        assert_eq!(req.referrer, "https://example.com/");
        assert_eq!(req.cookies, "session=abc123");
        let headers = req.headers.unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer token123");
        assert_eq!(headers.get("X-Custom").unwrap(), "value");
        assert_eq!(req.file_size, Some(1024));
        assert_eq!(req.mime_type.as_deref(), Some("application/zip"));
    }

    #[test]
    fn deserialize_download_request_without_headers() {
        let json = r#"{
            "url": "https://example.com/file.zip"
        }"#;
        let req: DownloadRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.url, "https://example.com/file.zip");
        assert!(req.headers.is_none());
        assert_eq!(req.cookies, "");
        assert_eq!(req.referrer, "");
    }

    #[test]
    fn deserialize_download_request_empty_headers() {
        let json = r#"{
            "url": "https://example.com/file.zip",
            "headers": {}
        }"#;
        let req: DownloadRequest = serde_json::from_str(json).unwrap();
        let headers = req.headers.unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn deserialize_download_request_skip_probe_hint() {
        // fileSize: -1 表示"跳过 probe"（资源面板触发的下载，大小未知但确认可下载）
        let json = r#"{
            "url": "https://example.com/bulletinPDF/abc?u_atoken=xxx",
            "cookies": "session=abc",
            "fileSize": -1
        }"#;
        let req: DownloadRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.file_size, Some(-1));
        assert_eq!(req.cookies, "session=abc");
    }

    #[test]
    fn deserialize_batch_url_with_newlines() {
        // 验证批量下载用换行符拼接的 URL 可以正确反序列化
        let json = r#"{
            "url": "https://a.com/1.zip\nhttps://b.com/2.zip",
            "cookies": "session=abc",
            "referrer": "https://example.com/"
        }"#;
        let req: DownloadRequest = serde_json::from_str(json).unwrap();
        let urls: Vec<&str> = req.url.split('\n').collect();
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://a.com/1.zip");
        assert_eq!(urls[1], "https://b.com/2.zip");
    }
}
