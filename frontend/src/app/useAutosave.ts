import { useEffect, useRef, useState } from 'react';
import { ApiError, request } from '../shared/lib/api';
import type { Settings } from '../shared/lib/types';

type Changes = Partial<Omit<Settings, 'revision' | 'games_available'>>;
export function useAutosave(
  settings: Settings | undefined,
  connected: boolean,
  saved: (settings: Settings, previousRevision: string | undefined) => void,
) {
  const latest = useRef(settings);
  latest.current = settings;
  const queued = useRef<Changes>({});
  const baseline = useRef(settings);
  const sending = useRef(false);
  const [changes, setChanges] = useState<Changes>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const pending = Object.keys(changes).length > 0;

  function change<K extends keyof Changes>(
    key: K,
    value: Settings[K] | ((current: Settings[K]) => Settings[K]),
  ) {
    if (!Object.keys(queued.current).length) baseline.current = latest.current;
    const current = { ...latest.current, ...queued.current } as Settings;
    queued.current = {
      ...queued.current,
      [key]: typeof value === 'function' ? value(current[key]) : value,
    };
    setChanges(queued.current);
    setError('');
  }

  async function save(refresh = false) {
    if (sending.current || !latest.current || !connected) return;
    const sent = queued.current;
    if (!Object.keys(sent).length) return;
    if (
      ('minimum_refresh_interval_minutes' in sent &&
        (!Number.isInteger(sent.minimum_refresh_interval_minutes) ||
          sent.minimum_refresh_interval_minutes! < 1 ||
          sent.minimum_refresh_interval_minutes! > 1440)) ||
      ('proxy' in sent && sent.proxy !== '' && !/^https?:\/\/[^\s]+$/i.test(sent.proxy ?? ''))
    ) {
      setError('settings_invalid');
      return;
    }
    sending.current = true;
    setBusy(true);
    setError('');
    try {
      const base = refresh
        ? await request<Settings>('/api/settings')
        : (baseline.current ?? latest.current);
      const result = await request<{ settings: Settings }>('/api/settings', {
        ...sent,
        revision: base.revision,
      });
      saved(result.settings, base.revision);
      baseline.current = result.settings;
      // Edits made while this request was in flight belong to the next save.
      queued.current = Object.fromEntries(
        Object.entries(queued.current).filter(
          ([key, value]) => JSON.stringify(value) !== JSON.stringify(sent[key as keyof Changes]),
        ),
      );
      setChanges(queued.current);
    } catch (failure) {
      setError(
        failure instanceof ApiError && failure.status === 409
          ? 'settings_conflict'
          : 'settings_save_failed',
      );
    } finally {
      sending.current = false;
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!pending || busy || !connected || error) return;
    const timer = window.setTimeout(() => void save(), 500);
    return () => window.clearTimeout(timer);
  }, [changes, pending, busy, connected, error]);
  useEffect(() => {
    if (connected) setError('');
  }, [connected]);
  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (pending || busy) event.preventDefault();
    };
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [pending, busy]);
  return {
    draft: settings ? { ...settings, ...changes } : undefined,
    change,
    busy,
    pending,
    error,
    retry: () => save(true),
  };
}
