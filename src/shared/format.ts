export function displayDate(value: string | null, includeTime = true): string {
  if (!value) return '尚未检查';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit', day: '2-digit',
    ...(includeTime ? { hour: '2-digit', minute: '2-digit' } as const : {}),
  }).format(date);
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export const sourceNames: Record<string, string> = {
  native: 'Native', npm: 'npm', winget: 'WinGet', external: '外部应用',
};

export const phaseNames: Record<string, string> = {
  planned: '计划已生成', waiting: '等待会话结束', preparing: '执行前核查',
  downloading: '下载中', verifying: '验证结果', installing: '安装中',
  applying: '应用更改', committing: '提交更改', recovering: '核查与恢复',
  completed: '已完成', finishing: '收尾验证', cancelled: '已取消',
};

export const statusNames: Record<string, string> = {
  running: '正在执行', succeeded: '已完成', failed: '未完成',
  cancelled: '已取消', attention: '需要处理',
};

export const policyNames: Record<string, string> = {
  official: '跟随官方更新', manual: '提醒后手动更新', hold: '保持当前版本',
};
