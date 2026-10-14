# Super Platinum: resume on the Windows laptop

## Windows continuation, 2026-10-14

Latest logic gate: **1,185 core tests / 5 ignored**, **1,110 desktop tests /
7 ignored** and both format checks pass. Twelve keyboard and six paste-runner
tests pass. The last completed dev build took **2m54s** (SHA
`4ed2dbf34851f27c7acea71ce2da80037a5096339824327eb6dfe9772e657b33`).
Trusted Windows keyboard editing and synthetic file-paste reruns pass at both
sizes with all 22 PNGs inspected in `windows-{keyboard,file-paste}-window-final/`.
Foreground restoration was requested but not observed; OS IME and the system
clipboard are not covered. Pasted filenames currently expose staging prefixes;
separate display/upload filename preservation is the next implementation slice.
Multi-conversation native wire checks pass all four fixtures with 26 inspected
PNGs. Corrected compact triggers now pass all eight normal/narrow fixtures,
with all 50 PNGs inspected in `windows-multi-{channels,conversation}-compact-final/`.
Keyboard and file-paste first native runs failed fixture invariants; corrected
full-row ownership checks now pass native reruns. Current-conversation
multi-selector defaults, ordinary autosave and actual OS IME/clipboard remain open.

The previous `9089c0f...` build passed normal/narrow multi-channel checks with
all 24 PNGs inspected in `windows-multi-channels-final/`, all four scoped contrast
fixtures with 12 inspected PNGs in `windows-contrast-final/`, and all 16 shared
picker regressions with 46 inspected PNGs in `windows-picker-regression-final/`.
Contrast evidence covers named actual/probe controls, not whole-theme certification.
Rejected-thread restore/restart now passes normal and narrow with all ten PNGs
inspected in `windows-rejected-thread-focused-final/`; earlier notice failures
are resolved for these flows. Original SQLite snapshot/broadcast and unrelated
drafts survive separate processes; no send occurs. Actual later-edit autosave,
OS IME and live/crash fault proof remain open. Canvas text/checklist native
checks now pass at both sizes with all 20 staged/final PNGs inspected in
`windows-canvas-final/`. File-operation recovery also
passes normal/narrow three-process checks with all six PNGs inspected in
`windows-file-recovery-final/`; exact rename data remains unchanged and only
explicit deletion retirement changes its receipt.

The user resumed on native Windows at `37d71bb` (`resync`). The checkout was
clean and all six named capture/Dock transfer artifacts below are tracked and
present. The native Rust host is `x86_64-pc-windows-msvc` (Rust 1.98.1).
Native dev compilation, isolated TCP ping/state, and full DPI-correct screenshot
capture now pass. PowerShell argument parsing and DPI coordinate bugs were fixed.
The first inspected captures also exposed broken Windows media URLs; the desktop
now adapts canonical IDs to Wry's exact synthetic HTTP origins and CSP sources.
Native checks prove ready/pending decode and same-element generation refresh.

Activity cursor reconciliation below is implemented, including revision guards
for channel cursor ABA (forward then backward while an HTTP mark is pending).
Activity Windows gates: **1,101 core tests / 5 ignored**, **959 desktop tests /
6 ignored**, both format checks, whitespace/Python syntax checks, and locked dev
build (**3m14s**). Normal/narrow Activity navigation, cursor/page reconciliation,
and media checks passed; root inspected their PNGs. Receipts are under
`tmp/windows-bootstrap/` and `tmp/agent-ui/windows-{activity,media}-final/`.
The earlier 947-desktop/1,098-core runs preceded final edits and are not final gates.

Ordinary-message `multi_users_select` and overflow are now implemented. Final
logic gates passed **1,112 core / 5 ignored** and **971 desktop / 6 ignored**;
the dev build passed in **3m01s**, followed by a **1m56s** compact-control layout
build. All five control families passed normal/narrow native checks; root inspected
all ten PNGs, then rechecked both corrected multi-user captures. Receipts are in
`tmp/agent-ui/windows-controls-final/`. Three before/after synthetic channel-scroll
runs show a 6.1ms median in both builds; no whole-app improvement is established.

List **Duplicate item** is now implemented through the source-verified create
endpoint, canonical adjacency and readback, with retained delivery locks across
same-user credential/transport rotation. Final gates: **1,118 core / 5 ignored**,
**981 desktop / 6 ignored**, both format checks and dev build **3m03s**. Normal
and narrow List interaction/duplicate and delete/undo checks passed; the team
inspected all **60 PNGs** in `tmp/agent-ui/windows-lists-final/`. Root additionally
inspected all four new duplicate screens and five narrow deletion captures.
Live server copy/subtask semantics and unsupported/incomplete List orderings
remain explicit boundaries.

Readable List timestamps are now implemented from captured client code, including
relative labels, tooltips, locale/time24 and visible minute refresh. The final
independently locked core suite passed **1,124 / 5 ignored**. Normal/narrow List
checks passed and the team inspected all **50 PNGs** in
`tmp/agent-ui/windows-timestamps-final/`. Windows taskbar badges now use a direct
HRESULT-reporting sink with retained recreation handling. Eleven native stages
passed; visible taskbar captures verify numeric/unread/clear states. The first
capture attempt exposed auto-hide, so the runner now offers temporary edge hover
with cursor restoration, without changing taskbar settings.

