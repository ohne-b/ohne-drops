import { useEffect, useRef, useState } from 'react';
import { Icon } from '@mdi/react';
import { mdiDockRight } from '@mdi/js';
import { useMiner } from '../../app/MinerProvider';
import { useT } from '../../shared/lib/i18n';
import { request } from '../../shared/lib/api';
import type { Campaign, HistoryEntry } from '../../shared/lib/types';
import type { CampaignSort } from './Campaigns';
import { Art, dateTime } from '../../shared/ui/index';

export function useHistory(active: boolean) {
  const { connected, data, historyRevision } = useMiner();
  const [loaded, setLoaded] = useState<{
    entries: HistoryEntry[];
    instance: string;
    clear_revision: number;
  } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [retry, setRetry] = useState(0);
  const claimed = data?.campaigns.reduce((sum, campaign) => sum + campaign.claimed_drops, 0);
  const checked = data?.inventory_status?.checked_at;
  const user = data?.login.user_id;
  const expected = useRef({
    instance: data?.instance,
    revision: data?.history_revision ?? 0,
    clear: historyRevision,
  });
  expected.current = {
    instance: data?.instance,
    revision: data?.history_revision ?? 0,
    clear: historyRevision,
  };
  const entries =
    loaded?.instance === data?.instance && loaded?.clear_revision === historyRevision
      ? loaded.entries
      : [];
  useEffect(() => {
    if (!active || !connected) return;
    const controller = new AbortController();
    setLoading(true);
    setError(false);
    request<{
      entries: HistoryEntry[];
      instance: string;
      revision: number;
      clear_revision: number;
    }>('/api/history', undefined, 'GET', controller.signal)
      .then((result) => {
        if (
          !controller.signal.aborted &&
          result.instance === expected.current.instance &&
          result.revision >= expected.current.revision &&
          result.clear_revision === expected.current.clear
        )
          setLoaded(result);
      })
      .catch(() => {
        if (!controller.signal.aborted) setError(true);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [
    active,
    connected,
    claimed,
    checked,
    user,
    historyRevision,
    data?.history_revision,
    data?.instance,
    retry,
  ]);
  return { entries, loading, error, retry: () => setRetry((value) => value + 1) };
}

export interface HistoryCampaign {
  id: string;
  name: string;
  game: string;
  entries: HistoryEntry[];
  metadata?: Campaign;
}
export function groupHistory(entries: HistoryEntry[], campaigns: Campaign[]): HistoryCampaign[] {
  const metadata = new Map(campaigns.map((campaign) => [campaign.id, campaign]));
  const groups = new Map<string, HistoryCampaign>();
  for (const entry of entries) {
    const id = entry.campaign_id || JSON.stringify([entry.game, entry.campaign]);
    let group = groups.get(id);
    if (!group) {
      group = {
        id,
        name: entry.campaign,
        game: entry.game,
        entries: [],
        metadata: metadata.get(entry.campaign_id),
      };
      groups.set(id, group);
    }
    group.entries.push(entry);
  }
  for (const group of groups.values()) {
    group.entries.sort(
      (a, b) => Date.parse(b.claimed_at) - Date.parse(a.claimed_at) || a.id.localeCompare(b.id),
    );
  }
  return [...groups.values()];
}
export function matchesHistory(group: HistoryCampaign, games: string[], search: string): boolean {
  return (
    (!games.length ||
      games.some((game) => game.toLocaleLowerCase() === group.game.toLocaleLowerCase())) &&
    `${group.name} ${group.game} ${group.entries.map((entry) => `${entry.drop_name} ${entry.benefits.join(' ')}`).join(' ')}`
      .toLocaleLowerCase()
      .includes(search.toLocaleLowerCase())
  );
}
export function historyOrder(a: HistoryCampaign, b: HistoryCampaign, sort: CampaignSort): number {
  const difference =
    sort === 'newest' || sort === 'ending'
      ? a.metadata && b.metadata
        ? sort === 'newest'
          ? Date.parse(b.metadata.starts_at) - Date.parse(a.metadata.starts_at)
          : Date.parse(a.metadata.ends_at) - Date.parse(b.metadata.ends_at)
        : Number(!!b.metadata) - Number(!!a.metadata)
      : sort === 'drops'
        ? b.entries.length - a.entries.length
        : sort === 'name'
          ? a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true })
          : 0;
  return (
    difference ||
    Date.parse(b.entries[0]!.claimed_at) - Date.parse(a.entries[0]!.claimed_at) ||
    a.id.localeCompare(b.id)
  );
}

export default function History({
  groups,
  list,
  onOpen,
  selected,
}: {
  groups: HistoryCampaign[];
  list: boolean;
  onOpen: (id: string) => void;
  selected: string | null;
}) {
  const t = useT();
  return (
    <div
      className={
        list
          ? 'panel overflow-hidden divide-y divide-divider'
          : 'grid items-start gap-4 md:grid-cols-2'
      }
    >
      {groups.map((group) => (
        <article
          key={group.id}
          className={`campaign-summary ${list ? '' : 'panel'} ${selected === group.id ? 'selected' : ''}`}
        >
          <button
            type="button"
            id={`campaign-open-${group.id}`}
            className="campaign-open"
            onClick={() => onOpen(group.id)}
            aria-label={t('inspect_campaign', { campaign: group.name })}
            title={t('campaign_details')}
            aria-current={selected === group.id ? 'true' : undefined}
            aria-controls={selected === group.id ? 'campaign-details' : undefined}
          >
            <Art url={group.metadata?.game_box_art_url || group.entries[0]?.image_url} />
            <span className="min-w-0 flex-1 text-start">
              <span className="campaign-title block font-medium">{group.name}</span>
              <span className="muted block">{group.game}</span>
              <span className="muted block text-xs mt-1">
                {dateTime(group.entries[0]?.claimed_at ?? '')}
              </span>
            </span>
            <span className="muted">{t('recorded_claims', { count: group.entries.length })}</span>
            <span className="campaign-detail-icon" aria-hidden="true">
              <Icon path={mdiDockRight} className="mdi-icon" />
            </span>
          </button>
        </article>
      ))}
    </div>
  );
}
