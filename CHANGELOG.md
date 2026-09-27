# Changelog

## [v0.1.1](https://github.com/ohne-b/twitch-drops-miner/releases/tag/v0.1.1) — 2026-09-27

- show only eligible selected-game channels and prioritize special-event campaigns correctly
- add manual channel entry by Twitch name or URL, with explicit game selection and eligibility checks
- refresh changed channels promptly, retry failures, preserve manual choices during refresh, and send smaller viewer updates
- rename the product to Twitch Drops Miner and enlarge the dashboard's Drops Miner title
- publish only to `ghcr.io/ohne-b/twitch-drops-miner`, retaining old images for rollback
- add the shared purple logo, refresh documentation, and preserve upstream license notices

Keep the existing `twitch-drops-miner` Compose service, container name and data/log mounts.
Source installations now use the `twitch-drops-miner` executable. No new login or data
migration is required from 0.1.0. The historical `ghcr.io/ohne-b/twitch-miner:0.1.0`
image remains available; change the image address to receive this release.

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
