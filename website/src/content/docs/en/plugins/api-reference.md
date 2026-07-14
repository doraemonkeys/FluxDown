---
title: Plugin API Reference
description: Entry-point function signatures, the flux.* API, and all runtime limits.
section: plugins
order: 4
---

Everything a plugin script can see: five entry points FluxDown calls, and the `flux` object it injects. All field names crossing the JS boundary are camelCase.

## Entry points

Entry points are plain global functions. `async` functions and returned Promises are fully supported — FluxDown awaits the result.

### `resolve(ctx)`

Called before protocol dispatch, on **every** start and resume of a matching task (resolution is lazy — see the [overview](/docs/en/plugins/overview/)).

`ctx` fields:

| Field | Type | Meaning |
|---|---|---|
| `taskId` | string | Task UUID. |
| `url` | string | The original task URL (never a previously resolved one). |
| `cookies` | string | Cookie header value attached to the task. |
| `referrer` | string | Referrer attached to the task. |
| `userAgent` | string | Effective User-Agent. |
| `extraHeaders` | object | Extra request headers as string key-values. |

Return `null` or `undefined` to pass through (FluxDown downloads `ctx.url` unchanged). Otherwise return an object; every field except `url` is optional:

| Field | Type | Meaning |
|---|---|---|
| `url` | string | The rewritten direct link. An empty string keeps the original URL. |
| `audioUrl` | string | Separate audio stream link, for DASH-style split audio/video. |
| `fileName` | string | Override the saved file name. |
| `totalBytes` | number | File size in bytes, if known. |
| `extraHeaders` | object | Headers to send when downloading the resolved link. |
| `ephemeral` | boolean | `true` = the link is one-shot / anti-hotlinked: skip the metadata probe (at the cost of weaker resume-integrity checks). Default `false`: probe normally and keep ETag-based resume validation. |
| `rangeSupported` | boolean | `true` = you guarantee the resolved host honours HTTP Range requests (e.g. googlevideo). Combined with `ephemeral`, FluxDown still skips the probe but plans a full multi-segment download right away instead of the conservative single-stream start. Default `false`: without a probe, Range capability is learned from the first response. |

After resolution, FluxDown re-examines the *resolved* URL to pick the protocol engine — a resolver may return an HLS playlist, a magnet link or an FTP URL and the right engine takes over.

Error behavior is fail-closed: an exception, timeout, invalid return value, or an uninstalled/disabled plugin all put the task into the error state. The original URL is never silently downloaded.

### `onStart(ctx)` / `onDone(ctx)` / `onError(ctx)` / `onMetaProbed(ctx)`

Notification hooks. All receive `{ event, taskId, url }` plus event-specific fields:

| Event | Extra fields |
|---|---|
| `onStart` | — |
| `onError` | `message` — the task's error text |
| `onDone` | `filePath` — absolute path of the finished file; `audioPath` — for track-pair tasks (separate video+audio streams) where muxing failed, the absolute path of the standalone audio file (`<stem>.audio.m4a`); `null` for single-file results (including successful mux); `muxed` — whether a track-pair task was successfully merged into a single file, always `false` for non-track-pair tasks |
| `onMetaProbed` | `fileName`, `totalBytes` — probe results |

`url` is always the task's original URL, and it's what the manifest's `hooks.match.urls` filter is applied to.

Hooks are fire-and-forget: exceptions and timeouts are logged and swallowed, and if the plugin runtime is saturated the notification is dropped. Nothing a hook does can change the task — with one exception, `flux.task.requestRetry`, valid only inside `onError`.

## The `flux` object

### `flux.fetch(opts)` → `Promise<response>`

HTTP client. `opts`:

| Field | Default | Notes |
|---|---|---|
| `method` | `"GET"` | |
| `url` | — | Required. |
| `headers` | `{}` | String key-values. |
| `body` | none | Request body, text only. |

Resolves to `{ status, headers, body, truncated }` — `status` is the numeric code, `body` is text (binary responses are not supported in v1), and `truncated` is `true` when the body hit the size cap. Network and guard failures reject the Promise.

Guard rails, all enforced host-side:

| Rule | Value |
|---|---|
| Schemes | `http` / `https` only |
| Destinations | Publicly routable addresses only. Loopback, LAN, link-local and cloud-metadata IPs are blocked — checked against the literal URL, at DNS resolution, and again on every redirect hop. |
| Response body cap | 8 MB, then truncated |
| Per-request timeout | 10 s |
| Concurrent requests | 8, shared across all plugins |
| Max redirects | 30 |

### `flux.storage`

Persistent key-value store, private to your plugin, survives app restarts (backed by the FluxDown database).

- `flux.storage.get(key)` → `Promise<string | null>`
- `flux.storage.set(key, value)` → `Promise<void>` — rejects when a single value exceeds **64 KB** or the plugin would exceed **100 keys**.

Values are strings; JSON-encode anything structured yourself.

### `flux.settings`

Read-only object with your manifest-declared settings, already typed: `string` fields arrive as strings, `number` as numbers, `boolean` as booleans. Unset fields carry their `default`.

### `flux.info`

`{ identity, version, appVersion }` — your plugin's ID and version, and the FluxDown version hosting it.

### `flux.logger` and `console`

`flux.logger.info/warn/error(...)` write to FluxDown's log file. `console.log/info/warn/error/debug` are mapped to the same place (`debug` logs at info level). Multiple arguments are joined with spaces; non-strings are JSON-stringified. Each line is truncated at 4 KB.

### `flux.task.requestRetry(opts)`

`flux.task.requestRetry({ delayMs: 5000 })` — ask FluxDown to retry the failed task after a delay. Only meaningful inside `onError`; called anywhere else it logs a warning and does nothing. Retries share the task's automatic-retry budget, so a plugin cannot retry forever.

## Runtime limits

Each invocation runs in a fresh QuickJS context: no globals survive between calls, timers and DOM APIs don't exist, and scripts load as classic scripts (top-level `function` declarations become globals; `export` syntax will not work).

| Budget | Resolve | Hooks |
|---|---|---|
| Timeout | 10 s (manifest `timeoutMs` overrides, 30 s hard ceiling) | 5 s |
| Memory | 64 MB | 32 MB |

Three consecutive timeouts or memory-limit hits trip the circuit breaker: the plugin is auto-disabled, the app shows a notice, and it stays off until manually re-enabled.
