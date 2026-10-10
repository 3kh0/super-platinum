# Super Platinum

A fast, focused Slack client built in Rust with [Dioxus Desktop](https://dioxuslabs.com/learn/0.7/guides/platforms/desktop/).

Super Platinum uses the operating system WebView for presentation while Slack networking, realtime delivery, caching, persistence, media authorization, and reducers remain native Rust.

## Layout

```text
crates/super-platinum-core/   Renderer-neutral domain: Slack, cache, config, state, agent protocol
src/desktop/         Dioxus Desktop shell: view, state, bootstrap, media, agent control plane
```

Import domain behavior through `super_platinum_core` only. The desktop package should not reach into core sources by path.

## Run

Install the system WebView development package on Linux (`libwebkit2gtk-4.1-dev` and `libgtk-3-dev` on Ubuntu/Debian). macOS and Windows use their built-in WKWebView and WebView2 runtimes.

```sh
cargo run --locked
```

## Develop

```sh
cargo fmt --check
cargo check --locked
cargo test --locked --manifest-path crates/super-platinum-core/Cargo.toml
cargo test --locked
scripts/agent-ui-check.sh
```

To inspect startup latency, launch with `SUPER_PLATINUM_STARTUP_TRACE=1`.
The log reports milliseconds from entry into `main` to the first painted frame,
initial cache load, usable conversation paint, fresh history paint, and realtime
connection. Paint markers wait for two WebView animation frames. These timings
also appear under `performance.startup` in `scripts/agentctl.sh state` when
`SUPER_PLATINUM_AGENT=1` is enabled. Timing logs contain no session credentials
or message content.
