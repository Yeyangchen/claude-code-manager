import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import App from './App';
import { createApi, plan, runningOperation } from '../test/fixtures';
import { previewSnapshot } from '../services/preview';
import type { Operation } from '../shared/types';

describe('desktop management workflows', () => {
  it('previews a precise update and waits for explicit execution', async () => {
    const api = createApi();
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '查看更新计划' }));
    const dialog = await screen.findByRole('dialog', { name: '更新 Claude Code' });
    expect(api.createPlan).toHaveBeenCalledWith({ kind: 'update', installationId: 'preview-native', targetVersion: '2.1.320' });
    expect(api.executePlan).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: '确认执行' }));
    await waitFor(() => expect(api.executePlan).toHaveBeenCalledWith('plan-123', false));
    expect(await screen.findByText('Preparing')).toBeInTheDocument();
  });

  it('requires acknowledgment for checksum-only historical releases', async () => {
    const api = createApi();
    vi.mocked(api.createPlan).mockResolvedValue({ ...plan, checksumOnly: true });
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '查看更新计划' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('button', { name: '确认执行' })).toBeDisabled();
    await user.click(within(dialog).getByRole('checkbox'));
    await user.click(within(dialog).getByRole('button', { name: '确认执行' }));
    expect(api.executePlan).toHaveBeenCalledWith('plan-123', true);
  });

  it('blocks mutations when persisted data uses a newer schema', async () => {
    const api = createApi({ ...structuredClone(previewSnapshot), readOnly: true });
    render(<App api={api} />);
    expect(await screen.findByRole('button', { name: '查看更新计划' })).toBeDisabled();
    expect(screen.getByText(/目前以只读方式打开/)).toBeInTheDocument();
    expect(api.createPlan).not.toHaveBeenCalled();
  });

  it('binds uninstall to the selected installation instead of the active one', async () => {
    const api = createApi();
    vi.mocked(api.createPlan).mockResolvedValue({ ...plan, kind: 'uninstall', title: '卸载 npm 实例', installationId: 'preview-npm' });
    const user = userEvent.setup();
    render(<App api={api} />);
    await screen.findByRole('button', { name: '查看更新计划' });
    await user.click(screen.getByRole('button', { name: /^环境/ }));
    await user.click(screen.getAllByRole('button', { name: '预览卸载' })[1]);
    expect(api.createPlan).toHaveBeenCalledWith({ kind: 'uninstall', installationId: 'preview-npm' });
    expect(api.executePlan).not.toHaveBeenCalled();
    expect(await screen.findByRole('dialog', { name: '卸载 npm 实例' })).toBeInTheDocument();
  });

  it('allows a historical candidate to be verified before selecting it', async () => {
    const api = createApi();
    const user = userEvent.setup();
    render(<App api={api} />);
    await screen.findByRole('button', { name: '查看更新计划' });
    await user.click(screen.getByRole('button', { name: '版本' }));
    await user.click(screen.getByRole('button', { name: '验证并选择' }));
    expect(api.createPlan).toHaveBeenCalledWith({ kind: 'reinstall', installationId: 'preview-native', targetVersion: '2.1.100' });
  });

  it('reattaches to an existing background task and refreshes after completion', async () => {
    const snapshot = { ...structuredClone(previewSnapshot), activeOperation: runningOperation };
    const api = createApi(snapshot);
    let publish: ((operation: Operation) => void) | undefined;
    vi.mocked(api.onOperation).mockImplementation(async (callback) => { publish = callback; return () => {}; });
    const user = userEvent.setup();
    render(<App api={api} />);
    const taskButton = await screen.findByRole('button', { name: /更新 Claude Code.*执行前核查.*查看详情/ });
    await user.click(taskButton);
    expect(await screen.findByText('Preparing')).toBeInTheDocument();
    const finished: Operation = { ...runningOperation, phase: 'completed', status: 'succeeded', result: '版本已验证' };
    vi.mocked(api.getSnapshot).mockResolvedValue({ ...snapshot, activeOperation: null, history: [finished] });
    await act(async () => { publish?.(finished); });
    expect(await screen.findByText('版本已验证')).toBeInTheDocument();
    expect(api.getSnapshot).toHaveBeenCalledTimes(2);
  });

  it('previews redacted diagnostics before exporting', async () => {
    const api = createApi();
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '诊断信息' }));
    const dialog = await screen.findByRole('dialog', { name: '诊断信息预览' });
    expect(within(dialog).getByText('{"user":"<redacted>"}')).toBeInTheDocument();
    expect(api.exportDiagnostics).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: '导出到本地文件' }));
    expect(await screen.findByText('已导出到 C:\\diagnostics.json')).toBeInTheDocument();
  });

  it('confirms the exact signed manager release before starting its task', async () => {
    const api = createApi();
    vi.mocked(api.checkManagerUpdate).mockResolvedValue({ currentVersion: '0.1.0-beta.1', latestVersion: '0.1.0', available: true, downloadUrl: 'https://github.com/Yeyangchen/claude-code-manager/releases', signedUpdaterReady: true, message: '签名更新可用' });
    const user = userEvent.setup();
    render(<App api={api} />);
    await screen.findByRole('button', { name: '查看更新计划' });
    await user.click(screen.getByRole('button', { name: '设置' }));
    await user.click(screen.getByRole('button', { name: '检查管理器更新' }));
    await user.click(await screen.findByRole('button', { name: '更新管理器' }));
    expect(api.installManagerUpdate).not.toHaveBeenCalled();
    const dialog = await screen.findByRole('dialog', { name: '确认更新管理器' });
    await user.click(within(dialog).getByRole('button', { name: '确认更新并重启' }));
    expect(api.installManagerUpdate).toHaveBeenCalledWith('0.1.0');
    expect(await screen.findByRole('dialog', { name: '更新管理器' })).toBeInTheDocument();
  });

  it('offers one-click setup when no terminal-usable installation exists', async () => {
    const snapshot = structuredClone(previewSnapshot);
    snapshot.installations = [{ ...snapshot.installations[1], id: 'ide-cli', source: 'external', capabilities: { upgrade: false, installVersion: false, uninstall: false } }];
    snapshot.checks.push({ id: 'claude-desktop', title: 'Claude 桌面应用', level: 'info', message: '检测到 Claude 桌面应用内置的 Claude Code 2.1.325。' });
    const api = createApi(snapshot);
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '一键自动管理' }));
    expect(screen.getByText(/检测到 Claude 桌面应用内置的 Claude Code 2.1.325/)).toBeInTheDocument();
    expect(api.createPlan).toHaveBeenCalledWith({ kind: 'setup', targetVersion: '2.1.320' });
    expect(api.executePlan).not.toHaveBeenCalled();
  });

  it('offers a PATH-only setup when the native install is not reachable from terminals', async () => {
    const snapshot = structuredClone(previewSnapshot);
    snapshot.installations = snapshot.installations.map((item) => ({ ...item, active: false }));
    const api = createApi(snapshot);
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '一键修复' }));
    expect(api.createPlan).toHaveBeenCalledWith({ kind: 'setup' });
    expect(screen.queryByRole('button', { name: '一键自动管理' })).not.toBeInTheDocument();
  });

  it('offers reinstall for an existing Native executable with an unreadable version', async () => {
    const snapshot = structuredClone(previewSnapshot);
    snapshot.installations[0].version = null;
    const api = createApi(snapshot);
    const user = userEvent.setup();
    render(<App api={api} />);
    const repair = await screen.findByRole('button', { name: '预览重新安装' });
    expect(screen.queryByText('尚未安装')).not.toBeInTheDocument();
    expect(screen.getByText(/已发现安装文件，但无法读取版本/)).toBeInTheDocument();
    await user.click(repair);
    expect(api.createPlan).toHaveBeenCalledWith({ kind: 'reinstall', installationId: 'preview-native', targetVersion: '2.1.320' });
  });

  it('shows machine-scope manual instructions even when direct mutation is unavailable', async () => {
    const snapshot = structuredClone(previewSnapshot);
    const manualCommand = 'npm install -g @anthropic-ai/claude-code@latest --prefix "C:\\Program Files\\npm"';
    snapshot.installations = [{ ...snapshot.installations[1], scope: 'machine', capabilities: { upgrade: false, uninstall: false, installVersion: false }, manualCommand, detail: '此实例需要管理员终端。' }];
    const api = createApi(snapshot);
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: /^环境/ }));
    expect(await screen.findByText(manualCommand)).toBeVisible();
    expect(screen.getByText('此实例需要管理员终端。')).toBeVisible();
    expect(screen.getByRole('button', { name: '检查升级计划' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '预览卸载' })).toBeDisabled();
    expect(api.createPlan).not.toHaveBeenCalled();
  });

  it('shows exact target and blocks a non-executable preview', async () => {
    const api = createApi();
    vi.mocked(api.createPlan).mockResolvedValue({ ...plan, canExecute: false, manualCommand: 'manual repair instruction', warnings: ['来源限制，无法直接执行。'] });
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '查看更新计划' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByText(previewSnapshot.installations[0].path)).toBeVisible();
    expect(within(dialog).getByText('manual repair instruction')).toBeVisible();
    expect(within(dialog).getByRole('button', { name: '确认执行' })).toBeDisabled();
    expect(api.executePlan).not.toHaveBeenCalled();
  });

  it('keeps a CAS conflict visible in the confirmation without claiming success', async () => {
    const api = createApi();
    vi.mocked(api.executePlan).mockRejectedValue({ code: 'state_conflict', message: '当前值已被外部工具更改，请重新生成计划。' });
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '查看更新计划' }));
    const dialog = await screen.findByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: '确认执行' }));
    expect(await within(dialog).findByText('当前值已被外部工具更改，请重新生成计划。')).toBeVisible();
    expect(within(dialog).getByRole('button', { name: '取消' })).toBeEnabled();
    expect(screen.queryByText('Preparing')).not.toBeInTheDocument();
  });

  it('surfaces hold drift even beyond the compact overview check limit', async () => {
    const snapshot = structuredClone(previewSnapshot);
    snapshot.policy.mode = 'hold';
    snapshot.policy.detail = '保持策略已启用。';
    snapshot.checks = [0, 1, 2].map((index) => ({ id: `other-${index}`, title: `其他检查 ${index}`, level: 'warning', message: '其他提示' }));
    snapshot.checks.push({ id: 'hold-version-drift', title: '保持版本发生变化', level: 'warning', message: '原保持版本 2.1.300，当前版本 2.1.311；请核查外部更新。' });
    const api = createApi(snapshot);
    render(<App api={api} />);
    expect(await screen.findByText('原保持版本 2.1.300，当前版本 2.1.311；请核查外部更新。')).toBeVisible();
    expect(api.executePlan).not.toHaveBeenCalled();
  });

  it('preserves an existing policy minimum and explains uninstall ledger retention', async () => {
    const snapshot = structuredClone(previewSnapshot);
    snapshot.policy.minimumVersion = '2.1.100';
    const api = createApi(snapshot);
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '设置' }));
    expect(await screen.findByRole('checkbox', { name: '保留版本下限 v2.1.100' })).toBeChecked();
    expect(screen.getByText(/卸载管理器不会自动还原 PATH/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: '预览策略更改' }));
    expect(api.createPlan).toHaveBeenCalledWith({ kind: 'policy', policyMode: 'manual', channel: 'stable', minimumVersion: '2.1.100' });
  });

  it('protects the consolidation keep target and binds PATH preview to it', async () => {
    const api = createApi();
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: /^环境/ }));
    await user.click(await screen.findByRole('button', { name: '整理多份安装' }));
    await user.selectOptions(screen.getByRole('combobox', { name: '保留项' }), 'preview-npm');
    expect(screen.getAllByRole('button', { name: '预览卸载' })[1]).toBeDisabled();
    await user.click(screen.getByRole('button', { name: '预览保留项 PATH 修复' }));
    expect(api.createPlan).toHaveBeenCalledWith({ kind: 'path-repair', installationId: 'preview-npm' });
  });

  it('does not replace a completed event with a late running execution response', async () => {
    const api = createApi();
    let publish: ((operation: Operation) => void) | undefined;
    let returnExecution: ((operation: Operation) => void) | undefined;
    vi.mocked(api.onOperation).mockImplementation(async (callback) => { publish = callback; return () => {}; });
    vi.mocked(api.executePlan).mockImplementation(() => new Promise((resolve) => { returnExecution = resolve; }));
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '查看更新计划' }));
    const dialog = await screen.findByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: '确认执行' }));
    const finished: Operation = { ...runningOperation, status: 'succeeded', phase: 'completed', result: '已经验证成功' };
    vi.mocked(api.getSnapshot).mockResolvedValue({ ...structuredClone(previewSnapshot), history: [finished] });
    await act(async () => { publish?.(finished); });
    await act(async () => { returnExecution?.(runningOperation); });
    expect(await screen.findByText('已经验证成功')).toBeVisible();
  });

  it('keeps the three-way comparison readable when a conflict blocks restoration', async () => {
    const api = createApi();
    vi.mocked(api.getChanges).mockResolvedValue([{
      id: 'path-conflict', kind: 'path', description: '用户 PATH 更改', createdAt: '2026-11-16T12:00:00Z',
      restored: false, canRestore: false, detail: '当前值存在外部修改，请查看差异。',
      comparison: [{ name: 'HKCU Environment Path', before: 'C:\\original', written: 'C:\\original;C:\\manager', current: 'C:\\original;C:\\external' }],
    }]);
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: '设置' }));
    expect(await screen.findByRole('button', { name: '预览还原' })).toBeDisabled();
    await user.click(screen.getByText('查看三方差异'));
    expect(screen.getByText('原值')).toBeVisible();
    expect(screen.getByText('管理器写入值')).toBeVisible();
    expect(screen.getByText('当前值')).toBeVisible();
    expect(screen.getByText('C:\\original')).toBeVisible();
    expect(screen.getByText('C:\\original;C:\\manager')).toBeVisible();
    expect(screen.getByText('C:\\original;C:\\external')).toBeVisible();
    expect(api.createPlan).not.toHaveBeenCalled();
    expect(api.executePlan).not.toHaveBeenCalled();
  });
});
