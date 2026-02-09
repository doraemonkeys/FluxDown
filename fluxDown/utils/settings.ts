/**
 * 插件设置管理模块
 */

export interface FluxDownSettings {
  /** 是否启用下载拦截 */
  enabled: boolean;
  /** 最小文件大小（字节），小于此值的文件不拦截 */
  minFileSize: number;
  /** 拦截的文件扩展名列表（为空则拦截所有） */
  interceptExtensions: string[];
  /** 排除的域名列表 */
  excludeDomains: string[];
  /** 是否显示通知 */
  showNotification: boolean;
}

const DEFAULT_SETTINGS: FluxDownSettings = {
  enabled: true,
  minFileSize: 1024 * 1024, // 1MB
  interceptExtensions: [
    // 压缩文件
    '.zip', '.rar', '.7z', '.tar', '.gz', '.bz2', '.xz',
    // 安装程序
    '.exe', '.msi', '.dmg', '.deb', '.rpm', '.appimage',
    // 磁盘镜像
    '.iso', '.img',
    // 视频
    '.mp4', '.mkv', '.avi', '.mov', '.wmv', '.flv', '.webm',
    // 音频
    '.mp3', '.flac', '.wav', '.aac', '.ogg',
    // 文档
    '.pdf', '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx',
    // 其他大文件
    '.bin', '.apk', '.ipa', '.torrent',
  ],
  excludeDomains: [],
  showNotification: true,
};

/**
 * 加载设置
 */
export async function loadSettings(): Promise<FluxDownSettings> {
  const result = await chrome.storage.sync.get('settings');
  if (result.settings) {
    return { ...DEFAULT_SETTINGS, ...result.settings };
  }
  return { ...DEFAULT_SETTINGS };
}

/**
 * 保存设置
 */
export async function saveSettings(settings: Partial<FluxDownSettings>): Promise<void> {
  const current = await loadSettings();
  const merged = { ...current, ...settings };
  await chrome.storage.sync.set({ settings: merged });
}

/**
 * 重置设置
 */
export async function resetSettings(): Promise<void> {
  await chrome.storage.sync.set({ settings: DEFAULT_SETTINGS });
}

/**
 * 判断 URL 是否应该被拦截
 */
export function shouldIntercept(
  url: string,
  fileSize: number | undefined,
  settings: FluxDownSettings,
): boolean {
  if (!settings.enabled) return false;

  // 检查域名排除
  try {
    const hostname = new URL(url).hostname;
    if (settings.excludeDomains.some((d) => hostname.includes(d))) {
      return false;
    }
  } catch {
    // URL 解析失败，不拦截
    return false;
  }

  // 检查文件大小
  if (fileSize !== undefined && fileSize > 0 && fileSize < settings.minFileSize) {
    return false;
  }

  // 如果设置了扩展名过滤
  if (settings.interceptExtensions.length > 0) {
    const urlPath = new URL(url).pathname.toLowerCase();
    return settings.interceptExtensions.some((ext) => urlPath.endsWith(ext));
  }

  // 默认拦截所有
  return true;
}

export { DEFAULT_SETTINGS };
