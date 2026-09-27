import { useEffect, useState } from 'react';
import { mdiRefresh, mdiCheck, mdiAlertCircleOutline } from '@mdi/js';
import { useMiner } from '../lib/state';
import { useT } from '../lib/i18n';
import { request } from '../lib/api';
import { Button, Icon, useAction } from './ui';

export function InventoryRefreshButton() {
  const { data, connected } = useMiner();
  const t = useT();
  const action = useAction();
  const refresh = data?.inventory_refresh;
  const sequence = refresh?.sequence ?? 0;
  const [dismissed, setDismissed] = useState(-1);
  useEffect(() => {
    action.clear();
  }, [sequence]);
  useEffect(() => {
    if (refresh?.state !== 'refreshed') return;
    const timer = window.setTimeout(() => setDismissed(sequence), 4000);
    return () => window.clearTimeout(timer);
  }, [refresh?.state, sequence]);
  const busy = action.busy || refresh?.state === 'refreshing';
  const error = action.error || refresh?.error;
  const failed = !busy && (Boolean(error) || refresh?.state === 'failed');
  const done = !busy && !failed && refresh?.state === 'refreshed' && dismissed !== sequence;
  return (
    <Button
      className="min-w-[160px]"
      disabled={!connected || !data?.login.user_id || busy}
      aria-busy={busy}
      title={failed ? (error ?? t('refresh_failed_detail')) : undefined}
      onClick={() => void action.run(() => request('/api/reload', {}))}
    >
      <Icon
        path={failed ? mdiAlertCircleOutline : done ? mdiCheck : mdiRefresh}
        className={busy ? 'animate-spin motion-reduce:animate-none' : ''}
      />
      <span aria-live="polite">
        {t(busy ? 'refreshing' : failed ? 'refresh_failed' : done ? 'refreshed' : 'refresh')}
      </span>
    </Button>
  );
}
