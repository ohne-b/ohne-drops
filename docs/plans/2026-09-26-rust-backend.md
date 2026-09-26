# Complete Rust backend replacement

## Outcome and scope

Replace the entire application backend with a Rust executable. Keep the React/TypeScript
dashboard, its subtle dark design, and its observable behavior, except that Telegram is
removed completely. The finished checkout has one backend implementation and one supported
build/test/deployment path. There is no interpreter, old backend source, old test server,
unused compatibility layer, or second runtime in the production image.

The user authorized the rewrite, repository cleanup, conventional commits, a pull request
to `ohne-b/twitch-miner`, independent review, and merge after checks. Implementation uses
one agent. Independent review remains separate. No home-server deployment is authorized.

Confirmed decisions:

- A new Twitch device-code login is acceptable. Do not deserialize the old cookie file.
- Preserve settings, explicit game selection, confirmed progress, claim history, completed
  campaigns, and dashboard password protection. Never wipe the data or logs directories.
- Keep all other user-facing features. Remove obsolete implementation rather than inventing
  additional feature cuts. There is no dashboard updater in this scope.
- Include the user's removals of `CLAUDE.md`, `GEMINI.md`, and `RELEASE_NOTES.md`; keep `AGENTS.md` as
  the only contribution harness and update the contradictory link requirement.
- Preserve Git history, license/attribution, and existing backups. Cleanup applies to the
  current checkout and current build instructions, not to historical commits.

Starting reference: `5958c44` on `main`; implementation branch:
`feat/rust-backend-rewrite`. Inspect and integrate any newer main before final review.

## Investigation and behavior inventory

The running application, frontend DTOs, protocol events, tests, persisted schemas, and
deployment files are the behavioral reference. Old prose alone is not authoritative.

Verified examples of removable implementation:

- An image-cache object that only returns its argument.
- Unused channel-points operations and unused notification listing/view operations.
- The unused playlist-watching/playback-token path.
- Desktop GUI compatibility wrappers, unnecessary model-to-client forwarding methods,
  obsolete client presets, and stale settings response aliases.
- Retired notification service, API, credentials, controls, translations, and tests.

The active watch mechanism discovers a beacon URL from a Twitch channel page or its
settings script, then POSTs a base64-encoded `minute-watched` event. Preserve that actual
path, its payload and status handling. Do not substitute a protocol described only in old
documentation. No stream audio/video is downloaded.

## Architecture

Use one Cargo package, not a workspace of tiny crates. Modules own concrete responsibilities;
domain structs and enums have methods where behavior belongs. Avoid replicating the former
class hierarchy, forwarding layers, or global mutable client objects.

```text
Cargo.toml / Cargo.lock / rust-toolchain.toml
src/
  main.rs                process configuration, logging, shutdown
  lib.rs                 shared application assembly
  config.rs              typed settings, normalization, revisions
  dto.rs                 dashboard snapshots and wire data
  domain.rs              games, channels, campaigns, drops, eligibility
  policy.rs              dependency-aware ignore/selection policy
  store.rs               atomic settings/history/archive/session persistence
  auth.rs                dashboard authentication and session lifecycle
  origin.rs              origin validation and cookie scheme
  twitch/
    mod.rs               authenticated HTTP transport and request policy
    oauth.rs             device authorization and session persistence
    operations.rs        used GraphQL operations only
    inventory.rs         account inventory, catalog, bounded recovery
    channels.rs          directory/ACL channels and beacon watch events
    pubsub.rs            topic shards, heartbeat, reconnect, typed messages
  miner.rs               owned session tasks and mining state transitions
  web/
    mod.rs               Axum routes, middleware and static assets
    socket.rs            authenticated Socket.IO snapshots/events
  bin/dashboard-fixture.rs  offline browser fixture, feature gated
tests/                    Rust integration tests and sanitized JSON fixtures
frontend/                 existing React application
scripts/                  current development/release helpers where useful
.github/                  validation, contribution and release automation
docs/                     current architecture, operation, and migration evidence
```

Module boundaries may be split when actual implementation size warrants it, without adding
unrequested extension interfaces. DTOs are distinct from authenticated Twitch response
data, so secrets and raw upstream payloads cannot accidentally enter dashboard broadcasts.

### Dependencies and reasons

