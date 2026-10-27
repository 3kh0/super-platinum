# Windows development build workflow

Feature expansion is frozen. Adopt **dev-fast, eight Cargo jobs** for desktop
iteration based on the completed continuation and two edited builds below. This
is a practical choice, not proof of a globally optimal configuration or a
controlled comparison against dev-rebuild. No second profile comparison was needed.

## Commands and cache policy

Use one Cargo pipeline. Desktop artifacts stay in `target/desktop`; independently
locked core tests reuse `crates/super-platinum-core/target`. The helper fixes the
same two desktop fixture features, preserves Ninja Multi-Config, bundled LLD,
`/STACK:16777216` and `RUST_MIN_STACK=16777216`, and restores its environment.
Core retains its existing Visual Studio 18 2026 CMake generator. Both lockfiles,
Cargo profiles, dependencies and Rust/Cargo 1.98.1 remain unchanged. No pagefile
settings or persistent system configuration changed.

Choose one relevant command per edit or coherent batch; this table is not a queue.
The process-only launcher handles this laptop's script execution restriction.

| Purpose | Command |
| --- | --- |
| Production type check | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task Check` |
| Changed Rust tests or fixtures: compile without linking | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task CheckTests` |
| Focused domain tests | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task CoreTest -Filter 'canvas::'` |
| Focused desktop test execution | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task DesktopTest -Filter 'mounted_vectors_share_dependency'` |
| Native fixture binary | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task Build` |
| Check the repaired example | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task Check -Example emoji_catalog_lifecycle` |

Defaults are `-Jobs 8 -Profile dev-fast` for desktop; CoreTest uses its independent
Cargo test profile and does not take `-Profile`. Add `-DryRun` to inspect commands.
The native executable is `target/desktop/dev-fast/super-platinum.exe`. Avoid direct
historical wrappers/target paths, routine profile switching, and `cargo clean`.
The old `cargo rebuild` alias still selects dev-rebuild and is not this workflow.

A test filter selects which tests execute. It does **not** avoid compilation or
linking of the entire desktop test binary. The helper's filtered DesktopTest also
selects `--bin super-platinum`, which excludes other Cargo targets. Core filters
similarly select `--lib`, then filter execution within that complete harness.
Unfiltered DesktopTest/CoreTest run the whole corresponding package suite,
including integration tests and applicable doctests/examples. CheckTests does
metadata/test compilation only and cannot substitute for executed tests.

Eight jobs are a reasonable starting point on 16 logical CPUs, not a measured
claim that eight beats every other count. Dependencies were already cached during
continuation. Cargo recorded only one active compilation unit: the application.
Jobs can affect codegen/native scheduling but do not parallelize its large
frontend or remove its peak memory. Do not raise concurrency to treat paging.
Batch coherent changes to amortize the frontend, cache loading and linking work.

## Measurements

