import { createContext, useContext, useEffect, useReducer, useRef, type ReactNode } from 'react';
import { io, type Socket } from 'socket.io-client';
import type { AuthStatus, ServerEvents, Snapshot } from '../shared/lib/types';
import { request } from '../shared/lib/api';
import { I18n } from '../shared/lib/i18n';
import { useAutosave } from './useAutosave';
import { initialState, reducer } from './reducer';

export function upsert<T extends { id: string | number }>(items: T[], item: T): T[] {
  return items.some((current) => current.id === item.id)
    ? items.map((current) => (current.id === item.id ? item : current))
    : [...items, item];
}
const Context = createContext<{
  data: Snapshot | null;
  connected: boolean;
  incompatible: boolean;
  historyRevision: number;
  autosave: ReturnType<typeof useAutosave>;
}>({
  data: null,
  connected: false,
  incompatible: false,
  historyRevision: 0,
  autosave: null as unknown as ReturnType<typeof useAutosave>,
});

export function MinerProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const transport = useRef<Socket<ServerEvents, { state_resync: () => void }> | null>(null);
  const autosave = useAutosave(state.data?.settings, state.hydrated, (settings, revision) => {
    dispatch({ type: 'settings', settings, revision });
  });
  useEffect(() => {
    if (state.resync) transport.current?.emit('state_resync');
  }, [state.resync]);
  useEffect(() => {
    const socket: Socket<ServerEvents, { state_resync: () => void }> = io({
      autoConnect: false,
      query: { protocol: '2' },
    });
    transport.current = socket;
    let disposed = false;
    let incompatible = false;
    const checkAuth = (reconnect = false) => {
      void request<AuthStatus>('/api/auth/status')
        .then((auth) => {
          if (disposed) return;
          if (!auth.authenticated) window.dispatchEvent(new Event('auth-expired'));
          else {
            window.dispatchEvent(new Event('auth-updated'));
            if (reconnect && !incompatible) socket.connect();
          }
        })
        .catch(() => {});
    };
    const mismatch = () => {
      incompatible = true;
      dispatch({ type: 'incompatible' });
    };
    socket.on('state_snapshot', (value) => dispatch({ type: 'snapshot', value }));
    socket.on('state_patch', (value) => dispatch({ type: 'patch', value }));
    socket.on('protocol_mismatch', mismatch);
    socket.on('initial_state', mismatch);
    socket.on('disconnect', (reason) => {
      dispatch({ type: 'disconnect' });
      checkAuth(reason === 'io server disconnect');
    });
    socket.on('connect_error', () => {
      dispatch({ type: 'disconnect' });
      checkAuth();
    });
    socket.on('notification', (value) => {
      if ('Notification' in window && Notification.permission === 'granted')
        new Notification(value.title, { body: value.message });
    });
    socket.connect();
    return () => {
      disposed = true;
      transport.current = null;
      socket.removeAllListeners();
      socket.disconnect();
    };
  }, []);
  return (
    <Context
      value={{
        data: state.data,
        connected: state.hydrated,
        incompatible: state.incompatible,
        historyRevision: state.data?.history_clear_revision ?? 0,
        autosave,
      }}
    >
      <I18n>{children}</I18n>
    </Context>
  );
}
export const useMiner = () => useContext(Context);
