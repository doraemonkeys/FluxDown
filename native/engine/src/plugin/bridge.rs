//! `EngineBridge` —— [`PluginBridge`] 实现：网络出口守卫 + storage + log + 重试意图。
//!
//! ## 网络出口守卫（防 SSRF）
//! 保护本机与 headless server 不被恶意/有 bug 的第三方插件访问内部服务。核心是
//! **单一判定函数** [`is_globally_routable_unicast`]，供三处复用（杜绝判定漂移）：
//! 1. 字面量 IP 前置校验（进入 reqwest 之前，挡 hyper-util 对字面量 IP 的短路）；
//! 2. 自定义 `reqwest::dns::Resolve`（解析后过滤，挡 DNS rebinding、消 TOCTOU）；
//! 3. 逐跳重定向 `Policy::custom`（手动重建 30 跳上限 + 每跳字面量 IP 校验）。
//!
//! ## v1 限制（记录在案）
//! - `proxy` 在 bridge 构造时快照（reqwest ClientBuilder 配置构建时定死）；运行期改
//!   代理后插件出口不随动（可接受，非安全问题）。
//! - 单次调用严格 per-call fetch 上限退化为**全局并发 fetch 上限**（对宿主保护更强）。
//! - 配置代理时 DNS 由代理侧解析，[`GuardResolver`] 不参与（hostname 级过滤失效；
//!   字面量 IP 前置校验与逐跳重定向校验仍然生效）。代理由用户显式配置，视为可信出口。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::{Semaphore, mpsc};

use crate::db::Db;
use crate::logger::{log_error, log_info};
use crate::proxy_config::ProxyConfig;

use super::runtime::{
    BridgeHttpRequest, BridgeHttpResponse, FfmpegAvailability, FfmpegOutcome, FfmpegSpec,
    PluginBridge, PluginError, PluginLogLevel,
};

/// 响应体上限（超限截断 + `truncated:true`）。
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
/// 单请求超时。
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// 全局并发 fetch 上限。
const MAX_CONCURRENT_FETCH: usize = 8;
/// 重定向跳数上限（Policy::custom 丢失 limited 默认保护，手动重建）。
const MAX_REDIRECTS: usize = 30;
/// 单值上限 64KB。
const MAX_STORAGE_VALUE: usize = 64 * 1024;
/// 单插件 storage 键数上限。
const MAX_STORAGE_KEYS: usize = 100;
/// 单条日志截断长度。
const MAX_LOG_BYTES: usize = 4 * 1024;
/// ffmpeg 单次调用默认超时（缺省 `timeoutMs` 时）。
const FFMPEG_DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// ffmpeg 单次调用超时上限（裁剪 `timeoutMs`）。
const FFMPEG_MAX_TIMEOUT: Duration = Duration::from_secs(1800);
/// 全局并发 ffmpeg 进程上限（CPU/IO 密集，保守取 2）。
const MAX_CONCURRENT_FFMPEG: usize = 2;
/// ffmpeg 参数条数上限。
const MAX_FFMPEG_ARGS: usize = 512;
/// ffmpeg 单参数字节上限。
const MAX_FFMPEG_ARG_LEN: usize = 8 * 1024;
/// ffmpeg stdout 回传上限（够 ffprobe 式 JSON；超限截断）。
const FFMPEG_STDOUT_CAP: usize = 256 * 1024;
/// ffmpeg stderr 回传上限（超限截断）。
const FFMPEG_STDERR_CAP: usize = 64 * 1024;

/// 唯一的「可全局路由单播」判定函数。三处复用，杜绝判定逻辑漂移。
///
/// 拒绝一切环回/私网/链路本地/文档/保留/元数据段等非公网单播地址。
///
/// # Examples
///
/// ```
/// use std::net::IpAddr;
/// use fluxdown_engine::plugin::bridge::is_globally_routable_unicast;
///
/// assert!(is_globally_routable_unicast("8.8.8.8".parse::<IpAddr>().unwrap()));
/// assert!(!is_globally_routable_unicast("127.0.0.1".parse::<IpAddr>().unwrap()));
/// assert!(!is_globally_routable_unicast("169.254.169.254".parse::<IpAddr>().unwrap()));
/// ```
pub fn is_globally_routable_unicast(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_v4_routable(v4),
        IpAddr::V6(v6) => is_v6_routable(v6),
    }
}

