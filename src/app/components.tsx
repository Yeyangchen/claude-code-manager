import { t } from '../shared/i18n';
import { useEffect, useId, useRef } from 'react';
import type { PropsWithChildren, ReactNode } from 'react';
import { AlertCircle, Check, Circle, LoaderCircle, X } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import type { ManagerApi } from '../services/manager';
import { safeHttpsUrl } from '../services/manager';
export function Panel({ title, action, children, className = '' }: PropsWithChildren<{
    title?: string;
    action?: ReactNode;
    className?: string;
}>) {
    return <section className={`panel ${className}`}>
    {(title || action) && <div className="panel-heading"><h2>{title}</h2>{action}</div>}
    {children}
  </section>;
}
export function Badge({ children, tone = 'neutral' }: PropsWithChildren<{
    tone?: 'neutral' | 'success' | 'warning' | 'error' | 'accent';
}>) {
    return <span className={`badge badge-${tone}`}>{children}</span>;
}
export function Notice({ children, tone = 'info' }: PropsWithChildren<{
    tone?: 'info' | 'warning' | 'error' | 'success';
}>) {
    return <div className={`notice notice-${tone}`} role={tone === 'error' ? 'alert' : undefined}>
    {tone === 'success' ? <Check size={17}/> : <AlertCircle size={17}/>}
    <div>{children}</div>
  </div>;
}
export function Empty({ title, children, action }: PropsWithChildren<{
    title: string;
    action?: ReactNode;
}>) {
    return <div className="empty-state"><Circle size={28} strokeWidth={1.2}/><h3>{title}</h3>{children && <p>{children}</p>}{action}</div>;
}
export function Loading({ label = t("正在读取本机状态") }: {
    label?: string;
}) {
    return <div className="loading-state" role="status"><LoaderCircle className="spin" size={24}/><p>{label}</p></div>;
}
export function Modal({ title, children, footer, onClose, wide = false }: PropsWithChildren<{
    title: string;
    footer?: ReactNode;
    onClose: () => void;
    wide?: boolean;
}>) {
    const titleId = useId();
    const ref = useRef<HTMLDivElement>(null);
    useEffect(() => {
        const previous = document.activeElement as HTMLElement | null;
        ref.current?.focus();
        const onKey = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                event.preventDefault();
                onClose();
            }
            if (event.key !== 'Tab')
                return;
            const elements = ref.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]');
            if (!elements?.length) {
                event.preventDefault();
                return;
            }
            const first = elements[0];
            const last = elements[elements.length - 1];
            if (event.shiftKey && (document.activeElement === first || document.activeElement === ref.current)) {
                event.preventDefault();
                last.focus();
            }
            else if (!event.shiftKey && (document.activeElement === last || document.activeElement === ref.current)) {
                event.preventDefault();
                first.focus();
            }
        };
        document.addEventListener('keydown', onKey);
        return () => { document.removeEventListener('keydown', onKey); previous?.focus(); };
    }, [onClose]);
    return <div className="modal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget)
        onClose(); }}>
    <div ref={ref} className={`modal ${wide ? 'modal-wide' : ''}`} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}>
      <div className="modal-heading"><h2 id={titleId}>{title}</h2><button className="icon-button" onClick={onClose} aria-label={t("关闭对话框")}><X size={20}/></button></div>
      <div className="modal-body">{children}</div>
      {footer && <div className="modal-footer">{footer}</div>}
    </div>
  </div>;
}
export function ReleaseNotes({ content, api, onError }: {
    content: string;
    api: ManagerApi;
    onError: (message: string) => void;
}) {
    return <div className="release-notes"><ReactMarkdown skipHtml urlTransform={(url) => safeHttpsUrl(url) ?? ''} components={{
            a: ({ href, children }) => href ? <button className="text-link" onClick={() => void api.openUrl(href).catch((error: unknown) => onError(error instanceof Error ? error.message : String(error)))}>{children}</button> : <span>{children}</span>,
            img: ({ alt }) => <span>{alt ?? ''}</span>,
        }}>{content || t("这个版本暂未提供更新说明。")}</ReactMarkdown></div>;
}
