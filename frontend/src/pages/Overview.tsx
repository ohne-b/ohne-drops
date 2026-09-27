import { useState } from 'react';
import { Link } from 'react-router';
import { mdiRefresh, mdiPlayOutline } from '@mdi/js';
import { useMiner } from '../lib/state';
import { useT } from '../lib/i18n';
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
  Input,
  Busy,
} from '../components/ui';
export default function Overview() {
  const { data, connected } = useMiner();
  const t = useT();
  const [search, setSearch] = useState('');
  const [channelInput, setChannelInput] = useState('');
  const [manualMinutes, setManualMinutes] = useState('');
  const [enterChannel, setEnterChannel] = useState(false);
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
    <div className="flex flex-col gap-6 xl:flex-1">
      <div className="flex shrink-0 flex-wrap items-start justify-between gap-3">
        <h1 className="text-[22px] font-semibold">{t('overview')}</h1>
        <Button
          disabled={!connected || action.busy}
          onClick={() => void action.run(() => request('/api/reload', {}), t('refresh_requested'))}
        >
          <Icon path={mdiRefresh} />
          {t('refresh')}
        </Button>
      </div>
      <ActionResult action={action} />
      {data.inventory_status?.available === false && !data.inventory_status.recovered && (
        <Notice error>{t('campaigns_unavailable')}</Notice>
      )}
      <section className="panel shrink-0 p-5 md:p-6" aria-labelledby="mining-heading">
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
              </div>
              {progress.confirmed_at && (
                <p className="muted mt-2">
                  {t('last_confirmed', { time: dateTime(progress.confirmed_at) })}
                </p>
              )}
            </div>
          </>
        ) : data.manual_mode.active ? (
          <p className="font-medium">
            {watching
              ? t('watching', { channel: watching.name })
              : t('gui.channels.waiting_for_live', {
                  channel: data.manual_mode.channel_name ?? '',
                })}
          </p>
        ) : (
          <Empty
            title={t('gui.progress.no_drop')}
            detail={t(
              !data.login.user_id
                ? 'connect_help'
                : data.settings.games_to_watch.length
                  ? 'waiting_help'
                  : 'select_games_help',
            )}
          >
            <Link className="button" to={data.login.user_id ? '/campaigns' : '/settings#account'}>
              {data.login.user_id ? t('campaigns') : t('account')}
            </Link>
          </Empty>
        )}
        {(data.manual_mode.active || data.manual_mode.pending_channel) && (
          <Button
            className="mt-4"
            disabled={!connected || action.busy}
            onClick={() => void action.run(() => request('/api/mode/exit-manual', {}))}
          >
            {t('gui.progress.return_to_auto')}
          </Button>
        )}
        {data.manual_mode.expires_at && (
          <p className="muted mt-2">
            {t('gui.channels.auto_at', { time: dateTime(data.manual_mode.expires_at) })}
          </p>
        )}
      </section>
      {/* Long lists must not contribute to the page's intrinsic minimum height. */}
      <div className="grid gap-6 xl:min-h-[240px] xl:flex-1 xl:grid-cols-2 xl:[contain:size]">
        <section className="panel order-2 flex min-h-0 flex-col overflow-hidden xl:order-1">
          <div className="shrink-0 space-y-4 border-b border-divider p-4">
            <div className="flex items-center justify-between">
              <h2 id="channels-heading" className="section-title">
                {t('gui.channels.name')}
              </h2>
              <div className="flex items-center gap-3">
                <span className="muted tabular-nums">{channels.length}</span>
                <Button aria-expanded={enterChannel} onClick={() => setEnterChannel(!enterChannel)}>
                  {t('gui.channels.mine_channel')}
                </Button>
              </div>
            </div>
            <Search value={search} onChange={setSearch} label={t('search_channels')} />
          </div>
          <div
            className="min-h-0 max-h-[440px] overflow-y-auto focus-visible:bg-field xl:max-h-none xl:flex-1"
            role="region"
            aria-labelledby="channels-heading"
            tabIndex={0}
          >
            {(enterChannel || data.manual_mode.pending_channel || data.manual_mode.error) && (
              <div className="space-y-3 border-b border-divider p-4">
                {enterChannel && (
                  <form
                    className="space-y-2"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void action.run(() =>
                        request('/api/channels/select', {
                          channel: channelInput,
                          duration_minutes: manualMinutes ? Number(manualMinutes) : null,
                        }),
                      );
                    }}
                  >
                    <div className="flex gap-2">
                      <Input
                        id="manual-channel"
                        aria-label={t('gui.channels.channel_input')}
                        placeholder={t('gui.channels.channel_input')}
                        value={channelInput}
                        maxLength={256}
                        onChange={(event) => setChannelInput(event.target.value)}
                      />
                      <Button
                        type="submit"
                        disabled={
                          !connected ||
                          !data.login.user_id ||
                          !channelInput.trim() ||
                          action.busy ||
                          !!data.manual_mode.pending_channel
                        }
                      >
                        {t('mine')}
                      </Button>
                    </div>
                    <Input
                      type="number"
                      min={1}
                      max={1440}
                      step={1}
                      aria-label={t('gui.channels.manual_timer')}
                      placeholder={t('gui.channels.manual_timer')}
                      value={manualMinutes}
                      onChange={(event) => setManualMinutes(event.target.value)}
                    />
                  </form>
                )}
                {data.manual_mode.pending_channel && (
                  <Busy
                    label={t('gui.channels.looking_up', {
                      channel: data.manual_mode.pending_channel,
                    })}
                  />
                )}
                {data.manual_mode.error && <Notice error>{data.manual_mode.error}</Notice>}
              </div>
            )}
            {channels.map((channel) => (
              <div className="row flex-wrap sm:flex-nowrap" key={channel.id}>
                <Art url={channel.game_icon} />
                <div className="min-w-0 flex-1">
                  <a
                    className="font-medium hover:underline"
                    href={`https://www.twitch.tv/${encodeURIComponent(channel.login || channel.name)}`}
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
        <section className="panel order-1 flex min-h-0 flex-col overflow-hidden xl:order-2">
          <div className="flex shrink-0 items-center justify-between border-b border-divider p-4">
            <h2 id="up-next-heading" className="section-title">
              {t('up_next')}
            </h2>
            <Link className="text-[13px] text-muted hover:text-text" to="/settings#mining">
              {t('edit')}
            </Link>
          </div>
          <div
            className="min-h-0 max-h-[440px] overflow-y-auto focus-visible:bg-field xl:max-h-none xl:flex-1"
            role="region"
            aria-labelledby="up-next-heading"
            tabIndex={0}
          >
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
                        <li className="flex items-start gap-3" key={`${drop.name}/${position}`}>
                          <Art url={drop.image_url} className="size-9 [&_img]:object-contain" />
                          <div className="min-w-0 flex-1">
                            <p>{drop.name}</p>
                            {drop.benefits.some((benefit) => benefit !== drop.name) && (
                              <p className="mt-0.5 text-xs">{drop.benefits.join(', ')}</p>
                            )}
                          </div>
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </div>
            ))}
            {!data.wanted_items.length && <Empty title={t('gui.wanted.none')} />}
          </div>
        </section>
      </div>
    </div>
  );
}