fn is_v4_routable(ip: Ipv4Addr) -> bool {
    if ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_documentation()
    {
        return false;
    }
    let o = ip.octets();
    // 0.0.0.0/8 "this network"
    if o[0] == 0 {
        return false;
    }
    // 100.64.0.0/10 CGNAT
    if o[0] == 100 && (o[1] & 0xc0) == 0x40 {
        return false;
    }
    // 198.18.0.0/15 benchmarking
    if o[0] == 198 && (o[1] & 0xfe) == 18 {
        return false;
    }
    // 240.0.0.0/4 reserved
    if (o[0] & 0xf0) == 240 {
        return false;
    }
    true
}

fn is_v6_routable(ip: Ipv6Addr) -> bool {
    // IPv4-mapped ::ffff:a.b.c.d → 解包递归。
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_v4_routable(v4);
    }
    let seg = ip.segments();
    // 6to4 2002::/16 → 内嵌 IPv4 递归。
    if seg[0] == 0x2002 {
        let v4 = Ipv4Addr::new(
            (seg[1] >> 8) as u8,
            (seg[1] & 0xff) as u8,
            (seg[2] >> 8) as u8,
            (seg[2] & 0xff) as u8,
        );
        return is_v4_routable(v4);
    }
    // NAT64 64:ff9b::/96 → 末 32bit 递归。
    if seg[0] == 0x0064 && seg[1] == 0xff9b {
        let v4 = Ipv4Addr::new(
            (seg[6] >> 8) as u8,
            (seg[6] & 0xff) as u8,
            (seg[7] >> 8) as u8,
            (seg[7] & 0xff) as u8,
        );
        return is_v4_routable(v4);
    }
    if ip.is_loopback()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
    {
        return false;
    }
    // Teredo 2001:0000::/32
    if seg[0] == 0x2001 && seg[1] == 0x0000 {
        return false;
    }
    // documentation 2001:0db8::/32
    if seg[0] == 0x2001 && seg[1] == 0x0db8 {
        return false;
    }
    true
}

/// 守卫用自定义 DNS 解析器：解析后仅保留可全局路由的地址。
struct GuardResolver;

impl reqwest::dns::Resolve for GuardResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let filtered: Vec<SocketAddr> = addrs
                .filter(|sa| is_globally_routable_unicast(sa.ip()))
                .collect();
            let iter: reqwest::dns::Addrs = Box::new(filtered.into_iter());
            Ok(iter)
        })
    }
}

#[derive(Debug, thiserror::Error)]
enum GuardError {
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("blocked: non-routable redirect target")]
    BlockedRedirect,
}

/// 引擎侧 `PluginBridge` 实现。
pub struct EngineBridge {
    client: reqwest::Client,
    db: Db,
    plugin_retry_tx: mpsc::UnboundedSender<(String, u64)>,
    fetch_sema: Arc<Semaphore>,
    /// 数据目录：供 `flux.ffmpeg` 解析生效 ffmpeg 路径（manual→managed→system）。
    data_dir: PathBuf,
    /// 全局并发 ffmpeg 进程限流。
    ffmpeg_sema: Arc<Semaphore>,
}

impl EngineBridge {
    /// 构造带守卫的 bridge。`proxy` 在此快照进 Client（v1 限制，见模块文档）。
    pub fn new(
        db: Db,
        proxy: &ProxyConfig,
        plugin_retry_tx: mpsc::UnboundedSender<(String, u64)>,
        data_dir: PathBuf,
    ) -> Result<Self, PluginError> {
        let mut builder = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .dns_resolver(Arc::new(GuardResolver))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.error(GuardError::TooManyRedirects);
                }
                if let Some(host) = attempt.url().host_str() {
                    let trimmed = host.trim_matches(|c| c == '[' || c == ']');
                    if let Ok(ip) = trimmed.parse::<IpAddr>()
                        && !is_globally_routable_unicast(ip)
                    {
                        return attempt.error(GuardError::BlockedRedirect);
                    }
                }
                attempt.follow()
            }));

        if let Some(url) = proxy.resolve().to_proxy_url()
            && let Ok(p) = reqwest::Proxy::all(&url)
        {
            builder = builder.proxy(p);
        }

        let client = builder
            .build()
            .map_err(|e| PluginError::Runtime(format!("构建守卫 Client 失败: {e}")))?;
        Ok(Self {
            client,
            db,
            plugin_retry_tx,
            fetch_sema: Arc::new(Semaphore::new(MAX_CONCURRENT_FETCH)),
            data_dir,
            ffmpeg_sema: Arc::new(Semaphore::new(MAX_CONCURRENT_FFMPEG)),
        })
    }
}

