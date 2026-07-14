//! 脚本运行时抽象层。**本文件禁止出现任何 rquickjs 类型**——v1 由
//! [`super::quickjs`] 实现；未来 deno_core 实现同一 trait。
//!
//! dyn 兼容论证同 `selection.rs`：`Engine` 以 `Arc<dyn>` 存字段跨任务共享。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 单次脚本调用的资源预算。`timeout` 由外层 `tokio::time::timeout` 强制（不依赖
/// QuickJS 检查点），`memory_limit_bytes` 交给运行时的内存限制器。
#[derive(Debug, Clone, Copy)]
pub struct ExecutionBudget {
    pub timeout: Duration,
    pub memory_limit_bytes: usize,
}

/// 已加载脚本的最小执行单元 —— 只承载 identity/源码/入口种类，与具体运行时无关。
#[derive(Debug, Clone)]
pub struct PluginScript {
    pub identity: String,
    pub source: String,
    pub entry_fn_hint: PluginEntryKind,
    /// 插件自身版本（供 `flux.info.version`）。
    pub version: String,
    /// 宿主 App 版本（供 `flux.info.appVersion`）。
    pub app_version: String,
}

/// 入口函数种类，决定运行时从 `globalThis` 取哪个函数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginEntryKind {
    /// resolver 入口：`globalThis.resolve`。
    Resolve,
    /// hook 入口：`globalThis.onStart/onError/onDone/onMetaProbed`（由 event 决定）。
    Hook,
}

// ---------------------------------------------------------------------------
// 跨 JS 边界结构：统一 #[serde(rename_all="camelCase")]，JS 侧字段名即 camelCase。
// ---------------------------------------------------------------------------

/// 传入 `resolve(ctx)` 的请求上下文。`url` 恒为 source_url（原始任务 URL）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveRequest {
    pub task_id: String,
    pub url: String,
    pub cookies: String,
    pub referrer: String,
    pub user_agent: String,
    pub extra_headers: HashMap<String, String>,
}

/// `resolve(ctx)` 的返回值。返回 `null`/`undefined` 表示放行不改写（映射为
/// `Ok(None)`）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResolveResult {
    /// 改写后的直链。
    pub url: String,
    /// 可选音频直链（用于 DASH 音视频分离场景）。
    pub audio_url: Option<String>,
    pub file_name: Option<String>,
    pub total_bytes: Option<i64>,
    pub extra_headers: Option<HashMap<String, String>>,
    /// 直链为一次性/防探测签名 URL 时置 true → 跳过 probe（牺牲 If-Range）；
    /// 默认 false → 正常 probe 取 ETag，保 resume 一致性。
    pub ephemeral: bool,
    /// 插件担保该直链所在服务支持 HTTP Range（如 googlevideo）。与 `ephemeral`
    /// 正交：`ephemeral` 表达"probe 会作废直链"，本字段表达"Range 请求安全"。
    /// 置 true 时引擎跳过 probe 的同时仍按已验证 Range 规划多段并发下载，
    /// 不落入配额型端点（fnOS）的保守单流启动。默认 false（保守）。
    pub range_supported: bool,
}

/// 通知事件。每个变体都带 `url`（= source_url），供 `notify()` 的 `match.urls` 过滤。
#[derive(Debug, Clone, Serialize)]
// `rename_all` 只重命名变体名；变体内字段须 `rename_all_fields` 才 camelCase 化
// （否则 JS 侧 ctx.task_id 而非 ctx.taskId，与 ResolveRequest 结构体不一致）。
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event"
)]
pub enum PluginEvent {
    Start {
        task_id: String,
        url: String,
    },
    Error {
        task_id: String,
        url: String,
        message: String,
    },
    Done {
        task_id: String,
        url: String,
        file_path: String,
        /// 轨对任务 mux 失败降级时，独立音频 sidecar 的绝对路径
        /// （`<stem>.audio.m4a`）；单文件产物（含 mux 成功）为 `None`。
        audio_path: Option<String>,
        /// 轨对任务（视频+音频离散轨）是否成功 mux 为单文件；非轨对任务恒 `false`。
        muxed: bool,
    },
    MetaProbed {
        task_id: String,
        url: String,
        file_name: String,
        total_bytes: i64,
    },
}

