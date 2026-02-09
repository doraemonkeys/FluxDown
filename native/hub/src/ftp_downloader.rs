//! FTP protocol download engine.
//!
//! Uses suppaftp's **synchronous** FTP API with `tokio::task::spawn_blocking`
//! to avoid async-runtime conflicts (suppaftp async depends on async-std).
//!
//! Architecture:
//! - Single-thread and multi-segment download modes
//! - REST command for breakpoint resume
//! - Each segment opens its own FTP connection (standard parallel FTP approach)
//! - Shared SpeedLimiter, DB persistence, progress reporting
//! - CancellationToken for pause/cancel

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::downloader::{
    dedup_filename, extract_from_url, sanitize_filename, DownloadError, DownloadParams, FileInfo,
    ProgressUpdate, SegmentProgressInfo, BUF_WRITER_CAPACITY, DB_SAVE_INTERVAL_SECS, TEMP_EXT,
};
use crate::speed_limiter::SpeedLimiter;

// ---------------------------------------------------------------------------
// FTP URL parsing
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FtpUrl {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub path: String,
}

pub fn parse_ftp_url(url: &str) -> Result<FtpUrl, DownloadError> {
    let lower = url.to_ascii_lowercase();
    let stripped = if lower.starts_with("ftp://") {
        &url[6..]
    } else {
        return Err(DownloadError::Other("not an FTP URL".to_string()));
    };

    // Use rfind to handle passwords containing literal '@' characters.
    // E.g. ftp://user:p@ss@host/file → userinfo="user:p@ss", hostpath="host/file"
    let (userinfo, hostpath) = if let Some(at_pos) = stripped.rfind('@') {
        (&stripped[..at_pos], &stripped[at_pos + 1..])
    } else {
        ("", stripped)
    };

    let (username, password) = if userinfo.is_empty() {
        ("anonymous".to_string(), "anonymous@".to_string())
    } else if let Some(colon) = userinfo.find(':') {
        (
            url_decode(&userinfo[..colon]),
            url_decode(&userinfo[colon + 1..]),
        )
    } else {
        (url_decode(userinfo), String::new())
    };

    let (hostport, path) = if let Some(slash) = hostpath.find('/') {
        (&hostpath[..slash], &hostpath[slash..])
    } else {
        (hostpath, "/")
    };

    let (host, port) = if let Some(colon) = hostport.rfind(':') {
        let port_str = &hostport[colon + 1..];
        match port_str.parse::<u16>() {
            Ok(p) => (hostport[..colon].to_string(), p),
            Err(_) => (hostport.to_string(), 21),
        }
    } else {
        (hostport.to_string(), 21)
    };

    if host.is_empty() {
        return Err(DownloadError::Other("empty FTP host".to_string()));
    }

    Ok(FtpUrl {
        host,
        port,
        username,
        password,
        path: url_decode(path),
    })
}

fn url_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                result.push(byte);
                i += 3;
                continue;
            }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(result).unwrap_or_else(|_| s.to_string())
}

// ---------------------------------------------------------------------------
// Sync FTP helper: connect + login + binary mode
// ---------------------------------------------------------------------------

use suppaftp::FtpStream;
use suppaftp::types::FileType;

/// Timeout for FTP data-connection reads.  Prevents blocking threads from
/// hanging indefinitely when the server stops sending data (e.g. on cancel).
/// Applied to the data stream TCP socket after `retr_as_stream`.
const FTP_DATA_READ_TIMEOUT: Duration = Duration::from_secs(60);

fn ftp_connect_sync(ftp_url: &FtpUrl) -> Result<FtpStream, DownloadError> {
    let addr = format!("{}:{}", ftp_url.host, ftp_url.port);

    let sock_addr: std::net::SocketAddr = addr
        .parse()
        .or_else(|_| {
            // hostname — resolve via DNS
            use std::net::ToSocketAddrs;
            addr.to_socket_addrs()
                .map_err(|e| DownloadError::Other(format!("DNS resolve error: {}", e)))?
                .next()
                .ok_or_else(|| DownloadError::Other("DNS returned no addresses".to_string()))
        })?;

    let mut stream = FtpStream::connect_timeout(sock_addr, Duration::from_secs(30))
        .map_err(|e| DownloadError::Other(format!("FTP connect error: {}", e)))?;

    stream
        .login(&ftp_url.username, &ftp_url.password)
        .map_err(|e| DownloadError::Other(format!("FTP login error: {}", e)))?;

    stream
        .transfer_type(FileType::Binary)
        .map_err(|e| DownloadError::Other(format!("FTP set binary mode error: {}", e)))?;

    Ok(stream)
}