#[async_trait::async_trait]
impl PluginBridge for EngineBridge {
    async fn http_request(
        &self,
        _plugin_id: &str,
        req: BridgeHttpRequest,
    ) -> Result<BridgeHttpResponse, PluginError> {
        // scheme 仅 http/https。
        let parsed = url::Url::parse(&req.url)
            .map_err(|e| PluginError::InvalidOutput(format!("URL 非法: {e}")))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(PluginError::InvalidOutput(
                "flux.fetch 仅支持 http/https".to_string(),
            ));
        }
        // 字面量 IP 前置校验（挡 hyper-util 对字面量 IP 的短路）。
        if let Some(host) = parsed.host_str() {
            let trimmed = host.trim_matches(|c| c == '[' || c == ']');
            if let Ok(ip) = trimmed.parse::<IpAddr>()
                && !is_globally_routable_unicast(ip)
            {
                return Err(PluginError::InvalidOutput(
                    "blocked: non-routable IP".to_string(),
                ));
            }
        }

        let permit = self
            .fetch_sema
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| PluginError::Runtime("fetch semaphore closed".to_string()))?;
        let _permit = permit;

        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|_| PluginError::InvalidOutput(format!("HTTP method 非法: {}", req.method)))?;
        let mut rb = self.client.request(method, parsed);
        for (k, v) in &req.headers {
            if let (Ok(name), Ok(value)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                reqwest::header::HeaderValue::from_str(v),
            ) {
                rb = rb.header(name, value);
            }
        }
        if let Some(body) = req.body {
            rb = rb.body(body);
        }

        let mut resp = rb
            .send()
            .await
            .map_err(|e| PluginError::Runtime(format!("fetch 失败: {e}")))?;
        let status = resp.status().as_u16();
        let mut headers = std::collections::HashMap::new();
        for (k, v) in resp.headers() {
            if let Ok(s) = v.to_str() {
                headers.insert(k.as_str().to_string(), s.to_string());
            }
        }

        let mut body = Vec::new();
        let mut truncated = false;
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > MAX_BODY_BYTES {
                        let take = MAX_BODY_BYTES - body.len();
                        body.extend_from_slice(&chunk[..take]);
                        truncated = true;
                        break;
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(PluginError::Runtime(format!("读取响应体失败: {e}"))),
            }
        }

        Ok(BridgeHttpResponse {
            status,
            headers,
            body: String::from_utf8_lossy(&body).to_string(),
            truncated,
        })
    }

    async fn storage_get(&self, plugin_id: &str, key: &str) -> Option<String> {
        let full = format!("plugin.{plugin_id}.kv.{key}");
        self.db.get_config(&full).await.ok().flatten()
    }

    async fn storage_set(
        &self,
        plugin_id: &str,
        key: &str,
        value: String,
    ) -> Result<(), PluginError> {
        if value.len() > MAX_STORAGE_VALUE {
            return Err(PluginError::InvalidOutput(format!(
                "storage 值超过 {MAX_STORAGE_VALUE} 字节上限"
            )));
        }
        let prefix = format!("plugin.{plugin_id}.kv.");
        let full = format!("{prefix}{key}");
        // 键数上限：仅当是新键时才计数。
        if let Ok(existing) = self.db.list_config_with_prefix(&prefix).await {
            let is_new = !existing.iter().any(|(k, _)| k == &full);
            if is_new && existing.len() >= MAX_STORAGE_KEYS {
                return Err(PluginError::InvalidOutput(format!(
                    "storage 键数超过 {MAX_STORAGE_KEYS} 上限"
                )));
            }
        }
        self.db
            .set_config(&full, &value)
            .await
            .map_err(|e| PluginError::Runtime(format!("storage 写入失败: {e}")))
    }

    fn log(&self, plugin_id: &str, level: PluginLogLevel, message: &str) {
        let truncated = if message.len() > MAX_LOG_BYTES {
            // 按字符边界安全截断。
            let mut end = MAX_LOG_BYTES;
            while end > 0 && !message.is_char_boundary(end) {
                end -= 1;
            }
            &message[..end]
        } else {
            message
        };
        match level {
            PluginLogLevel::Error => log_error!("[plugin:{}] {}", plugin_id, truncated),
            _ => log_info!("[plugin:{}] {}", plugin_id, truncated),
        }
    }

    fn request_retry(&self, task_id: &str, delay_ms: u64) {
        // fire-and-forget；限流在 actor 侧（max_auto_retries）。
        let _ = self.plugin_retry_tx.send((task_id.to_string(), delay_ms));
    }

    async fn record_artifact(
        &self,
        plugin_id: &str,
        task_id: &str,
        file_name: &str,
    ) -> Result<(), PluginError> {
        // 仅接受单层裸文件名（无路径分隔/盘符/`..`）；删除侧还有
        // `is_safe_file_name` 二次校验，双保险。
        let bad = file_name.is_empty()
            || file_name.len() > 512
            || file_name.contains(['/', '\\', ':'])
            || file_name == "."
            || file_name == "..";
        if bad {
            return Err(PluginError::Runtime(format!(
                "recordArtifact: 非法产物文件名: {file_name:?}"
            )));
        }
        self.db
            .add_task_artifact(task_id, file_name)
            .await
            .map_err(|e| PluginError::Runtime(format!("recordArtifact 落库失败: {e}")))?;
        log_info!(
            "[plugin:{}] recordArtifact: task={} file={}",
            plugin_id,
            task_id,
            file_name
        );
        Ok(())
    }

    async fn ffmpeg_available(&self) -> Option<FfmpegAvailability> {
        let status = crate::components::ffmpeg_status(&self.db, &self.data_dir).await;
        Some(FfmpegAvailability {
            available: !status.path.is_empty(),
            version: status.version,
            source: status.source.as_str().to_string(),
        })
    }

    async fn run_ffmpeg(
        &self,
        _plugin_id: &str,
        jail_root: PathBuf,
        spec: FfmpegSpec,
    ) -> Result<FfmpegOutcome, PluginError> {
        // 1) 参数校验（封网 + 封越牢路径；近乎全量 CLI）。先于二进制解析 fail-fast。
        if spec.args.is_empty() {
            return Err(PluginError::InvalidOutput(
                "ffmpeg args 不可为空".to_string(),
            ));
        }
        if spec.args.len() > MAX_FFMPEG_ARGS {
            return Err(PluginError::InvalidOutput(format!(
                "ffmpeg 参数过多（>{MAX_FFMPEG_ARGS}）"
            )));
        }
        validate_ffmpeg_args(&spec.args)?;

        // 2) 生效 ffmpeg 二进制（manual→managed→system；不触网）。
        let bin = crate::components::resolve_ffmpeg(&self.db, &self.data_dir)
            .await
            .ok_or_else(|| PluginError::Runtime("ffmpeg 未安装或不可用".to_string()))?;

        // 3) 工作目录：牢笼根（canonicalize 后）+ 可选安全 subdir，禁逃逸。
        let jail = tokio::fs::canonicalize(&jail_root)
            .await
            .map_err(|e| PluginError::Runtime(format!("ffmpeg 牢笼根无效: {e}")))?;
        let work = match spec.subdir.as_deref() {
            Some(sub) if !sub.is_empty() => {
                if !super::manifest::is_safe_relative_path(sub) {
                    return Err(PluginError::InvalidOutput(format!(
                        "ffmpeg subdir '{sub}' 非法"
                    )));
                }
                let cand = jail.join(sub);
                tokio::fs::create_dir_all(&cand)
                    .await
                    .map_err(|e| PluginError::Runtime(format!("创建 ffmpeg subdir 失败: {e}")))?;
                let real = tokio::fs::canonicalize(&cand)
                    .await
                    .map_err(|e| PluginError::Runtime(format!("ffmpeg subdir 无效: {e}")))?;
                if !real.starts_with(&jail) {
                    return Err(PluginError::InvalidOutput(
                        "ffmpeg subdir 逃逸牢笼".to_string(),
                    ));
                }
                real
            }
            _ => jail.clone(),
        };

        // 4) 超时（裁剪到上限）。
        let timeout = spec
            .timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(FFMPEG_DEFAULT_TIMEOUT)
            .min(FFMPEG_MAX_TIMEOUT);

        // 5) 并发限流。
        let _permit = self
            .ffmpeg_sema
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| PluginError::Runtime("ffmpeg semaphore closed".to_string()))?;

        // 6) 启动。`-nostdin` 前置注入；stdin=null；kill_on_drop 保超时/取消时清进程。
        let mut cmd = Command::new(&bin);
        cmd.current_dir(&work)
            .arg("-nostdin")
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = cmd
            .spawn()
            .map_err(|e| PluginError::Runtime(format!("启动 ffmpeg 失败: {e}")))?;
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(PluginError::Runtime(format!("ffmpeg 执行失败: {e}"))),
            Err(_) => {
                // 超时：future 被 drop → kill_on_drop 杀子进程。
                return Ok(FfmpegOutcome {
                    code: -1,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: true,
                    truncated_stdout: false,
                    truncated_stderr: false,
                });
            }
        };
        let (stdout, truncated_stdout) = truncate_utf8(&output.stdout, FFMPEG_STDOUT_CAP);
        let (stderr, truncated_stderr) = truncate_utf8(&output.stderr, FFMPEG_STDERR_CAP);
        Ok(FfmpegOutcome {
            code: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
            timed_out: false,
            truncated_stdout,
            truncated_stderr,
        })
    }
}