| Responsibility | Choice and acceptance condition |
| --- | --- |
| Async tasks, timers, signals | Tokio; tokio-util cancellation tokens |
| HTTP server | Axum and Tower middleware |
| Existing dashboard transport | Socketioxide; retain Socket.IO v4 client compatibility |
| HTTP/TLS/proxy | Reqwest with explicit TLS and proxy features; no ambient proxy surprises |
| Twitch OAuth | `twitch_oauth2` 0.17.1: injectable HTTP client, device request builder, token types and validation; retain explicit empty scopes and own the cancellable pending/slow-down loop |
| Upstream WebSockets | `reqwest-websocket` 0.6 uses the same Reqwest 0.13 client, TLS and proxy configuration |
| Data formats | Serde/serde_json, Chrono, CSV, URL |
| Persistence | Standard filesystem APIs plus tempfile for atomic replacement; one process per data directory |
| Passwords and tokens | RustCrypto scrypt/SHA-256, secure random bytes, constant-time comparison; preserve existing password and session formats |
| Diagnostics | tracing, tracing-subscriber, rotating file output |
| Frontend assets | rust-embed 8.12 embeds the built frontend in release executables; explicit SPA route allowlist |
| Tests | Rust test runner, temporary directories, mocked HTTP/WebSocket endpoints; existing Vitest/Playwright/axe |

Pin the toolchain and commit the lockfile. Choose compatible library versions from current
official documentation/source, disable unnecessary features, and inspect the dependency
tree. No Helix SDK is assumed to implement private Drops GraphQL. No custom cryptography,
HTTP parser, Socket.IO implementation, or home-grown replacement for an available library.

The inspected OAuth device builder omits the `scopes` field when empty and its convenience
poll loop handles pending authorization but not every required cancellation/slow-down case.
Adapt those two edges at the transport/session boundary; do not fork the crate. Requests and
token parsing remain library-owned. Use Socketioxide 0.18.7 with Rust 1.97.1 and Axum 0.8.

## Application ownership and cancellation

One application state supplies immutable dashboard snapshots and serialized persistence.
One miner supervisor owns each Twitch session and every task it spawns. HTTP handlers send
commands rather than invoking unrelated client methods through shared globals.

- Commands cover refresh, cache clear, settings changed, channel selection, exit manual,
  logout and shutdown. Preserve pending requests arriving during a network operation.
- Use bounded channels/queues. Coalesce refresh/settings requests without losing cache-clear
  intent or logout acknowledgements. No unbounded task spawning or detached claim work.
- Keep slow upstream I/O outside state locks. Disk writes commit before publishing successful
  settings/security changes. A failed save keeps the previous in-memory value and revision.
- Logout cancels and joins discovery, watch, channel delays, socket shards and message tasks
  before removing credentials. Concurrent logout requests coalesce. Client disconnection or
  process shutdown cannot interrupt the credential-removal transaction.
- Restart/login creates a fresh session generation. Results from an old generation cannot
  overwrite the new snapshot or write new credentials after logout.
- Use monotonic time for backoff/timeouts and UTC wall time for campaign eligibility/history.

## Twitch session and network behavior

Preserve the in-app device-code flow with the existing Smart TV client identity. No browser
import, browser automation, renewal container, OAuth callback server, or session scraping.

The selected OAuth crate must allow the existing client ID, appropriate default headers,
empty scopes, proxy, bounded timeouts, and cancellation. Validate returned tokens and client
identity before using them. Handle pending authorization, slow-down, expiration, denial,
revocation and HTTP failure distinctly without exposing tokens/device secrets in logs.

Persist a new versioned JSON session atomically with restricted permissions. Keep device
identity across restart, generate a new process session ID, and validate saved tokens on
startup. Refresh only when supported by the actual returned credential. An absent or invalid
session initiates the usual device login. Leave the old credential file untouched for rollback.

Authenticated GraphQL uses the same client identity and bounded request concurrency/rate.
Retain the connection-quality control: clamp to 1..6 and use 5×quality seconds to connect
and 10×quality seconds for total ordinary HTTP requests. Apply changed proxy/quality settings
to subsequent HTTP and socket connections, without exposing proxy credentials.
Preserve single and batched requests, partial server-error nulls, and independent valid
neighbors. Retry transient failures with capped backoff; authentication failures return to
the supervisor. Never log authorization headers, cookie values, proxy credentials or raw
upstream bodies that may contain credentials.

## Mining behavior to preserve

### Inventory and discovery

1. Fetch account inventory and claimed-benefit evidence.
2. Fetch the catalog; distinguish `null` from a valid empty list.
3. Fetch applicable campaign details in bounded batches. Skip missing details together with
   their incomplete summary, but retain independent inventory records.
