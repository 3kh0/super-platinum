# Current state and release handoff

**Bounded stabilization finished with a runnable developer candidate and unresolved release gates.** Feature expansion is frozen. Full Slack parity is future backlog, not this goal's completion condition. No commit, push, deployment, live Slack write, device/notification action or OS input was performed.

## Runnable executable

From a fresh PowerShell terminal, with no fixture environment set:

```powershell
Set-Location 'C:\Users\mortar\Documents\GitHub\super-platinum'
& .\target\desktop\dev-fast\super-platinum.exe
```

Absolute path: `C:\Users\mortar\Documents\GitHub\super-platinum\target\desktop\dev-fast\super-platinum.exe`.
SHA256: `df21424ab8605802406a477aec33b374e7a5f508dae67f19bf56aa36e1738e8f`.
The final locked dev-fast Build passed in **1m05s**. This replaces the executable containing the temporary measurement edit. Rust source did not change after this Build; subsequent edits affect Python verification and documentation only. This is a developer build with the existing fixture features, not a signed/package release.

For a disconnected preview in a separate terminal, set `$env:SUPER_PLATINUM_FIXTURE='channel'` before the same launch command. Native checks used private fixture endpoints and synthetic data. Actual existing-session restoration was not launched, because ordinary startup can trigger live Slack activity.

## Compact acceptance matrix

All new evidence is under [tmp/release-candidate-20261026](../tmp/release-candidate-20261026/). Unit-test counts below are actual executed passes within the otherwise failed full desktop attempt; they do not make that suite green.

| Selected journey | New acceptance result | Failure / unverified boundary |
| --- | --- | --- |
| Launch, session restore, warm boot | Source-matching executable launched in disconnected fixtures. Full core suite passed config/session migration and cache tests. Private thread-draft database survived a second process; file journal survived three processes. | Real persisted Slack session/warm-cache boot not run. Three desktop PlanInfo warm tests failed with `channel_initialization_unavailable`; the separate history fixture test failed with a dropped Dioxus signal. No general warm-boot acceptance claim. |
| Channel/DM navigation, history, opening position, threads | 28 shell-state tests, 4 ordinary-history ownership tests and 24 realtime tests passed. Native `channel-window-10000` passed five scroll jumps, bounded projection (21-35 mounted rows), selection and date/unread target agreement. Channel, DM empty surface and narrow thread PNGs inspected. | DM rendering is static coverage; no mounted DM row click was driven. Real cold history, cursor continuation and thread open/close against Slack were not exercised. |
| Composer/thread drafts across navigation, cancellation/reopening | Shell composer-custody tests passed. `rejected-thread-return` passed restore plus second-process reload: channel draft preserved, exact thread draft editable/focused, broadcast choice retained, uncertain sibling preserved, zero prepared requests. Five original PNGs inspected. Title lifecycle also exercised held Cancel and reopen custody. | Physical keyboard/IME and real-server draft synchronization remain unverified. |
| Supported send/edit/reply/reaction, files, search, Later | All 51 messaging tests and 12 Later tests passed; realtime reaction-copy/partial-merge tests passed. Existing native send-outcomes hook passed at narrow width, preserving the newer composer and reaching all four stopped delivery rows. File recovery passed three-process exact SQL readback/retirement; three PNGs inspected. Search-error and Later-error PNGs show usable Retry and retained cached items. | No live send/edit/reply/reaction/file mutation. Search success/result navigation and Later cold-navigation were not driven. Several Canvas/List comment/title tests remain failed; see failure report. |
| Offline/reconnect, stale responses, failure/Retry | Ordinary-history ownership, messaging and realtime generation guards passed, including late-release and old-account rejection. Native title failure/Retry/Cancel passed. File recovery and rejected-thread feedback passed. `connection-no-network` state reports `no_network` with cached transcript still visible. | Physical outage/reconnect, network probe recovery and live Retry not run. Older reconnect/cursor/Later native oracles are missing in this checkout. |
| Normal/narrow layout, focus, encoding | Four title processes/40 stage PNGs inspected individually; no blocking modal layout defect. Six static fixture PNGs, five draft PNGs, three file PNGs, one history PNG and one delivery-status PNG inspected. Main channel capture visibly confirms corrected arrows and ship emoji. Native draft hook proves focused/editable composer. | No physical Tab traversal, accessibility-tree or IME acceptance. Narrow thread overlays the channel as designed. Other fixture-only mojibake remains in `boot.rs`; broad typography cleanup was intentionally deferred. Static captures do not prove workflow execution. |

