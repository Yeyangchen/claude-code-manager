import { Component, StrictMode } from 'react';
import type { ErrorInfo, PropsWithChildren } from 'react';
import { createRoot } from 'react-dom/client';
import App from './app/App';
import './app/styles.css';

class ErrorBoundary extends Component<PropsWithChildren, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error: Error, info: ErrorInfo) {
    if (import.meta.env.DEV) console.error(error, info.componentStack);
  }
  render() {
    if (this.state.failed) return <main className="fatal-error"><h1>界面遇到了问题</h1><p>重新加载后将从后台恢复任务状态。</p><button className="button primary" onClick={() => window.location.reload()}>重新加载</button></main>;
    return this.props.children;
  }
}

createRoot(document.getElementById('root')!).render(<StrictMode><ErrorBoundary><App /></ErrorBoundary></StrictMode>);
