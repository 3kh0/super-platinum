# Current state and desktop failure repair

**Desktop failure repair verified locally.** All previously outstanding desktop test failures now pass. The user-authorized 48 GiB repair-phase budget is configured in the helper, and caches were retained. The earlier handoff below is historical baseline, superseded by the completed repair checkpoint.

## Completed repair checkpoint

- **The full locked desktop suite passed: 5,040 passed, 0 failed, 7 existing manual measurements ignored.** The root library and root doctest targets also completed successfully with zero tests. Final command: `scripts/rebuild.ps1 -Task DesktopTest -Jobs 1` (dev-fast, unchanged fixture features, two test threads). Compilation/linking took **4m30s**; desktop execution took **175.65s**. Log: `tmp/desktop-failure-repair-20261026/desktop-batch-d.log`.
- The final log was checked by exact test name against all **92 originally failing tests**: every one reports `ok`, including the three already repaired before this phase and the **89 outstanding cases repaired here**. All **30 original eight-file targets** are verified. No test was newly ignored. Machine-readable results, ignored names and executable hashes: [test-results-final.json](../tmp/desktop-failure-repair-20261026/test-results-final.json).
- Affected groups within the final full run: **Profile rich 257/257; Canvas 421/421; sharing-peer 6/6; Home inputs 280/280; List comments 328/328; group recovery 22/22**. These are matching groups in the full run, not additional test totals. Earlier explicit focused runs and failure reproductions remain in the repair directory.
- Production fixes stay in the desktop boundary: fixture history callbacks return safely before borrowing a disposed Signal; Home inline controls use the actual current view for source presence while retaining the conflicting dirty form; automatic List file acquisition cannot rearm from an old transcript after a socket change. A successful accepted transcript read for the current socket permits new work; a failed/offline refresh does not. Focused regression assertions cover the retained root, exact old row allocation, dirty source custody, and retry/read-generation boundaries.
- Fixture repairs supply supported synthetic identities, required profile objects, authenticated account/read contexts, paired Home visit owners, a Dioxus runtime for native document effects, and proper source-action/configuration/legacy-store prerequisites. Title display expectations match the existing trimming oracle while raw member whitespace remains checked. Four Unicode captures were repaired without changing UTF-16 offsets, trust/chronology checks or compact JSON formatting. Canvas certificate tests retain rich content created through the actual content-edit/ACK history; no Canvas proof guard was relaxed. Inline annotation setup now uses the production timestamped API with its original selection. Core source and lockfiles are unchanged.
- Verification progressed through **4,978 passed / 61 failed**, then **5,016 / 23**, then **5,038 / 2**, before the final green run above. Root `cargo fmt --check` and diff whitespace checks passed. One intermediate compile stopped before linking on a private fixture constant; it was corrected before subsequent execution. Failures were not counted repaired before their tests passed.
- The final developer build passed in **7m48s**: `scripts/rebuild.ps1 -Task Build -Jobs 1`. Current executable: `target/desktop/dev-fast/super-platinum.exe`, **278,606,336 bytes**, SHA256 `9999c0a5004fb618c5b1a779928eb5408f98aab6138834200104a031fb401c19`. The older executable hash below belongs to the historical handoff. No native UI capture or live Slack check was rerun for this repair phase; no layout or live-operation acceptance is claimed. The independently locked core suite was not repeated because core source is unchanged; its earlier 5,424-test plus one compile-fail-doctest pass remains baseline evidence.
- The helper consistently uses the user-authorized **48 GiB combined target budget** for this phase; the **50 GiB free-disk floor** and **4 GiB commit-headroom margin** remain unchanged. Final measured targets: **39.816 GiB** (desktop **24.433**, core **15.383**), free disk **170.65 GiB**, commit headroom **25.51 GiB**. [Final build/storage audit](../tmp/desktop-failure-repair-20261026/build-and-storage-final.json). All existing cache directories were retained; no cache cleanup was performed.
- Local repair and verification are complete. Changes remain uncommitted at HEAD `9de68905e314b0f49614b647306cc795160612d5`. No commit, push, live Slack operation or destructive cleanup was performed.

## Previous bounded stabilization handoff

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

Decisions needed before release approval: triage the unresolved desktop failure groups and arrange an explicitly authorized real-session/warm-boot/reconnect smoke test. The bounded stabilization handoff is complete; these are release decisions and future work, not permission to restart feature expansion. No live writes, cleanup, commit/push, packaging or deployment is authorized. That stop boundary belonged to the prior task. The newly authorized desktop-failure repair continues from the active checkpoint above.
