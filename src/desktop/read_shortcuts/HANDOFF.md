# Reading shortcuts renderer handoff

Root owns Cargo, native builds/launches and the parity worklog. No live Slack
writes were used to develop or verify this slice.

## Matching retained source

Aliases and SHA256 are recorded in `docs/parity-worklog.md`: R a66a78eeac9bb080,
B2 457d4b99bff69d13, S2 c8d6ded6605ce42d, V2 c3c571e00152d656. Offsets below are
decoded character offsets, not bytes; older B/S offsets are not interchangeable.

- R614311 defines plain Escape and ShiftEscape without `ignoreInputs`.
- R999130 ClientKeyboardShortcutsHome passes UbRM's command array into 3TJy.
  B2 16729466 (3TJy) invokes Ov+1's hook; the hook retains command options and
  passes them into o9RR.KeyCommands. B2 652538 (o9RR) overrides Mousetrap's
  default `stopCallback` to false and adds the original input filter only when
  `ignoreInputs` is truthy. These commands therefore accept both empty and
  nonempty plain canonical composers if the key remains unconsumed.
- B2 23914489/23914734 installs Texty's Escape binding. B2 3559004 (R/Vz)
  gives editor modules first use, then empty blockquote/code-block formatting
  cleanup, then the external Escape callback, otherwise permits propagation.
  B2 23946808 completion Escape closes its menu and consumes the key. B2
  19360929/19361529 binds message editing Escape to stopEditingMessage.
  We preserve native composition, completion/menu and explicit local-editor
  boundaries; we do not infer a typing timeout or discard nonempty drafts.
- B2 16640869/16641110 registers thread onEsc with the scoped view container;
  B2 16647897/16648779 reads the subscribed thread and clears initialLastRead
  before a later close. B2 29965159 rejects modified/prevented Escape and closes
  only the eligible focused container. An open background thread is insufficient.
- B2 2518757 (dbCg) requires an actual message, excludes xmpJ.A and iub5.aw.
  S2 3856198/3856314 resolves xmpJ to missing/temp (`x`) timestamp exclusion.
  S2 3838502 resolves iub5.aw to w9uO; S2 3861208 is `is_ephemeral`.
  No invented subtype/deleted-message exclusion was added.
- B2 4606620 uses the active primary channel and retained ghost/unread state.
  S2 2291322/3705366 selects the later channel metadata or suitable loaded ts.
  Core's conservative no-rewind handling compensates for our incomplete cache;
  it is documented as defensive handling, not a claim about Slack's default.
- S2 1430018 (rGEL fetchReplies) sends response messages to D8Sp preprocess;
  S2 2804124 calls slurpThreadSubscriptions at 2807312. Only a message with its
  own `subscribed` key contributes `{channelId, ts, subscribed, lastRead}` from
  `last_read`. Read-only thread reconciliation therefore requires the exact root,
  numeric cursor and an explicit subscription boolean. Cursor and subscription
  are adopted together under read/subscription revision and pending guards;
  false permits a fresh Escape to close without another mark. Absent/malformed
  subscription remains uncertain. It never infers
  a cursor from a reply or from subscriptions.thread.get membership IDs.

## Ownership and integration

- `eligibility.rs`: pure modifiers/editor/attention and exact captured visit.
- `keyboard.js`: native focus/IME/local menu/editor facts; never copies draft text.
- `bridge.js`: one-use event receipts, invalidated by focus transitions, blur,
  visibility, DOM detach, new key, rendered owner/view change or popup. A bounded
  observer exists only while a receipt is queued; it latches transient shell-key,
  popup visibility and blocker role/data-attribute transitions, including ABA.
- `escape.rs`: pure core decision wiring, credential capture before DOM await,
  same-decision revalidation, single-attempt channel/thread dispatch, exact
  pending ticket cleanup, read revision ACK guards, current-visit divider clear.
