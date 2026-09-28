import { describe, expect, it } from 'vitest';
import fixture from './fixture.json' with { type: 'json' };
import { safeUrl, moveGame } from '../src/lib/api';
import { matchesCampaign, campaignOrder } from '../src/pages/Campaigns';
import { plainText, translator } from '../src/lib/i18n';
import { upsert } from '../src/lib/state';
import type { Snapshot } from '../src/lib/types';
const snapshot: Snapshot = fixture;
it('orders active confirmed progress before other available campaigns', () => {
  const progress = snapshot.campaigns[0]!;
  const untouched = {
    ...progress,
    id: 'untouched',
    drops: progress.drops.map((drop) => ({ ...drop, confirmed_minutes: 0 })),
  };
  const upcoming = { ...untouched, id: 'upcoming', active: false, upcoming: true };
  const claimed = {
    ...untouched,
    id: 'claimed',
    drops: [{ ...untouched.drops[0]!, is_claimed: true }],
  };
  expect([upcoming, untouched, progress, claimed].sort(campaignOrder).map((c) => c.id)).toEqual([
    progress.id,
    'claimed',
    'untouched',
    'upcoming',
  ]);
});

it('sorts campaigns by start, end, total drops or name with deterministic default ties', () => {
  const original = snapshot.campaigns[0]!;
  const campaigns = [
    {
      ...original,
      id: 'a',
      name: 'Zeta',
      starts_at: '2026-09-01T00:00:00Z',
      ends_at: '2026-09-09T00:00:00Z',
      total_drops: 2,
    },
    {
      ...original,
      id: 'b',
      name: 'Alpha',
      starts_at: '2026-09-02T02:00:00+02:00',
      ends_at: '2026-09-08T00:00:00Z',
      total_drops: 5,
    },
    {
      ...original,
      id: 'c',
      name: 'beta',
      starts_at: '2026-09-02T01:00:00Z',
      ends_at: '2026-09-10T00:00:00Z',
      total_drops: 3,
    },
  ];
  const order = (sort: Parameters<typeof campaignOrder>[2]) =>
    [...campaigns].sort((a, b) => campaignOrder(a, b, sort)).map((campaign) => campaign.id);
  expect(order('default')).toEqual(['b', 'a', 'c']);
  expect(order('newest')).toEqual(['c', 'b', 'a']);
  expect(order('ending')).toEqual(['b', 'a', 'c']);
  expect(order('drops')).toEqual(['b', 'c', 'a']);
  expect(order('name')).toEqual(['b', 'c', 'a']);
  const tied = campaigns.map((campaign) => ({ ...campaign, total_drops: 5 }));
  expect(
    tied
      .reverse()
      .sort((a, b) => campaignOrder(a, b, 'drops'))
      .map((campaign) => campaign.id),
  ).toEqual(['b', 'a', 'c']);
  expect(campaigns.map((campaign) => campaign.id)).toEqual(['a', 'b', 'c']);
});
describe('boundary behavior', () => {
  it('rejects executable and credential-bearing URLs', () => {
    for (const url of [
      'javascript:alert(1)',
      'data:text/html,test',
      'https://user:secret@example.com',
      '/relative',
    ])
      expect(safeUrl(url)).toBeUndefined();
    expect(safeUrl('https://twitch.tv/test')).toBe('https://twitch.tv/test');
  });
  it('moves priorities without losing or duplicating games, rejecting invalid ranks', () => {
    expect(moveGame(['A', 'B', 'C'], 0, 99)).toEqual(['B', 'C', 'A']);
    expect(moveGame(['A', 'B'], 1, -99)).toEqual(['B', 'A']);
    expect(moveGame(['A', 'B'], 0, 1.5)).toEqual(['A', 'B']);
  });
  it('updates live records by ID instead of accumulating copies', () => {
    expect(upsert([{ id: 1, name: 'old' }], { id: 1, name: 'new' })).toEqual([
      { id: 1, name: 'new' },
    ]);
  });
  it('localizes with English fallback and preserves literal user text', () => {
    expect(translator({})('watching', { channel: '<b>name</b>' })).toBe('Watching <b>name</b>');
    expect(plainText('🎮 Active ✔')).toBe('Active');
  });
  it('combines campaign statuses with OR and link filtering with AND', () => {
    const campaign = snapshot.campaigns[0]!;
    const filters = snapshot.settings.inventory_filters;
    expect(matchesCampaign(campaign, { ...filters, show_upcoming: true }, '')).toBe(true);
    expect(matchesCampaign(campaign, { ...filters, show_only_not_linked: true }, '')).toBe(false);
    expect(
      matchesCampaign(
        { ...campaign, linked: null },
        { ...filters, show_only_not_linked: true },
        '',
      ),
    ).toBe(false);
    expect(matchesCampaign({ ...campaign, finished: true }, filters, '')).toBe(false);
    expect(
      matchesCampaign(
        { ...campaign, finished: true, active: false, expired: true },
        filters,
        '',
        true,
      ),
    ).toBe(true);
    expect(matchesCampaign({ ...campaign, active: false, expired: true }, filters, '', true)).toBe(
      false,
    );
    expect(matchesCampaign({ ...campaign, mining_finished: true }, filters, '')).toBe(true);
    expect(matchesCampaign({ ...campaign, drops: [] }, filters, '')).toBe(true);
    expect(matchesCampaign(campaign, { ...filters, game_name_search: ['RUST'] }, '')).toBe(true);
    expect(
      matchesCampaign(
        campaign,
        {
          ...filters,
          show_benefit_badge: false,
          show_benefit_emote: false,
          show_benefit_item: false,
          show_benefit_other: false,
        },
        '',
      ),
    ).toBe(false);
  });
});
