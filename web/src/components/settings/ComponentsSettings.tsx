// 组件：v1 仅 ffmpeg 托管安装管理。
// 状态探测 GET /api/v1/components/ffmpeg，可安装版本 GET .../versions，
// 安装/更新 POST .../install（后台执行，经 WS componentProgress/componentResult 推送），
// 卸载 POST .../uninstall。手动路径复用既有 config 端点（PUT /api/v1/config，
// 键 component.ffmpeg.path，与 native/engine `components::CONFIG_FFMPEG_PATH` 一致）。
//
// 优先级：手动路径 > 托管安装 > 系统 PATH（与引擎 resolve_ffmpeg 语义一致）。
import { useState, type ReactNode } from 'react'
import { RefreshCw, Trash2 } from 'lucide-react'
import { cn } from '../../lib/cn'
import { confirmDialog } from '../../lib/confirm'
import { fmtBytes } from '../../lib/format'
import { translateBackendMessage, useI18n } from '../../lib/i18n'
import type { FfmpegSource } from '../../lib/types'
import { componentProgressStore, componentResultStore, useStore } from '../../lib/ws'
import { CopyButton } from '../CopyButton'
import {
  useFfmpegStatusQuery,
  useFfmpegVersionsQuery,
  useInstallFfmpegMutation,
  useUninstallFfmpegMutation,
} from '../../hooks/useComponents'
import { useConfigMutation, useConfigQuery } from './useConfig'
import { SetRow, SetSelect, TextFieldRow } from './controls'

/** config 键：手动指定的 ffmpeg 路径，须与 native/engine `CONFIG_FFMPEG_PATH` 保持一致。 */
const CONFIG_FFMPEG_PATH = 'component.ffmpeg.path'

const SOURCE_TONE: Record<FfmpegSource, 'accent' | 'neutral' | 'danger'> = {
  manual: 'neutral',
  managed: 'accent',
  system: 'neutral',
  none: 'danger',
}

function SourceBadge({ source }: { source: FfmpegSource }) {
  const { t } = useI18n()
  const tone = SOURCE_TONE[source]
  return (
    <span
      className={cn(
        'rounded-full px-2 py-0.5 text-[11px] font-medium',
        tone === 'accent' && 'bg-accent-weak text-accent',
        tone === 'neutral' && 'bg-surface2 text-text3',
        tone === 'danger' && 'bg-danger/10 text-danger',
      )}
    >
      {t(`components.source.${source}`)}
    </span>
  )
}

/** 路径展示行：等宽字体 + 溢出省略，非空时附复制按钮（对齐 SecuritySettings.AddrRow）。 */
function PathRow({ title, desc, value, empty }: { title: string; desc?: string; value: string; empty: string }) {
  return (
    <SetRow title={title} desc={desc}>
      <div className="token-box" style={{ flex: 1, minWidth: 0 }}>
        <span
          style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
          title={value || undefined}
        >
          {value || empty}
        </span>
        {value ? <CopyButton value={value} /> : null}
      </div>
    </SetRow>
  )
}