4. Only if discovery is incomplete, recover metadata from live-channel `viewerDropCampaigns`.
   Omit account `self` edges. Bound recovery to 500 categories × 3 streams, up to 100 known
   or selected game slugs, and 60 seconds. Preserve nullable neighbors and cancellation.
5. Account records take precedence over recovered records as whole records. With a valid
   catalog, recover only IDs it lists as active/upcoming. A valid empty list starts no scan.
6. Preserve campaign/drop timing, ACL enablement, prerequisites and discovery-channel limits.
   Discovery channels are not a real ACL for cross-category eligibility.
7. Link state can be unknown. Unknown progress has no confirmation timestamp. Infer a claim
   without a self edge only with account evidence for every benefit in the applicable time.
8. Publish a complete replacement snapshot plus truthful catalog-availability metadata.

### Selection, channels and progress

- `games_to_watch` remains the explicit, ordered, case-insensitive mining allowlist.
  Discovery never adds games; an empty list sends no watch events.
- Preserve benefit filters and dependency-aware literal ignore rules. Ignored/skipped rewards
  are never counted as claimed. Handle missing prerequisites and cycles without recursion
  failure. Shared prerequisites remain useful to allowed descendants.
- Zero-minute subscription rewards stay out of campaign watch totals and Up next. Expired
  individual rewards stay out of the queue without hiding future/sequential rewards.
- Special Events and IRL may cross categories only through a nonempty enabled real ACL.
  Regular campaigns require matching category and drop-enabled eligibility. No linkage gate.
- Gather directory and participating channels, preserve nullable viewer counts, and limit
  tracking to 199. Order by selected/manual preference, saved game priority, ACL preference,
  and viewer count. An ineligible/offline current stream yields even when priorities tie.
- Keep the watched row visible while settings changes/rebuilds are pending; enforce current
  selection before the next watch send. Manual mode can fail over within its target game
  and returns to automatic selection when that game is no longer eligible.
- Beacon discovery accepts both existing page formats, validates URLs, and caches per stream.
  Preserve minute-watched fields and 59-second watch cadence. Do not download playlists.
- Prefer confirmed PubSub progress, then CurrentDrop polling. Any local estimate is separately
  represented and never creates a false confirmation timestamp or an earned claim.
- Cap unconfirmed estimates at 15 minutes. A stalled reward becomes temporarily ineligible
  and triggers reselection/recovery; fresh confirmed progress clears the estimate. Do not
  continue an unconfirmed watch forever merely because the display labels it as estimated.
- Claim on account-provided claim IDs, deduplicate concurrent requests, persist successful
  history before publishing success, update dependents/queue immediately, and continue the
  next eligible reward after Twitch's propagation delay. No external notification hooks.
- Claim already-earned rewards independently of selected games, ignored names and campaign
  active status. Preserve the strict 24-hour grace after campaign end; skip upcoming campaigns
  and never infer claim eligibility from local estimates alone.
- The saved minimum-refresh-interval control drives inventory refresh cadence. Preserve
  campaign/drop start/end scheduling. Consecutive
  identical idle/no-campaign console messages remain collapsed.

### Upstream real-time transport

Retain the currently used Twitch Drops/notifications and stream state/update topics. Shard
at 50 topics per connection, with at most eight shards. Handle LISTEN/UNLISTEN, responses,
application PING/PONG deadlines, server reconnect and transport close. Reconnect with capped
backoff and resubscribe. Offline, category-change, viewer-count, claim and reward-reminder
events feed the same owned miner state. Unknown events are ignored safely, malformed payloads
cannot kill unrelated streams, and logout drains every shard and callback.

## Dashboard contract and security

Keep current response shapes and existing React state/autosave behavior. Explicit routes:

| Surface | Retained behavior |
| --- | --- |
| `/`, `/campaigns`, `/history`, `/activity`, `/settings`, `/login` | SPA route allowlist; unknown API/socket paths stay errors |
| `/healthz` | Public process health, never presented as proof of Twitch earning |
| `/api/status`, `/api/channels`, `/api/campaigns`, `/api/console` | Current snapshots, protected when dashboard auth is enabled |
| `/api/channels/select`, `/api/mode/exit-manual` | Validate selection and dispatch owned commands |
| `/api/settings` GET/POST | Typed partial updates, revision conflict 409, atomic save, safe normalization |
| `/api/settings/verify-proxy` | Bounded connectivity test without credential disclosure |
| `/api/oauth/confirm`, `/api/twitch/logout` | Existing in-app device flow and separately drained Twitch logout |
| `/api/reload`, `/api/cache/clear`, `/api/close` | Preserve persistent data; cache clear only removes derived state |
| `/api/version` | Version/check information, no installer or dashboard updater |
| `/api/history`, `/api/history/stats`, `/api/history/export.csv` | Filters, newest-first entries, limits, statistics, Unicode/BOM CSV and clear-history semantics |
| `/api/auth/status`, `/api/auth/login`, `/api/auth/logout`, `/api/auth/settings` | Password protection, session lifecycle, redacted errors |
| Socket.IO | Complete initial snapshot, existing incremental event names and reconnect hydration |

