# Dashboard design specification

Status: implemented and reviewed. Prepared before implementation on 26 September 2026 for the local fork at `C:\Users\benno\Desktop\repos\twitch-miner`; updated with the final choices below.

The finished UI uses one explicit settings draft/save bar, native inline campaign details,
and existing timestamped activity messages with search. It does not invent structured log
levels or a Twitch reconnect endpoint. Validation and review evidence are recorded in the
[implementation plan](./2026-09-26-redesign.md#implementation-and-validation-record).

## Direction

A quiet, dark utility for checking progress and choosing what to mine. Hierarchy comes from alignment, spacing, type weight, and thin dividers. The interface should feel like one compact desktop application, including on a phone.

- Neutral charcoal surfaces and grayscale controls throughout.
- Manrope, matching the OhneGuessr reference; one family with a few weights.
- Small MDI icons beside meaningful labels. No emoji in interface text, logs, empty states, or notifications.
- Flat, bounded sections and compact rows. No gradients, glass, ornamental hero section, oversized numbers, colored status pills, or grid of unrelated dashboard cards.
- Original game/reward artwork remains recognizable in small thumbnails. Its original colors are content, not an accent palette for the interface.
- Dark mode is the sole initial theme. Retire the old light/dark setting through an explicit migration note.

Critique to resolve in the first browser prototype: dark minimal interfaces can become too faint, too empty, or ambiguous. Keep readable text contrast, visible keyboard focus, explicit status wording, and comfortable targets. Use the real campaign/queue density in fixtures; do not judge a mostly empty mockup.

## Reference: OhneGuessr controls

Read-only reference: `C:\Users\benno\Desktop\repos\freeguessr\ohneguessr`.

| Reference file | What to carry over |
| --- | --- |
| `frontend/src/features/map-library/MapSearch.svelte` | 36px field, compact inset icon, trailing clear control, 4px corners |
| `frontend/src/features/map-sync/sync.css` | Flat input background, consistent label/field spacing |
| `frontend/src/components/SelectControl.svelte` | Native select with a small current-color chevron |
| `frontend/src/components/CustomNumberInput.svelte` | Restrained numeric editing, explicit commit/cancel behavior |
| `frontend/src/components/ToggleSwitch.svelte` | Native checkbox semantics and compact switch geometry |
| `frontend/src/styles/theme.css` | Neutral `dark-mode` surfaces and text hierarchy |
| `frontend/src/styles/base.css` | Self-hosted Manrope and inherited control typography |

The reference uses Svelte and custom CSS. Reproduce the visual rules in React/Tailwind; do not transplant its framework, launcher behavior, green theme, or component internals. Preserve font/icon licenses when sourcing assets.

## Tokens

Define these once in the frontend stylesheet using Tailwind's CSS theme mechanism. Components use semantic utilities such as `bg-field`, `text-muted`, and `border-divider`; pages do not invent new hex values.

| Token | Initial value | Use |
| --- | --- | --- |
| `canvas` | `#181818` | Page background |
| `surface` | `#202020` | Sidebar, bounded content |
| `field` | `#242424` | Text fields and selects |
| `raised` | `#292929` | Dialogs and selected navigation |
| `hover` | `#333333` | Interactive row/button hover |
| `divider` | `#363636` | Decorative separators |
| `control-border` | `#717171` | Necessary input/checkbox boundaries |
| `focus` | `#c8c8c8` | Keyboard focus outline |
| `text` | `#f4f4f5` | Main text |
| `soft` | `#e4e4e7` | Field values and secondary headings |
| `muted` | `#a1a1aa` | Supporting text and placeholders |
| `primary` | `#e4e4e7` | Main action background, with charcoal text |

The field geometry/background match OhneGuessr. The necessary control outline is deliberately more legible than its faint decorative borders: `#717171` against `#242424` is approximately 3.18:1. Keep divider and control-border tokens separate. Test actual rendered combinations, including disabled states and autofill.

Errors use an MDI alert icon, an explicit sentence, and a field outline; they do not depend on red. Successful saves use a small check icon and text. Selection uses a neutral fill and stronger type, not a colored badge.

| Property | Rule |
| --- | --- |
| Typeface | Self-hosted Manrope variable WOFF2, system sans fallback |
| Page title | 22px / 30px, weight 600 |
| Section title | 15px / 22px, weight 600 |
| Body and control value | 15px / 22px, weight 400 or 500 |
| Metadata | 13px / 18px; use for supplementary information only |
| Spacing | 4, 8, 12, 16, 24, 32px |
| Corners | Fields/buttons 4px; outer sections/dialogs 6px |
| Borders | 1px; no doubled borders between adjacent rows |
| Icons | 18px standard; 16px inside fields; `currentColor` |
| Numbers | Tabular numerals for progress, timestamps, ranks, counts |
| Motion | 100-150ms opacity/background transitions; respect reduced motion |
| Shadows | Only a restrained dialog shadow; no lifted content cards |

Do not use an icon font. Import individual paths from `@mdi/js` into a small shared SVG component. Decorative icons are hidden from accessibility APIs; icon-only controls have labels and visible tooltips on hover/focus.

## Custom scrollbars

Match OhneGuessr's `frontend/src/styles/base.css`: 3px width and height in Chromium/WebKit, transparent track/corner, rounded thumb, and hidden arrow buttons. Use a neutral `#717171` thumb with a lighter hover state. Firefox gets `scrollbar-width: thin` and the same neutral thumb/transparent track. Apply consistently to the document, side panels, dialogs, textareas, navigation overflow, and logs. Use native scrolling; no JavaScript scroll emulation or scrollbar package. Do not hide scrollbars entirely. Preserve keyboard, wheel, touch, and forced-colors behavior, and keep padding so content never touches the thumb.
## App shell and navigation

Five routes: **Overview**, **Campaigns**, **History**, **Activity**, **Settings**. Use ordinary links so browser back/forward and direct URLs work. Help belongs under Settings > About and alongside relevant fields, replacing the current separate Help tab.

Desktop, starting at 1024px:

- A 192px left sidebar with the app name, five navigation links, and a GitHub icon at the bottom. Connection/account status belongs in Settings.
- A 56px page header with title on the left and the relevant page actions on the right.
- Content padding 24px, increasing to 32px on wide screens; inner content maximum 1440px.
- Selected navigation has a flat neutral background and medium-weight text. No pill shape.
- A server-connection warning appears below the header only while disconnected or stale. Twitch account state and browser/server connection state have different labels.
- Overview uses a main column plus a 300-340px queue column when space permits. Other pages use the full content width.

```text
+--------------------+------------------------------------------------------+
| Twitch miner       | Overview                           Refresh inventory |
|                    +------------------------------------------------------+
| Overview           | Mining                                               |
| Campaigns          | [art] Reward name                  Watching channel   |
| History            | Game / Campaign                                      |
| Activity           | ==================------------  42 / 60 min           |
| Settings           | Confirmed by Twitch 20 seconds ago                   |
|                    +----------------------------------+-------------------+
|                    | Channels                         | Up next           |
|                    | [Search channels              ]  | 1. Game           |
|                    | Channel       Game      Action   |    Campaign       |
|                    | ...                              |    Reward         |
|                    | ...                              | 2. Game           |
|                    +----------------------------------+-------------------+
| Twitch: username   | Recent activity                                     |
|                    | 14:32  Claimed Reward name                  View all |
+--------------------+------------------------------------------------------+
```

Tablet, 768-1023px: replace the sidebar with a compact horizontal navigation row. Overview sections stack; campaign details open below the selected row. No icon-only navigation rail.

Phone, 360-767px: a compact title row and a horizontally scrollable text navigation row. Page actions wrap to a second line. Use 16px outer spacing, one content column, and a minimum 44px touch target. Field values use 16px on phones to avoid focus zoom. Rows become labeled stacked records; the page itself must never require horizontal scrolling. A selected campaign expands inline instead of opening a narrow side drawer.

At every width the content order is mining status, current reward, queue, channels, recent activity. Desktop CSS may place queue/channels beside one another without changing a confusing keyboard order. Navigation and forms remain usable at 200% zoom.

## Overview

Purpose: answer what the miner is doing, what progress Twitch confirmed, and what comes next.

1. One compact mining section contains the current reward thumbnail, game/campaign, channel link, selection mode, confirmed minutes, remaining requirement, and last confirmed update.
2. A thin 6px neutral progress bar mirrors server-confirmed progress. Any estimated remaining time is labeled "Estimated". No simulated progress presented as an earned reward; only a confirmed claim moves a reward to History.
3. Automatic/manual selection is a plain sentence with a contextual "Return to automatic" action. Selecting a channel reports the resulting mode explicitly.
4. The queue groups rewards by priority game and campaign. A compact "Edit priorities" link opens the matching Settings section. Show why an item is waiting when useful: campaign starts later, prerequisite incomplete, or no eligible live channel.
5. Channels use a search field and simple rows: name, game, viewers, availability, and "Watch". A current channel has a leading indicator plus the word "Watching". Eligibility is rechecked by the server when selected.
6. Recent activity shows the last few meaningful events and a link to Activity. Logs no longer occupy half the main page.

No speculative earnings charts, uptime tiles, reward scores, or stream embeds. The app manages one account and one currently selected watched channel, while discovering/tracking multiple candidates.

| Situation | Main content/action |
| --- | --- |
| Twitch not connected | Account link opens the actual pending device-code flow in Settings |
| No games selected | "Choose games to start mining" and Edit priorities |
| Inventory loading | Loading text inside the stable application shell |
| No eligible live channels | Explain the wait and offer Refresh inventory |
| All selected rewards claimed | "No rewards left in your selected games" and Browse campaigns |
| Campaign access unavailable | Explain that Twitch did not return the catalog; retain available inventory and offer refresh |
| Browser disconnected | Keep last known data visibly stale; disable commands that need the server |
| Reconnected | Re-fetch authoritative state; remove the warning only after synchronization |

Do not collapse permission failures, an empty eligible inventory, and a broken connection into "No drops".

## Campaigns

Purpose: browse rewards, inspect requirements, and understand eligibility.

- Top row: search field and a restrained Filters button. Filters expand inline, including selected game names and an All games reset.
- Preserve existing active/upcoming/expired OR semantics; "Not linked" narrows the results. Finished and benefit-type filters retain their documented meaning.
- Default to a dense list with the game named on every campaign. Retain the compact/grid view setting with the same typography and neutral surfaces.
- Campaign row: small game art, campaign name, claimed/total watch rewards, and status at every width. Expand for timing, ordered rewards, requirements, account linking, and a link to Twitch campaign details.
- Details expand inline at every width using native details/summary. This keeps one accessible interaction and avoids a second selected-detail state.
- Clearly distinguish "Excluded by your rules", "Requires linked account", "Upcoming", "Expired", "Claimed", and "Available". A filter hiding a reward must not change whether it is mineable.
- Linking actions are normal validated external links. The application does not pretend it can link third-party game accounts itself.
- Zero-minute subscription-only rewards do not enter the timed mining queue. Prerequisite rewards and shared prerequisites remain understandable.

## History

Purpose: a reliable record of confirmed claims.

- Compact summary text for total claims and filtered result count; retain existing by-game/month statistics in a small expandable summary, not a dashboard chart.
- Search/filter controls: game and a native date field labeled "Since (UTC)" to preserve current date-only filter semantics. Display timestamps in the user's locale with timezone available in the column heading or tooltip.
- Rows: reward, game/campaign, claimed date/time, and required watch time. On phones, render those same fields as stacked records.
- Render 25 records per page with next/previous controls and server ordering. Retain the existing full filtered API response; keep filters in the URL and apply them to exports. Server pagination can be added if personal-use histories become large.
- Retain CSV export and JSON history export where currently available. Neutral Export menu or adjacent text buttons.
- "Clear local history" is a secondary button with a dialog explaining what will be deleted and that Twitch rewards are unaffected. Failed deletion remains visible inside the modal.
- The result count and empty message identify an empty view. A failed request gets an explicit error and retry action.

## Activity

Purpose: troubleshoot the miner without making the rest of the UI look like a terminal.

- Existing timestamped messages appear in compact searchable rows. The backend supplies strings without reliable structured levels, so do not invent a level filter or duplicate event model.
- Preserve the backend's localized messages; render their text safely and remove decorative emoji from presentation.
- Keep a bounded in-memory list, collapse consecutive identical waiting messages, and avoid unbounded DOM growth.
- Follow new entries only while the user is already at the bottom. When scrolled upward, offer a "New entries" button.
- Show refresh/login/reconnect/claim failures with meaningful context. Redact cookies, authorization headers, passwords, device codes, bot tokens, and credentials in proxy URLs.
- No emoji, colored log rainbow, perpetual animated terminal, or secret-bearing debug dumps.

## Settings

One route with anchored groups and a compact section index on desktop; the same groups stack on mobile. Avoid nested navigation systems.

| Group | Contents |
| --- | --- |
| Account | Twitch identity, connection state, logout and actual pending device-code authorization; dashboard logout is separate |
| Mining | All discovered games with ordered priorities, benefit types, ignored drop names, refresh interval |
| Connection | Connection quality, proxy field and verification result |
| Dashboard access | Enable/disable password, change password, remember-me explanation, session consequences |
| Maintenance and About | Refresh inventory, clear derived cache, advanced shutdown, version, help, source/license links |

Mining and connection changes autosave after a short debounce. Serialize writes while leaving fields editable, retain pending edits across routes, and show saving/saved/failed status with Retry. The original revision detects concurrent edits; retries apply only edited fields. Dashboard password controls keep their explicit separate actions. English is the only interface language.

All discovered games are included automatically; saved games set priority. Show game artwork and a subtle six-dot drag handle. Support mouse, touch, arrow-key reordering and Escape cancellation, and save only when dropped. Do not show numbers or arrow buttons. Search and Add preserve manual names with explicit confirmation; exact or unique partial matches may resolve automatically.

Ignored drop names use one shared multiline field with one literal substring per line. Explain case-insensitive matching, prerequisite effects, and that Twitch may still incidentally credit excluded rewards. Do not turn this into an unexplained regex editor.

Maintenance: refreshing inventory is routine. Clearing cache describes that settings, authentication, and claimed history survive. Shutdown is an advanced action with a concrete confirmation explaining loss of dashboard availability and deployment-specific restart behavior. It is not labeled "Pause".

## Shared controls

Keep shared controls in `frontend/src/components/ui.tsx`: Button, Icon, Field, Input, Check, Dialog, ProgressBar and the small feedback/artwork helpers used by pages. Textarea and Select remain native elements styled with the shared field class. Avoid unused wrappers.

### Fields

- Desktop height 36px; 10px horizontal padding; 4px radius; 1px border; flat `field` background. Native select uses the same height, typography, and border.
- Visible label above every field, separated by 6-8px. Placeholder is an example, never the only label.
- Help and errors appear below the field and are connected with `aria-describedby`; invalid fields set `aria-invalid`.
- Search has a 16px MDI magnifier at the leading edge and an accessible trailing Clear control only when nonempty. Escape clears search without losing focus.
- Consistent normal, hover, focus, disabled, invalid, readonly, autofill, dirty, and saving states. Readonly values remain selectable.
- Keyboard focus uses subtle backgrounds and borders without a surrounding ring. Preserve system focus outlines in forced-colors mode.
- Textarea shares the same padding/corners/type and can be resized vertically. Use enough initial rows for rules rather than a tiny message box.
- Numeric fields use native number behavior plus schema validation; Enter commits, Escape restores the last saved value where inline editing is used. Hiding decorative spinners must not remove keyboard support.
- Native selects for short choices, native dates for history, and native form/checkbox semantics. Avoid custom dropdown/listbox libraries for these controls.
- Password/token fields support password-manager/autocomplete semantics appropriate to their purpose. Any reveal control has an explicit accessible name; server-held secrets never travel back just for display.

### Actions, dialogs, and lists

- Primary action: neutral light background and dark text. Secondary: dark surface with border. Tertiary: text/icon with a restrained hover background.
- Use one primary action per task region. Repeated table actions stay quiet. Button dimensions align with fields; touch targets grow on phones.
- Native `<dialog>` where a modal is actually needed. Provide title/description, focus placement, Escape/cancel, and focus restoration. Danger wording names the exact local data/action.
- Lists use separators, explicit empty states, real headings, and consistent thumbnail sizes (32-40px in rows, up to 64px for the active reward).
- Reward/game artwork has meaningful alt text when it adds information; otherwise empty alt. Missing artwork uses a neutral MDI placeholder.
- Loading keeps layout stable. Action errors remain at the action/field; no toast storm. A persistent service issue gets a single inline banner.

## Interaction, accessibility, and localization acceptance

1. Every path works using the keyboard, with a visible focus indicator and correct reading order. No hover-only action, drag-only ordering, or icon-only navigation.
2. Main text meets 4.5:1 contrast; necessary control boundaries/focus indicators meet applicable 3:1 requirements. Decorative dividers can remain subtle.
3. All interface and miner messages use one English catalog; verify key/placeholder consistency.
4. Stress-test long international game/channel names, 200% zoom, and a 360px viewport.
5. Errors, pending states, and operation results are announced appropriately. Frequent progress/log changes must not flood a screen reader's live region.
6. No unsafe HTML from translations, Twitch, or logs. External URLs are validated and rendered with normal safe link behavior.
7. Browser/server reconnect cannot erase unsaved fields, duplicate toasts, lose priority edits, or report success for a command that never reached the server.
8. Multiple browsers see authoritative changes without overwriting pending local edits silently.

## Visual review before feature migration

Build the shell, shared field family, Overview, and a Settings group with offline fixtures first. Review all of these together:

- Desktop at 1440x900 and 1280x800; phone at 390x844 and 360px wide; tablet at 820px.
- Normal data, many campaigns, long names, missing images, a full priority list, and non-Latin text.
- Keyboard focus, invalid fields, autofill, readonly/disabled states, saved/unsaved forms, and destructive confirmation.
- Logged out, loading, empty, disconnected, Twitch-access failure, and reconnect recovery.

Review questions: Do all fields obviously belong to the same system? Are rows aligned? Is progress immediately understandable? Are secondary controls quiet? Is the first screen useful without scrolling past decorative material? Compare field geometry directly to OhneGuessr, then correct the shared primitives once before migrating the remaining pages.
