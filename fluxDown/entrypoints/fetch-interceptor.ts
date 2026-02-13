/**
 * Main World 脚本 — Fetch / XHR / Blob 拦截器
 *
 * 运行在页面的 Main World 中（与页面 JS 共享全局作用域），
 * 通过 monkey-patch 拦截页面的网络请求，捕获 HLS/DASH 清单等流媒体资源。
 *
 * 与 Content Script（Isolated World）通过 CustomEvent 通信。
 */
export default defineUnlistedScript(() => {
  // 防止重复注入
  if ((window as any).__fluxdown_interceptor__) return;
  (window as any).__fluxdown_interceptor__ = true;

  const FLUXDOWN_EVENT = 'fluxdown-resource-detected';

  /** 已通知过的 URL 集合（防止重复通知） */
  const notifiedUrls = new Set<string>();

  // ===== 流媒体 URL 检测 =====

  function isStreamingUrl(url: string): boolean {
    const lower = url.toLowerCase();
    return lower.includes('.m3u8') ||
      lower.includes('.mpd') ||
      lower.includes('/manifest') ||
      lower.includes('/playlist');
  }

  function isMediaContentType(ct: string): boolean {
    const lower = ct.toLowerCase();
    return lower.startsWith('video/') ||
      lower.startsWith('audio/') ||
      lower === 'application/vnd.apple.mpegurl' ||
      lower === 'application/x-mpegurl' ||
      lower === 'application/dash+xml';
  }

  function classifyStreamUrl(url: string): string {
    const lower = url.toLowerCase();
    if (lower.includes('.m3u8')) return 'hls-manifest';
    if (lower.includes('.mpd')) return 'dash-manifest';
    return 'stream-unknown';
  }

  /**
   * 通知 Content Script（Isolated World）
   */
  function notify(type: string, url: string, contentType?: string, size?: number): void {
    // 去重
    const key = `${type}:${url}`;
    if (notifiedUrls.has(key)) return;
    notifiedUrls.add(key);

    // 防止集合无限增长：整体清空而非删除前半部分
    // 下游 Content Script (reportedUrls) 和 resource-store 仍有去重兜底，
    // 最坏情况是短暂的重复通知被下游过滤掉
    if (notifiedUrls.size > 500) {
      notifiedUrls.clear();
    }

    document.dispatchEvent(new CustomEvent(FLUXDOWN_EVENT, {
      detail: { type, url, contentType, size },
    }));
  }

  // ===== 拦截 Fetch API =====

  const originalFetch = window.fetch;
  window.fetch = function (...args: Parameters<typeof fetch>) {
    let url = '';
    try {
      if (typeof args[0] === 'string') {
        url = args[0];
      } else if (args[0] instanceof Request) {
        url = args[0].url;
      } else if (args[0] instanceof URL) {
        url = args[0].href;
      }
    } catch {
      // ignore
    }

    // 请求阶段：检测流媒体 URL
    if (url && isStreamingUrl(url)) {
      notify('fetch-detected', url, classifyStreamUrl(url));
    }

    // 调用原始 fetch，检查响应
    return originalFetch.apply(this, args).then((response) => {
      try {
        const ct = response.headers.get('content-type') || '';
        const cl = response.headers.get('content-length');
        const finalUrl = response.url || url;

        if (ct && isMediaContentType(ct)) {
          notify('fetch-detected', finalUrl, ct, cl ? parseInt(cl, 10) : undefined);
        }
        // 检查响应 URL 是否是流媒体（可能是重定向后的 URL）
        if (finalUrl && finalUrl !== url && isStreamingUrl(finalUrl)) {
          notify('fetch-detected', finalUrl, ct || classifyStreamUrl(finalUrl));
        }
      } catch {
        // 不干扰原始响应
      }
      return response;
    });
  };

  // ===== 拦截 XMLHttpRequest =====

  const originalOpen = XMLHttpRequest.prototype.open;
  const originalSend = XMLHttpRequest.prototype.send;

  XMLHttpRequest.prototype.open = function (method: string, url: string | URL, ...rest: any[]) {
    const urlStr = typeof url === 'string' ? url : url.href;
    (this as any).__fluxdown_url = urlStr;

    if (isStreamingUrl(urlStr)) {
      notify('xhr-detected', urlStr, classifyStreamUrl(urlStr));
    }

    return originalOpen.apply(this, [method, url, ...rest] as any);
  };

  XMLHttpRequest.prototype.send = function (...args: any[]) {
    this.addEventListener('load', function () {
      try {
        const url = (this as any).__fluxdown_url as string | undefined;
        if (!url) return;

        const ct = this.getResponseHeader('content-type') || '';
        const cl = this.getResponseHeader('content-length');
        const responseUrl = this.responseURL || url;

        if (ct && isMediaContentType(ct)) {
          notify('xhr-detected', responseUrl, ct, cl ? parseInt(cl, 10) : undefined);
        }
        if (responseUrl !== url && isStreamingUrl(responseUrl)) {
          notify('xhr-detected', responseUrl, ct || classifyStreamUrl(responseUrl));
        }
      } catch {
        // ignore
      }
    });

    return originalSend.apply(this, args);
  };

  // ===== 拦截 URL.createObjectURL =====

  const originalCreateObjectURL = URL.createObjectURL;
  URL.createObjectURL = function (obj: Blob | MediaSource) {
    const blobUrl = originalCreateObjectURL.call(URL, obj);

    try {
      if (obj instanceof Blob && obj.size > 100 * 1024) {
        // Blob > 100KB，可能是有意义的媒体资源
        notify('blob-detected', blobUrl, obj.type || '', obj.size);
      }
    } catch {
      // ignore
    }

    return blobUrl;
  };

  console.log('[FluxDown] Fetch/XHR/Blob interceptor injected');
});
