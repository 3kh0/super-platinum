# Current state and handoff

Checkpoint: 2026-10-17, 21:22 EDT. **Feature work is paused. Full Slack parity remains unfinished project backlog.** This checkpoint supersedes historical “currently running” statements, without changing their evidence. All 18 non-root agents have stopped; none reports a live command. The existing root validation job below remains running intentionally.

## Installed main versus staged work

Workspace: `C:\Users\mortar\Documents\GitHub\super-platinum`. Git HEAD is `a7a8a17b73432c18426d840d109c5c23b6d147ef` (`reswap`); the working tree contains substantial existing changes. Do not reset, clean, or treat HEAD as the installed source inventory.

- **Installed main: NPC18, 3,291 source files.** [Installation receipt](../tmp/npc18-main-install1/result.json), [inventory](../tmp/npc18-main-install1/after.json). Inventory SHA256: `4c31d3abd4aa3eaf3eb11b68050a6129caad207c0efa7a91721da90677b9410e`. The existing V9 installer's read-only preflight was rerun during this checkpoint and verified the exact main preimages and 900-target patch; no installation occurred.
- **Existing NPC18 executable:** [build seal](../tmp/npc18-main-build1/build.json), `tmp/npc18-main-build1/native.exe`; executable SHA256 `849b02d31da425a3a8f2e6a6b49b58ab6525d8a7d0d275d24e03016858368982`. This is not a V9 build.
- **Staged V9:** `tmp/board-canvas-current30-integration-v9/`, 3,913 files. [Source manifest](../tmp/board-canvas-current30-integration-v9-prep/manifest.json) SHA256 `d70844c80462f70e2d70088ade71256e3a4148e8ef04f331516a57dd0bea126e`; [inventory](../tmp/board-canvas-current30-integration-v9-prep/after.json) SHA256 `d2a3e2bd1c28e91aa0fe897adb676babdafe4f2b209c78cc0ca3041d6db789b9`. Only two test bodies differ from V8. V9 is **uninstalled**; later feature drafts listed below are outside this cut.

## Remaining live validation

Owner: `/root`; tool session **37168**. Command, from the workspace:

```text
python -X utf8 -B tmp/finish-v9-runtime-gates.py --execute
```

Process details sampled21:20 EDT: parent Python PID18692; child runner PID6852; rustc PID16292 (559.016 CPU seconds). Session37168 was re-polled live at21:22. Stage: **desktop-focused**, still compiling; no focused-test result admitted. Child command:

```text
python -X utf8 -B tmp/run-board-canvas-current30-integration-v9-gate.py desktop-focused --execute --output tmp/board-canvas-current30-v9-desktop-focused1
```

Collect using `write_stdin(session_id=37168, chars="")`; inspect [events](../tmp/combined-v9-runtime-sequence1/events.ndjson), [current raw log](../tmp/board-canvas-current30-v9-desktop-focused1/cargo-1.log), and each completed stage's `gate.json`. If the tool handle disappears, inspect the original process identity, events and logs before deciding it stopped. An observation timeout is not termination.

The existing queue proceeds through focused68, full desktop, core check/production check, focused132, hosted5, full core, external suites and doctests; it stops on failure, with the runner's documented full-desktop diagnostic exception. It **does not install, build the main app, or launch native UI**. Do not restart it, duplicate Cargo, change its source/cache, or kill it to pause this project. Cache: `tmp/cmi1-target`; jobs remain serialized. The checkpoint's separate read-only preflight session13529 finished successfully.

## Validation evidence

| Status | Evidence and limit |
|---|---|
| PASS | V9 [formatting](../tmp/board-canvas-current30-v9-fmt1/gate.json), [production compilation](../tmp/board-canvas-current30-v9-production-check1/gate.json), [default test compilation](../tmp/board-canvas-current30-v9-default-test-check1/gate.json), [feature test compilation](../tmp/board-canvas-current30-v9-desktop-check1/gate.json). Four of thirteen stages closed. [Latest root verification](../tmp/v9-desktop-check-root-closure1/result.json). Compilation is not test execution. |
| FAILED | V8 [full desktop](../tmp/board-canvas-current30-v8-desktop-tests1/gate.json): 3,144 passed, one failed, seven ignored. Preserve it; the two corrected deadline tests require fresh V9 execution. |
| HOLD | Idle wrapper [root reproduction](../tmp/v9-idle-finality-root-repro1/result.json); scroll-v2 [root reproduction](../tmp/scroll-v2-result-write-root-repro1/result.json) and [review](../tmp/channel-scroll-native-combined-v9-v2-independent-review/manifest.json): secondary errors can mask primary failures. Earlier source clearance does not override these holds. |
| PREPARED | [V9 installation/build review](../tmp/combined-v9-main-custody-independent-review/manifest.json), [List tools review](../tmp/list-combined-v9-bindings-independent-review/manifest.json), [Canvas/motion/idle review](../tmp/three-native-v9-independent-review/manifest.json). Unbound; idle is subsequently held as above. |
| UNRUN | Remaining V9 test stages; actual V9 installation/build; native captures and image inspection; later aggregate Rust/native tests. [Warm notification pure checks](../tmp/windows-package-warm-v2-root-verification.json) passed14 methods, but no notification/audio/package execution occurred. |

