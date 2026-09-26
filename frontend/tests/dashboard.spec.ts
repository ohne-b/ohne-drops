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
  await expect(page.getByText('Watching: northwind', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Watching northwind', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Recent activity', exact: true })).toHaveCount(0);
  await expect(page.getByRole('progressbar', { name: 'Explorer jacket' })).toHaveAttribute(
    'aria-valuenow',
    '42',
  );
  await expect(page.getByText('48 / 60 min')).toHaveCount(0);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await expect(page.getByText('Twitch: 123456', { exact: true })).toBeVisible();
  const github = page.getByRole('link', { name: 'GitHub repository' }).locator('svg');
  const githubBox = (await github.boundingBox())!;
  expect(githubBox.width).toBe(32);
  const accountBox = (await page.getByText('Twitch: 123456', { exact: true }).boundingBox())!;
  expect(githubBox.y + githubBox.height).toBeLessThan(accountBox.y);
  const channels = page
    .locator('section')
    .filter({ has: page.getByRole('heading', { name: 'Channels', exact: true }) });
  const queue = page
    .locator('section')
    .filter({ has: page.getByRole('heading', { name: 'Up next', exact: true }) });
  expect((await channels.boundingBox())?.width).toBe((await queue.boundingBox())?.width);
  expect((await channels.boundingBox())?.height).toBe((await queue.boundingBox())?.height);
  await expect(page.getByText('Confirmed by Twitch', { exact: true })).toHaveCount(0);
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
test('Up next scrolls within its panel with reward artwork and safe fallbacks', async ({
  page,
  request,
}) => {
  const png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==';
  await page.route('https://art.example/**', (route) =>
    route.request().url().endsWith('broken.png')
      ? route.abort()
      : route.fulfill({ contentType: 'image/png', body: Buffer.from(png, 'base64') }),
  );
  const game = snapshot.wanted_items[0]!;
  await request.post('/__test/event', {
    headers,
    data: {
      event: 'channels_batch_update',
      data: {
        channels: Array.from({ length: 30 }, (_, i) => ({
          ...snapshot.channels[0]!,
          id: i + 1,
          name: `channel-${i}`,
          watching: i === 0,
        })),
      },
    },
  });
  await request.post('/__test/event', {
    headers,
    data: {
      event: 'wanted_items_update',
      data: [
        {
          ...game,
          campaigns: [
            {
              ...game.campaigns[0],
              drops: Array.from({ length: 30 }, (_, i) => ({
                name: `Reward ${i}`,
                benefits: [`Reward ${i}`],
                image_url:
                  i === 0
                    ? 'https://art.example/reward.png'
                    : i === 1
                      ? 'https://art.example/broken.png'
                      : '',
              })),
            },
          ],
        },
      ],
    },
  });
  const panel = page.getByRole('region', { name: 'Up next', exact: true });
  const channelPanel = page.getByRole('region', { name: 'Channels', exact: true });
  const channels = page.locator('section').filter({ has: channelPanel });
  const queue = page.locator('section').filter({ has: panel });
  await expect(panel.locator('li')).toHaveCount(30);
  await expect(panel.locator('li').first().locator('img')).toHaveAttribute(
    'src',
    'https://art.example/reward.png',
  );
  await expect(panel.locator('li').nth(1).locator('svg')).toBeVisible();
  await expect(panel.locator('li').nth(2).locator('svg')).toBeVisible();
  for (const [width, height] of [
    [1440, 900],
    [1598, 520],
    [390, 900],
  ] as const) {
    await page.setViewportSize({ width, height });
    if (width >= 1280) {
      expect((await channels.boundingBox())!.height).toBe((await queue.boundingBox())!.height);
      expect((await channels.boundingBox())!.y).toBe((await queue.boundingBox())!.y);
    } else {
      expect((await channels.boundingBox())!.y).toBeGreaterThan((await queue.boundingBox())!.y);
    }
    for (const region of [panel, channelPanel]) {
      expect((await region.boundingBox())!.height).toBeLessThanOrEqual(
        width >= 1280 ? height : 440,
      );
      expect(
        await region.evaluate(
          (el) => el.scrollHeight > el.clientHeight && getComputedStyle(el).overflowY === 'auto',
        ),
      ).toBe(true);
      await region.focus();
      await page.keyboard.press('End');
      await expect.poll(() => region.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
  for (const region of [panel, channelPanel])
    await region.evaluate((el) => {
      el.scrollTop = 0;
    });
  await page.screenshot({ path: '../artifacts/overview-queue.png', fullPage: true });
  await page.getByRole('searchbox', { name: 'Search channels' }).fill('no matching channel');
  await expect(channelPanel.getByText('No matching results')).toBeVisible();
  await request.post('/__test/event', {
    headers,
    data: { event: 'wanted_items_update', data: [] },
  });
  await expect(panel.getByText('No wanted drops queued...')).toBeVisible();
  expect((await channels.boundingBox())!.height).toBe((await queue.boundingBox())!.height);
});

test('Overview fits the desktop viewport and only scrolls the page when space is limited', async ({
  page,
}) => {
  for (const [width, height] of [
    [1920, 945],
    [1440, 900],
    [1280, 720],
  ]) {
    await page.setViewportSize({ width: width!, height: height! });
    expect(await page.evaluate(() => document.documentElement.scrollHeight)).toBeLessThanOrEqual(
      height!,
    );
    const channels = page.getByRole('region', { name: 'Channels', exact: true });
    const queue = page.getByRole('region', { name: 'Up next', exact: true });
    expect((await channels.boundingBox())!.height).toBeGreaterThan(100);
    expect((await queue.boundingBox())!.height).toBeGreaterThan(100);
  }
  await page.setViewportSize({ width: 1440, height: 480 });
  expect(await page.evaluate(() => document.documentElement.scrollHeight)).toBeGreaterThan(480);
  await page.getByRole('region', { name: 'Channels', exact: true }).scrollIntoViewIfNeeded();
  await expect(page.getByRole('button', { name: 'Watch harbor', exact: true })).toBeVisible();
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  expect((await page.locator('aside').boundingBox())!.y).toBeCloseTo(0, 0);
  await expect(
    page.getByRole('navigation').getByRole('link', { name: 'Overview' }),
  ).toBeInViewport();
  await expect(page.getByText('Twitch: 123456', { exact: true })).toBeInViewport();
  await page.screenshot({ path: '../artifacts/overview-short-window.png', fullPage: true });
});
test('every route loads directly and stays usable on a phone', async ({ page }) => {
  await page.goto('/settings');
  const proxy = await page.getByLabel('Proxy URL', { exact: true }).boundingBox();
  const quality = await page.getByLabel('Connection Quality:', { exact: true }).boundingBox();
  expect(proxy!.y).toBeCloseTo(quality!.y, 0);
  await page.screenshot({ path: '../artifacts/settings-desktop.png', fullPage: true });
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

test('Telegram saves explicitly, reuses the masked token, tests drafts and disables alerts', async ({
  page,
  request,
}) => {
  await page.goto('/settings#telegram');
  const section = page.locator('#telegram');
  const token = section.getByLabel('Telegram Bot Token', { exact: true });
  const chat = section.getByLabel('Telegram Chat ID', { exact: true });
  const save = section.getByRole('button', { name: 'Save Settings', exact: true });
  const testConnection = section.getByRole('button', { name: 'Test Connection', exact: true });
  await expect(token).toHaveAttribute('type', 'password');
  await token.fill('123456:browser_fixture');
  await chat.fill('42');
  expect((await (await request.get('/api/settings')).json()).telegram_configured).toBe(false);
  await save.click();
  await expect(section.getByText('Telegram settings saved.', { exact: true })).toBeVisible();
  await expect(token).toHaveValue('');
  const stored = await (await request.get('/api/settings')).json();
  expect(stored.telegram_bot_token).toBe('••••••••');
  expect(JSON.stringify(stored)).not.toContain('browser_fixture');
  await expect(section.getByText('A bot token is saved.')).toBeVisible();
  await chat.fill('reject');
  await testConnection.click();
  await expect(section.getByText('Telegram connection failed.', { exact: false })).toBeVisible();
  expect((await (await request.get('/api/settings')).json()).telegram_chat_id).toBe('42');
  await chat.fill('-7');
  await testConnection.click();
  await expect(section.getByText('Telegram connection successful. Settings saved.')).toBeVisible();
  expect((await (await request.get('/api/settings')).json()).telegram_chat_id).toBe('-7');
  await page.reload();
  await expect(token).toHaveValue('');
  await expect(chat).toHaveValue('-7');
  await chat.fill('');
  await save.click();
  await expect(section.getByText('Telegram settings saved.', { exact: true })).toBeVisible();
  const disabled = await (await request.get('/api/settings')).json();
  expect(disabled.telegram_chat_id).toBe('');
  expect(disabled.telegram_configured).toBe(true);
  await expect(testConnection).toBeDisabled();
  await section.getByText('How to Set Up', { exact: true }).click();
  await expect(section.getByRole('link', { name: '@BotFather', exact: true })).toHaveAttribute(
    'href',
    'https://t.me/BotFather',
  );
  await expect(section.getByText('Open your bot in Telegram and send /start.')).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await section.scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await page.screenshot({ path: '../artifacts/telegram-settings-phone.png', fullPage: true });
});

test('Telegram retains edits on failed or conflicting saves and does not overwrite other settings', async ({
  page,
  request,
}) => {
  await page.goto('/settings#telegram');
  const section = page.locator('#telegram');
  const token = section.getByLabel('Telegram Bot Token', { exact: true });
  const chat = section.getByLabel('Telegram Chat ID', { exact: true });
  const save = section.getByRole('button', { name: 'Save Settings', exact: true });
  await token.fill('123456:retained_fixture');
  await chat.fill('42');
  await page.route('**/api/settings', (route) =>
    route.request().method() === 'POST'
      ? route.fulfill({ status: 500, json: { detail: 'failed' } })
      : route.continue(),
  );
  await save.click();
  await expect(section.getByRole('alert')).toBeVisible();
  await expect(token).toHaveValue('123456:retained_fixture');
  await expect(chat).toHaveValue('42');
  await page.unroute('**/api/settings');
  await request.post('/api/settings', { headers, data: { connection_quality: 3 } });
  await expect(page.getByLabel('Connection Quality:', { exact: true })).toHaveValue('3');
  await save.click();
  await expect(section.getByRole('button', { name: 'Try again', exact: true })).toBeVisible();
  await expect(token).toHaveValue('123456:retained_fixture');
  await section.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(section.getByText('Telegram settings saved.', { exact: true })).toBeVisible();
  await expect(token).toHaveValue('');
  expect((await (await request.get('/api/settings')).json()).connection_quality).toBe(3);
});

test('keyboard focus remains visible without outlines across controls', async ({ page }) => {
  await page.goto('/settings');
  await page.keyboard.press('Tab');
  const field = page.getByLabel('Proxy URL', { exact: true });
  const before = await field.evaluate((element) => getComputedStyle(element).borderColor);
  await field.focus();
  expect(await field.evaluate((element) => getComputedStyle(element).borderColor)).not.toBe(before);
  expect(await field.evaluate((element) => getComputedStyle(element).outlineStyle)).toBe('none');
  for (const name of ['Log out of Twitch', 'Enable password protection']) {
    const button = page.getByRole('button', { name, exact: true });
    await button.focus();
    await expect
      .poll(() => button.evaluate((element) => getComputedStyle(element).backgroundColor))
      .toBe('rgb(51, 51, 51)');
  }
  const checkbox = page.getByRole('checkbox', { name: 'Badge', exact: true });
  await checkbox.focus();
  expect(
    await checkbox.evaluate(
      (element) => getComputedStyle(element.closest('label')!).backgroundColor,
    ),
  ).toBe('rgb(51, 51, 51)');
  await page.emulateMedia({ forcedColors: 'active' });
  expect(await checkbox.evaluate((element) => getComputedStyle(element).outlineStyle)).toBe(
    'solid',
  );
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

test('offline channels with unknown viewers do not crash Overview', async ({ page, request }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/settings');
  await expect(page.getByRole('button', { name: 'Reorder Rust', exact: true })).toBeEnabled();
  const offline = {
    ...snapshot.channels[0]!,
    name: 'offline-channel',
    viewers: null,
    online: false,
    watching: false,
    game: null,
  };
  await request.post('/__test/event', {
    headers,
    data: { event: 'channel_update', data: offline },
  });
  await page.getByRole('link', { name: 'Overview', exact: true }).click();
  const row = page
    .locator('.row')
    .filter({ has: page.getByRole('link', { name: 'offline-channel', exact: true }) });
  await expect(row).toContainText('—');
  await expect(row.getByRole('button', { name: 'Watch offline-channel' })).toBeDisabled();
  await request.post('/__test/event', {
    headers,
    data: { event: 'channels_batch_update', data: { channels: [offline] } },
  });
  await expect(page.locator('.row')).toHaveCount(1);
  await expect(page.getByRole('heading', { name: 'Overview', exact: true })).toBeVisible();
  expect(errors).toEqual([]);
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
test('Finished separates completed, expired, ignored, and unverifiable historical campaigns', async ({
  page,
  request,
}) => {
  await page.goto('/campaigns');
  const original = snapshot.campaigns[0]!;
  await expect(page.getByRole('button', { name: 'Stop mining Rust', exact: true })).toBeEnabled();
  const completed = {
    ...original,
    id: 'completed',
    name: 'Completed campaign',
    finished: true,
    active: false,
    expired: true,
    claimed_drops: 2,
    drops: original.drops.map((drop) => ({ ...drop, is_claimed: true, is_mineable: false })),
  };
  const expired = {
    ...original,
    id: 'expired',
    name: 'Expired campaign',
    active: false,
    expired: true,
  };
  const ignored = { ...original, id: 'ignored', name: 'Ignored campaign', mining_finished: true };
  await request.post('/__test/event', {
    headers,
    data: {
      event: 'inventory_batch_update',
      data: { campaigns: [expired, ignored, completed, original] },
    },
  });
  await expect(page.getByText('Completed campaign', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Ignored campaign', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Clear filters', exact: true }).click();
  await expect(page.getByText('Expired campaign', { exact: true })).toBeVisible();
  await page.route('**/api/history', (route) =>
    route.fulfill({
      json: {
        entries: [
          {
            id: 'legacy',
            campaign_id: 'old',
            game: 'Rust',
            campaign: 'Historical campaign',
            drop_name: 'Old reward',
            required_minutes: 30,
            benefits: ['Old reward'],
            claimed_at: '2025-01-01T00:00:00Z',
          },
        ],
      },
    }),
  );
  await page.getByRole('link', { name: 'Finished', exact: true }).click();
  await expect(page.getByText('Completed campaign', { exact: true })).toBeVisible();
  await expect(page.getByText('Completed', { exact: true }).last()).toBeVisible();
  await expect(page.getByText('Expired campaign', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Ignored campaign', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Completion unverified', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  await expect(page.getByRole('checkbox', { name: 'Item', exact: true })).toHaveCount(0);
  await page.getByRole('checkbox', { name: 'Rust', exact: true }).check();
  await expect(page.getByText('Completion unverified', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'All games', exact: true }).click();
  await page.getByRole('searchbox', { name: 'Search campaigns and rewards' }).fill('Completed');
  await expect(page.getByRole('link', { name: 'Finished', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.getByText('Completed campaign', { exact: true })).toBeVisible();
  await page.screenshot({ path: '../artifacts/campaigns-finished.png', fullPage: true });
});
test('Finished historical claims honor game filters and retry failed loading', async ({ page }) => {
  await page.route('**/api/history', (route) => route.fulfill({ status: 500, json: {} }), {
    times: 1,
  });
  await page.goto('/campaigns?tab=finished');
  await expect(page.getByRole('alert')).toContainText('Could not load history. Try again.');
  await page.route('**/api/history', (route) =>
    route.fulfill({
      json: {
        entries: [
          {
            id: 'legacy',
            campaign_id: 'old',
            game: 'Old game',
            campaign: 'Historical campaign',
            drop_name: 'Old reward',
            required_minutes: 30,
            benefits: ['Old reward'],
            claimed_at: '2025-01-01T00:00:00Z',
          },
        ],
      },
    }),
  );
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(page.getByText('Completion unverified', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  await expect(page.getByRole('checkbox', { name: 'Old game', exact: true })).toBeVisible();
  await page.getByRole('checkbox', { name: 'Rust', exact: true }).check();
  await expect(page.getByText('Completion unverified', { exact: true })).toHaveCount(0);
  await page.getByRole('checkbox', { name: 'Old game', exact: true }).check();
  await expect(page.getByText('Completion unverified', { exact: true })).toBeVisible();
});
test('discovery stays visible without mining until Mine is explicitly selected', async ({
  page,
  request,
}) => {
  await request.post('/api/settings', { headers, data: { games_to_watch: [] } });
  await page.goto('/campaigns');
  await expect(page.getByText('Autumn expedition', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Mine Rust', exact: true }).click();
  await expect
    .poll(async () => (await (await request.get('/api/settings')).json()).games_to_watch)
    .toEqual(['Rust']);
  await page.reload();
  await expect(page.getByRole('button', { name: 'Stop mining Rust', exact: true })).toBeVisible();
  await page.goto('/settings');
  await expect(page.locator('#mining [data-game]')).toHaveCount(1);
  await page.getByRole('button', { name: /Remove Rust/ }).click();
  await expect
    .poll(async () => (await (await request.get('/api/settings')).json()).games_to_watch)
    .toEqual([]);
  await page.goto('/campaigns');
  await expect(page.getByRole('button', { name: 'Mine Rust', exact: true })).toBeVisible();
});
test('game priorities show icons instead of editable numbers', async ({ page, request }) => {
  await page.goto('/settings');
  await page.getByRole('searchbox', { name: 'Search games...' }).fill('The Elder Scrolls Online');
  await page.getByRole('button', { name: 'Add Game', exact: true }).click();
  await expect(page.getByText('Changes saved.', { exact: true })).toBeVisible();
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
  await expect(page.getByText('Dashboard connected', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Log out of Twitch', exact: true }).click();
  await expect(page.getByText('NEWCODE', { exact: true })).toBeVisible();
  await expect(page.getByText('Dashboard connected', { exact: true })).toBeVisible();
  await expect(page.getByText('Connected', { exact: true })).toHaveCount(0);
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

test('Telegram starts unconfigured and rejects invalid credentials without echoing them', async ({
  page,
  request,
}) => {
  await page.goto('/settings');
  await expect(
    page.getByRole('heading', { name: 'Telegram Notifications', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Test Connection', exact: true })).toBeDisabled();
  const result = await request.post('/api/settings', {
    headers,
    data: { telegram_bot_token: 'discard-me', telegram_chat_id: '123' },
  });
  expect(result.status()).toBe(422);
  expect(JSON.stringify(await result.json())).not.toContain('discard-me');
  expect((await (await request.get('/api/settings')).json()).telegram_configured).toBe(false);
  const testResult = await request.post('/api/settings/test-telegram', { headers, data: {} });
  expect(testResult.status()).toBe(200);
  expect(await testResult.json()).toEqual({ success: false });
});

test('history displays saved reward artwork and preserves old entries', async ({
  page,
  request,
}) => {
  const history = await (await request.get('/api/history')).json();
  await page.route('https://static-cdn.jtvnw.net/reward.png', (route) =>
    route.fulfill({
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" />',
    }),
  );
  await page.route('**/api/history?*', (route) =>
    route.fulfill({
      json: {
        total: 2,
        entries: [
          { ...history.entries[0], image_url: 'https://static-cdn.jtvnw.net/reward.png' },
          { ...history.entries[0], id: 'legacy', drop_name: 'Older reward' },
        ],
      },
    }),
  );
  await page.goto('/history');
  const image = page.locator('img[src="https://static-cdn.jtvnw.net/reward.png"]');
  await expect(image).toBeVisible();
  await expect.poll(() => image.evaluate((node: HTMLImageElement) => node.naturalWidth)).toBe(40);
  await expect(page.getByText('Older reward', { exact: true })).toBeVisible();
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
  await expect(page.getByRole('alert')).toContainText(
    'Twitch did not provide the complete campaign catalog',
  );
  await request.post('/__test/event', {
    headers,
    data: { event: 'console_output', data: { message: '<img src=x onerror="alert(1)">' } },
  });
  await page.getByRole('link', { name: 'Activity', exact: true }).click();
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
test('recovered campaigns keep unknown account linkage without a discovery banner', async ({
  page,
  request,
}) => {
  await request.post('/__test/event', {
    headers,
    data: {
      event: 'initial_state',
      data: {
        ...snapshot,
        current_drop: { ...snapshot.current_drop!, drop_name: 'Recovered reward fixture' },
        inventory_status: { available: false, recovered: 1, checked_at: null },
        campaigns: snapshot.campaigns.map((campaign) => ({ ...campaign, linked: null })),
      },
    },
  });
  await expect(page.getByText('Recovered reward fixture', { exact: true })).toBeVisible();
  await expect(page.getByText(/Found \d+ campaigns through live Twitch channels/)).toHaveCount(0);
  await page.getByRole('link', { name: 'Campaigns', exact: true }).click();
  await expect(page.getByText(/Found \d+ campaigns through live Twitch channels/)).toHaveCount(0);
  await expect(page.getByText('Account link unknown', { exact: true }).last()).toBeVisible();
  await page.locator('summary').first().click();
  await expect(page.getByRole('link', { name: 'Check account link' }).first()).toBeVisible();
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

test('explicit game priorities preserve manual spelling and case-insensitive uniqueness', async ({
  page,
  request,
}) => {
  await request.post('/api/settings', {
    headers,
    data: { games_to_watch: ['Custom game', 'rust'] },
  });
  await page.goto('/settings');
  const rows = page.locator('#mining [data-game]');
  await expect(rows).toHaveCount(2);
  expect(
    await rows.evaluateAll((items) => items.map((item) => item.getAttribute('data-game'))),
  ).toEqual(['Custom game', 'rust']);
  await expect(page.getByRole('button', { name: 'Select All', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Reorder rust', exact: true }).press('ArrowUp');
  await expect(page.getByText('Changes saved.', { exact: true })).toBeVisible();
  const games = (await (await request.get('/api/settings')).json()).games_to_watch;
  expect(games).toEqual(['rust', 'Custom game']);
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

test('empty selection asks for an explicit mining choice', async ({ page, request }) => {
  await request.post('/__test/event', {
    headers,
    data: {
      event: 'settings_updated',
      data: { ...snapshot.settings, games_to_watch: [] },
    },
  });
  await request.post('/__test/event', { headers, data: { event: 'drop_progress_stop', data: {} } });
  await expect(page.getByText('Choose Mine on a campaign to select its game.')).toBeVisible();
  await expect(page.getByText('Choose the games you want to mine.')).toHaveCount(0);
});

test('phone campaign rows retain status and claimed counts', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto('/campaigns');
  await expect(page.getByText('0 / 2 claimed · Active', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: '../artifacts/campaigns-phone.png', fullPage: true });
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
  await request.post('/api/settings', {
    headers,
    data: { games_to_watch: ['Rust', 'Sea of Thieves', 'The Elder Scrolls Online'] },
  });
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
    'The Elder Scrolls Online',
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
    .toEqual(['Sea of Thieves', 'Rust']);
  await context.close();
});
