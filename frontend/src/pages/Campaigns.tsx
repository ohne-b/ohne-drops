import { Icon } from '@mdi/react';
import { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router';
import {
  mdiFilterOutline,
  mdiViewList,
  mdiViewGridOutline,
  mdiSortAscending,
  mdiFilterOffOutline,
  mdiPlayCircleOutline,
  mdiStopCircleOutline,
} from '@mdi/js';
import { useMiner } from '../lib/state';
import { useT } from '../lib/i18n';
import type { Campaign as CampaignData, Filters, Settings } from '../lib/types';
import { request } from '../lib/api';
import {
  Button,
  IconButton,
  Check,
  Empty,
  Search,
  useAction,
  ActionResult,
  Notice,
} from '../components/ui';
import { Campaign } from '../components/Campaign';
import History, { useHistory, groupHistory, matchesHistory, historyOrder } from './History';
export function matchesCampaign(campaign: CampaignData, filters: Filters, search: string): boolean {
  if (campaign.finished) return false;
  if (
    search &&
    !`${campaign.name} ${campaign.game_name} ${campaign.drops.map((drop) => drop.name).join(' ')}`
      .toLocaleLowerCase()
      .includes(search.toLocaleLowerCase())
  )
    return false;
  const selected = filters.show_active || filters.show_upcoming || filters.show_expired;
  if (
    selected &&
    !(
      (filters.show_active && campaign.active) ||
      (filters.show_upcoming && campaign.upcoming) ||
      (filters.show_expired && campaign.expired)
    )
  )
    return false;
  if (filters.show_only_not_linked && campaign.linked !== false) return false;
  if (
    filters.game_name_search.length &&
    !filters.game_name_search.some(
      (game) => game.toLocaleLowerCase() === campaign.game_name.toLocaleLowerCase(),
    )
  )
    return false;
  if (
    filters.show_benefit_badge &&
    filters.show_benefit_emote &&
    filters.show_benefit_item &&
    filters.show_benefit_other
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
const campaignSorts = ['default', 'newest', 'ending', 'drops', 'name'] as const;
export type CampaignSort = (typeof campaignSorts)[number];
export function campaignOrder(
  a: CampaignData,
  b: CampaignData,
  sort: CampaignSort = 'default',
): number {
  const difference =
    sort === 'newest'
      ? Date.parse(b.starts_at) - Date.parse(a.starts_at)
      : sort === 'ending'
        ? Date.parse(a.ends_at) - Date.parse(b.ends_at)
        : sort === 'drops'
          ? b.total_drops - a.total_drops
          : sort === 'name'
            ? a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true })
            : 0;
  if (difference) return difference;
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
  const historyTab = ['history', 'finished'].includes(params.get('tab') ?? '');
  const history = useHistory(historyTab);
  const action = useAction();
  const [pendingFilters, setPendingFilters] = useState<Filters | null>(null);
  useEffect(() => setPendingFilters(null), [data?.settings.revision]);
  if (!data) return <Empty title={t('loading')} />;
  const filters = pendingFilters ?? data.settings.inventory_filters;
  const selectedGames = (autosave.draft ?? data.settings).games_to_watch;
  const settingsBusy = action.busy || autosave.busy || autosave.pending;
  const search = params.get('q') ?? '';

  const sort = campaignSorts.find((value) => value === params.get('sort')) ?? 'default';
  const groups = groupHistory(history.entries, data.campaigns);
  const total = historyTab
    ? groups.length
    : data.campaigns.filter((campaign) => !campaign.finished).length;
  const campaigns = data.campaigns
    .filter((campaign) => matchesCampaign(campaign, filters, search))
    .sort((a, b) => campaignOrder(a, b, sort));
  const historical = groups
    .filter((group) => matchesHistory(group, filters.game_name_search, search))
    .sort((a, b) => historyOrder(a, b, sort));
  const page = Math.min(
    Math.max(0, Math.trunc(Number(params.get('page'))) || 0),
    Math.max(0, Math.ceil(historical.length / 25) - 1),
  );
  function setQuery(key: string, value: string) {
    const next = new URLSearchParams(params);
    if (key !== 'page') next.delete('page');
    value ? next.set(key, value) : next.delete(key);
    setParams(next, { replace: true });
  }
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
      ...(historyTab ? history.entries.map((entry) => entry.game) : []),
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
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-[22px] font-semibold">{t('campaigns')}</h1>
        <p className="muted ms-auto text-right">
          {t('campaign_count', { count: historyTab ? historical.length : campaigns.length, total })}
        </p>
      </header>
      <nav aria-label={t('campaign_views')} className="flex gap-5 border-b border-divider">
        {[false, true].map((value) => (
          <Link
            key={String(value)}
            className={`border-b-2 px-1 pb-3 text-[13px] ${historyTab === value ? 'border-soft text-text' : 'border-transparent text-muted hover:text-text'}`}
            aria-current={historyTab === value ? 'page' : undefined}
            to={{
              pathname: '/campaigns',
              search: new URLSearchParams({
                ...(search ? { q: search } : {}),
                ...(sort !== 'default' ? { sort } : {}),
                ...(value ? { tab: 'history' } : {}),
              }).toString(),
            }}
          >
            {t(value ? 'gui.tabs.history' : 'available_campaigns')}
          </Link>
        ))}
      </nav>
      {!historyTab &&
        data.inventory_status?.available === false &&
        data.inventory_status.checked_at && <Notice error>{t('campaigns_unavailable')}</Notice>}
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <Search
            value={search}
            onChange={(value) => setQuery('q', value)}
            label={t('search_campaigns')}
          />
        </div>
        <IconButton
          path={mdiFilterOutline}
          label={t('filters')}
          aria-expanded={showFilters}
          onClick={() => setShowFilters(!showFilters)}
        />
        <div
          className="icon-button"
          title={`${t(historyTab ? 'sort_history' : 'sort_campaigns')}: ${t(`sort_${sort}`)}`}
        >
          <Icon className="mdi-icon pointer-events-none" path={mdiSortAscending} />
          <select
            className="icon-select absolute inset-0 size-full cursor-pointer opacity-0"
            aria-label={t(historyTab ? 'sort_history' : 'sort_campaigns')}
            value={sort}
            onChange={(event) =>
              setQuery('sort', event.target.value === 'default' ? '' : event.target.value)
            }
          >
            {campaignSorts.map((value) => (
              <option key={value} value={value}>
                {t(`sort_${value}`)}
              </option>
            ))}
          </select>
        </div>
        <IconButton
          path={data.settings.inventory_list_view ? mdiViewGridOutline : mdiViewList}
          label={t('toggle_view')}
          disabled={!connected || settingsBusy}
          onClick={() => void update({ inventory_list_view: !data.settings.inventory_list_view })}
        />
      </div>
      {showFilters && (
        <div className="panel space-y-4 p-4">
          {!historyTab && (
            <div className="grid grid-cols-2 gap-x-6 gap-y-1 md:grid-cols-3">
              {filterOptions.map(([key, name]) => (
                <Check
                  key={key}
                  label={t(`gui.inventory.filters.${name}`)}
                  checked={filters[key]}
                  disabled={!connected || settingsBusy}
                  onChange={(value) => changeFilters({ ...filters, [key]: value })}
                />
              ))}
            </div>
          )}
          <div className={historyTab ? '' : 'border-t border-divider pt-3'}>
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
            <div className="mt-3 flex flex-wrap gap-3">
              <Button
                disabled={!connected || settingsBusy || !filters.game_name_search.length}
                onClick={() => changeFilters({ ...filters, game_name_search: [] })}
              >
                {t('all_games')}
              </Button>
              <IconButton
                path={mdiFilterOffOutline}
                label={t('clear_filters')}
                disabled={!connected || settingsBusy}
                onClick={() => {
                  setQuery('q', '');
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
              />
            </div>
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
      {historyTab ? (
        <>
          {history.error && (
            <Notice error>
              {t('history_error')} <Button onClick={history.retry}>{t('retry')}</Button>
            </Notice>
          )}
          {history.loading && (
            <p role="status" className="muted">
              {t('gui.history.loading')}
            </p>
          )}
          <History
            groups={historical.slice(page * 25, (page + 1) * 25)}
            list={data.settings.inventory_list_view}
          />
          {!historical.length && !history.loading && !history.error && (
            <Empty title={t(history.entries.length ? 'no_matches' : 'history_empty')} />
          )}
          {historical.length > 25 && (
            <nav className="flex items-center justify-end gap-3" aria-label={t('history_pages')}>
              <Button disabled={page === 0} onClick={() => setQuery('page', String(page - 1))}>
                {t('gui.history.previous')}
              </Button>
              <span className="muted">
                {page + 1} / {Math.ceil(historical.length / 25)}
              </span>
              <Button
                disabled={(page + 1) * 25 >= historical.length}
                onClick={() => setQuery('page', String(page + 1))}
              >
                {t('gui.history.next')}
              </Button>
            </nav>
          )}
        </>
      ) : (
        <>
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
                      <IconButton
                        path={
                          selectedGames.some(
                            (game) => game.toLowerCase() === campaign.game_name.toLowerCase(),
                          )
                            ? mdiStopCircleOutline
                            : mdiPlayCircleOutline
                        }
                        disabled={!connected || action.busy}
                        label={t(
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
                      />
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
        </>
      )}
    </div>
  );
}