Windows toast identity and direct WinRT submission are implemented. The first
native attempt found `Setting` unavailable before the first `Show`; the targeted
fix passed **996 desktop tests / 6 ignored** and a **2m33s** dev build. Native
submission/history/removal and registration restoration passed; root inspected
the actual banner at 0.5 and 5 seconds in `windows-notifications-timed/`.
The immediate capture preceded the animation. Production taskbar projection now
passes six native stages with all six taskbar PNGs inspected (6/clear/4/clear/4/clear).
Account-owned synthetic COM routing passes queued latest destination, exact thread
reply visibility and foreign-account rejection; root inspected its PNG. Actual
warm OS toast activation now passes: one exact fixture Invoke produces one COM
callback and restores the same minimized PID/HWND to the foreground. Root
inspected the nonce-bearing banner and activated-window PNG. Exact registration
and preview cleanup pass. Receipts: `windows-notification-activation-final/`.
Cold activation now passes **1,029 desktop tests / 6 ignored**, format and a
**3m30s** dev build, followed by actual Windows OS activation. One exact fixture
toast click launches a new PID after its publisher exits, delivers one callback,
and brings the child HWND forward. Root inspected the nonce-bearing banner and
fully rendered channel at one and three seconds. Child cleanup restores all four
registration artifacts, removes the preview and releases the lease. Warm activation
and synthetic routing regressions also pass with inspected PNGs. A first-attempt
runner bug querying a terminated image was fixed; exact recovery and idempotent
recovery both passed before the successful fresh-nonce runs. Evidence:
`tmp/agent-ui/windows-notifications-cold-painted/`. Selected custom sounds, real
Explorer restart, live account-owned routing and live delivery remain open.

Modal multi-user inputs are implemented. The independently locked core suite is
now **1,131 passed / 5 ignored**; the desktop suite is **1,013 / 6 ignored**.
Native testing found and fixed two concrete issues: by-value bootstrap future
copies overflowed Windows' 1 MiB main stack, and Escape from inline member choices
also closed the modal. Startup now boxes its future at the Dioxus hook boundary;
the largest startup spawn frame falls from 623,000 to 440 bytes, and plain-channel
TCP startup passes. The modal Escape fix uses the existing local Escape marker;
its full desktop tests, **2m28s** dev build and normal/narrow native checks pass.
All **ten** final modal PNGs were inspected, including a separate validation
capture with verified visible error text and hit testing at both sizes. The
final validation dev build passed in **2m07s**, with 1,013 desktop tests unchanged. All six
shared user/multi-user/conversation normal/narrow checks pass with inspected PNGs.
An additional 15s idle sample records host/WebView2 CPU **0.03125/0.0625s**,
total owned working-set/private-commit **530.71/311.04 MiB**, and first paint
**560.86ms**. It does not demonstrate improvement over the earlier single sample;
WebView2 memory is higher. Receipts: `tmp/agent-ui/windows-idle-modal-os/`.
Single-user modal support now passes **1,137 core / 5 ignored**, **1,017 desktop /
6 ignored**, both format checks and a **2m58s** dev build. Single/multi-user modal
checks pass at both sizes with all **18 PNGs** inspected; shared message pickers
add six passing checks and inspected PNGs. Receipts: `windows-modal-user-final/`
and `windows-single-shared-final/`.
Fixtures now use a unique Windows WebView2 directory and in-memory picture cache;
the pinned runtime otherwise shares the production profile despite TMP isolation.
Conversation and public-channel modal selectors are implemented with retained
opening-channel defaults, distinct state keys, full-state response URL flags and
Enterprise-context submission guards. **1,153 core / 5 ignored**, **1,059 desktop / 7 ignored**, both format checks
and 11 JavaScript tests pass; the dev build passes in **3m28s**. Scheduling now uses
a typed DOM receipt before local settlement, merges attachment-only edits, and
rejects stale same-channel returns. These guards passed independent review. Draft polling compares
metadata before cloning retained content; its paired component benchmark improves
unchanged/sparse-change cases and reports slower all-emitted cases in the worklog.
Both selector families now pass normal/narrow Windows checks with **20 inspected
PNGs**; modal user regressions and shared message pickers add **24 inspected PNGs**.
Scheduling passes all three fixtures with root-inspected captures. GUI-subsystem
PE=2, cold/warm activation, synthetic exact-reply routing and completed recovery
all pass with inspected captures and unchanged registration. Final binary SHA:
`02f559d717f400e36c83676bd0fb25dbc620cafe21a533fdfa121fbd680ddc32`.
Three interleaved idle pairs now confirm the GUI build has no owned console host;
CPU, private memory and first-paint differences vary across pairs. Scroll p50 is
6.1ms in both builds, but a blank transcript in A3 and a hover card in B1 prevent
clean visual comparison. A separate cursor-normalized repeat now passes all six
runs and 18 inspected immediate/delayed PNGs: both builds have 6.1ms p50 and
6.2ms median p95. No improvement is demonstrated; the original blank pane did
not reproduce and its cause remains unknown. Preserve both sets of artifacts.
The next source slice adds rejected thread draft restoration with isolated
restart proof and ordinary-message public-channel selection.
This next slice now passes **1,157 core / 5 ignored**, **1,074 desktop / 7 ignored**,
both format checks and a **3m14s** dev build, SHA
`4574d647738d5d329262ac14e06bca77eec9fa4511d1901ba967d4a73a190c59`.
Message channel selectors pass normal/narrow Windows checks with both PNGs read;
large channel/five thread fixtures and upload/forward recovery also pass with
inspected captures. Thread-return native verification exposed a success toast
covering its composer. The first placement repair passes normal restore/restart
on build SHA `a0425878533e4366b311d333a34e379ca7bc5a1aeb90146bdce904f26e30755e`
(1,074 desktop tests, ten JS tests, 2m54s dev build), but narrow restoration
exposes a notice covering focused Continue writing before the composer opens.
Focused-control clearance is now being fixed. Canvas text/checklist Windows
runners and capture handshakes are prepared; their new fixture source is not yet
compiled. Current source edits exceed the latest tested binary.
Continue the full acceptance backlog. Windows session files exist (names
only inspected), but no live sign-in/reference or cross-client proof is established.
Private cursor/app-control/duplicate proof and the broad acceptance matrix remain open.
No real Slack/session/draft writes, commit, push or packaging
occurred. Keep the dirty tree and new source/script files; HEAD alone omits this
continuation. Historical Mac details below remain useful context.