impl PluginEvent {
    /// 事件类型对应的 JS 全局函数名。
    pub fn hook_fn_name(&self) -> &'static str {
        match self {
            PluginEvent::Start { .. } => "onStart",
            PluginEvent::Error { .. } => "onError",
            PluginEvent::Done { .. } => "onDone",
            PluginEvent::MetaProbed { .. } => "onMetaProbed",
        }
    }

    /// 事件在 manifest `events` 声明中的名字（与 `hook_fn_name` 相同）。
    pub fn declared_name(&self) -> &'static str {
        self.hook_fn_name()
    }

    /// 事件的 source_url，供 match.urls 过滤。
    pub fn url(&self) -> &str {
        match self {
            PluginEvent::Start { url, .. }
            | PluginEvent::Error { url, .. }
            | PluginEvent::Done { url, .. }
            | PluginEvent::MetaProbed { url, .. } => url,
        }
    }
}

/// 插件日志级别，转发到文件日志。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLogLevel {
    Info,
    Warn,
    Error,
}

/// 插件系统错误。`Overloaded`/`Timeout`/`MemoryLimitExceeded` 是 fail-closed 触发点。
#[derive(thiserror::Error, Debug)]
pub enum PluginError {
    #[error("manifest 非法: {0}")]
    ManifestInvalid(String),
    #[error("脚本编译失败: {0}")]
    CompileFailed(String),
    #[error("脚本执行超时")]
    Timeout,
    #[error("脚本超出内存上限")]
    MemoryLimitExceeded,
    #[error("插件运行时过载（并发已满）")]
    Overloaded,
    #[error("插件输出非法: {0}")]
    InvalidOutput(String),
    #[error("缺少必填设置项: {0}")]
    MissingRequiredSetting(String),
    #[error("插件运行时错误: {0}")]
    Runtime(String),
}

// ---------------------------------------------------------------------------
// bridge 侧 HTTP 结构：JS 侧 flux.fetch(opts) 的 opts / 返回值字段名 == camelCase。
// ---------------------------------------------------------------------------

/// `flux.fetch(opts)` 的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BridgeHttpRequest {
    /// 默认 GET。
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

impl Default for BridgeHttpRequest {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            url: String::new(),
            headers: HashMap::new(),
            body: None,
        }
    }
}

/// `flux.fetch(opts)` 的返回值。`body` 为文本；二进制场景 v1 不支持。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// ffmpeg bridge：单一 near-raw argv 面（flux.ffmpeg.run / .available）。
// ---------------------------------------------------------------------------

/// `flux.ffmpeg.run(spec)` 的请求。**近乎全量 ffmpeg CLI**：`args` 直传给 ffmpeg
/// 二进制（不含程序名），仅经 bridge 侧「封网 + 封越牢路径」校验（见
/// [`super::bridge`]）。文件引用一律用**相对路径**（相对 cwd = 牢笼根/`subdir`），
/// 绝对路径 / `..` / URL scheme 会被拒。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FfmpegSpec {
    /// ffmpeg 参数数组（不含二进制名与自动注入的 `-nostdin`）。
    pub args: Vec<String>,
    /// 牢笼根下的工作子目录（可空；须为安全相对路径）。缺省时 cwd = 牢笼根本身。
    pub subdir: Option<String>,
    /// 本次调用超时（毫秒）。缺省取 bridge 默认值，并被 bridge 上限裁剪。
    pub timeout_ms: Option<u64>,
}

/// `flux.ffmpeg.run(spec)` 的返回值。`stdout`/`stderr` 均按 bridge 上限截断。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegOutcome {
    /// 进程退出码（被信号杀死或无码时为 -1）。
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    /// 命中超时被强杀。
    pub timed_out: bool,
    pub truncated_stdout: bool,
    pub truncated_stderr: bool,
}

/// `flux.ffmpeg.available()` 的返回值（探测生效 ffmpeg 路径）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegAvailability {
    pub available: bool,
    /// `ffmpeg -version` 探测到的版本串（不可用时为空）。
    pub version: String,
    /// 路径来源：`"manual"`/`"managed"`/`"system"`/`"none"`。
    pub source: String,
}

