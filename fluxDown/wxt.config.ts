import { defineConfig } from 'wxt';

export default defineConfig({
  manifest: {
    name: '__MSG_extensionName__',
    description: '__MSG_extensionDescription__',
    default_locale: 'en',
    version: '1.0.0',
    permissions: [
      'downloads',
      'cookies',
      'webRequest',
      'contextMenus',
      'storage',
      'notifications',
      'activeTab',
      'tabs',
      'scripting',
    ],
    host_permissions: ['<all_urls>'],
    web_accessible_resources: [
      {
        resources: ['/fetch-interceptor.js'],
        matches: ['<all_urls>'],
      },
    ],
    action: {
      default_icon: {
        16: '/icon/16.png',
        32: '/icon/32.png',
        48: '/icon/48.png',
        128: '/icon/128.png',
      },
    },
    icons: {
      16: '/icon/16.png',
      32: '/icon/32.png',
      48: '/icon/48.png',
      128: '/icon/128.png',
    },
  },
});
