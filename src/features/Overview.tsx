import { t } from '../shared/i18n';
import { ArrowDownToLine, ArrowRight, CircleCheck, Clock3, PackageCheck, ShieldCheck, Terminal, WandSparkles, Wrench } from 'lucide-react';
import { Badge, Empty, Notice, Panel } from '../app/components';
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
    const needsRepair = Boolean(selected && !current && selected.capabilities.installVersion);
    const hasTarget = Boolean(target && target !== current);
    const warnings = snapshot.checks.filter((check) => check.level === 'warning' || check.level === 'error');
    const holdDrift = warnings.find((check) => check.id === 'hold-version-drift');
    // Like a first-run takeover: with no terminal-usable install, one action
    // installs the official native build and wires it into the user PATH.
    const manageable = snapshot.installations.filter((item) => item.source !== 'external');
    const native = manageable.find((item) => item.source === 'native');
    const unmanaged = manageable.length === 0;
    const pathOnly = Boolean(native && !native.active && !manageable.some((item) => item.active));
    const desktopCli = snapshot.checks.find((check) => check.id === 'claude-desktop');
    return <div className="page-stack">
    {holdDrift && <Notice tone="warning"><strong>{holdDrift.title}</strong><p>{holdDrift.message}</p></Notice>}
    {unmanaged ? <section className="hero-panel">
      <div className="hero-top"><div className="hero-label"><Terminal size={17}/> CLAUDE CODE</div><Badge tone="accent">{t("等待接管")}</Badge></div>
      <div className="hero-main">
        <div><div className="eyebrow">{t("本机终端")}</div><div className="version-large">{t("尚未安装")}</div>
          <p className="hero-description">{t("一键安装官方原生版本，加入终端 PATH 并完成验证，之后由管理器负责检查与更新。")}</p>
        </div>
        <div className="hero-action">
          <button className="button primary hero-cta" disabled={disabled} onClick={() => onPlan({ kind: 'setup', targetVersion: target })}><WandSparkles size={18}/>{t("一键自动管理")}</button>
          <button className="text-link" onClick={() => onNavigate('versions')}>{t("选择其他版本")}<ArrowRight size={15}/></button>
        </div>
      </div>
      <div className="hero-bottom"><span><CircleCheck size={15}/>{t("未发现终端可用的 Claude Code")}</span><span>{target ? t("官方 {value0} · v{value1}", { value0: snapshot.policy.channel, value1: target }) : t("正在查询发布通道")}</span></div>
    </section> : <section className="hero-panel">
      <div className="hero-top"><div className="hero-label"><Terminal size={17}/> CLAUDE CODE</div><Badge tone={selected ? 'success' : 'neutral'}>{selected ? t("已发现安装") : t("等待安装")}</Badge></div>
      <div className="hero-main">
        <div><div className="eyebrow">{t("当前选择的版本")}</div><div className="version-large">{current ? <><span>v</span>{current}</> : selected ? t("版本未知") : t("尚未安装")}</div>
          <p className="hero-description">{selected ? t("{value0} 安装 · {value1}", { value0: sourceNames[selected.source], value1: selected.scope === 'user' ? t("当前用户") : selected.scope === 'machine' ? t("整台设备") : t("范围待确认") }) : t("从官方来源安装，完成后验证终端中的版本。")}</p>
        </div>
        <div className="hero-action">
          <button className="button primary" disabled={disabled || Boolean(selected && !(needsRepair ? selected.capabilities.installVersion : selected.capabilities.upgrade)) || !target} onClick={() => onPlan({ kind: needsRepair ? 'reinstall' : selected ? 'update' : 'install', installationId: selected?.id, targetVersion: target })}>
            <ArrowDownToLine size={18}/>{needsRepair ? t("预览重新安装") : selected ? (hasTarget ? t("查看更新计划") : t("检查安装计划")) : t("安装 Claude Code")}
          </button>
          <button className="text-link" onClick={() => onNavigate('versions')}>{t("选择其他版本")}<ArrowRight size={15}/></button>
        </div>
      </div>
      <div className="hero-bottom"><span><CircleCheck size={15}/>{selected?.active ? t("当前终端命中此安装") : selected ? t("终端命中情况见环境页") : t("未发现可管理的安装")}</span><span>{target ? t("官方 {value0} · v{value1}", { value0: snapshot.policy.channel, value1: target }) : t("正在查询发布通道")}</span></div>
    </section>}
    {unmanaged && desktopCli && <Notice>{desktopCli.message} {t("「一键自动管理」会另外安装一份终端可用的官方版本，不会改动桌面应用。")}</Notice>}
    {pathOnly && <div className="notice notice-warning setup-notice"><Wrench size={17}/><div><strong>{t("已安装原生 Claude Code，但新终端还找不到它")}</strong><p>{t("一键修复会把原生安装目录设为用户 PATH 中的 Claude Code 入口，并保存可撤销记录。")}</p></div><button className="button secondary small-button" disabled={disabled} onClick={() => onPlan({ kind: 'setup' })}>{t("一键修复")}</button></div>}
    {needsRepair && <Notice>{t("已发现安装文件，但无法读取版本。重新安装前会复核该实例及官方产物。")}</Notice>}

    <div className="summary-grid">
      <button className="summary-card" onClick={() => onNavigate('environment')}><span className="summary-icon"><PackageCheck size={19}/></span><span><span className="summary-title">{t("本机安装")}</span><strong>{snapshot.installations.length} <small>{t("份")}</small></strong><span className="summary-hint">{t("查看来源与实际路径")}</span></span><ArrowRight size={16}/></button>
      <button className="summary-card" onClick={() => onNavigate('settings')}><span className="summary-icon"><ShieldCheck size={19}/></span><span><span className="summary-title">{t("更新策略")}</span><strong className="summary-word">{policyNames[snapshot.policy.mode] ?? snapshot.policy.mode}</strong><span className="summary-hint">{snapshot.policy.channel}{t("通道")}</span></span><ArrowRight size={16}/></button>
    </div>

    {selected && <Panel title={t("安装位置")} action={<button className="text-link" onClick={() => onNavigate('environment')}>{t("查看环境")}<ArrowRight size={14}/></button>}>
      <div className="path-row"><Terminal size={18}/><code>{selected.path}</code><Badge>{sourceNames[selected.source]}</Badge></div>
      {selected.detail && <p className="panel-description">{selected.detail}</p>}
    </Panel>}

    <div className="two-columns">
      <Panel title={t("环境检查")} action={<button className="text-link" onClick={() => onNavigate('environment')}>{t("全部检查")}</button>}>
        {warnings.length ? <div className="compact-list">{warnings.slice(0, 3).map((check) => <button className="check-summary" key={check.id} onClick={() => onNavigate('environment')}><span className={`status-dot ${check.level}`}/><span><strong>{check.title}</strong><small>{check.message}</small></span><ArrowRight size={14}/></button>)}</div> : <div className="healthy"><CircleCheck size={27}/><div><strong>{t("环境检查已完成")}</strong><p>{t("当前检查项均可继续。")}</p></div></div>}
      </Panel>
      <Panel title={t("最近操作")} action={<button className="text-link" onClick={() => onNavigate('history')}>{t("全部记录")}</button>}>
        {snapshot.history.length ? <div className="compact-list">{snapshot.history.slice(0, 2).map((operation) => <div className="history-mini" key={operation.id}><Clock3 size={17}/><div><strong>{operation.title}</strong><small>{displayDate(operation.startedAt)}</small></div><Badge tone={operation.status === 'succeeded' ? 'success' : 'warning'}>{operation.status === 'succeeded' ? t("完成") : t("待核查")}</Badge></div>)}</div> : <Empty title={t("还没有操作记录")}>{t("安装、更新和环境更改会记录在这里。")}</Empty>}
      </Panel>
    </div>
  </div>;
}
