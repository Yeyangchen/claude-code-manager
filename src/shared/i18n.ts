import { zhCN } from './zh-CN';

/** All user-facing messages use one typed catalog, ready for another locale. */
export function t(key: keyof typeof zhCN, values?: Record<string, unknown>): string {
  const message: string = zhCN[key];
  return values ? message.replace(/\{(\w+)\}/g, (match, name: string) => name in values ? String(values[name]) : match) : message;
}