### Prior evidence carried with limits

The [Canvas cancellation/close-reopen acceptance](../tmp/canvas-retirement-cancellation-20261026/integration/acceptance.json), its build/check records, closed audit, native chain and inspection records remain present: historical **4 processes / 24 stages / 24 inspected PNGs**. Eleven of twelve frozen desktop bindings still match; `boot.rs` differs. This is retained prior bounded evidence, not a rerun on the new executable. Current retirement/readback unit-test failures are separately retained and must not be hidden by that native record.

Several historical reconnect, ordinary-history, Later, workflow-draft and idle/scroll artifacts/oracles are absent. Their Markdown summaries are historical claims, not currently inspectable acceptance. Earlier scope remains documented in [dated history](parity-history-20261026.md), [2026-10-25 history](parity-history-20261025.md) and the [parity worklog](parity-worklog.md); no full-history re-audit was attempted.

## In-flight title slice: accepted within its frozen descriptor contract

[Checkpoint](../tmp/profile-canvas-tone-title-20261026/checkpoint.json). The full core run and executed desktop mounted-vector regression passed. The existing direct lifecycle checker passed `tone` and `tone-empty` at normal and narrow widths: **4 processes, 40 stages, closed logs revalidated, exact raw title/request/readback custody checked, owned processes closed**. [Visual review](../tmp/release-candidate-20261026/title-visual-review.json) lists all 40 original images.

The first attempt failed before interaction because the stationary cursor overlapped the WebView. Retry used the existing `visual_placement.fit_work_area` clamp policy through a process-local override, preserving a fitting initial window; the existing passive pointer and 250 ms caption guards remained enabled. No OS pointer movement occurred. The aggregate historical wrapper was not used because it binds absent historical files; its existing per-case checker was reused.

Accepted labels visibly contain literal canonical descriptors such as `:wave::skin-tone-4:`. These captures establish descriptor selection/presentation, Use/Save/readback/reopen, failure/Retry, held Cancel and length limits. They do not establish emoji-glyph rendering, upstream parser admission, arbitrary aliases/custom/Unicode titles, locale breadth or live Slack behavior. A previous "Profile fields saved" status persists beside later failure/validation copy; current feedback remains visible, and that clarity limitation is retained.

The old portable tone command failed at import because `tmp/profile-canvas-emoji-title-20261024/scripts/test_emoji_title.py` is absent; **no portable tests executed in that attempt**. Its prior recorded passes were not relabeled as new results.

## Executed checks and failures

| Command / gate | Actual outcome |
| --- | --- |
| `cargo fmt --check` | Passed initially and after the Rust test repair. |
| `cargo fmt --manifest-path crates/super-platinum-core/Cargo.toml --check` | Passed. Core source unchanged in this stabilization. |
| `rebuild.ps1 -Task CheckTests` | Passed, **5m21s**; compilation only. Public title-fixture import repaired before this run. |
| `rebuild.ps1 -Task CoreTest` | Passed: **5,424 tests + 1 compile-fail doctest**, 6 ignored. Compile 5m29s; unit execution 331.98s. Four zero-test integration targets also completed. |
| `$env:RUST_TEST_THREADS='2'; ... rebuild.ps1 -Task DesktopTest -Jobs 1` | Linked and executed: **4,947 passed, 92 failed, 7 ignored**, 187.27s execution after 9m29s compilation/linking. This closes the historical execution/linker gap for this attempt, with a failing result. |
| `... rebuild.ps1 -Task DesktopTest -Jobs 1 -Filter 'app_home::autocomplete::observations::acquisition_tests'` (2 test threads) | After one evidence-based correction, **7 passed**, 0 failed; compile/link 4m41s, execution 0.04s. Three former failures fixed. |
| `rebuild.ps1 -Task Build` | Passed, **1m05s**, default 8 jobs, same features/profile/target. |
| `python -X utf8 -B -m unittest discover -s scripts -p test_visual_native.py -v` | **6 passed**, including shell-comment catalog regression. |
| Native checks | Four title lifecycles, rejected-thread restart, file recovery, bounded channel window, six static fixtures and existing narrow delivery-status hook passed their stated scopes. **17 successful disconnected processes / 56 inspected original PNGs**, plus one inspected guard-failure PNG. |

