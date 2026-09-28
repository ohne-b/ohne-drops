# Changelog

## [v1.1.4](https://github.com/ohne-b/twitch-drops-miner/releases/tag/v1.1.4) — 2026-09-28

- restore 15-second idle HTTP connection expiry so minute-spaced watch events do not reuse long-idle connections
- retry transient watch connection failures and HTTP 429/5xx responses promptly with bounded, cancellable backoff instead of immediately waiting for the next watch minute
- preserve the same watch payload across retries, stop after acknowledgement, and retain existing recovery when attempts are exhausted

Existing settings, credentials, data and container configuration remain compatible.
These changes restore transport behavior from the Python implementation; they do not
guarantee that Twitch credits watch time or establish the cause of every disconnect.

[Compare v1.1.3...v1.1.4](https://github.com/ohne-b/twitch-drops-miner/compare/v1.1.3...v1.1.4)

## [v1.1.3](https://github.com/ohne-b/twitch-drops-miner/releases/tag/v1.1.3) — 2026-09-28

- preserve valid Twitch logins when public channel pages or settings scripts return HTTP 401/403
- resume stalled rewards after a successful inventory refresh by clearing stale estimates while preserving confirmed progress and newer account evidence

Existing settings, credentials, data and container configuration remain compatible.
Device-code login is unchanged. Recovery does not guarantee that Twitch credits watch time.

[Compare v1.1.2...v1.1.3](https://github.com/ohne-b/twitch-drops-miner/compare/v1.1.2...v1.1.3)

## [v1.1.2](https://github.com/ohne-b/twitch-drops-miner/releases/tag/v1.1.2) — 2026-09-28

- discard failed watch beacon addresses and automatically renew Twitch connections after three consecutive watch failures ([#28](https://github.com/ohne-b/twitch-drops-miner/pull/28))
- import Twitch-confirmed rewards into History, including badges claimed outside the miner; label unknown claim times as first seen and keep cleared entries cleared
- preserve confirmed claims when inventory refresh overlaps progress updates, preventing awarded rewards from remaining eligible because of stale local state

Existing settings, credentials, data and container configuration remain compatible.
History imports require available campaign metadata; CSV exports add a final
`claimed_at_is_observed` column. Advanced diagnostics remain off by default.

[Compare v1.1.1...v1.1.2](https://github.com/ohne-b/twitch-drops-miner/compare/v1.1.1...v1.1.2)

## [v1.1.1](https://github.com/ohne-b/twitch-drops-miner/releases/tag/v1.1.1) — 2026-09-28

- add opt-in automatic badge and emote mining across games, including required prerequisite drops
- keep selected games first, then prioritize other matching campaigns by soonest expiry; preserve manual watching, benefit filters and ignore rules

Both new options default off under Settings > Mining. Existing selections, settings,
credentials, data and container configuration remain compatible.

[Compare v1.1.0...v1.1.1](https://github.com/ohne-b/twitch-drops-miner/compare/v1.1.0...v1.1.1)

## [v1.1.0](https://github.com/ohne-b/twitch-drops-miner/releases/tag/v1.1.0) — 2026-09-28

- add campaign sorting by Default, Newest, Ending Soonest, Most Drops and A-Z; move the campaign count into the heading and Clear filters inside Filters ([#24](https://github.com/ohne-b/twitch-drops-miner/pull/24))
- improve server-side diagnostics for upstream failures, invalid responses and incomplete catalog refreshes ([#23](https://github.com/ohne-b/twitch-drops-miner/pull/23))
- add opt-in advanced diagnostics with bounded, redacted JSON previews and nested transport causes; enable with `TDM_DIAGNOSTICS=true` or `--diagnostics`, independently of ordinary verbosity ([#25](https://github.com/ohne-b/twitch-drops-miner/pull/25))

Existing settings, credentials, progress and history remain compatible. Advanced diagnostics
are off by default and appear only in server logs, never in the dashboard.

## [v1.0.0](https://github.com/ohne-b/twitch-drops-miner/releases/tag/v1.0.0) — 2026-09-27

- restore campaign discovery through the public SunkwiBOT catalog while keeping device-code login and private account inventory with Twitch ([#20](https://github.com/ohne-b/twitch-drops-miner/pull/20))
- show eligible selected-game channels and allow manual watching by channel name or URL, with an optional timer back to automatic mode ([#18](https://github.com/ohne-b/twitch-drops-miner/pull/18), [#19](https://github.com/ohne-b/twitch-drops-miner/pull/19))
- show inventory refresh progress, completion and retry inside the button; remove redundant preparation and settings-save messages ([#19](https://github.com/ohne-b/twitch-drops-miner/pull/19), [#20](https://github.com/ohne-b/twitch-drops-miner/pull/20), [#21](https://github.com/ohne-b/twitch-drops-miner/pull/21))
- use Twitch Drops Miner branding, the new logo and a concise README with a dashboard preview ([#15](https://github.com/ohne-b/twitch-drops-miner/pull/15), [#16](https://github.com/ohne-b/twitch-drops-miner/pull/16), [#18](https://github.com/ohne-b/twitch-drops-miner/pull/18))
- publish amd64/arm64 images exclusively at `ghcr.io/ohne-b/twitch-drops-miner`, retaining `latest.json` release notices without a dashboard updater ([#17](https://github.com/ohne-b/twitch-drops-miner/pull/17), [#18](https://github.com/ohne-b/twitch-drops-miner/pull/18))
- adopt PolyForm Strict 1.0.0 for the project, preserve upstream MIT and asset notices, and remove redundant guides and archived plans ([#21](https://github.com/ohne-b/twitch-drops-miner/pull/21))

Update the image name when upgrading from 0.1.0. Existing settings, credentials and history
remain compatible. Earlier published copies retain their original license terms.
The public catalog is a third-party metadata source whose coverage can vary; it receives
no Twitch credentials or account identifiers.

## [v0.1.0](https://github.com/ohne-b/twitch-drops-miner/releases/tag/v0.1.0) — 2026-09-26

- introduce the standalone Rust backend with the React dashboard and original Twitch device-code login
- keep mining opt-in, order active campaigns with progress first, and retain completed campaigns in Finished
- preserve settings, mining selections, claim history and dashboard password protection across the migration
- add release availability notices in Settings > Maintenance and publish `latest.json` with every release
- publish multi-architecture images to GHCR, with optional Docker Hub publication from the same build
- remove Telegram, retire the previous backend and simplify repository setup and documentation

This starts the project's release sequence at 0.1.0. Builds carrying the inherited 1.3.2
development version need a manual upgrade to this release. One fresh Twitch device-code
login is required when migrating from the previous backend. Existing data and credentials
are preserved; updates are installed from the terminal.
