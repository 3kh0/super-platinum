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

On macOS, use `scripts/macos-app.sh --run` to build and launch the signed app
bundle. Native notifications require this bundle so macOS shows Super Platinum's
name and icon. Allow Super Platinum when macOS requests notification permission;
a bare `cargo run` still runs the client but does not deliver macOS notifications.

## Develop

On Windows, use the single [storage-conscious build workflow](docs/build-workflow.md).
The launcher uses a process-only execution-policy override because this laptop blocks
bare PowerShell scripts; it makes no persistent policy change. The helper preserves
the locked dependencies, Ninja Multi-Config, bundled LLD and Windows
stack settings, and uses stable desktop/core target directories. Choose the
smallest relevant command for an edit; these are alternatives, not a queue:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\rebuild.ps1 -Task Check
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\rebuild.ps1 -Task CheckTests
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\rebuild.ps1 -Task CoreTest -Filter 'canvas::'
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\rebuild.ps1 -Task DesktopTest -Filter 'mounted_vectors_share_dependency'
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\rebuild.ps1 -Task Build
```

The helper defaults to eight Cargo jobs and the measured `dev-fast` desktop
iteration profile. `-DryRun` prints a command without compiling. It refuses a second live
Cargo/rustc pipeline and checks the documented disk limits before starting.
See the workflow for continuation/warm-edit measurements, resource limits and
the final stabilization gate. Do not use the historical temporary build wrappers
or create another target tree.

Other platforms can still use locked Cargo commands (`cargo check --locked`,
`cargo test --locked`, and `cargo build --locked`). `cargo rebuild` remains the
`build --locked --profile dev-rebuild --timings` alias. The existing profiles are
unchanged: `dev-rebuild` omits incremental writes and debug info with 256 codegen
units; `dev-fast` retains incremental compilation and application line tables;
ordinary `dev` keeps its runtime optimization settings. Do not alternate profiles
for routine edits: they retain separate artifacts, even within one target tree.

Windows x64 MSVC still reserves a 16 MiB main-thread stack and uses Rust's bundled
`rust-lld`; Visual Studio Build Tools and the Windows SDK remain required.
The helper also sets `RUST_MIN_STACK=16777216` for Rust-created threads and restores
its process environment afterward. The repository-wide 12-job setting remains
available to direct Cargo calls; the selected laptop workflow uses eight jobs.
Test filters choose executed tests; they still compile/link the whole selected
test binary. Batch coherent edits and use the linked build only when needed.

To inspect startup latency, launch with `SUPER_PLATINUM_STARTUP_TRACE=1`.
The log reports milliseconds from entry into `main` to the first painted frame,
initial cache load, usable conversation paint, fresh history paint, and realtime
connection. Paint markers wait for two WebView animation frames. These timings
also appear under `performance.startup` in `scripts/agentctl.sh state` when
`SUPER_PLATINUM_AGENT=1` is enabled. Timing logs contain no session credentials
or message content.
