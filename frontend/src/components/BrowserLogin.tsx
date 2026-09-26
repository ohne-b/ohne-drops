import { useEffect, useState } from 'react';
import { ApiError, request } from '../lib/api';
import { useT } from '../lib/i18n';
import { useMiner } from '../lib/state';
import { Button, Field, Input, Notice, dateTime } from './ui';

interface SessionStatus {
  enabled: boolean;
  authentication_required: boolean;
  session?: { state: string; expires_at: number | null; paired: boolean; generation: number };
}

export function BrowserLogin() {
  const t = useT();
  const { connected, data } = useMiner();
  const [status, setStatus] = useState<SessionStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [statusError, setStatusError] = useState(false);
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    const load = () =>
      void request<SessionStatus>('/api/session', undefined, 'GET', controller.signal)
        .then((value) => {
          if (!controller.signal.aborted) {
            setStatus(value);
            setStatusError(false);
          }
        })
        .catch(() => {
          if (!controller.signal.aborted) setStatusError(true);
        });
    load();
    const timer = setInterval(load, 30000);
    return () => {
      controller.abort();
      clearInterval(timer);
    };
  }, [connected, data?.login.user_id, refresh]);
  async function run(task: () => Promise<void>) {
    setBusy(true);
    setError('');
    try {
      await task();
      setRefresh((value) => value + 1);
    } catch (failure) {
      const key = failure instanceof ApiError ? failure.message : 'session_format';
      const translated = t(key);
      setError(translated === key ? t('session_request_error') : translated);
    } finally {
      setBusy(false);
    }
  }
  const message = error || (statusError ? t('session_status_error') : '');
  if (!status?.enabled) return message ? <Notice error>{message}</Notice> : null;
  return (
    <div className="max-w-xl space-y-4 border-t border-divider pt-4">
      <h3 className="section-title">{t('browser_login')}</h3>
      <p className="muted">{t('browser_login_help')}</p>
      {message && <Notice error>{message}</Notice>}
      {status.authentication_required ? (
        <Notice>{t('session_auth_required')}</Notice>
      ) : (
        <>
          <p className="muted">{t(`session_state_${status.session?.state ?? 'waiting'}`)}</p>
          {status.session?.expires_at && (
            <p className="muted">
              {t('session_expires', {
                time: dateTime(new Date(status.session.expires_at * 1000).toISOString()),
              })}
            </p>
          )}
          <Field label={t('session_file')} help={t('session_file_help')}>
            <Input
              type="file"
              accept=".json,application/json"
              disabled={!connected || busy}
              onChange={(event) => {
                const file = event.target.files?.[0];
                event.target.value = '';
                if (!file) return;
                void run(async () => {
                  if (file.size > 65536) throw new ApiError(413, 'session_format');
                  const bundle: unknown = JSON.parse(await file.text());
                  await request('/api/session/import', bundle);
                });
              }}
            />
          </Field>
          <div className="flex flex-wrap gap-2">
            {!data?.login.user_id && (status.session?.generation ?? 0) > 0 && (
              <Button
                disabled={!connected || busy}
                onClick={() =>
                  void run(async () => {
                    await request('/api/twitch/logout', {});
                  })
                }
              >
                {t('twitch_logout')}
              </Button>
            )}
            <Button
              disabled={!connected || busy || status.session?.state !== 'ready'}
              onClick={() =>
                void run(async () => {
                  const connection = await request('/api/session/pair', {});
                  const url = URL.createObjectURL(
                    new Blob([JSON.stringify(connection)], { type: 'application/json' }),
                  );
                  const link = document.createElement('a');
                  link.href = url;
                  link.download = 'tdm-connection.json';
                  link.click();
                  setTimeout(() => URL.revokeObjectURL(url), 1000);
                })
              }
            >
              {t('session_pair')}
            </Button>
            <Button
              disabled={!connected || busy || !status.session?.paired}
              onClick={() =>
                void run(async () => {
                  await request('/api/session/revoke', {});
                })
              }
            >
              {t('session_revoke')}
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
