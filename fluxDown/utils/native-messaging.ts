/**
 * HTTP 通信模块
 * 负责与 FluxDown 桌面应用通过本地 HTTP 服务器通信
 *
 * FluxDown 桌面应用在 127.0.0.1:19527 启动 HTTP 服务器，
 * 浏览器扩展直接通过 fetch() 发送请求，无需 Native Messaging。
 */

const FLUXDOWN_BASE_URL = 'http://127.0.0.1:19527';

export interface DownloadRequest {
  url: string;
  filename?: string;
  referrer?: string;
  cookies?: string;
  headers?: Record<string, string>;
  fileSize?: number;
  mimeType?: string;
}

export interface ApiResponse {
  success: boolean;
  message?: string;
  taskId?: string;
}

/**
 * 发送下载请求到 FluxDown 桌面应用
 */
export async function sendDownloadRequest(request: DownloadRequest): Promise<ApiResponse> {
  try {
    const response = await fetch(`${FLUXDOWN_BASE_URL}/download`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
      signal: AbortSignal.timeout(5000),
    });
    return (await response.json()) as ApiResponse;
  } catch (e) {
    console.error('[FluxDown] HTTP request failed:', e);
    return {
      success: false,
      message: e instanceof Error ? e.message : 'Unknown error',
    };
  }
}

/**
 * 批量下载请求的单个条目
 */
export interface BatchDownloadItem {
  url: string;
  filename?: string;
  referrer?: string;
  cookies?: string;
  fileSize?: number;
  mimeType?: string;
}

/**
 * 批量发送下载请求到 FluxDown 桌面应用（单次 HTTP POST）
 *
 * 将所有条目的 URL 用换行符连接，作为一个 DownloadRequest 发送，
 * Flutter 端的 quick_download_dialog 会按换行符拆分并支持批量创建任务。
 */
export async function sendBatchDownloadRequest(items: BatchDownloadItem[]): Promise<ApiResponse> {
  if (items.length === 0) {
    return { success: false, message: 'No items' };
  }

  // 将所有 URL 用换行符连接成一个字符串
  const joinedUrl = items.map((item) => item.url).join('\n');

  // 合并 cookies（取第一个非空的）
  const cookies = items.find((item) => item.cookies)?.cookies || '';

  const request: DownloadRequest = {
    url: joinedUrl,
    filename: '', // 批量下载不支持单独重命名
    referrer: items[0]?.referrer || '',
    cookies,
  };

  try {
    const response = await fetch(`${FLUXDOWN_BASE_URL}/download`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
      signal: AbortSignal.timeout(5000),
    });
    return (await response.json()) as ApiResponse;
  } catch (e) {
    console.error('[FluxDown] Batch HTTP request failed:', e);
    return {
      success: false,
      message: e instanceof Error ? e.message : 'Unknown error',
    };
  }
}

/**
 * 检查 FluxDown 桌面应用是否在运行
 */
export async function checkFluxDownAvailable(): Promise<boolean> {
  try {
    const response = await fetch(`${FLUXDOWN_BASE_URL}/ping`, {
      method: 'GET',
      signal: AbortSignal.timeout(3000),
    });
    const data = (await response.json()) as ApiResponse;
    return data.success === true;
  } catch {
    return false;
  }
}
