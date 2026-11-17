import { t } from './i18n';
export function displayDate(value: string | null, includeTime = true): string {
    if (!value)
        return t("尚未检查");
    const date = new Date(value);
    if (Number.isNaN(date.getTime()))
        return value;
    return new Intl.DateTimeFormat('zh-CN', {
        month: '2-digit', day: '2-digit',
        ...(includeTime ? { hour: '2-digit', minute: '2-digit' } as const : {}),
    }).format(date);
}
export function formatBytes(bytes: number): string {
    if (bytes < 1024)
        return `${bytes} B`;
    if (bytes < 1024 * 1024)
        return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
export const sourceNames: Record<string, string> = {
    native: 'Native', npm: 'npm', winget: 'WinGet', external: t("外部应用"),
};
export const phaseNames: Record<string, string> = {
    planned: t("计划已生成"), waiting: t("等待会话结束"), preparing: t("执行前核查"),
    downloading: t("下载中"), verifying: t("验证结果"), installing: t("安装中"),
    applying: t("应用更改"), committing: t("提交更改"), recovering: t("核查与恢复"),
    completed: t("已完成"), finishing: t("收尾验证"), cancelled: t("已取消"),
};
export const statusNames: Record<string, string> = {
    running: t("正在执行"), succeeded: t("已完成"), failed: t("未完成"),
    cancelled: t("已取消"), attention: t("需要处理"),
};
export const policyNames: Record<string, string> = {
    official: t("跟随官方更新"), manual: t("提醒后手动更新"), hold: t("保持当前版本"),
};
