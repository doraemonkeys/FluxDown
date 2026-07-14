---
title: 插件 API 参考
description: 入口函数签名、flux.* 完整接口、全部运行时限制。
section: plugins
order: 4
sourceHash: "08a39add78a2"
---

插件脚本能看到的一切：FluxDown 会调用的五个入口函数，和注入的 `flux` 对象。跨越 JS 边界的字段名全部是 camelCase。

## 入口函数

入口就是普通的全局函数。`async` 函数和返回 Promise 都完全支持——FluxDown 会等待结果。

### `resolve(ctx)`

在协议分派之前调用，匹配任务的**每次**开始和恢复都会执行（惰性解析，见[概览](/docs/zh/plugins/overview/)）。

`ctx` 字段：

| 字段 | 类型 | 含义 |
|---|---|---|
| `taskId` | string | 任务 UUID。 |
| `url` | string | 任务的原始 URL（永远不是上次解析的结果）。 |
| `cookies` | string | 任务附带的 Cookie 值。 |
| `referrer` | string | 任务附带的 Referrer。 |
| `userAgent` | string | 生效的 User-Agent。 |
| `extraHeaders` | object | 额外请求头，字符串键值。 |

返回 `null` 或 `undefined` 表示放行（FluxDown 按 `ctx.url` 原样下载）。否则返回一个对象，除 `url` 外都可选：

| 字段 | 类型 | 含义 |
|---|---|---|
| `url` | string | 改写后的直链。空字符串表示保留原 URL。 |
| `audioUrl` | string | 独立音频流直链，用于 DASH 式音视频分离。 |
| `fileName` | string | 覆盖保存的文件名。 |
| `totalBytes` | number | 文件大小（字节），已知的话。 |
| `extraHeaders` | object | 下载解析后直链时附带的请求头。 |
| `ephemeral` | boolean | `true` = 直链是一次性的/有防盗链：跳过元数据探测（代价是续传一致性校验变弱）。默认 `false`：正常探测并保留基于 ETag 的续传校验。 |
| `rangeSupported` | boolean | `true` = 你担保解析后的服务支持 HTTP Range 请求（如 googlevideo）。与 `ephemeral` 组合时，FluxDown 依旧跳过探测，但直接按多线程分段规划下载，而不是保守的单流启动。默认 `false`：没有探测时，Range 能力只能从首个响应学习。 |

解析完成后，FluxDown 会用**解析后的** URL 重新判定协议引擎——resolver 可以返回 HLS 播放列表、磁力链接或 FTP 地址，对应引擎会自动接管。

错误行为是 fail-closed：抛异常、超时、返回值不合法、插件已卸载或被禁用，任务都进入错误状态。原始 URL 绝不会被悄悄下载。

### `onStart(ctx)` / `onDone(ctx)` / `onError(ctx)` / `onMetaProbed(ctx)`

通知钩子。都会收到 `{ event, taskId, url }`，再加各自的字段：

| 事件 | 额外字段 |
|---|---|
| `onStart` | — |
| `onError` | `message`——任务的错误文本 |
| `onDone` | `filePath`——完成文件的绝对路径；`audioPath`——轨对任务（视频+音频离散轨）mux 失败降级时独立音频文件（`<主干名>.audio.m4a`）的绝对路径，单文件产物（含 mux 成功）为 `null`；`muxed`——轨对任务是否已成功合并为单文件，非轨对任务恒 `false` |
| `onMetaProbed` | `fileName`、`totalBytes`——探测结果 |

`url` 恒为任务的原始 URL，manifest 里 `hooks.match.urls` 也是拿它过滤的。

钩子发出后不管结果：异常和超时只记日志然后吞掉，插件运行时忙不过来时通知直接丢弃。钩子做的任何事都改变不了任务——唯一例外是 `flux.task.requestRetry`，且只在 `onError` 里有效。

## `flux` 对象

### `flux.fetch(opts)` → `Promise<response>`

HTTP 客户端。`opts`：

| 字段 | 默认 | 说明 |
|---|---|---|
| `method` | `"GET"` | |
| `url` | — | 必填。 |
| `headers` | `{}` | 字符串键值。 |
| `body` | 无 | 请求体，仅文本。 |

resolve 出 `{ status, headers, body, truncated }`——`status` 是数字状态码，`body` 是文本（v1 不支持二进制响应），`truncated` 为 `true` 时表示响应体触顶被截断。网络失败和守卫拦截都会 reject。

安全护栏，全部在宿主侧强制：

| 规则 | 值 |
|---|---|
| 协议 | 仅 `http` / `https` |
| 目标地址 | 仅允许公网可路由地址。环回、局域网、link-local、云元数据 IP 全部拦截——对 URL 字面量、DNS 解析结果、每一跳重定向都检查。 |
| 响应体上限 | 8 MB，超出截断 |
| 单请求超时 | 10 秒 |
| 并发请求数 | 8，全部插件共享 |
| 最大重定向 | 30 跳 |

### `flux.storage`

插件私有的持久化键值存储，应用重启后仍在（存在 FluxDown 数据库里）。

- `flux.storage.get(key)` → `Promise<string | null>`
- `flux.storage.set(key, value)` → `Promise<void>`——单个值超过 **64 KB**，或插件键数将超过 **100 个**时 reject。

值只能是字符串；结构化数据自己 JSON 序列化。

### `flux.settings`

只读对象，装着 manifest 里声明的设置项，类型已经转好：`string` 项是字符串，`number` 是数字，`boolean` 是布尔。用户没填的项带着 `default` 值。

### `flux.info`

`{ identity, version, appVersion }`——插件自己的 ID 和版本，以及承载它的 FluxDown 版本。

### `flux.logger` 与 `console`

`flux.logger.info/warn/error(...)` 写入 FluxDown 日志文件。`console.log/info/warn/error/debug` 映射到同一处（`debug` 按 info 级别记）。多个参数用空格连接，非字符串会 JSON 序列化。每条日志截断在 4 KB。

### `flux.task.requestRetry(opts)`

`flux.task.requestRetry({ delayMs: 5000 })`——请求 FluxDown 在延迟后重试失败的任务。只在 `onError` 里有意义；其他地方调用只记一条警告，什么也不做。重试消耗任务自己的自动重试额度，插件无法无限重试。

## 运行时限制

每次调用都在全新的 QuickJS 上下文里跑：调用之间没有任何全局变量残留，没有定时器和 DOM API，脚本按 classic script 加载（顶层 `function` 声明自动成为全局函数；`export` 语法不能用）。

| 预算 | resolve | hooks |
|---|---|---|
| 超时 | 10 秒（manifest `timeoutMs` 可改，30 秒硬顶） | 5 秒 |
| 内存 | 64 MB | 32 MB |

连续 3 次超时或内存超限会触发熔断：插件被自动禁用，应用弹出提示，直到手动重新启用为止。