export function ComponentsSettings() {
  const { t } = useI18n()
  const { data: config, isLoading: configLoading } = useConfigQuery()
  const configMut = useConfigMutation()
  const { data: status, isLoading: statusLoading, isError: statusError } = useFfmpegStatusQuery()
  const versionsQuery = useFfmpegVersionsQuery(true)
  const installMut = useInstallFfmpegMutation()
  const uninstallMut = useUninstallFfmpegMutation()
  const [selectedVersion, setSelectedVersion] = useState('')

  const progress = useStore(componentProgressStore).ffmpeg
  const result = useStore(componentResultStore)
  const installing = installMut.isPending || progress !== undefined

  const manualPath = config?.[CONFIG_FFMPEG_PATH] ?? ''

  async function uninstall() {
    const ok = await confirmDialog({
      title: t('components.uninstallTitle'),
      message: t('components.uninstallMsg'),
      danger: true,
    })
    if (ok) uninstallMut.mutate()
  }

  const pct =
    progress && progress.totalBytes > 0 ? Math.round((progress.downloadedBytes / progress.totalBytes) * 100) : null

  let versionsBody: ReactNode
  if (versionsQuery.isLoading) {
    versionsBody = (
      <SetRow title={t('components.install')}>
        <span className="set-value">{t('common.loading')}</span>
      </SetRow>
    )
  } else if (versionsQuery.isError || !versionsQuery.data || versionsQuery.data.versions.length === 0) {
    const reason = versionsQuery.error
      ? translateBackendMessage(versionsQuery.error.message)
      : t('components.versionsEmpty')
    versionsBody = (
      <SetRow title={t('components.install')} align="start">
        <p className="text-[12px] text-text3">{t('components.versionsFailed', { error: reason })}</p>
      </SetRow>
    )
  } else {
    const data = versionsQuery.data
    versionsBody = (
      <SetRow title={t('components.install')} desc={t('components.installDesc')}>
        <div className="flex flex-shrink-0 items-center gap-2">
          <SetSelect
            value={selectedVersion}
            onValueChange={setSelectedVersion}
            width={180}
            options={[
              { value: '', label: t('components.latestStable', { version: data.latestStable }) },
              ...data.versions.map((v) => ({ value: v, label: v })),
            ]}
          />
          <button
            type="button"
            className="btn ghost sm flex-shrink-0"
            disabled={installing}
            onClick={() => installMut.mutate(selectedVersion || undefined)}
          >
            <RefreshCw size={14} />
            {installing
              ? t('components.installing')
              : status?.managedVersion
                ? t('components.update')
                : t('components.installNow')}
          </button>
        </div>
      </SetRow>
    )
  }

  return (
    <div className="max-w-[640px]">
      <h2 className="set-title">{t('set.components')}</h2>
      <p className="set-desc">{t('set.components.desc')}</p>

      <h3 className="set-title" style={{ fontSize: 13, marginTop: 0 }}>
        {t('components.ffmpeg')}
      </h3>
      <p className="set-desc">{t('components.ffmpegDesc')}</p>

      <div className="set-group">
        {statusLoading || configLoading ? (
          <SetRow title={t('components.status')}>
            <span className="set-value">{t('common.loading')}</span>
          </SetRow>
        ) : statusError || !status ? (
          <SetRow title={t('components.status')}>
            <span className="set-value text-danger">{t('set.loadFailed')}</span>
          </SetRow>
        ) : (
          <>
            <SetRow title={t('components.status')}>
              <SourceBadge source={status.source} />
            </SetRow>
            <PathRow title={t('components.effectivePath')} value={status.path} empty={t('components.none')} />
            <SetRow title={t('components.version')}>
              <span className="set-value">{status.version || t('components.unknown')}</span>
            </SetRow>
            <PathRow
              title={t('components.systemPath')}
              desc={t('components.systemPathDesc')}
              value={status.systemPath}
              empty={t('components.none')}
            />
          </>
        )}
        <TextFieldRow
          title={t('components.manualPath')}
          desc={t('components.manualPathDesc')}
          value={manualPath}
          placeholder={t('components.manualPathPlaceholder')}
          onCommit={(v) => configMut.mutate({ [CONFIG_FFMPEG_PATH]: v.trim() })}
        />
      </div>

      <div className="set-group">
        {versionsBody}
        {progress && (
          <div className="d-progress" style={{ padding: '0 16px 14px' }}>
            <div className="d-progress-num">
              <b>{pct !== null ? `${pct}%` : '…'}</b>
              <span>
                {fmtBytes(progress.downloadedBytes)}
                {progress.totalBytes > 0 ? ` / ${fmtBytes(progress.totalBytes)}` : ''}
              </span>
            </div>
            <div className="d-bar">
              <i style={{ width: pct !== null ? `${pct}%` : '100%' }} />
            </div>
          </div>
        )}
        {result && result.component === 'ffmpeg' && (
          <p
            className={cn('text-[12px]', result.ok ? 'text-success' : 'text-danger')}
            style={{ padding: '0 16px 14px' }}
          >
            {result.ok ? t('components.installOk') : translateBackendMessage(result.message)}
          </p>
        )}
        {status?.managedVersion ? (
          <SetRow
            title={t('components.uninstall')}
            desc={t('components.uninstallDesc', { version: status.managedVersion })}
          >
            <button
              type="button"
              className="btn ghost sm flex-shrink-0 text-danger"
              disabled={uninstallMut.isPending}
              onClick={() => void uninstall()}
            >
              <Trash2 size={14} />
              {t('components.uninstall')}
            </button>
          </SetRow>
        ) : null}
      </div>
    </div>
  )
}
