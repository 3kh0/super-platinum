# Current state and handoff

**Full parity ACTIVE; build tuning FROZEN.** Preserve accepted work and continue through observable user-facing gaps. The [original acceptance table](parity-worklog.md#workstreams-and-acceptance-evidence) retains the full scope; this checklist does not narrow it.

## Active integration

**Profile Canvas missing usergroup titles** - [checkpoint](../tmp/profile-canvas-missing-usergroup-title-20261024/checkpoint.json).

Known groups now work, but a title referencing a locally absent group cannot use the nested acquisition path: file dependencies contain groups, while recovery currently follows actual-user IDs or outer group tokens. Verify the retained original selector's immediate return, label and scheduling behavior before choosing acquisition/placeholder custody. Reuse shared source ownership; do not assume blocking recovery matches the original or fabricate marker authority. No new implementation is claimed yet.

## Latest accepted milestone

[Profile Canvas known usergroup titles](../tmp/profile-canvas-usergroup-title-20261024/integration/acceptance.json) passed bounded offline acceptance: **12 cases, 78 stages and all 78 original PNGs individually inspected**. The nested resolver keeps groups separate from users and pins shared history plus account/team/socket ownership. Rename preserves an open draft's label; fresh opening and actual Use/Save/readback adopt the current label. Cancel and stale pre-mount/socket/account responses passed.

Group refresh failures remain quiet and count as attempted/fresh. Immediate reopening sends no duplicate request; genuine reconnect permits recovery. The [closed audit](../tmp/profile-canvas-usergroup-title-20261024/integration/closed-audit-a.json) verified one binary, 156 capture edges, 34 primary group requests, 24 primary file reads and all 12 process closures. Numeric app exits remain unrecorded. Original harness and first native-oracle failures are preserved. Exact cold-file lease and mounted-editor Save assertions were corrected; fresh native acceptance used the same binary with no Rust change or rebuild.

Missing-channel acquisition, private-member/nonmember/public/emoji title acceptance, ordinary history, reconnect, Later, Canvas navigation/draft protection, Profile Save/readback and shared metadata carry forward. Exact older scopes/limits remain in [2026-10-20](parity-history-20261020.md), [2026-10-21](parity-history-20261021.md), [2026-10-22](parity-history-20261022.md), [2026-10-23](parity-history-20261023.md) and [2026-10-24 history](parity-history-20261024.md). Do not reopen settled work without new evidence.

## Remaining parity checklist

| Area | Accepted scope carried forward | Next observable boundary / limit |
|---|---|---|
| Canvas | Native navigation/draft custody, supported paragraph/title edits; bounded start/middle/end and repeated splitting/typing; rich owner/child/gap continuation; both deletion orders with exact histories and inert recovery. [Latest acceptance](../tmp/canvas-owner-first-pair-delete-20261024/integration/acceptance.json). | Broader multi-paragraph/rich editing, conditional-manager scheduling and Classic acquisition/editing remain open. General multi-splice/large split components and server retention are unverified. |
| Profile | Section Save/readback, supported member/channel/group labels and freshness, time refresh, supported Canvas title/reference/completion paths, failure/recovery/cancellation. See dated history. | Canonical ordinary emoji, known public/private-member title leaves and explicit English private-nonmember placeholders are accepted within recorded scopes. Shared missing-channel acquisition and sealed local-marker custody are accepted within the recorded offline scope. Known ordinary usergroup title leaves are accepted. **Active: locally absent usergroup title semantics/acquisition.** Remaining metadata-backed tokens and broader rich title/template paths need mounted Save/readback and failure evidence. |
| Shared metadata | Channel initialization/retention/socket ordering/selective recovery/queued freshness; mounted Activity clock refresh; PlanInfo warm persistence within recorded scopes. | Team replay, optional consumers and broader restart persistence. |
| Apps/workflows | Publication, permissions/managers, Add/Feature, details/navigation/draft custody, Share delivery/Cancel/Resend, posting refresh and supported people/channel/group destinations. [Detailed scope](parity-history-20261021.md). | Broader recipients/entry origins, CSV, rich/localized titles, polling, dynamic/branch/wrapped controls, shared acquisition and restart persistence. |
| Lists/comments | Supported rich/plain own edits, deletion and durable recovery; reply drafts, failure/Retry, reactions/navigation; supported title/emoji/member/group catalog and editor policies. See dated history for exact modes. | Semantic clipboard/native policy branches, unsupported edit modes, attachments/richer tokens, batch/history/suffix titles and broader field/table paths. OS keyboard/IME/restart remain unverified. |
| Conversation, compose, files, Later, navigation | Cold Later routing and stale-response ownership accepted above; other earlier scoped milestones retained in history. | Reconnect, first-unread opening and ordinary cursor continuation are accepted within the recorded offline scopes. Other cold history/outage, rich edit/mention, scheduled-forward, modal-refresh, uncertain-delivery and cross-surface gaps remain. |
| Notifications/huddles | Safe offline work and preserved drafts remain actionable. [Notification checkpoint](../tmp/windows-notification-package-cold-work/PAUSED-CHECKPOINT.json). | Account/DND/activation, Windows call/device and OS integration acceptance remain incomplete. Real notifications/calls/devices/package registration require specific authorization. |
| Visual, accessibility, performance | Normal/narrow checks are milestone-specific. [Idle/scroll acceptance](../tmp/idle-scroll-native-20261018/inspection.json) is bounded synthetic evidence. | Broader focus/accessibility/keyboard/IME and large-workspace runtime coverage. Preserve documented scroll/viewport limits; do not infer broad acceptance from sampled geometry. |

## Build and verification

Build optimization is frozen at the verified baseline: **12 Cargo jobs, Ninja Multi-Config, bundled LLD, existing 256-codegen-unit/unwinding `dev-rebuild` profile**. `scripts/rebuild.ps1` remains the full-rebuild helper. The measured cold baseline was 250.35 seconds and cached rerun 8.58 seconds ([evidence](../tmp/rebuild-speed5/result.json)); later experiments were not adopted. No cold benchmarks, cache isolation or profile experiments unless a concrete failure blocks product work.

The functional pipeline uses the existing memory workaround: `--locked --profile dev-rebuild --jobs 1 --target-dir tmp/dev-rebuild-ninja-multi-target5 --features original-download-native-fixture,protected-native-fixture`, the Visual Studio Ninja environment and `RUST_MIN_STACK=16777216`. This does not change the 12-job default. Core checks reuse `tmp/smd1-target`. Keep one Cargo pipeline and do not rebuild unchanged sources merely to repeat acceptance.

Latest accepted executable: `tmp/profile-canvas-usergroup-title-20261024/integration/build-a/native.exe`, SHA256 `279dcd41f72cf19964e86f4dbf0a6e31ab817d460981329677eda40af9b29ff2`. Cached build passed in 424.40 seconds with unchanged inputs. All twelve accepted cases used this executable; the audit matched 5,256 compiled inputs and 104 root helper bindings. No speed comparison is inferred.

Formatting passed. All 48 focused independently locked title tests, 35 portable checks, and 30 retained original nested vectors plus two guards passed. Desktop test compilation passed in 372.88 seconds; **desktop Rust tests were not executed** because of the recorded LLVM memory failure. The native user regression uses nullish Quip; nonempty/empty override bypass is distinguished by original/core evidence, not claimed from that native regression. Prior unchanged editor checks retain their recorded scope. No live Slack, notification/device/OS-input, server persistence or broad performance acceptance is claimed.

Native acceptance requires a matching executable, actual offline interaction, owned-process closure/cleanup evidence and original PNG inspection. Preserve the existing 250 ms response, pointer, visibility and ownership checks. Source review/compilation alone is not user-visible acceptance. Reuse valid unchanged evidence and report real failures.

## Boundaries

Preserve all edits, drafts, caches, sessions and evidence. No live Slack writes/messages/calls/huddles, notifications, device/audio/OS input, signing/trust/package registration, destructive cleanup, pushes, deployments or secret logging are authorized. Do not pause or complete the full-parity goal at an intermediate milestone. Follow [AGENTS.md](../AGENTS.md) and the [design system](design-system.md).

[Worklog](parity-worklog.md) is append-only in this session because its existing bytes include invalid UTF-8. Preserve current-state CRLF. Historical next-step statements are superseded by the active checkpoint above.
