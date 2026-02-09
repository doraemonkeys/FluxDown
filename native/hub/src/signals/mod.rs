use rinf::{DartSignal, RustSignal, SignalPiece};
use serde::{Deserialize, Serialize};

// ========== Dart → Rust signals ==========

/// Create a new download task
#[derive(Deserialize, DartSignal)]
pub struct CreateTask {
    pub url: String,
    pub save_dir: String,
    pub file_name: String, // empty = auto detect from server
    pub segments: i32,     // 0 = auto (default 8)
}

/// Control an existing task (pause/resume/cancel/delete)
#[derive(Deserialize, DartSignal)]
pub struct ControlTask {
    pub task_id: String,
    pub action: i32, // 0=pause, 1=resume, 2=cancel, 3=delete(+files), 4=delete(record only)
}

/// Request all persisted tasks (sent on app startup)
#[derive(Deserialize, DartSignal)]
pub struct RequestAllTasks {}

// ========== Rust → Dart signals ==========

/// Task progress update — sent periodically during download
#[derive(Serialize, RustSignal)]
pub struct TaskProgress {
    pub task_id: String,
    pub status: i32, // 0=pending, 1=downloading, 2=paused, 3=completed, 4=error
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub speed: i64, // bytes per second
    pub file_name: String,
    pub save_dir: String,
    pub url: String,
    pub error_message: String, // empty if no error
}

/// Response to RequestAllTasks — all persisted tasks
#[derive(Serialize, RustSignal)]
pub struct AllTasks {
    pub tasks: Vec<TaskInfo>,
}

/// Segment-level progress for download visualization (IDM-style)
#[derive(Serialize, RustSignal)]
pub struct SegmentProgress {
    pub task_id: String,
    pub total_bytes: i64,
    /// Number of segments (1 = single-thread download)
    pub segment_count: i32,
    pub segments: Vec<SegmentDetail>,
}

/// Per-segment byte range and progress
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct SegmentDetail {
    pub index: i32,
    pub start_byte: i64,
    pub end_byte: i64,
    pub downloaded_bytes: i64,
}

// ========== External download signals (browser extension → app) ==========

/// Notification to Dart that a download request arrived from the browser
/// extension via Native Messaging.  The Flutter UI should pop up a quick
/// confirmation dialog (independent download window).
#[derive(Serialize, RustSignal)]
pub struct ExternalDownloadRequest {
    pub url: String,
    pub filename: String,
    pub referrer: String,
    pub file_size: i64,    // 0 = unknown
    pub mime_type: String, // empty = unknown
}

/// Dart → Rust: user confirmed the external download request.
#[derive(Deserialize, DartSignal)]
pub struct ConfirmExternalDownload {
    pub url: String,
    pub save_dir: String,
    pub file_name: String, // empty = auto detect
    pub segments: i32,     // 0 = auto
}

// ========== Config signals ==========

/// Save a single config entry (Dart → Rust)
#[derive(Deserialize, DartSignal)]
pub struct SaveConfig {
    pub key: String,
    pub value: String,
}

/// Request all config entries (Dart → Rust, sent on app startup)
#[derive(Deserialize, DartSignal)]
pub struct RequestConfig {}

/// All config entries loaded from DB (Rust → Dart)
#[derive(Serialize, RustSignal)]
pub struct ConfigLoaded {
    pub entries: Vec<ConfigEntry>,
}

/// Single config key-value pair
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

/// Nested task info piece
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct TaskInfo {
    pub task_id: String,
    pub url: String,
    pub file_name: String,
    pub save_dir: String,
    pub status: i32, // 0=pending, 1=downloading, 2=paused, 3=completed, 4=error
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub error_message: String,
}
