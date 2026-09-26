import { describe, expect, it } from 'vitest';
import fixture from './fixture.json' with { type: 'json' };
import { safeUrl, moveGame } from '../src/lib/api';
import { matchesCampaign } from '../src/pages/Campaigns';
import { plainText, translator } from '../src/lib/i18n';
import { upsert } from '../src/lib/state';
import type { Snapshot } from '../src/lib/types';
const snapshot: Snapshot = fixture;
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
    expect(matchesCampaign({ ...campaign, mining_finished: true }, filters, '')).toBe(false);
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
