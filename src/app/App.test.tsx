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
});
