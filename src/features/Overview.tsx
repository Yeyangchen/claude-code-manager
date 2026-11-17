import { ArrowDownToLine, ArrowRight, CircleCheck, Clock3, PackageCheck, ShieldCheck, Terminal } from 'lucide-react';
import { Badge, Empty, Panel } from '../app/components';
import { displayDate, policyNames, sourceNames } from '../shared/format';
import type { Installation, PlanRequest, ReleaseCatalog, Snapshot } from '../shared/types';

interface Props {
  snapshot: Snapshot;
  selected: Installation | null;
  catalog: ReleaseCatalog | null;
  disabled: boolean;
  onPlan: (request: PlanRequest) => void;
  onNavigate: (page: 'versions' | 'environment' | 'settings' | 'history') => void;
}

export function Overview({ snapshot, selected, catalog, disabled, onPlan, onNavigate }: Props) {
  const target = snapshot.policy.channel === 'stable' ? catalog?.stable : catalog?.latest;
  const current = selected?.version;
  const hasTarget = Boolean(target && target !== current);
  const warnings = snapshot.checks.filter((check) => check.level === 'warning' || check.level === 'error');

  return <div className="page-stack">
    <section className="hero-panel">
      <div className="hero-top"><div className="hero-label"><Terminal size={17} /> CLAUDE CODE</div><Badge tone={selected ? 'success' : 'neutral'}>{selected ? '已发现安装' : '等待安装'}</Badge></div>
      <div className="hero-main">
        <div><div className="eyebrow">当前选择的版本</div><div className="version-large">{current ? <><span>v</span>{current}</> : '尚未安装'}</div>
          <p className="hero-description">{selected ? `${sourceNames[selected.source]} 安装 · ${selected.scope === 'user' ? '当前用户' : selected.scope === 'machine' ? '整台设备' : '范围待确认'}` : '从官方来源安装，完成后验证终端中的版本。'}</p>
        </div>
        <div className="hero-action">
          <button className="button primary" disabled={disabled || Boolean(selected && !selected.capabilities.upgrade) || !target} onClick={() => onPlan({ kind: selected ? 'update' : 'install', installationId: selected?.id, targetVersion: target })}>
            <ArrowDownToLine size={18} />{selected ? (hasTarget ? '查看更新计划' : '检查安装计划') : '安装 Claude Code'}
          </button>
          <button className="text-link" onClick={() => onNavigate('versions')}>选择其他版本 <ArrowRight size={15} /></button>
        </div>
      </div>
      <div className="hero-bottom"><span><CircleCheck size={15} />{selected?.active ? '当前终端命中此安装' : selected ? '终端命中情况见环境页' : '等待完成安装检测'}</span><span>{target ? `官方 ${snapshot.policy.channel} · v${target}` : '正在查询发布通道'}</span></div>
    </section>

    <div className="summary-grid">
      <button className="summary-card" onClick={() => onNavigate('environment')}><span className="summary-icon"><PackageCheck size={19} /></span><span><span className="summary-title">本机安装</span><strong>{snapshot.installations.length} <small>份</small></strong><span className="summary-hint">查看来源与实际路径</span></span><ArrowRight size={16} /></button>
      <button className="summary-card" onClick={() => onNavigate('settings')}><span className="summary-icon"><ShieldCheck size={19} /></span><span><span className="summary-title">更新策略</span><strong className="summary-word">{policyNames[snapshot.policy.mode] ?? snapshot.policy.mode}</strong><span className="summary-hint">{snapshot.policy.channel} 通道</span></span><ArrowRight size={16} /></button>
    </div>

    {selected && <Panel title="安装位置" action={<button className="text-link" onClick={() => onNavigate('environment')}>查看环境 <ArrowRight size={14} /></button>}>
      <div className="path-row"><Terminal size={18} /><code>{selected.path}</code><Badge>{sourceNames[selected.source]}</Badge></div>
      {selected.detail && <p className="panel-description">{selected.detail}</p>}
    </Panel>}

    <div className="two-columns">
      <Panel title="环境检查" action={<button className="text-link" onClick={() => onNavigate('environment')}>全部检查</button>}>
        {warnings.length ? <div className="compact-list">{warnings.slice(0, 3).map((check) => <button className="check-summary" key={check.id} onClick={() => onNavigate('environment')}><span className={`status-dot ${check.level}`} /><span><strong>{check.title}</strong><small>{check.message}</small></span><ArrowRight size={14} /></button>)}</div> : <div className="healthy"><CircleCheck size={27} /><div><strong>环境检查已完成</strong><p>当前检查项均可继续。</p></div></div>}
      </Panel>
      <Panel title="最近操作" action={<button className="text-link" onClick={() => onNavigate('history')}>全部记录</button>}>
        {snapshot.history.length ? <div className="compact-list">{snapshot.history.slice(0, 2).map((operation) => <div className="history-mini" key={operation.id}><Clock3 size={17} /><div><strong>{operation.title}</strong><small>{displayDate(operation.startedAt)}</small></div><Badge tone={operation.status === 'succeeded' ? 'success' : 'warning'}>{operation.status === 'succeeded' ? '完成' : '待核查'}</Badge></div>)}</div> : <Empty title="还没有操作记录">安装、更新和环境更改会记录在这里。</Empty>}
      </Panel>
    </div>
  </div>;
}
