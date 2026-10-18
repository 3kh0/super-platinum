# Current state and handoff

Checkpoint: 2026-10-18T03:14:49.952331+00:00. **The single V9 integration milestone is complete: all 13 gates passed, exact V9 source is installed, and its matching Windows dev executable is verified.** Full Slack parity and native acceptance remain separate backlog; later feature drafts remain paused.

## Verified installation and executable

Workspace: `C:\Users\mortar\Documents\GitHub\super-platinum`, branch `main`, HEAD `4d19f3f08ba2c00321075e48b0b8d3da3db3a272`. Installation is uncommitted. Preserve all installed changes, documents, ignored drafts, caches and failed receipts.

- **Installed V9: 3,913 files, 900 installation targets.** [Actual receipt](../tmp/combined-v9-main-install1/result.json), SHA256 `4f47727d8ce6ae87d7e17383bd899b9b93fb364a12bbcc93b8fcba09efe613c3`; [installed inventory](../tmp/combined-v9-main-install1/after.json). Reviewed source inventory SHA256 `d2a3e2bd1c28e91aa0fe897adb676babdafe4f2b209c78cc0ca3041d6db789b9`.
- **All 13 gates verified:** [validation closure](../tmp/combined-v9-runtime-sequence2/result.json), SHA256 `75b3cc44070448e6089ee9eaf4b6af47d63763655775e725fca90e9635b9ff26`.
- **Matching executable:** [native.exe](../tmp/combined-v9-main-build-parallel4/native.exe), SHA256 `ebffb73bd0bccb431cb3870d29407889fdcf89ec6b5a36c2e81b09702719ac21`. [Build seal](../tmp/combined-v9-main-build-parallel4/build.json), SHA256 `ded6e7bc693c6b2d74dfe300426094a78a3006a3351c71b5aff21a3cd63a2cda`.
- **Final verification:** [milestone closure](../tmp/combined-v9-milestone-parallel-closure1/result.json) rechecked source inventory, installation journal/backups, all gate admission, Cargo artifact identity, raw log, executable hashes and documents preserved through the build. This handoff and history are intentionally updated after that verification.
- Build owner session 17003 is closed. No compiler or native owner remains from this milestone. The NPC18 binary and cancelled V9 build remain historical evidence, not the current executable.

## Build concurrency improvement

**Full-rebuild follow-up, 2026-10-18 UTC:** `Cargo.toml` now adds the optional `dev-fast` profile. Use `cargo build --locked --profile dev-fast --jobs 12 --timings` for build-speed-focused iteration. The original `dev` and release profile semantics are unchanged. `dev-fast` uses optimization level 0 for the app and dependencies, application line-table debug information, and no dependency debug symbols; assertions, overflow checks and incremental compilation remain enabled. Runtime speed and variable-level debugging are the tradeoffs.

The [actual cold build](../tmp/dev-fast-rebuild1/result.json) passed in **358.09 seconds (5 minutes 58 seconds)** with **484 Cargo artifacts, zero fresh/cache-hit artifacts**, and an initially absent target directory. This includes native dependencies, core and desktop compilation. The [timing report](../tmp/dev-fast-rebuild1/cargo-timing.html) and raw log are retained. Binary: `tmp/dev-fast-rebuild-target1/dev-fast/super-platinum.exe`, SHA256 `813912916cd3cd4a4acbacce34e3c46637d988347506ae8ae0284c69ef3a354e`. Existing caches and sealed V9 executables were preserved. The previous four-job 797-second run reused dependencies, so these observations are not a controlled speedup ratio.

Verification: locked cold build passed, `cargo fmt --check` passed, and parsed-manifest comparison confirmed the new profile is the only semantic manifest change. No Rust/domain source changed; unit tests and native UI/runtime-performance checks were not rerun for this build-only follow-up. The historical thirteen V9 gates and exact source seal above remain historical evidence: the current inventory differs from that source by `Cargo.toml` only. `README.md` documents the new workflow. Compiler session 64197 exited 0.

