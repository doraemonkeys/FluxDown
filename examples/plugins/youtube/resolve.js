// FluxDown 插件：YouTube 视频解析（classic script，入口挂 globalThis）。
//
// 原理：调用 YouTube Innertube `/youtubei/v1/player`，伪装移动端客户端
// （返回的 streamingData 直链未做 signatureCipher 加密，且当前不要求 PO Token），
// 从 adaptiveFormats 里按设置挑选视频+音频直链。
//
// 抗封锁策略（LOGIN_REQUIRED "confirm you're not a bot" 为 IP 级间歇性风控）：
//   1. 多客户端回退链：ANDROID_VR → IOS → ANDROID，任一通过即用；
//   2. 首个客户端遇 LOGIN_REQUIRED 时获取 visitorData（匿名访客凭据）重试整条链；
//   3. visitorData 缓存于 flux.storage 复用，失效（仍被拒）时强制刷新一次。
//
// 返回值约定（ResolveResult）：
//   url        — 视频（或纯音频）直链
//   audioUrl   — 音视频分离时的音频直链，引擎按 DASH 分离下载后合并
//   ephemeral  — googlevideo 直链带签名且限时（~6h），置 true 跳过 probe，
//                每次 start/resume 重新 resolve 拿新链（惰性 resolve 天然防过期）
//   rangeSupported — googlevideo 完整支持 HTTP Range（YouTube 播放器本身靠
//                Range 分片拉流），置 true 让引擎跳过 probe 的同时仍按多段
//                并发下载，不落入保守单流启动

var INNERTUBE_PLAYER = 'https://www.youtube.com/youtubei/v1/player?prettyPrint=false';
var INNERTUBE_VISITOR = 'https://www.youtube.com/youtubei/v1/visitor_id?prettyPrint=false';
var VISITOR_KEY = 'visitorData';

// 回退链按「直链质量 + 风控通过率」排序。全部返回未加密 url 的客户端。
var CLIENTS = [
  {
    label: 'ANDROID_VR',
    clientName: '28',
    context: {
      clientName: 'ANDROID_VR',
      clientVersion: '1.62.27',
      deviceMake: 'Oculus',
      deviceModel: 'Quest 3',
      osName: 'Android',
      osVersion: '12L',
      androidSdkVersion: 32,
    },
    userAgent:
      'com.google.android.apps.youtube.vr.oculus/1.62.27 ' +
      '(Linux; U; Android 12L; eureka-user Build/SQ3A.220605.009.A1) gzip',
  },
  {
    label: 'IOS',
    clientName: '5',
    context: {
      clientName: 'IOS',
      clientVersion: '20.10.4',
      deviceMake: 'Apple',
      deviceModel: 'iPhone16,2',
      osName: 'iPhone',
      osVersion: '18.3.2.22D82',
    },
    userAgent: 'com.google.ios.youtube/20.10.4 (iPhone16,2; U; CPU iOS 18_3_2 like Mac OS X;)',
  },
  {
    label: 'ANDROID',
    clientName: '3',
    context: {
      clientName: 'ANDROID',
      clientVersion: '20.10.38',
      osName: 'Android',
      osVersion: '11',
      androidSdkVersion: 30,
    },
    userAgent: 'com.google.android.youtube/20.10.38 (Linux; U; Android 11) gzip',
  },
];

// 从各类 YouTube URL 形态中抽取 11 位 videoId。
function extractVideoId(url) {
  var patterns = [
    /[?&]v=([A-Za-z0-9_-]{11})/,           // watch?v=
    /youtu\.be\/([A-Za-z0-9_-]{11})/,      // 短链
    /\/shorts\/([A-Za-z0-9_-]{11})/,       // Shorts
    /\/embed\/([A-Za-z0-9_-]{11})/,        // 嵌入页
    /\/live\/([A-Za-z0-9_-]{11})/,         // 直播回放
  ];
  for (var i = 0; i < patterns.length; i++) {
    var m = patterns[i].exec(url);
    if (m) return m[1];
  }
  return null;
}

