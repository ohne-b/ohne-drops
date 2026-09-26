import { useEffect, useState, type ReactNode } from 'react';
import { mdiArrowUp, mdiArrowDown, mdiClose, mdiPlus, mdiOpenInNew } from '@mdi/js';
import type { AuthStatus, Result, Settings as SettingsData } from '../lib/types';
import { moveGame, request, safeUrl } from '../lib/api';
import { useMiner } from '../lib/state';
import { plainText, useT } from '../lib/i18n';
import {
  ActionResult,
  Button,
  Check,
  Dialog,
  Empty,
  Field,
  Icon,
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
  title: string;
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
const editable = (settings: SettingsData): SettingsData => ({
  ...settings,
});
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
    }, t('saved'));
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
function SettingsContent({ settings, auth }: { settings: SettingsData; auth: AuthStatus }) {
  const { data, connected } = useMiner();
  const t = useT();
  const [base, setBase] = useState(() => editable(settings));
  const [draft, setDraft] = useState(() => editable(settings));
  const [search, setSearch] = useState('');
  const [gameError, setGameError] = useState('');
  const [confirmation, setConfirmation] = useState<{
    title: string;
    text: string;
    action: () => Promise<unknown>;
  } | null>(null);
  const [version, setVersion] = useState('');
  const saveAction = useAction();
  const command = useAction();
  const proxyAction = useAction();
  const oauthAction = useAction();
  const dirty = JSON.stringify(draft) !== JSON.stringify(base);
  useEffect(() => {
    if (!dirty) {
      const next = editable(settings);
      setBase(next);
      setDraft(next);
    }
  }, [settings, dirty]);
  useEffect(() => {
    void request<{ current_version: string }>('/api/version')
      .then((result) => setVersion(result.current_version))
      .catch(() => {});
  }, []);
  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (dirty) event.preventDefault();
    };
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [dirty]);
  function change<K extends keyof SettingsData>(key: K, value: SettingsData[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
    saveAction.clear();
  }
  async function save() {
    await saveAction.run(async () => {
      const { games_available: _games, ...payload } = draft;
      const result = await request<{ settings: SettingsData }>('/api/settings', payload);
      const next = editable(result.settings);
      setDraft(next);
      setBase(next);
    }, t('saved'));
  }
  function addGame(name: string) {
    setDraft((current) => ({
      ...current,
      games_to_watch: current.games_to_watch.some(
        (game) => game.toLocaleLowerCase() === name.toLocaleLowerCase(),
      )
        ? current.games_to_watch
        : [...current.games_to_watch, name],
    }));
    saveAction.clear();
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
        <p className="mt-1 text-muted">{t('settings_description')}</p>
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
      <Section id="account" title={t('account')}>
        <p>{plainText(data?.login.status ?? '')}</p>
        {data?.login.user_id && <p className="muted">Twitch ID: {data.login.user_id}</p>}
        {oauth ? (
          <div className="panel max-w-lg space-y-4 p-5">
            <p className="text-[13px] text-muted">{t('gui.login.oauth_prompt')}</p>
            <div className="flex flex-wrap items-center gap-4">
              <code className="select-all rounded border border-divider bg-field px-4 py-2 text-xl tracking-[.2em]">
                {oauth.code}
              </code>
              <a className="button" href={safeUrl(oauth.url)} target="_blank" rel="noreferrer">
                {t('gui.login.oauth_activate')}
                <Icon path={mdiOpenInNew} />
              </a>
            </div>
            <Button
              primary
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
            <ActionResult action={oauthAction} />
          </div>
        ) : (
          !data?.login.user_id && <Notice>{t('authorization_pending')}</Notice>
        )}
      </Section>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <fieldset disabled={saveAction.busy} className="min-w-0 space-y-8">
          <Section id="mining" title={t('mining')} help={t('priority_help')}>
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
              <Button onClick={resolveGame} disabled={!search.trim()}>
                <Icon path={mdiPlus} />
                {t('gui.settings.add_game')}
              </Button>
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
            <div className="flex gap-2">
              <Button
                onClick={() =>
                  change('games_to_watch', [
                    ...draft.games_to_watch,
                    ...(settings.games_available ?? []).filter(
                      (game) =>
                        !draft.games_to_watch.some(
                          (existing) => existing.toLocaleLowerCase() === game.toLocaleLowerCase(),
                        ),
                    ),
                  ])
                }
              >
                {t('gui.settings.select_all')}
              </Button>
              <Button
                disabled={!draft.games_to_watch.length}
                onClick={() =>
                  setConfirmation({
                    title: t('gui.settings.deselect_all'),
                    text: t('gui.settings.deselect_all_warning'),
                    action: async () => change('games_to_watch', []),
                  })
                }
              >
                {t('gui.settings.deselect_all')}
              </Button>
            </div>
            <div className="panel">
              {draft.games_to_watch.map((game, index) => (
                <div className="row" key={game}>
                  <Input
                    key={`${game}:${index}`}
                    className="w-14 shrink-0 text-center tabular-nums"
                    type="number"
                    min={1}
                    step={1}
                    aria-label={t('gui.settings.game_priority', { game })}
                    defaultValue={index + 1}
                    onBlur={(event) => {
                      const value = event.target.value.trim();
                      if (!value || !Number.isInteger(Number(value)))
                        event.target.value = String(index + 1);
                      else {
                        const rank = Math.min(
                          draft.games_to_watch.length,
                          Math.max(1, Number(value)),
                        );
                        event.target.value = String(rank);
                        change('games_to_watch', moveGame(draft.games_to_watch, index, rank - 1));
                      }
                    }}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault();
                        event.currentTarget.blur();
                      }
                      if (event.key === 'Escape') {
                        event.currentTarget.value = String(index + 1);
                        event.currentTarget.blur();
                      }
                    }}
                  />
                  <span className="min-w-0 flex-1 break-words text-[13px]">{game}</span>
                  <div className="flex gap-1">
                    <Button
                      className="px-2"
                      disabled={index === 0}
                      aria-label={t('move_up', { game })}
                      title={t('move_up', { game })}
                      onClick={() =>
                        change('games_to_watch', moveGame(draft.games_to_watch, index, index - 1))
                      }
                    >
                      <Icon path={mdiArrowUp} />
                    </Button>
                    <Button
                      className="px-2"
                      disabled={index === draft.games_to_watch.length - 1}
                      aria-label={t('move_down', { game })}
                      title={t('move_down', { game })}
                      onClick={() =>
                        change('games_to_watch', moveGame(draft.games_to_watch, index, index + 1))
                      }
                    >
                      <Icon path={mdiArrowDown} />
                    </Button>
                    <Button
                      className="px-2"
                      aria-label={t('gui.settings.remove_game', { game })}
                      title={t('gui.settings.remove_game', { game })}
                      onClick={() =>
                        change(
                          'games_to_watch',
                          draft.games_to_watch.filter((item) => item !== game),
                        )
                      }
                    >
                      <Icon path={mdiClose} />
                    </Button>
                  </div>
                </div>
              ))}
              {!draft.games_to_watch.length && (
                <Empty title={t('gui.settings.no_games_selected')} />
              )}
            </div>
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
                value={draft.drop_name_blacklist.join('\n')}
                onChange={(event) => change('drop_name_blacklist', event.target.value.split('\n'))}
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
          <Section id="interface" title={t('interface')}>
            <p className="muted">{t('dark_appearance')}</p>
          </Section>
          {(dirty || saveAction.error || saveAction.success) && (
            <div className="sticky bottom-0 z-10 space-y-3 border-t border-divider bg-canvas/95 py-4">
              <ActionResult action={saveAction} />
              {dirty && base.revision !== settings.revision && (
                <Notice error>{t('settings_conflict')}</Notice>
              )}
              <div className="flex items-center gap-2">
                <Button type="submit" primary disabled={!dirty || !connected || saveAction.busy}>
                  {t(saveAction.busy ? 'saving' : 'save_changes')}
                </Button>
                <Button
                  disabled={!dirty || saveAction.busy}
                  onClick={() => {
                    const next = editable(settings);
                    setBase(next);
                    setDraft(next);
                    saveAction.clear();
                  }}
                >
                  {t('cancel')}
                </Button>
                {dirty && <span className="ms-2 text-[13px] text-muted">{t('unsaved')}</span>}
              </div>
            </div>
          )}
        </fieldset>
      </form>
      <Access initial={auth} disabled={dirty || !connected} />
      <Section id="maintenance" title={t('maintenance')}>
        <div className="flex flex-wrap gap-2">
          <Button
            disabled={!connected || command.busy}
            onClick={() =>
              void command.run(() => request('/api/reload', {}), t('refresh_requested'))
            }
          >
            {t('refresh')}
          </Button>
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
          <p>Twitch miner {version && `· ${version}`}</p>
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
          <p>{t('help_priorities')}</p>
          <a
            className="text-link inline-block"
            href="https://github.com/ohne-b/twitch-miner"
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
