# Contributing

Read this guide before planning, editing, testing or reviewing. Its checklist applies to
people and coding agents. The canonical repository is `ohne-b/twitch-miner`, targeting
`main`. Preserve the project's [MIT license](LICENSE), full upstream license in
[NOTICE.md](NOTICE.md), attribution and frontend asset licenses. Distribute both root
license files with production images.

This is a personal, self-hosted hobby project. Multiple accounts, channel-points mining,
a desktop GUI and services hosted for other users are outside the current scope.
Discuss substantial features or refactoring with the maintainer first; an explicit task
authorization covers its necessary implementation and cleanup.

## Report a problem

Search [issues](https://github.com/ohne-b/twitch-miner/issues) and
[PRs](https://github.com/ohne-b/twitch-miner/pulls) first. Include version/commit, OS,
installation method, steps, expected/actual behavior and minimal redacted evidence.
For mining issues, distinguish displayed progress from Twitch inventory progress; include
campaign eligibility, filters, selected games and simultaneous manual viewing.

Never post credentials, device codes, cookies, passwords, an entire data directory or
unredacted logs/settings. Report suspected vulnerabilities privately. Feature requests
should explain a concrete user problem, proposed behavior and acceptance criteria.

## Development

Install Rust through rustup and Node.js 24. `rust-toolchain.toml` pins Rust and its
fmt/Clippy components. Windows needs the Visual Studio C++ build tools. From the root:

```bash
npm --prefix frontend ci
npm --prefix frontend run build
cargo run --locked -- --host 127.0.0.1
```

Running the normal executable contacts Twitch and can claim rewards. Automated checks
must use mocked transports and temporary storage. Never reuse a live miner for testing.

| Location | Purpose |
| --- | --- |
| `src/domain.rs`, `src/policy.rs`, `src/miner.rs` | Eligibility and owned mining lifecycle |
| `src/twitch/` | OAuth, HTTP/GQL, inventory, channels and PubSub |
| `src/store.rs`, `src/config.rs`, `src/auth.rs`, `src/origin.rs` | Persistence, settings and security |
| `src/web/`, `src/dto.rs` | Axum/Socket.IO dashboard boundary |
| `src/fixture.rs`, `src/bin/dashboard-fixture.rs` | Offline browser fixture |
| `frontend/`, `lang/English.json` | Dashboard and English messages |
| `.github/` | Validation and release automation |

Use concrete Rust structs with methods/composition and shared policies; keep business
logic out of route handlers. Preserve async cancellation, bounded work, validation,
credential redaction and atomic disk-before-memory updates. Consult [AGENTS.md](AGENTS.md)
for detailed domain, security and UI contracts.

Edit frontend sources, never generated `web/`. Update README and AGENTS for relevant
behavior/architecture changes, and English messages when UI/console text changes. Render
translations as React text with validated links. Commit dependency lockfiles and avoid
unrelated upgrades. Cargo owns version/lock consistency; Vite owns asset hashes.

## Pull requests

1. Inspect the working tree and preserve others' changes. Start a descriptive `feat/` or
   `fix/` branch from current canonical main. Use conventional commits without assistant branding.
2. Implement one coherent change with backend unit/regression tests and frontend coverage
   where practical. Reproduce bugs first; test meaningful success and failure behavior.
3. Fetch and integrate current main before final validation/review, and again before merge
   if it advances. Resolve conflicts deliberately and rerun affected checks.
4. Obtain independent adversarial review. Keep an incomplete PR in draft.
5. Submit through a PR; ordinary changes never go directly to main. Follow through on
   findings and CI. Release publication requires separate explicit authorization.

For a writable origin pointing to this repository:

```bash
git fetch origin
git switch -c fix/short-description origin/main
# Before final review/merge:
git fetch origin
git merge origin/main
```

For forks, use `upstream` pointing to the canonical repository in place of `origin` for
integration, and push to your fork. Rebase is acceptable on a branch you own; coordinate
before rewriting shared history and use `--force-with-lease` only when authorized.

## Required validation

Run focused tests during development, then the baseline for code PRs:

```bash
npm --prefix frontend ci
npm --prefix frontend run format:check
npm --prefix frontend test
npm --prefix frontend run build
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
node --test .github/scripts/test/*.test.mjs
node .github/scripts/release.mjs read
node .github/scripts/release.mjs artifacts "$(node .github/scripts/release.mjs read)"
git diff --check
cd frontend
npx playwright install chromium
npm run test:browser
```

Playwright starts the Rust fixture with its explicit feature on loopback port 8765,
verifies readiness/reset, uses temporary data, and refuses server reuse. Vitest and
Playwright/axe cover frontend logic, browser behavior and accessibility. No automated
test needs credentials, sends real watch events, claims real rewards or contacts bots.

CI also builds and smoke-tests production images for amd64 and arm64, including UID/GID,
licenses and isolated health. Required checks must pass on the final PR revision. A health
check or mock test does not prove live Twitch earning. Disclose unrun/unavailable checks,
skips and limitations; do not substitute a green workflow for unperformed validation.

Select regression coverage according to the actual change:

| Area | Evidence |
| --- | --- |
| Mining | Timing, ACLs, prerequisites, ignore/selection, claim recovery and late responses |
| Persistence | Existing-file round trips, atomic failures, defaults, corruption and restarts |
| Security | Authorization, CSRF/origins, cookies, expiry/revocation, rate limits and redaction |
| Frontend | Build/types, unit/browser/axe, reconnect/autosave and responsive/focus states |
| Tooling | Lock/version agreement, script contracts, workflow trust boundaries and image builds |
| Docs only | Accurate commands/links/claims and readable Markdown; normal PR CI still runs |

For documentation-only changes, the final row replaces the local code baseline. After
review fixes or main integration, rerun affected checks; do not cite superseded results.

## Independent adversarial review

Every PR, including docs-only changes, needs a human or separate review agent that did
not author it. An author's reread does not qualify. For agent-authored work, use a separate
read-only reviewer when available; otherwise request an independent human. If neither is
available, keep the PR in draft and state the gap. One implementation agent can still use
a separate independent reviewer.

Provide the goal, final diff/revision, these instructions, acceptance criteria and actual
test evidence. Ask for counterexamples involving correctness, security/privacy, missed
edge cases, regressions, inadequate tests, scope and documentation claims. Record reviewer
identity/role, reviewed commit, findings and resolutions. Explain rejected findings with
evidence. Have substantive fixes and later relevant changes rechecked. Unresolved blockers
prevent readiness. Agent review does not replace required GitHub/maintainer approvals.

Use the [PR template](.github/pull_request_template.md). Include incorporated main and
tested head commits, exact validation results, relevant screenshots, independent review,
and remaining limitations. Stage only intended files. Never fabricate results or weaken
checks/policy to appear complete.

- [ ] Latest canonical main is integrated and conflicts resolved.
- [ ] Diff is focused and contains no secrets, local data or unrelated changes.
- [ ] Applicable unit/frontend/regression coverage is included.
- [ ] Required checks pass on the final revision; gaps/skips are disclosed.
- [ ] README, AGENTS, English messages and relevant workflow docs are current.
- [ ] Independent adversarial review is recorded and blockers resolved/rechecked.
- [ ] PR description matches the final implementation and evidence.

## Release and automation

Version ownership is `Cargo.toml` and `Cargo.lock`. **Prepare release**, manually run on
main, uses `PUBLISHER_TOKEN` to create a draft version PR whose checks run normally. The
token needs repository contents/PR access; configure it as a secret, never in source.
Add a concise, reviewed `CHANGELOG.md` entry to that draft PR, matching the existing
version/link/date heading and change-list style. Review and merge it under the same policy.
**Publish release** then runs manually
from main for that version, requires successful push or manually dispatched validation on the exact commit,
and uses the `prod` environment. It builds both architectures, publishes
`ghcr.io/ohne-b/twitch-miner:VERSION`, and creates a `Twitch miner vVERSION` draft release
with the reviewed changelog notes, comparison link and issue link. It attaches and verifies
`latest.json` before publication. The manifest uses `schemaVersion: 1`, a canonical SemVer
`version`, and a `notes` link; it contains no installer or executable commands. This applies
to stable releases and prereleases. Stable releases update `latest`; prereleases do not.
The first GHCR package may
need public visibility configured for anonymous pulls. Ordinary merges publish nothing.
Commits made with a workflow token do not trigger push workflows; run **validation**
manually on main before publishing when its latest commit has no matching push validation.
SemVer build metadata uses `_` in place of `+` in the Docker tag.
To publish the same multi-architecture build to Docker Hub, create a public repository
and set Actions repository variables `DOCKERHUB_IMAGE` (`namespace/repository`) and
`DOCKERHUB_USERNAME`, plus secret `DOCKERHUB_TOKEN` (a Read & Write personal access token).
Leave `DOCKERHUB_IMAGE` unset for GHCR-only publication. Missing enabled credentials or
failed login stop before the build; repository write access is checked by the actual push.
Both registries receive the version tag; only stable releases advance their `latest` tags.
The release and its verified manifest become public before registry `latest` tags advance.
Registry writes are not transactional: a failed multi-registry push can leave version tags,
and a failed promotion can leave one `latest` behind. Inspect the failed run and registry
digests before retrying. If a run leaves a draft, verify its target commit against the
validated revision and download/compare `latest.json` with the generated artifact before
finishing its publication. Do not delete or rewrite published tags, or rerun the full
workflow over an existing release. If publication succeeded but a `latest` promotion
failed, verify the public release/tag, manifest and image revision/digest against the
successful build, authenticate to the affected registry, and finish only that promotion:

```bash
docker buildx imagetools create --tag REGISTRY/IMAGE:latest REGISTRY/IMAGE:VERSION
```

Maintenance reads the latest stable release's manifest with bounded requests and a short
shared cache. Unknown/unreachable metadata must never be reported as up to date. It only
offers release notes and manual checks; installing updates remains a terminal operation.

Do not rewrite published tags or bypass checks. Revert source through a normal PR; an
installation rollback redeploys a previously validated image with its backed-up data.

Keep upstream attribution and license links in README; contributor/PR tables are not
maintained. README changes follow the same PR workflow as other documentation.
Keep Buildx/Build Push action pins consistent between validation and release workflows.
Do not alter trust boundaries in ordinary contributions.

Agents must pass this policy to reviewers, preserve existing user changes, and distinguish
editing authorization from PR/merge/release/deployment authorization. Final handoffs report
actual changes, tests, review and unfinished work honestly.
