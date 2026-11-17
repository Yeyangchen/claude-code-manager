import { t } from '../shared/i18n';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { ChangeEntry, ManagerUpdate, Operation, Plan, PlanRequest, ReleaseCatalog, Settings, Snapshot, } from '../shared/types';
export interface ManagerApi {
    getSnapshot(): Promise<Snapshot>;
    checkReleases(force?: boolean): Promise<ReleaseCatalog>;
    createPlan(request: PlanRequest): Promise<Plan>;
    executePlan(planId: string, allowChecksumOnly: boolean): Promise<Operation>;
    getOperation(operationId: string): Promise<Operation>;
    cancelOperation(operationId: string): Promise<Operation>;
    saveSettings(settings: Settings, proxyUsername?: string, proxyPassword?: string): Promise<Settings>;
    getChanges(): Promise<ChangeEntry[]>;
    previewDiagnostics(): Promise<string>;
    exportDiagnostics(): Promise<{
        path: string;
    } | null>;
    openUrl(url: string): Promise<void>;
    checkManagerUpdate(): Promise<ManagerUpdate>;
    installManagerUpdate(expectedVersion: string): Promise<Operation>;
    onOperation(callback: (operation: Operation) => void): Promise<() => void>;
}
export function safeHttpsUrl(value: string): string | null {
    try {
        const url = new URL(value);
        if (url.protocol !== 'https:' || url.username || url.password)
            return null;
        return url.href;
    }
    catch {
        return null;
    }
}
export function errorMessage(error: unknown): string {
    if (typeof error === 'object' && error !== null && 'message' in error) {
        return String(error.message);
    }
    if (typeof error === 'string')
        return error;
    return t("操作未完成，请重新检查当前状态。");
}
export const desktopApi: ManagerApi = {
    getSnapshot: () => invoke('get_snapshot'),
    checkReleases: (force = false) => invoke('check_releases', { force }),
    createPlan: (request) => invoke('create_plan', { request }),
    executePlan: (planId, allowChecksumOnly) => invoke('execute_plan', { planId, allowChecksumOnly }),
    getOperation: (operationId) => invoke('get_operation', { operationId }),
    cancelOperation: (operationId) => invoke('cancel_operation', { operationId }),
    saveSettings: (settings, proxyUsername, proxyPassword) => invoke('save_settings', {
        settings, proxyUsername, proxyPassword,
    }),
    getChanges: () => invoke('get_changes'),
    previewDiagnostics: () => invoke('preview_diagnostics'),
    exportDiagnostics: () => invoke('export_diagnostics'),
    openUrl: (url) => {
        const safeUrl = safeHttpsUrl(url);
        if (!safeUrl)
            return Promise.reject(new Error(t("仅支持无认证信息的 HTTPS 链接。")));
        return invoke('open_url', { url: safeUrl });
    },
    checkManagerUpdate: () => invoke('check_manager_update'),
    installManagerUpdate: (expectedVersion) => invoke('install_manager_update', { expectedVersion }),
    onOperation: (callback) => listen<Operation>('manager://operation', (event) => callback(event.payload)),
};
export const previewMode = import.meta.env.DEV && !isTauri();
export async function getManagerApi(): Promise<ManagerApi> {
    if (isTauri())
        return desktopApi;
    if (previewMode)
        return (await import('./preview')).previewApi;
    throw new Error(t("请通过桌面应用打开 Claude Code Manager，以读取和管理本机安装。"));
}
