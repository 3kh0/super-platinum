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

For fast full rebuilds:

```powershell
# Windows: also parallelizes native TLS compilation with Ninja when available.
.\scripts\rebuild.ps1
```

The Windows helper finds Ninja on PATH or in Visual Studio, uses its Multi-Config
generator to preserve MSVC library paths, defaults to 12 jobs,
and restores its temporary environment changes afterward. Use `-Jobs 8` to
reduce concurrency or `-Features 'original-download-native-fixture,protected-native-fixture'`
for those offline fixture features. It preserves explicit CMake generator settings
and falls back to the default generator if Ninja is unavailable. Its default
Ninja build goes under `target/rebuild-ninja-multi/dev-rebuild`; keep that cache for
subsequent builds. `-TargetDir <path>` overrides the build directory.

On other platforms, or to use the default native build system directly:

```sh
cargo rebuild
```

This alias runs `cargo build --locked --profile dev-rebuild --timings`. The
rebuild profile disables Rust optimization, debug-info generation, and incremental-cache writes,
and uses 256 codegen units for parallel compilation. Assertions and overflow
checks remain enabled. Its executable is under `target/dev-rebuild`. Cargo still
reuses unchanged artifacts; the command does not delete build caches.

The repository defaults to 12 build jobs (override with `--jobs N` or
`CARGO_BUILD_JOBS`). Windows x64 MSVC builds use Rust's bundled `rust-lld` linker.
They reserve a 16 MiB main-thread stack (with the default initial commitment),
which prevents unoptimized desktop startup from overflowing the 1 MiB default.
Visual Studio Build Tools and the Windows SDK are still required for native
dependencies. To opt back into Microsoft's linker, pass
`--config 'target.x86_64-pc-windows-msvc.linker="link.exe"'` to Cargo.

For repeated edits and line-number backtraces:

```sh
cargo build --locked --profile dev-fast --timings
cargo run --locked --profile dev-fast
```

`dev-fast` skips application and dependency optimization, retains application
line numbers for backtraces, and omits dependency debug symbols. Debug assertions,
overflow checks, and incremental compilation remain enabled. It trades runtime
speed and variable-level debugging for shorter builds; use the ordinary `dev`
profile for performance investigations or full debugger support. Its artifacts
live separately under `target/dev-fast`, so keep that directory between builds.
The 12 jobs are logical workers, not a guarantee of 12 busy physical cores.

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