**Earlier concurrency-only follow-up: 12 parallel jobs, requested by the user after the four-job build.** The [twelve-job successor](../tmp/combined-v9-main-build-parallel12-prep/build.py) defaults to 12 without a command-line override. [Verification build](../tmp/combined-v9-main-build-parallel12/build.json) passed in 3.74 seconds using the existing cached executable; its binary hash is identical to the four-job build. This validates configuration and cache reuse, not a twelve-worker full-rebuild speedup. The machine has 8 physical cores and 16 logical processors: twelve jobs means up to twelve logical workers, with actual use depending on available compiler work and memory. No source, profile or test changes were made. Historical four-job evidence follows.

The user requested faster threading/multicore builds. The old wrapper forced `CARGO_BUILD_JOBS=1`, restricting both concurrent crates and LLVM workers through Cargo's jobserver. The previous reviewed [four-job successor](../tmp/combined-v9-main-build-parallel-prep/build.py) makes `--jobs` configurable with **default 4**, and enables Cargo `--timings`. Source, dev profiles, features and all installation/artifact checks stayed unchanged. This laptop has 8 physical cores, 16 logical processors and 16 GiB RAM; four jobs balances concurrency with the compiler's substantial memory use.

The successful build took **13.29 minutes** (797.22 seconds). [CPU samples](../tmp/combined-v9-main-build-parallel4/cpu-samples.ndjson) observed up to **3.10 cores** averaged over a sample, confirming actual multicore code generation. This is not an apples-to-apples speedup ratio: the one-job run was interrupted and the successor reused the dependency cache. Stable Rust 1.98.1 still has a mostly serial frontend; unsupported frontend flags and nightly/bootstrap were not used.

For subsequent ordinary dev builds on this machine:

```powershell
cargo build --locked --jobs 12 --timings
```

Cargo emits its HTML timing report under the selected target directory's `cargo-timings/`. This milestone reused `tmp/cmi1-target`; changing target directories would forfeit that cache. The sealed milestone helper requires a fresh output directory and the exact installed source; do not rerun it after editing source or overwrite its output. Ordinary Cargo builds already support multicore by default; the forced one-job wrapper was the restriction removed here.

The one-job build remains [intentionally cancelled](../tmp/combined-v9-main-build-cancelled1/intent.json): [gate](../tmp/combined-v9-main-build1/gate.json), exit 101 after 1,791.97 seconds, source/documents unchanged. The old owner session 97701 and closure1 retain their nonzero results. Do not restart or relabel them. Successor manifest SHA256 `62ed87e0cc968a789a30607f757b2424c9698b275c732386aedebc218d24d560`; helper SHA256 `7385ff3605799a0d0d366a5e1e76751ca3243c2833d75cc920d03b67bb801ca3`.

## Validation and preserved evidence

| Gate group | Verified outcome |
|---|---|
| Formatting and desktop compilation | Root/core `cargo fmt --check`; locked production, default-test and fixture-feature test checks passed. Four original V9 receipts were re-admitted against unchanged source/configuration/raw logs. |
| Desktop execution | [Focused68](../tmp/board-canvas-current30-v9-desktop-focused2/gate.json) passed; [full desktop](../tmp/board-canvas-current30-v9-desktop-tests2/gate.json): **3,145 passed, zero failed, seven ignored**. |
| Independent core compilation | [Core check](../tmp/board-canvas-current30-v9-core-check2/gate.json) and [no-default-features production](../tmp/board-canvas-current30-v9-core-production-check2/gate.json) passed. |
| Core execution | [Focused132](../tmp/board-canvas-current30-v9-focused-regressions2/gate.json), [hosted5](../tmp/board-canvas-current30-v9-hosted-retirement-module2/gate.json), and [full core](../tmp/board-canvas-current30-v9-core-tests2/gate.json) passed; full core: **3,477 passed, zero failed, six ignored**. |
| External suites and doctest | [Four external suites](../tmp/board-canvas-current30-v9-core-external2/gate.json): five tests passed. [Exact compile-fail doctest](../tmp/board-canvas-current30-v9-core-doctests2/gate.json): one passed. |

Ignored tests are explicitly marked manual benchmarks or interoperability exports; no native acceptance is inferred. All gates use locked Cargo commands where applicable. The build concurrency change does not alter source, profile, features or the test receipts.

Preserve the [V8 failed desktop gate](../tmp/board-canvas-current30-v8-desktop-tests1/gate.json) (3,144/1/7) and the [interrupted original V9 focused log](../tmp/board-canvas-current30-v9-desktop-focused1/cargo-1.log) (unknown outcome). The successful successor did not overwrite or relabel them. [Validation events](../tmp/combined-v9-runtime-sequence2/events.ndjson) retain their hashes. Installation backups and document snapshots remain under `tmp/combined-v9-main-install1/`.

