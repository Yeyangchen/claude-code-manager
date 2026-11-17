import { useEffect, useState } from 'react';
import { ExternalLink, Monitor, Moon, RotateCcw, ShieldCheck, Sun } from 'lucide-react';
import { Badge, Empty, Notice, Panel } from '../app/components';
import { errorMessage } from '../services/manager';
import type { ManagerApi } from '../services/manager';
import type { ChangeEntry, ManagerUpdate, PlanRequest, Settings as SettingsData, Snapshot } from '../shared/types';
import { displayDate } from '../shared/format';

interface Props {
  api: ManagerApi;
  snapshot: Snapshot;
  disabled: boolean;
  onPlan: (request: PlanRequest) => void;
  onSettingsSaved: (settings: SettingsData) => void;
  onError: (message: string) => void;
}

export function Settings({ api, snapshot, disabled, onPlan, onSettingsSaved, onError }: Props) {
  const [settings, setSettings] = useState(snapshot.settings);
  const [mode, setMode] = useState<'official' | 'manual' | 'hold'>(snapshot.policy.mode === 'official' || snapshot.policy.mode === 'hold' ? snapshot.policy.mode : 'manual');
  const [channel, setChannel] = useState<'stable' | 'latest'>(snapshot.policy.channel === 'stable' ? 'stable' : 'latest');
  const [keepMinimum, setKeepMinimum] = useState(false);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [changes, setChanges] = useState<ChangeEntry[]>([]);
  const [update, setUpdate] = useState<ManagerUpdate | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const activeVersion = snapshot.installations.find((item) => item.active)?.version ?? snapshot.installations[0]?.version;

  useEffect(() => {
    let active = true;
    api.getChanges().then((next) => { if (active) setChanges(next); }).catch((failure) => { if (active) onError(errorMessage(failure)); });
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
    } catch (failure) { onError(errorMessage(failure)); }
    finally { setSaving(false); }
  }

  return <div className="page-stack">
    <Panel title="Claude Code 更新策略" action={<ShieldCheck size={18} className="muted" />}>
      <div className="option-list">{[
        { value: 'official', title: '跟随官方更新', detail: '使用该安装来源提供的后台更新机制。' },
        { value: 'manual', title: '提醒后手动更新', detail: '由管理器检查版本，安装前预览操作计划。' },
        { value: 'hold', title: '保持当前版本', detail: '关闭后台升级，并提示外部工具造成的版本变化。' },
      ].map((option) => <label className={`option-card ${mode === option.value ? 'selected' : ''}`} key={option.value}><input type="radio" name="policy" value={option.value} checked={mode === option.value} onChange={() => setMode(option.value as typeof mode)} /><span><strong>{option.title}</strong><small>{option.detail}</small></span></label>)}</div>
      <div className="form-grid policy-fields"><label className="field"><span>发布通道</span><select value={channel} onChange={(event) => setChannel(event.target.value as typeof channel)}><option value="stable">Stable · 稳定通道</option><option value="latest">Latest · 最新通道</option></select></label><div className="field"><span>版本下限</span><label className="checkbox-label"><input type="checkbox" checked={keepMinimum} disabled={!activeVersion} onChange={(event) => setKeepMinimum(event.target.checked)} />保持当前版本下限{activeVersion ? ` v${activeVersion}` : ''}</label></div></div>
      <p className="panel-description">{snapshot.policy.detail}</p>
      {snapshot.policy.disabledUpdates && <Notice tone="warning">当前配置限制手动更新。请查看下方来源，再决定如何调整。</Notice>}
      {snapshot.policy.sources.length > 0 && <details className="details-box"><summary>查看生效配置来源</summary>{snapshot.policy.sources.map((source, index) => <div className="source-row" key={`${source.name}-${index}`}><span>{source.name}</span><code>{source.value}</code></div>)}</details>}
      <div className="panel-footer"><button className="button primary" disabled={disabled} onClick={() => onPlan({ kind: 'policy', policyMode: mode, channel, minimumVersion: keepMinimum && activeVersion ? activeVersion : undefined })}>预览策略更改</button></div>
    </Panel>

    <Panel title="管理器偏好">
      <label className="field"><span>外观</span></label>
      <div className="theme-options">{[{ value: 'system', label: '跟随系统', icon: Monitor }, { value: 'light', label: '浅色', icon: Sun }, { value: 'dark', label: '深色', icon: Moon }].map(({ value, label, icon: Icon }) => <button key={value} className={`theme-option ${settings.theme === value ? 'selected' : ''}`} aria-pressed={settings.theme === value} onClick={() => { setSettings({ ...settings, theme: value as SettingsData['theme'] }); setSaved(false); }}><Icon size={18} />{label}</button>)}</div>
      <div className="form-grid"><label className="field"><span>网络连接</span><select value={settings.proxyMode} onChange={(event) => { setSettings({ ...settings, proxyMode: event.target.value as SettingsData['proxyMode'] }); setSaved(false); }}><option value="system">使用系统代理</option><option value="direct">直接连接</option><option value="custom">自定义代理</option></select></label><label className="field"><span>后台检查间隔</span><select value={settings.checkIntervalHours} onChange={(event) => { setSettings({ ...settings, checkIntervalHours: Number(event.target.value) }); setSaved(false); }}><option value={0}>仅手动检查</option><option value={1}>每 1 小时</option><option value={6}>每 6 小时</option><option value={12}>每 12 小时</option><option value={24}>每 24 小时</option></select></label></div>
      {settings.proxyMode === 'custom' && <div className="proxy-fields"><label className="field"><span>代理地址</span><input placeholder="http://127.0.0.1:7890" value={settings.proxyUrl} onChange={(event) => { setSettings({ ...settings, proxyUrl: event.target.value }); setSaved(false); }} spellCheck={false} autoComplete="off" /></label><div className="form-grid"><label className="field"><span>代理用户名（可选）</span><input value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="off" /></label><label className="field"><span>代理密码（可选）</span><input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="new-password" placeholder={settings.hasProxyCredentials ? '已保存到 Windows 凭据管理器' : ''} /></label></div><p className="panel-description">认证信息由 Windows 凭据管理器保存。留空沿用已保存的凭据。</p></div>}
      <div className="panel-footer"><span className="muted small">检查任务在管理器运行期间执行。</span><div className="button-row">{saved && <span className="saved-label" role="status">已保存</span>}<button className="button primary" disabled={saving || disabled} onClick={() => void save()}>{saving ? '保存中…' : '保存偏好'}</button></div></div>
    </Panel>

    <Panel title="还原管理器所做的更改" action={<RotateCcw size={18} className="muted" />}>
      <p className="panel-description first-description">卸载管理器前，可在这里还原更新策略与 PATH。还原前会比较当前值，保留之后发生的外部改动。</p>
      {changes.length ? <div className="change-list">{changes.map((change) => <div className="change-entry" key={change.id}><div><strong>{change.description}</strong><p>{displayDate(change.createdAt)} · {change.detail}</p></div>{change.restored ? <Badge tone="success">已还原</Badge> : <button className="button secondary small-button" disabled={disabled || !change.canRestore} onClick={() => onPlan({ kind: 'restore-change', changeId: change.id })}>预览还原</button>}</div>)}</div> : <Empty title="暂无可还原更改">管理器写入的策略与 PATH 更改会显示在这里。</Empty>}
    </Panel>

    <Panel title="关于 Claude Code Manager" action={<Badge>v{snapshot.appVersion}</Badge>}>
      <p className="about-copy">本项目为非官方社区工具，与 Anthropic 无隶属或背书关系。Claude 和 Claude Code 是 Anthropic 的商标。</p>
      <div className="about-links"><button className="text-link" onClick={() => void api.openUrl('https://github.com/Yeyangchen/claude-code-manager').catch((failure) => onError(errorMessage(failure)))}>GitHub 仓库 <ExternalLink size={14} /></button><button className="text-link" onClick={() => void api.openUrl('https://github.com/Yeyangchen/claude-code-manager/issues').catch((failure) => onError(errorMessage(failure)))}>反馈问题 <ExternalLink size={14} /></button></div>
      <div className="data-location"><span>本地数据目录</span><code>{snapshot.dataDir}</code></div>
      <div className="panel-footer"><span className="muted small">{update?.message ?? '通过 GitHub Releases 获取管理器更新。'}</span><button className="button secondary small-button" disabled={checkingUpdate} onClick={async () => { setCheckingUpdate(true); try { setUpdate(await api.checkManagerUpdate()); } catch (failure) { onError(errorMessage(failure)); } finally { setCheckingUpdate(false); } }}>{checkingUpdate ? '检查中…' : '检查管理器更新'}</button></div>
      {update?.available && <div className="update-available"><span>可用版本 {update.latestVersion}</span><button className="button secondary small-button" onClick={() => void api.openUrl(update.downloadUrl).catch((failure) => onError(errorMessage(failure)))}>前往发布页 <ExternalLink size={14} /></button></div>}
    </Panel>
  </div>;
}
