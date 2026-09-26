import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router';
import { mdiDownload, mdiDeleteOutline } from '@mdi/js';
import { useMiner } from '../lib/state';
import { useT } from '../lib/i18n';
import { request } from '../lib/api';
import type { HistoryEntry } from '../lib/types';
import {
  Button,
  Dialog,
  Empty,
  Field,
  Icon,
  Input,
  Notice,
  useAction,
  ActionResult,
  dateTime,
} from '../components/ui';
export default function History() {
  const t = useT();
  const { connected, data } = useMiner();
  const [params, setParams] = useSearchParams();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [confirm, setConfirm] = useState(false);
  const action = useAction();
  const game = params.get('game') ?? '';
  const since = params.get('since') ?? '';
  const page = Math.max(0, Number(params.get('page')) || 0);
  const query = new URLSearchParams({ ...(game ? { game } : {}), ...(since ? { since } : {}) });
  const queryString = query.toString();
  // ponytail: fetch the personal history once; add API pagination if its size becomes a problem.
  const claimed = data?.campaigns.reduce((sum, campaign) => sum + campaign.claimed_drops, 0);
  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setError(false);
    request<{ entries: HistoryEntry[]; total: number }>(
      `/api/history?${queryString}`,
      undefined,
      'GET',
      controller.signal,
    )
      .then((result) => {
        if (!controller.signal.aborted) {
          setEntries(result.entries);
          setTotal(result.total);
        }
      })
      .catch(() => {
        if (!controller.signal.aborted) setError(true);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [queryString, refresh, connected, claimed]);
  function filter(key: string, value: string) {
    const next = new URLSearchParams(params);
    value ? next.set(key, value) : next.delete(key);
    next.delete('page');
    setParams(next, { replace: true });
  }
  const pages = Math.max(1, Math.ceil(entries.length / 25));
  const currentPage = Math.min(page, pages - 1);
  const games = [
    ...new Set([
      ...(data?.campaigns.map((item) => item.game_name) ?? []),
      ...entries.map((item) => item.game),
      ...(game ? [game] : []),
    ]),
  ].sort();
  function exportJson() {
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(entries, null, 2)], { type: 'application/json' }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = 'drop-history.json';
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap justify-between gap-3">
        <div>
          <h1 className="text-[22px] font-semibold">{t('gui.tabs.history')}</h1>
        </div>
        <div className="flex gap-2">
          <a href={`/api/history/export.csv?${queryString}`} className="button">
            <Icon path={mdiDownload} />
            CSV
          </a>
          <Button disabled={loading || error} onClick={exportJson}>
            JSON
          </Button>
        </div>
      </div>
      <div className="grid gap-4 sm:grid-cols-2 lg:max-w-xl">
        <Field label={t('game')}>
          <select
            className="field"
            value={game}
            onChange={(event) => filter('game', event.target.value)}
          >
            <option value="">{t('all_games')}</option>
            {games.map((name) => (
              <option key={name}>{name}</option>
            ))}
          </select>
        </Field>
        <Field label={t('since_utc')}>
          <Input
            type="date"
            value={since}
            onChange={(event) => filter('since', event.target.value)}
          />
        </Field>
      </div>
      <div className="flex items-center justify-between gap-3">
        <p className="muted">{t('gui.history.filtered_count', { shown: entries.length, total })}</p>
        <Button disabled={!connected || !total} onClick={() => setConfirm(true)}>
          <Icon path={mdiDeleteOutline} />
          {t('clear_history')}
        </Button>
      </div>
      {!confirm && <ActionResult action={action} />}
      {error ? (
        <Notice error>
          {t('history_error')}
          <Button className="ms-auto" onClick={() => setRefresh((value) => value + 1)}>
            {t('retry')}
          </Button>
        </Notice>
      ) : loading ? (
        <Empty title={t('gui.history.loading')} />
      ) : (
        <div className="panel">
          {entries.slice(currentPage * 25, (currentPage + 1) * 25).map((entry) => (
            <div className="row flex-col items-start sm:flex-row sm:items-center" key={entry.id}>
              <div className="min-w-0 flex-1">
                <p className="font-medium">{entry.drop_name}</p>
                <p className="muted">
                  {entry.game} / {entry.campaign}
                </p>
                {entry.benefits.length > 0 && <p className="muted">{entry.benefits.join(', ')}</p>}
              </div>
              <div className="text-[13px] text-muted sm:text-end">
                <p>{dateTime(entry.claimed_at)}</p>
                <p>
                  {entry.required_minutes} {t('gui.history.minutes')}
                </p>
              </div>
            </div>
          ))}
          {!entries.length && <Empty title={t('gui.history.empty')} />}
        </div>
      )}
      {pages > 1 && (
        <div className="flex items-center justify-between">
          <Button
            disabled={currentPage === 0}
            onClick={() => {
              const next = new URLSearchParams(params);
              next.set('page', String(currentPage - 1));
              setParams(next);
            }}
          >
            {t('gui.history.previous')}
          </Button>
          <span className="muted tabular-nums">
            {currentPage + 1} / {pages}
          </span>
          <Button
            disabled={currentPage >= pages - 1}
            onClick={() => {
              const next = new URLSearchParams(params);
              next.set('page', String(currentPage + 1));
              setParams(next);
            }}
          >
            {t('gui.history.next')}
          </Button>
        </div>
      )}
      <details className="border-t border-divider pt-4">
        <summary className="text-[13px] text-muted">{t('summary')}</summary>
        <div className="mt-3 grid gap-5 sm:grid-cols-2">
          {['game', 'month'].map((group) => {
            const counts = new Map<string, number>();
            for (const entry of entries) {
              const key = group === 'game' ? entry.game : entry.claimed_at.slice(0, 7);
              counts.set(key, (counts.get(key) ?? 0) + 1);
            }
            return (
              <div key={group}>
                <h2 className="section-title">{t(`gui.history.by_${group}`)}</h2>
                {[...counts]
                  .sort(([a], [b]) => a.localeCompare(b))
                  .map(([key, count]) => (
                    <p
                      className="flex justify-between border-b border-divider py-2 text-[13px] text-muted"
                      key={key}
                    >
                      <span>{key}</span>
                      <span className="tabular-nums">{count}</span>
                    </p>
                  ))}
              </div>
            );
          })}
        </div>
      </details>
      <Dialog open={confirm} title={t('clear_history')} onClose={() => setConfirm(false)}>
        <ActionResult action={action} />
        <p className="text-muted">
          {t('gui.history.clear_confirm')} {t('history_clear_help')}
        </p>
        <div className="mt-6 flex justify-end gap-2">
          <Button onClick={() => setConfirm(false)}>{t('cancel')}</Button>
          <Button
            primary
            disabled={!connected || action.busy}
            onClick={() =>
              void action.run(async () => {
                await request('/api/history', {}, 'DELETE');
                setConfirm(false);
                setRefresh((value) => value + 1);
              }, t('gui.history.cleared'))
            }
          >
            {t('clear_history')}
          </Button>
        </div>
      </Dialog>
    </div>
  );
}
