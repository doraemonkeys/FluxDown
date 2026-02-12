/**
 * 资源检测 Content Script
 *
 * 运行在 Isolated World（与页面 JS 隔离，但共享 DOM）。
 *
 * 职责：
 * 1. 扫描页面 DOM 中的 video/audio/source/a[href] 等媒体元素
 * 2. 通过 MutationObserver 持续监听动态添加的元素
 * 3. 注入 Main World 脚本拦截 fetch/XHR（检测 HLS/DASH 流媒体）
 * 4. 将检测到的资源通过 runtime.sendMessage 转发给 Background
 */

import type { ResourceMessagePayload, FetchInterceptDetail, ResourceType } from '@/utils/resource-types';

export default defineContentScript({
  matches: ['<all_urls>'],
  runAt: 'document_idle',

  async main(ctx) {
    /** 已报告的 URL 集合（防止重复上报） */
    const reportedUrls = new Set<string>();

    // ===== 1. 初始 DOM 扫描 =====
    const initialResources = scanPageResources();
    if (initialResources.length > 0) {
      reportResources(initialResources);
    }

    // ===== 2. MutationObserver 持续监听 =====
    const observer = new MutationObserver((mutations) => {
      const found: ResourceMessagePayload[] = [];

      for (const mutation of mutations) {
        // 新增节点
        for (const node of mutation.addedNodes) {
          if (!(node instanceof HTMLElement)) continue;
          found.push(...checkElement(node));
          // 检查子元素
          const children = node.querySelectorAll('video, audio, source, a[href], embed, object');
          for (const child of children) {
            found.push(...checkElement(child as HTMLElement));
          }
        }

        // 属性变化（如 video.src 被 JS 修改）
        if (mutation.type === 'attributes' && mutation.target instanceof HTMLElement) {
          found.push(...checkElement(mutation.target));
        }
      }

      if (found.length > 0) {
        reportResources(found);
      }
    });

    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['src', 'href', 'data'],
    });

    // 扩展失效时断开观察
    ctx.onInvalidated(() => observer.disconnect());

    // ===== 3. 注入 Main World 拦截脚本 =====
    try {
      await injectScript('/fetch-interceptor.js', { keepInDom: true });
    } catch (e) {
      console.warn('[FluxDown] Failed to inject fetch interceptor:', e);
    }

    // ===== 4. 监听 Main World 的 CustomEvent =====
    const handleFetchEvent = (event: Event) => {
      const detail = (event as CustomEvent).detail as FetchInterceptDetail | undefined;
      if (!detail || !detail.url) return;

      const payload: ResourceMessagePayload = {
        url: detail.url,
        type: mapFetchEventType(detail.type),
        mimeType: detail.contentType,
        size: detail.size,
        detectedBy: detail.type.startsWith('xhr') ? 'xhr-intercept'
          : detail.type.startsWith('blob') ? 'blob-intercept'
            : 'fetch-intercept',
      };

      reportResources([payload]);
    };

    document.addEventListener('fluxdown-resource-detected', handleFetchEvent);
    ctx.onInvalidated(() => {
      document.removeEventListener('fluxdown-resource-detected', handleFetchEvent);
    });

    // ===== 扫描函数 =====

    function scanPageResources(): ResourceMessagePayload[] {
      const resources: ResourceMessagePayload[] = [];

      // <video> 元素
      for (const video of document.querySelectorAll('video')) {
        if (video.src && !video.src.startsWith('blob:') && !video.src.startsWith('data:')) {
          resources.push({
            url: video.src,
            type: 'video',
            quality: detectQuality(video),
            detectedBy: 'dom-scan',
          });
        }
        if (video.currentSrc && video.currentSrc !== video.src
          && !video.currentSrc.startsWith('blob:') && !video.currentSrc.startsWith('data:')) {
          resources.push({
            url: video.currentSrc,
            type: 'video',
            quality: detectQuality(video),
            detectedBy: 'dom-scan',
          });
        }
      }

      // <audio> 元素
      for (const audio of document.querySelectorAll('audio')) {
        if (audio.src && !audio.src.startsWith('blob:') && !audio.src.startsWith('data:')) {
          resources.push({
            url: audio.src,
            type: 'audio',
            detectedBy: 'dom-scan',
          });
        }
      }

      // <source> 元素
      for (const source of document.querySelectorAll('source')) {
        if (source.src && !source.src.startsWith('blob:') && !source.src.startsWith('data:')) {
          const type: ResourceType = source.type?.startsWith('video/') ? 'video'
            : source.type?.startsWith('audio/') ? 'audio'
              : 'other';
          resources.push({
            url: source.src,
            type,
            mimeType: source.type || undefined,
            detectedBy: 'dom-scan',
          });
        }
      }

      // <a> 标签中的下载链接
      for (const a of document.querySelectorAll<HTMLAnchorElement>('a[href]')) {
        const href = a.href;
        if (!href || href.startsWith('blob:') || href.startsWith('data:')
          || href.startsWith('javascript:') || href.startsWith('#')) continue;
        if (!href.startsWith('http://') && !href.startsWith('https://') && !href.startsWith('ftp://')) continue;

        // 仅收集有明确下载意图的链接（有 download 属性 或 看起来像文件 URL）
        if (a.download || isDownloadableUrl(href)) {
          resources.push({
            url: href,
            type: classifyByUrlExtension(href),
            filename: a.download || undefined,
            detectedBy: 'dom-scan',
          });
        }
      }

      // <embed> / <object>
      for (const el of document.querySelectorAll<HTMLEmbedElement | HTMLObjectElement>('embed[src], object[data]')) {
        const url = (el as HTMLEmbedElement).src || (el as HTMLObjectElement).data;
        if (url && url.startsWith('http')) {
          resources.push({
            url,
            type: 'other',
            detectedBy: 'dom-scan',
          });
        }
      }

      return resources;
    }

    function checkElement(el: HTMLElement): ResourceMessagePayload[] {
      const results: ResourceMessagePayload[] = [];
      const tag = el.tagName.toLowerCase();

      if (tag === 'video' || tag === 'audio') {
        const media = el as HTMLMediaElement;
        if (media.src && !media.src.startsWith('blob:') && !media.src.startsWith('data:')) {
          results.push({
            url: media.src,
            type: tag === 'video' ? 'video' : 'audio',
            quality: tag === 'video' ? detectQuality(media as HTMLVideoElement) : undefined,
            detectedBy: 'mutation-observer',
          });
        }
      } else if (tag === 'source') {
        const source = el as HTMLSourceElement;
        if (source.src && !source.src.startsWith('blob:') && !source.src.startsWith('data:')) {
          results.push({
            url: source.src,
            type: source.type?.startsWith('video/') ? 'video'
              : source.type?.startsWith('audio/') ? 'audio' : 'other',
            mimeType: source.type || undefined,
            detectedBy: 'mutation-observer',
          });
        }
      } else if (tag === 'a') {
        const a = el as HTMLAnchorElement;
        if (a.href && (a.download || isDownloadableUrl(a.href))
          && (a.href.startsWith('http://') || a.href.startsWith('https://'))) {
          results.push({
            url: a.href,
            type: classifyByUrlExtension(a.href),
            filename: a.download || undefined,
            detectedBy: 'mutation-observer',
          });
        }
      }

      return results;
    }

    /**
     * 上报资源给 Background Service Worker
     */
    function reportResources(resources: ResourceMessagePayload[]): void {
      // 去重
      const fresh = resources.filter((r) => {
        if (reportedUrls.has(r.url)) return false;
        reportedUrls.add(r.url);
        return true;
      });

      if (fresh.length === 0) return;

      // 补充 pageUrl
      for (const r of fresh) {
        if (!r.pageUrl) {
          r.pageUrl = location.href;
        }
      }

      chrome.runtime.sendMessage({
        action: 'resourceDetected',
        resources: fresh,
      }).catch(() => {
        // 扩展可能已失效
      });
    }

    // ===== 辅助函数 =====

    function detectQuality(video: HTMLVideoElement): string | undefined {
      const h = video.videoHeight;
      if (h >= 2160) return '4K';
      if (h >= 1440) return '1440p';
      if (h >= 1080) return '1080p';
      if (h >= 720) return '720p';
      if (h >= 480) return '480p';
      if (h >= 360) return '360p';
      if (h > 0) return `${h}p`;
      return undefined;
    }

    const DOWNLOADABLE_EXTS = new Set([
      'zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'zst',
      'exe', 'msi', 'dmg', 'deb', 'rpm', 'appimage', 'apk', 'ipa',
      'iso', 'img',
      'mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'ts', 'm4v',
      'mp3', 'flac', 'wav', 'aac', 'ogg', 'wma', 'm4a', 'opus',
      'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx',
      'bin', 'torrent',
      'm3u8', 'mpd',
    ]);

    function isDownloadableUrl(url: string): boolean {
      try {
        const pathname = new URL(url).pathname.toLowerCase();
        const ext = pathname.split('.').pop() || '';
        return DOWNLOADABLE_EXTS.has(ext);
      } catch {
        return false;
      }
    }

    function classifyByUrlExtension(url: string): ResourceType {
      try {
        const pathname = new URL(url).pathname.toLowerCase();
        const ext = pathname.split('.').pop() || '';
        const videoExts = new Set(['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'ts', 'm4v']);
        const audioExts = new Set(['mp3', 'flac', 'wav', 'aac', 'ogg', 'wma', 'm4a', 'opus']);
        const docExts = new Set(['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx']);
        const archiveExts = new Set(['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'iso', 'img']);
        const streamExts = new Set(['m3u8', 'mpd']);

        if (videoExts.has(ext)) return 'video';
        if (audioExts.has(ext)) return 'audio';
        if (docExts.has(ext)) return 'document';
        if (archiveExts.has(ext)) return 'archive';
        if (streamExts.has(ext)) return 'stream';
        if (ext === 'torrent') return 'torrent';
        if (ext === 'exe' || ext === 'msi' || ext === 'dmg' || ext === 'apk') return 'executable';
      } catch {
        // ignore
      }
      return 'other';
    }

    function mapFetchEventType(type: string): ResourceType {
      if (type === 'hls-manifest' || type === 'dash-manifest') return 'stream';
      return 'other'; // Background 会根据 MIME 重新分类
    }
  },
});
