# Twitch Drops Miner

Mine timed Twitch Drops from a quiet, self-hosted dashboard without downloading stream
video or audio. Rust backend, React/TypeScript frontend, Docker support.

## Getting started

```bash
mkdir -p data logs
docker compose up -d --build
```

Open <http://127.0.0.1:8080>. In **Settings > Twitch account**, authorize the displayed
Twitch device code and confirm in the dashboard. Choose **Mine** on a campaign to start.
**Only games you select are mined.** Reorder them in Settings to set priority.

The container runs as UID/GID `1000:1000`; its data/log directories must be writable by
that user. Compose binds to loopback by default. For LAN access, set an explicit LAN
address in the port mapping and enable the dashboard password in Settings.

## Dashboard

- **Overview:** current progress, live channels and the next rewards.
- **Campaigns:** discovered campaigns, filters and Mine/Stop mining controls. Active
  campaigns with progress appear first. Clear filters if fewer results appear than expected.
- **Finished:** completed campaigns retained across restarts and refreshes. Expired
  campaigns remain distinct; older incomplete records are labeled completion unverified.
- **History:** claimed rewards, filters, statistics and CSV/JSON export.
- **Activity and Settings:** diagnostics, game priorities, ignore rules, account and access controls.

Twitch may withhold part of its campaign catalog. Live-channel discovery improves coverage,
but cannot guarantee every campaign appears. Progress confirmed by Twitch is distinguished
from local estimates. A healthy dashboard does not prove live earning.

## Updates and data

Back up `data/` and your Compose file before upgrading. For a source checkout:

```bash
git pull --ff-only
docker compose build
docker compose up -d --force-recreate
```

Restarting alone does not install new code. Preserve your existing Compose mounts, network
binding and ownership. There is no in-dashboard updater.

