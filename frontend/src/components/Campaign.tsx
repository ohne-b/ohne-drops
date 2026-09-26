import { mdiChevronDown, mdiOpenInNew } from '@mdi/js';
import type { Campaign as CampaignData } from '../lib/types';
import { safeUrl } from '../lib/api';
import { useT } from '../lib/i18n';
import { Art, Icon, ProgressBar, dateTime } from './ui';
export function Campaign({ campaign }: { campaign: CampaignData }) {
  const t = useT();
  const status = !campaign.linked
    ? t('gui.inventory.filters.not_linked')
    : t(
        `gui.inventory.status.${campaign.expired ? 'expired' : campaign.upcoming ? 'upcoming' : 'active'}`,
      );
  return (
    <details className="group border-b border-divider last:border-0">
      <summary className="flex list-none items-center gap-3 px-4 py-4 hover:bg-field">
        <Art url={campaign.game_box_art_url} />
        <div className="min-w-0 flex-1">
          <p className="font-medium">{campaign.name}</p>
          <p className="muted">{campaign.game_name}</p>
          <p className="muted mt-1 sm:hidden">
            {campaign.claimed_drops} / {campaign.total_drops} {t('gui.inventory.claimed_drops')} ·{' '}
            {status}
          </p>
        </div>
        <div className="hidden text-end text-[13px] sm:block">
          <p>
            {campaign.claimed_drops} / {campaign.total_drops} {t('gui.inventory.claimed_drops')}
          </p>
          <p className="text-muted">{status}</p>
        </div>
        <Icon
          path={mdiChevronDown}
          className="text-muted transition-transform group-open:rotate-180"
        />
      </summary>
      <div className="space-y-4 border-t border-divider bg-canvas/40 p-4 md:px-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="muted">
            {t(campaign.upcoming ? 'gui.inventory.starts' : 'gui.inventory.ends', {
              time: dateTime(campaign.upcoming ? campaign.starts_at : campaign.ends_at),
            })}
          </p>
          <div className="flex gap-4">
            {!campaign.linked && safeUrl(campaign.link_url) && (
              <a
                href={safeUrl(campaign.link_url)}
                target="_blank"
                rel="noreferrer"
                className="text-link text-[13px]"
              >
                {t('link_account')}
              </a>
            )}
            {safeUrl(campaign.campaign_url) && (
              <a
                href={safeUrl(campaign.campaign_url)}
                target="_blank"
                rel="noreferrer"
                className="flex items-center gap-1 text-link text-[13px]"
              >
                {t('details')}
                <Icon path={mdiOpenInNew} className="size-3.5" />
              </a>
            )}
          </div>
        </div>
        <div className="divide-y divide-divider">
          {campaign.drops.map((drop) => (
            <div key={drop.id} className="flex items-start gap-3 py-4">
              <Art url={drop.benefits[0]?.image_url} />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap justify-between gap-2">
                  <p className="font-medium">{drop.name}</p>
                  <span className="muted">
                    {drop.is_claimed
                      ? t('gui.inventory.status.claimed')
                      : drop.is_ignored
                        ? t('gui.inventory.status.ignored')
                        : drop.is_skipped
                          ? t('gui.inventory.status.skipped')
                          : t('minutes_progress', {
                              current: drop.confirmed_minutes ?? 0,
                              total: drop.required_minutes,
                            })}
                  </span>
                </div>
                <p className="muted mt-1">
                  {drop.benefits.map((benefit) => benefit.name).join(', ')}
                </p>
                {!drop.is_claimed && !drop.is_ignored && !drop.is_skipped && (
                  <div className="mt-3">
                    <ProgressBar
                      current={drop.confirmed_minutes ?? 0}
                      total={drop.required_minutes}
                      label={drop.name}
                    />
                  </div>
                )}
                {drop.ignored_keyword && (
                  <p className="muted mt-2">
                    {t('gui.inventory.ignored_keyword_reason', { keyword: drop.ignored_keyword })}
                  </p>
                )}
                {drop.ignored_precondition && (
                  <p className="muted mt-2">
                    {t('gui.inventory.ignored_precondition_reason', {
                      drop: drop.ignored_precondition,
                    })}
                  </p>
                )}
                {drop.is_skipped && (
                  <p className="muted mt-2">{t('gui.inventory.skipped_branch_reason')}</p>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>
    </details>
  );
}
