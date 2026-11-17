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

  const refresh = useCallback(async () => {
    if (!api) return;
    const next = await api.getSnapshot();
    if (!mounted.current) return;
    setSnapshot(next);
    if (next.activeOperation) setOperation(next.activeOperation);
    setLoading(false);
  }, [api]);

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
    if (!injectedApi) getManagerApi().then(setApi).catch((failure) => {
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
      setOperation(next);
      if (next.status !== 'running') refresh().catch((failure) => setError(errorMessage(failure)));
    }).then((stop) => { if (disposed) stop(); else unsubscribe = stop; }).catch((failure) => {
      if (!disposed) setError(errorMessage(failure));
    });
    return () => { disposed = true; unsubscribe?.(); };
  }, [api, refresh, checkReleases]);

  useEffect(() => {
    if (!api || operation?.status !== 'running') return;
    const timer = window.setInterval(() => {
      api.getOperation(operation.id).then((next) => {
        setOperation(next);
        if (next.status !== 'running') refresh().catch((failure) => setError(errorMessage(failure)));
      }).catch((failure) => setError(errorMessage(failure)));
    }, 2000);
    return () => window.clearInterval(timer);
  }, [api, operation?.id, operation?.status, refresh]);

  const intervalHours = snapshot?.settings.checkIntervalHours;
  useEffect(() => {
    if (!intervalHours || intervalHours < 1) return;
    const timer = window.setInterval(() => { void checkReleases(false); }, intervalHours * 3_600_000);
    return () => window.clearInterval(timer);
  }, [intervalHours, checkReleases]);

  return { api, snapshot, setSnapshot, catalog, operation, setOperation, loading, checking, error, setError, refresh, checkReleases };
}
