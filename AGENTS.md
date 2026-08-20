# AGENTS.md

Guidance for agents working in this repository.

## Project Shape

Super Platinum is a Rust desktop Slack client built with Dioxus Desktop.

Important boundaries:

- `crates/super-platinum-core/` owns renderer-neutral domain code: cache, config/session,
  Slack API/realtime/models, workspace state helpers, palette ranking, commands,
  reducers, supervisors, and the stable agent protocol.
- `src/desktop/main.rs` is the Dioxus application entry point.
- `src/desktop/state/` owns the serial shell state and mutation boundary
  (projections, selection, timeline window, composer, fixtures).
- `src/desktop/bootstrap/`, `messaging.rs`, and `realtime.rs` own native async
  Slack work (session, history, discovery, persistence).
- `src/desktop/view/`, `overlays.rs`, and `src/desktop/styles/` own the typed
  DOM UI and CSS cascade modules.
- `src/desktop/media.rs` owns the opaque `super-platinum-media://` protocol.
- `src/desktop/agent.rs` is the optional live control plane (`SUPER_PLATINUM_AGENT=1`).
- `src/desktop/auth.rs` owns the Slack sign-in WebView flow (tao/wry).

Keep changes inside the smallest boundary that matches the task. Domain behavior
belongs in `super-platinum-core`; DOM focus, selection, scrolling, capture, and other
renderer-owned behavior belongs in `src/desktop/`. Keep implementation modules
focused: aim for 300–700 lines and split before 1,000 lines when cohesive.

## Dioxus Documentation Rule

Do not guess Dioxus APIs from memory. This project uses an exact pinned git
revision rather than a crates.io range.

For application setup, components, signals, hooks, document evaluation, desktop
configuration, custom protocols, or runtime behavior, check the Dioxus 0.7 docs
first:

<https://dioxuslabs.com/learn/0.7/>

When documentation disagrees with the pinned revision in `Cargo.toml`, the
repository and compiler win. Prefer small compile-backed changes.

## Development Commands

Use locked Cargo commands by default:

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
```

For most Rust changes, run `cargo fmt --check` and `cargo test --locked` before
calling the work done. Also run the independently locked core suite when domain
behavior changes. Use `cargo check --locked` for faster iteration.

If a build fails with stale dependency artifacts under `target/debug/deps`, a clean rebuild has fixed that class of local issue before:

```sh
cargo clean
cargo build --locked
```

Do not treat local environment noise, such as shell startup warnings, as the root cause of Rust or app failures without evidence.

## Persistence And Secrets

Be careful around `crates/super-platinum-core/src/config/`.

- The app stores Slack session secrets through the configured secret backend.
- `STORAGE_QUALIFIER` and `KEYRING_SERVICE` still say `snack` after the Super
  Platinum rebrand. That is deliberate: renaming either one orphans the existing
  config directory, warm cache, and Keychain item, signing every user out. Change
  them only together with a migration.
- Tests should not touch the real macOS Keychain or platform keyring.
- Keep test-only secret isolation behind `cfg(test)`.
- When changing session format, preserve migration behavior and add round-trip tests for both current and legacy shapes.

The app should not introduce repeated keychain prompts on boot or during tests.

## Performance Expectations

This is intended to feel fast in dev and release builds.

- Do not add synchronous disk or network work to Dioxus render/event paths.
- Prefer async tasks or background work for cache writes and Slack calls.
- Keep rendered message lists bounded or lazily computed where possible.
- Be careful with supervisors and periodic ticks; avoid always-on work unless needed.
- Preserve `[profile.dev]` settings unless there is a measured reason to change
  them.

## UI Expectations

Super Platinum should feel like a focused desktop Slack client, not a marketing page.

- Keep the UI quiet, dense, and readable.
- Use the existing CSS custom properties and appearance helpers.
- Prefer existing Dioxus components over one-off presentation logic.
- Keep controls stable in size; avoid layout shifts on hover, loading, or text changes.
- Do not add decorative chrome that competes with channels, messages, threads, and search.

## Slack Behavior

Slack-facing behavior needs defensive handling.

- Respect rate limits and `Retry-After` behavior.
- Preserve realtime generation guards and stale-event protection.
- Keep warm-boot/cache paths working when network calls fail.
- Do not assume all Slack messages are plain text; Block Kit, files, reactions, threads, edits, deletes, and notifications already exist in the product surface.

## Testing Guidance

Add focused tests when changing:

- session/config persistence,
- cache serialization or warm boot behavior,
- Slack API pagination/rate-limit handling,
- realtime event handling,
- message/thread/reaction/file/search state transitions,
- UI logic that can be tested through pure helpers.

Prefer small regression tests that encode the bug or behavior contract. Avoid large fixture churn unless the task specifically requires it.

## Agent UI Verification

Agents should **not** wait on a human to `cargo run`, click around, and paste screenshots for ordinary UI work. Use offline fixtures and/or the live control plane below, then **read the PNGs yourself** (image-read tool) before claiming layout is correct.

| Mode | When | Entry point |
| --- | --- | --- |
| Offline fixtures | Chrome, layout, message rendering, modals — no real Slack data needed | `scripts/agent-ui-check.sh` |
| Live control plane | Real channels/messages, palette ranking, search, warm cache, realtime | `SUPER_PLATINUM_AGENT=1` + `scripts/agentctl.sh` |
| Real Slack reference | What Slack itself renders for a shape (HTML, computed styles, payloads) | `scripts/capture-slack-cdp.mjs` |

Still run `cargo fmt --check` and `cargo test --locked` (or a focused subset) for logic. Captures are not a substitute for unit tests.

### Offline fixture captures (no Slack session)

```sh
scripts/agent-ui-check.sh
```

What it does:

- Builds and launches the real Dioxus Desktop binary once per offline fixture.
- Drives the unchanged agent protocol and captures the native WebView window.
- Includes multi-paragraph rich text and custom emoji fixtures that reproduce
  the `#ship` “Hack Piano” layout class of bugs.
