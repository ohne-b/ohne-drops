import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import { io, type Socket } from 'socket.io-client';
import type { AuthStatus, Campaign, Channel, ServerEvents, Snapshot } from '../shared/lib/types';
import { request } from '../shared/lib/api';
import { I18n } from '../shared/lib/i18n';
import { useAutosave } from './useAutosave';
export function upsert<T extends { id: string | number }>(items: T[], item: T): T[] {
  return items.some((current) => current.id === item.id)
    ? items.map((current) => (current.id === item.id ? item : current))
    : [...items, item];
}
const Context = createContext<{
  data: Snapshot | null;
  connected: boolean;
  historyRevision: number;
  autosave: ReturnType<typeof useAutosave>;
}>({
  data: null,
  connected: false,
  historyRevision: 0,
  autosave: null as unknown as ReturnType<typeof useAutosave>,
});
export function MinerProvider({ children }: { children: ReactNode }) {
  const [data, setData] = useState<Snapshot | null>(null);
  const [historyRevision, setHistoryRevision] = useState(0);
  const [connected, setConnected] = useState(false);
  const autosave = useAutosave(data?.settings, connected, (settings, revision) => {
    setData((current) =>
      current &&
      (current.settings.revision === revision || current.settings.revision === settings.revision)
        ? { ...current, settings }
        : current,
    );
  });
  useEffect(() => {
    const socket: Socket<ServerEvents> = io({ autoConnect: false });
    const update = (fn: (state: Snapshot) => Snapshot) =>
      setData((current) => (current ? fn(current) : current));
    const channel = (item: Channel) =>
      update((state) => ({ ...state, channels: upsert(state.channels, item) }));
    const campaign = (item: Campaign) =>
      update((state) => ({ ...state, campaigns: upsert(state.campaigns, item) }));
    const checkAuth = (reconnect = false) => {
      void request<AuthStatus>('/api/auth/status')
        .then((auth) => {
          if (!auth.authenticated) window.dispatchEvent(new Event('auth-expired'));
          else {
            window.dispatchEvent(new Event('auth-updated'));
            if (reconnect) socket.connect();
          }
        })
        .catch(() => {});
    };
    socket.on('initial_state', (snapshot) => {
      setData({ ...snapshot, console: snapshot.console.slice(-1000) });
      setConnected(true);
    });
    socket.on('disconnect', (reason) => {
      setConnected(false);
      checkAuth(reason === 'io server disconnect');
    });
    socket.on('connect_error', () => {
      setConnected(false);
      checkAuth();
    });
    socket.on('status_update', (value) => update((state) => ({ ...state, status: value.status })));
    socket.on('console_output', (value) =>
      update((state) => ({ ...state, console: [...state.console, value.message].slice(-1000) })),
    );
    socket.on('channel_add', channel);
    socket.on('channel_update', channel);
    socket.on('channel_remove', (value) =>
      update((state) => ({
        ...state,
        channels: state.channels.filter((item) => item.id !== value.id),
      })),
    );
    socket.on('channels_clear', () => update((state) => ({ ...state, channels: [] })));
    socket.on('channels_batch_update', (value) =>
      update((state) => ({ ...state, channels: value.channels })),
    );
    socket.on('channel_watching', (value) =>
      update((state) => ({
        ...state,
        channels: state.channels.map((item) => ({ ...item, watching: item.id === value.id })),
      })),
    );
    socket.on('channel_watching_clear', () =>
      update((state) => ({
        ...state,
        channels: state.channels.map((item) => ({ ...item, watching: false })),
      })),
    );
    socket.on('drop_progress', (value) => update((state) => ({ ...state, current_drop: value })));
    socket.on('drop_progress_stop', () => update((state) => ({ ...state, current_drop: null })));
    socket.on('campaign_add', campaign);
    socket.on('history_cleared', () => setHistoryRevision((value) => value + 1));
    socket.on('inventory_clear', () => update((state) => ({ ...state, campaigns: [] })));
    socket.on('inventory_batch_update', (value) =>
      update((state) => ({ ...state, campaigns: value.campaigns })),
    );
    socket.on('inventory_status', (value) =>
      update((state) => ({ ...state, inventory_status: value })),
    );
    socket.on('inventory_refresh', (value) =>
      update((state) =>
        value.sequence >= (state.inventory_refresh?.sequence ?? 0)
          ? { ...state, inventory_refresh: value }
          : state,
      ),
    );
    socket.on('drop_update', (value) =>
      update((state) => ({
        ...state,
        campaigns: state.campaigns.map((item) =>
          item.id === value.campaign_id
            ? { ...item, ...value.campaign, drops: value.drops ?? upsert(item.drops, value.drop) }
            : item,
        ),
      })),
    );
    socket.on('settings_updated', (value) => update((state) => ({ ...state, settings: value })));
    socket.on('games_available', (value) =>
      update((state) => ({
        ...state,
        settings: { ...state.settings, games_available: value.games },
      })),
    );
    socket.on('login_status', (value) => update((state) => ({ ...state, login: value })));
    socket.on('login_required', () =>
      update((state) => ({ ...state, login: { status: '', user_id: null } })),
    );
    socket.on('oauth_code_required', (value) =>
      update((state) => ({ ...state, login: { ...state.login, oauth_pending: value } })),
    );
    socket.on('manual_mode_update', (value) =>
      update((state) => ({ ...state, manual_mode: value })),
    );
    socket.on('wanted_items_update', (value) =>
      update((state) => ({ ...state, wanted_items: value })),
    );
    socket.on('notification', (value) => {
      if ('Notification' in window && Notification.permission === 'granted')
        new Notification(value.title, { body: value.message });
    });
    socket.connect();
    return () => {
      socket.removeAllListeners();
      socket.disconnect();
    };
  }, []);
  return (
    <Context value={{ data, connected, historyRevision, autosave }}>
      <I18n>{children}</I18n>
    </Context>
  );
}
export const useMiner = () => useContext(Context);