Remove the unused credential-submission API if no current frontend caller exists. Remove
all retired notification routes and reject/discard their stored fields rather than echoing
secrets. Keep frontend commands disabled until a complete reconnect snapshot arrives.

Security equivalence is a release gate:

- Default-off dashboard protection, existing scrypt hashes and SHA-256 session digests,
  fixed 30-day expiry, remember-me cookie behavior, secure random tokens, and session rotation.
- Corrupt auth state fails closed. Enabling auth disconnects anonymous sockets before private
  broadcasts; password change revokes other sessions; disable requires current password.
- HTTP writes require `X-TDM-Request: 1`. Check Origin/Fetch Metadata for writes and both
  Socket.IO transports. Bound request bodies and rate-limit hashing attempts globally/by peer.
- Recheck authorization on every socket event and broadcast, close idle expired sessions,
  and cancel associated timers/tasks on disconnect. Do not rely solely on handshake auth.
- Preserve PUBLIC_BASE_URL normalization, rejection of ambiguous numeric IP forms and URL
  credentials/paths/fragments, Secure-cookie behavior, and independence from forwarded-IP
  trust. No wildcard CORS or implicit trusted forwarding.
- Public login assets only; no-store private responses, no-cache HTML and immutable hashed
  assets. Retain security headers, safe DOM rendering, link/artwork validation and focus/accessibility.

## Persistence and migration

Keep the existing version-1 claim-history, completion-archive and dashboard-auth JSON formats.
Typed settings load known fields with defaults and the narrow existing default-filter
migration; intentional custom filters and list order survive. Remove retired fields on the
next successful settings write. No whole-data-directory reset and no automatic credential dump.

Atomic file replacement uses a temporary file in the destination directory, flush/sync, and
rename/persist. Update in-memory state only after a successful write. Protect against a
second process using the same data directory. Tests cover failed writes and corrupt files.

Completed campaigns remain separate from the live mining inventory. Only all-claimed watch
rewards mean completion; expiry/ignore/skip do not. Preserve archived completions across
refresh/restart; invalidate them durably only when new account evidence or a changed reward
set disproves them. Older claim-only history remains completion-unverified. Keep artwork
optional and do not delete completed campaigns when clearing claim history.

Confirmed live progress is restored from Twitch. If a local snapshot is used while loading,
it is display-only until refreshed; it cannot authorize mining or manufacture progress.
Retain all existing data and backup files during the source migration.

## Repository and delivery cleanup

- Remove every previous backend source/test file, entry point, dependency manifest/lock,
  virtualenv instruction, lint/typecheck configuration, and obsolete generated/cache files.
- Replace the browser fixture with a Rust executable behind an explicit test feature. It
  binds only loopback:8765, uses temporary storage, exposes fixture readiness/reset, starts
  no miner and has no real Twitch transport. Production builds exclude its routes.
- Retain the useful browser/accessibility suite; delete notification-only coverage and add
  a regression asserting the removed controls/API are absent. Replace backend test coverage
  by observable Rust unit/integration/network tests, not a file-for-file translation.
- Replace contributor automation with Node and its tests. Preserve trusted-main checkout
  under pull_request_target, strict contributor markers, human authors and alphabetic order.
- Move release version ownership to Cargo.toml/Cargo.lock. Update release extraction,
  creation, rollback, notes and tests together. Remove obsolete manifests from all workflows.
  Generate release notes from repository changes without an external AI-service credential.
  Do not recreate the user-deleted release-notes file; GitHub releases own published notes.
- Build frontend with Node 24, backend with pinned Rust, and run only the Rust executable in
  a minimal non-root production image. Preserve port 8080, user 1000:1000, data/log mounts,
  health check and both amd64/arm64 builds. Do not invent a nonexistent Docker target.
- Rewrite README, CONTRIBUTING, AGENTS, release/development docs around the actual final
  architecture. Remove obsolete plans/instructions rather than keeping conflicting guides.
  Preserve LICENSE, attribution and frontend font/icon notices.
