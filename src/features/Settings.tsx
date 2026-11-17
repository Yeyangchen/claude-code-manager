import { t } from '../shared/i18n';
import { useCallback, useEffect, useState } from 'react';
import { ExternalLink, Monitor, Moon, RotateCcw, ShieldCheck, Sun } from 'lucide-react';
import { Badge, Empty, Modal, Notice, Panel } from '../app/components';
import { errorMessage } from '../services/manager';
import type { ManagerApi } from '../services/manager';
import type { ChangeEntry, ManagerUpdate, Operation, PlanRequest, Settings as SettingsData, Snapshot } from '../shared/types';
import { displayDate } from '../shared/format';
interface Props {
    api: ManagerApi;
    snapshot: Snapshot;
    disabled: boolean;
    onPlan: (request: PlanRequest) => void;
    onSettingsSaved: (settings: SettingsData) => void;
    onError: (message: string) => void;
    onOperationStarted: (operation: Operation) => void;
}
export function Settings({ api, snapshot, disabled, onPlan, onSettingsSaved, onError, onOperationStarted }: Props) {
    const [settings, setSettings] = useState(snapshot.settings);
    const [mode, setMode] = useState<'official' | 'manual' | 'hold'>(snapshot.policy.mode === 'official' || snapshot.policy.mode === 'hold' ? snapshot.policy.mode : 'manual');
    const [channel, setChannel] = useState<'stable' | 'latest'>(snapshot.policy.channel === 'stable' ? 'stable' : 'latest');
    const [keepMinimum, setKeepMinimum] = useState(Boolean(snapshot.policy.minimumVersion));
    const [username, setUsername] = useState('');
    const [password, setPassword] = useState('');
    const [saving, setSaving] = useState(false);
    const [saved, setSaved] = useState(false);
    const [changes, setChanges] = useState<ChangeEntry[]>([]);
    const [update, setUpdate] = useState<ManagerUpdate | null>(null);
    const [checkingUpdate, setCheckingUpdate] = useState(false);
    const [updateTarget, setUpdateTarget] = useState<string | null>(null);
    const [installingUpdate, setInstallingUpdate] = useState(false);
    const [updateError, setUpdateError] = useState<string | null>(null);
    const closeUpdate = useCallback(() => { if (!installingUpdate)
        setUpdateTarget(null); }, [installingUpdate]);
    const activeVersion = snapshot.installations.find((item) => item.active)?.version ?? snapshot.installations[0]?.version;
    const minimumVersion = snapshot.policy.minimumVersion ?? activeVersion;
    useEffect(() => {
        let active = true;
        api.getChanges().then((next) => { if (active)
            setChanges(next); }).catch((failure) => { if (active)
            onError(errorMessage(failure)); });
        return () => { active = false; };
    }, [api, snapshot.history, onError]);
    async function save() {
        setSaving(true);
        setSaved(false);
        try {
            const next = await api.saveSettings(settings, username || undefined, password || undefined);
            setSettings(next);
            setPassword('');
            setUsername('');
            onSettingsSaved(next);
            setSaved(true);
        }
        catch (failure) {
            onError(errorMessage(failure));
        }
        finally {
            setSaving(false);
        }
    }
    return <div className="page-stack">
    <Panel title={t("Claude Code 更新策略")} action={<ShieldCheck size={18} className="muted"/>}>
      <div className="option-list">{[
            { value: 'official', title: t("跟随官方更新"), detail: t("使用该安装来源提供的后台更新机制。") },
            { value: 'manual', title: t("提醒后手动更新"), detail: t("由管理器检查版本，安装前预览操作计划。") },
            { value: 'hold', title: t("保持当前版本"), detail: t("关闭后台升级，并提示外部工具造成的版本变化。") },
        ].map((option) => <label className={`option-card ${mode === option.value ? 'selected' : ''}`} key={option.value}><input type="radio" name="policy" value={option.value} checked={mode === option.value} onChange={() => setMode(option.value as typeof mode)}/><span><strong>{option.title}</strong><small>{option.detail}</small></span></label>)}</div>
      <div className="form-grid policy-fields"><label className="field"><span>{t("发布通道")}</span><select value={channel} onChange={(event) => setChannel(event.target.value as typeof channel)}><option value="stable">{t("Stable · 稳定通道")}</option><option value="latest">{t("Latest · 最新通道")}</option></select></label><div className="field"><span>{t("版本下限")}</span><label className="checkbox-label"><input type="checkbox" checked={keepMinimum} disabled={!minimumVersion} onChange={(event) => setKeepMinimum(event.target.checked)}/>{t("保留版本下限")}{minimumVersion ? ` v${minimumVersion}` : ''}</label></div></div>
      <p className="panel-description">{snapshot.policy.detail}</p>
      {snapshot.policy.disabledUpdates && <Notice tone="warning">{t("当前配置限制手动更新。请查看下方来源，再决定如何调整。")}</Notice>}
      {snapshot.policy.sources.length > 0 && <details className="details-box"><summary>{t("查看生效配置来源")}</summary>{snapshot.policy.sources.map((source, index) => <div className="source-row" key={`${source.name}-${index}`}><span>{source.name}</span><code>{source.value}</code></div>)}</details>}
      <div className="panel-footer"><button className="button primary" disabled={disabled} onClick={() => onPlan({ kind: 'policy', policyMode: mode, channel, minimumVersion: keepMinimum && minimumVersion ? minimumVersion : undefined })}>{t("预览策略更改")}</button></div>
    </Panel>

    <Panel title={t("管理器偏好")}>
      <label className="field"><span>{t("外观")}</span></label>
      <div className="theme-options">{[{ value: 'system', label: t("跟随系统"), icon: Monitor }, { value: 'light', label: t("浅色"), icon: Sun }, { value: 'dark', label: t("深色"), icon: Moon }].map(({ value, label, icon: Icon }) => <button key={value} className={`theme-option ${settings.theme === value ? 'selected' : ''}`} aria-pressed={settings.theme === value} onClick={() => { setSettings({ ...settings, theme: value as SettingsData['theme'] }); setSaved(false); }}><Icon size={18}/>{label}</button>)}</div>
      <div className="form-grid"><label className="field"><span>{t("网络连接")}</span><select value={settings.proxyMode} onChange={(event) => { setSettings({ ...settings, proxyMode: event.target.value as SettingsData['proxyMode'] }); setSaved(false); }}><option value="system">{t("使用系统代理")}</option><option value="direct">{t("直接连接")}</option><option value="custom">{t("自定义代理")}</option></select></label><label className="field"><span>{t("后台检查间隔")}</span><select value={settings.checkIntervalHours} onChange={(event) => { setSettings({ ...settings, checkIntervalHours: Number(event.target.value) }); setSaved(false); }}><option value={0}>{t("仅手动检查")}</option><option value={1}>{t("每 1 小时")}</option><option value={6}>{t("每 6 小时")}</option><option value={12}>{t("每 12 小时")}</option><option value={24}>{t("每 24 小时")}</option></select></label></div>
      {settings.proxyMode === 'custom' && <div className="proxy-fields"><label className="field"><span>{t("代理地址")}</span><input placeholder="http://127.0.0.1:7890" value={settings.proxyUrl} onChange={(event) => { setSettings({ ...settings, proxyUrl: event.target.value }); setSaved(false); }} spellCheck={false} autoComplete="off"/></label><div className="form-grid"><label className="field"><span>{t("代理用户名（可选）")}</span><input value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="off"/></label><label className="field"><span>{t("代理密码（可选）")}</span><input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="new-password" placeholder={settings.hasProxyCredentials ? t("已保存到 Windows 凭据管理器") : ''}/></label></div><p className="panel-description">{t("认证信息由 Windows 凭据管理器保存。留空沿用已保存的凭据。")}</p></div>}
      <div className="panel-footer"><span className="muted small">{t("检查任务在管理器运行期间执行。")}</span><div className="button-row">{saved && <span className="saved-label" role="status">{t("已保存")}</span>}<button className="button primary" disabled={saving || disabled} onClick={() => void save()}>{saving ? t("保存中…") : t("保存偏好")}</button></div></div>
    </Panel>

    <Panel title={t("还原管理器所做的更改")} action={<RotateCcw size={18} className="muted"/>}>
      <p className="panel-description first-description">{t("卸载管理器前，可在这里还原更新策略与 PATH。还原前会比较当前值，保留之后发生的外部改动。")}</p>
      <Notice>{t("卸载管理器不会自动还原 PATH 或更新策略。请先逐项还原；卸载时若选择删除应用数据，变更台账也会被删除。")}</Notice>
      {changes.length ? <div className="change-list">{changes.map((change) => <div className="change-entry" key={change.id}>
        <div className="change-entry-main"><div><strong>{change.description}</strong><p>{displayDate(change.createdAt)} · {change.detail}</p></div>{change.restored ? <Badge tone="success">{t("已还原")}</Badge> : <button className="button secondary small-button" disabled={disabled || !change.canRestore} onClick={() => onPlan({ kind: 'restore-change', changeId: change.id })}>{t("预览还原")}</button>}</div>
        {!!change.comparison?.length && <details className="details-box change-comparison"><summary>{t("查看三方差异")}</summary>{change.comparison.map((comparison, index) => <section key={`${comparison.name}-${index}`} className="comparison-item"><h3>{comparison.name}</h3><dl className="comparison-values"><div><dt>{t("原值")}</dt><dd><pre>{comparison.before || t("未设置")}</pre></dd></div><div><dt>{t("管理器写入值")}</dt><dd><pre>{comparison.written || t("未设置")}</pre></dd></div><div><dt>{t("当前值")}</dt><dd><pre>{comparison.current || t("未设置")}</pre></dd></div></dl></section>)}</details>}
      </div>)}</div> : <Empty title={t("暂无可还原更改")}>{t("管理器写入的策略与 PATH 更改会显示在这里。")}</Empty>}
    </Panel>

    <Panel title={t("关于 Claude Code Manager")} action={<Badge>v{snapshot.appVersion}</Badge>}>
      <p className="about-copy">{t("本项目为非官方社区工具，与 Anthropic 无隶属或背书关系。Claude 和 Claude Code 是 Anthropic 的商标。")}</p>
      <div className="about-links"><button className="text-link" onClick={() => void api.openUrl('https://github.com/Yeyangchen/claude-code-manager').catch((failure) => onError(errorMessage(failure)))}>{t("GitHub 仓库")}<ExternalLink size={14}/></button><button className="text-link" onClick={() => void api.openUrl('https://github.com/Yeyangchen/claude-code-manager/issues').catch((failure) => onError(errorMessage(failure)))}>{t("反馈问题")}<ExternalLink size={14}/></button></div>
      <div className="data-location"><span>{t("本地数据目录")}</span><code>{snapshot.dataDir}</code></div>
      <div className="panel-footer"><span className="muted small">{update?.message ?? t("通过 GitHub Releases 获取管理器更新。")}</span><button className="button secondary small-button" disabled={checkingUpdate} onClick={async () => { setCheckingUpdate(true); try {
        setUpdate(await api.checkManagerUpdate());
    }
    catch (failure) {
        onError(errorMessage(failure));
    }
    finally {
        setCheckingUpdate(false);
    } }}>{checkingUpdate ? t("检查中…") : t("检查管理器更新")}</button></div>
      {update?.available && <div className="update-available"><span>{t("可用版本")}{update.latestVersion}</span><div className="button-row"><button className="button secondary small-button" onClick={() => void api.openUrl(update.downloadUrl).catch((failure) => onError(errorMessage(failure)))}>{t("前往发布页")}<ExternalLink size={14}/></button>{update.signedUpdaterReady && update.latestVersion && <button className="button primary small-button" disabled={disabled} onClick={() => { setUpdateError(null); setUpdateTarget(update.latestVersion); }}>{t("更新管理器")}</button>}</div></div>}
    </Panel>
    {updateTarget && <Modal title={t("确认更新管理器")} onClose={closeUpdate} footer={<><button className="button secondary" disabled={installingUpdate} onClick={closeUpdate}>{t("取消")}</button><button className="button primary" disabled={installingUpdate || disabled} onClick={async () => {
                    setInstallingUpdate(true);
                    setUpdateError(null);
                    try {
                        const operation = await api.installManagerUpdate(updateTarget);
                        setUpdateTarget(null);
                        onOperationStarted(operation);
                    }
                    catch (failure) {
                        setUpdateError(errorMessage(failure));
                    }
                    finally {
                        setInstallingUpdate(false);
                    }
                }}>{installingUpdate ? t("启动更新…") : t("确认更新并重启")}</button></>}>
      <p>{t("Claude Code Manager 将从 v")}{snapshot.appVersion}{t("更新到 v")}{updateTarget}。</p>
      <Notice>{t("管理器将下载并验证发布签名，随后关闭当前应用并启动安装。请先完成正在查看的工作。")}</Notice>
      {updateError && <Notice tone="error">{updateError}</Notice>}
    </Modal>}
  </div>;
}
