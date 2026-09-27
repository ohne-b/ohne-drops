import { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router';
import { mdiFilterVariant, mdiViewList, mdiViewGridOutline } from '@mdi/js';
import { useMiner } from '../lib/state';
import { useT } from '../lib/i18n';
import type { Campaign as CampaignData, Filters, HistoryEntry, Settings } from '../lib/types';
import { request } from '../lib/api';
import {
  Button,
  Check,
  Empty,
  Icon,
  Search,
  useAction,
  ActionResult,
  Notice,
  Art,
} from '../components/ui';
import { Campaign } from '../components/Campaign';
export function matchesCampaign(
  campaign: CampaignData,
  filters: Filters,
  search: string,
  finished = false,
): boolean {
  if (campaign.finished !== finished) return false;
  if (
    search &&
    !`${campaign.name} ${campaign.game_name} ${campaign.drops.map((drop) => drop.name).join(' ')}`
      .toLocaleLowerCase()
      .includes(search.toLocaleLowerCase())
  )
    return false;
  const selected = filters.show_active || filters.show_upcoming || filters.show_expired;
  if (
    !finished &&
    selected &&
    !(
      (filters.show_active && campaign.active) ||
      (filters.show_upcoming && campaign.upcoming) ||
      (filters.show_expired && campaign.expired)
    )
  )
    return false;
  if (!finished && filters.show_only_not_linked && campaign.linked !== false) return false;
  if (
    filters.game_name_search.length &&
    !filters.game_name_search.some(
      (game) => game.toLocaleLowerCase() === campaign.game_name.toLocaleLowerCase(),
    )
  )
    return false;
  if (
    finished ||
    (filters.show_benefit_badge &&
      filters.show_benefit_emote &&
      filters.show_benefit_item &&
      filters.show_benefit_other)
  )
    return true;
  const types: Record<string, boolean> = {
    BADGE: filters.show_benefit_badge,
    EMOTE: filters.show_benefit_emote,
    DIRECT_ENTITLEMENT: filters.show_benefit_item,
    UNKNOWN: filters.show_benefit_other,
  };
  return campaign.drops.some((drop) =>
    drop.benefits.some(
      (benefit) => types[benefit.type.toUpperCase()] ?? filters.show_benefit_other,
    ),
  );
}
export function campaignOrder(a: CampaignData, b: CampaignData): number {
  const rank = (campaign: CampaignData) =>
    campaign.active &&
    campaign.drops.some((drop) => drop.is_claimed || (drop.confirmed_minutes ?? 0) > 0)
      ? 0
      : campaign.active
        ? 1
        : campaign.upcoming
          ? 2
          : 3;
  return rank(a) - rank(b) || a.ends_at.localeCompare(b.ends_at) || a.id.localeCompare(b.id);
}
export default function Campaigns() {
  const { data, connected, autosave } = useMiner();
  const t = useT();
  const [params, setParams] = useSearchParams();
  const [showFilters, setShowFilters] = useState(false);
  const [historicalGames, setHistoricalGames] = useState<string[]>([]);
  const action = useAction();
  const [pendingFilters, setPendingFilters] = useState<Filters | null>(null);
  useEffect(() => setPendingFilters(null), [data?.settings.revision]);
  if (!data) return <Empty title={t('loading')} />;
  const filters = pendingFilters ?? data.settings.inventory_filters;
  const selectedGames = (autosave.draft ?? data.settings).games_to_watch;
  const settingsBusy = action.busy || autosave.busy || autosave.pending;
  const search = params.get('q') ?? '';
  const finished = params.get('tab') === 'finished';
  const total = data.campaigns.filter((campaign) => campaign.finished === finished).length;
  const campaigns = data.campaigns
    .filter((campaign) => matchesCampaign(campaign, filters, search, finished))
    .sort(campaignOrder);
  const update = (patch: Partial<Settings>) =>
    action.run(() => request('/api/settings', { ...patch, revision: data.settings.revision }));
  function changeFilters(next: Filters) {
    setPendingFilters(next);
    void update({ inventory_filters: next }).then((saved) => {
      if (!saved) setPendingFilters(null);
    });
  }
  const games = [
    ...new Set([
      ...data.campaigns.map((campaign) => campaign.game_name),
      ...(finished ? historicalGames : []),
      ...filters.game_name_search,
    ]),
  ].sort();
  const filterOptions: [keyof Omit<Filters, 'game_name_search'>, string][] = [
    ['show_active', 'active'],
    ['show_upcoming', 'upcoming'],
    ['show_expired', 'expired'],
    ['show_only_not_linked', 'not_linked'],
    ['show_benefit_badge', 'badge'],
    ['show_benefit_emote', 'emote'],
    ['show_benefit_item', 'item'],
    ['show_benefit_other', 'other'],
  ];
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-[22px] font-semibold">{t('campaigns')}</h1>
      </div>
      <nav aria-label={t('campaign_views')} className="flex gap-5 border-b border-divider">
        {[false, true].map((value) => (
          <Link
            key={String(value)}
            className={`border-b-2 px-1 pb-3 text-[13px] ${finished === value ? 'border-soft text-text' : 'border-transparent text-muted hover:text-text'}`}
            aria-current={finished === value ? 'page' : undefined}
            to={{
              pathname: '/campaigns',
              search: new URLSearchParams({
                ...(search ? { q: search } : {}),
                ...(value ? { tab: 'finished' } : {}),
              }).toString(),
            }}
          >
            {t(value ? 'finished_campaigns' : 'available_campaigns')}
          </Link>
        ))}
      </nav>
      {data.inventory_status?.available === false && data.inventory_status.checked_at && (
        <Notice error>{t('campaigns_unavailable')}</Notice>
      )}
      <div className="flex flex-wrap gap-3">
        <div className="min-w-48 flex-1">
          <Search
            value={search}
            onChange={(value) =>
              setParams({ ...(finished ? { tab: 'finished' } : {}), q: value }, { replace: true })
            }
            label={t('search_campaigns')}
          />
        </div>
        <Button aria-expanded={showFilters} onClick={() => setShowFilters(!showFilters)}>
          <Icon path={mdiFilterVariant} />
          {t('filters')}
        </Button>
        <Button
          aria-label={t('toggle_view')}
          title={t('toggle_view')}
          disabled={!connected || settingsBusy}
          onClick={() => void update({ inventory_list_view: !data.settings.inventory_list_view })}
        >
          <Icon path={data.settings.inventory_list_view ? mdiViewGridOutline : mdiViewList} />
        </Button>
      </div>
      {showFilters && (
        <div className="panel space-y-4 p-4">
          <div className="grid grid-cols-2 gap-x-6 gap-y-1 md:grid-cols-3">
            {filterOptions
              .filter(() => !finished)
              .map(([key, name]) => (
                <Check
                  key={key}
                  label={t(`gui.inventory.filters.${name}`)}
                  checked={filters[key]}
                  disabled={!connected || settingsBusy}
                  onChange={(value) => changeFilters({ ...filters, [key]: value })}
                />
              ))}
          </div>
          <div className="border-t border-divider pt-3">
            <p className="mb-2 text-[13px] font-medium">{t('game')}</p>
            <div className="grid max-h-48 grid-cols-1 overflow-y-auto sm:grid-cols-2">
              {games.map((game) => (
                <Check
                  key={game}
                  label={game}
                  checked={filters.game_name_search.includes(game)}
                  disabled={!connected || settingsBusy}
                  onChange={(checked) =>
                    changeFilters({
                      ...filters,
                      game_name_search: checked
                        ? [...filters.game_name_search, game]
                        : filters.game_name_search.filter((name) => name !== game),
                    })
                  }
                />
              ))}
            </div>
            <Button
              disabled={!connected || settingsBusy || !filters.game_name_search.length}
              onClick={() => changeFilters({ ...filters, game_name_search: [] })}
            >
              {t('all_games')}
            </Button>
          </div>
        </div>
      )}
      <ActionResult action={action} />
      {autosave.error && (
        <Notice error>
          {t(autosave.error)}{' '}
          <Button disabled={!connected || autosave.busy} onClick={() => void autosave.retry()}>
            {t('retry')}
          </Button>
        </Notice>
      )}
      {(autosave.busy || autosave.pending) && !autosave.error && (
        <p className="muted" role="status">
          {t('saving')}
        </p>
      )}
      <div className="flex items-center justify-between gap-3">
        <p className="muted">{t('campaign_count', { count: campaigns.length, total })}</p>
        {campaigns.length < total && (
          <Button
            disabled={!connected || settingsBusy}
            onClick={() => {
              setParams(finished ? { tab: 'finished' } : {}, { replace: true });
              changeFilters({
                ...filters,
                show_active: true,
                show_upcoming: true,
                show_expired: true,
                show_finished: true,
                show_only_not_linked: false,
                game_name_search: [],
                show_benefit_badge: true,
                show_benefit_emote: true,
                show_benefit_item: true,
                show_benefit_other: true,
              });
            }}
          >
            {t('clear_filters')}
          </Button>
        )}
      </div>
      <div
        className={
          data.settings.inventory_list_view
            ? 'panel overflow-hidden'
            : 'grid items-start gap-4 md:grid-cols-2'
        }
      >
        {campaigns.map((campaign) => (
          <div
            key={campaign.id}
            className={data.settings.inventory_list_view ? '' : 'panel overflow-hidden'}
          >
            <Campaign
              campaign={campaign}
              action={
                !campaign.finished &&
                !campaign.expired && (
                  <Button
                    disabled={!connected || action.busy}
                    title={t('mine_game_help')}
                    aria-label={t(
                      selectedGames.some(
                        (game) => game.toLowerCase() === campaign.game_name.toLowerCase(),
                      )
                        ? 'stop_mining_game'
                        : 'mine_game',
                      { game: campaign.game_name },
                    )}
                    onClick={() => {
                      autosave.change('games_to_watch', (games) =>
                        games.some(
                          (game) => game.toLowerCase() === campaign.game_name.toLowerCase(),
                        )
                          ? games.filter(
                              (game) => game.toLowerCase() !== campaign.game_name.toLowerCase(),
                            )
                          : [...games, campaign.game_name],
                      );
                    }}
                  >
                    {t(
                      selectedGames.some(
                        (game) => game.toLowerCase() === campaign.game_name.toLowerCase(),
                      )
                        ? 'stop_mining'
                        : 'mine',
                    )}
                  </Button>
                )
              }
            />
          </div>
        ))}
      </div>
      {!campaigns.length && (
        <Empty
          title={t(data.campaigns.length ? 'no_matches' : 'gui.inventory.no_campaigns')}
          detail={t('campaign_empty_help')}
        />
      )}
      {finished && (
        <HistoricalClaims
          knownIds={data.campaigns.map((campaign) => campaign.id)}
          search={search}
          games={filters.game_name_search}
          setGames={setHistoricalGames}
        />
      )}
    </div>
  );
}

