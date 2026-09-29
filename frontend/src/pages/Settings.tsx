import { Icon } from '@mdi/react';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { mdiPlus, mdiOpenInNew, mdiLogout, mdiContentCopy, mdiReload } from '@mdi/js';
import type { AuthStatus, ReleaseInfo, Result, Settings as SettingsData } from '../lib/types';
import { request, safeUrl } from '../lib/api';
import { useMiner } from '../lib/state';
import { GamePriorities } from '../components/GamePriorities';
import { InventoryRefreshButton } from '../components/InventoryRefreshButton';
import { plainText, useT } from '../lib/i18n';
import {
  ActionResult,
  Button,
  Check,
  Dialog,
  Empty,
  Field,
  IconButton,
  Input,
  Notice,
  Search,
  useAction,
} from '../components/ui';
function Section({
  id,
  title,
  help,
  children,
}: {
  id: string;
  title: ReactNode;
  help?: string;
  children: ReactNode;
}) {
  return (
    <section id={id} className="scroll-mt-6 border-b border-divider pb-8 last:border-0">
      <h2 className="mb-1 text-base font-semibold">{title}</h2>
      {help && <p className="mb-5 max-w-2xl text-[13px] leading-relaxed text-muted">{help}</p>}
      <div className="mt-5 space-y-4">{children}</div>
    </section>
  );
}
function Access({ initial, disabled }: { initial: AuthStatus; disabled: boolean }) {
  const t = useT();
  const [auth, setAuth] = useState(initial);
  useEffect(() => setAuth(initial), [initial]);
  const [current, setCurrent] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [disableDialog, setDisableDialog] = useState(false);
  const action = useAction();
  async function save(kind: 'enable' | 'change' | 'disable') {
    await action.run(async () => {
      const result = await request<AuthStatus>('/api/auth/settings', {
        action: kind,
        current_password: current,
        password,
        confirm_password: confirm,
      });
      setAuth({ ...auth, enabled: result.enabled });
      setCurrent('');
      setPassword('');
      setConfirm('');
      setDisableDialog(false);
      window.dispatchEvent(new Event('auth-updated'));
    });
  }
  return (
    <Section id="access" title={t('gui.auth.title')} help={t('gui.auth.help')}>
      <p className="muted">{t(auth.enabled ? 'gui.auth.enabled' : 'gui.auth.disabled')}</p>
      {disabled && <Notice>{t('save_first')}</Notice>}
      <form
        className="max-w-md space-y-4"
        onSubmit={(event) => {
          event.preventDefault();
          void save(auth.enabled ? 'change' : 'enable');
        }}
      >
        {auth.enabled && (
          <Field label={t('gui.auth.current_password')}>
            <Input
              type="password"
              autoComplete="current-password"
              value={current}
              onChange={(event) => setCurrent(event.target.value)}
              required
              maxLength={1024}
            />
          </Field>
        )}
        <Field label={t('gui.auth.new_password')}>
          <Input
            type="password"
            autoComplete="new-password"
            minLength={8}
            maxLength={1024}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            required
          />
        </Field>
        <Field label={t('gui.auth.confirm_password')}>
          <Input
            type="password"
            autoComplete="new-password"
            maxLength={1024}
            value={confirm}
            onChange={(event) => setConfirm(event.target.value)}
            required
          />
        </Field>
        <ActionResult action={action} />
        <div className="flex flex-wrap gap-2">
          <Button type="submit" disabled={disabled || action.busy} primary>
            {t(auth.enabled ? 'gui.auth.change' : 'gui.auth.enable')}
          </Button>
          {auth.enabled && (
            <Button
              disabled={disabled || action.busy || !current}
              onClick={() => setDisableDialog(true)}
            >
              {t('disable_protection')}
            </Button>
          )}
        </div>
      </form>
      <Dialog
        open={disableDialog}
        title={t('disable_protection')}
        onClose={() => setDisableDialog(false)}
      >
        <p className="text-muted">{t('disable_help')}</p>
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={() => setDisableDialog(false)}>{t('cancel')}</Button>
          <Button primary disabled={action.busy} onClick={() => void save('disable')}>
            {t('disable_protection')}
          </Button>
        </div>
        <ActionResult action={action} />
      </Dialog>
    </Section>
  );
}
function ReleaseNotice({ disabled }: { disabled: boolean }) {
  const t = useT();
  const [release, setRelease] = useState<ReleaseInfo | null>(null);
  const [busy, setBusy] = useState(true);
  const controller = useRef<AbortController | null>(null);
  async function check() {
    controller.current?.abort();
    const next = new AbortController();
    controller.current = next;
    setBusy(true);
    try {
      const result = await request<ReleaseInfo>('/api/version', undefined, 'GET', next.signal);
      if (!next.signal.aborted) setRelease(result);
    } catch {
      if (!next.signal.aborted)
        setRelease((previous) => previous && { ...previous, check_succeeded: false });
    } finally {
      if (!next.signal.aborted) setBusy(false);
    }
  }
  useEffect(() => {
    void check();
    return () => controller.current?.abort();
  }, []);
  const available = release?.check_succeeded && release.update_available;
  const releaseUrl = available ? safeUrl(release.download_url) : undefined;
  return (
    <div className="space-y-3 text-[13px]">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-muted">Drops Miner{release && ` · ${release.current_version}`}</p>
        <Button disabled={disabled || busy} onClick={() => void check()}>
          {t(busy ? 'checking_updates' : 'check_updates')}
        </Button>
      </div>
      <div>
        {busy ? (
          <p role="status" className="text-muted">
            {t('checking_updates')}
          </p>
        ) : !release?.check_succeeded ? (
          <p role="status" className="text-muted">
            {t('update_check_failed')}
          </p>
        ) : available ? (
          <Notice>
            <div>
              <p>{t('update_available', { version: release.latest_version ?? '' })}</p>
              <p className="mt-1 text-muted">{t('update_manually')}</p>
              {releaseUrl && (
                <a
                  className="text-link mt-2 inline-block"
                  href={releaseUrl}
                  target="_blank"
                  rel="noreferrer"
                >
                  {t('release_notes')}
                </a>
              )}
            </div>
          </Notice>
        ) : (
          <p role="status" className="text-muted">
            {t('up_to_date')}
          </p>
        )}
      </div>
    </div>
  );
}

