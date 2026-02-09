use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::HeaderValue;
use reqwest::Client;
use thiserror::Error;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::speed_limiter::SpeedLimiter;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("db error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct FileInfo {
    pub file_name: String,
    pub total_bytes: i64,
    pub supports_range: bool,
}

pub struct ProgressUpdate {
    pub task_id: String,
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub status: i32,
    pub error_message: String,
    /// Non-empty only on initial status=1 update (resolved file name).
    pub file_name: String,
    /// Per-segment progress info (for IDM-style visualization).
    /// `None` for single-thread downloads; `Some(vec)` for multi-segment.
    pub segment_details: Option<Vec<SegmentProgressInfo>>,
}

/// Snapshot of a single segment's progress, sent from downloader to progress_reporter.
#[derive(Clone)]
pub struct SegmentProgressInfo {
    pub index: i32,
    pub start_byte: i64,
    pub end_byte: i64,
    pub downloaded_bytes: i64,
}

pub struct DownloadParams {
    pub task_id: String,
    pub url: String,
    pub save_dir: String,
    pub file_name: String,
    pub segment_count: i32,
    /// When `true`, skip file-name dedup — the file on disk belongs to *this*
    /// task and should be reused, not treated as a naming collision.
    pub is_resume: bool,
    pub db: Db,
    pub client: Client,
    pub progress_tx: mpsc::Sender<ProgressUpdate>,
    pub cancel_token: CancellationToken,
    /// Global speed limiter — shared across all concurrent downloads.
    pub speed_limiter: SpeedLimiter,
}

// ---------------------------------------------------------------------------
// HTTP client builder (shared config)
// ---------------------------------------------------------------------------

/// Build a properly configured HTTP client that mirrors Chrome's capabilities.
pub fn build_client() -> Result<Client, DownloadError> {
    let client = Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/131.0.0.0 Safari/537.36",
        )
        // TLS — reqwest with rustls-tls feature handles HTTPS automatically
        .use_rustls_tls()
        // Redirects — follow up to 30 hops like Chrome
        .redirect(reqwest::redirect::Policy::limited(30))
        // Timeouts
        .connect_timeout(Duration::from_secs(30))
        // No global timeout — downloads can be very long
        // Connection pool — close idle connections after 90s to avoid
        // stale connections, and keep at most 8 idle per host.
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(8)
        // Cookies — needed for session-based downloads (Google Drive, etc.).
        // reqwest follows RFC 6265: cookies are scoped to their domain.
        .cookie_store(true)
        // Do NOT enable auto-decompression (.gzip/.brotli/.deflate).
        // A download manager must receive raw bytes so that:
        //  1. Content-Length matches the actual bytes written to disk.
        //  2. Range-based multi-segment downloads use correct byte offsets.
        //  3. The integrity check (file size vs Content-Length) works reliably.
        //
        // IMPORTANT: The gzip/brotli/deflate Cargo *features* are enabled in
        // Cargo.toml (needed so reqwest links the decompression libs).  Even
        // though we never call `.gzip(true)` etc., reqwest still advertises
        // `Accept-Encoding: gzip, br, deflate` by default once those features
        // are compiled in.  Servers/CDNs (e.g. GitHub raw, Cloudflare) may
        // then respond with compressed content whose Content-Length reflects
        // the *compressed* size, while reqwest transparently decompresses the
        // body — causing a size mismatch at our integrity check.
        //
        // Fix: explicitly set `Accept-Encoding: identity` so the server never
        // sends compressed content and Content-Length always equals raw bytes.
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::ACCEPT_ENCODING,
                HeaderValue::from_static("identity"),
            );
            h
        })
        .build()?;
    Ok(client)
}

// ---------------------------------------------------------------------------
// Resolve file info (HEAD probe → GET fallback)
// ---------------------------------------------------------------------------

/// Timeout for the probe requests (HEAD / GET Range:0-0).
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum retries for the probe phase (HEAD + GET).
const PROBE_MAX_RETRIES: u32 = 3;