**Settings > Maintenance** checks for new releases and links to their release notes.
It distinguishes an available update from a failed check. Every release includes
[`latest.json`](https://github.com/ohne-b/twitch-miner/releases/latest/download/latest.json);
the [changelog](CHANGELOG.md) describes each release. Versioning starts at `0.1.0` for
this project; installations labeled `1.3.2` need one manual upgrade to this release series.

Release images for amd64 and arm64 are available on
[Docker Hub](https://hub.docker.com/r/ohneb/twitch-miner) as `ohneb/twitch-miner:VERSION`
and GHCR as `ghcr.io/ohne-b/twitch-miner:VERSION`. Both also provide `latest` for stable releases.
To use a published image with the supplied Compose file, remove `build: .`, change
`image:` to `ohneb/twitch-miner:latest`, then run `docker compose pull` and
`docker compose up -d`. Keep your existing mounts, ownership and port mapping.
Registry publishing setup is covered in
[CONTRIBUTING.md](CONTRIBUTING.md#release-and-automation).

The Rust version reads existing settings, mining selections, history, completed campaigns
and dashboard protection. **One new Twitch device-code login is required.** Existing
credential files stay untouched for rollback; live progress is restored from Twitch.
Telegram is removed. See [operations and migration details](docs/operations.md) for data
files, reverse-proxy configuration and password recovery.

## Run from source

Install [Rust through rustup](https://rustup.rs/) and Node.js 24. Windows also needs the
Visual Studio C++ build tools. The repository pins the Rust toolchain and dependency locks.

```bash
npm --prefix frontend ci
npm --prefix frontend run build
cargo run --locked -- --host 127.0.0.1
```

`cargo build --release --locked --bin twitch-miner` builds a standalone executable with
the dashboard embedded. Use `--help` for host/port/data/log options. Docker images support
amd64 and arm64 and run without Node or a second backend runtime.

[CONTRIBUTING.md](CONTRIBUTING.md) covers development, testing and releases.
[AGENTS.md](AGENTS.md) records the architecture and behavior contracts.

## License and contributors

[MIT](LICENSE), copyright 2026 ohne-b (OhneB). Based on
[rangermix/TwitchDropsMiner](https://github.com/rangermix/TwitchDropsMiner)
and its upstream contributors; the original MIT license is preserved in [NOTICE.md](NOTICE.md).
Font/icon licenses are in
[frontend/public/assets/licenses](frontend/public/assets/licenses).
This hobby project supports personal use on your own hardware and home network.

<!-- contributors:start -->
| Contributor | Merged pull requests |
| --- | --- |
| [@3lb0z0](https://github.com/3lb0z0) | [#110](https://github.com/rangermix/TwitchDropsMiner/pull/110) |
| [@birdhimself](https://github.com/birdhimself) | [#41](https://github.com/rangermix/TwitchDropsMiner/pull/41) |
| [@capkz](https://github.com/capkz) | [#70](https://github.com/rangermix/TwitchDropsMiner/pull/70) |
| [@EthanBlazkowicz](https://github.com/EthanBlazkowicz) | [#33](https://github.com/rangermix/TwitchDropsMiner/pull/33) |
| [@Klages](https://github.com/Klages) | [#94](https://github.com/rangermix/TwitchDropsMiner/pull/94) · [#95](https://github.com/rangermix/TwitchDropsMiner/pull/95) |
| [@Knight-sys](https://github.com/Knight-sys) | [#3](https://github.com/rangermix/TwitchDropsMiner/pull/3) |
| [@ohne-b](https://github.com/ohne-b) | [#1](https://github.com/ohne-b/twitch-miner/pull/1) · [#2](https://github.com/ohne-b/twitch-miner/pull/2) · [#3](https://github.com/ohne-b/twitch-miner/pull/3) · [#4](https://github.com/ohne-b/twitch-miner/pull/4) · [#5](https://github.com/ohne-b/twitch-miner/pull/5) · [#6](https://github.com/ohne-b/twitch-miner/pull/6) · [#7](https://github.com/ohne-b/twitch-miner/pull/7) · [#8](https://github.com/ohne-b/twitch-miner/pull/8) · [#9](https://github.com/ohne-b/twitch-miner/pull/9) · [#10](https://github.com/ohne-b/twitch-miner/pull/10) · [#11](https://github.com/ohne-b/twitch-miner/pull/11) · [#12](https://github.com/ohne-b/twitch-miner/pull/12) · [#13](https://github.com/ohne-b/twitch-miner/pull/13) · [#14](https://github.com/ohne-b/twitch-miner/pull/14) |
| [@rangermix](https://github.com/rangermix) | [#1](https://github.com/rangermix/TwitchDropsMiner/pull/1) · [#2](https://github.com/rangermix/TwitchDropsMiner/pull/2) · [#7](https://github.com/rangermix/TwitchDropsMiner/pull/7) · [#8](https://github.com/rangermix/TwitchDropsMiner/pull/8) · [#9](https://github.com/rangermix/TwitchDropsMiner/pull/9) · [#13](https://github.com/rangermix/TwitchDropsMiner/pull/13) · [#20](https://github.com/rangermix/TwitchDropsMiner/pull/20) · [#24](https://github.com/rangermix/TwitchDropsMiner/pull/24) · [#29](https://github.com/rangermix/TwitchDropsMiner/pull/29) · [#32](https://github.com/rangermix/TwitchDropsMiner/pull/32) · [#45](https://github.com/rangermix/TwitchDropsMiner/pull/45) · [#74](https://github.com/rangermix/TwitchDropsMiner/pull/74) · [#79](https://github.com/rangermix/TwitchDropsMiner/pull/79) · [#80](https://github.com/rangermix/TwitchDropsMiner/pull/80) · [#84](https://github.com/rangermix/TwitchDropsMiner/pull/84) · [#86](https://github.com/rangermix/TwitchDropsMiner/pull/86) · [#88](https://github.com/rangermix/TwitchDropsMiner/pull/88) · [#93](https://github.com/rangermix/TwitchDropsMiner/pull/93) · [#89](https://github.com/rangermix/TwitchDropsMiner/pull/89) · [#90](https://github.com/rangermix/TwitchDropsMiner/pull/90) · [#91](https://github.com/rangermix/TwitchDropsMiner/pull/91) · [#92](https://github.com/rangermix/TwitchDropsMiner/pull/92) · [#104](https://github.com/rangermix/TwitchDropsMiner/pull/104) · [#105](https://github.com/rangermix/TwitchDropsMiner/pull/105) · [#116](https://github.com/rangermix/TwitchDropsMiner/pull/116) · [#119](https://github.com/rangermix/TwitchDropsMiner/pull/119) · [#120](https://github.com/rangermix/TwitchDropsMiner/pull/120) |
| [@Sean-Destefano](https://github.com/Sean-Destefano) | [#49](https://github.com/rangermix/TwitchDropsMiner/pull/49) |
| [@SimpliAj](https://github.com/SimpliAj) | [#72](https://github.com/rangermix/TwitchDropsMiner/pull/72) |
| [@Stein-N](https://github.com/Stein-N) | [#71](https://github.com/rangermix/TwitchDropsMiner/pull/71) |
| [@vurmil](https://github.com/vurmil) | [#12](https://github.com/rangermix/TwitchDropsMiner/pull/12) · [#17](https://github.com/rangermix/TwitchDropsMiner/pull/17) · [#18](https://github.com/rangermix/TwitchDropsMiner/pull/18) · [#100](https://github.com/rangermix/TwitchDropsMiner/pull/100) |
<!-- contributors:end -->
