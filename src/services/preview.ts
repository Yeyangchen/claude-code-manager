import type { ManagerApi } from './manager';
import { version as appVersion } from '../../package.json';
import type { Operation, Plan, PlanRequest, Snapshot, ChangeEntry } from '../shared/types';

// Imported only by the development browser entry. This adapter never reads the host.
export const previewSnapshot: Snapshot = {
  appVersion,
  environment: {
    os: 'Windows 11', arch: 'x64', userName: 'preview', elevated: false,
    configDir: 'C:\\Users\\preview\\.claude', supported: true,
  },
  installations: [
    {
      id: 'preview-native', source: 'native', path: 'C:\\Users\\preview\\.local\\bin\\claude.exe',
      version: '2.1.311', scope: 'user', active: true,
      capabilities: { upgrade: true, installVersion: true, uninstall: true },
      detail: '官方原生安装 · 当前用户', manualCommand: null,
    },
    {
      id: 'preview-npm', source: 'npm', path: 'C:\\Users\\preview\\AppData\\Roaming\\npm\\claude.cmd',
      version: '2.1.300', scope: 'user', active: false,
      capabilities: { upgrade: true, installVersion: false, uninstall: true },
      detail: '另有一份 npm 安装，当前终端优先使用 Native。', manualCommand: null,
    },
  ],
  shells: [
    { name: 'PowerShell', path: 'C:\\Users\\preview\\.local\\bin\\claude.exe', candidates: [], detail: '新进程命中 Native 安装' },
    { name: 'CMD', path: 'C:\\Users\\preview\\.local\\bin\\claude.exe', candidates: [], detail: '新进程命中 Native 安装' },
  ],
  checks: [
    { id: 'os', title: 'Windows 系统', level: 'ok', message: 'Windows 11 x64，满足运行条件。' },
    { id: 'git', title: 'Git for Windows', level: 'ok', message: '已检测到 Git Bash。' },
    { id: 'node', title: 'Node.js', level: 'ok', message: 'v24.14.0，npm 环境可用。' },
    { id: 'duplicates', title: '多份安装', level: 'warning', message: '发现 2 份安装，可在环境页选择保留项。' },
  ],
  policy: { mode: 'manual', channel: 'stable', minimumVersion: null, disabledUpdates: false, sources: [{ name: '用户配置', value: '手动更新 · stable' }], detail: '此策略影响使用该配置目录的安装。' },
  settings: { theme: 'system', proxyMode: 'system', proxyUrl: '', hasProxyCredentials: false, checkIntervalHours: 6 },
  activeOperation: null, history: [], readOnly: false,
  dataDir: 'C:\\Users\\preview\\AppData\\Roaming\\io.github.yeyangchen.claude-code-manager',
};

const snapshot = structuredClone(previewSnapshot);
// `?state=empty` previews a machine where only Claude Desktop's private CLI exists.
if (typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('state') === 'empty') {
  snapshot.installations = [];
  snapshot.shells = snapshot.shells.map((shell) => ({ ...shell, path: null, detail: '新进程未找到 claude 命令' }));
  snapshot.checks = [
    ...snapshot.checks.filter((check) => check.id !== 'duplicates'),
    { id: 'claude-desktop', title: 'Claude 桌面应用', level: 'info', message: '检测到 Claude 桌面应用内置的 Claude Code 2.1.325。它只供桌面应用自身使用，不在终端 PATH 中，管理器不会修改它；在终端使用 claude 需要另外安装。' },
  ];
}
const plans = new Map<string, { plan: Plan; request: PlanRequest }>();
const changes: ChangeEntry[] = [];
const listeners = new Set<(operation: Operation) => void>();

function emit(operation: Operation) {
  snapshot.activeOperation = operation.status === 'running' ? operation : null;
  if (operation.status !== 'running') snapshot.history = [operation, ...snapshot.history];
  listeners.forEach((listener) => listener(structuredClone(operation)));
}