/// Delay between probe retries.
const PROBE_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Resolve file info with automatic retry on transient failures.
///
/// On Windows, the very first HTTPS request from a new process can fail due to
/// DNS resolver cold-start, rustls TLS session initialisation, or firewall
/// first-connection inspection.  Retrying transparently hides this from users.
pub async fn resolve_file_info(client: &Client, url: &str) -> Result<FileInfo, DownloadError> {
    let mut last_err = None;
    for attempt in 0..PROBE_MAX_RETRIES {
        match resolve_file_info_once(client, url).await {
            Ok(info) => return Ok(info),
            Err(e) => {
                rinf::debug_print!(
                    "[resolve] probe attempt {}/{} failed: {}",
                    attempt + 1,
                    PROBE_MAX_RETRIES,
                    e
                );
                last_err = Some(e);
                if attempt + 1 < PROBE_MAX_RETRIES {
                    tokio::time::sleep(PROBE_RETRY_DELAY).await;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        DownloadError::Other("probe failed after retries".to_string())
    }))
}

async fn resolve_file_info_once(client: &Client, url: &str) -> Result<FileInfo, DownloadError> {
    // --- Phase 1: HEAD probe for size & range info --------------------------
    let head_ok = client.head(url).timeout(PROBE_TIMEOUT).send().await;

    let (mut headers, mut final_url) = match head_ok {
        Ok(r) if r.status().is_success() => {
            let u = r.url().clone();
            (r.headers().clone(), u)
        }
        _ => {
            // HEAD failed or returned error — we'll rely on GET below.
            (reqwest::header::HeaderMap::new(), reqwest::Url::parse(url)
                .unwrap_or_else(|_| reqwest::Url::parse("http://invalid").unwrap_or_else(|_| unreachable!())))
        }
    };

    // --- Phase 2: GET Range:0-0 for Content-Disposition ---------------------
    // Many servers/CDNs only send Content-Disposition on GET, not HEAD.
    // Also needed when HEAD failed entirely.
    let need_get = !headers.contains_key(reqwest::header::CONTENT_DISPOSITION)
        || headers.is_empty();

    if need_get {
        match client
            .get(url)
            .header("Range", "bytes=0-0")
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                let get_url = r.url().clone();
                let get_headers = r.headers().clone();
                let got_206 = r.status() == reqwest::StatusCode::PARTIAL_CONTENT;
                drop(r); // release connection immediately

                if headers.is_empty() {
                    // HEAD failed — use all GET headers
                    headers = get_headers;
                    final_url = get_url;
                } else {
                    // HEAD succeeded but lacked Content-Disposition; merge.
                    if let Some(cd) = get_headers.get(reqwest::header::CONTENT_DISPOSITION) {
                        headers.insert(reqwest::header::CONTENT_DISPOSITION, cd.clone());
                    }
                    if let Some(ct) = get_headers.get(reqwest::header::CONTENT_TYPE) {
                        // Prefer GET Content-Type (may be more specific)
                        headers.insert(reqwest::header::CONTENT_TYPE, ct.clone());
                    }
                    // Use GET's final URL (may differ after redirect)
                    final_url = get_url;
                    // If GET gave us 206, copy Content-Range
                    if got_206
                        && let Some(cr) = get_headers.get("content-range") {
                            headers.insert(
                                reqwest::header::HeaderName::from_static("content-range"),
                                cr.clone(),
                            );
                        }
                }
            }
            _ => {
                // GET also failed — we'll work with whatever HEAD gave us
                if headers.is_empty() {
                    return Err(DownloadError::Other(
                        "both HEAD and GET probes failed".to_string(),
                    ));
                }
            }
        }
    }

    // --- Phase 3: Parse metadata from merged headers ------------------------
    let supports_range = headers
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v != "none");

    let total_bytes = if let Some(cr) = headers.get("content-range") {
        // e.g. "bytes 0-0/12345"
        cr.to_str()
            .ok()
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    } else {
        headers
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    };

    let file_name = extract_filename(&headers, final_url.as_str());
    rinf::debug_print!(
        "[resolve] url={} → name={}, size={}, range={}",
        url, file_name, total_bytes, supports_range
    );

    Ok(FileInfo {
        file_name,
        total_bytes,
        supports_range,
    })
}

// ---------------------------------------------------------------------------
// File-name extraction
// ---------------------------------------------------------------------------

/// MIME type → common extension mapping for when there is no filename.
fn mime_to_ext(content_type: &str) -> Option<&'static str> {
    let ct = content_type.split(';').next().unwrap_or("").trim();
    match ct {
        "application/pdf" => Some("pdf"),
        "application/zip" => Some("zip"),
        "application/x-gzip" | "application/gzip" => Some("gz"),
        "application/x-tar" => Some("tar"),
        "application/x-bzip2" => Some("bz2"),
        "application/x-xz" => Some("xz"),
        "application/x-7z-compressed" => Some("7z"),
        "application/x-rar-compressed" | "application/vnd.rar" => Some("rar"),
        "application/json" => Some("json"),
        "application/xml" | "text/xml" => Some("xml"),
        "application/javascript" | "text/javascript" => Some("js"),
        "application/wasm" => Some("wasm"),
        "application/octet-stream" => None, // generic binary
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
            Some("pptx")
        }
        "application/msword" => Some("doc"),
        "application/vnd.ms-excel" => Some("xls"),
        "application/vnd.ms-powerpoint" => Some("ppt"),
        "application/x-iso9660-image" => Some("iso"),
        "application/x-msdownload" | "application/x-dosexec" => Some("exe"),
        "application/vnd.android.package-archive" => Some("apk"),
        "application/java-archive" => Some("jar"),
        "application/x-shockwave-flash" => Some("swf"),
        "application/x-debian-package" => Some("deb"),
        "application/x-rpm" => Some("rpm"),
        "application/x-msi" => Some("msi"),
        "application/vnd.apple.installer+xml" => Some("pkg"),
        "text/html" => Some("html"),
        "text/css" => Some("css"),
        "text/csv" => Some("csv"),
        "text/plain" => Some("txt"),
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/bmp" => Some("bmp"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("ico"),
        "image/tiff" => Some("tiff"),
        "image/avif" => Some("avif"),
        "audio/mpeg" => Some("mp3"),
        "audio/ogg" => Some("ogg"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/flac" => Some("flac"),
        "audio/aac" => Some("aac"),
        "audio/mp4" | "audio/x-m4a" => Some("m4a"),
        "audio/webm" => Some("weba"),
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/x-matroska" => Some("mkv"),
        "video/x-msvideo" => Some("avi"),
        "video/quicktime" => Some("mov"),
        "video/x-flv" => Some("flv"),
        "video/mp2t" => Some("ts"),
        "video/3gpp" => Some("3gp"),
        "font/woff" => Some("woff"),
        "font/woff2" => Some("woff2"),
        "font/ttf" | "application/x-font-ttf" => Some("ttf"),
        "font/otf" => Some("otf"),
        _ => None,
    }
}

