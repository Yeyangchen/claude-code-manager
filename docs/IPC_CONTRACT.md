# Desktop IPC contract

All objects use camelCase. Commands reject with `{ code: string, message: string }`. Paths and commands are resolved by Rust. All mutation flows require a preview plan and `execute_plan` with the returned opaque ID. Values shown below are TypeScript descriptions.

```ts
type Source = 'native' | 'npm' | 'winget' | 'external';
interface Installation {
  id: string; source: Source; path: string; version: string | null;
  scope: 'user' | 'machine' | 'unknown'; active: boolean;
  capabilities: { upgrade: boolean; installVersion: boolean; uninstall: boolean };
  detail: string; manualCommand: string | null;
}
interface Check { id: string; title: string; level: 'ok' | 'info' | 'warning' | 'error'; message: string }
interface ShellResolution { name: string; path: string | null; candidates: string[]; detail: string }
interface Environment { os: string; arch: string; userName: string; elevated: boolean; configDir: string; supported: boolean }
interface PolicyState { mode: string; channel: string; minimumVersion: string | null; disabledUpdates: boolean; sources: { name: string; value: string }[]; detail: string }
interface Settings { theme: 'system' | 'light' | 'dark'; proxyMode: 'system' | 'direct' | 'custom'; proxyUrl: string; hasProxyCredentials: boolean; checkIntervalHours: number }
interface Operation {
  id: string; kind: string; title: string; phase: string; status: 'running' | 'succeeded' | 'failed' | 'cancelled' | 'attention';
  startedAt: string; finishedAt: string | null; messages: string[];
  cancellable: boolean; progress: { downloaded: number; total: number | null } | null;
  result: string | null; installationId: string | null; targetVersion: string | null;
}
interface Snapshot {
  appVersion: string; environment: Environment; installations: Installation[];
  shells: ShellResolution[]; checks: Check[]; policy: PolicyState; settings: Settings;
  activeOperation: Operation | null; history: Operation[]; readOnly: boolean; dataDir: string;
}
interface Release { version: string; channel: string | null; publishedAt: string | null; notes: string; verified: boolean; available: boolean; source: string }
interface ReleaseCatalog { latest: string; stable: string; releases: Release[]; checkedAt: string; cached: boolean; warning: string | null }
interface PlanRequest {
  kind: 'install' | 'update' | 'reinstall' | 'uninstall' | 'path-repair' | 'policy' | 'restore-change';
  installationId?: string; targetVersion?: string; channel?: 'latest' | 'stable';
  policyMode?: 'official' | 'manual' | 'hold'; minimumVersion?: string; changeId?: string;
}
interface Plan {
  id: string; kind: string; title: string; installationId: string | null;
  currentVersion: string | null; targetVersion: string | null; steps: string[];
  warnings: string[]; changes: { name: string; before: string; after: string }[];
  canExecute: boolean; manualCommand: string | null; checksumOnly: boolean;
}
interface ChangeEntry { id: string; kind: string; description: string; createdAt: string; restored: boolean; canRestore: boolean; detail: string }
interface ManagerUpdate { currentVersion: string; latestVersion: string | null; available: boolean; downloadUrl: string; signedUpdaterReady: boolean; message: string }
```

| Command | Arguments | Result |
| --- | --- | --- |
| `get_snapshot` | none | `Snapshot` |
| `check_releases` | `{ force?: boolean }` | `ReleaseCatalog` |
| `create_plan` | `{ request: PlanRequest }` | `Plan` |
| `execute_plan` | `{ planId: string, allowChecksumOnly?: boolean }` | `Operation` |
| `get_operation` | `{ operationId: string }` | `Operation` |
| `cancel_operation` | `{ operationId: string }` | `Operation` |
| `save_settings` | `{ settings: Settings, proxyUsername?: string, proxyPassword?: string }` | `Settings` |
| `get_changes` | none | `ChangeEntry[]` |
| `preview_diagnostics` | none | `string` |
| `export_diagnostics` | none | `{ path: string }` or `null` if dialog cancelled |
| `open_url` | `{ url: string }` | `void` |
| `check_manager_update` | none | `ManagerUpdate` |

Subscribe to `manager://operation` for `Operation` snapshots and also poll `get_snapshot` after reloading the renderer. `execute_plan` returns immediately after the task starts. Cancel applies while waiting/preparing, not during an installer commit. Checksum-only native versions need explicit checkbox acknowledgment in the preview UI.

Browser preview must display a clear preview indicator and use an isolated simulation adapter. The production desktop always invokes Rust.