- `media-loading-state` captures the cold-boot state — every image registered,
  no bytes landed — so a regression back to broken-image icons is visible.
- Holds the display awake for the run (macOS) and retries each capture: a
  sleeping display has no window surface, and `screencapture -l` fails outright
  with “could not create image from window”, losing the whole sweep.
- Writes PNGs under `tmp/agent-ui/` (override with `SUPER_PLATINUM_UI_CAPTURE_DIR`).
- Fixture state and rendering live under `src/desktop/`.

After the script finishes, **read the PNGs** and verify layout, copy, and chrome.

Rules:

- Fixtures only — do not put tokens or real session secrets in tests.
- Do not claim visual verification without running this harness (or having live screenshots you inspected).
- When you add a new screen, modal, or message-layout path, add a named fixture
  to `ShellState::fixture_core` and `scripts/agent-ui-check.sh`.

### Live control plane (real session + drive the UI)

For features that need real data (quick switcher ranking, search hits, warm cache, live message layout), run Super Platinum with the agent socket and drive it via `scripts/agentctl.sh`.

Implementation: `src/desktop/agent.rs` (Unix socket or Windows loopback TCP
NDJSON into the serial dispatcher). Wired only when `SUPER_PLATINUM_AGENT` is set.

#### Boot

```sh
# Prefer a built binary once code is compiled (faster restarts).
cargo build --locked

# Clear a stale socket if a previous agent run died hard.
rm -f "${TMPDIR:-/tmp}/super-platinum-agent.sock" "${TMPDIR:-/tmp}/super-platinum-agent.sock.path"

# Uses the normal Super Platinum session / Keychain (macOS).
SUPER_PLATINUM_AGENT=1 ./target/debug/super-platinum
# equivalent: SUPER_PLATINUM_AGENT=1 cargo run --locked
```

Socket path: `SUPER_PLATINUM_AGENT_SOCK`, else `$TMPDIR/super-platinum-agent.sock` (also written to `$TMPDIR/super-platinum-agent.sock.path` for discovery). If `agentctl` gets `Connection refused`, remove the stale sock and restart with `SUPER_PLATINUM_AGENT=1`.

#### Drive the UI

```sh
scripts/agentctl.sh ping
scripts/agentctl.sh wait signed_in=true
scripts/agentctl.sh state                    # JSON snapshot
scripts/agentctl.sh open-palette
scripts/agentctl.sh set-query ship
scripts/agentctl.sh wait 'entries>=1'
scripts/agentctl.sh submit
# Channel switch is async — poll until active channel matches.
scripts/agentctl.sh wait channel=ship
scripts/agentctl.sh screenshot tmp/agent-ui/live-ship.png
```

