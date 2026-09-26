# Browser login and server renewal

This optional recovery uses a native Chrome login to obtain campaign access that
Smart TV authorization may lack. It adapts the MIT-licensed upstream implementation
at [9eec0c0](https://github.com/rangermix/TwitchDropsMiner/commit/9eec0c0535e5f0fd829cdb115e8102d9a9708336).
It is experimental: Twitch can reject or revoke sessions, and long outages may
require another desktop login. A successful import checks identity, Inventory and
the campaign catalog directly with Twitch before saving anything.

## Prepare the miner

Enable a dashboard password in Settings. Run the ordinary production image with
`TDM_SESSION_IMPORT=1`; no Chrome process runs inside the miner. Existing valid
Smart TV credentials remain usable until a validated import replaces the active
session. They are kept separate from browser credentials. Without a saved device
session, the miner waits for import. Do not delete your existing cookie file.

## One-time Windows connection

Use a dedicated Chrome profile, not your everyday browser profile. Run PowerShell:

```powershell
$private = Join-Path $env:LOCALAPPDATA 'TwitchMiner\BrowserLogin'
New-Item -ItemType Directory -Force -Path $private | Out-Null
$identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
icacls $private /inheritance:r /grant:r "${identity}:(OI)(CI)F" 'SYSTEM:(OI)(CI)F'
if ($LASTEXITCODE -ne 0) { throw 'Could not protect the login directory' }
$chrome = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
Start-Process $chrome -ArgumentList @('--remote-debugging-address=127.0.0.1', '--remote-debugging-port=9223', "--user-data-dir=`"$private\profile`"", '--no-first-run', 'https://www.twitch.tv/drops/campaigns')
```

Log into Twitch in that window, complete any verification, and let the campaign
page load. Never expose the debugging port outside localhost. In a terminal at
this repository, activate `env`, then export:

```powershell
python -m src.auth.session_helper export --browser http://127.0.0.1:9223 --output "$private\tdm-session.json" --server-seed "$private\server-seed.json"
```

Keep the directory restricted to your Windows account and SYSTEM. POSIX systems
use owner-only directories/files; Windows uses the directory ACL above. The helper
exports only the matching request context and scoped SDK cookie, never your whole
browser profile. It validates catalog access independently. Output contains only
success and expiry, not credentials. Close the dedicated browser after exporting.

In Settings → Twitch account → Browser login, choose `tdm-session.json`, then
download the renewal connection. A new download invalidates the previous helper
connection. The session file, connection and server seed are credentials: do not
commit them or paste them in chat. Import promptly before the short-lived session expires.

## Home-server helper

Build the optional helper from the same checkout as the miner:

```sh
docker build --target renewal -t twitch-miner:renewal .
```

On hosts using Docker's legacy builder, pass the native build platform explicitly:

```sh
{ printf 'ARG BUILDPLATFORM=linux/amd64\n'; cat Dockerfile; } |
  docker build --target renewal -f - -t twitch-miner:renewal .
```

Privately transfer `server-seed.json` and the downloaded connection to a directory
named `renewal-state`, as `server-seed.json` and `renewal.json`. Set directory mode
700 and both files to 600. In `renewal.json`, set only `endpoint` to
`http://127.0.0.1:8080/api/session/renew`; preserve its other fields. Start:

```sh
docker run -d --name twitch-miner-renewal --init --stop-timeout 20 \
  --restart unless-stopped --network container:twitch-drops-miner --shm-size=256m \
  --mount type=bind,source="$PWD/renewal-state",target=/state \
  twitch-miner:renewal
```

No browser or debugging port is published. Chromium runs only during renewal, with
an isolated temporary profile. The helper checks its pairing before each browser
launch, validates the renewed account/catalog, saves replacement SDK state, and
normally renews five minutes before expiry. Recreate the helper whenever the miner
container is recreated so it joins the new container's network namespace.

Log out of Twitch to stop miner account work, delete the saved imported session and
invalidate its pairing. In-flight renewal cannot reinstall it. A revoked helper
fails its next pairing check before launching another browser. A capture already
in flight may finish, but delivery is rejected. For permanent disconnection, stop
and remove the helper and securely delete its seed/connection and desktop exports;
the miner cannot erase files owned by those separate processes.

## Verification

Check that Settings reports campaign access verified, the Campaigns page loads a
complete catalog, and the renewal generation increases after a helper cycle. Then
verify actual watched minutes in Twitch's inventory. Mocked tests and a successful
catalog query do not prove earning or claiming; observe those separately.