export const previewApi: ManagerApi = {
  getSnapshot: async () => structuredClone(snapshot),
  checkReleases: async () => ({
    latest: '2.1.326', stable: '2.1.311', checkedAt: new Date().toISOString(), cached: false,
    warning: '开发预览数据，版本号与安装状态仅用于界面演示。',
    releases: [
      { version: '2.1.326', channel: 'latest', publishedAt: '2026-11-16T17:26:18Z', notes: '## 更新内容\n\n- 改善后台会话恢复。\n- 优化设置错误的提示。\n\n[查看官方更新日志](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md)', verified: true, available: true, source: 'native' },
      { version: '2.1.311', channel: 'stable', publishedAt: null, notes: '当前稳定通道版本。', verified: true, available: true, source: 'native' },
      { version: '2.1.300', channel: null, publishedAt: null, notes: '历史版本。安装前会重新核对官方产物。', verified: true, available: true, source: 'native' },
    ],
  }),
  createPlan: async (request) => {
    const installation = snapshot.installations.find((item) => item.id === request.installationId);
    const titles: Record<string, string> = { setup: '自动管理本机 Claude Code', install: '安装 Claude Code', update: '更新 Claude Code', reinstall: '重装指定版本', uninstall: '卸载安装实例', 'path-repair': '修复用户 PATH', policy: '更改更新策略', 'restore-change': '还原更改' };
    const plan: Plan = {
      id: crypto.randomUUID(), kind: request.kind, title: titles[request.kind],
      installationId: request.installationId ?? null, currentVersion: installation?.version ?? null,
      targetVersion: request.targetVersion ?? (['setup', 'install', 'update'].includes(request.kind) ? '2.1.326' : null),
      steps: request.kind === 'setup'
        ? ['复核安装和配置状态', '从官方来源下载并安装原生 Claude Code', '核验目标版本、SHA256 与 Windows 签名', '把原生安装目录加入用户 PATH', '通过新进程复核终端命中关系']
        : ['核对目标安装和当前状态', '执行选定操作', '检查版本与终端命中关系'],
      warnings: ['开发预览只模拟任务，所有数据保存在当前页面内存中。'],
      changes: request.kind === 'path-repair' || request.kind === 'setup' ? [{ name: '用户 PATH', before: '%USERPROFILE%\\AppData\\Roaming\\npm', after: '%USERPROFILE%\\AppData\\Roaming\\npm;%USERPROFILE%\\.local\\bin' }] : [],
      canExecute: true, manualCommand: null, checksumOnly: false,
    };
    plans.set(plan.id, { plan, request });
    return plan;
  },
  executePlan: async (planId) => {
    const stored = plans.get(planId);
    if (!stored) throw new Error('操作计划已过期，请重新预览。');
    const { plan, request } = stored;
    const operation: Operation = {
      id: crypto.randomUUID(), kind: plan.kind, title: plan.title, phase: 'preparing', status: 'running',
      startedAt: new Date().toISOString(), finishedAt: null, messages: ['正在核对安装状态…'],
      cancellable: true, progress: null, result: null, installationId: plan.installationId,
      targetVersion: plan.targetVersion,
    };
    emit(operation);
    window.setTimeout(() => {
      if (operation.status === 'cancelled') return;
      operation.phase = 'verifying';
      operation.cancellable = false;
      operation.messages.push('预览执行器已完成模拟操作。');
      emit(operation);
    }, 450);
    window.setTimeout(() => {
      if (operation.status === 'cancelled') return;
      if (plan.kind === 'uninstall') snapshot.installations = snapshot.installations.filter((item) => item.id !== plan.installationId);
      if (plan.kind === 'setup' && !snapshot.installations.some((item) => item.source === 'native')) {
        snapshot.installations.unshift({ ...structuredClone(previewSnapshot.installations[0]), version: plan.targetVersion });
        snapshot.shells = structuredClone(previewSnapshot.shells);
      }
      if (plan.targetVersion) {
        const item = snapshot.installations.find((entry) => entry.id === plan.installationId);
        if (item) item.version = plan.targetVersion;
      }
      if (plan.kind === 'policy') {
        snapshot.policy.mode = request.policyMode ?? 'manual';
        snapshot.policy.channel = request.channel ?? 'stable';
      }
      if (plan.kind === 'path-repair' || plan.kind === 'policy') changes.unshift({ id: operation.id, kind: plan.kind, description: plan.title, createdAt: operation.startedAt, restored: false, canRestore: true, detail: '预览变更记录' });
      if (plan.kind === 'restore-change') {
        const change = changes.find((entry) => entry.id === request.changeId);
        if (change) { change.restored = true; change.canRestore = false; }
      }
      operation.status = 'succeeded';
      operation.phase = 'completed';
      operation.finishedAt = new Date().toISOString();
      operation.result = '预览操作已完成，本机环境未发生变化。';
      operation.messages.push(operation.result);
      emit(operation);
    }, 1100);
    return structuredClone(operation);
  },
  getOperation: async (id) => {
    const operation = snapshot.activeOperation?.id === id ? snapshot.activeOperation : snapshot.history.find((item) => item.id === id);
    if (!operation) throw new Error('未找到操作记录。');
    return structuredClone(operation);
  },
  cancelOperation: async (id) => {
    const operation = snapshot.activeOperation;
    if (!operation || operation.id !== id || !operation.cancellable) throw new Error('该阶段暂时无法取消。');
    operation.status = 'cancelled';
    operation.phase = 'cancelled';
    operation.finishedAt = new Date().toISOString();
    operation.cancellable = false;
    emit(operation);
    return structuredClone(operation);
  },
  saveSettings: async (settings, username, password) => {
    snapshot.settings = { ...settings, hasProxyCredentials: Boolean(username && password) || settings.hasProxyCredentials };
    return structuredClone(snapshot.settings);
  },
  getChanges: async () => structuredClone(changes),
  previewDiagnostics: async () => JSON.stringify({ mode: 'development-preview', version: snapshot.appVersion, installations: snapshot.installations.map((item) => ({ source: item.source, version: item.version, path: item.path.replace('preview', '<user>') })) }, null, 2),
  exportDiagnostics: async () => { throw new Error('请在桌面应用中导出本地诊断文件。'); },
  openUrl: async (url) => { window.open(url, '_blank', 'noopener,noreferrer'); },
  checkManagerUpdate: async () => ({ currentVersion: snapshot.appVersion, latestVersion: null, available: false, downloadUrl: 'https://github.com/Yeyangchen/claude-code-manager/releases', signedUpdaterReady: false, message: '开发预览。正式更新信息由 GitHub Releases 提供。' }),
  installManagerUpdate: async () => { throw new Error('管理器更新需要在桌面应用中执行。'); },
  onOperation: async (callback) => { listeners.add(callback); return () => { listeners.delete(callback); }; },
};