Useful commands (full list: `scripts/agentctl.sh help` or `agentctl help`):

| Command | Notes |
| --- | --- |
| `state` | Screen, active channel, palette entries, recent messages, search, toasts |
| `open-palette` / `set-query` / `move` / `submit` | Quick switcher; prefer this over `select-channel` when name resolution is ambiguous |
| `select-channel <id\|name>` | Direct open; fails if the name is not found in the loaded workspace map |
| `search` / `clear-search` | Message search overlay |
| `screenshot [path]` | Live window PNG for multimodal inspection |
| `wait <predicate>` | Poll `state` until match (`channel=…`, `signed_in=true`, `entries>=N`, …) |
| `allow-destructive` / `send` | Composer send; blocked unless destructive mode is enabled |

#### Live workflow tips

- After `submit` / channel open, **wait or poll `state`** — `active_channel` can lag the submit response by a frame or network history load.
- Prefer `state` for structural checks; use `screenshot` when layout/typography matters, then **read the PNG**.
- Recent messages in `state` are text snippets only; full Block Kit layout needs a screenshot or an offline fixture built from known blocks.
- Do not assume the viewport shows a particular historical message — the live list is scrolled to recent. For a fixed layout repro, use `multi_paragraph_emoji_app` offline rather than scrolling the live client.
- Destructive actions (`send`) require `SUPER_PLATINUM_AGENT_ALLOW_DESTRUCTIVE=1` or `scripts/agentctl.sh allow-destructive true`. Never enable that casually.
- Live mode uses the real Slack session. Never print tokens, cookies, or secrets.
- Prefer offline `agent-ui-check.sh` when live data is not needed.

### Real Slack as the reference implementation

When the question is "what does Slack actually render / send for this shape",
drive the real desktop app over CDP rather than guessing:

```sh
osascript -e 'quit app "Slack"'
open -a Slack --args --remote-debugging-port=9222
node scripts/capture-slack-cdp.mjs --list
node scripts/capture-slack-cdp.mjs --eval '(() => document.querySelector("[data-qa=message_attachment]").outerHTML)()'
node scripts/capture-slack-cdp.mjs --screenshot tmp/slack-reference.png
node scripts/capture-slack-cdp.mjs --filter conversations.history --reload   # network capture, Ctrl+C to stop
```

- `--eval` runs in the `app.slack.com/client` renderer; wrap multi-statement
  expressions in an IIFE and return a value, or the result reads back `undefined`.
- Do not navigate the renderer to a non-`/client` URL; it lands on `app://error/`
  and has to be steered back.
- Network captures go to `captures/` (gitignored — they contain cookies, tokens,
  and real messages). Never commit or paste their contents.
- Getting the raw JSON for a channel is often faster through a **temporary**
  env-gated probe against the persisted session (`config::load_session()` +
  `Transport` + `api::conversations_history`, print `transport.execute` output)
  than through the UI. Delete the probe when done.

### Message rendering notes (for UI work)

Message bodies are typed `RichNode` trees; never inject raw Slack HTML. The
pipeline is:

| Module | Owns |
| --- | --- |
| `src/desktop/blocks.rs` | Block Kit blocks/elements → `RichNode` (`rich_text*`, `section`, `context`, `header`, `divider`, `actions`, `image`, `button`) |
| `src/desktop/blocks/text.rs` | Slack's text layer: `mrkdwn` spans, `<…>` entities, `:emoji:`, mention chips |
| `src/desktop/unfurl.rs` | `message.attachments` → `AttachmentVm` (message unfurls, link unfurls, app unfurls, legacy bot attachments) |
| `src/desktop/message_vm.rs` | Assembles `MessageVm`: body, reactions, attachments, reply bar, timeline annotation |
| `src/desktop/view/rich.rs` | Renders all of the above; `src/desktop/styles/blocks.css` owns the CSS |

- Slack often packs multi-paragraph posts as **one** `rich_text_section` with embedded `\n` in text leaves. Block rendering **must** split those into separate lines (`split_section_on_newlines` in `blocks.rs`).
- Do not reintroduce “one big line with `\n` inside a wrapping row of text chips” — that produces floating mid-line words (the old `#ship` Hack Piano bug).
- Standard emoji resolve through `state::emoji_glyph`; custom workspace emoji
  become `RichNode::EmojiImage` / `ReactionVm::media` — opaque native media IDs
  sized to the text, never attachment-sized images.
