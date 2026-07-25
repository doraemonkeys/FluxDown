# Fix HTTPS proxy connectivity test

Closes #183

## Problem

When an HTTPS proxy is configured manually, the connectivity test fails for plain HTTP mixed ports such as Clash Verge's `127.0.0.1:7897`. HTTP and SOCKS proxy tests still pass.

## Root cause

The `https` setting was converted to an `https://` proxy URL. Reqwest then tried to establish TLS with the proxy itself, but a mixed port expects a plain HTTP `CONNECT` request.

## Fix

- Use plain HTTP `CONNECT` transport for the global HTTPS proxy setting.
- Preserve explicit `https://` proxy URLs as TLS-to-proxy configurations.
- Add a regression test with a local plain HTTP mixed-port proxy.

## Verification

- `cargo test -p fluxdown_engine`
- `cargo clippy -p fluxdown_engine --lib -- -D warnings`
