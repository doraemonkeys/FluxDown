// 版本更新检测：对比服务器版本（/api/v1/info）与 GitHub 最新 server-v* release。
// 直查 GitHub API（带 CORS 通配头）；每会话查一次，失败静默（视为无更新）。

import { useQuery } from '@tanstack/react-query'
import { api } from './api'

const RELEASES_URL = 'https://api.github.com/repos/zerx-lab/FluxDown/releases?per_page=30'
const SERVER_TAG_RE = /^server-v(\d+\.\d+\.\d+)$/

interface GitHubRelease {
  tag_name: string
  html_url: string
  draft: boolean
  prerelease: boolean
}

interface LatestServerRelease {
  version: string
  url: string
}

export interface UpdateState {
  /** 当前服务器版本（info 未加载时为 null）。 */
  current: string | null
  /** 最新 server release 版本（检测失败/未完成时为 null）。 */
  latest: string | null
  /** release 页面地址（手动升级入口）。 */
  releaseUrl: string | null
  /** 有可用新版本。 */
  hasUpdate: boolean
}

/** 语义化版本比较：a > b 返回正数。非法段按 0 处理。 */
function cmpVersion(a: string, b: string): number {
  const pa = a.split('.').map((n) => Number.parseInt(n, 10) || 0)
  const pb = b.split('.').map((n) => Number.parseInt(n, 10) || 0)
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0)
    if (d !== 0) return d
  }
  return 0
}

async function fetchLatestServerRelease(): Promise<LatestServerRelease | null> {
  const res = await fetch(RELEASES_URL, {
    headers: { Accept: 'application/vnd.github+json' },
  })
  if (!res.ok) throw new Error(`github releases: ${res.status}`)
  const releases = (await res.json()) as GitHubRelease[]
  for (const r of releases) {
    if (r.draft || r.prerelease) continue
    const m = SERVER_TAG_RE.exec(r.tag_name)
    if (m) return { version: m[1], url: r.html_url }
  }
  return null
}

/** 启动后自动检测新版本；结果全会话缓存，失败静默。 */
export function useUpdateCheck(): UpdateState {
  const { data: info } = useQuery({ queryKey: ['info'], queryFn: api.info })
  const { data: latest } = useQuery({
    queryKey: ['latest-server-release'],
    queryFn: fetchLatestServerRelease,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
    retry: 1,
  })

  const current = info?.version ?? null
  return {
    current,
    latest: latest?.version ?? null,
    releaseUrl: latest?.url ?? null,
    hasUpdate: current != null && latest != null && cmpVersion(latest.version, current) > 0,
  }
}