fn extract_filename(headers: &reqwest::header::HeaderMap, url: &str) -> String {
    // 1. Try Content-Disposition: attachment; filename="xxx"
    if let Some(name) = extract_from_content_disposition(headers) {
        return name;
    }

    // 2. Try URL path (after removing query & fragment)
    if let Some(name) = extract_from_url(url) {
        return name;
    }

    // 3. Try Content-Type → build "download.ext"
    if let Some(ct) = headers.get(reqwest::header::CONTENT_TYPE)
        && let Ok(ct_str) = ct.to_str()
        && let Some(ext) = mime_to_ext(ct_str)
    {
        return format!("download.{}", ext);
    }

    "download".to_string()
}

fn extract_from_content_disposition(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let disposition = headers.get(reqwest::header::CONTENT_DISPOSITION)?;
    let value = disposition.to_str().ok()?;

    // Prefer filename*= (RFC 5987 / RFC 6266) over filename=
    for part in value.split(';') {
        let trimmed = part.trim();
        if let Some(name) = trimmed.strip_prefix("filename*=") {
            // Format: charset'language'percent-encoded-name
            // e.g. UTF-8''My%20File.pdf
            let name = name.trim();
            if let Some(encoded) = name.split('\'').nth(2)
                && let Ok(decoded) = urlencoding_decode(encoded)
            {
                let decoded = decoded.trim();
                if !decoded.is_empty() {
                    return Some(sanitize_filename(decoded));
                }
            }
        }
    }

    for part in value.split(';') {
        let trimmed = part.trim();
        if let Some(name) = trimmed.strip_prefix("filename=") {
            let name = name.trim_matches(|c| c == '"' || c == '\'' || c == ' ');
            if !name.is_empty() {
                return Some(sanitize_filename(name));
            }
        }
    }

    None
}

pub fn extract_from_url(url: &str) -> Option<String> {
    // Strip query and fragment
    let path = url.split('?').next().unwrap_or(url);
    let path = path.split('#').next().unwrap_or(path);
    let segment = path.rsplit('/').next()?;
    let decoded = urlencoding_decode(segment).unwrap_or_else(|_| segment.to_string());
    let decoded = decoded.trim();
    if decoded.is_empty() || decoded == "/" {
        return None;
    }
    Some(sanitize_filename(decoded))
}

/// Remove or replace characters that are illegal in file names on Windows/macOS/Linux.
pub fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let s = s.trim_matches(|c: char| c == '.' || c == ' ');
    if s.is_empty() {
        "download".to_string()
    } else {
        s.to_string()
    }
}

fn urlencoding_decode(s: &str) -> Result<String, String> {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                result.push(byte);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            result.push(b' ');
            i += 1;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(result).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Dedup file name: "file.txt" → "file (1).txt" etc.
// ---------------------------------------------------------------------------

pub async fn dedup_filename(dir: &Path, name: &str) -> String {
    use std::ffi::OsStr;

    // Phase 1: fast probe — most of the time there is no conflict.
    let candidate = dir.join(name);
    let temp_candidate = PathBuf::from(format!("{}{}", candidate.display(), TEMP_EXT));
    if !tokio::fs::try_exists(&candidate).await.unwrap_or(false)
        && !tokio::fs::try_exists(&temp_candidate).await.unwrap_or(false)
    {
        return name.to_string();
    }

    // Phase 2: conflict detected — scan directory into memory to avoid
    // up to 19998 filesystem calls in the dedup loop.
    let existing = {
        let mut set = std::collections::HashSet::new();
        if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                set.insert(entry.file_name()); // OsString: handles non-UTF-8
            }
        }
        set
    };

    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let ext = Path::new(name).extension().and_then(|s| s.to_str());

    for i in 1..=9999 {
        let new_name = if let Some(ext) = ext {
            format!("{} ({}).{}", stem, i, ext)
        } else {
            format!("{} ({})", stem, i)
        };
        let temp_name = format!("{}{}", new_name, TEMP_EXT);
        // Check both the final and in-progress file names.
        if !existing.contains(OsStr::new(&new_name))
            && !existing.contains(OsStr::new(&temp_name))
        {
            return new_name;
        }
    }
    name.to_string()
}

// ---------------------------------------------------------------------------
// Retry helper
// ---------------------------------------------------------------------------

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(3);

/// Temporary file extension used during download (like Chrome's `.crdownload`).
/// The file is renamed to the final name only after all data is verified.
pub const TEMP_EXT: &str = ".fdownloading";

