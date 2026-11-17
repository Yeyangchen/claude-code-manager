import { useState } from 'react';
import { ArrowDownToLine, ChevronDown, ChevronUp, FileText, ShieldCheck } from 'lucide-react';
import { Badge, Empty, Notice, Panel, ReleaseNotes } from '../app/components';
import type { ManagerApi } from '../services/manager';
import type { Installation, PlanRequest, ReleaseCatalog } from '../shared/types';
import { displayDate } from '../shared/format';

interface Props {
  api: ManagerApi;
  selected: Installation | null;
  catalog: ReleaseCatalog | null;
  disabled: boolean;
  onPlan: (request: PlanRequest) => void;
  onError: (message: string) => void;
}

export function Versions({ api, selected, catalog, disabled, onPlan, onError }: Props) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const [version, setVersion] = useState('');
  const [versionError, setVersionError] = useState('');
  const canSelectVersion = !selected || selected.capabilities.installVersion;
  const choose = (targetVersion: string) => onPlan({ kind: selected ? 'reinstall' : 'install', installationId: selected?.id, targetVersion });

  return <div className="page-stack">
    {catalog?.warning && <Notice tone="warning">{catalog.warning}</Notice>}
    <div className="channel-grid">
      <Panel className="channel-card"><div className="channel-title"><ShieldCheck size={19} /><h2>Stable</h2><Badge>稳定通道</Badge></div><div className="channel-version">{catalog?.stable ? `v${catalog.stable}` : '等待查询'}</div><p>延后接收更新，优先使用经过观察的版本。</p><button className="button secondary" disabled={disabled || !canSelectVersion || !catalog?.stable} onClick={() => catalog && choose(catalog.stable)}>预览安装计划</button></Panel>
      <Panel className="channel-card"><div className="channel-title"><ArrowDownToLine size={19} /><h2>Latest</h2><Badge tone="accent">最新通道</Badge></div><div className="channel-version">{catalog?.latest ? `v${catalog.latest}` : '等待查询'}</div><p>在官方发布后获取新功能和修复。</p><button className="button secondary" disabled={disabled || !canSelectVersion || !catalog?.latest} onClick={() => catalog && choose(catalog.latest)}>预览安装计划</button></Panel>
    </div>
    {!canSelectVersion && <Notice>当前安装来源通过自己的包管理器升级。指定版本重装适用于 Native 安装。</Notice>}
    <Panel title="指定版本" action={<Badge>Native</Badge>}>
      <form className="inline-form" onSubmit={(event) => {
        event.preventDefault();
        if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version.trim())) { setVersionError('请输入完整版本号，例如 2.1.311。'); return; }
        setVersionError(''); choose(version.trim());
      }}>
        <label className="field grow"><span>版本号</span><input value={version} onChange={(event) => { setVersion(event.target.value); setVersionError(''); }} placeholder="例如 2.1.311" autoComplete="off" aria-invalid={Boolean(versionError)} /></label>
        <button className="button secondary" type="submit" disabled={disabled || !canSelectVersion}>查询并预览</button>
      </form>
      {versionError && <p className="field-error" role="alert">{versionError}</p>}
      <p className="panel-description">安装前会重新确认官方产物、版本和校验能力。</p>
    </Panel>
    <Panel title="版本与更新说明" action={catalog && <span className="muted small">{displayDate(catalog.checkedAt)}{catalog.cached ? ' · 缓存' : ''}</span>}>
      {!catalog?.releases.length ? <Empty title="暂无版本信息">使用右上角的检查更新获取官方发布信息。</Empty> : <div className="release-list">{catalog.releases.map((release) => <article key={`${release.source}-${release.version}`} className="release-row">
        <div className="release-main"><div className="release-version"><FileText size={19} /><strong>v{release.version}</strong>{selected?.version === release.version && <Badge tone="success">当前版本</Badge>}{release.channel && <Badge>{release.channel}</Badge>}</div><div className="release-actions"><span className="muted small">{release.verified ? '具备签名校验' : '校验能力待核实'}</span><button className="button small-button secondary" disabled={disabled || !canSelectVersion} onClick={() => choose(release.version)}>{release.available ? '选择版本' : '验证并选择'}</button><button className="icon-button" aria-label={`${expanded === release.version ? '收起' : '查看'} ${release.version} 更新说明`} aria-expanded={expanded === release.version} onClick={() => setExpanded(expanded === release.version ? null : release.version)}>{expanded === release.version ? <ChevronUp size={18} /> : <ChevronDown size={18} />}</button></div></div>
        {expanded === release.version && <ReleaseNotes content={release.notes} api={api} onError={onError} />}
      </article>)}</div>}
    </Panel>
  </div>;
}
