import { useCallback, useEffect, useRef, useState } from 'react';
import type { ManagerApi } from '../services/manager';
import { errorMessage, getManagerApi } from '../services/manager';
import type { Operation, ReleaseCatalog, Snapshot } from '../shared/types';

export function useManager(injectedApi?: ManagerApi) {
  const [api, setApi] = useState<ManagerApi | null>(injectedApi ?? null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [catalog, setCatalog] = useState<ReleaseCatalog | null>(null);
  const [operation, setOperation] = useState<Operation | null>(null);
  const [loading, setLoading] = useState(true);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);

  const acceptOperation = useCallback((next: Operation) => {
    setOperation((previous) => previous?.id === next.id && previous.status !== 'running' && next.status === 'running' ? previous : next);
  }, []);

  const refresh = useCallback(async () => {
    if (!api) return;
    const next = await api.getSnapshot();
    if (!mounted.current) return;
    setSnapshot(next);
    if (next.activeOperation) acceptOperation(next.activeOperation);
    else setOperation((previous) => previous ? next.history.find((item) => item.id === previous.id) ?? previous : null);
    setLoading(false);
  }, [api, acceptOperation]);

  const checkReleases = useCallback(async (force = true) => {
    if (!api) return;
    setChecking(true);
    try {
      const next = await api.checkReleases(force);
      if (mounted.current) setCatalog(next);
    } catch (failure) {
      if (mounted.current) setError(errorMessage(failure));
    } finally {
      if (mounted.current) setChecking(false);
    }
  }, [api]);

  useEffect(() => {
    mounted.current = true;
    if (!injectedApi) getManagerApi().then((next) => { if (mounted.current) setApi(next); }).catch((failure) => {
      if (mounted.current) { setError(errorMessage(failure)); setLoading(false); }
    });
    return () => { mounted.current = false; };
  }, [injectedApi]);

  useEffect(() => {
    if (!api) return;
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    refresh().catch((failure) => { if (!disposed) { setError(errorMessage(failure)); setLoading(false); } });
    void checkReleases(false);
    api.onOperation((next) => {
      if (disposed) return;
      acceptOperation(next);
      if (next.status !== 'running') refresh().catch((failure) => setError(errorMessage(failure)));
    }).then((stop) => { if (disposed) stop(); else unsubscribe = stop; }).catch((failure) => {
      if (!disposed) setError(errorMessage(failure));
    });
    return () => { disposed = true; unsubscribe?.(); };
  }, [api, refresh, checkReleases, acceptOperation]);

  useEffect(() => {
    if (!api || operation?.status !== 'running') return;
    let disposed = false;
    let pending = false;
    const timer = window.setInterval(() => {
      if (pending) return;
      pending = true;
      api.getOperation(operation.id).then((next) => {
        if (disposed) return;
        acceptOperation(next);
        if (next.status !== 'running') refresh().catch((failure) => setError(errorMessage(failure)));
      }).catch((failure) => { if (!disposed) setError(errorMessage(failure)); }).finally(() => { pending = false; });
    }, 2000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [api, operation?.id, operation?.status, refresh, acceptOperation]);

  const intervalHours = snapshot?.settings.checkIntervalHours;
  useEffect(() => {
    if (!intervalHours || intervalHours < 1) return;
    const timer = window.setInterval(() => {
      void checkReleases(false);
      void refresh().catch((failure) => { if (mounted.current) setError(errorMessage(failure)); });
    }, intervalHours * 3_600_000);
    return () => window.clearInterval(timer);
  }, [intervalHours, checkReleases, refresh]);

  return { api, snapshot, setSnapshot, catalog, operation, setOperation: acceptOperation, loading, checking, error, setError, refresh, checkReleases };
}
