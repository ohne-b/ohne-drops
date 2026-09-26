# Agent instructions

This is the canonical repository harness. Read [CONTRIBUTING.md](CONTRIBUTING.md) before
planning, editing, testing or reviewing. Its PR checklist is mandatory. The user removed
the alternate agent instruction files; do not recreate copies or links.

## Workflow

- Use descriptive `feat/` or `fix/` branches, conventional commits and PRs against
  `ohne-b/twitch-miner:main`. Keep branches, commits and documentation free of assistant branding.
- Preserve existing user changes, data, credentials, logs and backups. Ask before significant
  refactoring unless the current task already authorizes it. A full rewrite authorization
  covers its necessary cleanup. Never change a running deployment without authorization.
- Use one implementation agent when requested. Required independent adversarial review uses
  a separate read-only reviewer; the author cannot approve their own work.
- Integrate current main before final validation/review and again before merge if it advances.
  Keep incomplete PRs in draft. Record tested/reviewed revisions and actual results; never
  describe unrun checks, live progress or review as successful.
- Add backend unit/regression tests; cover frontend changes where practical. Always update
  README and this harness when behavior/architecture changes. Update English messages for UI
  or console changes. There are no other locales or language settings.

## Architecture

One Rust Cargo package owns the backend. Use concrete structs with methods and composition
for domain/services (the repository's OOP requirement), typed enums and DRY shared policies.
Do not add forwarding hierarchies or speculative traits with one implementation.

| Module | Responsibility |
| --- | --- |
| `main.rs` | CLI/env, credential-safe logging, owned shutdown, healthcheck |
| `config.rs`, `dto.rs` | Typed settings, compatible migration, API snapshots |
| `domain.rs`, `policy.rs` | Campaign/drop/channel eligibility and dependency-aware ignores |
| `store.rs` | Exclusive data directory, atomic settings/history/archive/claim journal |
| `auth.rs`, `origin.rs` | Dashboard passwords/sessions, origin and cookie policy |
| `twitch/` | OAuth, bounded HTTP/GQL, inventory recovery, beacon watch, PubSub shards |
| `miner.rs` | Session supervisor and owned mining jobs, scheduling and reconciliation |
| `web/` | Axum HTTP, Socketioxide, protected snapshots and embedded frontend |
| `fixture.rs`, `bin/dashboard-fixture.rs` | Feature-gated offline browser fixture |
| `frontend/`, `lang/English.json` | React/TypeScript/Tailwind and one message catalog |

Build frontend assets before backend/static tests. `web/` is ignored output. Release binaries
embed it and run without a build tool/runtime companion. Production builds never enable
`dashboard-fixture`; fixture routes must return 404 in production.

## Mining contracts

- Discovery does not select games. `games_to_watch` is the ordered Unicode-casefolded
  allowlist. Empty means no watch events. Mine selects a game across eligible campaigns.
- Preserve campaign/drop timing, prerequisites, claim state, benefit filters and ignore rules.
  Literal ignore substrings cascade through dependents while retaining shared prerequisites.
  Zero-minute subscription rewards are omitted from Campaigns/Up next; expired drops leave
  the queue without hiding upcoming or sequential rewards. Ignore/skip is never completion.
- Active campaigns with existing progress appear first. Finished requires all watch rewards
  claimed; expiry alone never qualifies. Persistent completion archives are display-only,
  survive cache clears, and can be invalidated by newer contradictory account evidence or
  changed rewards. Older claim-only history remains completion-unverified.
- Inventory/details win over metadata recovery as whole records. Preserve independent
  in-progress records when details are null. Valid empty catalogs never trigger recovery.
  With a valid catalog, recovery is restricted to its active/upcoming IDs.
- Recovery uses authenticated `viewerDropCampaigns` without `self` edges: at most 500
  categories × 3 streams, 100 known/saved slugs, and 60 seconds. Propagate logout/auth failures.
  Skip nullable neighbors individually. Partial coverage stays partial; no false diagnostics
  that relogin, cache clearing or client substitution repairs an upstream catalog restriction.
- Unknown linkage is null and unknown progress has no confirmed timestamp. Infer claims from
  awards only when every benefit has evidence in the drop's time window and no explicit
  account record contradicts it. Recovered channels constrain eligibility but are not a real ACL.
- Special Events (`509663`) and IRL (`509672`) cross categories only with a nonempty enabled
  actual ACL. Regular drops need matching category and drops-enabled status; all need live
  channels, selected games and eligible rewards. Offline/ineligible streams yield even at tied
  fallback priority. Preserve nullable viewer counts and the watching row during rebuilds.
- Watch events use validated Twitch beacon URLs and a base64 minute-watched payload every
  59 seconds. No playlists/video/audio downloads. Confirm via PubSub or CurrentDrop, distinguish
  estimates, and recover at 15 unconfirmed estimates. Only currently eligible drop progress
  suppresses fallback. Late request results cannot overwrite newer account/stream events.
- Claims require account-issued instance IDs, skip upcoming campaigns and stop at the strict
  campaign-end + 24-hour deadline. Earned claims are independent of mining/ignore selection.
  Persist the account-scoped intent before RPC, then its success receipt and history. Keep the
  receipt until the owner acknowledges domain state and archives completion; reconcile after
  restart even without catalog metadata. Never retire it before durable history/archive writes.
- Every session/job/socket task is owned and drained. Logout coalesces and removes only Twitch
  credentials after drainage; concurrent shutdown cannot interrupt removal in either queue order.
  Hourly validation/network reconfiguration preserves eligible manual selection and queues
  new channel choices until fresh channel eligibility is available. Cache clear
  preserves settings, credentials, claim history and completed campaigns.
- Requests use bounded concurrency/rate, retries and cancellation. Quality 1..6 controls connect
  timeout 5×quality and total 10×quality seconds; the saved refresh interval actually schedules
  inventory work. Slow discovery must not block watch cadence. Duplicate idle prompts collapse.

## Authentication and storage

- Original in-app device code only, Smart TV client identity, `twitch_oauth2` types/requests.
  Keep explicit empty scopes, pending/slow-down/expiry/denial handling and token validation.
  Channel pages use the web client URL. No browser-import or browser-renewal code/services.
- New sessions use `twitch_session.json`; keep old credentials/backups untouched for rollback.
  Invalid new sessions are preserved separately before reauthorization. Never log OAuth tokens,
  device secrets, proxy credentials, cookie values or raw authenticated transport frames.
- Existing settings, version-1 history, completion and dashboard-auth formats remain compatible.
  Atomic replacement commits before memory/publication. Corrupt auth fails closed; corrupt
  history/archive are preserved read-only; unreadable settings never silently reset. One miner
  process per data directory. Keep retired credential filenames ignored to protect old installs.
- Optional password-only dashboard auth uses compatible scrypt and SHA-256 token digests,
  fixed 30-day sessions, HttpOnly/SameSite=Strict cookies and Secure on HTTPS. Password change
  revokes other sessions; disable requires current password. Twitch logout is separate.
- Outer middleware protects HTTP and both Socket.IO transports. Writes require `X-TDM-Request: 1`,
  same-origin/Fetch Metadata checks and bounded bodies. Rate-limit hashing by peer and globally.
  Recheck socket authorization for events/broadcasts and expire idle sessions. Enabling auth
  evicts anonymous sockets before private publication. Detached password/settings writes are owned.
- `PUBLIC_BASE_URL` is one normalized HTTP(S) root origin. Reject credentials, paths, queries,
  fragments, lists/wildcards and ambiguous numeric IPv4 forms. It controls browser origin/cookie
  scheme, not forwarded client-IP trust. Do not trust proxy headers implicitly.
- Serve SPA only on explicit dashboard routes. Preserve API/socket 404s. Public login code/fonts
  do not make account data public. HTML revalidates; hashed assets are immutable; private data
  is no-store. Dashboard protection recovery removes only web_auth.json while stopped/restricted.

## Dashboard design and contracts

- Keep the subtle charcoal/Manrope design, individual MDI paths and shared native controls.
  Render strings as React text, validate external links/artwork, expand Twitch image placeholders.
  No injected HTML or CDN scripts. Art provides safe missing/broken-image fallbacks.
- Sidebar: enlarged GitHub glyph above Twitch account ID, overriding shared icon sizing.
  Connection status lives in Settings and is labeled Dashboard connected, separate from Twitch.
- Overview: watching information only in Mining, no status subtitle or Recent activity. Channels
  and Up next have equal desktop dimensions and internal scrolling; stack on narrow screens and
  preserve access on short windows. Show confirmed values/timestamps without redundant labels.
- History artwork is optional; retain old rows and use matching live benefits as display fallback.
  No Telegram controls/API/credentials in responses and no dashboard updater.
- Maintenance checks the latest stable release's `latest.json`, compares SemVer precedence
  without build metadata, and distinguishes failure from up-to-date status. Keep requests
  bounded/coalesced and release links within this repository. No install/download execution.
- Shared Field content starts at the top; helper text cannot stretch neighboring label rows.
  No focus rings, but visible keyboard background/border changes must outrank utility layers;
  keep system focus in forced colors. Verify computed field/button/checkbox focus and axe checks.
- One typed provider hydrates complete snapshots and incremental events. Commands stay disabled
  until reconnect hydration. Editable settings drafts are separate from live data; serialize
  autosaves with original revisions. HTTP409 preserves edits for Retry and only changes touched fields.

## Validation and release

Use the complete commands in CONTRIBUTING. Rust tests use temporary files and mock transports;
Playwright starts its own loopback:8765 fixture, verifies readiness/reset, and refuses reuse.
Never use live credentials, a live miner or real notifications for automated checks.

CI requires Rust fmt/Clippy/tests, frontend format/type/build/unit/browser/axe, automation
contracts, version/lock agreement, and amd64/arm64 production image builds plus isolated health.
Keep the project's MIT license in `LICENSE` and the full upstream license in `NOTICE.md`;
include both with frontend asset notices in production images. Preserve 1000:1000 ownership,
mounts and port. Health does not prove earning.

Cargo manifest/lock own the version. Prepare release opens a draft PR; publish is manual from
validated main and uses reviewed CHANGELOG notes/GHCR. Every release attaches and verifies
`latest.json` before publishing; stable releases alone move the latest pointer. Use scoped
conventional commit messages and concise release change lists with comparison/issue links.
Accept exact-commit push or manual
validation; contributor-token commits do not automatically trigger push workflows.
Optional Docker Hub publication uses `DOCKERHUB_IMAGE`/`DOCKERHUB_USERNAME` variables and
the `DOCKERHUB_TOKEN` secret; push one multi-architecture build to both registries and
advance `latest` only after a stable release and its manifest are public. Missing enabled
credentials fail before publication; document partial registry-push/promotion recovery.
Keep Buildx/Build Push action pins identical
between validation and release. Contributor credit runs on trusted default-branch code only
under pull_request_target; never execute a PR head with its write token. Preserve exactly one
README contributor marker pair/header and fail closed on malformed tables. No ordinary code
merge may publish a release or bypass independent review/checks.

For home-server work, inspect the current checkout/Compose/image before assumptions. Ask before
changing the running deployment. Build while it runs, back up data and Compose before replacement,
retain rollback, and require interactive sudo in the user's terminal. Always provide the actual
PowerShell update command in the handoff; never ask for a sudo password in chat.
