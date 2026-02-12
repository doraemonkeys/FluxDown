/**
 * FluxDown Background Service Worker
 *
 * 职责：
 * 1. 拦截浏览器下载事件，转发给 FluxDown 桌面应用
 * 2. 注册右键菜单（发送链接到 FluxDown）
 * 3. 管理与 Native Host 的通信
 * 4. 响应 popup 的消息
 * 5. 维护拦截统计数据
 * 6. 多语言支持
 *
 * === 下载拦截三层防线 ===
 *
 * 第一层（HTTP 响应感知）: webRequest.onHeadersReceived
 *   - 监听所有请求的响应头
 *   - 当响应包含 Content-Disposition: attachment 或 下载类 Content-Type 时，
 *     将该 URL 标记为"已知下载"，缓存 Content-Type / Content-Length / 文件名等
 *   - 为后续 onCreated 兜底提供可靠的元数据来源
 *
 * 第二层（主拦截）: downloads.onDeterminingFilename
 *   - 浏览器弹出「另存为」之前触发，suggest({ cancel: true }) 可取消下载
 *   - 最优先、最干净的拦截方式
 *   - 但对 JS location.href / meta refresh 触发的"导航转下载"存在 MV3 时序问题
 *
 * 第三层（兜底拦截）: downloads.onCreated + onChanged
 *   - onCreated 始终可靠触发，配合 onChanged 等待元数据就绪后再判断
 *   - 如果 onDeterminingFilename 在限定时间内未处理，由此层接管
 *   - 利用第一层缓存的 HTTP 响应信息来补全 downloadItem 中缺失的元数据
 */

import { sendDownloadRequest, checkFluxDownAvailable } from '@/utils/native-messaging';
import type { DownloadRequest } from '@/utils/native-messaging';
import { loadSettings, shouldIntercept } from '@/utils/settings';
import type { DownloadItemInfo } from '@/utils/settings';
import { initI18n, t } from '@/utils/i18n';
import { isSniffableContentType, classifyResource, extractFilenameFromUrl } from '@/utils/resource-types';
import type { ResourceMessagePayload } from '@/utils/resource-types';
import {
  addResources,
  addSniffedResource,
  getResourcesForTab,
  getResourceCountForTab,
  clearResourcesForTab,
  updateBadgeForTab,
  initTabLifecycleListeners,
} from '@/utils/resource-store';

// ===== 统计相关 =====
interface DailyStats {
  sent: number;
  failed: number;
  date: string;
}

async function getTodayStats(): Promise<DailyStats> {
  const today = new Date().toDateString();
  const result = await chrome.storage.local.get('stats');
  const stats: DailyStats = result.stats || { sent: 0, failed: 0, date: '' };

  // 跨天自动重置
  if (stats.date !== today) {
    const resetStats: DailyStats = { sent: 0, failed: 0, date: today };
    await chrome.storage.local.set({ stats: resetStats });
    return resetStats;
  }

  return stats;
}

async function incrementStat(field: 'sent' | 'failed') {
  const stats = await getTodayStats();
  stats[field]++;
  await chrome.storage.local.set({ stats });
}