All three completed commands were exactly:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task Build -Profile dev-fast -Jobs 8
```

Both warm runs changed compiled error text in `src/desktop/main.rs`. Run 1 added
punctuation to the adoption error. Run 2 restored that text and changed the
separate enrollment error. The original source was restored byte for byte after
both runs. Neither was a no-op. No dependencies or flags changed between runs.
The last measured executable contained the temporary second error-text edit.
The final stabilization Build has now replaced it with a source-matching executable;
see docs/CURRENT-STATE.md for the hash, executed failures and native acceptance.

| Run | Complete monitored elapsed | Cargo total | Application unit | Target growth | Combined targets after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Cached continuation | 345.21 s | 342.7 s | 333.28 s, start 9.45 s | +4.853 GiB | 13.941 GiB |
| Warm edit 1 | 117.85 s | 113.9 s | 102.40 s, start 11.45 s | +3.573 GiB | 17.514 GiB |
| Warm edit 2 | 73.17 s | 67.9 s | 66.77 s, start 1.13 s | -0.111 GiB | 17.403 GiB |

Cargo reports and raw samples are under `tmp/build-workflow-20261026/` in
`continuation-8`, `warm-edit-1` and `warm-edit-2`. The HTML reports show dependencies
reused and no finer frontend/codegen/link sections (`sections: null`). Unit start
and duration above are Cargo's actual timing data; frontend/codegen/link activity
was observed through process/CPU/I/O samples, not assigned invented phase times.
Complete monitored elapsed also includes helper preflight and sampling delay.

The earlier cold preparation was forcibly stopped at 401.45 s by a physical-RAM
policy while commit headroom remained around 17.6 GiB. That was not compiler OOM
and did not disprove dev-fast. This follow-up reused its partial cache and
completed. Active build time across that interrupted attempt and continuation was
746.66 s; this is not a clean-cold throughput measurement or a profile comparison.

| Observation | Continuation | Warm 1 | Warm 2 |
| --- | ---: | ---: | ---: |
| Peak summed compiler private memory | 16.77 GiB | 10.87 GiB | 10.24 GiB |
| Peak summed compiler working sets | 10.18 GiB | 8.79 GiB | 7.99 GiB |
| Minimum available physical RAM | 0.225 GiB | 1.807 GiB | 2.295 GiB |
| Minimum system commit headroom | 8.98 GiB | 15.41 GiB | 16.09 GiB |
| Mean / peak sampled system CPU | 15.1% / 47% | 9.0% / 33% | 7.3% / 20% |
| Approx. hard-page input / output | 12.34 / 0.56 GiB | 5.52 / 0 GiB | 3.33 / 0.19 GiB |
| Mean system disk read / write | 38.7 / 36.2 MiB/s | 80.4 / 24.1 MiB/s | 49.8 / 12.1 MiB/s |
| Peak sampled disk queue | 4 | 3 | 5 |

Samples were taken roughly every 3-4 seconds; brief peaks can be missed. Summed
working sets may double-count shared pages. System CPU/disk counters include other activity. Hard-page input includes mapped-file
reads, so these totals are not pagefile-only traffic. Observed pagefile allocation was
20 GiB; observed use peaked around 3,000 MiB during continuation. Physical RAM
became scarce but compiler CPU time and output continued advancing; eight jobs
were retained without adjustments. Warm 1 includes incremental state loading and
file I/O. Warm 2 was faster with a modest paging burst and no sustained growth.
The preliminary compiler CPU accumulator undercounted short deltas; system CPU
above is the utilization measurement, while that accumulator was used only as a
progress signal (corrected before warm 2).

## Resource limits and final gate

Keep **40 GiB combined target high-water** and a **50 GiB free-space floor**.
The helper checks both before starting, plus a **4 GiB commit-headroom safety
margin**. Ordinary helper runs report memory but do not monitor/kill after launch.
The experimental monitor enforced the storage/commit thresholds throughout; low
physical RAM, paging and high CPU were observations, never automatic kills.
Check growth after heavy commands; Rust manages its incremental sessions, but
future growth is not guaranteed bounded by these two edits. No caches, evidence
or sources were deleted. No cleanup is currently proposed.

The follow-up has a 45-minute active-execution cap. No additional benchmark is
planned. A progressing build at that cap must be reported and preserved rather
than killed for elapsed time. No extra dev-rebuild profile artifacts were created.

Run the final stabilization gate once the coherent stabilization work is ready,
serially, with resource checks between heavy stages:

```powershell
cargo fmt --check
cargo fmt --manifest-path crates/super-platinum-core/Cargo.toml --check
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task CheckTests
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task CoreTest
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task DesktopTest
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1 -Task Build
# Run the relevant owned native fixtures against that binary and inspect PNGs.
```

The checked-in emoji lifecycle example now imports the maintained public core
Catalog, with its output describing that current implementation. No historical
scratch tree was restored. Its locked dev-fast check passed in 60.27 s complete
elapsed (Cargo 58.80 s); log: `tmp/build-workflow-20261026/example-check.log`.
Root `cargo fmt --check`, helper PowerShell parsing/eight command previews,
generator-conflict refusal/environment restoration, and `git diff --check` passed.
Application restoration and unchanged manifest/lock/config hashes were verified.
The independently locked core source/cache are unchanged; no new core test run.
After the example metadata check, desktop/core targets total 18.044 GiB
(12.428+5.616), with 197.54 GiB free. Only dev-fast exists under the desktop root;
no compiler/Cargo pipeline remains running. No additional evidence framework was added.

Remaining gates: full test-target compilation, desktop test linking/execution
(including the previously observed LLVM memory limitation), complete core tests
and native fixture/image validation were not run in this follow-up. The public
fixture-import repair remains source-reviewed rather than test-compiled. The
last measured executable contains the temporary error-text edit until the next
ordinary Build; restored source is authoritative. The three completed builds and
example check consumed about 9m57s of measured execution, within the 45-minute cap.
No claim of global optimization or a completed profile comparison is made.
Product feature expansion remains paused; stop build experiments here.

## Final bounded stabilization outcome

The final gate has now run; it is not green. Core passed 5,424 tests plus one
compile-fail doctest (six ignored). Desktop linked and executed: 4,947 passed,
92 failed, seven ignored. One focused acquisition-mock repair passed all seven
module tests, addressing three failures; 89 reported failures remain unresolved.
Final Build passed in 1m05s and the bounded native checks ran on that executable.
Current scope, logs, launch instructions and limitations are in
[the release handoff](CURRENT-STATE.md). Historical pending statements above
describe the earlier build-preparation follow-up, not the latest gate.

The immediate post-build inventory reported about 40.13 GiB, just over the
40 GiB budget. The final PowerShell and Python inventories both agree on
39.726 GiB, with about 173.2 GiB free. The difference is unexplained; the earlier
overrun observation is retained. No cache cleanup occurred. The cleanup request
is withdrawn because none is required to finish the handoff. Future heavy builds
need another headroom review; no additional build is planned for this goal.