/// Buffer size for `BufWriter` wrapping file I/O during downloads.
/// 256 KB reduces the frequency of syscalls compared to the default 8 KB,
/// significantly improving throughput especially with many concurrent segments.
pub const BUF_WRITER_CAPACITY: usize = 256 * 1024;

/// Interval (in seconds) between DB persistence of download progress.
/// Balances crash-recovery granularity (max ~3 s of re-download) against
/// SQLite Mutex contention (reduces writes from ~80/s to ~5/s with 16 segments).
pub const DB_SAVE_INTERVAL_SECS: u64 = 3;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn run_download(params: DownloadParams) {
    let task_id_log = params.task_id.clone();
    let result = run_download_inner(&params).await;

    match result {
        Ok(total) => {
            rinf::debug_print!("[download] task {} completed, total={} bytes", task_id_log, total);
            let _ = params.db.update_task_status(&params.task_id, 3, "").await;
            let _ = params
                .progress_tx
                .send(ProgressUpdate {
                    task_id: params.task_id,
                    downloaded_bytes: total,
                    total_bytes: total,
                    status: 3,
                    error_message: String::new(),
                    file_name: String::new(),
                    segment_details: None,
                })
                .await;
        }
        Err(DownloadError::Cancelled) => {
            rinf::debug_print!("[download] task {} cancelled", task_id_log);
            // pause / cancel already handled upstream — nothing to do
        }
        Err(e) => {
            let msg = e.to_string();
            rinf::debug_print!("[download] task {} error: {}", task_id_log, msg);
            let _ = params.db.update_task_status(&params.task_id, 4, &msg).await;
            let _ = params
                .progress_tx
                .send(ProgressUpdate {
                    task_id: params.task_id,
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    status: 4,
                    error_message: msg,
                    file_name: String::new(),
                    segment_details: None,
                })
                .await;
        }
    }
}

/// Run the segment advisor to dynamically compute optimal segment count.
/// Updates `tasks.segments` in DB so that subsequent resumes skip the probe.
async fn compute_segments_with_advisor(p: &DownloadParams, info: &FileInfo) -> i32 {
    use crate::segment_advisor::{
        advise_static, advise_with_bandwidth, probe_bandwidth, AdvisorInput,
    };
    let advisor_input = AdvisorInput {
        total_bytes: info.total_bytes,
        supports_range: info.supports_range,
    };

    // Phase 1: static recommendation (file size + CPU cores).
    let static_advice = advise_static(&advisor_input);
    rinf::debug_print!(
        "[download] task {} static advice: segments={}, reason={}",
        p.task_id,
        static_advice.segments,
        static_advice.reason
    );

    let result = if static_advice.segments > 1 {
        // Phase 2: bandwidth probe to refine the recommendation.
        match probe_bandwidth(&p.client, &p.url, info.supports_range, &p.cancel_token).await {
            Some(bw) => {
                let bw_advice = advise_with_bandwidth(&advisor_input, bw);
                rinf::debug_print!(
                    "[download] task {} bandwidth probe: {:.1} KB/s → segments={}, reason={}",
                    p.task_id,
                    bw / 1024.0,
                    bw_advice.segments,
                    bw_advice.reason
                );
                bw_advice.segments
            }
            None => {
                rinf::debug_print!(
                    "[download] task {} bandwidth probe failed/cancelled, using static advice",
                    p.task_id
                );
                static_advice.segments
            }
        }
    } else {
        static_advice.segments
    };

    // Persist to DB so resume_task can skip the advisor.
    // If this write fails, the advisor will re-run on resume — acceptable.
    if let Err(e) = p.db.update_task_segments(&p.task_id, result).await {
        rinf::debug_print!(
            "[download] task {} failed to persist segment count to DB: {}",
            p.task_id, e
        );
    }

    result
}