/// 单次插件调用的**宿主侧上下文**（不跨 JS 边界，插件不可设置）。承载 ffmpeg
/// 能力门 + 文件访问牢笼根，由 manager 按调用上下文（事件/resolve）注入。
#[derive(Debug, Clone, Default)]
pub struct HostContext {
    /// manifest `permissions` 是否含 `"ffmpeg"`——决定是否注入 `flux.ffmpeg` 门面。
    pub ffmpeg_permitted: bool,
    /// ffmpeg 允许读写的牢笼根（通常为任务 `save_dir`）。`None` = 无牢笼
    /// （resolve / 无产物事件）→ `flux.ffmpeg.run` 一律被拒。
    pub ffmpeg_root: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// trait
// ---------------------------------------------------------------------------

/// 脚本运行时抽象。v1 由 quickjs 实现；未来 deno_core 实现同一 trait。
#[async_trait::async_trait]
pub trait ScriptRuntime: Send + Sync {
    /// 编译期检查源码语法（不执行副作用）。
    fn check_compile(&self, source: &str) -> Result<(), PluginError>;

    /// 用 JS `RegExp` 校验 pattern 语法是否合法（能否 `new RegExp(pattern)`）。
    fn regex_valid(&self, pattern: &str) -> bool;

    /// 用 JS `RegExp` 测试 `value` 是否匹配 `pattern`；pattern 非法时返回 false。
    fn regex_test(&self, pattern: &str, value: &str) -> bool;

    /// 调用 `globalThis.resolve(ctx)`。返回 `Ok(None)` = 放行不改写。
    /// `settings_json` 为 manager 预构建的**类型化**只读设置 JSON 对象字符串
    /// （string→JS string、number→JS number、boolean→JS boolean），注入为
    /// `flux.settings`。
    async fn invoke_resolve(
        &self,
        plugin: &PluginScript,
        req: ResolveRequest,
        settings_json: String,
        bridge: Arc<dyn PluginBridge>,
        budget: ExecutionBudget,
        host: HostContext,
    ) -> Result<Option<ResolveResult>, PluginError>;

    /// 通知钩子；**全部事件（含 Error）统一 fire-and-forget，实现方吞掉一切错误
    /// （仅日志），无返回值**。重试意图由脚本经 [`PluginBridge::request_retry`]
    /// 命令式发起，不走返回值通道。`settings_json` 同 [`Self::invoke_resolve`]。
    async fn invoke_hook(
        &self,
        plugin: &PluginScript,
        event: PluginEvent,
        settings_json: String,
        bridge: Arc<dyn PluginBridge>,
        budget: ExecutionBudget,
        host: HostContext,
    );

    /// 供 off-actor worker `spawn` 用的 tokio `Handle`（专用 multi_thread runtime）。
    /// **禁止裸 `tokio::spawn`**——那会把 resolve future 落到 hub 的 current_thread
    /// 唯一线程上、冻结全命令面。
    fn spawn_handle(&self) -> tokio::runtime::Handle;
}

/// 宿主向脚本暴露的能力桥。`flux.settings` 不在此——它由 manager 从 manifest+config
/// 构建为类型化 JSON 后经 invoke 方法传入（bridge 无 manifest 语义，无法做类型化）。
#[async_trait::async_trait]
pub trait PluginBridge: Send + Sync {
    /// `flux.fetch`：经守卫 Client 发 HTTP 请求（防 SSRF）。
    async fn http_request(
        &self,
        plugin_id: &str,
        req: BridgeHttpRequest,
    ) -> Result<BridgeHttpResponse, PluginError>;

    /// `flux.storage.get`。
    async fn storage_get(&self, plugin_id: &str, key: &str) -> Option<String>;

    /// `flux.storage.set`。值 ≤64KB、单插件 ≤100 键。
    async fn storage_set(
        &self,
        plugin_id: &str,
        key: &str,
        value: String,
    ) -> Result<(), PluginError>;

    /// `flux.logger.*` / `console.*`。
    fn log(&self, plugin_id: &str, level: PluginLogLevel, message: &str);