function SettingsContent({ settings, auth }: { settings: SettingsData; auth: AuthStatus }) {
  const { data, connected, autosave } = useMiner();
  const t = useT();
  const draft = autosave.draft ?? settings;
  const [ignoredText, setIgnoredText] = useState(draft.drop_name_blacklist.join('\n'));
  const [editingIgnored, setEditingIgnored] = useState(false);
  useEffect(() => {
    if (!editingIgnored) setIgnoredText(draft.drop_name_blacklist.join('\n'));
  }, [draft.drop_name_blacklist, editingIgnored]);
  const [search, setSearch] = useState('');
  const [gameError, setGameError] = useState('');
  const [confirmation, setConfirmation] = useState<{
    title: string;
    text: string;
    action: () => Promise<unknown>;
  } | null>(null);
  const command = useAction();
  const proxyAction = useAction();
  const oauthAction = useAction();
  const logoutAction = useAction();
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle');
  useEffect(() => setCopyState('idle'), [data?.login.oauth_pending?.code]);
  const dirty = autosave.pending || autosave.busy;
  const change = autosave.change;
  function addGame(name: string) {
    change('games_to_watch', (games) =>
      games.some((game) => game.toLowerCase() === name.toLowerCase()) ? games : [...games, name],
    );
    setSearch('');
    setGameError('');
  }
  function resolveGame() {
    const name = search.trim();
    if (!name) return;
    const games = settings.games_available ?? [];
    const exact = games.find((item) => item.toLocaleLowerCase() === name.toLocaleLowerCase());
    const matches = games.filter((item) =>
      item.toLocaleLowerCase().includes(name.toLocaleLowerCase()),
    );
    const selected = exact ?? (matches.length === 1 ? matches[0] : undefined);
    if (selected) {
      if (!draft.games_to_watch.includes(selected)) addGame(selected);
      return;
    }
    if (matches.length > 1) {
      setGameError(t('gui.settings.multiple_games_found'));
      return;
    }
    setConfirmation({
      title: t('gui.settings.add_game'),
      text: t('gui.settings.manual_game_warning', { game: name }),
      action: async () => {
        addGame(name);
      },
    });
  }
  const available = (settings.games_available ?? []).filter(
    (game) =>
      !draft.games_to_watch.includes(game) &&
      game.toLocaleLowerCase().includes(search.toLocaleLowerCase()),
  );
  const oauth = data?.login.oauth_pending;
  async function test(path: string, payload: unknown) {
    const result = await request<Result>(path, payload);
    if (!result.success) throw new Error(result.message);
  }
  return (
    <div className="max-w-4xl space-y-8">
      <div>
        <h1 className="text-[22px] font-semibold">{t('gui.tabs.settings')}</h1>
      </div>
      <nav
        aria-label={t('settings_sections')}
        className="flex flex-wrap gap-x-5 gap-y-2 text-[13px] text-muted"
      >
        {['account', 'mining', 'connection', 'access', 'maintenance'].map((id) => (
          <a className="hover:text-text" key={id} href={`#${id}`}>
            {t(id)}
          </a>
        ))}
      </nav>
      <Section
        id="account"
        title={
          <>
            {t('account')}:{' '}
            <span className="font-normal text-soft">{plainText(data?.login.status ?? '')}</span>
          </>
        }
      >
        {data?.login.user_id && <p className="muted">Twitch ID: {data.login.user_id}</p>}
        {data?.login.user_id && (
          <IconButton
            path={mdiLogout}
            label={t('twitch_logout')}
            disabled={!connected || logoutAction.busy}
            onClick={() => void logoutAction.run(() => request('/api/twitch/logout', {}))}
          />
        )}
        <ActionResult action={logoutAction} />
        {oauth ? (
          <div className="space-y-3">
            <div className="authorization-row">
              <div className="flex items-center gap-2 rounded border border-divider bg-field ps-3 pe-1">
                <code className="select-all text-lg tracking-[.2em]">{oauth.code}</code>
                <IconButton
                  path={mdiContentCopy}
                  label={t('copy_code')}
                  title={t(copyState === 'copied' ? 'code_copied' : 'copy_code')}
                  className="size-7 max-md:size-9"
                  onClick={() => {
                    void (async () => {
                      try {
                        await navigator.clipboard.writeText(oauth.code);
                        setCopyState('copied');
                      } catch {
                        setCopyState('failed');
                      }
                    })();
                  }}
                />
              </div>
              <a className="button" href={safeUrl(oauth.url)} target="_blank" rel="noreferrer">
                {t('gui.login.oauth_activate')}
                <Icon className="mdi-icon" path={mdiOpenInNew} />
              </a>
              <Button
                disabled={!connected || oauthAction.busy}
                onClick={() =>
                  void oauthAction.run(
                    () => request('/api/oauth/confirm', {}),
                    t('authorization_waiting'),
                  )
                }
              >
                {t('gui.login.oauth_confirm')}
              </Button>
            </div>
            {copyState === 'copied' && (
              <span className="sr-only" role="status">
                {t('code_copied')}
              </span>
            )}
            {copyState === 'failed' && <Notice error>{t('copy_code_failed')}</Notice>}
            <ActionResult action={oauthAction} />
          </div>
        ) : (
          !data?.login.user_id && <Notice>{t('authorization_pending')}</Notice>
        )}
      </Section>
      <form
        onSubmit={(event) => {
          event.preventDefault();
        }}
      >
        <fieldset disabled={!connected} className="min-w-0 space-y-8">
          <Section id="mining" title={t('mining')}>
            <div className="flex gap-2">
              <div
                className="flex-1"
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    event.preventDefault();
                    resolveGame();
                  }
                }}
              >
                <Search
                  value={search}
                  onChange={(value) => {
                    setSearch(value);
                    setGameError('');
                  }}
                  label={t('gui.settings.search_games')}
                />
              </div>
              <IconButton
                path={mdiPlus}
                label={t('gui.settings.add_game')}
                onClick={resolveGame}
                disabled={!search.trim()}
              />
            </div>
            {gameError && <Notice error>{gameError}</Notice>}
            {search && available.length > 0 && (
              <div className="max-h-40 overflow-y-auto rounded border border-divider">
                {available.map((game) => (
                  <button
                    type="button"
                    key={game}
                    className="block w-full px-3 py-2 text-start text-[13px] hover:bg-hover"
                    onClick={() => addGame(game)}
                  >
                    {game}
                  </button>
                ))}
              </div>
            )}
            <p className="muted">{t('selected_games_help')}</p>
            <GamePriorities
              games={draft.games_to_watch}
              campaigns={data?.campaigns ?? []}
              onChange={(games) => change('games_to_watch', games)}
            />
            <fieldset className="space-y-2">
              <legend className="text-[13px] font-medium">{t('auto_mine_types')}</legend>
              <p className="muted">{t('auto_mine_types_help')}</p>
              <div className="flex flex-wrap gap-x-6">
                <Check
                  label={t('auto_mine_badges')}
                  checked={draft.auto_mine_badges}
                  onChange={(value) => change('auto_mine_badges', value)}
                />
                <Check
                  label={t('auto_mine_emotes')}
                  checked={draft.auto_mine_emotes}
                  onChange={(value) => change('auto_mine_emotes', value)}
                />
              </div>
            </fieldset>
            <div>
              <p className="mb-2 text-[13px] font-medium">{t('gui.settings.mining_benefits')}</p>
              <div className="flex flex-wrap gap-x-6">
                {[
                  ['BADGE', 'badge'],
                  ['EMOTE', 'emote'],
                  ['DIRECT_ENTITLEMENT', 'item'],
                  ['UNKNOWN', 'other'],
                ].map(
                  ([key, label]) =>
                    key && (
                      <Check
                        key={key}
                        label={t(`gui.inventory.filters.${label}`)}
                        checked={draft.mining_benefits[key] ?? true}
                        onChange={(value) =>
                          change('mining_benefits', { ...draft.mining_benefits, [key]: value })
                        }
                      />
                    ),
                )}
              </div>
            </div>
            <Field
              label={t('gui.settings.drop_name_blacklist')}
              help={t('gui.settings.drop_name_blacklist_help')}
            >
              <textarea
                className="field"
                value={ignoredText}
                onFocus={() => setEditingIgnored(true)}
                onBlur={() => setEditingIgnored(false)}
                onChange={(event) => {
                  setIgnoredText(event.target.value);
                  change('drop_name_blacklist', event.target.value.split('\n'));
                }}
              />
            </Field>
            <Field label={t('gui.settings.minimum_refresh')}>
              <Input
                type="number"
                min={1}
                max={1440}
                step={1}
                required
                value={
                  Number.isFinite(draft.minimum_refresh_interval_minutes)
                    ? draft.minimum_refresh_interval_minutes
                    : ''
                }
                onChange={(event) =>
                  change('minimum_refresh_interval_minutes', event.target.valueAsNumber)
                }
              />
            </Field>
          </Section>
          <Section id="connection" title={t('connection')}>
            <p className="muted">{t(connected ? 'connected' : 'connecting')}</p>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t('proxy')} help={t('proxy_help')}>
                <Input
                  type="url"
                  autoComplete="off"
                  value={draft.proxy}
                  placeholder="http://127.0.0.1:8080"
                  onChange={(event) => change('proxy', event.target.value)}
                />
              </Field>
              <Field label={t('gui.settings.connection_quality')}>
                <select
                  className="field"
                  value={draft.connection_quality}
                  onChange={(event) => change('connection_quality', Number(event.target.value))}
                >
                  {[1, 2, 3, 4, 5, 6].map((value) => (
                    <option key={value} value={value}>
                      {value}
                    </option>
                  ))}
                </select>
              </Field>
            </div>
            <Button
              disabled={!connected || !draft.proxy || proxyAction.busy}
              onClick={() =>
                void proxyAction.run(
                  () => test('/api/settings/verify-proxy', { proxy: draft.proxy }),
                  t('connection_verified'),
                )
              }
            >
              {t('verify_proxy')}
            </Button>
            <ActionResult action={proxyAction} />
          </Section>
          <div className="space-y-2" aria-live="polite">
            {autosave.error && (
              <Notice error>
                {t(autosave.error)}{' '}
                <IconButton
                  path={mdiReload}
                  label={t('retry')}
                  disabled={!connected || autosave.busy}
                  onClick={() => void autosave.retry()}
                />
              </Notice>
            )}
          </div>
        </fieldset>
      </form>
      <Access initial={auth} disabled={dirty || !connected} />
      <Section id="maintenance" title={t('maintenance')}>
        <ReleaseNotice disabled={!connected} />
        <div className="flex flex-wrap gap-2">
          <InventoryRefreshButton />
          <Button
            disabled={!connected || command.busy}
            onClick={() =>
              setConfirmation({
                title: t('gui.settings.clear_all_cache'),
                text: t('cache_help'),
                action: () => request('/api/cache/clear', {}),
              })
            }
          >
            {t('gui.settings.clear_all_cache')}
          </Button>
        </div>
        <details>
          <summary className="text-[13px] text-muted">{t('advanced')}</summary>
          <Button
            className="mt-3"
            disabled={!connected || command.busy}
            onClick={() =>
              setConfirmation({
                title: t('shutdown'),
                text: t('shutdown_help'),
                action: () => request('/api/close', {}),
              })
            }
          >
            {t('shutdown')}
          </Button>
        </details>
        <ActionResult action={command} />
        <div className="space-y-2 pt-2 text-[13px] text-muted">
          <p>{t('gui.help.about_text')}</p>
          <p>
            {t('help_link_accounts')}{' '}
            <a
              className="text-link"
              href="https://www.twitch.tv/drops/campaigns"
              target="_blank"
              rel="noreferrer"
            >
              Twitch
            </a>
          </p>
          <a
            className="text-link inline-block"
            href="https://github.com/ohne-b/twitch-drops-miner"
            target="_blank"
            rel="noreferrer"
          >
            {t('source_license')}
          </a>
        </div>
      </Section>
      <Dialog
        open={confirmation !== null}
        title={confirmation?.title ?? ''}
        onClose={() => setConfirmation(null)}
      >
        <p className="text-muted">{confirmation?.text}</p>
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={() => setConfirmation(null)}>{t('cancel')}</Button>
          <Button
            primary
            disabled={command.busy}
            onClick={() =>
              void command.run(async () => {
                await confirmation?.action();
                setConfirmation(null);
              })
            }
          >
            {t('gui.settings.confirm_btn')}
          </Button>
        </div>
        <ActionResult action={command} />
      </Dialog>
    </div>
  );
}
export default function Settings({ auth }: { auth: AuthStatus }) {
  const { data } = useMiner();
  const t = useT();
  return data ? (
    <SettingsContent settings={data.settings} auth={auth} />
  ) : (
    <Empty title={t('loading')} />
  );
}