async fn run_download_inner(p: &DownloadParams) -> Result<i64, DownloadError> {
    rinf::debug_print!("[download] task {} starting, url={}", p.task_id, p.url);

    let client = &p.client;

    rinf::debug_print!("[download] task {} resolving file info...", p.task_id);
    let info = resolve_file_info(client, &p.url).await?;
    rinf::debug_print!(
        "[download] task {} resolved: name={}, size={}, range={}",
        p.task_id,
        info.file_name,
        info.total_bytes,
        info.supports_range
    );

    let auto_name = if p.file_name.is_empty() {
        info.file_name.clone()
    } else {
        p.file_name.clone()
    };

    let save_dir = PathBuf::from(&p.save_dir);

    // When resuming, the file on disk belongs to *this* task — skip dedup.
    // For new downloads, dedup to avoid overwriting unrelated files.
    let actual_name = if p.is_resume {
        auto_name.clone()
    } else {
        dedup_filename(&save_dir, &auto_name).await
    };

    p.db.update_task_file_info(&p.task_id, &actual_name, info.total_bytes)
        .await?;

    let _ = p.db.update_task_status(&p.task_id, 1, "").await;

    // Immediately notify Dart: status=1 with resolved file name & total size
    let _ = p
        .progress_tx
        .send(ProgressUpdate {
            task_id: p.task_id.clone(),
            downloaded_bytes: 0,
            total_bytes: info.total_bytes,
            status: 1,
            error_message: String::new(),
            file_name: actual_name.clone(),
            segment_details: None,
        })
        .await;

    let dest_path = save_dir.join(&actual_name);
    // Chrome-style: write to a temporary file during download, rename on success.
    let temp_path = PathBuf::from(format!("{}{}", dest_path.display(), TEMP_EXT));

    // Dynamic segment calculation when user chose "auto" (segment_count <= 0).
    let segments = if p.segment_count <= 0 {
        // When resuming, check if DB already has segment rows from a previous
        // run.  If so, reuse that count — avoids a redundant bandwidth probe
        // and guarantees segment definitions stay consistent with what's on disk.
        if p.is_resume {
            let existing = p.db.load_segments(&p.task_id).await.unwrap_or_default();
            if !existing.is_empty() {
                let n = existing.len() as i32;
                rinf::debug_print!(
                    "[download] task {} resume: reusing {} existing segment(s) from DB",
                    p.task_id, n
                );
                n
            } else {
                // Segment rows were lost (e.g. crash between tasks.segments
                // update and insert_segments).  Fall through to advisor.
                compute_segments_with_advisor(p, &info).await
            }
        } else {
            compute_segments_with_advisor(p, &info).await
        }
    } else {
        p.segment_count
    };

    // Use multi-segment only when the server supports Range,
    // file is > 1 MB, and we asked for more than 1 segment.
    let use_segments = info.supports_range && info.total_bytes > 1_048_576 && segments > 1;

    rinf::debug_print!(
        "[download] task {} mode={}, segments={}, temp={}, dest={}",
        p.task_id,
        if use_segments { "multi-segment" } else { "single" },
        segments,
        temp_path.display(),
        dest_path.display()
    );

    if use_segments {
        download_multi_segment(
            &p.task_id,
            &p.url,
            &temp_path,
            info.total_bytes,
            segments,
            client,
            &p.db,
            &p.progress_tx,
            &p.cancel_token,
            &p.speed_limiter,
        )
        .await?;
    } else {
        download_single(
            &p.task_id,
            &p.url,
            &temp_path,
            info.total_bytes,
            info.supports_range,
            client,
            &p.db,
            &p.progress_tx,
            &p.cancel_token,
            &p.speed_limiter,
        )
        .await?;
    }

    // Integrity check — verify download completeness.
    if info.total_bytes > 0 {
        if use_segments {
            // Multi-segment: file is pre-allocated via set_len() so metadata
            // size always == total_bytes.  Check actual progress from DB instead.
            let segs = p.db.load_segments(&p.task_id).await?;
            let seg_total: i64 = segs.iter().map(|s| s.downloaded_bytes).sum();
            if seg_total != info.total_bytes {
                return Err(DownloadError::Other(format!(
                    "segment integrity failed: expected {} bytes, segments downloaded {} bytes",
                    info.total_bytes, seg_total
                )));
            }
        } else {
            // Single-thread: no pre-allocation, file size == downloaded bytes.
            let meta = tokio::fs::metadata(&temp_path).await?;
            if (meta.len() as i64) != info.total_bytes {
                return Err(DownloadError::Other(format!(
                    "size mismatch: expected {} bytes, got {} bytes",
                    info.total_bytes, meta.len()
                )));
            }
        }
    }

    // All data verified — rename temp file to final destination.
    // This is the atomic moment the file "appears" as complete.
    tokio::fs::rename(&temp_path, &dest_path).await.map_err(|e| {
        DownloadError::Other(format!(
            "failed to rename {} → {}: {}",
            temp_path.display(),
            dest_path.display(),
            e
        ))
    })?;

    rinf::debug_print!(
        "[download] task {} renamed {} → {}",
        p.task_id,
        temp_path.display(),
        dest_path.display()
    );

    Ok(info.total_bytes)
}

