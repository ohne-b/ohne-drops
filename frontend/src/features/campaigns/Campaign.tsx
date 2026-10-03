import { Icon } from '@mdi/react';
import { mdiDockRight } from '@mdi/js';
import type { ReactNode } from 'react';
import type { Campaign as CampaignData } from '../../shared/lib/types';
import { useT } from '../../shared/lib/i18n';
import { Art, dateTime } from '../../shared/ui/index';

export function Campaign({
  campaign,
  action,
  onOpen,
  selected,
}: {
  campaign: CampaignData;
  action?: ReactNode;
  onOpen: () => void;
  selected: boolean;
}) {
  const t = useT();
  return (
    <article className={`campaign-summary ${selected ? 'selected' : ''}`}>
      <button
        type="button"
        id={`campaign-open-${campaign.id}`}
        onClick={onOpen}
        className="campaign-open"
        aria-label={t('inspect_campaign', { campaign: campaign.name })}
        title={t('campaign_details')}
        aria-current={selected ? 'true' : undefined}
        aria-controls={selected ? 'campaign-details' : undefined}
      >
        <Art url={campaign.game_box_art_url} className="size-12" />
        <span className="min-w-0 flex-1 text-start">
          <span className="campaign-title block font-medium text-text">{campaign.name}</span>
          <span className="muted mt-1 block">{campaign.game_name}</span>
          <span className="muted mt-1 block text-xs">
            {t(campaign.upcoming ? 'gui.inventory.starts' : 'gui.inventory.ends', {
              time: dateTime(campaign.upcoming ? campaign.starts_at : campaign.ends_at),
            })}
          </span>
        </span>
        <span className="shrink-0 text-end text-[13px]">
          <span className="block text-soft tabular-nums">
            {campaign.claimed_drops} / {campaign.total_drops}
          </span>
          <span className="muted block">
            {t(
              `gui.inventory.status.${campaign.expired ? 'expired' : campaign.upcoming ? 'upcoming' : 'active'}`,
            )}
          </span>
        </span>
        <span className="campaign-detail-icon" aria-hidden="true">
          <Icon path={mdiDockRight} className="mdi-icon" />
        </span>
      </button>
      {action && <div className="campaign-action">{action}</div>}
    </article>
  );
}
