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

### Message rendering notes (for UI work)

- Message bodies are typed DOM nodes built by `src/desktop/message_vm.rs` and
  rendered in `src/desktop/view.rs`; never inject raw Slack HTML.
- Slack often packs multi-paragraph posts as **one** `rich_text_section` with embedded `\n` in text leaves. Block rendering **must** split those into separate lines (`split_segments_on_newlines` in `blocks.rs`).
- Standard emoji resolve through `state::emoji_glyph`; custom workspace emoji
  use opaque native media IDs and inline image nodes.
- Do not reintroduce “one big line with `\n` inside a wrapping row of text chips” — that produces floating mid-line words (the old `#ship` Hack Piano bug).

## Working Style

- Start from the concrete file, error, route, or behavior the user named.
- Read the existing code before proposing architecture.
- Keep edits scoped and behavior-preserving unless the user asked for a redesign.
- Report exactly which checks passed and which were not run.
- If a task is routed through a plan or handoff file, update that file as part of the work and keep its next steps testable.
