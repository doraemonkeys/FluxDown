import type { Messages } from "./en";

const zhCN: Messages = {
  // Nav
  "nav.features": "功能",
  "nav.extension": "浏览器扩展",
  "nav.download": "下载",
  "nav.feedback": "反馈",

  // Hero
  "hero.badge": "由 Rust 驱动",
  "hero.title1": "下载，",
  "hero.title2": "全面加速。",
  "hero.subtitle":
    "一款现代化的下载管理器，支持多线程加速、智能分段和浏览器集成。极速下载，永久免费。",
  "hero.cta": "下载 Windows 版",
  "hero.stat1.value": "10x",
  "hero.stat1.label": "下载加速",
  "hero.stat2.value": "100%",
  "hero.stat2.label": "完全免费",
  "hero.stat3.value": "多协议",
  "hero.stat3.label": "全面支持",

  // Hero mockup
  "mockup.category": "分类",
  "mockup.allFiles": "全部文件",
  "mockup.video": "视频",
  "mockup.audio": "音频",
  "mockup.document": "文档",
  "mockup.image": "图片",
  "mockup.archive": "压缩包",
  "mockup.other": "其他",
  "mockup.tabAll": "全部",
  "mockup.tabDownloading": "下载中",
  "mockup.tabCompleted": "已完成",
  "mockup.tabPaused": "已暂停",
  "mockup.tabError": "出错",
  "mockup.colFilename": "文件名",
  "mockup.colProgress": "进度",
  "mockup.colSpeed": "速度",
  "mockup.colStatus": "状态",
  "mockup.download": "下载",
  "mockup.downloading": "下载中",
  "mockup.statusActive": "{n} 活跃 · {p} 暂停 · {t} 总计",
  "mockup.noTasks": "暂无任务",
  "mockup.detail": "详情",
  "mockup.distLabel": "下载分布",
  "mockup.labelSize": "大小",
  "mockup.labelDownloaded": "已下载",
  "mockup.labelSpeed": "速度",
  "mockup.labelRemaining": "剩余",
  "mockup.labelStatus": "状态",
  "mockup.labelThreads": "线程",
  "mockup.labelPath": "路径",
  "mockup.labelUrl": "地址",
  "mockup.labelError": "错误",
  "mockup.threadsValue": "{n} 线程",
  "mockup.btnPause": "暂停",
  "mockup.btnResume": "继续",
  "mockup.btnDelete": "删除",
  "mockup.statusPaused": "已暂停",
  "mockup.statusCompleted": "已完成",
  "mockup.statusDownloading": "下载中",
  "mockup.statusError": "出错",
  "mockup.subtitlePaused": "已暂停",
  "mockup.subtitleTimeout": "连接超时",
  "mockup.eta": "{n} 秒",
  "mockup.errorTimeout": "连接超时 (ETIMEDOUT)",

  // Features
  "features.badge": "核心功能",
  "features.title": "极速下载，",
  "features.titleHighlight": "一应俱全",
  "features.subtitle":
    "基于现代技术构建的强大下载管理器，为你带来卓越的性能与可靠性。",
  "features.rustTitle": "Rust 高性能引擎",
  "features.rustDesc":
    "基于 Rust 和 Tokio 构建，实现最大吞吐量。零开销抽象在保证内存安全的同时，提供原生级别的并发下载性能。",
  "features.segTitle": "智能分段",
  "features.segDesc":
    "IDM 风格的智能文件分段。根据文件大小、CPU 核心数和可用带宽，自动计算最优分段数量。",
  "features.protoTitle": "多协议支持",
  "features.protoDesc":
    "开箱即用的 HTTP、HTTPS 和 FTP 支持。每种协议都有专属的优化下载引擎，确保最大传输速率。",
  "features.speedTitle": "速度控制",
  "features.speedDesc":
    "基于令牌桶算法的全局限速器。设置带宽限制，让下载在后台运行的同时保持流畅的浏览体验。",
  "features.resumeTitle": "断点续传",
  "features.resumeDesc":
    "完整的断点续传支持。所有下载状态持久化到 SQLite — 安全关闭和重启，不丢失任何一个字节。",
  "features.browserTitle": "浏览器集成",
  "features.browserDesc":
    "Chrome 扩展自动拦截下载。可配置文件类型过滤器、域名规则和大小阈值，实现无缝工作流。",

  // Extension
  "ext.badge": "浏览器扩展",
  "ext.title": "无缝接管",
  "ext.titleHighlight": "下载任务",
  "ext.subtitle":
    "安装 Chrome 扩展，自动拦截浏览器下载并发送到 FluxDown。支持任意网站，可按需配置。",
  "ext.feat1.title": "一键拦截",
  "ext.feat1.desc": "自动捕获下载请求，或通过右键菜单手动发送，完全掌控",
  "ext.feat2.title": "本地通信",
  "ext.feat2.desc":
    "安全的 localhost:19527 HTTP 端点 — 无云端、无追踪，数据全部本地化",
  "ext.feat3.title": "智能过滤",
  "ext.feat3.desc": "按文件扩展名、域名黑白名单和最小文件大小进行过滤",
  "ext.addToChrome": "添加到 Chrome",
  "ext.connected": "已连接",
  "ext.paused": "已暂停",
  "ext.today": "今日",
  "ext.thisWeek": "本周",
  "ext.total": "总计",
  "ext.autoIntercept": "自动拦截",
  "ext.recentCatches": "最近拦截",
  "ext.fileTypeFilters": "文件类型过滤",
  "ext.minFileSize": "最小文件大小",

  // Download
  "dl.badge": "下载",
  "dl.title": "准备好",
  "dl.titleHighlight": "加速了吗",
  "dl.subtitle": "下载适合你平台的 FluxDown。永久免费。",
  "dl.windows": "Windows",
  "dl.macos": "macOS",
  "dl.linux": "Linux",
  "dl.availableNow": "立即可用",
  "dl.comingSoon": "即将推出",
  "dl.downloadBtn": "下载",
  "dl.version": "v{version}",
  "dl.loading": "加载中...",
  "dl.installPkg": "安装包",
  "dl.portablePkg": "便携版",
  "dl.extensionTitle": "浏览器扩展",
  "dl.extensionDesc":
    "拦截浏览器下载并发送到 FluxDown，支持 Chrome 和 Firefox。",
  "dl.downloadExtension": "下载扩展",
  "dl.totalDownloads": "次下载",

  // Feedback
  "fb.badge": "\u53cd\u9988\u5efa\u8bae",
  "fb.title": "\u5e2e\u52a9\u6211\u4eec\u505a\u5f97",
  "fb.titleHighlight": "\u66f4\u597d",
  "fb.subtitle":
    "\u6709\u529f\u80fd\u60f3\u6cd5\u6216\u53d1\u73b0\u4e86 Bug\uff1f\u6211\u4eec\u5f88\u4e50\u610f\u542c\u53d6\u4f60\u7684\u610f\u89c1\u3002\u4f60\u7684\u53cd\u9988\u5c06\u5e2e\u52a9\u5851\u9020 FluxDown \u7684\u672a\u6765\u3002",
  "fb.typeLabel": "\u53cd\u9988\u7c7b\u578b",
  "fb.type.feature": "\u529f\u80fd\u5efa\u8bae",
  "fb.type.bug": "Bug \u62a5\u544a",
  "fb.type.other": "\u5176\u4ed6",
  "fb.titleLabel": "\u6807\u9898",
  "fb.titlePlaceholder": "\u7b80\u8981\u63cf\u8ff0\u4f60\u7684\u53cd\u9988",
  "fb.descLabel": "\u8be6\u7ec6\u63cf\u8ff0",
  "fb.descPlaceholder":
    "\u8be6\u7ec6\u63cf\u8ff0\u4f60\u7684\u60f3\u6cd5\u6216\u9047\u5230\u7684\u95ee\u9898...",
  "fb.contactLabel": "\u8054\u7cfb\u65b9\u5f0f",
  "fb.contactPlaceholder":
    "\u90ae\u7bb1\u6216\u5176\u4ed6\u8054\u7cfb\u65b9\u5f0f",
  "fb.optional": "\u53ef\u9009",
  "fb.submit": "\u63d0\u4ea4\u53cd\u9988",
  "fb.submitting": "\u63d0\u4ea4\u4e2d...",
  "fb.success": "\u611f\u8c22\u4f60\u7684\u53cd\u9988\uff01",
  "fb.submitError":
    "\u63d0\u4ea4\u5931\u8d25\uff0c\u8bf7\u7a0d\u540e\u91cd\u8bd5\u3002",
  "fb.rateLimited":
    "\u63d0\u4ea4\u592a\u9891\u7e41\uff0c\u8bf7\u7a0d\u7b49\u7247\u523b\u3002",

  // 404
  "notFound.title": "页面未找到",
  "notFound.desc": "你访问的页面不存在或已被移动。",
  "notFound.home": "返回首页",
  "notFound.feedback": "发送反馈",

  // FAQ
  "faq.badge": "常见问题",
  "faq.title": "常见",
  "faq.titleHighlight": "问题解答",
  "faq.subtitle": "关于 FluxDown 你需要知道的一切。",
  "faq.moreQuestions": "还有其他问题？",
  "faq.contactUs": "给我们发送反馈",
  "faq.items.0.q": "FluxDown 是免费的吗？",
  "faq.items.0.a":
    "是的，FluxDown 完全免费，没有广告、没有订阅、没有隐藏费用。所有功能对每位用户开放。",
  "faq.items.1.q": "FluxDown 如何加速下载？",
  "faq.items.1.a":
    "FluxDown 使用多线程下载和智能分段技术。它将文件拆分为多个部分并同时下载，原理类似 IDM。基于 Rust 的引擎确保了最大吞吐量和最低的资源占用。",
  "faq.items.2.q": "FluxDown 安全吗？",
  "faq.items.2.a":
    "完全安全。FluxDown 使用 Rust 构建，保证内存安全。浏览器扩展仅通过本地地址 (127.0.0.1:19527) 通信——不会向外部服务器发送任何数据。所有下载数据都保留在你的设备上。",
  "faq.items.3.q": "支持哪些浏览器？",
  "faq.items.3.a":
    "浏览器扩展支持 Chrome、Edge 及其他基于 Chromium 的浏览器，同时也支持 Firefox。扩展会自动拦截下载并发送到 FluxDown 进行加速下载。",
  "faq.items.4.q": "FluxDown 和 IDM 有什么区别？",
  "faq.items.4.a":
    "FluxDown 提供类似的多线程下载加速功能，但完全免费，且使用现代技术（Rust + Flutter）构建。支持 HTTP、HTTPS 和 FTP 协议，具备基于系统配置的智能分段功能，提供原生桌面体验。",
  "faq.items.5.q": "FluxDown 支持断点续传吗？",
  "faq.items.5.a":
    "支持。FluxDown 具备完整的断点续传功能。所有下载进度都持久化到本地 SQLite 数据库中。你可以安全地关闭应用或重启电脑，不会丢失任何进度。",
  "faq.items.6.q": "支持哪些操作系统？",
  "faq.items.6.a":
    "目前完整支持 Windows。macOS 和 Linux 支持已在规划中，即将推出。",
  "faq.items.7.q": "如何安装浏览器扩展？",
  "faq.items.7.a":
    "从下载区域下载扩展 zip 文件，解压后打开浏览器的扩展管理页面（chrome://extensions），开启开发者模式，点击「加载已解压的扩展程序」选择解压后的文件夹即可。",

  // Footer
  "footer.desc": "由 Rust 驱动的现代多协议下载管理器。高速、可靠、完全免费。",
  "footer.product": "产品",
  "footer.features": "功能特性",
  "footer.browserExtension": "浏览器扩展",
  "footer.download": "下载",
  "footer.support": "支持",
  "footer.documentation": "文档",
  "footer.faq": "常见问题",
  "footer.contact": "联系我们",
  "footer.feedback": "反馈建议",
  "footer.copyright": "© {year} FluxDown. 保留所有权利。",
  "footer.builtWith": "用",
  "footer.using": "❤ 和 Astro + Rust 构建",
};

export default zhCN;
