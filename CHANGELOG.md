# Changelog

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