function HistoricalClaims({
  knownIds,
  search,
  games,
  setGames,
}: {
  knownIds: string[];
  search: string;
  games: string[];
  setGames: (games: string[]) => void;
}) {
  const t = useT();
  const { connected, data } = useMiner();
  const claimed = data?.campaigns.reduce((sum, campaign) => sum + campaign.claimed_drops, 0);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [failed, setFailed] = useState(false);
  const [loading, setLoading] = useState(true);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setFailed(false);
    setLoading(true);
    request<{ entries: HistoryEntry[] }>('/api/history', undefined, 'GET', controller.signal)
      .then((result) => {
        if (!controller.signal.aborted) {
          setEntries(result.entries);
          setGames([...new Set(result.entries.map((entry) => entry.game))]);
        }
      })
      .catch(() => {
        if (!controller.signal.aborted) setFailed(true);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [retry, connected, claimed, setGames]);
  const groups = new Map<string, HistoryEntry[]>();
  for (const entry of entries) {
    if (knownIds.includes(entry.campaign_id)) continue;
    const group = groups.get(entry.campaign_id) ?? [];
    group.push(entry);
    groups.set(entry.campaign_id, group);
  }
  return (
    <section className="space-y-3">
      <h2 className="section-title">{t('historical_claims')}</h2>
      <p className="muted">{t('historical_claims_help')}</p>
      {loading && <p className="muted">{t('loading')}</p>}
      {failed && (
        <Notice error>
          {t('history_error')} <Button onClick={() => setRetry(retry + 1)}>{t('retry')}</Button>
        </Notice>
      )}
      {[...groups.entries()]
        .filter(([, group]) =>
          group.some(
            (entry) =>
              (!games.length ||
                games.some((game) => game.toLowerCase() === entry.game.toLowerCase())) &&
              `${entry.game} ${entry.campaign} ${entry.drop_name}`
                .toLowerCase()
                .includes(search.toLowerCase()),
          ),
        )
        .map(([id, group]) => (
          <details className="panel" key={id}>
            <summary className="cursor-pointer p-4 text-[13px]">
              {group[0]!.campaign} · {group[0]!.game}
              <span className="muted ms-2">{t('completion_unknown')}</span>
            </summary>
            <div className="border-t border-divider px-4">
              {group.map((entry) => (
                <div className="flex items-center gap-3 py-3" key={entry.id}>
                  <Art url={entry.image_url} />
                  <div>
                    <p>{entry.drop_name}</p>
                    <p className="muted">{t('gui.inventory.status.claimed')}</p>
                  </div>
                </div>
              ))}
            </div>
          </details>
        ))}
      <Link className="text-link text-[13px]" to="/history">
        {t('view_claim_history')}
      </Link>
    </section>
  );
}