Historical accepted slices and unresolved workflows remain in [the acceptance table](parity-worklog.md#workstreams-and-acceptance-evidence) and [dated evidence](parity-worklog.md#verified-evidence-and-open-failures). They do not establish current-tree parity.

## Stopped owners and unintegrated changes

Checkpoint links retain exact file identities, tests and resume details. Source-reviewed leaves are not compiled or integrated unless explicitly stated.

| Owner | Preserved work and outstanding boundary |
|---|---|
| acceptance_audit | [Canvas checkpoint](../tmp/canvas-link-layout-followup/checkpoint-paused-20261017.json):40 mutable files,33 Rust tests unrun; hydration, mounted title/conversion and Classic behavior incomplete. |
| acceptance_review; canvas_bindings | [Channel checkpoint](../tmp/channel-conditional-native-pause-handoff/manifest.json):46 sparse files; resync, actual major initialization and queues incomplete. [Scalar trace](../tmp/canvas-bindings-checkpoint-2026-10-17.txt): “BootTimestamp” is actually screen-reader state; self effects are role thunks. Correct before wiring. |
| list_grouping_review; desktop_runtime_fixes | [Management handoff](../tmp/app-workflow-management-product-work/HANDOFF.txt):4,452-file mutable composition; Add/dynamic/branch surfaces incomplete. [Branch checkpoint](../tmp/desktop-runtime-branch-pause-checkpoint.json): wrapped/OAuth/function-ID reviews pending. |
| profile_recording; save_workflow_review | [Apps checkpoint](../tmp/profile-recording-pause-handoff.json): native registry/hydration unintegrated; hydration review unopened. [Missing-object drafts](../tmp/directory-app-missing-paused-checkpoint.json):six drafts,12 Rust tests unrun. |
| activity_core | [Profile checkpoint](../tmp/profile-rich-fixture-titles-work/checkpoint.json):five mutable File/Canvas/date fixture files over reviewed metadata fixture; composition, Rust/native checks pending. |
| board_heading_review; field_delete_core/desktop | [Comments handoff](../tmp/list-record-comments-pause-checkpoint1/HANDOFF.json):4,149-file uncompiled parent; editor modes disabled, emoji allocation-budget HOLD, unadopted leaves and missing exact ECharts asset evidence. |
| activity_desktop | [Cold notification checkpoint](../tmp/windows-notification-package-cold-work/PAUSED-CHECKPOINT.json):11 draft files; publisher release acknowledgment unwired; nine Rust tests unrun. |
| save_dialog_review; interaction_review | [Idle correction](../tmp/idle-native-combined-v9-finality-prep/manifest.json) `bf72b40a…`:12 pure tests pass, review pending. `tmp/channel-scroll-native-combined-v9-v3-prep/` remains mutable/unreviewed. [Review handoff](../tmp/interaction-review-pause-handoff-20261017/handoff.json) preserves holds and queued reviews. |

Other review agents (`list_bindings`, `workflow_runner`, `workflow_uia`, `slash_perf_probe`) are stopped; their reviews are referenced by these handoffs. No new feature assignments remain active.

## Next single integration milestone

**On explicit resumption: install and produce the matching V9 dev executable.** Completion requires all13 original V9 stages passing with raw outputs/source identities verified; resolve failures through explicit successors, never relabel failed evidence. Then use the existing reviewed [installer](../tmp/combined-v9-main-integration-prep/install.py) and [build driver](../tmp/combined-v9-main-build-prep/build.py), preserving backups/documents. Require actual installation receipt, successful Cargo artifact/log, unchanged installed inventory and sealed executable. Native acceptance and later feature integration remain subsequent work—not this milestone's completion claim.

## Safety and unresolved decisions

Preserve user changes, drafts, sessions, frozen candidates and failed receipts. No cleanup/reset, token logging, live Slack writes/huddles, device/audio action, notification, signing/trust/package registration or guessed publisher identity. Package identity/migration and later OS invocation authorization remain unresolved. Use dev builds; no stack/profile changes or parallel compiler. Native verification requires original-process ownership, matching build, cleanup evidence and inspected PNGs. Keep idle/scroll holds until corrected and reviewed. Follow [repository guidance](../AGENTS.md) and [design system](design-system.md). No commit was made for this checkpoint.