- A Block Kit body wins over `message.text`; the text field is only a fallback
  when the blocks render nothing (otherwise bots double-print, and alt text like
  `user pfp` leaks into the body).
- Bot avatars follow Slack: per-message `icons` first, then the posting user's
  workspace profile image, and only then `bot_profile.icons`.
- Slack's own footer strings for shared-message unfurls are useless
  (`"Thread in Slack Conversation"`); synthesize them from `is_msg_unfurl` /
  `is_reply_unfurl` plus `channel_id` and `ts`, like the real client does.
- Attachment `ts` is a string for unfurls and a bare epoch **number** for legacy
  bot attachments. Model shapes must stay permissive — a strict field there broke
  warm boot from the on-disk cache.

### Media loading rules

`src/desktop/media.rs` registers sources during projection and fetches them
afterwards, so anything painted before the bytes land holds a failed request.

**A broken-image icon is a bug.** Two mechanisms keep it off screen, and new
image sites must use one of them:

- Anywhere with a real fallback (initials, an event glyph), gate the `img` on
  `MediaRegistry::is_ready`. See `view/mod.rs`, `chrome.rs`, `secondary.rs`.
- Everything else falls through to the protocol, which answers a pending asset
  with a placeholder (`200`, `no-store`) rather than `404` — a skeleton box, or a
  transparent gap for emoji, which sit inline in a sentence.

- Render media with `MediaAssetId::uri_at(media_epoch)`, never bare `uri()`. An
  element `key` is **not** a diffed attribute; `src` is, so only a changing URL
  makes the WebView retry. `FromStr` ignores the `?v=` stamp.
- `runtime::ticks` owns `media_epoch`: it bumps once per tick when
  `take_dirty()` reports bytes, so a slow host cannot hold back avatars that
  already arrived. Do not bump it from a load path.
- The same tick sweeps `has_pending()` every second. Sources are also registered
  during *render* (hover cards, activity rows), which no Slack call follows, so
  without the sweep those images would never load.
- Media fetches must stay time-bounded (`MEDIA_FETCH_TIMEOUT` in
  `slack/transport.rs`). Loads are serialized behind one lock, so a single
  hanging third-party host otherwise wedges every later refresh.
- A fetch failure is either permanent (4xx: the URL is gone, abandon it) or
  transient (timeout, 5xx, 429: retry on a doubling delay). With a sweep running
  every second, retrying a dead URL forever is a steady stream of doomed
  requests; giving up on a timeout loses the image for the whole session.
- `register_image` decides the session-cookie question centrally: Slack hosts get
  the cookie, Block Kit and unfurl images (arbitrary third-party hosts) must not.

#### The persistent picture cache

`MediaStore` (`core/src/media/store.rs`) keeps avatars, emoji, and icon-sized
images on disk so a relaunch paints faces without touching the network.

- Entries are keyed by a **slot** — a stable identity (`avatar/U123`,
  `emoji/party`), not the URL. Slack rotates the avatar URL whenever somebody
  changes their picture, so URL keying misses exactly where a user would notice.
- A slot hit at a *different* URL is inserted as **provisional**: it paints
  immediately (the person's previous picture) and stays pending so the new bytes
  replace it. An exact URL hit skips the network entirely.
- Register through `register_avatar(identity, url)` / `register_emoji(name, url)`
  / `register_icon(kind, url)`. Plain `register_image` is for one-off images
  (unfurl previews, file thumbnails) that must not fill the cache.
- Slots must be stable *and* unambiguous. Webhook and bot icon keys from
  `state::message_avatar` deliberately embed the URL: one webhook posts as many
  different people, and an identity-keyed slot there would flash the wrong face.
- Entry writes are capped (`MAX_ENTRY_BYTES`) and the directory is pruned to
  `MAX_ENTRIES` by mtime. `slot_hash` is FNV-1a, pinned by a test: a
  `DefaultHasher` is not stable across Rust releases and would orphan the cache.

## Working Style

- Start from the concrete file, error, route, or behavior the user named.
- Read the existing code before proposing architecture.
- Keep edits scoped and behavior-preserving unless the user asked for a redesign.
- Report exactly which checks passed and which were not run.
- If a task is routed through a plan or handoff file, update that file as part of the work and keep its next steps testable.
