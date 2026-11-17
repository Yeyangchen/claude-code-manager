import { vi } from 'vitest';
import type { ManagerApi } from '../services/manager';
import { previewSnapshot } from '../services/preview';
import type { Operation, Plan, ReleaseCatalog, Snapshot } from '../shared/types';

export const catalog: ReleaseCatalog = {
  latest: '2.1.326', stable: '2.1.320', checkedAt: '2026-11-16T12:00:00Z', cached: false, warning: null,
  releases: [
    { version: '2.1.326', channel: 'latest', publishedAt: null, notes: 'Release notes', verified: true, available: true, source: 'native' },
    { version: '2.1.100', channel: null, publishedAt: null, notes: 'Old release', verified: false, available: false, source: 'native' },
  ],
};

export const plan: Plan = {
  id: 'plan-123', kind: 'update', title: '更新 Claude Code', installationId: 'preview-native',
  currentVersion: '2.1.311', targetVersion: '2.1.320', steps: ['检查原生安装', '执行升级', '验证命中'],
  warnings: [], changes: [], canExecute: true, manualCommand: null, checksumOnly: false,
};

export const runningOperation: Operation = {
  id: 'operation-123', kind: 'update', title: '更新 Claude Code', phase: 'preparing', status: 'running',
  startedAt: '2026-11-16T12:00:00Z', finishedAt: null, messages: ['Preparing'], cancellable: true,
  progress: null, result: null, installationId: 'preview-native', targetVersion: '2.1.320',
};

export function createApi(state: Snapshot = structuredClone(previewSnapshot)): ManagerApi {
  return {
    getSnapshot: vi.fn().mockResolvedValue(state),
    checkReleases: vi.fn().mockResolvedValue(catalog),
    createPlan: vi.fn().mockResolvedValue(plan),
    executePlan: vi.fn().mockResolvedValue(runningOperation),
    getOperation: vi.fn().mockResolvedValue(runningOperation),
    cancelOperation: vi.fn().mockResolvedValue({ ...runningOperation, status: 'cancelled' }),
    saveSettings: vi.fn().mockImplementation(async (settings) => settings),
    getChanges: vi.fn().mockResolvedValue([]),
    previewDiagnostics: vi.fn().mockResolvedValue('{"user":"<redacted>"}'),
    exportDiagnostics: vi.fn().mockResolvedValue({ path: 'C:\\diagnostics.json' }),
    openUrl: vi.fn().mockResolvedValue(undefined),
    checkManagerUpdate: vi.fn().mockResolvedValue({ currentVersion: '0.1.0-beta.1', latestVersion: null, available: false, downloadUrl: '', signedUpdaterReady: false, message: 'No release' }),
    installManagerUpdate: vi.fn().mockResolvedValue({ ...runningOperation, kind: 'manager-update', title: '更新管理器', targetVersion: '0.1.0' }),
    onOperation: vi.fn().mockResolvedValue(() => {}),
  };
}
