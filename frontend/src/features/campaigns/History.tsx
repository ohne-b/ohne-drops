import { useEffect, useState } from 'react';
import { Icon } from '@mdi/react';
import { mdiChevronDown } from '@mdi/js';
import { useMiner } from '../../app/MinerProvider';
import { useT } from '../../shared/lib/i18n';
import { request } from '../../shared/lib/api';
import type { Campaign, HistoryEntry } from '../../shared/lib/types';
import type { CampaignSort } from './Campaigns';
import { Art, dateTime } from '../../shared/ui/index';

export function useHistory(active: boolean) {
  const { connected, data, historyRevision } = useMiner();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [retry, setRetry] = useState(0);
  const claimed = data?.campaigns.reduce((sum, campaign) => sum + campaign.claimed_drops, 0);
  const checked = data?.inventory_status?.checked_at;
  const user = data?.login.user_id;
  useEffect(() => setEntries([]), [historyRevision]);
  useEffect(() => {
    if (!active || !connected) return;
    const controller = new AbortController();
    setLoading(true);
    setError(false);
    request<{ entries: HistoryEntry[] }>('/api/history', undefined, 'GET', controller.signal)
      .then((result) => {
        if (!controller.signal.aborted) setEntries(result.entries);
      })
      .catch(() => {
        if (!controller.signal.aborted) setError(true);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [active, connected, claimed, checked, user, historyRevision, retry]);
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

export default function History({ groups, list }: { groups: HistoryCampaign[]; list: boolean }) {
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
        <details key={group.id} className={`group ${list ? '' : 'panel overflow-hidden'}`}>
          <summary className="flex list-none items-center gap-3 p-4 hover:bg-field">
            <Art url={group.metadata?.game_box_art_url || group.entries[0]?.image_url} />
            <div className="min-w-0 flex-1">
              <p className="font-medium">{group.name}</p>
              <p className="muted">{group.game}</p>
            </div>
            <p className="muted">{t('recorded_claims', { count: group.entries.length })}</p>
            <Icon
              path={mdiChevronDown}
              className="mdi-icon text-muted transition-transform group-open:rotate-180"
            />
          </summary>
          <div className="divide-y divide-divider border-t border-divider bg-canvas/40 px-4 md:px-6">
            {group.entries.map((entry) => (
              <div key={entry.id} className="flex items-start gap-3 py-4">
                <Art
                  url={
                    entry.image_url ||
                    group.metadata?.drops.find((drop) => drop.id === entry.id)?.benefits[0]
                      ?.image_url
                  }
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
                    <p className="font-medium">{entry.drop_name}</p>
                    <p className="muted">
                      {t(entry.claimed_at_is_observed ? 'first_observed' : 'claimed_at', {
                        time: dateTime(entry.claimed_at),
                      })}
                    </p>
                  </div>
                  <p className="muted mt-1">{entry.benefits.join(', ')}</p>
                  <p className="muted mt-1">
                    {t('watch_minutes', { count: entry.required_minutes })}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </details>
      ))}
    </div>
  );
}
