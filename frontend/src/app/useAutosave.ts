import { useEffect, useRef, useState } from 'react';
import { ApiError, request } from '../shared/lib/api';
import type { Filters, Settings } from '../shared/lib/types';

export type Changes = Partial<
  Omit<Settings, 'revision' | 'games_available' | 'game_keys' | 'inventory_filters'>
> & { inventory_filters?: Partial<Filters> };
const nested = ['inventory_filters', 'mining_benefits'] as const;
const reloadKey = 'tdm.settings-draft';
export function mergeDraft(settings: Settings, changes: Changes): Settings {
  return {
    ...settings,
    ...changes,
    inventory_filters: { ...settings.inventory_filters, ...changes.inventory_filters },
    mining_benefits: { ...settings.mining_benefits, ...changes.mining_benefits },
  };
}
export function remainingChanges(queued: Changes, sent: Changes): Changes {
  const next = { ...queued };
  for (const key of Object.keys(next) as (keyof Changes)[]) {
    if ((nested as readonly string[]).includes(key)) {
      const rest = Object.fromEntries(
        Object.entries(next[key] ?? {}).filter(
          ([field, value]) =>
            JSON.stringify(value) !==
            JSON.stringify((sent[key] as Record<string, unknown> | undefined)?.[field]),
        ),
      );
      if (Object.keys(rest).length) Object.assign(next, { [key]: rest });
      else delete next[key];
    } else if (JSON.stringify(next[key]) === JSON.stringify(sent[key])) delete next[key];
  }
  return next;
}
function readReloadDraft(): { changes: Changes; revision?: string } | null {
  try {
    const raw = sessionStorage.getItem(reloadKey);
    if (!raw || raw.length > 100_000) return null;
    const value = JSON.parse(raw);
    if (
      typeof value.expires !== 'number' ||
      value.expires < Date.now() ||
      !value.changes ||
      typeof value.changes !== 'object' ||
      Array.isArray(value.changes)
    )
      return null;
    return value;
  } catch {
    return null;
  }
}
export function useAutosave(
  settings: Settings | undefined,
  connected: boolean,
  saved: (settings: Settings, previousRevision: string | undefined) => void,
) {
  const [restored] = useState(readReloadDraft);
  useEffect(() => {
    try {
      sessionStorage.removeItem(reloadKey);
    } catch {
      /* Storage can be disabled. */
    }
  }, []);
  const latest = useRef(settings);
  latest.current = settings;
  const queued = useRef<Changes>(restored?.changes ?? {});
  const baseline = useRef<{ revision?: string } | undefined>(restored ?? settings);
  const sending = useRef(false);
  const [changes, setChanges] = useState<Changes>(restored?.changes ?? {});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(restored ? 'settings_restored' : '');
  const pending = Object.keys(changes).length > 0;

  function change<K extends keyof Changes>(
    key: K,
    value: Settings[K] | ((current: Settings[K]) => Settings[K]),
  ) {
    if (!Object.keys(queued.current).length) baseline.current = latest.current;
    if (!latest.current) return;
    const current = mergeDraft(latest.current, queued.current);
    let next = typeof value === 'function' ? value(current[key]) : value;
    if ((nested as readonly string[]).includes(key)) {
      next = {
        ...(queued.current[key] as object),
        ...Object.fromEntries(
          Object.entries(next as object).filter(
            ([field, item]) =>
              JSON.stringify(item) !==
              JSON.stringify((current[key] as Record<string, unknown>)[field]),
          ),
        ),
      } as Settings[K];
    }
    queued.current = {
      ...queued.current,
      [key]: next,
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
      queued.current = remainingChanges(queued.current, sent);
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
    if (connected && !restored) setError('');
  }, [connected]);
  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (pending || busy) event.preventDefault();
    };
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [pending, busy]);
  return {
    draft: settings ? mergeDraft(settings, changes) : undefined,
    change,
    busy,
    pending,
    error,
    retry: () => save(true),
    reload: () => {
      try {
        if (Object.keys(queued.current).length)
          sessionStorage.setItem(
            reloadKey,
            JSON.stringify({
              changes: queued.current,
              revision: baseline.current?.revision,
              expires: Date.now() + 600_000,
            }),
          );
        window.location.reload();
      } catch {
        setError('settings_reload_failed');
      }
    },
  };
}