export default defineBackground(() => {
  console.log('[FluxDown] Background service worker started');

  // 初始化 i18n
  initI18n().then(() => {
    console.log('[FluxDown] i18n initialized');
  });

  // 初始化 tab 生命周期监听器（自动清理关闭/导航的 tab 资源）
  initTabLifecycleListeners();

  // ==========================================
  // 第一层：HTTP 响应感知（webRequest 缓存）
  // ==========================================

  // 请求头缓存（Cookie / Authorization）
  const requestHeaderCache = new Map<string, { cookies: string; headers: Record<string, string>; ts: number }>();

  // 响应头缓存 —— 当 HTTP 响应指示"这是一个下载"时，缓存其元数据
  // 这是第三层兜底拦截的关键数据来源
  interface ResponseDownloadInfo {
    url: string;
    contentType: string;         // Content-Type
    contentLength: number;       // Content-Length（-1 = 未知）
    dispositionFilename: string; // 从 Content-Disposition 解析出的文件名
    ts: number;
  }
  const responseDownloadCache = new Map<string, ResponseDownloadInfo>();

  // Chrome MV3: 需要 'extraHeaders' 才能看到 Cookie / Authorization 等敏感头
  try {
    chrome.webRequest.onSendHeaders.addListener(
      (details) => {
        if (!details.requestHeaders) return;
        const headers: Record<string, string> = {};
        let cookies = '';
        for (const h of details.requestHeaders) {
          if (h.name && h.value) {
            headers[h.name] = h.value;
            if (h.name.toLowerCase() === 'cookie') {
              cookies = h.value;
            }
          }
        }
        requestHeaderCache.set(details.url, { cookies, headers, ts: Date.now() });

        // 清理 60 秒前的缓存条目
        for (const [url, entry] of requestHeaderCache) {
          if (Date.now() - entry.ts > 60_000) {
            requestHeaderCache.delete(url);
          }
        }
      },
      { urls: ['<all_urls>'] },
      ['requestHeaders', 'extraHeaders'],
    );
    console.log('[FluxDown] webRequest.onSendHeaders listener registered');
  } catch (e) {
    console.warn('[FluxDown] Failed to register webRequest.onSendHeaders listener:', e);
  }

  // === 响应头监听：检测"导航转下载"场景 ===
  // 当浏览器主框架导航的响应带有 Content-Disposition: attachment 或
  // 下载类 Content-Type 时，说明这是一个"导航转下载"的请求。
  // 缓存其信息，供 onCreated 兜底拦截使用。
  try {
    chrome.webRequest.onHeadersReceived.addListener(
      (details) => {
        // 只关注主框架导航（sub_frame、xhr 等交给正常 download 流程处理）
        if (details.type !== 'main_frame') return;
        if (!details.responseHeaders) return;

        let contentType = '';
        let contentLength = -1;
        let contentDisposition = '';

        for (const h of details.responseHeaders) {
          const name = h.name.toLowerCase();
          if (name === 'content-type' && h.value) {
            contentType = h.value.split(';')[0].trim().toLowerCase();
          } else if (name === 'content-length' && h.value) {
            const parsed = parseInt(h.value, 10);
            if (!isNaN(parsed)) contentLength = parsed;
          } else if (name === 'content-disposition' && h.value) {
            contentDisposition = h.value;
          }
        }

        // 判断该响应是否会触发下载
        const isAttachment = contentDisposition.toLowerCase().startsWith('attachment');
        const isDownloadMime = isDownloadContentType(contentType);

        if (!isAttachment && !isDownloadMime) return;

        // 从 Content-Disposition 提取文件名
        const dispositionFilename = parseContentDispositionFilename(contentDisposition);

        const info: ResponseDownloadInfo = {
          url: details.url,
          contentType,
          contentLength,
          dispositionFilename,
          ts: Date.now(),
        };

        responseDownloadCache.set(details.url, info);
        console.log('[FluxDown] Detected download-triggering response (onHeadersReceived):', info);

        // 60 秒后自动清理
        setTimeout(() => responseDownloadCache.delete(details.url), 60_000);
      },
      { urls: ['<all_urls>'] },
      ['responseHeaders'],
    );
    console.log('[FluxDown] webRequest.onHeadersReceived listener registered');
  } catch (e) {
    console.warn('[FluxDown] Failed to register webRequest.onHeadersReceived listener:', e);
  }

  // ==========================================
  // 资源嗅探层：监听所有 media / XHR 类型请求的响应头
  // 检测可下载的媒体资源，加入资源列表供 UI 展示
  // ==========================================
  try {
    chrome.webRequest.onHeadersReceived.addListener(
      (details) => {
        // 跳过无效或非 tab 请求
        if (details.tabId < 0 || !details.responseHeaders) return;

        let contentType = '';
        let contentLength = -1;
        let contentDisposition = '';

        for (const h of details.responseHeaders) {
          const name = h.name.toLowerCase();
          if (name === 'content-type' && h.value) {
            contentType = h.value.split(';')[0].trim().toLowerCase();
          } else if (name === 'content-length' && h.value) {
            const parsed = parseInt(h.value, 10);
            if (!isNaN(parsed)) contentLength = parsed;
          } else if (name === 'content-disposition' && h.value) {
            contentDisposition = h.value;
          }
        }

        // 判断是否是有价值的资源
        const isSniffable = isSniffableContentType(contentType);
        const isAttachment = contentDisposition.toLowerCase().startsWith('attachment');

        if (!isSniffable && !isAttachment) return;

        // 提取文件名
        let filename = '';
        if (contentDisposition) {
          filename = parseContentDispositionFilename(contentDisposition);
        }
        if (!filename) {
          filename = extractFilenameFromUrl(details.url);
        }

        // 添加到资源存储（传递 isAttachment 标记用于可信度计算）
        const added = addSniffedResource(
          details.tabId,
          details.url,
          contentType,
          contentLength,
          filename,
          isAttachment,
        );

        if (added > 0) {
          // 更新 Badge
          updateBadgeForTab(details.tabId);
          // 推送给 Content Script UI
          notifyContentScript(details.tabId);
        }
      },
      { urls: ['<all_urls>'], types: ['media', 'xmlhttprequest', 'object', 'other'] },
      ['responseHeaders'],
    );
    console.log('[FluxDown] Resource sniffer (onHeadersReceived for media) registered');
  } catch (e) {
    console.warn('[FluxDown] Failed to register resource sniffer:', e);
  }

  /**
   * 向指定 tab 的 Content Script 推送最新资源列表
   */
  async function notifyContentScript(tabId: number): Promise<void> {
    const resources = getResourcesForTab(tabId);
    try {
      await chrome.tabs.sendMessage(tabId, {
        action: 'resourcesUpdated',
        resources,
      });
    } catch {
      // Content script 可能还未注入
    }
  }

  // ===== 右键菜单 =====
  chrome.runtime.onInstalled.addListener(async () => {
    // 确保 i18n 已初始化
    await initI18n();

    chrome.contextMenus.create({
      id: 'fluxdown-download-link',
      title: t('contextMenu.downloadLink'),
      contexts: ['link'],
    });

    chrome.contextMenus.create({
      id: 'fluxdown-download-media',
      title: t('contextMenu.downloadMedia'),
      contexts: ['image', 'video', 'audio'],
    });

    chrome.contextMenus.create({
      id: 'fluxdown-download-page',
      title: t('contextMenu.downloadPage'),
      contexts: ['page'],
    });

    console.log('[FluxDown] Context menus created');
  });

  // ===== 右键菜单点击处理 =====
  chrome.contextMenus.onClicked.addListener(async (info, tab) => {
    let url: string | undefined;

    switch (info.menuItemId) {
      case 'fluxdown-download-link':
        url = info.linkUrl;
        break;
      case 'fluxdown-download-media':
        url = info.srcUrl;
        break;
      case 'fluxdown-download-page':
        if (tab?.id) {
          await handleDownloadPageLinks(tab.id, tab.url);
        }
        return;
    }

    if (url) {
      await sendToFluxDown(url, tab?.url);
    }
  });

  // ==========================================
  // 第二层 + 第三层：下载事件拦截
  // ==========================================

  // 缓存 onCreated 中的 downloadItem 信息，供 onDeterminingFilename 使用
  const downloadItemCache = new Map<number, chrome.downloads.DownloadItem>();

  // 协调标记：记录各层的处理状态，防止重复发送
  // 'primary' = 由 onDeterminingFilename 处理
  // 'fallback' = 由 onCreated 兜底处理
  const handledDownloads = new Map<number, 'primary' | 'fallback'>();

  // === 第三层：onCreated 兜底 + onChanged 元数据补全 ===
  chrome.downloads.onCreated.addListener((downloadItem) => {
    const downloadId = downloadItem.id;
    const url = downloadItem.url;

    // 缓存 downloadItem 信息，onDeterminingFilename 会用到
    downloadItemCache.set(downloadId, downloadItem);

    // 跳过 blob 和 data URL
    if (url.startsWith('blob:') || url.startsWith('data:')) return;

    // 启动兜底计时器
    // 给 onDeterminingFilename 一个处理窗口，超时后由 onCreated 兜底
    //
    // 关键点：不使用固定的"猜测"超时，而是利用 onChanged 获取完整元数据后再判断。
    // 这里只是一个启动延迟，等 onDeterminingFilename 有机会先处理。
    // 如果 onDeterminingFilename 已处理，兜底逻辑会跳过。
    //
    // 注意：我们注册一个 onChanged 监听器，一旦 downloadItem 的 filename 或 mime
    // 字段被填充（说明浏览器已解析完响应头），就可以做出更准确的判断。
    startFallbackInterception(downloadId, downloadItem);

    // 30 秒后全面清理
    setTimeout(() => {
      downloadItemCache.delete(downloadId);
      handledDownloads.delete(downloadId);
    }, 30_000);
  });

  /**
   * 兜底拦截入口。策略：
   *
   * 1. 立即检查 responseDownloadCache（第一层的 HTTP 响应缓存），
   *    如果命中说明这是已确认的"导航转下载"，可以直接拦截，不必等待。
   *
   * 2. 如果缓存未命中，等待 150ms（给 onDeterminingFilename 机会先处理），
   *    然后用 chrome.downloads.search() 查询最新的 downloadItem 元数据，
   *    获取浏览器已解析的 filename / mime / fileSize，再做判断。
   */
  async function startFallbackInterception(downloadId: number, originalItem: chrome.downloads.DownloadItem) {
    const url = originalItem.url;

    // === 路径 A：检查 HTTP 响应缓存（即时判断，不等待） ===
    const responseCached = responseDownloadCache.get(url);
    if (responseCached) {
      // 响应头已确认这是下载 — 不必等 onDeterminingFilename
      // 但仍给它一个极短的窗口（50ms），因为如果 onDeterminingFilename 能处理，
      // 它的 suggest({ cancel: true }) 比 downloads.cancel() 更干净
      await sleep(50);
      if (handledDownloads.has(downloadId)) return;

      console.log('[FluxDown] Fallback (path A - response cache hit):', {
        id: downloadId,
        url,
        contentType: responseCached.contentType,
        contentLength: responseCached.contentLength,
        dispositionFilename: responseCached.dispositionFilename,
      });

      const settings = await loadSettings();
      if (!settings.enabled) return;

      // 用响应头缓存的信息构造 DownloadItemInfo
      const itemInfo: DownloadItemInfo = {
        url,
        fileSize: responseCached.contentLength > 0 ? responseCached.contentLength : undefined,
        mime: responseCached.contentType || undefined,
        filename: responseCached.dispositionFilename || originalItem.filename || undefined,
      };

      if (!shouldIntercept(itemInfo, settings)) return;

      await executeFallbackIntercept(downloadId, url, originalItem.referrer, itemInfo);
      responseDownloadCache.delete(url);
      return;
    }

    // === 路径 B：响应缓存未命中 — 等待后查询最新元数据 ===
    // 等待 150ms，给 onDeterminingFilename 优先处理的时间
    await sleep(150);
    if (handledDownloads.has(downloadId)) return;

    // 用 chrome.downloads.search 查询最新状态（此时浏览器可能已解析了响应头）
    let freshItems: chrome.downloads.DownloadItem[];
    try {
      freshItems = await chrome.downloads.search({ id: downloadId });
    } catch {
      return; // 下载可能已被删除
    }

    if (freshItems.length === 0) return; // 下载已不存在
    if (handledDownloads.has(downloadId)) return; // 检查期间被 onDeterminingFilename 处理了

    const freshItem = freshItems[0];
    // 如果下载已经完成或被取消了，不处理
    if (freshItem.state === 'complete' || (freshItem as any).state === 'interrupted') {
      return;
    }

    const settings = await loadSettings();
    if (!settings.enabled) return;

    const mime = freshItem.mime || originalItem.mime || undefined;
    const fileSize = (freshItem.fileSize > 0 ? freshItem.fileSize : undefined)
      ?? (originalItem.fileSize > 0 ? originalItem.fileSize : undefined);
    const filename = freshItem.filename || originalItem.filename || undefined;

    const itemInfo: DownloadItemInfo = {
      url: freshItem.url || url,
      fileSize,
      mime,
      filename,
    };

    if (!shouldIntercept(itemInfo, settings)) return;

    // 最后一次检查——避免和 onDeterminingFilename 竞态
    if (handledDownloads.has(downloadId)) return;

    console.log('[FluxDown] Fallback (path B - search query):', {
      id: downloadId,
      url: itemInfo.url,
      mime,
      filename,
      fileSize,
    });

    await executeFallbackIntercept(downloadId, itemInfo.url, freshItem.referrer || originalItem.referrer, itemInfo);
  }

  /**
   * 执行兜底拦截：cancel + erase + 发送到 FluxDown
   */
  async function executeFallbackIntercept(
    downloadId: number,
    url: string,
    referrer: string | undefined,
    itemInfo: DownloadItemInfo,
  ) {
    // 标记为 fallback 已处理
    handledDownloads.set(downloadId, 'fallback');

    // cancel + erase（替代 suggest({ cancel: true })）
    try {
      await chrome.downloads.cancel(downloadId);
    } catch (e) {
      console.warn('[FluxDown] Fallback: failed to cancel download:', e);
    }
    try {
      chrome.downloads.erase({ id: downloadId });
    } catch (e) {
      console.warn('[FluxDown] Fallback: failed to erase download:', e);
    }

    // 发送到 FluxDown
    const cleanFilename = extractCleanFilename(itemInfo.filename, url);
    await sendToFluxDown(url, referrer, cleanFilename, itemInfo.fileSize, itemInfo.mime);
  }

  // === 第二层：onDeterminingFilename（主拦截） ===
  // 在浏览器弹出「另存为」对话框之前触发，
  // suggest({ cancel: true }) 可以在不弹出任何浏览器下载 UI 的情况下直接取消下载。
  chrome.downloads.onDeterminingFilename.addListener(
    (downloadItem, suggest) => {
      const url = downloadItem.url;

      // 跳过 blob 和 data URL
      if (url.startsWith('blob:') || url.startsWith('data:')) {
        suggest({ filename: downloadItem.filename });
        return;
      }

      // 如果已被兜底层处理，直接取消（不重复发送）
      if (handledDownloads.get(downloadItem.id) === 'fallback') {
        console.log('[FluxDown] onDeterminingFilename: already handled by fallback, cancelling:', downloadItem.id);
        suggest({ cancel: true });
        return;
      }

      // 异步判断
      (async () => {
        try {
          // 再次检查兜底状态（await 期间可能被兜底层抢先处理了）
          if (handledDownloads.get(downloadItem.id) === 'fallback') {
            suggest({ cancel: true });
            return;
          }

          const settings = await loadSettings();
          if (!settings.enabled) {
            suggest({ filename: downloadItem.filename });
            return;
          }

          // 合并 onCreated 缓存的额外信息
          const cached = downloadItemCache.get(downloadItem.id);
          const mime = downloadItem.mime || cached?.mime || undefined;
          const fileSize = (downloadItem.fileSize > 0 ? downloadItem.fileSize : undefined)
            ?? (cached && cached.fileSize > 0 ? cached.fileSize : undefined);
          const referrer = cached?.referrer || undefined;

          const itemInfo: DownloadItemInfo = {
            url,
            fileSize,
            mime,
            filename: downloadItem.filename || undefined,
          };

          if (!shouldIntercept(itemInfo, settings)) {
            suggest({ filename: downloadItem.filename });
            return;
          }

          // 最后一次竞态检查
          if (handledDownloads.has(downloadItem.id)) {
            suggest({ cancel: true });
            return;
          }

          console.log('[FluxDown] Intercepting download (onDeterminingFilename):', {
            url,
            mime,
            filename: downloadItem.filename,
            fileSize,
            mode: settings.interceptMode,
          });

          // 标记为主拦截已处理
          handledDownloads.set(downloadItem.id, 'primary');

          // 取消浏览器下载
          suggest({ cancel: true });

          // 清理下载记录
          try {
            chrome.downloads.erase({ id: downloadItem.id });
          } catch (e) {
            console.warn('[FluxDown] Failed to erase download:', e);
          }

          // 发送到 FluxDown
          const cleanFilename = extractCleanFilename(downloadItem.filename, url);
          await sendToFluxDown(url, referrer, cleanFilename, fileSize, mime);
        } catch (e) {
          console.error('[FluxDown] Error in onDeterminingFilename handler:', e);
          // 出错时放行下载，不阻塞用户
          suggest({ filename: downloadItem.filename });
        } finally {
          downloadItemCache.delete(downloadItem.id);
        }
      })();

      // 返回 true 表示 suggest 将被异步调用
      return true;
    },
  );

  // ===== 消息处理（Popup + Content Script） =====
  chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    handleMessage(message, sender).then(sendResponse);
    return true; // 保持消息通道开放（异步响应）
  });

  // ===== 下载此页面所有链接 =====
  async function handleDownloadPageLinks(tabId: number, pageUrl?: string) {
    try {
      // 注入脚本提取页面中所有可下载链接
      const results = await chrome.scripting.executeScript({
        target: { tabId },
        func: () => {
          const links = new Set<string>();

          // 提取所有 <a> 标签的 href
          for (const a of document.querySelectorAll<HTMLAnchorElement>('a[href]')) {
            const href = a.href;
            if (href && (href.startsWith('http://') || href.startsWith('https://') || href.startsWith('ftp://'))) {
              links.add(href);
            }
          }

          // 提取所有 <video> / <audio> / <source> 的 src
          for (const el of document.querySelectorAll<HTMLMediaElement | HTMLSourceElement>('video[src], audio[src], source[src]')) {
            const src = el.src;
            if (src && (src.startsWith('http://') || src.startsWith('https://'))) {
              links.add(src);
            }
          }

          return Array.from(links);
        },
      });

      const allLinks: string[] = results?.[0]?.result || [];
      if (allLinks.length === 0) {
        notify(t('notify.batchNoLinks'), t('notify.batchNoLinksDetail'));
        return;
      }

      // 过滤出可下载的链接（排除页面导航链接）
      const downloadableExts = [
        '.zip', '.rar', '.7z', '.tar', '.gz', '.bz2', '.xz',
        '.exe', '.msi', '.dmg', '.deb', '.rpm', '.appimage',
        '.iso', '.img',
        '.mp4', '.mkv', '.avi', '.mov', '.wmv', '.flv', '.webm',
        '.mp3', '.flac', '.wav', '.aac', '.ogg',
        '.pdf', '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx',
        '.bin', '.apk', '.ipa', '.torrent',
      ];

      const downloadLinks = allLinks.filter((link) => {
        try {
          const pathname = new URL(link).pathname.toLowerCase();
          return downloadableExts.some((ext) => pathname.endsWith(ext));
        } catch {
          return false;
        }
      });

      if (downloadLinks.length === 0) {
        notify(t('notify.batchNoLinks'), t('notify.batchNoDownloadableLinks'));
        return;
      }

      // 批量发送到 FluxDown
      let sentCount = 0;
      let failedCount = 0;

      for (const link of downloadLinks) {
        try {
          await sendToFluxDown(link, pageUrl);
          sentCount++;
        } catch {
          failedCount++;
        }
      }

      notify(
        t('notify.batchComplete'),
        t('notify.batchResult', {
          total: String(downloadLinks.length),
          sent: String(sentCount),
          failed: String(failedCount),
        }),
      );
    } catch (e) {
      console.error('[FluxDown] Failed to extract page links:', e);
      notify(t('notify.sendFailed'), t('notify.batchExtractFailed'));
    }
  }

  // ===== 核心：发送下载请求到 FluxDown App =====
  async function sendToFluxDown(
    url: string,
    referrer?: string,
    filename?: string,
    fileSize?: number,
    mimeType?: string,
  ) {
    // === 提取认证信息（Cookie / Authorization 等） ===
    // 策略 1：从 webRequest 缓存获取（最可靠 — 浏览器真正发出的请求头）
    let cookieString = '';
    const cached = requestHeaderCache.get(url);
    if (cached) {
      cookieString = cached.cookies;
      console.log('[FluxDown] Cookies from webRequest cache:', cookieString.length, 'chars');
      requestHeaderCache.delete(url); // 使用后清理
    }

    // 策略 2：通过 chrome.cookies API 提取（兜底）
    if (!cookieString) {
      try {
        const cookies = await chrome.cookies.getAll({ url });
        cookieString = cookies.map((c) => `${c.name}=${c.value}`).join('; ');
        console.log('[FluxDown] Cookies from cookies API:', cookies.length, 'cookies,', cookieString.length, 'chars');
      } catch (e) {
        console.warn('[FluxDown] Failed to extract cookies via API:', e);
      }
    }

    if (!cookieString) {
      console.log('[FluxDown] No cookies available for URL:', url);
    }

    const request: DownloadRequest = {
      url,
      filename: filename || '',
      referrer: referrer || '',
      cookies: cookieString,
      fileSize,
      mimeType,
    };

    console.log('[FluxDown] Sending to FluxDown app:', request);

    const response = await sendDownloadRequest(request);

    if (response.success) {
      // 统计：接管成功
      await incrementStat('sent');
    } else {
      // 统计：接管失败
      await incrementStat('failed');

      notify(
        t('notify.sendFailed'),
        t('notify.connectionFailed', { message: response.message }),
      );
    }
  }

  // ===== 统一消息处理（Popup + Content Script） =====
  async function handleMessage(message: any, sender: chrome.runtime.MessageSender): Promise<any> {
    switch (message.action) {
      // --- Popup 消息 ---
      case 'getStatus': {
        const available = await checkFluxDownAvailable();
        const settings = await loadSettings();
        return { connected: available, settings };
      }

      case 'toggleEnabled': {
        const currentSettings = await loadSettings();
        const newEnabled = !currentSettings.enabled;
        await chrome.storage.sync.set({
          settings: { ...currentSettings, enabled: newEnabled },
        });
        updateIcon(newEnabled);
        return { enabled: newEnabled };
      }

      case 'updateSettings': {
        const currentSettings = await loadSettings();
        const merged = { ...currentSettings, ...message.settings };
        await chrome.storage.sync.set({ settings: merged });
        return { success: true, settings: merged };
      }

      case 'checkConnection': {
        const isAvailable = await checkFluxDownAvailable();
        return { connected: isAvailable };
      }

      // --- Content Script: 资源检测上报 ---
      case 'resourceDetected': {
        const tabId = sender.tab?.id;
        if (!tabId || tabId < 0) return { success: false };

        const pageUrl = sender.tab?.url || sender.url || '';
        const payloads: ResourceMessagePayload[] = message.resources || [];

        if (payloads.length === 0) return { success: true, added: 0 };

        const added = addResources(tabId, pageUrl, payloads);
        if (added > 0) {
          await updateBadgeForTab(tabId);
          await notifyContentScript(tabId);
        }
        return { success: true, added };
      }

      // --- Content Script UI: 请求当前 tab 的资源列表 ---
      case 'getResources': {
        const tabId = sender.tab?.id;
        if (!tabId || tabId < 0) return { resources: [] };
        return { resources: getResourcesForTab(tabId) };
      }

      // --- Content Script UI / Popup: 触发单个资源下载 ---
      case 'downloadResource': {
        const url = message.url as string;
        if (!url) return { success: false, message: 'No URL' };
        await sendToFluxDown(
          url,
          message.referrer,
          message.filename,
          message.fileSize,
          message.mimeType,
        );
        return { success: true };
      }

      // --- Popup: 切换资源面板显示（发消息给当前活跃 tab 的 Content Script） ---
      case 'toggleResourcePanel': {
        try {
          const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true });
          if (activeTab?.id) {
            await chrome.tabs.sendMessage(activeTab.id, { action: 'toggleResourcePanel' });
          }
        } catch {
          // tab 可能未注入 content script
        }
        return { success: true };
      }

      default:
        return { error: 'Unknown action' };
    }
  }

  // ===== 工具函数 =====

  function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * 判断 Content-Type 是否为"下载类型"（即浏览器会将导航转为下载的类型）
   */
  function isDownloadContentType(contentType: string): boolean {
    const ct = contentType.toLowerCase();
    const downloadTypes = [
      'application/octet-stream',
      'application/x-download',
      'application/force-download',
      'application/zip',
      'application/x-rar-compressed',
      'application/x-7z-compressed',
      'application/gzip',
      'application/x-tar',
      'application/x-bzip2',
      'application/x-xz',
      'application/x-msdownload',
      'application/x-msi',
      'application/x-apple-diskimage',
      'application/vnd.debian.binary-package',
      'application/x-iso9660-image',
      'application/x-raw-disk-image',
      'application/pdf',
      'application/vnd.android.package-archive',
      'application/x-bittorrent',
    ];
    // 精确匹配 + 前缀匹配
    if (downloadTypes.includes(ct)) return true;
    if (ct.startsWith('video/') || ct.startsWith('audio/')) return true;
    if (ct.startsWith('application/vnd.openxmlformats-officedocument')) return true;
    if (ct.startsWith('application/vnd.ms-')) return true;
    return false;
  }

  /**
   * 从 Content-Disposition 头解析文件名
   *
   * 支持格式：
   * - Content-Disposition: attachment; filename="report.pdf"
   * - Content-Disposition: attachment; filename=report.pdf
   * - Content-Disposition: attachment; filename*=UTF-8''%E6%8A%A5%E5%91%8A.pdf
   */
  function parseContentDispositionFilename(disposition: string): string {
    if (!disposition) return '';

    // 优先尝试 filename*（RFC 5987 编码）
    const starMatch = disposition.match(/filename\*\s*=\s*(?:UTF-8|utf-8)'[^']*'(.+?)(?:;|$)/i);
    if (starMatch) {
      try {
        return decodeURIComponent(starMatch[1].trim());
      } catch {
        // fallthrough
      }
    }

    // 再尝试 filename="..."（带引号）
    const quotedMatch = disposition.match(/filename\s*=\s*"(.+?)"/i);
    if (quotedMatch) {
      return quotedMatch[1];
    }

    // 最后尝试 filename=...（无引号）
    const plainMatch = disposition.match(/filename\s*=\s*([^\s;]+)/i);
    if (plainMatch) {
      return plainMatch[1];
    }

    return '';
  }

  /**
   * 从浏览器的 downloadItem.filename（本地保存路径）和 URL 中提取有意义的文件名。
   *
   * 策略：
   * 1. 如果浏览器给出的 filename 有合法扩展名 → 使用它（浏览器已解析了 Content-Disposition）
   * 2. 否则尝试从 URL 路径提取
   * 3. 如果都无法获得有意义的文件名 → 返回空字符串，交给 Rust 引擎通过 HTTP 探测获取
   */
  function extractCleanFilename(browserFilename: string | undefined, url: string): string {
    // 从浏览器的本地路径中提取纯文件名
    if (browserFilename) {
      // downloadItem.filename 是完整路径，如 "C:\Users\xxx\Downloads\report.pdf"
      // 或 "/home/user/Downloads/report.pdf"
      const basename = browserFilename.split(/[/\\]/).pop() || '';
      if (basename && looksLikeRealFilename(basename)) {
        return basename;
      }
    }

    // 从 URL 路径提取
    try {
      const pathname = new URL(url).pathname;
      const segments = pathname.split('/');
      const lastSegment = decodeURIComponent(segments[segments.length - 1] || '');
      if (lastSegment && looksLikeRealFilename(lastSegment)) {
        return lastSegment;
      }
    } catch {
      // ignore
    }

    // 无法确定有意义的文件名，返回空字符串
    // Rust 端会通过 HTTP HEAD/GET 探测 Content-Disposition 获取真实文件名
    return '';
  }

  /**
   * 判断一个文件名是否看起来像真实的文件名（而非 CDN hash / UUID / 无意义路径段）
   *
   * 真实文件名特征：有常见扩展名，如 "report.pdf", "video.mp4"
   * 非真实文件名：纯 hash "a1b2c3d4e5f6", UUID "550e8400-e29b-41d4-a716-446655440000",
   *               无扩展名 "download", 单字母段 "f", 短 ID "j5g6z92sied"
   */
  function looksLikeRealFilename(name: string): boolean {
    // 必须包含扩展名（至少一个点，且点后有 1-10 个字母/数字）
    const extMatch = name.match(/\.([a-zA-Z0-9]{1,10})$/);
    if (!extMatch) return false;

    // 排除看起来像网页路径的扩展名
    const webExts = ['html', 'htm', 'php', 'asp', 'aspx', 'jsp', 'cgi'];
    if (webExts.includes(extMatch[1].toLowerCase())) return false;

    return true;
  }

  function notify(title: string, message: string) {
    chrome.notifications.create({
      type: 'basic',
      iconUrl: '/icon/128.png',
      title: `FluxDown - ${title}`,
      message,
    });
  }

  function updateIcon(enabled: boolean) {
    const suffix = enabled ? '' : '-disabled';
    chrome.action.setIcon({
      path: {
        16: `/icon/16${suffix}.png`,
        32: `/icon/32${suffix}.png`,
        48: `/icon/48${suffix}.png`,
        128: `/icon/128${suffix}.png`,
      },
    });
  }

  // 启动时检查连接状态
  loadSettings().then((s) => updateIcon(s.enabled));
});
