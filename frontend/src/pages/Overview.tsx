import { useState } from 'react';
import { Link } from 'react-router';
import { mdiArrowRight, mdiRefresh, mdiPlayOutline } from '@mdi/js';
import { useMiner } from '../lib/state';
import { useT, plainText } from '../lib/i18n';
import { request, safeUrl } from '../lib/api';
import {
  Art,
  Button,
  Empty,
  Icon,
  ProgressBar,
  Search,
  Notice,
  useAction,
  ActionResult,
  dateTime,
} from '../components/ui';
export default function Overview() {
  const { data, connected } = useMiner();
  const t = useT();
  const [search, setSearch] = useState('');
  const action = useAction();
  if (!data) return <Empty title={t('loading')} />;
  const progress = data.current_drop;
  const campaign = data.campaigns.find((item) => item.id === progress?.campaign_id);
  const watching = data.channels.find((channel) => channel.watching);
  const channels = data.channels
    .filter((channel) =>
      `${channel.name} ${channel.game ?? ''}`
        .toLocaleLowerCase()
        .includes(search.toLocaleLowerCase()),
    )
    .sort(
      (a, b) => Number(b.watching) - Number(a.watching) || (b.viewers ?? -1) - (a.viewers ?? -1),
    );
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-[22px] font-semibold">{t('overview')}</h1>
          <p className="mt-1 text-muted">{plainText(data.status)}</p>
        </div>
        <Button
          disabled={!connected || action.busy}
          onClick={() => void action.run(() => request('/api/reload', {}), t('refresh_requested'))}
        >
          <Icon path={mdiRefresh} />
          {t('refresh')}
        </Button>
      </div>
      <ActionResult action={action} />
      {data.inventory_status?.available === false && (
        <Notice error>{t('campaigns_unavailable')}</Notice>
      )}
      <section className="panel p-5 md:p-6" aria-labelledby="mining-heading">
        <div className="mb-5 flex items-center justify-between gap-3">
          <h2 id="mining-heading" className="section-title">
            {t('mining')}
          </h2>
          <span className="muted">{data.manual_mode.active ? t('manual') : t('automatic')}</span>
        </div>
        {progress ? (
          <>
            <div className="flex items-start gap-4">
              <Art
                url={
                  campaign?.drops.find((drop) => drop.id === progress.drop_id)?.benefits[0]
                    ?.image_url ?? campaign?.game_box_art_url
                }
                className="size-16"
              />
              <div className="min-w-0 flex-1">
                <p className="text-lg font-semibold">{progress.drop_name}</p>
                <p className="muted mt-1">
                  {progress.game_name} / {progress.campaign_name}
                </p>
                {watching && (
                  <p className="muted mt-1">{t('watching', { channel: watching.name })}</p>
                )}
              </div>
            </div>
            <div className="mt-6">
              <ProgressBar
                current={progress.confirmed_minutes ?? 0}
                total={progress.required_minutes}
                label={progress.drop_name}
              />
              <div className="mt-2 flex flex-wrap justify-between gap-2 text-[13px]">
                <span className="tabular-nums">
                  {t('minutes_progress', {
                    current: progress.confirmed_minutes ?? 0,
                    total: progress.required_minutes,
                  })}
                </span>
                <span className="text-muted">{t('confirmed_progress')}</span>
              </div>
              {progress.confirmed_at && (
                <p className="muted mt-2">
                  {t('last_confirmed', { time: dateTime(progress.confirmed_at) })}
                </p>
              )}
            </div>
          </>
        ) : (
          <Empty
            title={t('gui.progress.no_drop')}
            detail={t(data.login.user_id ? 'waiting_help' : 'connect_help')}
          >
            <Link
              className="button"
              to={data.login.user_id ? '/settings#mining' : '/settings#account'}
            >
              {data.login.user_id ? t('edit_priorities') : t('account')}
            </Link>
          </Empty>
        )}
        {data.manual_mode.active && (
          <Button
            className="mt-4"
            disabled={!connected || action.busy}
            onClick={() => void action.run(() => request('/api/mode/exit-manual', {}))}
          >
            {t('gui.progress.return_to_auto')}
          </Button>
        )}
      </section>
      <div className="grid items-start gap-6 xl:grid-cols-2">
        <section className="panel order-2 xl:order-1">
          <div className="space-y-4 border-b border-divider p-4">
            <div className="flex items-center justify-between">
              <h2 className="section-title">{t('gui.channels.name')}</h2>
              <span className="muted tabular-nums">{channels.length}</span>
            </div>
            <Search value={search} onChange={setSearch} label={t('search_channels')} />
          </div>
          <div className="max-h-[440px] overflow-y-auto">
            {channels.map((channel) => (
              <div className="row flex-wrap sm:flex-nowrap" key={channel.id}>
                <Art url={channel.game_icon} />
                <div className="min-w-0 flex-1">
                  <a
                    className="font-medium hover:underline"
                    href={`https://www.twitch.tv/${encodeURIComponent(channel.name)}`}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {channel.name}
                  </a>
                  <p className="muted truncate">
                    {channel.game ?? t('unknown_game')} · {channel.viewers?.toLocaleString() ?? '—'}{' '}
                    {t('gui.channels.viewers')}
                  </p>
                </div>
                {channel.watching ? (
                  <span className="muted">{t('watching_now')}</span>
                ) : (
                  <Button
                    aria-label={t('watch_channel', { channel: channel.name })}
                    disabled={!connected || !channel.online || action.busy}
                    onClick={() =>
                      void action.run(() =>
                        request('/api/channels/select', { channel_id: channel.id }),
                      )
                    }
                  >
                    <Icon path={mdiPlayOutline} />
                    {t('watch')}
                  </Button>
                )}
              </div>
            ))}
            {!channels.length && (
              <Empty title={t(search ? 'no_matches' : 'gui.channels.no_channels')} />
            )}
          </div>
        </section>
        <section className="panel order-1 xl:order-2">
          <div className="flex items-center justify-between border-b border-divider p-4">
            <h2 className="section-title">{t('up_next')}</h2>
            <Link className="text-[13px] text-muted hover:text-text" to="/settings#mining">
              {t('edit')}
            </Link>
          </div>
          {data.wanted_items.map((game, index) => (
            <div key={game.game_name} className="border-b border-divider p-4 last:border-0">
              <div className="flex items-center gap-3">
                <span className="w-4 text-[13px] tabular-nums text-muted">{index + 1}</span>
                <Art url={game.game_icon} className="size-8" />
                <p className="font-medium">{game.game_name}</p>
              </div>
              {game.campaigns.map((item) => (
                <div className="mt-3 ps-7 text-[13px]" key={item.id}>
                  {safeUrl(item.url) ? (
                    <a
                      className="text-link"
                      href={safeUrl(item.url)}
                      target="_blank"
                      rel="noreferrer"
                    >
                      {item.name}
                    </a>
                  ) : (
                    <p className="text-soft">{item.name}</p>
                  )}
                  <ul className="mt-2 space-y-2 text-muted">
                    {item.drops.map((drop, position) => (
                      <li key={`${drop.name}/${position}`}>
                        <p>{drop.name}</p>
                        {drop.benefits.some((benefit) => benefit !== drop.name) && (
                          <p className="mt-0.5 text-xs">{drop.benefits.join(', ')}</p>
                        )}
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
            </div>
          ))}
          {!data.wanted_items.length && <Empty title={t('gui.wanted.none')} />}
        </section>
      </div>
      <section>
        <div className="mb-3 flex items-center justify-between">
          <h2 className="section-title">{t('recent_activity')}</h2>
          <Link
            className="flex items-center gap-1 text-[13px] text-muted hover:text-text"
            to="/activity"
          >
            {t('view_all')}
            <Icon path={mdiArrowRight} />
          </Link>
        </div>
        <div className="divide-y divide-divider">
          {data.console
            .slice(-3)
            .reverse()
            .map((line, index) => (
              <p className="break-words py-2 text-[13px] text-muted" key={index}>
                {plainText(line)}
              </p>
            ))}
        </div>
      </section>
    </div>
  );
}
