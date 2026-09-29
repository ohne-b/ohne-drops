import { Icon } from '@mdi/react';
import { useEffect, useState } from 'react';
import { mdiRefresh, mdiCheck, mdiAlertCircleOutline } from '@mdi/js';
import { useMiner } from '../lib/state';
import { useT } from '../lib/i18n';
import { request } from '../lib/api';
import { Button, dateTime, useAction } from './ui';

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
  const catalogTime = data?.inventory_status?.catalog_updated_at;
  return (
    <Button
      className="min-w-[160px] justify-start"
      disabled={!connected || !data?.login.user_id || busy}
      aria-busy={busy}
      title={
        failed
          ? (error ?? t('refresh_failed_detail'))
          : catalogTime
            ? t('catalog_updated', { time: dateTime(catalogTime) })
            : undefined
      }
      onClick={() => void action.run(() => request('/api/reload', {}))}
    >
      <Icon
        path={failed ? mdiAlertCircleOutline : done ? mdiCheck : mdiRefresh}
        className={busy ? 'animate-spin motion-reduce:animate-none' : ''}
      />
      <span className="grid" aria-live="polite">
        {['refresh', 'refreshing', 'refreshed', 'refresh_failed'].map((key) => (
          <span key={key} className="invisible col-start-1 row-start-1" aria-hidden="true">
            {t(key)}
          </span>
        ))}
        <span className="col-start-1 row-start-1">
          {t(busy ? 'refreshing' : failed ? 'refresh_failed' : done ? 'refreshed' : 'refresh')}
        </span>
      </span>
    </Button>
  );
}
