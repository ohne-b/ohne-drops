# Changelog

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
