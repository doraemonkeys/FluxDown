//! Deterministic reproduction for GitHub issue #183.
//!
//! The reported Clash mixed port is a plain HTTP proxy (it also accepts
//! SOCKS), not an HTTPS proxy.  `ProxyType::Https` must therefore use an
//! HTTP CONNECT transport; an `https://` proxy URL would make reqwest attempt
//! a TLS handshake with the proxy before sending the connectivity-check request.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::time::Duration;

use fluxdown_engine::downloader::build_client;
use fluxdown_engine::proxy_config::{ProxyConfig, ProxyMode, ProxyType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;

/// A minimal plaintext HTTP proxy. It responds to absolute-form HTTP requests
/// without contacting the requested origin, like a successful connectivity
/// check would observe.
struct PlainHttpProxy {
    addr: SocketAddr,
    accept_task: JoinHandle<()>,
}

impl PlainHttpProxy {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local proxy");
        let addr = listener.local_addr().expect("read local proxy address");
        let accept_task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(handle_proxy_connection(stream));
            }
        });
        Self { addr, accept_task }
    }

    fn config(&self, proxy_type: ProxyType) -> ProxyConfig {
        ProxyConfig {
            mode: ProxyMode::Manual,
            proxy_type,
            host: self.addr.ip().to_string(),
            port: self.addr.port(),
            ..ProxyConfig::default()
        }
    }
}

impl Drop for PlainHttpProxy {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

async fn handle_proxy_connection(mut stream: TcpStream) {
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];

    loop {
        let Ok(read) = stream.read(&mut chunk).await else {
            return;
        };
        if read == 0 {
            return;
        }

        // An HTTPS proxy attempt starts with a TLS record (0x16), not an HTTP
        // method. Closing it immediately keeps the reproduction fast.
        if request.is_empty() && !chunk[..read].first().is_some_and(u8::is_ascii_alphabetic) {
            return;
        }

        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
            let _ = stream.write_all(response).await;
            return;
        }
    }
}

#[tokio::test]
async fn https_proxy_type_should_work_with_a_plain_http_mixed_port() {
    let proxy = PlainHttpProxy::start().await;
    let request_url = "http://issue-183.invalid/connectivity-check";

    // This is the behavior observed with the same kind of Clash mixed port:
    // HTTP proxy mode reaches the local proxy and gets its 200 response.
    let http_client =
        build_client(&proxy.config(ProxyType::Http), "").expect("build HTTP-proxy client");
    let http_response = timeout(Duration::from_secs(2), http_client.get(request_url).send())
        .await
        .expect("HTTP proxy request timed out")
        .expect("HTTP proxy request failed");
    assert_eq!(http_response.status(), reqwest::StatusCode::OK);

    // Issue #183: the HTTPS destination option must also work with a mixed
    // HTTP/SOCKS port. The proxy connection stays plaintext HTTP; reqwest
    // uses CONNECT when the destination itself is HTTPS.
    let https_client =
        build_client(&proxy.config(ProxyType::Https), "").expect("build HTTPS-proxy client");
    let https_result = timeout(Duration::from_secs(2), https_client.get(request_url).send())
        .await
        .expect("HTTPS proxy request timed out");
    assert!(
        https_result.is_ok(),
        "HTTPS proxy mode should reach a plaintext mixed port, got: {https_result:?}"
    );
}
