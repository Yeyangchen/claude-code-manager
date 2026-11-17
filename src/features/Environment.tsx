import { t } from '../shared/i18n';
import { useState } from 'react';
import { AlertTriangle, CircleCheck, Info, Layers, Terminal, Trash2, Wrench } from 'lucide-react';
import { Badge, Empty, Notice, Panel } from '../app/components';
import { sourceNames } from '../shared/format';
import type { PlanRequest, Snapshot } from '../shared/types';
export function Environment({ snapshot, disabled, onPlan, onDiagnostics }: {
    snapshot: Snapshot;
    disabled: boolean;
    onPlan: (request: PlanRequest) => void;
    onDiagnostics: () => void;
}) {
    const [keepId, setKeepId] = useState(snapshot.installations.find((item) => item.source === 'native')?.id ?? snapshot.installations[0]?.id ?? '');
    const [consolidating, setConsolidating] = useState(false);
    const selectedKeepId = snapshot.installations.find((item) => item.id === keepId)?.id ?? snapshot.installations.find((item) => item.source === 'native')?.id ?? snapshot.installations[0]?.id ?? '';
    const remaining = snapshot.installations.filter((item) => item.id !== selectedKeepId);
    return <div className="page-stack">
    <Panel title={t("前置条件")} action={<button className="text-link" onClick={onDiagnostics}>{t("查看脱敏诊断")}</button>}>
      <div className="checks-grid">{snapshot.checks.map((check) => <div key={check.id} className={`check-card check-${check.level}`}>{check.level === 'ok' ? <CircleCheck size={19}/> : check.level === 'warning' || check.level === 'error' ? <AlertTriangle size={19}/> : <Info size={19}/>}<div><strong>{check.title}</strong><p>{check.message}</p></div></div>)}</div>
    </Panel>

    <Panel title={t("安装实例")} action={snapshot.installations.length > 1 && <button className="button secondary small-button" onClick={() => setConsolidating(!consolidating)}><Layers size={15}/>{consolidating ? t("关闭收敛引导") : t("整理多份安装")}</button>}>
      {consolidating && <div className="consolidation"><h3>{t("选择要保留的安装")}</h3><p>{t("逐个预览其余实例的卸载计划，完成后重新核对终端命中关系。")}</p><label className="field"><span>{t("保留项")}</span><select value={selectedKeepId} onChange={(event) => setKeepId(event.target.value)}>{snapshot.installations.map((item) => <option key={item.id} value={item.id}>{sourceNames[item.source]} · {item.version ?? t("版本待确认")} · {item.path}</option>)}</select></label>{remaining.length === 0 && <Notice tone="success">{t("已保留选定实例，请重新检查终端命中路径。")}</Notice>}</div>}
      {snapshot.installations.length === 0 ? <Empty title={t("未发现 Claude Code 安装")}>{t("可在概览页安装官方 Native 版本。")}</Empty> : <div className="installation-list">{snapshot.installations.map((item) => <article className="installation" key={item.id}>
        <div className="installation-head"><div className="installation-name"><span className="source-icon"><Terminal size={18}/></span><strong>{sourceNames[item.source]}</strong><span className="mono">{item.version ? `v${item.version}` : t("版本未知")}</span>{item.active && <Badge tone="success">{t("当前命中")}</Badge>}{consolidating && item.id === selectedKeepId && <Badge tone="accent">{t("保留")}</Badge>}</div><Badge>{item.scope === 'user' ? t("用户安装") : item.scope === 'machine' ? t("系统安装") : t("范围未知")}</Badge></div>
        <code className="path-block">{item.path}</code><p className="muted">{item.detail}</p>
        {item.manualCommand && <div className="manual-command"><span>{t("此安装需要在对应权限的终端中操作")}</span><code>{item.manualCommand}</code></div>}
        <div className="installation-buttons"><button className="button secondary small-button" disabled={disabled || !item.capabilities.upgrade} onClick={() => onPlan({ kind: !item.version && item.capabilities.installVersion ? 'reinstall' : 'update', installationId: item.id })}>{!item.version && item.capabilities.installVersion ? t("预览重新安装") : t("检查升级计划")}</button><button className="button danger-ghost small-button" disabled={disabled || !item.capabilities.uninstall || (consolidating && item.id === selectedKeepId)} onClick={() => onPlan({ kind: 'uninstall', installationId: item.id })}><Trash2 size={14}/>{t("预览卸载")}</button></div>
      </article>)}</div>}
    </Panel>

    <Panel title={t("终端命中关系")} action={<button className="button secondary small-button" disabled={disabled} onClick={() => onPlan({ kind: 'path-repair', ...(consolidating && selectedKeepId ? { installationId: selectedKeepId } : {}) })}><Wrench size={14}/>{consolidating ? t("预览保留项 PATH 修复") : t("预览 PATH 修复")}</button>}>
      <p className="panel-description first-description">{t("检测对应下列 Shell 的新进程。已打开的终端可能仍持有之前的环境变量。")}</p>
      {snapshot.shells.length ? <div className="shell-list">{snapshot.shells.map((shell) => <div className="shell-row" key={shell.name}><Terminal size={18}/><div><strong>{shell.name}</strong><code>{shell.path ?? t("未找到 claude 命令")}</code><p>{shell.detail}</p>{shell.candidates.length > 1 && <details><summary>{t("查看全部候选路径")}</summary>{shell.candidates.map((path) => <code key={path}>{path}</code>)}</details>}</div></div>)}</div> : <Empty title={t("暂无终端解析结果")}>{t("重新扫描环境后查看。")}</Empty>}
    </Panel>
  </div>;
}
