import type { Settings, Snapshot, StatePatch } from '../shared/lib/types';

export interface MinerState {
  data: Snapshot | null;
  hydrated: boolean;
  incompatible: boolean;
  resync: boolean;
}
export const initialState: MinerState = {
  data: null,
  hydrated: false,
  incompatible: false,
  resync: false,
};
export type StateAction =
  | { type: 'snapshot'; value: Snapshot }
  | { type: 'patch'; value: StatePatch }
  | { type: 'disconnect' }
  | { type: 'incompatible' }
  | { type: 'settings'; settings: Settings; revision?: string };

export function reducer(state: MinerState, action: StateAction): MinerState {
  if (action.type === 'disconnect') return { ...state, hydrated: false };
  if (action.type === 'incompatible') return { ...state, hydrated: false, incompatible: true };
  if (action.type === 'settings') {
    const current = state.data;
    if (
      !current ||
      (current.settings.revision !== action.revision &&
        current.settings.revision !== action.settings.revision)
    )
      return state;
    return { ...state, data: { ...current, settings: action.settings } };
  }
  const value = action.value;
  if (value.protocol !== 2) return { ...state, hydrated: false, incompatible: true };
  if (!value.instance || !Number.isSafeInteger(value.revision) || value.revision < 1)
    return { ...state, hydrated: false, resync: true };
  if (action.type === 'snapshot') {
    if (state.data?.instance === value.instance && value.revision < state.data.revision)
      return state;
    return { data: action.value, hydrated: true, incompatible: false, resync: false };
  }
  const current = state.data;
  if (current?.instance === value.instance && value.revision <= current.revision) return state;
  if (
    !state.hydrated ||
    !current ||
    value.instance !== current.instance ||
    action.value.base_revision !== current.revision
  )
    return { ...state, hydrated: false, resync: true };
  return { ...state, data: { ...current, ...action.value.changes, revision: value.revision } };
}