// Windows/Unix 通用的文件名净化。
function sanitizeFileName(name) {
  return name
    .replace(/[\\/:*?"<>|\u0000-\u001f]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 120) || 'youtube-video';
}

function parseQualityHeight(quality) {
  var m = /^(\d+)p$/.exec(quality);
  return m ? Number(m[1]) : Infinity; // 'best' → 不设上限
}

// 在 adaptiveFormats 中挑视频流：<= 目标高度里取最高；容器偏好仅作排序权重，
// 目标高度下无偏好容器时回退另一容器（不因容器损失画质档位）。
function pickVideo(formats, maxHeight, preferMp4) {
  var candidates = formats.filter(function (f) {
    return (
      f.url &&
      f.qualityLabel &&
      f.height &&
      f.height <= maxHeight &&
      /^video\//.test(f.mimeType || '')
    );
  });
  candidates.sort(function (a, b) {
    if (a.height !== b.height) return b.height - a.height;
    var aMp4 = /^video\/mp4/.test(a.mimeType) ? 1 : 0;
    var bMp4 = /^video\/mp4/.test(b.mimeType) ? 1 : 0;
    if (aMp4 !== bMp4) return preferMp4 ? bMp4 - aMp4 : aMp4 - bMp4;
    return (b.bitrate || 0) - (a.bitrate || 0);
  });
  return candidates[0] || null;
}

// 挑音频流：偏好 m4a（audio/mp4，与 mp4 视频合并无需转码），按码率取最高。
function pickAudio(formats) {
  var candidates = formats.filter(function (f) {
    return f.url && /^audio\//.test(f.mimeType || '');
  });
  candidates.sort(function (a, b) {
    var aM4a = /^audio\/mp4/.test(a.mimeType) ? 1 : 0;
    var bM4a = /^audio\/mp4/.test(b.mimeType) ? 1 : 0;
    if (aM4a !== bM4a) return bM4a - aM4a;
    return (b.bitrate || 0) - (a.bitrate || 0);
  });
  return candidates[0] || null;
}

function extOf(mimeType, isAudio) {
  if (/webm/.test(mimeType)) return isAudio ? '.webm' : '.webm';
  return isAudio ? '.m4a' : '.mp4';
}

// 获取匿名访客凭据（visitorData），风控重试用。失败返回 null（不阻断主流程）。
async function fetchVisitorData(verbose) {
  try {
    var resp = await flux.fetch({
      method: 'POST',
      url: INNERTUBE_VISITOR,
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        context: { client: { clientName: 'WEB', clientVersion: '2.20250312.04.00', hl: 'en', gl: 'US' } },
      }),
    });
    if (resp.status !== 200) return null;
    var vd = JSON.parse(resp.body).responseContext;
    vd = vd && vd.visitorData;
    if (vd && verbose) flux.logger.info('[youtube] fetched visitorData (len=' + vd.length + ')');
    return vd || null;
  } catch (e) {
    if (verbose) flux.logger.warn('[youtube] visitor_id fetch failed:', String(e));
    return null;
  }
}

// 单客户端调 player。返回 { ok, player?, status?, reason? }。
async function callPlayer(client, videoId, visitorData, cookies) {
  var clientCtx = {};
  for (var k in client.context) clientCtx[k] = client.context[k];
  clientCtx.hl = 'en';
  clientCtx.gl = 'US';
  if (visitorData) clientCtx.visitorData = visitorData;

  var headers = {
    'content-type': 'application/json',
    'user-agent': client.userAgent,
    'x-youtube-client-name': client.clientName,
    'x-youtube-client-version': client.context.clientVersion,
  };
  if (visitorData) headers['x-goog-visitor-id'] = visitorData;
  if (cookies) headers['cookie'] = cookies;

  var resp = await flux.fetch({
    method: 'POST',
    url: INNERTUBE_PLAYER,
    headers: headers,
    body: JSON.stringify({
      context: { client: clientCtx },
      videoId: videoId,
      contentCheckOk: true,
      racyCheckOk: true,
    }),
  });
  if (resp.status !== 200) {
    return { ok: false, status: 'HTTP_' + resp.status, reason: '' };
  }
  if (resp.truncated) {
    return { ok: false, status: 'TRUNCATED', reason: 'Innertube 响应超出体积上限' };
  }
  var player = JSON.parse(resp.body);
  var ps = player.playabilityStatus || {};
  if (ps.status !== 'OK') {
    return { ok: false, status: ps.status || 'UNKNOWN', reason: ps.reason || '' };
  }
  return { ok: true, player: player };
}