// ---------------------------------------------------------------------------
// Single-thread download (with resume support)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn download_single(
    task_id: &str,
    url: &str,
    dest: &Path,
    total_bytes: i64,
    supports_range: bool,
    client: &Client,
    db: &Db,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
    cancel_token: &CancellationToken,
    speed_limiter: &SpeedLimiter,
) -> Result<(), DownloadError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Check if there's an existing partial file we can resume
    let existing_len = match tokio::fs::metadata(dest).await {
        Ok(m) => m.len() as i64,
        Err(_) => 0,
    };

    // Resume only if server supports Range and we have a partial file that is
    // smaller than total (or total is unknown)
    let resume = supports_range && existing_len > 0 && (total_bytes == 0 || existing_len < total_bytes);

    let mut downloaded: i64;
    let mut file;

    let mut req = client.get(url);
    if resume {
        req = req.header("Range", format!("bytes={}-", existing_len));
        downloaded = existing_len;
        let mut raw_file = OpenOptions::new().write(true).open(dest).await?;
        raw_file.seek(std::io::SeekFrom::End(0)).await?;
        file = tokio::io::BufWriter::with_capacity(BUF_WRITER_CAPACITY, raw_file);
    } else {
        downloaded = 0;
        file = tokio::io::BufWriter::with_capacity(
            BUF_WRITER_CAPACITY,
            File::create(dest).await?,
        );
        // Reset DB progress so the UI doesn't show stale values
        let _ = db.update_task_progress(task_id, 0).await;
    }

    let resp = req.send().await?.error_for_status()?;

    // Try extracting a better filename from the actual download response.
    // This is the ultimate fallback — the real GET may have Content-Disposition
    // even when the probe HEAD/GET-Range:0-0 didn't.
    let resp_name = extract_filename(resp.headers(), resp.url().as_str());
    if !resp_name.is_empty()
        && resp_name != "download"
        && resp.headers().contains_key(reqwest::header::CONTENT_DISPOSITION)
    {
        rinf::debug_print!("[download-single] got better name from response: {}", resp_name);
        let _ = progress_tx
            .send(ProgressUpdate {
                task_id: task_id.to_string(),
                downloaded_bytes: downloaded,
                total_bytes,
                status: 1,
                error_message: String::new(),
                file_name: resp_name,
                segment_details: Some(vec![SegmentProgressInfo {
                    index: 0,
                    start_byte: 0,
                    end_byte: if total_bytes > 0 { total_bytes - 1 } else { 0 },
                    downloaded_bytes: downloaded,
                }]),
            })
            .await;
    }

    let mut stream = resp.bytes_stream();

    let mut last_report = std::time::Instant::now();
    let mut last_db_save = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                file.flush().await?;
                let _ = db.update_task_progress(task_id, downloaded).await;
                return Err(DownloadError::Cancelled);
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        // --- Speed limiter: write in sub-chunks as tokens allow ---
                        let mut offset = 0usize;
                        let chunk_len = bytes.len();
                        while offset < chunk_len {
                            let remaining = (chunk_len - offset) as u64;
                            let allowed = speed_limiter.consume(remaining).await;
                            let end = offset + allowed as usize;
                            file.write_all(&bytes[offset..end]).await?;
                            offset = end;
                        }
                        let len = chunk_len as i64;
                        downloaded += len;

                        // Progress report to Dart — every 200ms for smooth UI.
                        if last_report.elapsed().as_millis() >= 200 {
                            let _ = progress_tx
                                .send(ProgressUpdate {
                                    task_id: task_id.to_string(),
                                    downloaded_bytes: downloaded,
                                    total_bytes,
                                    status: 1,
                                    error_message: String::new(),
                                    file_name: String::new(),
                                    segment_details: Some(vec![SegmentProgressInfo {
                                        index: 0,
                                        start_byte: 0,
                                        end_byte: if total_bytes > 0 { total_bytes - 1 } else { 0 },
                                        downloaded_bytes: downloaded,
                                    }]),
                                })
                                .await;
                            last_report = std::time::Instant::now();
                        }

                        // DB persistence — periodic save for crash recovery.
                        if last_db_save.elapsed().as_secs() >= DB_SAVE_INTERVAL_SECS {
                            let _ = db.update_task_progress(task_id, downloaded).await;
                            last_db_save = std::time::Instant::now();
                        }
                    }
                    Some(Err(e)) => {
                        file.flush().await?;
                        let _ = db.update_task_progress(task_id, downloaded).await;
                        return Err(DownloadError::Request(e));
                    }
                    None => break,
                }
            }
        }
    }

    file.flush().await?;
    let _ = db.update_task_progress(task_id, downloaded).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-segment download
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn download_multi_segment(
    task_id: &str,
    url: &str,
    dest: &Path,
    total_bytes: i64,
    segment_count: i32,
    client: &Client,
    db: &Db,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
    cancel_token: &CancellationToken,
    speed_limiter: &SpeedLimiter,
) -> Result<(), DownloadError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Load existing segments from DB (for resume) or create new ones
    let mut existing_segments = db.load_segments(task_id).await?;

    // If we have saved segment progress, run integrity checks before reusing.
    if !existing_segments.is_empty() {
        // Check 1: Verify the file on disk is intact.
        // When the user deletes the file externally, the DB still holds stale
        // progress — we must detect this and reset to avoid a corrupted result.
        let db_downloaded: i64 = existing_segments.iter().map(|s| s.downloaded_bytes).sum();
        let file_len = match tokio::fs::metadata(dest).await {
            Ok(m) => m.len() as i64,
            Err(_) => 0,
        };

        if db_downloaded > 0 && (file_len == 0 || file_len < db_downloaded) {
            rinf::debug_print!(
                "[download] task {} file integrity mismatch: file_len={}, db_downloaded={}. Resetting segments.",
                task_id, file_len, db_downloaded
            );
            db.reset_segments_progress(task_id).await?;
            existing_segments = db.load_segments(task_id).await?;
        }

        // Check 2: Verify total_bytes hasn't changed since segments were created.
        // The server may have updated the file (different Content-Length).
        // If so, the old byte ranges are invalid — we must discard and recreate.
        if !existing_segments.is_empty() {
            let last_seg = existing_segments.iter().max_by_key(|s| s.index);
            if let Some(last) = last_seg {
                let expected_end = total_bytes - 1;
                if last.end_byte != expected_end {
                    rinf::debug_print!(
                        "[download] task {} total_bytes changed: segment end_byte={}, expected={}. Discarding old segments.",
                        task_id, last.end_byte, expected_end
                    );
                    // Delete stale segment rows and let them be recreated below.
                    db.delete_segments(task_id).await?;
                    existing_segments = Vec::new();
                }
            }
        }
    }

    let seg_defs: Vec<(i32, i64, i64, i64)> = if existing_segments.is_empty() {
        let chunk_size = total_bytes / segment_count as i64;
        let mut defs = Vec::new();
        for i in 0..segment_count {
            let start = i as i64 * chunk_size;
            let end = if i == segment_count - 1 {
                total_bytes - 1
            } else {
                (i as i64 + 1) * chunk_size - 1
            };
            defs.push((i, start, end, 0i64));
        }
        let db_segs: Vec<(i32, i64, i64)> = defs.iter().map(|(i, s, e, _)| (*i, *s, *e)).collect();
        db.insert_segments(task_id, &db_segs).await?;
        defs
    } else {
        existing_segments
            .iter()
            .map(|s| (s.index, s.start_byte, s.end_byte, s.downloaded_bytes))
            .collect()
    };

    let total_downloaded = Arc::new(AtomicI64::new(
        seg_defs.iter().map(|(_, _, _, d)| d).sum::<i64>(),
    ));

    // Shared segment progress state for IDM-style visualization.
    // Each spawned segment task updates its own entry; when any segment
    // sends a ProgressUpdate it snapshots the entire vector.
    let seg_states: Arc<StdMutex<Vec<SegmentProgressInfo>>> = Arc::new(StdMutex::new(
        seg_defs
            .iter()
            .map(|(idx, start, end, dl)| SegmentProgressInfo {
                index: *idx,
                start_byte: *start,
                end_byte: *end,
                downloaded_bytes: *dl,
            })
            .collect(),
    ));

    // Pre-allocate file to full size (Chrome-style).
    // The .x_down extension tells the user the download is in progress.
    // Each segment seeks to its byte range and writes directly.
    {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(dest)
            .await?;
        if file.metadata().await?.len() < total_bytes as u64 {
            file.set_len(total_bytes as u64).await?;
        }
    }

    let mut handles = Vec::new();

    for (idx, start, end, already_downloaded) in &seg_defs {
        let actual_start = start + already_downloaded;
        if actual_start > *end {
            // This segment is already complete
            continue;
        }
        let client = client.clone();
        let url = url.to_string();
        let dest = dest.to_path_buf();
        let cancel = cancel_token.clone();
        let total_dl = total_downloaded.clone();
        let seg_states = seg_states.clone();
        let db = db.clone();
        let task_id = task_id.to_string();
        let seg_idx = *idx;
        let seg_start = *start;
        let seg_end = *end;
        let progress_tx = progress_tx.clone();
        let total = total_bytes;
        let limiter = speed_limiter.clone();

        let handle = tokio::spawn(async move {
            do_segment_with_retry(
                &task_id,
                seg_idx,
                &url,
                &dest,
                seg_start,
                actual_start,
                seg_end,
                &client,
                &cancel,
                &total_dl,
                total,
                &db,
                &progress_tx,
                &seg_states,
                &limiter,
            )
            .await
        });
        handles.push(handle);
    }

    let mut final_error = None;
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(DownloadError::Cancelled)) => {
                if final_error.is_none() {
                    final_error = Some(DownloadError::Cancelled);
                }
            }
            Ok(Err(e)) => {
                if final_error.is_none() {
                    cancel_token.cancel();
                    final_error = Some(e);
                }
            }
            Err(e) => {
                if final_error.is_none() {
                    cancel_token.cancel();
                    final_error = Some(DownloadError::Other(e.to_string()));
                }
            }
        }
    }

    if let Some(err) = final_error {
        return Err(err);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Segment download with retry
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn do_segment_with_retry(
    task_id: &str,
    seg_idx: i32,
    url: &str,
    dest: &Path,
    seg_start: i64,
    mut actual_start: i64,
    seg_end: i64,
    client: &Client,
    cancel: &CancellationToken,
    total_downloaded: &AtomicI64,
    total_bytes: i64,
    db: &Db,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
    seg_states: &Arc<StdMutex<Vec<SegmentProgressInfo>>>,
    speed_limiter: &SpeedLimiter,
) -> Result<(), DownloadError> {
    let mut attempts = 0u32;

    loop {
        match do_segment(
            task_id,
            seg_idx,
            url,
            dest,
            seg_start,
            actual_start,
            seg_end,
            client,
            cancel,
            total_downloaded,
            total_bytes,
            db,
            progress_tx,
            seg_states,
            speed_limiter,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(DownloadError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(e) => {
                attempts += 1;
                if attempts >= MAX_RETRIES {
                    return Err(e);
                }
                // Update actual_start from DB for resume after partial failure
                if let Ok(segs) = db.load_segments(task_id).await
                    && let Some(seg) = segs.iter().find(|s| s.index == seg_idx)
                {
                    actual_start = seg_start + seg.downloaded_bytes;
                    if actual_start > seg_end {
                        return Ok(()); // completed during previous attempt
                    }
                }
                // Wait before retry (respecting cancellation)
                tokio::select! {
                    _ = cancel.cancelled() => return Err(DownloadError::Cancelled),
                    _ = tokio::time::sleep(RETRY_DELAY) => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Single segment download
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn do_segment(
    task_id: &str,
    seg_idx: i32,
    url: &str,
    dest: &Path,
    seg_start: i64,
    actual_start: i64,
    seg_end: i64,
    client: &Client,
    cancel: &CancellationToken,
    total_downloaded: &AtomicI64,
    total_bytes: i64,
    db: &Db,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
    seg_states: &Arc<StdMutex<Vec<SegmentProgressInfo>>>,
    speed_limiter: &SpeedLimiter,
) -> Result<(), DownloadError> {
    let range = format!("bytes={}-{}", actual_start, seg_end);
    let resp = client
        .get(url)
        .header("Range", range)
        .send()
        .await?
        .error_for_status()?;

    // For the first segment, try extracting a better filename from response.
    if seg_idx == 0
        && let Some(cd) = resp.headers().get(reqwest::header::CONTENT_DISPOSITION) {
            let resp_name = extract_filename(resp.headers(), resp.url().as_str());
            if !resp_name.is_empty() && resp_name != "download" {
                rinf::debug_print!("[download-seg0] got better name from response: {} (cd={:?})", resp_name, cd);
                let snapshot = seg_states.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let _ = progress_tx
                    .send(ProgressUpdate {
                        task_id: task_id.to_string(),
                        downloaded_bytes: total_downloaded.load(Ordering::Relaxed),
                        total_bytes,
                        status: 1,
                        error_message: String::new(),
                        file_name: resp_name,
                        segment_details: Some(snapshot),
                    })
                    .await;
            }
        }

    let mut stream = resp.bytes_stream();

    // Open the shared pre-allocated file; seek to this segment's write position.
    let file = OpenOptions::new().write(true).open(dest).await?;
    let mut file = tokio::io::BufWriter::with_capacity(BUF_WRITER_CAPACITY, file);
    file.seek(std::io::SeekFrom::Start(actual_start as u64))
        .await?;

    let mut seg_downloaded = actual_start - seg_start;
    let mut last_report = std::time::Instant::now();
    let mut last_db_save = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                file.flush().await?;
                // Update shared state before exiting
                if let Ok(mut states) = seg_states.lock()
                    && let Some(s) = states.iter_mut().find(|s| s.index == seg_idx) {
                        s.downloaded_bytes = seg_downloaded;
                    }
                let _ = db.update_segment_progress(task_id, seg_idx, seg_downloaded).await;
                return Err(DownloadError::Cancelled);
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        // --- Speed limiter: write in sub-chunks as tokens allow ---
                        let mut offset = 0usize;
                        let chunk_len = bytes.len();
                        while offset < chunk_len {
                            let remaining = (chunk_len - offset) as u64;
                            let allowed = speed_limiter.consume(remaining).await;
                            let end = offset + allowed as usize;
                            file.write_all(&bytes[offset..end]).await?;
                            offset = end;
                        }
                        let len = chunk_len as i64;
                        seg_downloaded += len;
                        total_downloaded.fetch_add(len, Ordering::Relaxed);

                        // Update shared segment state (cheap — std::sync::Mutex, no await)
                        if let Ok(mut states) = seg_states.lock()
                            && let Some(s) = states.iter_mut().find(|s| s.index == seg_idx) {
                                s.downloaded_bytes = seg_downloaded;
                            }

                        // Progress report to Dart — every 200ms for smooth UI.
                        if last_report.elapsed().as_millis() >= 200 {
                            let current_total =
                                total_downloaded.load(Ordering::Relaxed);
                            let snapshot = seg_states.lock().unwrap_or_else(|e| e.into_inner()).clone();
                            let _ = progress_tx
                                .send(ProgressUpdate {
                                    task_id: task_id.to_string(),
                                    downloaded_bytes: current_total,
                                    total_bytes,
                                    status: 1,
                                    error_message: String::new(),
                                    file_name: String::new(),
                                    segment_details: Some(snapshot),
                                })
                                .await;
                            last_report = std::time::Instant::now();
                        }

                        // DB persistence — every few seconds is sufficient
                        // for resume.  With 16 segments this reduces DB
                        // writes from ~80/s to ~5/s, cutting Mutex contention.
                        if last_db_save.elapsed().as_secs() >= DB_SAVE_INTERVAL_SECS {
                            let _ = db
                                .update_segment_progress(
                                    task_id,
                                    seg_idx,
                                    seg_downloaded,
                                )
                                .await;
                            last_db_save = std::time::Instant::now();
                        }
                    }
                    Some(Err(e)) => {
                        file.flush().await?;
                        if let Ok(mut states) = seg_states.lock()
                            && let Some(s) = states.iter_mut().find(|s| s.index == seg_idx) {
                                s.downloaded_bytes = seg_downloaded;
                            }
                        let _ = db
                            .update_segment_progress(task_id, seg_idx, seg_downloaded)
                            .await;
                        return Err(DownloadError::Request(e));
                    }
                    None => break,
                }
            }
        }
    }

    file.flush().await?;
    // Update shared state for final progress
    if let Ok(mut states) = seg_states.lock()
        && let Some(s) = states.iter_mut().find(|s| s.index == seg_idx) {
            s.downloaded_bytes = seg_downloaded;
        }
    let _ = db
        .update_segment_progress(task_id, seg_idx, seg_downloaded)
        .await;
    Ok(())
}