/// 校验 ffmpeg 参数：仅封堵网络协议与越牢路径引用，其余（滤镜/编码器/复用器
/// /元数据…）近乎全量放行。文件引用一律相对 cwd（牢笼根/subdir）。
fn validate_ffmpeg_args(args: &[String]) -> Result<(), PluginError> {
    for a in args {
        if a.len() > MAX_FFMPEG_ARG_LEN {
            return Err(PluginError::InvalidOutput("ffmpeg 参数过长".to_string()));
        }
        if a.contains('\0') {
            return Err(PluginError::InvalidOutput("ffmpeg 参数含 NUL".to_string()));
        }
        if let Some(reason) = arg_reject_reason(a) {
            return Err(PluginError::InvalidOutput(format!(
                "ffmpeg 参数 '{a}' 被拒: {reason}"
            )));
        }
    }
    Ok(())
}

/// 单参数拒绝原因（`None` = 放行）。判定：绝对路径 / 盘符 / `..` / URL scheme /
/// 协议前缀 / 内嵌绝对路径。除法（`30000/1001`）、流选择器（`0:a`/`-c:v`）、
/// 滤镜分隔（`scale=1280:720`）等合法语法均放行。
fn arg_reject_reason(a: &str) -> Option<&'static str> {
    // 绝对路径 / 分隔符开头。
    if a.starts_with('/') || a.starts_with('\\') {
        return Some("绝对路径");
    }
    // Windows 盘符（X: 开头，含 UNC 前缀由上面的 `\` 覆盖）。
    let b = a.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return Some("盘符路径");
    }
    // `..` 路径段（含内嵌 `foo/../bar`）。
    if a.split(['/', '\\']).any(|seg| seg == "..") {
        return Some(".. 越级");
    }
    // 显式 URL（http:// / file:// / rtmp:// …）。
    if a.contains("://") {
        return Some("URL scheme");
    }
    // 无 `//` 的协议前缀（file:/concat:/crypto:/data:/pipe:/subfile: …）：首个 `:`
    // 前缀是 ≥2 位、字母起头的合法 scheme 字符集时判为协议。`-c:v`（`-` 起头）、
    // `0:a`（数字/单字符）、`scale=…:…`（含 `=`）均不满足，放行。
    if let Some(idx) = a.find(':') {
        let scheme = &a[..idx];
        if scheme.len() >= 2
            && scheme.as_bytes()[0].is_ascii_alphabetic()
            && scheme
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'.' || c == b'-')
        {
            return Some("协议前缀");
        }
    }
    // 选项值内嵌的绝对路径（如 `subtitles=/etc/passwd`、`movie=C\:/x`）。
    if a.contains("=/") || a.contains("=\\") || a.contains(":/") || a.contains(":\\") {
        return Some("内嵌绝对路径");
    }
    None
}

