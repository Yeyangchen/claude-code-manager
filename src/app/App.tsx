import { t } from '../shared/i18n';
import { version as appVersion } from '../../package.json';
import { useCallback, useEffect, useState } from 'react';
import { Activity, ArrowDownToLine, ArrowRight, Boxes, CircleHelp, Clock3, ExternalLink, LayoutDashboard, LoaderCircle, Monitor, RefreshCw, Settings2, Terminal, X } from 'lucide-react';
import { Badge, Loading, Modal, Notice } from './components';
import { useManager } from './useManager';
import { errorMessage, previewMode } from '../services/manager';
import type { ManagerApi } from '../services/manager';
import type { Operation, Plan, PlanRequest } from '../shared/types';
import { Overview } from '../features/Overview';
import { Versions } from '../features/Versions';
import { Environment } from '../features/Environment';
import { Settings } from '../features/Settings';
import { History, TaskBar, TaskDetails } from '../features/Tasks';
import { displayDate, sourceNames } from '../shared/format';
type Page = 'overview' | 'versions' | 'environment' | 'settings' | 'history';
const pages = {
    overview: { label: t("概览"), title: t("掌握你的 Claude Code"), subtitle: t("查看本机安装，选择合适的版本。"), icon: LayoutDashboard },
    versions: { label: t("版本"), title: t("版本管理"), subtitle: t("查看发布通道与更新说明，选择要安装的版本。"), icon: Boxes },
    environment: { label: t("环境"), title: t("本机环境"), subtitle: t("核对安装来源、前置条件与终端命中路径。"), icon: Monitor },
    settings: { label: t("设置"), title: t("设置"), subtitle: t("管理更新策略、网络与本地更改。"), icon: Settings2 },
    history: { label: t("操作记录"), title: t("操作记录"), subtitle: t("每次变更的过程与结果，都有记录。"), icon: Clock3 },
};
export default function App({ api: injectedApi }: {
    api?: ManagerApi;
}) {
    const manager = useManager(injectedApi);
    const { api, snapshot, catalog, operation, loading, checking, error, setError, refresh } = manager;
    const [page, setPage] = useState<Page>('overview');
    const [selectedId, setSelectedId] = useState('');
    const [plan, setPlan] = useState<Plan | null>(null);
    const [planning, setPlanning] = useState(false);
    const [executing, setExecuting] = useState(false);
    const [checksumAccepted, setChecksumAccepted] = useState(false);
    const [taskOpen, setTaskOpen] = useState(false);
    const [historyTask, setHistoryTask] = useState<Operation | null>(null);
    const [diagnostics, setDiagnostics] = useState<string | null>(null);
    const [exporting, setExporting] = useState(false);
    const [exportPath, setExportPath] = useState<string | null>(null);
    const [scanning, setScanning] = useState(false);
    const selected = snapshot?.installations.find((item) => item.id === selectedId) ?? snapshot?.installations.find((item) => item.active) ?? snapshot?.installations[0] ?? null;
    const disabled = Boolean(snapshot?.readOnly || !snapshot?.environment.supported || operation?.status === 'running' || planning || executing);
    useEffect(() => {
        const theme = snapshot?.settings.theme ?? 'system';
        const media = window.matchMedia('(prefers-color-scheme: dark)');
        const apply = () => { document.documentElement.dataset.theme = theme === 'system' ? (media.matches ? 'dark' : 'light') : theme; };
        apply();
        media.addEventListener('change', apply);
        return () => media.removeEventListener('change', apply);
    }, [snapshot?.settings.theme]);
    const onError = useCallback((message: string) => setError(message), [setError]);
    const closePlan = useCallback(() => { if (!executing)
        setPlan(null); }, [executing]);
    const closeTask = useCallback(() => { setTaskOpen(false); setHistoryTask(null); }, []);
    const closeDiagnostics = useCallback(() => { setDiagnostics(null); setExportPath(null); }, []);
    async function createPlan(request: PlanRequest) {
        if (!api || disabled)
            return;
        setPlanning(true);
        setError(null);
        setChecksumAccepted(false);
        try {
            setPlan(await api.createPlan(request));
        }
        catch (failure) {
            setError(errorMessage(failure));
        }
        finally {
            setPlanning(false);
        }
    }
    async function executePlan() {
        if (!api || !plan || disabled || !plan.canExecute || (plan.checksumOnly && !checksumAccepted))
            return;
        setExecuting(true);
        try {
            manager.setOperation(await api.executePlan(plan.id, checksumAccepted));
            setPlan(null);
            setHistoryTask(null);
            setTaskOpen(true);
        }
        catch (failure) {
            setError(errorMessage(failure));
        }
        finally {
            setExecuting(false);
        }
    }
    async function inspectDiagnostics() {
        if (!api)
            return;
        try {
            setExportPath(null);
            setDiagnostics(await api.previewDiagnostics());
        }
        catch (failure) {
            setError(errorMessage(failure));
        }
    }
    async function rescan() {
        setScanning(true);
        try {
            await refresh();
        }
        catch (failure) {
            setError(errorMessage(failure));
        }
        finally {
            setScanning(false);
        }
    }
    const activeTask = historyTask ?? operation;
    const plannedInstallation = snapshot?.installations.find((item) => item.id === plan?.installationId);
    const retryable = activeTask && ['failed', 'cancelled', 'attention'].includes(activeTask.status) && ['install', 'update', 'reinstall', 'uninstall', 'path-repair'].includes(activeTask.kind);
    return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><div className="brand-mark"><Terminal size={22} strokeWidth={1.8}/></div><div><strong>Claude Code</strong><span>MANAGER</span></div></div>
      <div className="workspace-label">{t("工作空间")}</div>
      <nav aria-label={t("主导航")}>{(['overview', 'versions', 'environment', 'history'] as Page[]).map((key) => { const Icon = pages[key].icon; return <button key={key} className={`nav-item ${page === key ? 'active' : ''}`} onClick={() => setPage(key)} aria-current={page === key ? 'page' : undefined}><Icon size={18}/><span>{pages[key].label}</span>{key === 'environment' && snapshot && snapshot.checks.some((item) => item.level === 'warning' || item.level === 'error') && <span className="nav-notice" aria-label={t("有需要关注的环境检查")}/>}</button>; })}</nav>
      <div className="sidebar-spacer"/>
      <div className="local-device"><span className="device-symbol"><Monitor size={18}/></span><div><strong>{snapshot?.environment.os ?? t("本机设备")}</strong><span>{snapshot ? `${snapshot.environment.arch} · ${snapshot.environment.userName}` : t("正在连接")}</span></div><span className="connected-dot"/></div>
      <button className={`nav-item ${page === 'settings' ? 'active' : ''}`} onClick={() => setPage('settings')} aria-current={page === 'settings' ? 'page' : undefined}><Settings2 size={18}/><span>{t("设置")}</span></button>
      <button className="nav-item" onClick={() => api && void api.openUrl('https://github.com/Yeyangchen/claude-code-manager/issues').catch((failure) => setError(errorMessage(failure)))}><CircleHelp size={18}/><span>{t("帮助与反馈")}</span><ExternalLink size={12}/></button>
      <div className="sidebar-footer"><span>{t("非官方社区工具")}</span><span>v{snapshot?.appVersion ?? appVersion}</span></div>
    </aside>

    <main className="main-content">
      <div className="topbar"><div className="breadcrumbs"><span>{t("工作空间")}</span><span>/</span><strong>{pages[page].label}</strong></div><div className="topbar-actions">{previewMode && <Badge tone="warning">{t("开发预览 · 模拟数据")}</Badge>}<span className="local-status"><span className="connected-dot"/>{t("本地运行")}</span></div></div>
      <div className="content-wrap">
        <header className="page-header"><div><h1>{pages[page].title}</h1><p>{pages[page].subtitle}</p></div><button className="button secondary" disabled={checking || scanning || !api} onClick={() => { void manager.checkReleases(true); void rescan(); }}><RefreshCw size={16} className={checking || scanning ? 'spin' : ''}/>{checking || scanning ? t("检查中…") : t("检查更新")}</button></header>
        {error && <div className="error-banner" role="alert"><Notice tone="error">{error}</Notice><button className="icon-button" aria-label={t("关闭错误提示")} onClick={() => setError(null)}><X size={17}/></button></div>}
        {snapshot?.readOnly && <Notice tone="warning">{t("应用数据来自更新版本，目前以只读方式打开。请更新管理器后再进行环境更改。")}</Notice>}
        {snapshot?.environment.elevated && <Notice tone="warning">{t("管理器正以提升权限运行。检测对应用户")}{snapshot.environment.userName}{t("，请核对是否为日常使用的账号。")}</Notice>}
        {loading ? <Loading /> : !snapshot || !api ? <div className="startup-error"><Activity size={30}/><h2>{t("尚未连接到本机服务")}</h2><p>{t("请确认正在使用桌面应用，然后重新加载。")}</p><button className="button secondary" onClick={() => window.location.reload()}>{t("重新加载")}</button></div> : <>
          {(page === 'overview' || page === 'versions') && <div className="selection-bar"><label><span>{t("管理对象")}</span><select aria-label={t("管理对象")} value={selected?.id ?? ''} onChange={(event) => setSelectedId(event.target.value)}>{snapshot.installations.length ? snapshot.installations.map((item) => <option key={item.id} value={item.id}>{sourceNames[item.source]} · {item.version ?? t("版本未知")}{item.active ? t(" · 当前命中") : ''}</option>) : <option value="">{t("新建 Native 安装")}</option>}</select></label><span className="muted small">{catalog ? t("检查于 {value0}{value1}", { value0: displayDate(catalog.checkedAt), value1: catalog.cached ? t(" · 缓存") : '' }) : t("发布信息待查询")}</span></div>}
          {page === 'overview' && <Overview snapshot={snapshot} selected={selected} catalog={catalog} disabled={disabled} onPlan={(request) => void createPlan(request)} onNavigate={setPage}/>}
          {page === 'versions' && <Versions api={api} selected={selected} catalog={catalog} disabled={disabled} onPlan={(request) => void createPlan(request)} onError={onError}/>}
          {page === 'environment' && <><div className="section-toolbar"><span className="muted">{t("检测上下文 ·")}{snapshot.environment.userName}</span><button className="button secondary small-button" disabled={scanning} onClick={() => void rescan()}><RefreshCw size={14} className={scanning ? 'spin' : ''}/>{scanning ? t("扫描中…") : t("重新扫描")}</button></div><Environment snapshot={snapshot} disabled={disabled} onPlan={(request) => void createPlan(request)} onDiagnostics={() => void inspectDiagnostics()}/></>}
          {page === 'settings' && <Settings api={api} snapshot={snapshot} disabled={disabled} onPlan={(request) => void createPlan(request)} onSettingsSaved={(settings) => manager.setSnapshot({ ...snapshot, settings })} onError={onError} onOperationStarted={(next) => { manager.setOperation(next); setHistoryTask(null); setTaskOpen(true); }}/>}
          {page === 'history' && <History history={snapshot.history} onOpen={(task) => { setHistoryTask(task); setTaskOpen(true); }}/>}
          <footer className="content-footer"><span>{t("非官方社区工具，与 Anthropic 无隶属或背书关系。")}</span><button className="text-link" onClick={() => void inspectDiagnostics()}>{t("诊断信息")}<ArrowUpRightIcon /></button></footer>
        </>}
      </div>
      {operation && <TaskBar operation={operation} onOpen={() => { setHistoryTask(null); setTaskOpen(true); }}/>}
    </main>

    {planning && <div className="planning-toast" role="status"><LoaderCircle size={17} className="spin"/>{t("正在核实目标并生成计划…")}</div>}
    {plan && <Modal title={plan.title} onClose={closePlan} wide footer={<><button className="button secondary" disabled={executing} onClick={closePlan}>{t("取消")}</button><button className={`button ${plan.kind === 'uninstall' ? 'danger' : 'primary'}`} disabled={disabled || !plan.canExecute || Boolean(plan.checksumOnly && !checksumAccepted)} onClick={() => void executePlan()}>{executing ? <LoaderCircle size={16} className="spin"/> : <ArrowDownToLine size={16}/>}{executing ? t("启动任务…") : plan.kind === 'uninstall' ? t("确认卸载此实例") : t("确认执行")}</button></>}>
      {error && <Notice tone="error">{error}</Notice>}
      {plannedInstallation && <div className="change-preview"><h3>{t("目标安装实例")} · {sourceNames[plannedInstallation.source]}</h3><code className="path-block">{plannedInstallation.path}</code><p className="muted">{plannedInstallation.detail}</p></div>}
      {(plan.currentVersion || plan.targetVersion) && <div className="version-transition"><span><small>{t("当前版本")}</small><strong>{plan.currentVersion ? `v${plan.currentVersion}` : plannedInstallation ? t("版本未知") : t("尚未安装")}</strong></span><ArrowRight size={21}/><span><small>{t("目标版本")}</small><strong>{plan.targetVersion ? `v${plan.targetVersion}` : t("卸载实例")}</strong></span></div>}
      {plan.warnings.map((warning, index) => <Notice key={index} tone="warning">{warning}</Notice>)}
      {plan.kind === 'uninstall' && snapshot?.installations.length === 1 && <Notice tone="warning">{t("这是检测到的最后一份安装。卸载后需要重新安装才能在终端运行 Claude Code。")}</Notice>}
      <h3 className="modal-section-title">{t("执行步骤")}</h3><ol className="plan-steps">{plan.steps.map((step, index) => <li key={index}>{step}</li>)}</ol>
      {plan.changes.map((change, index) => <div className="change-preview" key={index}><h3>{change.name}</h3><div><span>{t("更改前")}</span><pre>{change.before || t("未设置")}</pre></div><div><span>{t("更改后")}</span><pre>{change.after || t("移除该值")}</pre></div></div>)}
      {plan.manualCommand && <div className="manual-command"><span>{t("在对应权限的终端中执行后，重新扫描环境")}</span><code>{plan.manualCommand}</code></div>}
      {plan.checksumOnly && <label className="checksum-confirm"><input type="checkbox" checked={checksumAccepted} onChange={(event) => setChecksumAccepted(event.target.checked)}/><span>{t("此历史版本仅能校验摘要。我已了解校验能力，并选择继续安装。")}</span></label>}
      {!plan.canExecute && <Notice>{t("该计划当前无法直接执行，请查看限制说明并重新检查环境。")}</Notice>}
    </Modal>}
    {taskOpen && activeTask && <Modal title={activeTask.title} onClose={closeTask} wide>{error && <Notice tone="error">{error}</Notice>}<TaskDetails operation={activeTask} busy={executing || planning} onRetry={retryable && !disabled ? () => { closeTask(); void createPlan({ kind: activeTask.kind as PlanRequest['kind'], installationId: activeTask.installationId ?? undefined, targetVersion: activeTask.targetVersion ?? undefined }); } : undefined} onExportLogs={() => { closeTask(); void inspectDiagnostics(); }} onCancel={async () => { if (!api)
        return; try {
        manager.setOperation(await api.cancelOperation(activeTask.id));
    }
    catch (failure) {
        setError(errorMessage(failure));
    } }}/></Modal>}
    {diagnostics !== null && <Modal title={t("诊断信息预览")} onClose={closeDiagnostics} wide footer={<><button className="button secondary" onClick={closeDiagnostics}>{t("关闭")}</button><button className="button primary" disabled={exporting} onClick={async () => { if (!api)
        return; setExporting(true); try {
        const result = await api.exportDiagnostics();
        if (result)
            setExportPath(result.path);
    }
    catch (failure) {
        setError(errorMessage(failure));
    }
    finally {
        setExporting(false);
    } }}>{exporting ? t("导出中…") : t("导出到本地文件")}</button></>}><p className="panel-description first-description">{t("请检查脱敏后的内容，再保存到你选择的位置。")}</p>{error && <Notice tone="error">{error}</Notice>}<pre className="log-output diagnostics-output" tabIndex={0}>{diagnostics}</pre>{exportPath && <Notice tone="success">{t("已导出到 {path}", { path: exportPath })}</Notice>}</Modal>}
  </div>;
}
function ArrowUpRightIcon() { return <ExternalLink size={12}/>; }
