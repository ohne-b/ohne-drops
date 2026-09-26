import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import fixture from './fixture.json' with { type: 'json' };
import type { Snapshot } from '../src/lib/types';
const snapshot: Snapshot = fixture;
const headers = { 'X-TDM-Request': '1' };
test.beforeEach(async ({ request, page }) => {
  const reset = await request.post('/__test/reset', { headers, data: {} });
  expect(reset.ok()).toBe(true);
  expect(await reset.json()).toEqual({ ok: true });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Overview', exact: true })).toBeVisible();
});
test('confirmed progress and compact desktop design', async ({ page }) => {
  await expect(page.getByText('42 / 60 min', { exact: true })).toBeVisible();
  await expect(page.getByRole('progressbar', { name: 'Explorer jacket' })).toHaveAttribute(
    'aria-valuenow',
    '42',
  );
  await expect(page.getByText('48 / 60 min')).toHaveCount(0);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.screenshot({ path: '../artifacts/redesign-desktop.png', fullPage: true });
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme)).toBe(
    'dark',
  );
  expect(
    await page
      .locator('body')
      .evaluate((element) => getComputedStyle(element, '::-webkit-scrollbar').width),
  ).toBe('3px');
});
test('every route loads directly and stays usable on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  for (const [route, title] of [
    ['/campaigns', 'Campaigns'],
    ['/history', 'History'],
    ['/activity', 'Activity'],
    ['/settings', 'Settings'],
  ]) {
    await page.goto(route!);
    await expect(page.getByRole('heading', { name: title!, exact: true })).toBeVisible();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
  }
  await page.screenshot({ path: '../artifacts/redesign-settings-mobile.png', fullPage: true });
});
test('channel search, clear, selection and automatic mode', async ({ page }) => {
  await page.getByRole('searchbox', { name: 'Search channels' }).fill('HARBOR');
  await expect(page.getByRole('link', { name: 'northwind', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Watch harbor' }).click();
  await expect(page.getByText('Manual selection')).toBeVisible();
  await page.getByRole('button', { name: 'Return to Auto Mode' }).click();
  await expect(page.getByText('Automatic selection')).toBeVisible();
  await page.getByRole('button', { name: 'Clear search' }).click();
  await expect(page.getByRole('link', { name: 'northwind', exact: true })).toBeVisible();
});
test('campaign filtering and truthful expanded progress', async ({ page }) => {
  await page.goto('/campaigns');
  await page.getByText('Autumn expedition', { exact: true }).click();
  await expect(page.getByText('42 / 60 min')).toBeVisible();
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  await page.getByLabel('Not Linked', { exact: true }).check();
  await expect(page.getByText('No matching results')).toBeVisible();
  await page.getByLabel('Not Linked', { exact: true }).uncheck();
  await expect(page.getByText('Autumn expedition', { exact: true })).toBeVisible();
});
test('game priorities show icons instead of editable numbers', async ({ page, request }) => {
  await page.goto('/settings');
  await expect(page.getByRole('spinbutton', { name: /Priority for/ })).toHaveCount(0);
  await page
    .getByRole('button', { name: 'Reorder The Elder Scrolls Online', exact: true })
    .press('ArrowUp');
  await expect
    .poll(async () => (await (await request.get('/api/settings')).json()).games_to_watch)
    .toEqual(['Rust', 'The Elder Scrolls Online', 'Sea of Thieves']);
  await page.reload();
  await expect(page.locator('#mining [data-game]').nth(1)).toHaveAttribute(
    'data-game',
    'The Elder Scrolls Online',
  );
});

test('Twitch logout leaves the dashboard available and shows the next login', async ({ page }) => {
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Log out of Twitch', exact: true }).click();
  await expect(page.getByText('NEWCODE', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Log out of Twitch', exact: true })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByText('NEWCODE', { exact: true })).toBeVisible();
});

test('manual game confirmation supports Escape and safe literal names', async ({ page }) => {
  await page.goto('/settings');
  await page.getByRole('searchbox', { name: 'Search games...' }).fill('<script>new game</script>');
  await page.getByRole('button', { name: 'Add Game', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await page.getByRole('button', { name: 'Add Game', exact: true }).click();
  await page.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect(page.getByText('<script>new game</script>', { exact: true })).toBeVisible();
});
test('autosave retains conflicting edits and retries only edited fields', async ({
  page,
  request,
}) => {
  await page.goto('/settings');
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(
    '**/api/settings',
    async (route) => {
      await gate;
      await route.continue();
    },
    { times: 1 },
  );
  const sent = page.waitForRequest('**/api/settings');
  const interval = page.getByLabel('Minimum Refresh Interval (minutes):', { exact: true });
  await interval.fill('45');
  await sent;
  const current = await (await request.get('/api/settings')).json();
  await request.post('/api/settings', {
    headers,
    data: {
      revision: current.revision,
      minimum_refresh_interval_minutes: 90,
      connection_quality: 3,
    },
  });
  release();
  await expect(page.getByRole('alert')).toContainText('Settings changed on another device');
  await expect(interval).toHaveValue('45');
  expect((await (await request.get('/api/settings')).json()).minimum_refresh_interval_minutes).toBe(
    90,
  );
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect
    .poll(
      async () =>
        (await (await request.get('/api/settings')).json()).minimum_refresh_interval_minutes,
    )
    .toBe(45);
  expect((await (await request.get('/api/settings')).json()).connection_quality).toBe(3);
});

test('removed notifications have no controls, API or saved credentials', async ({
  page,
  request,
}) => {
  await page.goto('/settings');
  await expect(page.getByText('Telegram', { exact: false })).toHaveCount(0);
  const result = await request.post('/api/settings', {
    headers,
    data: { telegram_bot_token: 'discard-me', telegram_chat_id: '123' },
  });
  expect(JSON.stringify(await result.json())).not.toContain('discard-me');
  expect((await request.post('/api/settings/test-telegram', { headers, data: {} })).status()).toBe(
    404,
  );
});

test('history filters, export and confirmed clearing', async ({ page }) => {
  await page.goto('/history');
  await expect(page.getByText('Canvas pack', { exact: true }).first()).toBeVisible();
  const csv = page.waitForEvent('download');
  await page.getByRole('link', { name: 'CSV', exact: true }).click();
  expect((await csv).suggestedFilename()).toBe('drop_history.csv');
  const json = page.waitForEvent('download');
  await page.getByRole('button', { name: 'JSON', exact: true }).click();
  expect((await json).suggestedFilename()).toBe('drop-history.json');
  await page.getByLabel('Since (UTC)', { exact: true }).fill('2026-10-01');
  await expect(page.getByText('No drops match these filters.')).toBeVisible();
  await page.getByLabel('Since (UTC)', { exact: true }).fill('');
  await page.getByRole('button', { name: 'Clear local history', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Clear local history', exact: true })
    .click();
  await expect(page.getByText('No drops match these filters.')).toBeVisible();
});
test('catalog restrictions and hostile strings remain explicit and inert', async ({
  page,
  request,
}) => {
  await request.post('/__test/event', {
    headers,
    data: { event: 'inventory_status', data: { available: false, checked_at: null } },
  });
  await expect(page.getByRole('alert')).toContainText('Twitch did not return the campaign catalog');
  await request.post('/__test/event', {
    headers,
    data: { event: 'console_output', data: { message: '<img src=x onerror="alert(1)">' } },
  });
  await expect(page.getByText('<img src=x onerror="alert(1)">', { exact: true })).toBeVisible();
  expect(await page.locator('img[src="x"]').count()).toBe(0);
});
test('snapshot replaces stale entities and keeps settings draft', async ({ page, request }) => {
  await page.goto('/settings');
  const interval = page.getByLabel('Minimum Refresh Interval (minutes):', { exact: true });
  await interval.fill('45');
  await request.post('/__test/event', {
    headers,
    data: { event: 'initial_state', data: { ...snapshot, channels: [], current_drop: null } },
  });
  await expect(interval).toHaveValue('45');
});
test('dashboard password, login, logout and API guard', async ({ page, browser, request }) => {
  await page.goto('/settings');
  await page
    .getByLabel('New password (at least 8 characters)', { exact: true })
    .fill('example-test-password');
  await page.getByLabel('Confirm new password', { exact: true }).fill('example-test-password');
  await page.getByRole('button', { name: 'Enable password protection', exact: true }).click();
  await expect(page.getByText('Password protection is enabled.', { exact: true })).toBeVisible();
  expect((await request.get('/api/settings')).status()).toBe(401);
  const context = await browser.newContext();
  const other = await context.newPage();
  await other.goto('http://127.0.0.1:8765/campaigns');
  await expect(other.getByRole('heading', { name: 'Unlock dashboard' })).toBeVisible();
  await other.getByLabel('Password', { exact: true }).fill('wrong');
  await other.getByRole('button', { name: 'Log in', exact: true }).click();
  await expect(other.getByRole('alert')).toContainText('Incorrect password');
  await other.getByLabel('Password', { exact: true }).fill('example-test-password');
  await other.getByRole('button', { name: 'Log in', exact: true }).click();
  await expect(other.getByRole('heading', { name: 'Overview', exact: true })).toBeVisible();
  await other.getByRole('button', { name: 'Log out', exact: true }).click();
  await expect(other.getByRole('heading', { name: 'Unlock dashboard' })).toBeVisible();
  await context.close();
});
test('mutation requests without the CSRF marker are blocked', async ({ request }) => {
  expect((await request.post('/api/cache/clear', { data: {} })).status()).toBe(403);
});

test('pages and confirmation dialogs meet automated accessibility checks', async ({ page }) => {
  for (const route of ['/', '/campaigns', '/history', '/activity', '/settings']) {
    await page.goto(route);
    await expect(page.locator('h1')).toBeVisible();
    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21aa'])
      .analyze();
    expect(results.violations, `${route}: ${JSON.stringify(results.violations)}`).toEqual([]);
  }
  await page.getByRole('searchbox', { name: 'Search games...' }).fill('Custom game');
  await page.getByRole('button', { name: 'Add Game', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});

test('saved game filters can be cleared after a campaign disappears', async ({ page, request }) => {
  const settings = await (await request.get('/api/settings')).json();
  await request.post('/api/settings', {
    headers,
    data: {
      revision: settings.revision,
      inventory_filters: { ...settings.inventory_filters, game_name_search: ['Old game'] },
    },
  });
  await page.goto('/campaigns');
  await expect(page.getByText('No matching results')).toBeVisible();
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  await expect(page.getByLabel('Old game', { exact: true })).toBeChecked();
  await page.getByRole('button', { name: 'All games', exact: true }).click();
  await expect(page.getByText('Autumn expedition', { exact: true })).toBeVisible();
});

test('a failed initial auth status remains recoverable without a page reload', async ({ page }) => {
  await page.route(
    '**/api/auth/status',
    (route) => route.fulfill({ status: 503, json: { detail: 'unavailable' } }),
    { times: 1 },
  );
  await page.reload();
  await expect(page.getByLabel('Password', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Overview', exact: true })).toBeVisible();
});

test('failed settings save retains input', async ({ page }) => {
  await page.goto('/settings');
  await page.getByLabel('Proxy URL', { exact: true }).fill('http://127.0.0.1:9999');
  await page.route(
    '**/api/settings',
    (route) => route.fulfill({ status: 500, json: { detail: 'save_failed' } }),
    { times: 1 },
  );
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByLabel('Proxy URL', { exact: true })).toHaveValue('http://127.0.0.1:9999');
  await expect(page.getByText('gui.auth.save_failed')).toHaveCount(0);
});

test('autosave queues newer input while an older request is pending', async ({ page, request }) => {
  await page.goto('/settings');
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(
    '**/api/settings',
    async (route) => {
      await gate;
      await route.continue();
    },
    { times: 1 },
  );
  const sent = page.waitForRequest('**/api/settings');
  const interval = page.getByLabel('Minimum Refresh Interval (minutes):', { exact: true });
  await interval.fill('45');
  await sent;
  await expect(interval).toBeEnabled();
  await interval.fill('60');
  release();
  await expect
    .poll(
      async () =>
        (await (await request.get('/api/settings')).json()).minimum_refresh_interval_minutes,
    )
    .toBe(60);
  await expect(interval).toHaveValue('60');
});

test('manual game confirmation appends to the latest settings from another device', async ({
  page,
  request,
}) => {
  await page.goto('/settings');
  await page.getByRole('searchbox', { name: 'Search games...' }).fill('Manual name');
  await page.getByRole('button', { name: 'Add Game', exact: true }).click();
  const settings = await (await request.get('/api/settings')).json();
  await request.post('/api/settings', {
    headers,
    data: { revision: settings.revision, games_to_watch: ['Another device'] },
  });
  await expect(page.locator('[data-game="Another device"]')).toHaveCount(1);
  await page.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect(page.getByText('Changes saved.', { exact: true })).toBeVisible();
  expect((await (await request.get('/api/settings')).json()).games_to_watch).toEqual([
    'Another device',
    'Manual name',
  ]);
});

test('autosave survives reconnect and navigation', async ({ page, request }) => {
  await page.goto('/settings');
  await page.getByLabel('Minimum Refresh Interval (minutes):', { exact: true }).fill('45');
  await request.post('/__test/reconnect', { headers, data: {} });
  await page.getByRole('link', { name: 'Overview', exact: true }).click();
  await expect
    .poll(
      async () =>
        (await (await request.get('/api/settings')).json()).minimum_refresh_interval_minutes,
    )
    .toBe(45);
  await page.goto('/settings');
  await expect(page.getByLabel('Minimum Refresh Interval (minutes):', { exact: true })).toHaveValue(
    '45',
  );
  await expect(page.getByRole('button', { name: 'Save changes', exact: true })).toHaveCount(0);
});

test('activity follows through bounded-buffer rollover and pauses for reading', async ({
  page,
  request,
}) => {
  await page.goto('/activity');
  await expect(page.getByLabel('Activity', { exact: true }).locator('p')).toHaveCount(3);
  await request.post('/__test/event', {
    headers,
    data: {
      event: 'initial_state',
      data: { ...snapshot, console: Array.from({ length: 1000 }, (_, i) => `Message ${i}`) },
    },
  });
  const log = page.getByLabel('Activity', { exact: true });
  await expect(log.locator('p')).toHaveCount(1000);
  await request.post('/__test/event', {
    headers,
    data: { event: 'console_output', data: { message: 'Newest message' } },
  });
  await expect
    .poll(() => log.evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight))
    .toBeLessThan(2);
  await log.evaluate((el) => {
    el.scrollTop = 0;
  });
  await expect(page.getByRole('button', { name: 'Follow latest', exact: true })).toBeVisible();
  await request.post('/__test/event', {
    headers,
    data: { event: 'console_output', data: { message: 'Another message' } },
  });
  await expect.poll(() => log.evaluate((el) => el.scrollTop)).toBe(0);
});

test('artwork expands Twitch dimensions before making a request', async ({ page, request }) => {
  const art = 'https://example.test/art/game-{width}x{height}.png';
  await page.route('https://example.test/**', (route) =>
    route.fulfill({
      contentType: 'image/png',
      body: Buffer.from(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aVRsAAAAASUVORK5CYII=',
        'base64',
      ),
    }),
  );
  await request.post('/__test/event', {
    headers,
    data: { event: 'channel_update', data: { ...snapshot.channels[0], game_icon: art } },
  });
  await expect(page.locator('img[src="https://example.test/art/game-80x112.png"]')).toBeVisible();
});

test('login uses translated strings supplied by the public auth endpoint', async ({ page }) => {
  await page.route('**/api/auth/status', (route) =>
    route.fulfill({
      json: {
        enabled: true,
        authenticated: false,
        translations: {
          login_title: 'Dashboard entsperren',
          password: 'Passwort',
          login: 'Anmelden',
        },
      },
    }),
  );
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Dashboard entsperren' })).toBeVisible();
  await expect(page.getByLabel('Passwort', { exact: true })).toBeVisible();
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});

test('password errors stay in Settings and protection changes reach a second browser', async ({
  page,
  browser,
}) => {
  await page.goto('/settings');
  await page
    .getByLabel('New password (at least 8 characters)', { exact: true })
    .fill('example-test-password');
  await page.getByLabel('Confirm new password', { exact: true }).fill('example-test-password');
  await page.getByRole('button', { name: 'Enable password protection', exact: true }).click();
  await expect(page.getByText('Password protection is enabled.', { exact: true })).toBeVisible();
  const context = await browser.newContext();
  const other = await context.newPage();
  await other.goto('http://127.0.0.1:8765/settings');
  await other.getByLabel('Password', { exact: true }).fill('example-test-password');
  await other.getByRole('button', { name: 'Log in', exact: true }).click();
  await expect(other.getByRole('heading', { name: 'Overview', exact: true })).toBeVisible();
  await other.goto('http://127.0.0.1:8765/settings');
  await expect(other.getByText('Password protection is enabled.', { exact: true })).toBeVisible();
  await page.getByLabel('Current password', { exact: true }).fill('wrong-password');
  await page.getByRole('button', { name: 'Disable protection', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Disable protection', exact: true })
    .click();
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('Incorrect password');
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByLabel('Current password', { exact: true }).fill('example-test-password');
  await page.getByRole('button', { name: 'Disable protection', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Disable protection', exact: true })
    .click();
  await expect(other.getByText('Password protection is disabled.', { exact: true })).toBeVisible();
  await expect(
    other.getByRole('button', { name: 'Enable password protection', exact: true }),
  ).toBeVisible();
  await context.close();
});

test('Select All preserves manual spelling, order and case-insensitive uniqueness', async ({
  page,
  request,
}) => {
  await request.post('/api/settings', {
    headers,
    data: { games_to_watch: ['Custom game', 'rust'] },
  });
  await page.goto('/settings');
  await expect(
    page.getByRole('spinbutton', { name: 'Priority for rust', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Select All', exact: true }).click();
  await expect(page.getByText('Changes saved.', { exact: true })).toBeVisible();
  const games = (await (await request.get('/api/settings')).json()).games_to_watch;
  expect(games.slice(0, 2)).toEqual(['Custom game', 'rust']);
  expect(games.filter((name: string) => name.toLowerCase() === 'rust')).toHaveLength(1);
});

test('history refreshes after claims and reports clear failure inside its dialog', async ({
  page,
  request,
}) => {
  await page.goto('/history');
  await expect(page.getByText('Canvas pack', { exact: true }).first()).toBeVisible();
  const history = await (await request.get('/api/history')).json();
  await page.route('**/api/history?*', (route) =>
    route.fulfill({
      json: {
        total: 2,
        entries: [
          ...history.entries,
          { ...history.entries[0], id: 'new-claim', drop_name: 'New reward' },
        ],
      },
    }),
  );
  await request.post('/__test/event', {
    headers,
    data: {
      event: 'drop_update',
      data: {
        campaign_id: 'campaign-1',
        campaign: { claimed_drops: 1 },
        drop: { ...snapshot.campaigns[0]!.drops[0], is_claimed: true },
      },
    },
  });
  await expect(page.getByText('New reward', { exact: true })).toBeVisible();
  await page.route('**/api/history', (route) =>
    route.fulfill({ status: 500, json: { detail: 'failure' } }),
  );
  await page.getByRole('button', { name: 'Clear local history', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Clear local history', exact: true })
    .click();
  await expect(page.getByRole('dialog').getByRole('alert')).toBeVisible();
  expect((await (await request.get('/api/history')).json()).entries).toHaveLength(1);
});

test('all channels remain available when priorities change', async ({ page, request }) => {
  await request.post('/api/settings', { headers, data: { games_to_watch: ['rUsT'] } });
  await expect(page.getByRole('link', { name: 'harbor', exact: true })).toBeVisible();
  await request.post('/api/settings', { headers, data: { games_to_watch: ['Other game'] } });
  await expect(page.getByRole('link', { name: 'harbor', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'northwind', exact: true })).toBeVisible();
});

test('phone campaign rows retain status and claimed counts', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto('/campaigns');
  await expect(page.getByText('0 / 2 claimed · Active', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('long international labels remain usable at phone, tablet and zoom-equivalent widths', async ({
  page,
  request,
}) => {
  await request.post('/api/settings', {
    headers,
    data: {
      games_to_watch: ['EinSehrLangerSpielnameOhneTrennzeichen'.repeat(4), '日本語のゲーム'],
      language: 'Deutsch',
    },
  });
  await page.goto('/settings');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  for (const width of [360, 640, 820]) {
    await page.setViewportSize({ width, height: 844 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  }
  await request.post('/api/settings', { headers, data: { language: 'العربية' } });
  await expect(page.locator('html')).not.toHaveAttribute('dir', 'rtl');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('autosave keeps text editing stable and blocks invalid values', async ({ page, request }) => {
  await page.goto('/settings');
  const ignored = page.getByLabel('Ignored Drop Keywords', { exact: true });
  await ignored.fill('Mask\n');
  await expect
    .poll(async () => (await (await request.get('/api/settings')).json()).drop_name_blacklist)
    .toEqual(['Mask']);
  await expect(ignored).toHaveValue('Mask\n');
  const interval = page.getByLabel('Minimum Refresh Interval (minutes):', { exact: true });
  await interval.fill('');
  await expect(page.getByRole('alert')).toContainText('whole refresh interval');
  expect((await (await request.get('/api/settings')).json()).minimum_refresh_interval_minutes).toBe(
    30,
  );
  await interval.fill('20');
  await expect
    .poll(
      async () =>
        (await (await request.get('/api/settings')).json()).minimum_refresh_interval_minutes,
    )
    .toBe(20);
});

test('pointer dragging saves on drop and Escape cancels a second drag', async ({
  page,
  request,
}) => {
  await page.goto('/settings');
  const handle = page.getByRole('button', { name: 'Reorder Rust', exact: true });
  await handle.scrollIntoViewIfNeeded();
  const from = (await handle.boundingBox())!;
  const last = (await page.locator('[data-game="The Elder Scrolls Online"]').boundingBox())!;
  await page.mouse.move(from.x + 10, from.y + 10);
  await page.mouse.down();
  await page.mouse.move(last.x + 70, last.y + last.height - 3, { steps: 8 });
  expect((await (await request.get('/api/settings')).json()).games_to_watch).toEqual([
    'Rust',
    'Sea of Thieves',
  ]);
  await page.mouse.up();
  await expect
    .poll(async () => (await (await request.get('/api/settings')).json()).games_to_watch)
    .toEqual(['Sea of Thieves', 'The Elder Scrolls Online', 'Rust']);
  const moved = (await handle.boundingBox())!;
  const first = (await page.locator('[data-game="Sea of Thieves"]').boundingBox())!;
  await page.mouse.move(moved.x + 10, moved.y + 10);
  await page.mouse.down();
  await page.mouse.move(first.x + 70, first.y + 10, { steps: 8 });
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect(page.locator('#mining [data-game]').last()).toHaveAttribute('data-game', 'Rust');
  expect((await (await request.get('/api/settings')).json()).games_to_watch).toEqual([
    'Sea of Thieves',
    'The Elder Scrolls Online',
    'Rust',
  ]);
});

test('touch dragging reorders game priorities', async ({ browser, request }) => {
  const context = await browser.newContext({
    hasTouch: true,
    isMobile: true,
    viewport: { width: 390, height: 844 },
  });
  const page = await context.newPage();
  await page.goto('http://127.0.0.1:8765/settings');
  const handle = page.getByRole('button', { name: 'Reorder Sea of Thieves', exact: true });
  await handle.scrollIntoViewIfNeeded();
  const from = (await handle.boundingBox())!;
  const first = (await page.locator('[data-game="Rust"]').boundingBox())!;
  const session = await context.newCDPSession(page);
  await session.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [{ x: from.x + 10, y: from.y + 10 }],
  });
  await session.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [{ x: first.x + 70, y: first.y + 10 }],
  });
  await session.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect
    .poll(async () => (await (await request.get('/api/settings')).json()).games_to_watch)
    .toEqual(['Sea of Thieves', 'Rust', 'The Elder Scrolls Online']);
  await context.close();
});
