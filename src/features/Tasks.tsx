import { t } from '../shared/i18n';
import { useEffect, useState } from 'react';
import { ArrowUpRight, Check, Clock3, LoaderCircle } from 'lucide-react';
import { Badge, Empty, Notice, Panel } from '../app/components';
import { displayDate, formatBytes, phaseNames, statusNames } from '../shared/format';
import type { Operation } from '../shared/types';
export function TaskDetails({ operation, onCancel, busy, onRetry, onExportLogs }: {
    operation: Operation;
    onCancel: () => void;
    busy: boolean;
    onRetry?: () => void;
    onExportLogs: () => void;
}) {
    const percent = operation.progress?.total ? Math.min(100, (operation.progress.downloaded / operation.progress.total) * 100) : null;
    const [now, setNow] = useState(Date.now);
    useEffect(() => {
        if (operation.status !== 'running')
            return;
        const timer = window.setInterval(() => setNow(Date.now()), 1000);
        return () => window.clearInterval(timer);
    }, [operation.status]);
    const seconds = Math.max(0, Math.floor(((operation.finishedAt ? Date.parse(operation.finishedAt) : now) - Date.parse(operation.startedAt)) / 1000));
    const elapsed = Number.isFinite(seconds) ? t("{value0} 分 {value1} 秒", { value0: Math.floor(seconds / 60), value1: seconds % 60 }) : t("待核查");
    return <div className="task-details">
    <div className="task-status"><Badge tone={operation.status === 'succeeded' ? 'success' : operation.status === 'failed' ? 'error' : 'accent'}>{statusNames[operation.status]}</Badge><span>{phaseNames[operation.phase] ?? operation.phase}</span></div>
    <dl className="detail-grid"><div><dt>{t("开始时间")}</dt><dd>{displayDate(operation.startedAt)}</dd></div><div><dt>{t("目标版本")}</dt><dd>{operation.targetVersion ? `v${operation.targetVersion}` : t("环境更改")}</dd></div><div><dt>{t("任务编号")}</dt><dd className="mono break-all">{operation.id}</dd></div><div><dt>{t("结束时间")}</dt><dd>{operation.finishedAt ? displayDate(operation.finishedAt) : t("执行中")}</dd></div></dl>
    {operation.progress && <div className="progress-section"><div className="progress-caption"><span>{formatBytes(operation.progress.downloaded)}{operation.progress.total ? ` / ${formatBytes(operation.progress.total)}` : ''}</span>{percent !== null && <strong>{percent.toFixed(0)}%</strong>}</div><progress max={100} value={percent ?? undefined} aria-label={t("下载进度")}/></div>}
    {operation.result && <Notice tone={operation.status === 'succeeded' ? 'success' : 'warning'}>{operation.result}</Notice>}
    <div className="log-heading"><h3>{t("操作输出")}</h3><span className="muted small">{t("耗时")}{elapsed}{t("· 已脱敏")}</span></div>
    <pre className="log-output" tabIndex={0}>{operation.messages.length ? operation.messages.join('\n') : t("等待后台输出…")}</pre>
    {operation.status === 'running' && <div className="panel-footer"><span className="muted small">{operation.cancellable ? t("可以取消当前等待或准备阶段。") : t("安装器正在执行，请等待结果核查。")}</span><button className="button secondary" disabled={!operation.cancellable || busy} onClick={onCancel}>{t("取消任务")}</button></div>}
    {operation.status !== 'running' && <div className="panel-footer"><button className="text-link" onClick={onExportLogs}>{t("预览并导出诊断日志")}</button>{onRetry && <button className="button secondary" disabled={busy} onClick={onRetry}>{t("重新检查并预览")}</button>}</div>}
  </div>;
}
export function TaskBar({ operation, onOpen }: {
    operation: Operation;
    onOpen: () => void;
}) {
    return <button className={`task-bar ${operation.status}`} onClick={onOpen}><span className="task-bar-icon">{operation.status === 'running' ? <LoaderCircle size={19} className="spin"/> : operation.status === 'succeeded' ? <Check size={19}/> : <Clock3 size={19}/>}</span><span><strong>{operation.title}</strong><small>{operation.status === 'running' ? phaseNames[operation.phase] ?? operation.phase : statusNames[operation.status]}</small></span><span className="task-bar-action">{t("查看详情")}<ArrowUpRight size={16}/></span></button>;
}
export function History({ history, onOpen }: {
    history: Operation[];
    onOpen: (operation: Operation) => void;
}) {
    return <Panel title={t("操作记录")}><p className="panel-description first-description">{t("安装、更新与环境更改的结果均保存在本地。")}</p>{history.length ? <div className="history-list">{history.map((operation) => <button className="history-row" key={operation.id} onClick={() => onOpen(operation)}><Clock3 size={19}/><span><strong>{operation.title}</strong><small>{displayDate(operation.startedAt)}{operation.targetVersion ? ` · v${operation.targetVersion}` : ''}</small></span><Badge tone={operation.status === 'succeeded' ? 'success' : operation.status === 'failed' ? 'error' : 'neutral'}>{statusNames[operation.status]}</Badge><ArrowUpRight size={16}/></button>)}</div> : <Empty title={t("还没有操作记录")}>{t("完成第一次操作后，可在这里查看结果和日志。")}</Empty>}</Panel>;
}