// 依次尝试回退链；遇 LOGIN_REQUIRED（bot 风控）时补 visitorData 重试整条链。
async function resolvePlayer(videoId, cookies, verbose) {
  var visitorData = await flux.storage.get(VISITOR_KEY);
  if (verbose) {
    // 出口诊断：风控是 IP 级的，先看引擎实际用哪个出口访问外网。
    try {
      var ipResp = await flux.fetch({ url: 'https://api.ipify.org?format=json' });
      flux.logger.info('[youtube] egress ip =', ipResp.body.trim());
    } catch (e) {
      flux.logger.warn('[youtube] egress probe failed:', String(e));
    }
  }
  var failures = [];
  var attempts = visitorData ? [visitorData] : [null];

  for (var round = 0; round < attempts.length; round++) {
    var vd = attempts[round];
    for (var i = 0; i < CLIENTS.length; i++) {
      var client = CLIENTS[i];
      var r = await callPlayer(client, videoId, vd, cookies);
      if (r.ok) {
        if (verbose) {
          flux.logger.info('[youtube]', videoId, 'client=' + client.label, vd ? '(visitorData)' : '');
        }
        return r.player;
      }
      failures.push(client.label + (vd ? '+vd' : '') + '=' + r.status);
      if (verbose) {
        flux.logger.warn('[youtube]', client.label, '→', r.status, r.reason);
      }
      // 非风控类拒绝（地区/会员/删除）→ 换客户端也无意义的状态仅 UNPLAYABLE 例外，
      // ERROR（视频不存在）直接终止整条链。
      if (r.status === 'ERROR') {
        throw new Error('YouTube 拒绝播放 [ERROR]: ' + (r.reason || '视频不存在或已删除'));
      }
      // 首次遇风控 → 追加一轮带（新）visitorData 的重试。
      if (r.status === 'LOGIN_REQUIRED' && attempts.length === round + 1 && attempts.length < 2) {
        var fresh = await fetchVisitorData(verbose);
        if (fresh && fresh !== vd) {
          await flux.storage.set(VISITOR_KEY, fresh);
          attempts.push(fresh);
        }
      }
    }
  }
  // 全链被拒：用 oembed 判别是「视频已删除/不存在」还是真·IP 风控。
  // YouTube 的 bot 墙（LOGIN_REQUIRED）按「出口 IP 信誉 × 视频热度」打分：
  // 数据中心/代理 IP 访问非热门视频时，所有免登录客户端都会被要求登录验证。
  var exists = null; // null = oembed 探测失败，不下结论
  try {
    var oe = await flux.fetch({
      url: 'https://www.youtube.com/oembed?format=json&url=' +
        encodeURIComponent('https://www.youtube.com/watch?v=' + videoId),
    });
    if (oe.status === 200) exists = true;
    else if (oe.status >= 400 && oe.status < 500) exists = false;
  } catch (e) {
    // oembed 自身失败 → 不影响主错误。
  }
  if (exists === false) {
    throw new Error('视频不存在或已删除: ' + videoId);
  }
  throw new Error(
    'YouTube 要求登录验证 [' + failures.join(', ') + ']。' +
    (exists ? '视频确认存在，' : '') +
    '当前网络出口（代理/数据中心 IP）被 YouTube 风控，对非热门视频强制登录。' +
    '解决办法：切换代理节点（优先住宅 IP）后重试，或稍后再试。'
  );
}

globalThis.resolve = async (ctx) => {
  var verbose = flux.settings.verbose;
  var videoId = extractVideoId(ctx.url);
  if (!videoId) {
    // 非视频页（频道页/播放列表等误匹配）→ 放行原始 URL。
    if (verbose) flux.logger.info('[youtube] no videoId in', ctx.url, '— pass through');
    return null;
  }

  var player = await resolvePlayer(videoId, ctx.cookies, verbose);

  var sd = player.streamingData || {};
  var adaptive = sd.adaptiveFormats || [];
  var title = (player.videoDetails && player.videoDetails.title) || videoId;
  var base = sanitizeFileName(title);
  var quality = flux.settings.quality;
  var preferMp4 = flux.settings.preferMp4;

  // 仅音频模式：单流下载，无需合并。
  if (quality === 'audio') {
    var audioOnly = pickAudio(adaptive);
    if (!audioOnly) throw new Error('未找到可用音频流: ' + videoId);
    if (verbose) {
      flux.logger.info('[youtube]', videoId, 'audio itag=' + audioOnly.itag, audioOnly.mimeType);
    }
    return {
      url: audioOnly.url,
      fileName: base + extOf(audioOnly.mimeType, true),
      totalBytes: Number(audioOnly.contentLength) || null,
      ephemeral: true,
      rangeSupported: true,
    };
  }

  var maxHeight = parseQualityHeight(quality);
  var video = pickVideo(adaptive, maxHeight, preferMp4);

  // 目标画质下无独立视频流时回退 muxed 渐进流（如 itag 18，自带音轨）。
  if (!video) {
    var muxed = (sd.formats || []).filter(function (f) { return f.url; });
    muxed.sort(function (a, b) { return (b.height || 0) - (a.height || 0); });
    if (!muxed.length) throw new Error('未找到可用视频流: ' + videoId);
    var m = muxed[0];
    if (verbose) flux.logger.info('[youtube]', videoId, 'muxed itag=' + m.itag, m.qualityLabel);
    return {
      url: m.url,
      fileName: base + extOf(m.mimeType, false),
      totalBytes: Number(m.contentLength) || null,
      ephemeral: true,
      rangeSupported: true,
    };
  }

  var audio = pickAudio(adaptive);
  if (verbose) {
    flux.logger.info(
      '[youtube]', videoId,
      'video itag=' + video.itag, video.qualityLabel, video.mimeType,
      audio ? 'audio itag=' + audio.itag : 'no-audio'
    );
  }

  var result = {
    url: video.url,
    fileName: base + extOf(video.mimeType, false),
    totalBytes: Number(video.contentLength) || null,
    ephemeral: true,
    rangeSupported: true,
  };
  if (audio) result.audioUrl = audio.url;
  return result;
};
