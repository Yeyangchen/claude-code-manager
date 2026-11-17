export type Source = 'native' | 'npm' | 'winget' | 'external';

export interface Installation {
  id: string;
  source: Source;
  path: string;
  version: string | null;
  scope: 'user' | 'machine' | 'unknown';
  active: boolean;
  capabilities: { upgrade: boolean; installVersion: boolean; uninstall: boolean };
  detail: string;
  manualCommand: string | null;
}

export interface Check {
  id: string;
  title: string;
  level: 'ok' | 'info' | 'warning' | 'error';
  message: string;
}

export interface ShellResolution {
  name: string;
  path: string | null;
  candidates: string[];
  detail: string;
}

export interface Environment {
  os: string;
  arch: string;
  userName: string;
  elevated: boolean;
  configDir: string;
  supported: boolean;
}

export interface PolicyState {
  mode: string;
  channel: string;
  minimumVersion: string | null;
  disabledUpdates: boolean;
  sources: { name: string; value: string }[];
  detail: string;
}

export interface Settings {
  theme: 'system' | 'light' | 'dark';
  proxyMode: 'system' | 'direct' | 'custom';
  proxyUrl: string;
  hasProxyCredentials: boolean;
  checkIntervalHours: number;
}

export interface Operation {
  id: string;
  kind: string;
  title: string;
  phase: string;
  status: 'running' | 'succeeded' | 'failed' | 'cancelled' | 'attention';
  startedAt: string;
  finishedAt: string | null;
  messages: string[];
  cancellable: boolean;
  progress: { downloaded: number; total: number | null } | null;
  result: string | null;
  installationId: string | null;
  targetVersion: string | null;
}

export interface Snapshot {
  appVersion: string;
  environment: Environment;
  installations: Installation[];
  shells: ShellResolution[];
  checks: Check[];
  policy: PolicyState;
  settings: Settings;
  activeOperation: Operation | null;
  history: Operation[];
  readOnly: boolean;
  dataDir: string;
}

export interface Release {
  version: string;
  channel: string | null;
  publishedAt: string | null;
  notes: string;
  verified: boolean;
  available: boolean;
  source: string;
}

export interface ReleaseCatalog {
  latest: string;
  stable: string;
  releases: Release[];
  checkedAt: string;
  cached: boolean;
  warning: string | null;
}

export interface PlanRequest {
  kind: 'install' | 'update' | 'reinstall' | 'uninstall' | 'path-repair' | 'policy' | 'restore-change';
  installationId?: string;
  targetVersion?: string;
  channel?: 'latest' | 'stable';
  policyMode?: 'official' | 'manual' | 'hold';
  minimumVersion?: string;
  changeId?: string;
}

export interface Plan {
  id: string;
  kind: string;
  title: string;
  installationId: string | null;
  currentVersion: string | null;
  targetVersion: string | null;
  steps: string[];
  warnings: string[];
  changes: { name: string; before: string; after: string }[];
  canExecute: boolean;
  manualCommand: string | null;
  checksumOnly: boolean;
}

export interface ChangeEntry {
  id: string;
  kind: string;
  description: string;
  createdAt: string;
  restored: boolean;
  canRestore: boolean;
  detail: string;
  comparison?: { name: string; before: string; written: string; current: string }[];
}

export interface ManagerUpdate {
  currentVersion: string;
  latestVersion: string | null;
  available: boolean;
  downloadUrl: string;
  signedUpdaterReady: boolean;
  message: string;
}
