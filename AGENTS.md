# Agent instructions

This is the canonical repository harness. Read [CONTRIBUTING.md](CONTRIBUTING.md) before
planning, editing, testing or reviewing. Its PR checklist is mandatory. The user removed
the alternate agent instruction files; do not recreate copies or links.

## Workflow

- Use descriptive `feat/` or `fix/` branches, conventional commits and PRs against
  `ohne-b/twitch-drops-miner:main`. Keep branches, commits and documentation free of assistant branding.
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
- Keep user guidance in README and contributor policy in CONTRIBUTING. Do not create
  `docs/`, `plans/` or repository planning documents.

## Architecture

The product is Twitch Drops Miner (dashboard: Drops Miner), repository and
Cargo package/binary are twitch-drops-miner. Preserve the existing Compose service/container name,
data/log directories, TDM log prefix, auth cookie and CSRF header for upgrade compatibility.

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
| `twitch/` | OAuth, bounded HTTP/GQL, inventory/catalog, beacon watch, PubSub shards |
| `miner.rs` | Session supervisor and owned mining jobs, scheduling and reconciliation |
| `web/` | Axum HTTP, Socketioxide, protected snapshots and embedded frontend |
| `fixture.rs`, `bin/dashboard-fixture.rs` | Feature-gated offline browser fixture |
| `frontend/`, `lang/English.json` | React/TypeScript/Tailwind and one message catalog |

Build frontend assets before backend/static tests. `web/` is ignored output. Release binaries
embed it and run without a build tool/runtime companion. Production builds never enable
`dashboard-fixture`; fixture routes must return 404 in production.

## Mining contracts

- Discovery does not select games. `games_to_watch` is the ordered Unicode-casefolded
  automatic allowlist. Empty means no automatic watch events. Mine selects a game across
  eligible campaigns. An explicitly chosen manual channel overrides automatic selection
  without changing saved games, filters or priorities.
- Preserve campaign/drop timing, prerequisites, claim state, benefit filters and ignore rules.
  Literal ignore substrings cascade through dependents while retaining shared prerequisites.
  Zero-minute subscription rewards are omitted from Campaigns/Up next; expired drops leave
  the queue without hiding upcoming or sequential rewards. Ignore/skip is never completion.
- Active campaigns with existing progress appear first. Finished requires all watch rewards
  claimed; expiry alone never qualifies. Persistent completion archives are display-only,
  survive cache clears, and can be invalidated by newer contradictory account evidence or
  changed rewards. Older claim-only history remains completion-unverified.
- Fetch Twitch account Inventory and `https://twitch-drops-api.sunkwi.com/v2/drops` concurrently.
  SunkwiBOT is the catalog source; remove Twitch catalog/detail operations and live-channel
  campaign scans. Inventory wins as whole records, including explicit unclaimed evidence;
  malformed account records must not fall back to public account assumptions. Public HTTP
  uses an isolated client with no Twitch credentials, cookies or identifiers, no redirects,
  the configured proxy/timeouts, and bounded retries within 30 seconds. Cap bodies at 16 MiB
  and campaigns at 2000. Reject timestamps older than 30 minutes or over 5 minutes ahead.
  Strip public campaign/drop `self` records; preserve real ACLs, dependencies and timing.
  Reject mixed-null enabled ACLs. Shared domain parsing rejects campaign/drop dates without
  room for the scheduler's one-hour lead and the claim journal's 24-hour grace period.
  Missing restrictions/dependencies, malformed/null entries and duplicate IDs are partial,
  not empty success. Keep known active/upcoming records on partial refresh; valid empty
  feeds are authoritative. A feed 401/403 never logs out Twitch; Twitch auth/cancellation
  failures propagate. Coverage can vary, and restarts need the feed for non-inventory
  campaigns. Never claim relogin/cache clearing repairs feed coverage. Unknown
  PubSub/CurrentDrop progress queues inventory refresh at most once per minute.
- Unknown linkage is null and unknown progress has no confirmed timestamp. Infer claims from
  awards only when every benefit has evidence in the drop's time window and no explicit
  account record contradicts it.
- Special Events (`509663`) and IRL (`509672`) cross categories only with a nonempty enabled
  actual ACL. Automatic drops need matching category and drops-enabled status; all need live
  channels, selected games and eligible rewards. Offline/ineligible streams yield even at tied
  fallback priority. Preserve nullable viewer counts and the watching row during rebuilds.