// ---------------------------------------------------------------------------
// Resolve FTP file info
// ---------------------------------------------------------------------------

const PROBE_MAX_RETRIES: u32 = 3;
const PROBE_RETRY_DELAY: Duration = Duration::from_secs(2);

pub async fn resolve_ftp_file_info(url: &str) -> Result<FileInfo, DownloadError> {
    let ftp_url = parse_ftp_url(url)?;

    let mut last_err = None;
    for attempt in 0..PROBE_MAX_RETRIES {
        let fu = ftp_url.clone();
        let result = tokio::task::spawn_blocking(move || resolve_ftp_info_sync(&fu))
            .await
            .map_err(|e| DownloadError::Other(format!("spawn_blocking join error: {}", e)))?;

        match result {
            Ok(info) => return Ok(info),
            Err(e) => {
                rinf::debug_print!(
                    "[ftp-resolve] attempt {}/{} failed: {}",
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
    Err(last_err.unwrap_or_else(|| DownloadError::Other("FTP probe failed".to_string())))
}

fn resolve_ftp_info_sync(ftp_url: &FtpUrl) -> Result<FileInfo, DownloadError> {
    let mut ftp = ftp_connect_sync(ftp_url)?;

    let total_bytes = match ftp.size(&ftp_url.path) {
        Ok(size) => size as i64,
        Err(e) => {
            rinf::debug_print!("[ftp-resolve] SIZE failed: {}, assuming unknown", e);
            0
        }
    };

    let file_name = ftp_url
        .path
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(sanitize_filename)
        .or_else(|| extract_from_url(&format!("ftp://{}{}", ftp_url.host, ftp_url.path)))
        .unwrap_or_else(|| "download".to_string());

    let supports_range = total_bytes > 0;

    let _ = ftp.quit();

    rinf::debug_print!(
        "[ftp-resolve] path={}, name={}, size={}, range={}",
        ftp_url.path,
        file_name,
        total_bytes,
        supports_range
    );

    Ok(FileInfo {
        file_name,
        total_bytes,
        supports_range,
    })
}

// ---------------------------------------------------------------------------
// FTP bandwidth probe
// ---------------------------------------------------------------------------

pub async fn probe_ftp_bandwidth(
    url: &str,
    cancel_token: &CancellationToken,
) -> Option<f64> {
    const PROBE_BYTES: u64 = 512 * 1024;

    let ftp_url = match parse_ftp_url(url) {
        Ok(u) => u,
        Err(_) => return None,
    };

    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();

    // Watch for cancellation in the background.
    let cancel_watcher = {
        let token = cancel_token.clone();
        let flag = cancelled.clone();
        tokio::spawn(async move {
            token.cancelled().await;
            flag.store(true, Ordering::SeqCst);
        })
    };

    let result = tokio::task::spawn_blocking(move || {
        let mut ftp = match ftp_connect_sync(&ftp_url) {
            Ok(f) => f,
            Err(_) => return None,
        };

        let start = std::time::Instant::now();

        let mut data_stream = match ftp.retr_as_stream(&ftp_url.path) {
            Ok(s) => s,
            Err(_) => {
                let _ = ftp.quit();
                return None;
            }
        };

        // Set read timeout on data connection to prevent indefinite blocking.
        data_stream
            .get_ref()
            .set_read_timeout(Some(FTP_DATA_READ_TIMEOUT))
            .ok();

        let mut buf = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;

        loop {
            if cancelled_clone.load(Ordering::SeqCst) {
                break;
            }
            match data_stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    total += n as u64;
                    if total >= PROBE_BYTES {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        // On cancel or early break, drop data_stream first to close the data
        // connection, then try to clean up.  finalize_retr_stream may block
        // waiting for a 226 response that never comes if we aborted early.
        if cancelled_clone.load(Ordering::SeqCst) {
            drop(data_stream);
            let _ = ftp.quit();
        } else {
            let _ = ftp.finalize_retr_stream(data_stream);
            let _ = ftp.quit();
        }

        let elapsed = start.elapsed();
        if elapsed.as_millis() < 50 || total < 1024 {
            return None;
        }

        Some(total as f64 / elapsed.as_secs_f64())
    })
    .await
    .unwrap_or(None);

    cancel_watcher.abort();
    result
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn run_ftp_download(params: DownloadParams) {
    let task_id_log = params.task_id.clone();
    let result = run_ftp_download_inner(&params).await;

    match result {
        Ok(total) => {
            rinf::debug_print!(
                "[ftp-download] task {} completed, total={} bytes",
                task_id_log,
                total
            );
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
            rinf::debug_print!("[ftp-download] task {} cancelled", task_id_log);
        }
        Err(e) => {
            let msg = e.to_string();
            rinf::debug_print!("[ftp-download] task {} error: {}", task_id_log, msg);
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

async fn compute_ftp_segments(p: &DownloadParams, info: &FileInfo) -> i32 {
    use crate::segment_advisor::{advise_static, advise_with_bandwidth, AdvisorInput};

    let advisor_input = AdvisorInput {
        total_bytes: info.total_bytes,
        supports_range: info.supports_range,
    };

    let static_advice = advise_static(&advisor_input);
    rinf::debug_print!(
        "[ftp-download] task {} static advice: segments={}, reason={}",
        p.task_id,
        static_advice.segments,
        static_advice.reason
    );

    let result = if static_advice.segments > 1 {
        match probe_ftp_bandwidth(&p.url, &p.cancel_token).await {
            Some(bw) => {
                let bw_advice = advise_with_bandwidth(&advisor_input, bw);
                rinf::debug_print!(
                    "[ftp-download] task {} bandwidth: {:.1} KB/s → segments={}",
                    p.task_id,
                    bw / 1024.0,
                    bw_advice.segments
                );
                bw_advice.segments
            }
            None => static_advice.segments,
        }
    } else {
        static_advice.segments
    };

    if let Err(e) = p.db.update_task_segments(&p.task_id, result).await {
        rinf::debug_print!(
            "[ftp-download] task {} failed to persist segment count: {}",
            p.task_id,
            e
        );
    }

    result
}

async fn run_ftp_download_inner(p: &DownloadParams) -> Result<i64, DownloadError> {
    rinf::debug_print!("[ftp-download] task {} starting, url={}", p.task_id, p.url);

    let info = resolve_ftp_file_info(&p.url).await?;
    rinf::debug_print!(
        "[ftp-download] task {} resolved: name={}, size={}, range={}",
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
    let actual_name = if p.is_resume {
        auto_name.clone()
    } else {
        dedup_filename(&save_dir, &auto_name).await
    };

    p.db.update_task_file_info(&p.task_id, &actual_name, info.total_bytes)
        .await?;
    let _ = p.db.update_task_status(&p.task_id, 1, "").await;

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
    let temp_path = PathBuf::from(format!("{}{}", dest_path.display(), TEMP_EXT));

    // Dynamic segment calculation
    let segments = if p.segment_count <= 0 {
        if p.is_resume {
            let existing = p.db.load_segments(&p.task_id).await.unwrap_or_default();
            if !existing.is_empty() {
                existing.len() as i32
            } else {
                compute_ftp_segments(p, &info).await
            }
        } else {
            compute_ftp_segments(p, &info).await
        }
    } else {
        p.segment_count
    };

    let use_segments = info.supports_range && info.total_bytes > 1_048_576 && segments > 1;

    rinf::debug_print!(
        "[ftp-download] task {} mode={}, segments={}",
        p.task_id,
        if use_segments { "multi-segment" } else { "single" },
        segments,
    );

    let ftp_url = parse_ftp_url(&p.url)?;

    if use_segments {
        ftp_download_multi_segment(
            &p.task_id,
            &ftp_url,
            &temp_path,
            info.total_bytes,
            segments,
            &p.db,
            &p.progress_tx,
            &p.cancel_token,
            &p.speed_limiter,
        )
        .await?;
    } else {
        // Retry wrapper for single-thread FTP download.
        // ftp_download_single supports resume (checks existing file length),
        // so retrying after a transient failure is safe.
        let mut attempts = 0u32;
        loop {
            match ftp_download_single(
                &p.task_id,
                &ftp_url,
                &temp_path,
                info.total_bytes,
                info.supports_range,
                &p.db,
                &p.progress_tx,
                &p.cancel_token,
                &p.speed_limiter,
            )
            .await
            {
                Ok(()) => break,
                Err(DownloadError::Cancelled) => return Err(DownloadError::Cancelled),
                Err(e) => {
                    attempts += 1;
                    if attempts >= MAX_RETRIES {
                        return Err(e);
                    }
                    rinf::debug_print!(
                        "[ftp-download] task {} single-thread attempt {}/{} failed: {}",
                        p.task_id,
                        attempts,
                        MAX_RETRIES,
                        e
                    );
                    tokio::select! {
                        _ = p.cancel_token.cancelled() => return Err(DownloadError::Cancelled),
                        _ = tokio::time::sleep(RETRY_DELAY) => {}
                    }
                }
            }
        }
    }

    // Integrity check
    if info.total_bytes > 0 {
        if use_segments {
            let segs = p.db.load_segments(&p.task_id).await?;
            let seg_total: i64 = segs.iter().map(|s| s.downloaded_bytes).sum();
            if seg_total != info.total_bytes {
                return Err(DownloadError::Other(format!(
                    "FTP segment integrity failed: expected {} bytes, got {} bytes",
                    info.total_bytes, seg_total
                )));
            }
        } else {
            let meta = tokio::fs::metadata(&temp_path).await?;
            if (meta.len() as i64) != info.total_bytes {
                return Err(DownloadError::Other(format!(
                    "FTP size mismatch: expected {} bytes, got {} bytes",
                    info.total_bytes,
                    meta.len()
                )));
            }
        }
    }

    tokio::fs::rename(&temp_path, &dest_path).await.map_err(|e| {
        DownloadError::Other(format!(
            "failed to rename {} → {}: {}",
            temp_path.display(),
            dest_path.display(),
            e
        ))
    })?;

    Ok(info.total_bytes)
}

// ---------------------------------------------------------------------------
// Single-thread FTP download
// ---------------------------------------------------------------------------

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(3);

/// Single-thread FTP download using sync FTP in a blocking task.
/// Progress is reported back to the async world via mpsc channel.
#[allow(clippy::too_many_arguments)]
async fn ftp_download_single(
    task_id: &str,
    ftp_url: &FtpUrl,
    dest: &Path,
    total_bytes: i64,
    supports_range: bool,
    db: &Db,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
    cancel_token: &CancellationToken,
    speed_limiter: &SpeedLimiter,
) -> Result<(), DownloadError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let existing_len = match tokio::fs::metadata(dest).await {
        Ok(m) => m.len() as i64,
        Err(_) => 0,
    };

    let resume = supports_range
        && existing_len > 0
        && (total_bytes == 0 || existing_len < total_bytes);

    // Reset DB progress if starting fresh
    if !resume {
        let _ = db.update_task_progress(task_id, 0).await;
    }

    let ftp_url = ftp_url.clone();
    let dest = dest.to_path_buf();
    let task_id = task_id.to_string();
    let db = db.clone();
    let progress_tx = progress_tx.clone();
    let cancel_token = cancel_token.clone();
    let speed_limiter = speed_limiter.clone();

    // The blocking thread reads FTP data and sends chunks via channel
    // to the async side which handles file I/O and progress reporting.
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<Vec<u8>>(32);
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_writer = cancelled.clone();

    // Cancel watcher
    let cancel_watcher = {
        let token = cancel_token.clone();
        let flag = cancelled.clone();
        tokio::spawn(async move {
            token.cancelled().await;
            flag.store(true, Ordering::SeqCst);
        })
    };

    // Blocking FTP reader thread
    let ftp_reader = {
        let ftp_url = ftp_url.clone();
        let cancelled = cancelled.clone();
        let resume_offset = if resume { existing_len } else { 0 };

        tokio::task::spawn_blocking(move || -> Result<(), DownloadError> {
            let mut ftp = ftp_connect_sync(&ftp_url)?;

            if resume_offset > 0 {
                ftp.resume_transfer(resume_offset as usize)
                    .map_err(|e| DownloadError::Other(format!("FTP REST error: {}", e)))?;
            }

            let mut data_stream = ftp
                .retr_as_stream(&ftp_url.path)
                .map_err(|e| DownloadError::Other(format!("FTP RETR error: {}", e)))?;

            // Set read timeout so cancellation eventually unblocks this thread.
            data_stream
                .get_ref()
                .set_read_timeout(Some(FTP_DATA_READ_TIMEOUT))
                .ok();

            let mut buf = vec![0u8; 64 * 1024];

            loop {
                if cancelled.load(Ordering::SeqCst) {
                    break;
                }
                match data_stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if chunk_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break; // receiver dropped
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut
                              || e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Read timeout — check cancel flag and retry.
                        if cancelled.load(Ordering::SeqCst) {
                            break;
                        }
                        continue;
                    }
                    Err(e) => {
                        drop(data_stream);
                        let _ = ftp.quit();
                        return Err(DownloadError::Io(e));
                    }
                }
            }

            // On cancel, skip finalize_retr_stream (it blocks waiting for 226).
            if cancelled.load(Ordering::SeqCst) {
                drop(data_stream);
            } else {
                let _ = ftp.finalize_retr_stream(data_stream);
            }
            let _ = ftp.quit();
            Ok(())
        })
    };

    // Async writer: receives chunks and writes to file with speed limiting
    let mut downloaded: i64 = if resume { existing_len } else { 0 };
    let mut file = if resume {
        let f = OpenOptions::new().write(true).open(&dest).await?;
        let mut f = tokio::io::BufWriter::with_capacity(BUF_WRITER_CAPACITY, f);
        f.seek(std::io::SeekFrom::End(0)).await?;
        f
    } else {
        tokio::io::BufWriter::with_capacity(BUF_WRITER_CAPACITY, File::create(&dest).await?)
    };

    let mut last_report = std::time::Instant::now();
    let mut last_db_save = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                cancelled_writer.store(true, Ordering::SeqCst);
                file.flush().await?;
                let _ = db.update_task_progress(&task_id, downloaded).await;
                cancel_watcher.abort();
                // Wait for blocking thread to finish
                let _ = ftp_reader.await;
                return Err(DownloadError::Cancelled);
            }
            chunk = chunk_rx.recv() => {
                match chunk {
                    Some(bytes) => {
                        let n = bytes.len();
                        // Speed limiter
                        let mut offset = 0usize;
                        while offset < n {
                            let remaining = (n - offset) as u64;
                            let allowed = speed_limiter.consume(remaining).await;
                            let end = offset + allowed as usize;
                            file.write_all(&bytes[offset..end]).await?;
                            offset = end;
                        }
                        downloaded += n as i64;

                        if last_report.elapsed().as_millis() >= 200 {
                            let _ = progress_tx
                                .send(ProgressUpdate {
                                    task_id: task_id.clone(),
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

                        if last_db_save.elapsed().as_secs() >= DB_SAVE_INTERVAL_SECS {
                            let _ = db.update_task_progress(&task_id, downloaded).await;
                            last_db_save = std::time::Instant::now();
                        }
                    }
                    None => break, // channel closed — FTP reader done
                }
            }
        }
    }

    file.flush().await?;
    let _ = db.update_task_progress(&task_id, downloaded).await;
    cancel_watcher.abort();

    // Check reader result
    let reader_result = ftp_reader
        .await
        .map_err(|e| DownloadError::Other(format!("FTP reader join error: {}", e)))?;
    reader_result?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-segment FTP download
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn ftp_download_multi_segment(
    task_id: &str,
    ftp_url: &FtpUrl,
    dest: &Path,
    total_bytes: i64,
    segment_count: i32,
    db: &Db,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
    cancel_token: &CancellationToken,
    speed_limiter: &SpeedLimiter,
) -> Result<(), DownloadError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Load or create segment definitions
    let mut existing_segments = db.load_segments(task_id).await?;

    if !existing_segments.is_empty() {
        let db_downloaded: i64 = existing_segments.iter().map(|s| s.downloaded_bytes).sum();
        let file_len = match tokio::fs::metadata(dest).await {
            Ok(m) => m.len() as i64,
            Err(_) => 0,
        };

        if db_downloaded > 0 && (file_len == 0 || file_len < db_downloaded) {
            db.reset_segments_progress(task_id).await?;
            existing_segments = db.load_segments(task_id).await?;
        }

        if !existing_segments.is_empty() {
            let last_seg = existing_segments.iter().max_by_key(|s| s.index);
            if let Some(last) = last_seg
                && last.end_byte != total_bytes - 1 {
                    db.delete_segments(task_id).await?;
                    existing_segments = Vec::new();
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

    // Pre-allocate file
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
            continue;
        }

        let ftp_url = ftp_url.clone();
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
            ftp_do_segment_with_retry(
                &task_id,
                seg_idx,
                &ftp_url,
                &dest,
                seg_start,
                actual_start,
                seg_end,
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
// Per-segment download with retry
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn ftp_do_segment_with_retry(
    task_id: &str,
    seg_idx: i32,
    ftp_url: &FtpUrl,
    dest: &Path,
    seg_start: i64,
    mut actual_start: i64,
    seg_end: i64,
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
        match ftp_do_segment(
            task_id,
            seg_idx,
            ftp_url,
            dest,
            seg_start,
            actual_start,
            seg_end,
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
                if let Ok(segs) = db.load_segments(task_id).await
                    && let Some(seg) = segs.iter().find(|s| s.index == seg_idx) {
                        actual_start = seg_start + seg.downloaded_bytes;
                        if actual_start > seg_end {
                            return Ok(());
                        }
                    }
                tokio::select! {
                    _ = cancel.cancelled() => return Err(DownloadError::Cancelled),
                    _ = tokio::time::sleep(RETRY_DELAY) => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Single segment download (blocking FTP reader + async file writer)
// ---------------------------------------------------------------------------

/// Each FTP segment: blocking thread reads from FTP, sends chunks via channel;
/// async side handles file seek/write, speed limiting, and progress reporting.
#[allow(clippy::too_many_arguments)]
async fn ftp_do_segment(
    task_id: &str,
    seg_idx: i32,
    ftp_url: &FtpUrl,
    dest: &Path,
    seg_start: i64,
    actual_start: i64,
    seg_end: i64,
    cancel: &CancellationToken,
    total_downloaded: &AtomicI64,
    total_bytes: i64,
    db: &Db,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
    seg_states: &Arc<StdMutex<Vec<SegmentProgressInfo>>>,
    speed_limiter: &SpeedLimiter,
) -> Result<(), DownloadError> {
    let bytes_needed = (seg_end - actual_start + 1) as u64;

    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_writer = cancelled.clone();

    let cancel_watcher = {
        let token = cancel.clone();
        let flag = cancelled.clone();
        tokio::spawn(async move {
            token.cancelled().await;
            flag.store(true, Ordering::SeqCst);
        })
    };

    // Channel for data chunks from blocking reader to async writer.
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<Vec<u8>>(16);

    // Blocking FTP reader
    let ftp_reader = {
        let ftp_url = ftp_url.clone();
        let cancelled = cancelled.clone();
        let seg_bytes_needed = bytes_needed;

        tokio::task::spawn_blocking(move || -> Result<(), DownloadError> {
            let mut ftp = ftp_connect_sync(&ftp_url)?;

            ftp.resume_transfer(actual_start as usize)
                .map_err(|e| DownloadError::Other(format!("FTP REST error (seg {}): {}", seg_idx, e)))?;

            let mut data_stream = ftp
                .retr_as_stream(&ftp_url.path)
                .map_err(|e| DownloadError::Other(format!("FTP RETR error (seg {}): {}", seg_idx, e)))?;

            // Set read timeout so cancellation eventually unblocks this thread.
            data_stream
                .get_ref()
                .set_read_timeout(Some(FTP_DATA_READ_TIMEOUT))
                .ok();

            let mut buf = vec![0u8; 64 * 1024];
            let mut bytes_read: u64 = 0;

            loop {
                if cancelled.load(Ordering::SeqCst) {
                    break;
                }
                let remaining = seg_bytes_needed - bytes_read;
                if remaining == 0 {
                    break;
                }
                let to_read = (remaining as usize).min(buf.len());

                match data_stream.read(&mut buf[..to_read]) {
                    Ok(0) => break,
                    Ok(n) => {
                        bytes_read += n as u64;
                        if chunk_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut
                              || e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Read timeout — check cancel flag and retry.
                        if cancelled.load(Ordering::SeqCst) {
                            break;
                        }
                        continue;
                    }
                    Err(e) => {
                        drop(data_stream);
                        let _ = ftp.quit();
                        return Err(DownloadError::Io(e));
                    }
                }
            }

            // On cancel, skip finalize_retr_stream (it blocks waiting for 226).
            if cancelled.load(Ordering::SeqCst) {
                drop(data_stream);
            } else {
                let _ = ftp.finalize_retr_stream(data_stream);
            }
            let _ = ftp.quit();
            Ok(())
        })
    };

    // Async writer: write to pre-allocated file at correct offset.
    let raw_file = OpenOptions::new().write(true).open(dest).await?;
    let mut file = tokio::io::BufWriter::with_capacity(BUF_WRITER_CAPACITY, raw_file);
    file.seek(std::io::SeekFrom::Start(actual_start as u64)).await?;

    let mut seg_downloaded = actual_start - seg_start;
    let mut last_report = std::time::Instant::now();
    let mut last_db_save = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                cancelled_writer.store(true, Ordering::SeqCst);
                file.flush().await?;
                if let Ok(mut states) = seg_states.lock()
                    && let Some(s) = states.iter_mut().find(|s| s.index == seg_idx) {
                        s.downloaded_bytes = seg_downloaded;
                    }
                let _ = db.update_segment_progress(task_id, seg_idx, seg_downloaded).await;
                cancel_watcher.abort();
                let _ = ftp_reader.await;
                return Err(DownloadError::Cancelled);
            }
            chunk = chunk_rx.recv() => {
                match chunk {
                    Some(bytes) => {
                        let n = bytes.len();
                        // Speed limiter
                        let mut offset = 0usize;
                        while offset < n {
                            let rem = (n - offset) as u64;
                            let allowed = speed_limiter.consume(rem).await;
                            let end = offset + allowed as usize;
                            file.write_all(&bytes[offset..end]).await?;
                            offset = end;
                        }

                        let len = n as i64;
                        seg_downloaded += len;
                        total_downloaded.fetch_add(len, Ordering::Relaxed);

                        if let Ok(mut states) = seg_states.lock()
                            && let Some(s) = states.iter_mut().find(|s| s.index == seg_idx) {
                                s.downloaded_bytes = seg_downloaded;
                            }

                        if last_report.elapsed().as_millis() >= 200 {
                            let current_total = total_downloaded.load(Ordering::Relaxed);
                            let snapshot = seg_states
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .clone();
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

                        if last_db_save.elapsed().as_secs() >= DB_SAVE_INTERVAL_SECS {
                            let _ = db
                                .update_segment_progress(task_id, seg_idx, seg_downloaded)
                                .await;
                            last_db_save = std::time::Instant::now();
                        }
                    }
                    None => break,
                }
            }
        }
    }

    file.flush().await?;
    if let Ok(mut states) = seg_states.lock()
        && let Some(s) = states.iter_mut().find(|s| s.index == seg_idx) {
            s.downloaded_bytes = seg_downloaded;
        }
    let _ = db.update_segment_progress(task_id, seg_idx, seg_downloaded).await;
    cancel_watcher.abort();

    let reader_result = ftp_reader
        .await
        .map_err(|e| DownloadError::Other(format!("FTP segment reader join error: {}", e)))?;
    reader_result?;

    Ok(())
}