    /// 命令式重试请求（onError 钩子专用，复刻 gopeed `ctx.task.continue()`）：
    /// 内部经 `plugin_retry_tx` 通道发起延迟 resume；限流在 actor 侧
    /// （`max_auto_retries`）；不阻塞、不返回决策。
    fn request_retry(&self, task_id: &str, delay_ms: u64);

    /// `flux.task.recordArtifact(name)`：登记任务的衍生产物文件名（同
    /// `save_dir` 下的相对文件名，如转码产物 `<stem>.mp4`）。onDone 钩子专用；
    /// 登记后「删除任务并删除文件」会连同产物一并删除，保证单一任务的所有
    /// 文件成组管理。默认实现拒绝（无持久化能力的 bridge）。
    async fn record_artifact(
        &self,
        _plugin_id: &str,
        _task_id: &str,
        _file_name: &str,
    ) -> Result<(), PluginError> {
        Err(PluginError::Runtime("此 bridge 不支持产物登记".to_string()))
    }

    /// `flux.ffmpeg.available()`：探测生效 ffmpeg（manual→managed→system）。
    /// 只读、不触网、不落盘。默认实现返回 `None`（无 ffmpeg 支持的 bridge）。
    async fn ffmpeg_available(&self) -> Option<FfmpegAvailability> {
        None
    }

    /// `flux.ffmpeg.run(spec)`：在 `jail_root` 牢笼内执行 ffmpeg。
    ///
    /// `jail_root` 由宿主按调用上下文注入（见 [`HostContext::ffmpeg_root`]），
    /// **插件无法设置**；实现方须把一切文件访问约束在 `jail_root` 内、并封死
    /// 网络协议（见 [`super::bridge`] 的校验器）。默认实现拒绝调用。
    async fn run_ffmpeg(
        &self,
        _plugin_id: &str,
        _jail_root: PathBuf,
        _spec: FfmpegSpec,
    ) -> Result<FfmpegOutcome, PluginError> {
        Err(PluginError::Runtime("此 bridge 不支持 ffmpeg".to_string()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{PluginEvent, ResolveRequest};
    use std::collections::HashMap;

    #[test]
    fn plugin_event_fields_are_camel_case() {
        // 通知事件跨 JS 边界的字段名必须 camelCase（与 hooks.js 的 ctx.taskId 等一致）。
        let done = PluginEvent::Done {
            task_id: "t1".into(),
            url: "http://x/".into(),
            file_path: "/tmp/a.bin".into(),
            audio_path: Some("/tmp/a.audio.m4a".into()),
            muxed: false,
        };
        let v: serde_json::Value = serde_json::to_value(&done).expect("serialize");
        assert_eq!(v["taskId"], "t1");
        assert_eq!(v["filePath"], "/tmp/a.bin");
        assert_eq!(v["audioPath"], "/tmp/a.audio.m4a");
        assert_eq!(v["muxed"], false);
        assert!(v.get("audio_path").is_none());
        assert!(
            v.get("task_id").is_none(),
            "must not emit snake_case task_id"
        );
        assert!(v.get("file_path").is_none());

        let meta = PluginEvent::MetaProbed {
            task_id: "t2".into(),
            url: "http://y/".into(),
            file_name: "b.mp4".into(),
            total_bytes: 42,
        };
        let mv: serde_json::Value = serde_json::to_value(&meta).expect("serialize");
        assert_eq!(mv["fileName"], "b.mp4");
        assert_eq!(mv["totalBytes"], 42);
    }

    #[test]
    fn resolve_request_fields_are_camel_case() {
        let req = ResolveRequest {
            task_id: "t".into(),
            url: "u".into(),
            cookies: String::new(),
            referrer: String::new(),
            user_agent: "UA".into(),
            extra_headers: HashMap::new(),
        };
        let v: serde_json::Value = serde_json::to_value(&req).expect("serialize");
        assert_eq!(v["taskId"], "t");
        assert_eq!(v["userAgent"], "UA");
        assert!(v.get("extra_headers").is_none());
        assert!(v.get("extraHeaders").is_some());
    }
}
