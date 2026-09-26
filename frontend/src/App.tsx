import { useEffect, useState } from 'react';
import { NavLink, Navigate, Route, Routes, useLocation, useNavigate } from 'react-router';
import {
  mdiViewDashboardOutline,
  mdiGiftOutline,
  mdiHistory,
  mdiTextBoxOutline,
  mdiCogOutline,
  mdiLogout,
  mdiGithub,
} from '@mdi/js';
import type { AuthStatus } from './lib/types';
import { request } from './lib/api';
import { I18n, useT } from './lib/i18n';
import { MinerProvider, useMiner } from './lib/state';
import { Button, Empty, Icon, Notice } from './components/ui';
import Overview from './pages/Overview';
import Campaigns from './pages/Campaigns';
import History from './pages/History';
import Activity from './pages/Activity';
import Settings from './pages/Settings';
import Login from './pages/Login';
function Shell({ auth, onLogout }: { auth: AuthStatus; onLogout: () => Promise<void> }) {
  const { data, connected } = useMiner();
  const t = useT();
  const location = useLocation();
  const [logoutError, setLogoutError] = useState(false);
  const links = [
    ['/', 'overview', mdiViewDashboardOutline],
    ['/campaigns', 'campaigns', mdiGiftOutline],
    ['/history', 'gui.tabs.history', mdiHistory],
    ['/activity', 'activity', mdiTextBoxOutline],
    ['/settings', 'gui.tabs.settings', mdiCogOutline],
  ] as const;
  useEffect(() => {
    if (location.hash)
      requestAnimationFrame(() =>
        document.getElementById(location.hash.slice(1))?.scrollIntoView(),
      );
    else window.scrollTo(0, 0);
  }, [location]);
  return (
    <div className="min-h-dvh lg:grid lg:grid-cols-[192px_minmax(0,1fr)]">
      <a href="#main" className="sr-only fixed z-50 bg-soft p-3 text-canvas focus:not-sr-only">
        {t('skip_content')}
      </a>
      <aside className="border-b border-divider bg-surface lg:sticky lg:top-0 lg:flex lg:h-dvh lg:flex-col lg:border-e lg:border-b-0">
        <div className="flex h-16 items-center justify-between px-5">
          <NavLink to="/" className="font-semibold tracking-tight">
            Twitch miner
          </NavLink>
        </div>
        <nav
          aria-label={t('navigation')}
          className="flex gap-1 overflow-x-auto px-3 pb-3 lg:flex-col lg:py-2"
        >
          {links.map(([to, label, icon]) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                `flex min-h-10 shrink-0 items-center gap-3 rounded px-3 text-[13px] transition-colors ${isActive ? 'bg-raised font-semibold text-text' : 'text-muted hover:bg-field hover:text-soft'}`
              }
            >
              <Icon path={icon} className="hidden lg:block" />
              {t(label)}
            </NavLink>
          ))}
        </nav>
        <div className="mt-auto hidden border-t border-divider p-4 lg:block">
          <a
            className="inline-flex size-11 items-center justify-center rounded text-muted transition-colors hover:text-text"
            href="https://github.com/ohne-b/twitch-miner"
            target="_blank"
            rel="noreferrer"
            aria-label="GitHub repository"
            title="GitHub"
          >
            <Icon path={mdiGithub} className="size-8!" />
          </a>
          {data?.login.user_id != null && (
            <p className="mt-2 text-xs tabular-nums text-muted">Twitch: {data.login.user_id}</p>
          )}
          {auth.enabled && (
            <Button
              className="mt-3 w-full"
              onClick={() => void onLogout().catch(() => setLogoutError(true))}
            >
              <Icon path={mdiLogout} />
              {t('gui.auth.logout')}
            </Button>
          )}
        </div>
      </aside>
      <main id="main" tabIndex={-1} className="min-w-0 p-4 outline-none md:p-6 xl:p-8">
        <div className="mx-auto max-w-[1440px]">
          {!connected && (
            <div className="mb-5">
              <Notice>{t(data ? 'disconnected_help' : 'connecting')}</Notice>
            </div>
          )}
          {logoutError && <Notice error>{t('gui.auth.request_failed')}</Notice>}
          {auth.enabled && (
            <div className="mb-4 text-end lg:hidden">
              <Button onClick={() => void onLogout().catch(() => setLogoutError(true))}>
                {t('gui.auth.logout')}
              </Button>
            </div>
          )}
          <Routes>
            <Route path="/" element={<Overview />} />
            <Route path="/campaigns" element={<Campaigns />} />
            <Route path="/history" element={<History />} />
            <Route path="/activity" element={<Activity />} />
            <Route path="/settings" element={<Settings auth={auth} />} />
            <Route path="/login" element={<Navigate to="/" replace />} />
            <Route path="*" element={<Empty title={t('not_found')} />} />
          </Routes>
        </div>
      </main>
    </div>
  );
}
export default function App() {
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [error, setError] = useState(false);
  const navigate = useNavigate();
  const t = useT();
  async function refresh() {
    const state = await request<AuthStatus>('/api/auth/status');
    setAuth(state);
    setError(false);
    if (state.authenticated && location.pathname === '/login') navigate('/', { replace: true });
  }
  useEffect(() => {
    void refresh().catch(() => setError(true));
    const expire = () => {
      setAuth((current) => ({ ...current, enabled: true, authenticated: false }));
      navigate('/login', { replace: true });
    };
    window.addEventListener('auth-expired', expire);
    const updated = () => {
      void refresh().catch(() => setError(true));
    };
    window.addEventListener('auth-updated', updated);
    return () => {
      window.removeEventListener('auth-expired', expire);
      window.removeEventListener('auth-updated', updated);
    };
  }, []);
  if (!auth && error) return <Login onLogin={refresh} statusError />;
  if (!auth) return <Empty title={t('loading')} />;
  if (!auth.authenticated)
    return (
      <I18n messages={{ gui: { auth: auth.translations ?? {} } }}>
        <Login onLogin={refresh} />
      </I18n>
    );
  return (
    <MinerProvider>
      <Shell
        auth={auth}
        onLogout={async () => {
          await request('/api/auth/logout', {});
          await refresh();
        }}
      />
    </MinerProvider>
  );
}