/// UTF-8 有损转换 + 按字符边界截断到 `cap` 字节。返回 `(文本, 是否截断)`。
fn truncate_utf8(bytes: &[u8], cap: usize) -> (String, bool) {
    let s = String::from_utf8_lossy(bytes);
    if s.len() <= cap {
        return (s.into_owned(), false);
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::{
        arg_reject_reason, is_globally_routable_unicast, truncate_utf8, validate_ffmpeg_args,
    };
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap_or_else(|_| panic!("bad ip {s}"))
    }

    #[test]
    fn ffmpeg_args_accept_legit_syntax() {
        // 常见合法参数：滤镜/编码器/流选择器/除法/时间戳/相对文件名，均须放行。
        for a in [
            "-i",
            "video.ts",
            "-c",
            "copy",
            "out.mp4",
            "-vf",
            "scale=1280:720",
            "-r",
            "30000/1001",
            "-map",
            "0:a",
            "-c:v",
            "libx264",
            "-b:v",
            "2M",
            "-ss",
            "00:01:02",
            "-metadata:s:a:0",
            "title=x",
            "-vf",
            "setpts=0.5*PTS",
            "-filter_complex",
            "overlay=W-w:H-h",
            "sub.srt",
            "clip.audio.m4a",
            "-y",
        ] {
            assert!(
                arg_reject_reason(a).is_none(),
                "'{a}' should be accepted, got {:?}",
                arg_reject_reason(a)
            );
        }
    }

    #[test]
    fn ffmpeg_args_reject_network_and_escape() {
        // 越牢路径 + 网络协议，须逐一拒绝。
        for a in [
            "/etc/passwd",
            "\\\\server\\share",
            "C:\\Windows\\system32",
            "../secret",
            "a/../../b",
            "http://evil.example/x",
            "https://evil/x",
            "file:/etc/passwd",
            "concat:a.ts|b.ts",
            "crypto:key",
            "subfile:,start,0,end,10,:in",
            "subtitles=/etc/passwd",
            "movie=C\\:/secret",
        ] {
            assert!(arg_reject_reason(a).is_some(), "'{a}' should be rejected");
        }
    }

    #[test]
    fn ffmpeg_validate_rejects_nul_and_reports() {
        assert!(validate_ffmpeg_args(&["ok.mp4".into()]).is_ok());
        assert!(validate_ffmpeg_args(&["bad\0name".into()]).is_err());
        assert!(validate_ffmpeg_args(&["/abs".into()]).is_err());
    }

    #[test]
    fn truncate_utf8_respects_char_boundary() {
        let (s, t) = truncate_utf8("hello".as_bytes(), 100);
        assert_eq!(s, "hello");
        assert!(!t);
        // 3 字节字符边界：cap=4 落在第二个 '啊'(3 字节) 中间 → 回退到 3。
        let (s, t) = truncate_utf8("啊啊".as_bytes(), 4);
        assert_eq!(s, "啊");
        assert!(t);
    }

    #[test]
    fn v4_rejects_private_and_special() {
        for s in [
            "127.0.0.1",       // loopback
            "10.0.0.1",        // private
            "172.16.0.1",      // private
            "192.168.1.1",     // private
            "169.254.169.254", // link-local (cloud metadata)
            "100.64.0.1",      // CGNAT
            "198.18.0.1",      // benchmarking
            "240.0.0.1",       // reserved
            "0.0.0.0",         // this-network
            "255.255.255.255", // broadcast
            "224.0.0.1",       // multicast
            "192.0.2.1",       // documentation
        ] {
            assert!(
                !is_globally_routable_unicast(ip(s)),
                "{s} should be rejected"
            );
        }
    }

    #[test]
    fn v4_allows_public() {
        for s in ["8.8.8.8", "1.1.1.1", "93.184.216.34"] {
            assert!(is_globally_routable_unicast(ip(s)), "{s} should be allowed");
        }
    }

    #[test]
    fn v6_rejects_special() {
        for s in [
            "::1",                // loopback
            "fe80::1",            // link-local
            "fc00::1",            // ULA
            "fd00::1",            // ULA
            "2001:db8::1",        // documentation
            "2001:0::1",          // Teredo
            "ff02::1",            // multicast
            "::",                 // unspecified
            "::ffff:127.0.0.1",   // v4-mapped loopback
            "2002:0a00:0001::",   // 6to4 embedding 10.0.0.1
            "64:ff9b::0a00:0001", // NAT64 embedding 10.0.0.1
        ] {
            assert!(
                !is_globally_routable_unicast(ip(s)),
                "{s} should be rejected"
            );
        }
    }

    #[test]
    fn v6_allows_public() {
        for s in ["2606:4700:4700::1111", "2001:4860:4860::8888"] {
            assert!(is_globally_routable_unicast(ip(s)), "{s} should be allowed");
        }
    }
}