Handoff prepared on **2026-10-13** from `/Users/sh/Documents/GitHub/snack` on the
user's MacBook. The user explicitly **paused** the long-running goal to move
machines. Resume implementation when the user asks to resume on the new machine.
The project is **not complete**, and no defensible overall completion percentage
has been established.

## Start here

1. Read this file, the repository `AGENTS.md`, and `docs/design-system.md`.
2. Confirm that the **working tree**, including untracked source files, arrived.
   A fresh clone of the current commit alone loses the latest work.
3. Build and run an isolated **dev copy on native Windows**. Establish Windows
   compilation, agent TCP access, WebView2 rendering, and screenshot capture
   before interpreting old macOS evidence as applicable to this machine.
4. Windows Activity cursors, ordinary-message multi-user/overflow, List Duplicate
   and timestamps are implemented and fixture-verified. Finish **modal multi-user
   and Windows toast activation** proof (production taskbar projection passes),
   retaining private cross-client
   verification as an open acceptance requirement.
5. Keep the full parity/design/performance objective intact. Use the acceptance
   table and chronological receipts in `docs/parity-worklog.md`; later entries
   supersede earlier hypotheses and open items.

## What the user wants and how to work

The original objective is full Slack feature parity, a premium compact interface
inspired by [Vercel Geist](https://vercel.com/geist/introduction), and measured
performance improvements. This is a native Rust/Dioxus Slack client, not an
Electron wrapper or a static visual clone.

- **Use dev builds.** The user explicitly asked us to stop spending time building
  signed app bundles. Use `cargo build --locked` and the built executable for
  normal iteration. Old README/worklog references to signed Mac builds do not
  override this preference. Packaging is not an ordinary UI verification gate.
- Work autonomously, implement scoped fixes, and distinguish source evidence,
  unit tests, native fixtures, and live cross-client proof.
- The user requested parallel agents, up to **20 maximum**. The last runtime
  actually allowed only root plus three workers. Use the available limit; do not
  assume 20 concurrent slots. Divide file ownership and keep one integration
  owner. Previously root alone ran Cargo/builds/native/live checks to avoid
  competing builds and disk pressure.
- Real message sends are authorized **only in private `#echotestchamber`,
  channel ID `C0AQDL3V5GU`**. Other test writes must also stay in private artifacts.
  Do not use arbitrary public channels for testing. Reading/focusing conversations
  can mark them read; do not navigate the user's active client casually.
- Real huddle joining needs a human in the loop. Global preferences, notification
  schedules, profile changes, and unrelated production state are not blanket
  write-testing authorization. Implement and verify those in disconnected
  fixtures unless separately authorized.
- Preserve user drafts, sessions, unrelated edits, and the active app. Do not
  commit/push, deploy, or kill unrelated processes without authorization.
- The user offered `/Users/sh/Documents/GitHub/multiSlack` as inspiration.
  It is an optional separate reference repository, not a dependency. Its path
  will need replacing on Windows if it is transferred.

## Exact checkpoint and transfer requirements

At handoff, Git branch is **`main`**, HEAD **`26003ce` (`sync`)**. There are tracked
modifications and untracked source trees. Earlier parity work is already present
in the checkout/commit history; the newest capture/Dock work is still dirty.
No commit or push was made for this handoff.

Run `git status --short` on both machines and compare. In particular, transfer:

- `crates/super-platinum-core/src/state/dock_badge.rs` and its directory.
- `src/desktop/notification/badge.rs` and its directory.
- `src/desktop/state/dock_badge.rs` and its directory.
- `src/desktop/agent/capture.rs` and its directory.
- `scripts/check-dock-badge-native.py` and `scripts/check-capture-responsiveness.py`.
- All tracked edits accompanying those modules: manifests, module declarations,
  constructors, counts/realtime/read paths, notification adoption hooks, tests,
  and docs. The appendix lists the dirty tracked paths at handoff.
- This `docs/RESUME.md` file itself.

Do not rely on `git diff` alone to transfer work: it omits untracked files.
Do not copy the Mac `target/` or `crates/super-platinum-core/target/` directories
to Windows; rebuild native artifacts there. Do not copy stale socket markers as
if they were live endpoints.

`tmp/` and `captures/` are ignored local evidence, not guaranteed to accompany a
clone. Some contain real Slack messages, cookies, signed URLs, or tokens. Keep
them private; never commit or paste raw network captures. The static JavaScript
reference bundles and selected sanitized logs/fixture PNGs are useful to transfer
separately. Protocol docs preserve many findings if local captures are absent.

Authentication is not source code. Sign in normally on Windows rather than
assuming the Mac Keychain/session transfers with Git. In debug builds the current
config code uses `session.secrets.dev.json` under the platform config directory;
release builds use the configured keyring. Do not log or commit either session
secrets or raw captures. `STORAGE_QUALIFIER` and `KEYRING_SERVICE` deliberately
still say **snack**; changing them without migration orphans existing data.

## Windows bootstrap and verification

Current repository CI defines native `x86_64-pc-windows-msvc` and
`aarch64-pc-windows-msvc` builds in `.github/workflows/ci.yml`. Match the laptop's
architecture and native MSVC/C++ build environment. The ARM64 BoringSSL toolchain
configuration is `.github/cmake/windows-arm64.cmake`. Check the actual Windows
toolchain and current Dioxus platform documentation if prerequisites are missing.
Do not treat WSL/Linux validation as native Windows/WebView2 proof.

The Dioxus revision is pinned exactly to
`57d6794ad60b949e5bd8aa282f6f8c3dc97a365e`. Check
[Dioxus 0.7 documentation](https://dioxuslabs.com/learn/0.7/) before changing its
APIs; pinned source and compiler win when docs differ. Use Dioxus's Tao/Wry
reexports, not incompatible separate native types. Keep the existing optimized
dev profile unless measurements justify changing it.

From the repo root in PowerShell, after toolchain setup:

```powershell
cargo fmt --check
cargo fmt --check --manifest-path crates/super-platinum-core/Cargo.toml
cargo test --locked --manifest-path crates/super-platinum-core/Cargo.toml
cargo test --locked
cargo build --locked

$env:SUPER_PLATINUM_AGENT = "1"
$env:SUPER_PLATINUM_FIXTURE = "channel"
.\target\debug\super-platinum.exe
```

The core crate is independently locked (`[workspace]` in its own manifest).
Run its suite separately for domain changes. These commands describe the native
machine's gates; the continuation receipts above name the verified checkpoints.

On Windows the agent server uses loopback TCP, default `127.0.0.1:0`, not a Unix
socket. Optional `SUPER_PLATINUM_AGENT_ADDR` controls the listen address; keep it
loopback. It writes the actual `tcp://host:port` endpoint to
`[IO.Path]::GetTempPath()/super-platinum-agent.sock.path`.
`SUPER_PLATINUM_AGENT_SOCK` is the CLI's endpoint override, not the Windows
server bind setting.

A minimal second-terminal probe, assuming the Python launcher `py` is available:

```powershell
@'
import json, pathlib, socket, tempfile
endpoint = (pathlib.Path(tempfile.gettempdir()) / "super-platinum-agent.sock.path").read_text().strip()
assert endpoint.startswith("tcp://127.0.0.1:")
host, port = endpoint[6:].rsplit(":", 1)
request = {"id": 1, "cmd": "ping"}
with socket.create_connection((host, int(port)), timeout=35) as connection:
    connection.sendall((json.dumps(request) + "\n").encode())
    with connection.makefile("rb") as response:
        print(json.dumps(json.loads(response.readline()), indent=2))
'@ | py -
```

Change the request to `{"id":2,"cmd":"state"}` or
`{"id":3,"cmd":"screenshot","path":"tmp/agent-ui/windows-channel.png"}`
for structural and image checks. Read the generated PNG yourself. The PowerShell
launch/probe recipe was checked against repository code, not executed on Windows
during this handoff. `scripts/agentctl.sh` also supports TCP when run through an
appropriate Bash/Python environment, but its default `/tmp` marker lookup can
differ from native Windows temp; use the explicit endpoint if needed.

Stop only the fixture instance you launched. Remove the fixture variable before
launching a real session:

```powershell
Remove-Item Env:\SUPER_PLATINUM_FIXTURE -ErrorAction SilentlyContinue
$env:SUPER_PLATINUM_AGENT = "1"
.\target\debug\super-platinum.exe
```

Leave destructive agent actions disabled by default. The agent's `help` command
and `crates/super-platinum-core/src/agent_protocol.rs` describe the typed protocol.
There is no general-purpose live DOM-eval agent command.

Important platform gaps:

- Many `scripts/check-*-native.py` runners assume a Unix executable path, `/tmp`,
  Unix sockets, and the shell CLI. Adapt process/endpoint handling before using
  them on Windows; their current existence is not Windows test coverage.
- `src/desktop/agent/capture.rs` has a Windows PowerShell/native-window capture
  branch. Its latest async refactor was exercised on macOS, not Windows.
- The new Dock label and attention modules are macOS-gated. They do not implement
  Windows taskbar badges or prove Windows toast delivery.
- The offline fixture names and in-app check logic remain useful. See
  `scripts/agent-ui-check.sh`, `src/desktop/state/boot.rs`, and the relevant check
  module. Prefer focused fixtures over launching the entire large sweep.

## Architecture and design boundaries

| Area | Ownership |
| --- | --- |
| `crates/super-platinum-core/` | Slack API/realtime/models, cache/config, reducers, domain state, agent protocol |
| `src/desktop/main.rs` | Dioxus application and root hooks |
| `src/desktop/state/` | Serial shell state, projections, bounded windows, selection, composer, fixtures |
| `src/desktop/bootstrap/`, `messaging.rs`, `realtime.rs` | Async Slack orchestration, guarded adoption and persistence |
| `src/desktop/view/`, `styles/`, `overlays.rs` | Typed UI and CSS |
| `src/desktop/media.rs` | Opaque native media protocol and loading |
| `src/desktop/agent.rs` | Optional local control plane |

The visual source of truth is `docs/design-system.md`: local Geist/Geist Mono,
neutral graphite surfaces, compact controls, restrained borders, stable layout,
clear secondary text. Reuse CSS tokens in `base.css` and `design-system.css`.
The user's selected appearance can differ from the default design palette.

All interface icons must use **Google Material Symbols Rounded** via
`src/desktop/icons.rs`. No emoji/text glyph substitutes or hand-drawn SVG icons.
User-authored emoji, avatars, brand marks, and actual message content are content.

## What has been implemented during the larger run

This is an orientation summary, not a fresh claim that every workflow passed on
every platform. Detailed receipts, limits, and historical tests live in the
7,000+ line `docs/parity-worklog.md`. Its top acceptance table is useful but some
rows lag the latest dated entries.

| Workstream | Implemented work and evidence boundaries |
| --- | --- |
| Visual system | Geist fonts/tokens, denser shared surfaces, hover/selection cleanup, normal/narrow fixtures. Full visual parity and accessibility coverage remain incomplete. |
| System messages | Explicit lifecycle/pin subtype allowlist; body uses `--muted`, author remains bright. Shared timeline/thread renderer and native fixture verified. Ordinary chat/app/broadcast content retains normal contrast. |
| Dependencies | Requested updates already present. Current root lock has Tokio 1.53.2 and Rustls 0.23.45; objc2 0.6.5, Tao 0.34.8/Wry 0.53.5 aligned with pinned Dioxus. Historical remote CI covered an older committed dependency baseline, not this dirty tree. |
| Messages/composers | Formatting, Unicode/selection handling, emoji/mention work, thread broadcasts, message controls, retries, scheduling/drafts and local composer recovery. Complex destination/queue and cross-client races still need coverage. |
| Navigation/read windows | Channel/thread anchoring, older exact reply targets, bounded rich rendering with complete cheap row metadata, thread pagination and geometry improvements. Keep canonical data separate from rendered windows. |
| DMs/profiles/channels | Recipient discovery and DM/group-DM opening, presence/profile panes, status text/emoji/expiry, own-profile text/title editors, channel create/about/members/archive/join/leave/invitation actions. Some real mutation round trips remain unverified. |
| Search | Filters, sort, scoped pagination, shortcuts, cold metadata resolution and exact channel/thread destinations. Broader filters and multi-account/outage/live cases remain. |
| Files/Later/Pins/bookmarks | Browsing/paging, upload/download, code/text/audio/PDF previews, own-file rename/delete, saved messages/files/reminders, pin navigation, bookmark links/folders/order. Private API and many native checks exist; full file-format and realtime coverage does not. |
| Recovery | Account/destination-owned drafts; persistent uncertain upload delivery journal; explicit readback/recovery rather than automatic duplicate sends. Preserve newer local edits and retained operation ownership. |
| Forwarding | Rich notes and destination selection, plus a durable account/source/destination journal for uncertain delivery. Restart recovery offers receipt checks, explicit resend and stop tracking; opening recovery never sends automatically. Native recovery checks passed historically; broader live and scheduling parity remains unfinished. |
| Canvases | Reader/create/rename plus bounded checkbox and existing-paragraph/rich-text edits using the actual collaboration protocol; private save/restore proof. Full authoring, comments and collaboration remain incomplete. |
| Lists | Saved tables/boards, record detail, ordinary cell editing, create/delete and same-ID Undo/Redo. Source-backed Duplicate and stable delivery locks now pass Windows normal/narrow checks. Readable timestamp display, server continuation, live duplicate semantics, view writes and richer fields/actions remain. |
| App interactions | Typed app modals and ordinary message buttons, static select/multi-select, radio/checkbox, date/time pickers, user and conversation selectors with retained ownership. Ordinary-message multi-user and overflow now pass Windows normal/narrow native checks. Modal multi-user and live app receipt/response coverage remain incomplete. |
| Notifications | Channel mute/temporary mute/settings, recurring schedules, global defaults/keywords, sound/preview/mute, macOS attention and canonical Activity/DM badges. Real OS delivery and several global live writes remain unverified. |
| Huddles | Slack room + Chime state/media bridge, camera/sharing/device UI. Real call/media proof still requires a controlled human-approved call. |
| Performance | Measured palette, message snapshot, DM lookup, geometry/thread-index and reaction projection improvements; bounded rendering, media loading and timer fixes. Component CPU wins are not whole-app FPS or Windows latency proof. |

## Latest completed and verified slices

### Async screenshots and a disproven rendering hypothesis

The old agent screenshot path blocked the Dioxus main thread while waiting for a
child process. It now awaits bounded `tokio::process` capture, keeps native-handle
access on the main thread, performs file I/O off-thread, preserves serial agent
responses, and kills/reaps timed-out or cancelled children. Files:
`src/desktop/agent.rs`, `agent/io.rs`, `agent/capture.rs`, and its tests.

In an isolated Mac fixture with an injected two-second capture delay:

- Baseline AX resize failed after 1.695s; screenshot finished in 2.142s at the old
  2200×1480 window size.
- Patched resize succeeded in 0.201s while capture was pending; screenshot took
  2.174s and had the new 2000×1400 size.
- An injected 30-second child returned timeout at 10.004s and the exact child
  PID was gone. This is scoped responsiveness/fault proof, not general FPS.

Evidence: `scripts/check-capture-responsiveness.py`,
`docs/runtime-responsiveness.md`, and ignored
`tmp/agent-ui/capture-responsiveness-{baseline-idle,patched}.json`.

**Do not reopen the alleged missing-text repaint bug based on the image tool
alone.** Three separately captured PNGs had identical decoded RGB bytes while
the image viewer presented them differently. The hypothesis was disproven;
`tmp/capture-pixel-audit.json` records the comparison. No repaint hack or generic
DOM-eval command was added. If this recurs, compare actual pixels/independent
display before changing rendering code.

### Activity/DM policy and Dock badge

- Activity Inbox badges use the exact allowed categories and experiment gates;
  reactions and unknown categories are not blindly added. Known Inbox with
  missing authoritative counts does not fall back to a guessed raw total.
- **DM rail and Dock DM are different selectors.** The rail uses eligible
  per-conversation highlights/forced-unread state; the Dock opt-out contribution
  uses the Activity `dm` category. Do not conflate them.
- Canonical HTTP counts and realtime events have revision/owner guards. Omitted
  DM `mention_count` in authoritative counts clears an old numeric count before
  forced-unread fallback; partial metadata refreshes retain merge semantics.
- New core Dock projections consume canonical Activity, legacy channel-badge,
  thread and saved overdue data. Numeric counts win regardless of bullet opt-in.
  Later eligibility/visibility and raw JavaScript preference truthiness matter.
- Saved counts update background workspaces and reject older HTTP replacement.
  A valid saved object missing overdue retires its authority; malformed/null
  input preserves prior authority. Legacy maps merge; Activity maps replace.
- Shell caches update at mutation boundaries. The native hook reads a cached
  scalar/owner, not all channels on every render/tick. Ownership retirement
  clears the badge. Known numeric zero cannot retain a stale number merely
  because ordinary-unread/Enterprise context is unknown.
- **Remaining gap:** authentic unified-user/MRU/org-channel Enterprise context
  is not ingested. Legacy numeric and ordinary-unread branches needing it are
  explicitly unknown. Do not infer it from enterprise ID or current tab.
- The macOS adapter handles decimal label, U+2022 bullet, unchanged suppression,
  account transitions and drop cleanup. Windows taskbar behavior is not added.

Read `docs/notification-presentation-protocol.md` for exact category gates,
defaults, source modules, normalization and aggregation formulas.

### Last full gates at the code checkpoint

| Check | Result / local receipt |
| --- | --- |
| Independently locked core suite | **1,090 passed, 5 ignored**, `tmp/dock-badge-core-tests.log` |
| Locked desktop suite | **964 passed, 6 ignored**, `tmp/dock-badge-desktop-tests.log` |
| Both formatting checks / diff whitespace | Passed |
| Bare dev build | Passed in **3m00s**, `tmp/dock-badge-dev-build.log` |
| Native adapter check | Actual AppKit labels; ten writes, unchanged input zero writes; `tmp/dock-badge-native-check.log` |
| Production hook check | Activity6 excluding900 reactions → clear → Later4 → hidden Later clear → Later4 → account-retirement clear; `tmp/dock-badge-projection-check.log` |

Native badge runs used disconnected fixtures, no Slack transport or preference
writes. They establish actual AppKit label state, **not rendered Dock pixels,
live event agreement, Windows behavior, or complete Slack parity**. The build
reported the existing future-incompatibility notice for `block v0.1.6`.

## Activity cursor reconciliation: implemented Windows checkpoint

At the original Mac handoff this task had no edits. The Windows continuation now
implements the following recorded source contract and passes pure regressions
plus normal/narrow native fixtures. It does **not** establish live cross-client
cursor delivery. The original defect list and ownership plan below are retained
as context for reviewing the fix, not as unimplemented work.

Original source-backed defects (now addressed):

1. `src/desktop/realtime.rs` channel/thread marked events update conversation or
   thread cursors without reconciling matching Activity rows/category counts.
2. `crates/super-platinum-core/src/activity_feed.rs` currently has uncursored
   `mark_channel_read_categories(channel)`. It can clear entries newer than the
   acknowledged cursor and excludes only `item.is_thread()`.
3. A threaded `at_user`/keyword item with `message.thread_ts` is not necessarily
   a thread-kind item, so opening its channel can incorrectly clear it.
4. `crates/super-platinum-core/src/thread_reads.rs` updates the Threads surface,
   not all matching Activity state. Reading a thread in Slack/native can leave
   its Activity row unread until another refresh.

Captured source contract from `/DLK` (S5145254), `MdxY` E/w (S5086450 onward),
and `YfTA` (S4902019):

- Channel cursor: same channel, **no resolved thread timestamp**, mapped message
  timestamp at or before the acknowledged `lastReadCursor`.
- Thread cursor: same channel and exact root, mapped message timestamp at or
  before that cursor.
- Ordinary message mapping uses message `ts`/`thread_ts`; `thread_v2` uses
  **`min_unread_ts || latest_ts`**, not its root timestamp.
- Decrement once per previously raw-`is_unread` root item/category; clamp at zero.
  Effective unread bundle counts and raw category-count eligibility differ.
- Preserve the current direct-item-read rule: `QDAL.m` changes the row, not the
  canonical Inbox category map. Do not indiscriminately decrement on all reads.

Ownership used for the implementation:

- Core worker: pure bounded channel/thread reducers, timestamp mapping, category
  deltas, and `ActivitySync` overlays so an older in-flight page cannot resurrect
  read state. Own `activity_feed.rs` and model/helpers/tests as needed.
- Desktop worker: `realtime/activity_counts/reads.rs`, its tests, native ACK in
  `bootstrap/history.rs`, thread read paths, and realtime marked events. Pass the
  **captured acknowledged cursor**, never a newer current latest timestamp.
  Preserve account/team/socket generation and per-surface read ownership.
- Source/review worker: verify raw root/bundle and timestamp semantics against
  the captured modules before approving implementation.
- Root: integration, locked suites, native normal/narrow fixture/check extension
  (`src/desktop/activity_feed/check.rs`, `scripts/check-activity-native.py`),
  screenshots and private-only live checks if useful/authorized.

Required regressions: newly arriving rows beyond the cursor; threaded mentions
during channel reads; exact thread roots; duplicate/out-of-order marks; wrong
workspace/account/socket generation; marks while Activity pagination is in
flight; raw-false bundles; category decrement once rather than by message units.

## Other concrete findings from the final read-only audit

**Lists continuation:** 50 visible rows is local paging over all fetched entries,
not silent server truncation. Server continuation really is absent: one
`lists.records.list` request, retained cursor, explicit incomplete banner;
Refresh repeats page one. However, captured stock callers also fetch once without
cursor/limit and no available live evidence proves a cursor-bearing List. Do not
invent a continuation parameter/contract just to hide the warning. First obtain
a real read-only large-List response and verified next-page request. Missing
**Duplicate item** was the immediately source-backed next feature at the handoff:
stock uses `duplicatedRecordId` plus adjacent position in the existing creation
endpoint. It is now implemented and Windows fixture-verified. The exact original
B/S/V bundles were recovered from public Slack asset URLs; the dated source
section in `slack-lists-protocol.md` records hashes, offsets and support boundaries.

**Ordinary message `multi_users_select`:** implemented and Windows fixture-verified
after the handoff; it was inert and could disable sibling actions at the Mac pause.
Captured B module `hGZA`, offset13286237, uses
`savedUsers || initialUsers || []`, includeSelf/allowEmptyQuery, staged selection,
maxSelectedItems, Cancel restore, and submit (13288513) with
type/actionId/blockId/selectedUsers plus initialUsers when supplied. Reuse the
single-user directory and staged multi-static UI rather than inventing behavior.
Cover required/max/owner/stale/429/full-sibling state and narrow interaction.

**Ordinary message overflow:** implemented and Windows fixture-verified after the
handoff; it was an inert More affordance at the Mac pause. Captured B
`UVRG`13292917 / action13293902 sends `selectedOption:{text,value}`; option URL
handling follows confirmation separately. Add typed bounded options, exact
payload/confirmation/URL behavior and no-replay tests. Both app-control features
now have those tests and normal/narrow native prepared-request checks. Owned
private live app receipt/response proof remains open.

Other broad open areas include richer canvas authoring/comments, List view
writes/field types/actions, custom channel tabs, workflow discovery/execution,
live app receipts, controlled huddle media, Windows-specific OS integration,
accessibility/IME and full visual/performance sweeps. Do not redefine success as
only the features already implemented.

## Lessons that should prevent regressions

- Realtime messages are **partials**: merge them. A `message_replied` frame can
  omit reactions; assignment erases pills. Update channel and every thread copy.
- A surface owns its destination. Global opens go to Home; DMs/Activity keep
  their own selected conversation and empty state. Reuse the shared message row.
- Async work needs identity/generation guards after awaits. An uncertain write
  remains owned after its view closes; verification failure is not permission
  to replay it. Preserve drafts and newer edits while recovering.
- Slack API fields are not always canonical client fields. Trace the mapper
  before copying selectors. IM/MPIM membership/cross-team normalization changed
  our conclusions about DM eligibility. Prefer source-backed exact contracts.
- Canonical counts, loaded feed rows, rendered windows and UI category counts
  are different data. Do not use whichever total is easiest to access.
- Treat HTTP absence, explicit zero/empty, partial patches and malformed input
  distinctly. Guard snapshots against events and local writes during the fetch.
- Slack text is typed `RichNode` content, never raw HTML injection. Split embedded
  rich-text newlines properly; blocks win over fallback text when they render.
- Broken-image icons are bugs. Use ready gates/fallbacks or native placeholders;
  URLs must include `uri_at(media_epoch)`. Source registration can happen during
  render, so the bounded sweep is necessary. Retry transient failures, abandon
  permanent ones, and keep third-party image fetches free of Slack cookies.
- Persistent media slots are stable identities, not rotating URLs. Provisional
  old bytes can paint while updated bytes load. Webhook identity needs extra care.
- Respect offline holds, Retry-After, stale-event guards and warm caches. Use
  `ShellState::report_failure`, not many raw error toasts containing signed URLs.
- Dioxus eval lifetime matters. A JavaScript function finishing can retire the
  eval owner; keep receivers alive until the expected response. Verify native
  interactions instead of relying exclusively on pure tests or guessed APIs.
- Do not add network/disk work to render paths, full-map scans on media/typing
  ticks, or always-on observers for occasional work. Measure both the component
  cost and the actual user-visible path, labeling one-run evidence honestly.
- macOS App Nap/background suspension explains some observation delays; do not
  declare a process dead because a single agent observation timed out. Poll the
  same live process/handle before restarting. This is not automatically a Windows
  explanation.

## Reference map and local evidence

| Resource | Purpose |
| --- | --- |
| `AGENTS.md` | Architecture, safety, Slack/media/realtime invariants, verification rules |
| `docs/parity-worklog.md` | Broad acceptance matrix plus chronological implementation/validation receipts |
| `docs/design-system.md` | Geist-based visual contract |
| `docs/slack-lists-protocol.md` | Exact List cell/action payloads, ordering and mutation/recovery findings |
| `docs/slack-canvas-edit-protocol.md` | Collaboration session/Automerge/provenance/save/restore findings |
| `docs/channel-notifications-protocol.md` | Channel preference contracts |
| `docs/notification-schedule-protocol.md` | Recurring schedule behavior |
| `docs/global-notifications-protocol.md` | Global defaults/keywords |
| `docs/notification-presentation-protocol.md` | Sounds, attention, Activity/DM/Dock counts and source offsets |
| `docs/runtime-responsiveness.md` | Startup/idle/App Nap/edit/capture investigations and limits |

Source-offset shorthand used in research:

- **B**: `tmp/sb/js/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`
- **S**: `tmp/sb/js/gantry-v2-shared.full-capture.0b81595876785d7a.min.js`
- **V**: `tmp/sb/js/gantry-v2-vendors.522a064003a98b05.min.js`
- **M/P**: installed Mac Slack4.52.178 `app.asar` main/preload bundles. That
  installation path does not exist on Windows; offsets are version-specific.

Do not run a normal line-oriented search that prints an entire minified bundle.
Use bounded substring extraction/module offsets. Do not dump raw JSON captures.
`scripts/capture-slack-cdp.mjs` and Playwright/CDP were used for the real Slack
reference. Tool availability/login/debug endpoints must be re-established on
Windows; never navigate a real Slack renderer to a non-`/client` URL casually.

Mac-only process state at handoff (historical, not a Windows endpoint):

- Bare live dev PID **98097**, socket **`/tmp/sp-dev-live.sock`** responded to ping.
  The user was in an Activity thread; it was not restarted or navigated.
- `target/debug/super-platinum` on disk contains the newest capture/Dock work;
  the already-running process is an older mapped binary. Do not infer deployed
  runtime version from the executable's current path alone.
- Latest read-only PNG: `tmp/agent-ui/parity-next-live.png`, inspected. It contains
  real conversation content and is private evidence, not a fixture to commit.
- No Cargo/rustc or feature implementation agents remained active at handoff.
- Storage was approximately11GiB free after removing only superseded incremental
  cache directories. Do not resume a broad disk cleanup; Windows needs a fresh
  build, not the accumulated Mac target trees.

## Dirty tracked paths at handoff

These are in addition to the untracked source trees listed above. Preserve them
as a coherent working-tree transfer; some include the prior capture slice.

```text
Cargo.toml
crates/super-platinum-core/src/cache.rs
crates/super-platinum-core/src/later.rs
crates/super-platinum-core/src/later/tests.rs
crates/super-platinum-core/src/palette.rs
crates/super-platinum-core/src/slack/events.rs
crates/super-platinum-core/src/slack/realtime.rs
crates/super-platinum-core/src/state/activity_counts/policy.rs
crates/super-platinum-core/src/state/mod.rs
crates/super-platinum-core/src/state/notifications.rs
crates/super-platinum-core/src/state/sidebar.rs
crates/super-platinum-core/src/state/tests/users.rs
crates/super-platinum-core/src/state/tests/workspace.rs
docs/notification-presentation-protocol.md
docs/parity-worklog.md
docs/runtime-responsiveness.md
src/desktop/agent.rs
src/desktop/agent/io.rs
src/desktop/bootstrap/session.rs
src/desktop/channel_discovery.rs
src/desktop/global_notifications/lifecycle.rs
src/desktop/later.rs
src/desktop/main.rs
src/desktop/notification.rs
src/desktop/presentation_notifications/lifecycle.rs
src/desktop/realtime.rs
src/desktop/realtime/activity_counts.rs
src/desktop/realtime/activity_counts/rules.rs
src/desktop/realtime/activity_counts/tests.rs
src/desktop/state/boot.rs
src/desktop/state/mod.rs
src/desktop/state/shell.rs
```

There is no requirement to recreate the old agent names or memory directory.
Use the repository and this handoff as the durable context. Resume with fresh
evidence, update the worklog, and keep the full goal active until its actual
requirements are implemented and verified.
