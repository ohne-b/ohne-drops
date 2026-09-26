# Operations and upgrades

## Persistent data

Stop the miner and back up the entire data directory before changing an installation.
Only one process may use a data directory at a time; an OS file lock enforces this.

| File in `data/` | Purpose |
| --- | --- |
| `settings.json` | Existing preferences, selection order, filters and ignore rules |
| `drop_history.json` | Compatible version-1 claimed-reward history |
| `completed_campaigns.json` | Compatible version-1 completion snapshots |
| `web_auth.json` | Compatible password hash and dashboard session digests |
| `twitch_session.json` | Versioned OAuth credentials for device login |
| `pending_claims.json` | Account-scoped intents/receipts for interrupted claim recovery |

The Rust migration accepts the existing settings/history/completion/auth formats and
requires **one fresh Twitch device-code login**. Confirmed live progress comes from Twitch.
Existing credential files and backups are left untouched for rollback. An unreadable new
Twitch session is preserved under a separate `twitch_session.invalid-*.json` name before
offering fresh login. Corrupt history/archive files are preserved; corrupt dashboard
authentication fails closed. Settings that cannot be read are never silently reset.

The retired Telegram integration is removed: no worker, API, controls or outgoing delivery.
Its old settings fields are omitted from responses and removed on the next successful save.
There is no browser-session import, browser renewal service, or dashboard installer/updater.
Maintenance can check releases; installation remains an explicit terminal operation.

Keep all data private, including settings containing proxy credentials. Never attach an
entire data directory, OAuth/device codes, cookies, passwords, or unredacted logs to an issue.

## Dashboard protection

Password protection defaults off. Enable it in Settings before exposing the dashboard to
other users. Passwords use salted scrypt; session tokens are stored as SHA-256 digests.
Sessions expire after 30 days; Remember me adds a matching browser-cookie lifetime.
Password changes revoke other sessions, disabling protection requires the current password,
and dashboard logout is separate from Twitch logout. Use HTTPS for remote access.

`PUBLIC_BASE_URL` accepts one absolute HTTP(S) root URL, for example
`https://drops.example.com`. It defines the allowed browser origin and Secure-cookie
behavior behind a proxy. It does not trust forwarded client-IP headers, allow subpaths,
or permit wildcard origins. Requests derived directly from Host remain the default.

Local password recovery: stop the miner, restrict network access, back up and remove only
`data/web_auth.json`, restart, and set a new password. Preserve all other files.


Deployment changes require rebuilding the image and recreating the container. Build while
the current container runs, back up Compose and data before replacement, verify the new
image and health, and keep the previous image/configuration available for rollback.
Preserve existing mount paths, ownership and network binding. Interactive sudo belongs
in the operator?s terminal, never in chat or a saved script.