Native wrappers remain held: [idle reproduction](../tmp/v9-idle-finality-root-repro1/result.json) and [scroll reproduction](../tmp/scroll-v2-result-write-root-repro1/result.json). Source clearance does not override these holds. No live Slack writes, calls, device/audio action, notifications or package registration were performed.

## Stopped owners and unintegrated changes

Checkpoint links retain exact file identities, tests and resume details. Source-reviewed leaves are not compiled or integrated unless explicitly stated.

| Owner | Preserved work and outstanding boundary |
|---|---|
| acceptance_audit | [Canvas checkpoint](../tmp/canvas-link-layout-followup/checkpoint-paused-20261017.json): 40 mutable files, 33 Rust tests unrun; hydration, mounted title/conversion and Classic behavior incomplete. |
| acceptance_review; canvas_bindings | [Channel checkpoint](../tmp/channel-conditional-native-pause-handoff/manifest.json): 46 sparse files; resync, actual major initialization and queues incomplete. [Scalar trace](../tmp/canvas-bindings-checkpoint-2026-10-17.txt): “BootTimestamp” is actually screen-reader state; self effects are role thunks. Correct before wiring. |
| list_grouping_review; desktop_runtime_fixes | [Management handoff](../tmp/app-workflow-management-product-work/HANDOFF.txt): 4,452-file mutable composition; Add/dynamic/branch surfaces incomplete. [Branch checkpoint](../tmp/desktop-runtime-branch-pause-checkpoint.json): wrapped/OAuth/function-ID reviews pending. |
| profile_recording; save_workflow_review | [Apps checkpoint](../tmp/profile-recording-pause-handoff.json): native registry/hydration unintegrated; hydration review unopened. [Missing-object drafts](../tmp/directory-app-missing-paused-checkpoint.json): six drafts, 12 Rust tests unrun. |
| activity_core | [Profile checkpoint](../tmp/profile-rich-fixture-titles-work/checkpoint.json): five mutable File/Canvas/date fixture files over reviewed metadata fixture; composition, Rust/native checks pending. |
| board_heading_review; field_delete_core/desktop | [Comments handoff](../tmp/list-record-comments-pause-checkpoint1/HANDOFF.json): 4,149-file uncompiled parent; editor modes disabled, emoji allocation-budget HOLD, unadopted leaves and missing exact ECharts asset evidence. |
| activity_desktop | [Cold notification checkpoint](../tmp/windows-notification-package-cold-work/PAUSED-CHECKPOINT.json): 11 draft files; publisher release acknowledgment unwired; nine Rust tests unrun. |
| save_dialog_review; interaction_review | [Idle correction](../tmp/idle-native-combined-v9-finality-prep/manifest.json) `bf72b40a…`: 12 pure tests pass, review pending. `tmp/channel-scroll-native-combined-v9-v3-prep/` remains mutable/unreviewed. [Review handoff](../tmp/interaction-review-pause-handoff-20261017/handoff.json) preserves holds and queued reviews. |

Other review agents (`list_bindings`, `workflow_runner`, `workflow_uia`, `slash_perf_probe`) are stopped; their reviews are referenced by these handoffs. No new feature assignments remain active. These are historical ownership labels, not currently addressable agents; `/root` holds the handoff for future resumption. Their checkpoint files report empty live handles.

## Subsequent scope and safeguards

After this installation/build milestone is verified, report remaining parity separately; do not silently resume feature work. Historical accepted slices and unresolved workflows remain in [the acceptance table](parity-worklog.md#workstreams-and-acceptance-evidence) and [dated evidence](parity-worklog.md#verified-evidence-and-open-failures).

Preserve user changes, drafts, sessions, frozen candidates and failed receipts. No cleanup/reset, token logging, live Slack writes/huddles, device/audio action, notification, signing/trust/package registration or guessed publisher identity. Package identity/migration and later OS invocation authorization remain unresolved. Keep idle/scroll holds until corrected and reviewed. Native verification requires original-process ownership, a matching build, cleanup evidence and inspected PNGs. Follow [repository guidance](../AGENTS.md) and [design system](design-system.md). No commit or native capture was made during this milestone.
