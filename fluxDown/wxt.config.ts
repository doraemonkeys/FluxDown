import { defineConfig } from 'wxt';

export default defineConfig({
  manifest: {
    name: 'FluxDown',
    description: '拦截浏览器下载，发送到 FluxDown 桌面应用进行高速下载',
    version: '1.0.0',
    permissions: [
      'downloads',
      'contextMenus',
      'storage',
      'notifications',
      'activeTab',
      'tabs',
    ],
    action: {
      default_icon: {
        16: '/icon/icon.svg',
        32: '/icon/icon.svg',
        48: '/icon/icon.svg',
        128: '/icon/icon.svg',
      },
    },
    icons: {
      16: '/icon/icon.svg',
      32: '/icon/icon.svg',
      48: '/icon/icon.svg',
      128: '/icon/icon.svg',
    },
  },
});
