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
 */

import { sendDownloadRequest, checkFluxDownAvailable } from '@/utils/native-messaging';
import type { DownloadRequest } from '@/utils/native-messaging';
import { loadSettings, shouldIntercept } from '@/utils/settings';
import type { DownloadItemInfo } from '@/utils/settings';
import { initI18n, t } from '@/utils/i18n';

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
          // TODO: 实现全部链接提取
          notify(t('notify.featureInDev'), t('notify.batchDownloadComing'));
        }
        return;
    }

    if (url) {
      await sendToFluxDown(url, tab?.url);
    }
  });

  // ===== 下载拦截 =====
  chrome.downloads.onCreated.addListener(async (downloadItem) => {
    const settings = await loadSettings();

    if (!settings.enabled) return;

    const url = downloadItem.url;
    const fileSize = downloadItem.fileSize > 0 ? downloadItem.fileSize : undefined;

    // 跳过 blob 和 data URL
    if (url.startsWith('blob:') || url.startsWith('data:')) return;

    // 构建下载项信息，供综合判断
    const itemInfo: DownloadItemInfo = {
      url,
      fileSize,
      mime: downloadItem.mime || undefined,
      filename: downloadItem.filename || undefined,
    };

    // 判断是否需要拦截
    if (!shouldIntercept(itemInfo, settings)) return;

    console.log('[FluxDown] Intercepting download:', {
      url,
      mime: downloadItem.mime,
      filename: downloadItem.filename,
      fileSize,
      mode: settings.interceptMode,
    });

    // 取消浏览器的下载
    try {
      await chrome.downloads.cancel(downloadItem.id);
      chrome.downloads.erase({ id: downloadItem.id });
    } catch (e) {
      console.warn('[FluxDown] Failed to cancel download:', e);
    }

    // 发送到 FluxDown
    // downloadItem.filename 是浏览器本地保存路径（如 C:\Users\xxx\Downloads\file.zip），
    // 需要提取纯文件名部分；如果看起来不像真实文件名就传空，让 Rust 引擎通过
    // HTTP Content-Disposition 自动探测真实文件名。
    const cleanFilename = extractCleanFilename(downloadItem.filename, url);
    await sendToFluxDown(
      url,
      downloadItem.referrer,
      cleanFilename,
      fileSize,
      downloadItem.mime,
    );
  });

  // ===== Popup 消息处理 =====
  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    handlePopupMessage(message).then(sendResponse);
    return true; // 保持消息通道开放（异步响应）
  });

  // ===== 核心：发送下载请求到 FluxDown App =====
  async function sendToFluxDown(
    url: string,
    referrer?: string,
    filename?: string,
    fileSize?: number,
    mimeType?: string,
  ) {
    const settings = await loadSettings();

    const request: DownloadRequest = {
      url,
      filename: filename || '',
      referrer: referrer || '',
      fileSize,
      mimeType,
    };

    console.log('[FluxDown] Sending to FluxDown app:', request);

    const response = await sendDownloadRequest(request);

    if (response.success) {
      // 统计：接管成功
      await incrementStat('sent');

      if (settings.showNotification) {
        notify(
          t('notify.downloadSent'),
          t('notify.sentToFluxDown', { name: request.filename || url }),
        );
      }
    } else {
      // 统计：接管失败
      await incrementStat('failed');

      notify(
        t('notify.sendFailed'),
        t('notify.connectionFailed', { message: response.message }),
      );
    }
  }

  // ===== Popup 消息处理逻辑 =====
  async function handlePopupMessage(message: any): Promise<any> {
    switch (message.action) {
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

      default:
        return { error: 'Unknown action' };
    }
  }

  // ===== 工具函数 =====

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