- Channels publishes currently eligible selected-game streams plus the explicitly selected
  manual channel. Rank automatic candidates before the channel limit using matching campaign
  priority, including actual-ACL special-category streams. Mine channel accepts a validated
  Twitch login/root URL and resolves it with owned bounded work independently of inventory.
  Manual watching requires a live identity, not catalog coverage, selected games, a drops tag
  or local reward eligibility. Do not persist game selections or fabricate reward progress.
  Direct lookup and manual stream refresh use a short best-effort metadata attempt;
  missing/failed metadata never blocks watching, but auth/cancellation still propagates.
  Show a known manual reward only after Twitch reports it; do not estimate manual rewards.
  Preserve pending lookups and the confirmed channel separately across network generations;
  late/superseded results cannot restore manual mode. Manual stream metadata never overrides
  real ACLs or account records. Pending stream refresh pauses watching; retry failed refreshes.
  An optional 1..1440-minute timer uses a monotonic deadline starting at selection, survives
  network renewal/browser reconnect, and returns to automatic selection at expiry. Offline
  manual channels wait without switching targets or stopping the timer. Exit, logout, cache
  clear and process restart end manual mode. Viewer counts use channel-only events.
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
  Hourly validation/network reconfiguration preserves manual selection/deadlines and queues
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

- Use `frontend/src/assets/twitch-drops-miner-logo.svg` for the app, login, favicon and README.
  Preserve its artwork and aspect ratio; Vite emits one hashed asset for browser caching.
  Keep adjacent brand text accessible and sidebar navigation reachable in short windows.
- Keep the subtle charcoal/Manrope design, individual MDI paths and shared native controls.
  Render strings as React text, validate external links/artwork, expand Twitch image placeholders.
  No injected HTML or CDN scripts. Art provides safe missing/broken-image fallbacks.
- Sidebar: enlarged GitHub glyph above Twitch account ID, overriding shared icon sizing.
  Connection status lives in Settings and is labeled Dashboard connected, separate from Twitch.
- Overview: watching information only in Mining, no status subtitle or Recent activity. Channels
  and Up next have equal desktop dimensions and internal scrolling; stack on narrow screens and
  preserve access on short windows. Show confirmed values/timestamps without redundant labels.
  Keep expanded channel-entry controls and feedback inside the scrollable list body.
  Manual lookup has no preparing message; retain the busy/disabled Mine button, inline
  errors and return-to-auto control while a lookup is pending.
  Channel name/URL and optional timer use accessible input placeholders. Settings autosave
  has no saving/saved notices; preserve errors, edits and Retry.
- Overview and Maintenance share inventory refresh feedback inside the button. Track queued
  and running work through publication, coalesce requests, preserve state on reconnect and
  ignore stale completion events. An acknowledgement is not completion. Keep request errors
  and partial-catalog failures retryable without clearing previous results or adding notices.
  Keep the refresh/check icon at the same left-aligned position when the label changes.
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
Keep the project's PolyForm Strict 1.0.0 license in `LICENSE` and the full upstream MIT
license in `NOTICE.md`; third-party components retain their original licenses;
include both with frontend asset notices in production images. Preserve 1000:1000 ownership,
mounts and port. Health does not prove earning.

Cargo manifest/lock own the version. Prepare release opens a draft PR; a maintainer can use
the same local release script and draft-PR process if its token is unavailable. Publish is manual from
validated main and uses reviewed CHANGELOG notes/GHCR. Every release attaches and verifies
`latest.json` before publishing; stable releases alone move the latest pointer. Use scoped
conventional commit messages and concise release change lists with comparison/issue links.
Accept exact-commit push or manual validation.
Publish only to GHCR at ghcr.io/ohne-b/twitch-drops-miner using the scoped workflow token.
Advance latest only after the stable release and its manifest are public. Preserve
published old-name images and document promotion recovery and first-package visibility.
Keep Buildx/Build Push action pins identical between validation and release. README uses
a centered title/tagline/license opener and GitHub Flavored Markdown alerts. Keep upstream
attribution in License and credits; do not add a contributor/PR table or automation that
rewrites README after merges. No ordinary code merge may publish a release or bypass
independent review/checks.

For home-server work, inspect the current checkout/Compose/image before assumptions. Ask before
changing the running deployment. Build while it runs, back up data and Compose before replacement,
retain rollback, and require interactive sudo in the user's terminal. Always provide the actual
PowerShell update command in the handoff; never ask for a sudo password in chat.