- `receipts.rs`: uncertain writes retain account/user/target barriers across shell
  replacement, blocking automatic, manual and bulk writes. Another Escape only
  performs read-only reconciliation. A further fresh gesture can release a
  successful readback only if its strict owner, observed cursor and post-apply
  revision and subscription value/revision still match, with no subscription
  mutation pending. A changed proof returns to uncertainty without resending.
  Readback publication additionally requires the captured view and surviving
  DOM/native focus; stale completion cannot publish a current-pane decision.
  Channel recovery retains the exact counts row and adopts it through the
  canonical single-row read-state reducer under cursor/revision guards. Remote
  manual unread can move this authoritative cursor backwards while preserving
  its unread/mention counts; no one-row membership snapshot is constructed.
  A captured target-only counts/latest baseline rejects newer realtime counts
  even when they did not change the cursor revision; unrelated channels may move.
- Shared hooks: main keyboard routing, rendered shell key, shell State/defaults,
  thread initial-divider accessors, automatic-read overlap guards, realtime
  independent clear-all confirmation preference update and generation test.
- Bulk confirmation/preference/Undo and reconciliation are owned by read_settings
  and bulk_reconcile. `bulk::{request,view,poll,blocks_reads}` is the interface.
- Native fixture/checker belongs to read_navigation_fixture. Only exact
  `fixture::owner` permits no-session mark preparation. `record_mark` must return
  true for the explicitly opted-in disconnected fixture before the production
  ACK application helper runs. Its receipt is synthetic ACK evidence, not HTTP.

## Validation and next gate

Thirteen keyboard/bridge Node tests pass, covering transient blockers and shell-key
ABA as well as one-use receipts, focus, IME and draft preservation. Root's initial
focused Rust gate passed 36 tests before the final retained-proof delta. New
focused regressions cover lost ACK ordering across shell replacement, proof
invalidation by cursor/revision/credentials, and unsubscribed thread recovery
with subscription revision/pending/ABA guards.
Channel recovery also tests a lower authoritative cursor, preserved unread and
mention totals, barrier release, and rejection after a newer local read revision.
Same-cursor incoming count/latest drift is rejected; unrelated-channel drift is
accepted.
The first native Escape and bulk runs stopped before their first action. A
concrete receipt false-invalidation fix now ignores unchanged attribute writes,
empty completion child-list updates, and unrelated child updates beneath an
ancestor with retained hidden blockers. True popup/key ABA tests still pass;
bounded fixture diagnostics and a fresh native run must establish the failure's
runtime cause and verify the corrected path.
Scoped rustfmt parsing passes; root owns compilation of these final changes.
Root must run final locked core/desktop gates and inspect normal/narrow native
fixture PNGs before claiming complete shortcut parity. Pending native scenarios:
metadata-only channel, retained ghost, first thread read/next close, background
thread focus, empty/nonempty composer draft preservation, completion/IME/menu,
modifier/repeat, old scope/account receipt rejection. Bulk has separate coverage.

## Windows native continuation, 2026-10-14

The corrected Escape bridge passes both Windows sizes in `tmp/agent-ui/rs-next/`
on SHA `50c8ca39e2d281240ce167db7659b14cab7c767225e39e8ff0ddbb0739f5dad9`.
All twenty PNGs were inspected. Each run prepares four conversations.mark and one
subscriptions.thread.mark request. Metadata-only channel, ghost, first thread
read/second close, primary versus background thread focus, draft text/selection,
eight guarded keys and three stale events pass exact request/state assertions.
These are synthetic keyboard events and acknowledgements, not OS keyboard or
live Slack proof. The first failed captures remain retained.

Bulk also passes both sizes in `tmp/agent-ui/br-final3/` on SHA
`13e28b398bc82ce3e10cb32b2a1716ca5adcb38c2f4df3af2cf1798a82a9fa52`.
All 28 PNGs and both closed receipts were independently audited, including exact
request ordering, Undo, expiry, focus return, owner guards, process identity and
pointer restoration. Its reconciliation fixture is counts-only. The full async
Threads/Activity/history/replies pipeline now passes 25 focused async/reducer
tests, including eighteen new cases using the actual production sequence with
synthetic execution. Exact subscription/cursor guards reject missing or malformed
reply authority and ABA; successful explicit retry restores the notice copy.
The full source gates pass core 1,398 / 5 ignored and desktop 1,427 / 7 ignored.
This source cohort has not been rebuilt into a dev executable. Live
acknowledgements and full workspace-history coverage remain open.
