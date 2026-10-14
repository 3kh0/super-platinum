# Native draft projection receipt: implementation plan

This records the reviewed design and its bounded implementation. The Editor
and native projection APIs now pass disconnected Windows verification on
`431a9eccce174e7048bb8797a50e08c6f1ab3aeb04261a62f1d20292dd605356`:
1,235 core / 1,183 desktop tests, formatting and locked dev build; 20 normal/narrow
native cases, 40 processes and all 120 PNGs inspected. Exact exercise/restart
durable dictionaries match. See the worklog for failures, corrections and limits.
Production realtime draft ingress and ordinary remote autosave remain unwired.
This slice does not enable either one.

Read with [draft-observation-integration.md](draft-observation-integration.md).
The original plan used no native/session work. Implementation verification uses
isolated accounts and storage, without real session, clipboard or user draft
reads. Synthetic composition does not establish OS IME behavior; persisted
selection is not proof of backward native-caret restoration. Reference labels
remain JavaScript-controller coverage because the managed plain-text codec
rejects those rich shapes.

## Pinned input ordering: retain the existing handlers

The Dioxus pin in `Cargo.toml` is
`57d6794ad60b949e5bd8aa282f6f8c3dc97a365e`. The
[0.7 direct DOM documentation](https://dioxuslabs.com/learn/0.7/essentials/ui/escape/)
supports `document::eval` and typed `recv`; ordering below comes from the pin,
not a general promise about asynchronous eval or other renderers.

The pinned source establishes this chain:

| Pinned source | Relevant behavior |
| --- | --- |
| [`packages/interpreter/src/ts/core.ts:109`](https://github.com/DioxusLabs/dioxus/blob/57d6794ad60b949e5bd8aa282f6f8c3dc97a365e/packages/interpreter/src/ts/core.ts#L109) | Bubbling input is handled by the root listener. |
| [`packages/interpreter/src/ts/native.ts:44`](https://github.com/DioxusLabs/dioxus/blob/57d6794ad60b949e5bd8aa282f6f8c3dc97a365e/packages/interpreter/src/ts/native.ts#L44) | Native initialization sets `liveview = false`. |
| [`packages/interpreter/src/ts/native.ts:353`](https://github.com/DioxusLabs/dioxus/blob/57d6794ad60b949e5bd8aa282f6f8c3dc97a365e/packages/interpreter/src/ts/native.ts#L353) | `handleEvent` serializes the target and calls `sendSerializedEvent`. Desktop uses `handleVirtualdomEventSync`. |
| [`packages/interpreter/src/ts/native.ts:566`](https://github.com/DioxusLabs/dioxus/blob/57d6794ad60b949e5bd8aa282f6f8c3dc97a365e/packages/interpreter/src/ts/native.ts#L566) | Synchronous XHR (`open(..., false)`, line 574) sends to `/__events` and waits for the response. |
| [`packages/desktop/src/protocol.rs:62`](https://github.com/DioxusLabs/dioxus/blob/57d6794ad60b949e5bd8aa282f6f8c3dc97a365e/packages/desktop/src/protocol.rs#L62) | `/__events` routes to `WebviewEdits::handle_event`. |
| [`packages/desktop/src/webview.rs:43`](https://github.com/DioxusLabs/dioxus/blob/57d6794ad60b949e5bd8aa282f6f8c3dc97a365e/packages/desktop/src/webview.rs#L43) | The response is sent after `try_handle_event` and `handle_html_event`; the latter calls `runtime.handle_event` at line 191. |
| [`packages/core/src/runtime.rs:369`](https://github.com/DioxusLabs/dioxus/blob/57d6794ad60b949e5bd8aa282f6f8c3dc97a365e/packages/core/src/runtime.rs#L369) | Dispatch calls the listener synchronously (`listener.call`, line 460). |

Application handlers in `src/desktop/view/composer.rs` synchronously assign text,
capture the DraftBook and checkpoint. `state/drafts.rs::checkpoint_composers`
calls `drafts/integration.rs::route`, whose existing managed writer calls
`Editor::submit`. `submit` synchronously increments the serial and records the
pending value before spawning persistence. There is no await in this path.
Fixture routing intentionally returns early, so a native isolated fixture must
explicitly bind its isolated Editor rather than claim ordinary fixture routing
exercises this submit chain.

Consequently, a serialized predecessor input cannot remain queued in this
desktop path after JavaScript resumes from its dispatch. An input delivered
before native compare/apply has already changed Rust state/editor serial; an
input delivered afterwards serializes the then-current textarea value. Keep
the synchronous handlers. Do not add a global custom input bridge, encode
metadata into text, monkey-patch Dioxus serialization or await eval inside the
synchronous input handler. Revisit this proof if the pin, renderer, handlers or
submission path changes. It is not a FIFO claim about completion/eval messages.

`SerializedFormData` contains value/values/valid, not native visit, selection or
input epoch. Strengthen the existing handler's account/key check with captured
full `field_scope` equality. Native node identity belongs in the projection
protocol, not in an invented FormEvent field.

## Native identity and capture

Use a focused `view/composer/projection.rs` plus `projection.js`. A WeakMap owns
a unique visit token and counters for each actual textarea object. A ticket
retains that object; require `isConnected` and
`document.getElementById(field_id) === captured_field` before every mutation.
`field_scope` already includes account epoch, view identity, destination and
channel/thread generation, but cannot distinguish a same-scope node remount.

Capture bounded typed data:

```text
NativeSnapshot {
  field_id, field_scope, node_visit,
  input_revision, composition_revision, composing,
  wire_text, selection_start_utf16, selection_end_utf16, selection_direction
}
```

Install observational counter listeners at mount; do not cancel or redispatch
normal input. Require the references/completion controller to be ready before
offering projection. `composer_references.js` already owns reference parsing,
native undo and instance accessors. Read `field.value`, `field.selectionStart`
and `field.selectionEnd`: these expose canonical Slack wire text and UTF-16
wire offsets. The prototype getter exposes display labels and is unsuitable.
Reuse the Rust UTF-16/byte conversion helpers and preserve direction in the
native receipt even though core selection has only start/end.

Raw display-selection offsets are also captured as supplementary activity
guards: moving within a rendered mention can leave its wire offsets unchanged.
These offsets never become canonical selection authority. Proposed reference
labels come from the captured workspace and are installed through the existing
reference controller only after native comparison guards pass.

The reference capture listener updates its model before Dioxus's root listener.
Its instance setter intentionally suppresses intermediate input while performing
native replacement; preserve that implementation. Apply through the existing
instance setter and `setSelectionRange`, then verify actual wire/selection by
reading them back. Do not call the prototype setter or synthesize input to make
a replacement count as local authorship. Do not focus a different field.

The apply operation is one synchronous JavaScript turn with no await between
comparison and replacement. Compare the full native snapshot, field scope and
node identity, and reject disabled/read-only fields. A changed selection is
reason to recapture, not permission to move the user's caret. Any active or
intervening composition invalidates the proposal. Do not automatically retry
from `compositionend`: final input can occur on either side of that event.
Keep the field and barrier untouched and require a fresh quiescent capture or
explicit review; no timing heuristic is proof of OS IME completion.

The initial native capture is not automatically the authorized predecessor.
Its wire text must equal the barrier's expected predecessor text and the
guarded live/Book predecessor. If it is already ahead of Rust, return `Changed`
and preserve it before offering any replacement. Capture the actual native
selection as the comparison baseline; ordinary oninput's end-of-text Rust
selection is not evidence of native caret position. The replacement wire and
selection come from the exact proposed `after` draft, with checked conversion.

Return a bounded tagged receipt tied to one private proposal token:
`Applied { before, after }`, `Changed { current }`, `SelectionChanged`,
`Composing`, or `Unavailable`. Malformed, late, duplicate and timed-out receipts
never authorize clearing. Counter wrap/invalid values retire the visit.

## Editor authority and durable preservation

Add focused `drafts/checkpoint_projection.rs` and tests. Keep
`ProjectionBarrier { id, scope, serial, before, after }` immutable; never replace
its expected serial or revision merely to make an old receipt acceptable.
`after=None` is ineligible for automatic projection.

Use private types so callers cannot manufacture an acknowledgment from a bool:

```text
ProjectionTicket: exact barrier identity, scope, serial, durable review,
                  native visit/snapshot, expected live/Book pair
Submission: scope, writer_id, serial, exact LocalDraft
PreservedAlternate: submission plus verified durable evidence
AppliedProjection: validated native receipt bound to ProjectionTicket
RendererApplied: AppliedProjection plus guarded live/Book installation,
                 exact scope/visit, after LocalDraft and serial
```

Issue a ticket under the editor gate, then owner gate. Require exact live
barrier identity, unchanged serial equal to `barrier.serial`, no pending/run,
settlement, release or return handoff. Read transactional `LocalReview` and
require the exact `after` managed revision, paired local and semantic/full
state, `blocked=None`, and no alternates. Recheck editor state after the await.
Release gates before the native await; never hold a state mutex or Signal guard
across storage/native awaits.

**Storage revision alone is insufficient.**
`core/drafts/store/sync/alternates.rs::save_owned_local_alternate` can insert a
blocked/alternate version without changing `composer_draft_sync.storage_revision`.
The storage worker's `ReadLocalReview` also refuses pending legacy updates.
Ticket and acknowledgment must inspect this inventory, not only `View.stored`.

For `Changed`, capture exact wire text and selection into the captured owner's
`LocalDraft`, retaining verified attachment/reply-broadcast metadata. Reject
ambiguous changed metadata rather than copying another destination's state.
Preserve through the existing parked writer and retain the barrier:

1. Extract the synchronous enqueue helper used by `submit` so it can return a
   private `Submission` without changing ordinary submission behavior.
2. `preserve_projection_input(barrier_id, expected_serial, local)` runs retained;
   under the editor gate/short state lock validate barrier and serial, enqueue
   under the existing writer, then release the gate so the existing run executes.
3. Await run idleness, reacquire editor then owner gate, and call backend
   `read_local_review` directly. Do not recursively call `Owner::review_local`
   while already holding its gate.
4. Prove an alternate with exact scope, writer ID, revision equal to submitted
   serial, and `same_local` content. Recheck current serial/writer/pending after
   the await. Missing, retired or superseded evidence returns stale; I/O failure
   returns storage failure. Only this evidence returns `PreservedAlternate`.

`flush()` still returns the parked error after successful preservation, and a
successful `preserve_alternate` call may be an already-reviewed/older no-op.
Neither is a durability receipt. The receipt certifies the observed snapshot;
it cannot clear the barrier or settle another writer's version. A changed native
draft therefore enters explicit recovery/review, not an automatic refreshed
projection attempt.

## Apply, paired UI state and acknowledgment

After a validated `Applied` receipt, recheck exact account/epoch/field scope,
the receipt's current renderer/node visit, current Editor barrier identity and
captured serial, and the live composer and Book predecessor. Content equality
alone is insufficient: type then undo can restore equal live/Book content while
advancing the serial. Full field scope alone cannot detect a same-scope remount.
The renderer handoff must track the mounted visit and refuse installation if
that receipt's visit is no longer current or cannot be established; do not wait
until the later Editor acknowledgment to check these conditions.
Install only the captured destination's
projected live/Book pair while its barrier is still present. Use a focused clean
projection helper, not generic `DraftBook::capture` followed by local checkpoint
routing. Do not replace the whole book or lose unrelated dirty entries. This
provisional paired install is one synchronous UI turn. If input already changed
the pair, do not overwrite it; retain the barrier and preserve the newer input.
Only that guarded install constructs the private `RendererApplied` receipt.

Then strict acknowledgment reacquires editor followed by owner gate, repeats
the full transactional review and short state checks, and clears barrier/error
synchronously only for the exact ticket and `RendererApplied`. No await may
occur between the final state check and clear. Input after provisional UI install
but before acknowledgment remains parked and advances the serial, making the
acknowledgment stale. Clearing in a background task before installing the UI
pair would create a gap and is not the proposed order.

A failed/stale acknowledgment leaves the barrier and any already projected
native/live pair in place; do not blindly roll back the DOM. Retain the operation
through foreground cancellation. If cancellation interrupts UI work, the barrier
still prevents authoritative checkpointing and later input remains recoverable.
No late result may redirect itself to a new active account or destination.
Report success only after both the paired installation and strict acknowledgment.

The owner gate excludes changes through the same Account during final review
and clear. Independent SQLite writers can linearize after that read: this plan
does not claim memory clearing is atomic with another process. Subsequent
checkpoint/consume/release must keep their existing load/CAS/readiness guards.

## Implementation boundary and regression gates

Keep `integration::poll` metadata-only and retain its cheap unchanged signature.
Do not wire realtime observation, remote timers, enrollment, Send or Schedule.
Use the typed scheduled receipt pattern from
`view/scheduled/composer_clear.rs`, not the fire-and-forget reviewed replacement.

| Boundary | Required regression |
| --- | --- |
| Pinned input ordering | Native dispatch of an actual input immediately before compare/apply has already reached synchronous Rust handling; changed serial/native snapshot rejects old projection. Queue native dispatch before the apply task too. An input after apply/before ack persists as alternate and prevents clear. |
| Reference/selection | Channel and thread, Unicode and emoji UTF-16 offsets, backward selection, mention wire distinct from visible labels; other composer unchanged. |
| Native ahead of Rust | Change field without dispatching input; `Changed` read-back proves exact writer/serial/content durably preserved with canonical observation untouched. |
| Composition | Active/intervening composition leaves native content untouched; test final-input-before-end and end-before-final-input. Synthetic events do not certify OS IME behavior. |
| Visit | Same-ID/same-scope remount, channel A/B/A, thread reopen, account switch; no foreign mutation or barrier clear. |
| Storage | Same managed revision with a new alternate blocks ticket/ack; exact preserved row versus retired/older no-op; changed paired state; pending legacy worker update. |
| Lifecycle | Dropped foreground waiter, timeout, malformed/duplicate/late receipt, Applied followed by stale ack; no rollback or automatic rebase. |
| Restart | Separate isolated process reads canonical observation and committed alternate; old DOM/barrier is not claimed to survive restart. |

Preservation tests must distinguish another writer with identical content, an
equal/older serial no-op with different content, a newer submission during
read-back, and a read failure after a committed save. None supplies the captured
tuple's receipt. A retry may prove that exact tuple. `after=None` and even a
metadata-only revision advance invalidate an outstanding ticket; duplicate ack
cannot clear another barrier.

Add normal/narrow named fixtures with explicit isolated Account/Editor bindings,
staged channel/thread captures, bounded acknowledgment files and a fresh private
SQLite directory. A narrow fixture-only branch before `integration::route`'s
fixture early return may route changes solely to preinstalled isolated writers
for the two exact fixture names. Require explicit check opt-in, the injected
absolute private database path, known fixture account and no session/transport.
Never fall through to `Account::production`, create an owner implicitly or route
unrecognized fixture destinations. This preserves the actual synchronous
oninput-to-submit chain under test without replacing oninput.

Reuse the portable capture runner where possible. Assert full
canonical channel rows, not viewport `shell.messages`; assert no send/session/
transport construction. Inspect every PNG after native execution. Remove only
runner-owned temporary storage after its process exits.

The smallest expected file split is:

| Files | Scope |
| --- | --- |
| `drafts/checkpoint_projection.rs`, focused tests, `checkpoint.rs` | Private authority, enqueue receipt and retained preservation/ack operations. |
| `view/composer/projection.rs`, `projection.js`, focused JS tests | Native capture, node visits, comparison and typed receipts. |
| `view/composer.rs` | Full captured scope guard and a small mount hook; preserve direct input behavior. |
| A focused clean DraftBook helper and tests if needed | Install only the exact paired key without marking remote projection as a local edit. |
| `drafts/projection_check.rs`, `.js`, fixture support and portable runner | Explicit isolated writer binding, native ordering proof, normal/narrow capture and restart checks. |
| Main/boot/named-fixture wiring | Start only the explicit disconnected verification mode; root coordinates these shared files. |

Completed gates include focused checkpoint/storage and JavaScript regressions,
locked desktop tests and formatting, independently locked core tests, and
normal/narrow native receipts and restart captures. Owned processes exited and
pointer restoration was observed; geometry/ownership checks are sampled rather
than continuous. Private fixture databases are retained as audit evidence.
Production ingress, enrollment, write scheduling, Send/Schedule integration and
live cross-client conflict behavior remain separate required work.