All helper invocations used `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/rebuild.ps1`, locked dependencies and the [adopted workflow](build-workflow.md). No profile/dependency/generator change or clean rebuild was introduced. Existing warnings remain. NativeFixture terminates its owned GUI processes after checks; some receipts record exit code 1. This is owned closure evidence, not graceful-shutdown acceptance.

The desktop [failure list with assertions](../tmp/release-candidate-20261026/desktop-failures.json) contains all 92 original failures. **89 reported failures remain unresolved** after the three acquisition mock failures were corrected. The complete desktop suite was not repeated after that focused repair. Unfiltered Cargo stopped at the failed desktop binary: later root targets/doctests were not executed; example compilation is not an executed test pass. Ignored core cases are four benchmarks plus two interoperability exports; all seven ignored desktop cases are manual measurements.

Failure families include member/channel observation prerequisites (including 26 Profile rich-title cases), app input/runtime lifetime assumptions, media-queue assertions, Canvas retirement/readback/structure assertions and List comment/delete/title assertions. Their status as fixture drift versus product regressions is not fully diagnosed. One media assertion reproduces alone (`!media.has_pending()`); single-threading is not a demonstrated fix. The acquisition mock incorrectly treated HTTP `POST` as the Slack API endpoint; its helper now extracts the endpoint from the prepared URL and still verifies POST. No production transport behavior was changed.

**Release approval remains blocked by these unresolved failures and live-session/warm-boot/reconnect gaps.** This handoff supplies a runnable candidate, not a green or service-verified release.

## Build resources and cleanup decision

Targets started near **18.04 GiB** and were 39.73 GiB before final Build. An immediate post-build inventory reported about **40.13 GiB**, briefly over the 40 GiB budget; that observation is retained rather than relabeled as a passing storage result.

The final handoff recheck independently used PowerShell and Python file inventories. Both now agree on **39.726 GiB** combined (desktop 24.343 + independently locked core 15.383), below the 40 GiB budget. The cause of the difference from the earlier reading is unverified. C: has about **173.2 GiB free**, above the 50 GiB floor. Last sampled commit headroom was **26.7 GiB**; sampled headroom during CheckTests reached 4.66 GiB. These are samples, not continuous peak telemetry. No build was killed for physical-RAM usage, and no pagefile/system setting changed.

**No source, session, cache or evidence cleanup was performed. The prior cleanup request is withdrawn because cleanup is no longer required to finish this handoff.** No Cargo or application process remains running. Future heavy builds have little target-budget headroom and need another resource review before starting; no additional build is planned here.

For reference only, the earlier optional proposal was the older completed application incremental session:

`target/desktop/dev-fast/incremental/super_platinum-1byexd45mmrx9/s-hnl1d7f9n2-0u59n4n-ag0w4q2x9qcjqvayxk5wa9ci9` - **3.573 GiB**.

That directory remains present. Any future deletion needs fresh, specific authorization. The runnable executable, newer application session, executable test artifacts, independently locked core cache, source, sessions and acceptance evidence are retained. No extra executable copy was made.

## Changes, backlog and stop boundary

Preserved prior uncommitted work: README/build workflow, rebuild helper, maintained Catalog example repair and public title-fixture test import. New changes: two observed main-fixture encoding literals; acquisition test endpoint mock repair; static visual catalog comment parsing plus regression test; current handoff/workflow/checkpoint results. Both lockfiles, manifests, profiles, persistent settings, user sessions and caches remain untouched.

Full parity work (broader Canvas editing/cancellation, Profile title/parser/locale/glyph breadth, workflows, Lists, OS integration, notifications/huddles, accessibility/IME and large-workspace performance) remains future backlog. Other synthetic `boot.rs` copy defects and stale Profile success-status clarity are recorded limitations, not an invitation to restart expansion.

Decisions needed before release approval: triage the unresolved desktop failure groups and arrange an explicitly authorized real-session/warm-boot/reconnect smoke test. The bounded stabilization handoff is complete; these are release decisions and future work, not permission to restart feature expansion. No live writes, cleanup, commit/push, packaging or deployment is authorized. **Stop here; do not automatically start another milestone.**