- Audit tracked files and ignored generated directories before deleting only verified
  obsolete development artifacts inside the workspace. Never remove data, logs or backups.

## Implementation sequence and commit boundaries

1. **Plan and contracts:** finish this behavior map, record user removals and dependency
   decisions, establish sanitized fixtures and acceptance coverage. Commit the plan/removals.
2. **Foundation and domain:** Cargo package, typed configuration/DTOs/domain/policy, atomic
   stores, compatible history/archive loading and domain regression tests.
3. **Security and dashboard server:** auth/origin, APIs, authenticated Socket.IO, static
   assets and Rust fixture; run existing frontend tests against the real new server boundary.
4. **Twitch integration:** library-backed device login and session persistence, HTTP/GQL,
   inventory/recovery, channel watch transport and topic shards with mock endpoint tests.
5. **Mining lifecycle:** supervisor, scheduling, selection/manual failover, progress/claims,
   cancellation/logout/shutdown and persistence integration. No placeholder production routes.
6. **Frontend and repository cutover:** remove notifications and unused contracts, change
   build/test/release/contributor/Docker paths, replace docs, delete the retired backend and
   obsolete artifacts only after their useful behaviors have tests in the new implementation.
7. **Validation and adversarial review:** full final baseline, dependency/secret/dead-code
   audit, independent review and fixes, current-main integration, PR CI and Docker validation.
8. **Merge and handoff:** merge only with green required checks and no unresolved review
   blockers; prepare a reviewed source/image-pinned PowerShell installer after read-only
   deployment inspection. User performs interactive sudo. Do not deploy on their behalf.

## Acceptance and test matrix

| Area | Required evidence |
| --- | --- |
| Configuration | defaults, existing-file round trip, narrow filter migration, Unicode casefold/dedup, partial changes, revisions, failure rollback, no retired credentials in API |
| Domain | timing boundaries, prerequisites/cycles/missing nodes, benefit filters, ignores/shared branches, zero-minute rewards, real ACL vs discovery limits, special categories |
| Inventory | valid/null/empty catalogs, missing details, whole-record precedence, nullable neighbors, recovery limits/timeouts, account evidence, cancellation |
| Selection | empty allowlist, saved priority, no automatic additions, manual/fallback, offline ties, nullable viewers, retained watching row during rebuild |
| Watch and claims | both beacon formats, exact decoded event, HTTP failure, no video requests, CurrentDrop/WS confirmation, 15-minute estimate ceiling/reset/reselection, idempotent claims/history and immediate queue refresh, independent claims and strict 24-hour grace |
| Session lifecycle | fresh/expired/denied device flow, restart validation, correct client identity, refresh where applicable, quality-scaled timeouts/proxy reconfiguration, user-controlled refresh interval, logout during discovery/watch/reconnect/claim and concurrent shutdown |
| Persistence | existing history/auth/archive fixtures, restart, atomic failure, corrupt-file preservation/fail-closed auth, durable archive invalidation, data-dir exclusivity |
| Web security | every guarded route, auth lifecycle, scrypt compatibility, cookie attributes, expiry/revocation, hashing limits, both socket transports, CSRF/origin/public URL and secret-free errors |
| Dashboard | full retained Playwright/axe and Vitest suites, desktop/phone/short-height behavior, realtime/reconnect, autosave races, no notification controls, production fixture routes absent |
| Packaging | clean checkout frontend + Cargo build, fmt, Clippy with warnings denied, all Rust tests, locked dependencies, language JSON checks, script/contributor tests, amd64/arm64 Docker CI |
| Cleanup | no retired runtime sources/tools in tracked tree or production image; no unused production dependencies; links/commands/license attribution valid |

Use temporary data and synthetic Twitch responses. No test sends a real watch event, claims
a real drop, or changes the home server. A live device login/mining check requires the user
after installation; automated parity evidence is not described as proof of live earning.

## Completion record

Keep this section updated with actual decisions, commands, reviewed commit, CI links and
remaining limitations. Do not mark implementation complete because it compiles or because
the deadline is inconvenient. Any unfinished production behavior or security/review gate
keeps the PR in draft. The final handoff always includes the PowerShell update command.

Plan review: the independent reviewer identified three parity details before implementation:
claiming independent of mining selection with the 24-hour grace, the 15-minute estimate
ceiling with recovery, and wiring retained network/refresh controls into real behavior.
All three are included above and have explicit acceptance cases. No architecture blocker
was reported. Final implementation review remains required.
