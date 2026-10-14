# Draft observation integration: implementation and remaining boundaries

The Editor observation handoff and native projection APIs are implemented and
have disconnected Windows verification; see [draft-native-projection.md](draft-native-projection.md)
and the parity worklog. The original editor plan below is retained as its design
contract, not an unimplemented backlog. **Production realtime observation and
ordinary remote composer autosave remain disabled.** No remote write timer is
authorized merely by completion of the observation/projection APIs.

## Current prerequisite implementation, 2026-10-14

`integration::route` checkpoints existing managed editors or delegates to the
legacy local writer; it does not enroll ordinary composers or schedule network
dispatch. Realtime draft frames still do not enter the captured observation owner
pipeline. Existing explicit conflict review still rejects pending operations.
The new explicit recovery path checks a frozen uncertain Send or Update; it does
not enter conflict review or enroll ordinary composers.

The following recovery source changes pass independently locked core tests
(1,368), desktop tests (1,401), formatting and Windows dev builds. The latest
recovery binary is `151e9a95`. Native `ur-final3/` passes all six normal pairs
(26 inspected PNGs) and four narrow pairs (18 inspected PNGs). Narrow cooldown
fails because duplicate toast feedback covers the inline status; all three
failure PNGs were inspected. Narrow pending and cooldown restart remain unrun
on this binary. See the parity worklog for exact evidence and retained failures.

The pure `sync::owned_index` module is now compiled: independently locked core
tests pass 1,384 / 5 ignored, including sixteen new routing tests. It indexes
only durable acknowledged IDs and pending Create UUIDs, with historical identities
as collision barriers and full-row/instance revalidation. Host invalidation on
account visits, release, re-enrollment and inventory replacement remains required.
This helper has no production ingress hook; desktop verification is pending.

1. Raw server denial envelopes are separated from trusted transport failures in
   `drafts/network.rs`. A raw server error named
   `draft_mutation_not_sent_offline` cannot reach the trusted predispatch
   sentinel classifier. Clean known
   denials may release ownership; contradictory mutation evidence, malformed
   acknowledgments and unexpected outcome fields must retain raw uncertainty.
2. `network/update_readback.rs` uses a single `drafts.info` POST form read of the
   acknowledged server draft ID. Positive confirmation requires exact captured
   account/session, ID, client UUID, destination, supported lifecycle, frozen
   payload and a strictly newer version. `recovery/ui.rs` preserves current typing
   and checks the exact optional editor identity at projection. Update confirmation
   never releases its editor, clears its composer or uses Send delivery cleanup.
3. Format 5 persists a full-operation check ticket before requesting data. An
   unresolved ticket remains blocked indefinitely across restart. Completion,
   monotonic cooldown and any positive receipt commit in one SQLite transaction.
   Failed receipt storage retains a process-local completion for an explicit
   storage-only retry; after restart, the unresolved durable ticket remains
   blocked. There is no automatic request replay or continuous polling.

Required regression cases include the server-spoofed offline sentinel, clean
versus contradictory denial, wrong identity/content/version, stale callbacks,
typing during readback, restart-preserved uncertainty/cooldown and failed storage.
Uncertain Create has no acknowledged ID: do not invent a UUID lookup API.
Missing info/list results do not confirm Delete. These prerequisites alone do
not establish ordinary autosave or live synchronization parity.

The strict decoder deliberately refuses unsupported extra draft fields, even
when their values are empty or zero. Production `Transport::execute` preserves
positive raw responses but converts negative read envelopes into API errors;
those errors never confirm an Update. The isolated native fixture covers actual
prepared POST forms and shared persistence/UI code with synthetic responses; it
does not prove live Slack payload compatibility or make a live request. Existing
frozen Reading/Escape captures do not verify this new draft cohort.

## Existing boundaries

- `src/desktop/drafts/observation.rs` captures a `Source` for an accepted socket
  generation. Retain it before awaiting frames; do not recapture current
  credentials around a frame that was already queued. `Source::prepare` checks
  the captured account, epoch, transport, credentials, authenticated workspace
  user and socket generation against the current shell.
- Its conservative decoder produces a private `Prepared` observation. It does
  not locate or enroll a destination owner. Unverified lifecycle/content fields
  on a matched draft remain raw unsupported evidence rather than a narrowed
  active draft. This decoder is not proof of complete live payload coverage.
- `Owner::observe_event` rechecks current durable ownership under the account/key
  gate and commits the observation with its paired local value. It returns
  `Observed { before, after, changed }` after persistence. It does not rebase an
  editor or mutate a composer.
- A delete without a client UUID resolves only from the exact scoped,
  acknowledged server draft ID under that gate. Pending creates and the visible
  destination cannot supply the UUID. Already recorded duplicate/out-of-order
  retirements can be recognized as inert without deriving fresh identity.
- `crates/super-platinum-core/src/drafts/sync/observation.rs` orders events per
  owned server identity. Event timestamps require six fractional digits and are
  compared only with event timestamps; payload revisions use their own ordering.
  Contradictory equal-clock events and unsupported owned evidence block writes.
  A delete event never confirms a Send.
- `crates/super-platinum-core/src/drafts/store/sync.rs` writes format 5 and reads
  formats 1 through 4. Event evidence, ordering, read-check tickets and local
  content share the existing SQLite transaction. Older binaries reject format 5
  rather than discard guards.
- `src/desktop/drafts/checkpoint.rs` already serializes local checkpoint,
  consumption, release and explicit settlement. A submitted edit can arrive
  while a storage await is in progress. `rebased` accepts only a changed storage
  revision whose paired local content and semantic revision are unchanged.
- `src/desktop/drafts/integration.rs::poll` updates recovery metadata. It
  deliberately does not replace the DraftBook or visible composer.

## Implemented editor slice: retained design contract

Add `Editor::observe_event` in a focused
`src/desktop/drafts/checkpoint_observation.rs` module. Keep realtime ingress and
native DOM replacement out of this slice. Test it with isolated account storage
and deterministic injected observation waits.

### Required invariants

1. A remote observation never silently rebases input that was composed against
   another local value.
2. Input submitted before observation is durably checkpointed first. Input
   submitted during or after an observation handoff remains recoverable.
3. Updating a storage revision does not prove the native field has accepted the
   new remote content.
4. Persisted remote content and competing local input remain separately owned;
   no merge or overwrite is inferred from matching destination alone.
5. The operation survives cancellation of its foreground waiter. Its result
   belongs to the captured account/key, not a later active visit.
6. No network request, enrollment, automatic retry or send is introduced.

### Algorithm

1. Start a retained runtime task, following `Editor::consume` and
   `Editor::settle`, so dropping a component waiter cannot interrupt handoff.
2. Flush earlier submitted input, acquire `Editor::gate`, then recheck whether a
   run began between flush and acquisition. If so, release the gate and retry
   the local serialization step. Never await a run while holding its gate.
3. Reject released ownership, an existing settlement or an unresolved
   observation handoff. Under the short state mutex, capture the editor serial,
   authoritative base, semantic local revision and writer identity. Mark the
   observation active for polling. Do not hold that mutex across storage awaits.
4. Call `Owner::observe_event` while retaining the editor gate. `submit` may
   replace the pending value and increment its serial, but its run task waits
   on the same gate.
5. Compare the returned `before` view with the captured editor base. Require the
   paired local value and semantic local revision to agree. Storage revision
   alone is insufficient: another owner may have committed during the wait.
6. Complete one of these handoffs before releasing the gate:

   | Result | Editor treatment |
   | --- | --- |
   | Metadata-only change with matching predecessor | Advance storage metadata; preserve pending input and serial so it can checkpoint against unchanged content. |
   | Paired local content/revision changed, or predecessor mismatch | Install a projection barrier. Preserve the durable result as metadata, but park pending and subsequent input as writer-owned alternates. |
   | Remote conflict with unchanged local value | Retain both versions and advance metadata. Local input may continue; the reducer conflict still prevents consumption and dispatch. |
   | Storage failure or uncertain handoff | Retain pending input and park authoritative writes. Do not assume the older editor base remains authoritative. |

7. Notify waiters after finishing the handoff. A returned projection proposal is
   not authorization to clear the barrier.

The projection barrier should contain an immutable proposal identity, exact
owner/key, local predecessor, resulting storage revision and captured editor
serial. Polls, reloads and later observations must not clear it implicitly.
Subsequent input remains a durable alternate until an explicit projection receipt
or reviewed conflict decision resolves the handoff. Across restart, canonical
storage and committed alternatives remain authoritative; the previous DOM no
longer exists.

Checking only whether the serial changed during SQLite I/O is insufficient.
Even with no submission during the await, the DOM still displays the old draft
until projection. The user's next keystroke must not overwrite the newly
installed remote base.

Do not copy `checkpoint_settlement.rs`'s successful reset indiscriminately.
Explicit settlement authorizes a reviewed version choice. An unsolicited remote
event does not.

### Files and regression checks

- Add `checkpoint_observation.rs` and `checkpoint_observation_tests.rs`.
- Wire the module from `checkpoint.rs`; add only the needed state/snapshot
  fields and observation activity/proposal identity in `checkpoint_poll.rs`.
- Reuse or extract `Editor::run`'s writer-owned alternate preservation rather
  than duplicating its persistence loop.
- Keep `integration::poll` metadata-only. Preserve the existing cheap unchanged
  signature comparison.

Use channel/barrier-controlled futures rather than sleeps to verify:

- Input pending before observation commits first; a differing remote value
  becomes a conflict rather than replacing it.
- Input during the storage await remains pending and then durable as an
  alternate, with remote canonical content intact.
- Input after observation returns but before projection acknowledgment is also
  parked. Test this separately from the in-flight submission race.
- Multiple submissions retain the latest value for that writer without
  replacing another writer's version.
- Metadata-only observations permit later normal checkpointing without a false
  content conflict.
- A different owner changing the paired predecessor prevents silent rebasing.
- Foreground cancellation/account navigation neither cancels the durable
  transition nor redirects its result.
- Storage failure retains input and blocks subsequent authoritative writes.
- Consume, release and explicit settlement cannot cross an unresolved handoff.
- Restart preserves canonical remote content and committed local alternatives.
- Polling emits barrier changes and does not repeatedly clone unchanged payloads.

Run the locked desktop suite and formatting checks for the editor change. If the
implementation changes core persistence/reducers, also run the independently
locked core suite. Report actual results; the listed checks are requirements,
not evidence that the planned slice has passed.

## Later gate: native projection receipt

Implement this separately before connecting unsolicited events to visible
composers. A proposal needs exact field scope/visit, unchanged native wire text,
selection and composition checks, followed by a typed receipt. Rust signal
equality alone cannot establish that native input is unchanged.

If the native field contains newer text, recapture that text into the editor's
preservation path. Active composition leaves the field untouched and postpones
replacement. Only a matching successful receipt, unchanged editor serial and
unchanged storage revision may authorize clearing the projection barrier.
`view/scheduled/composer_clear.rs` demonstrates a typed native receipt boundary;
the fire-and-forget reviewed-field replacement is not sufficient authorization
for unsolicited remote content.

Add normal/narrow isolated fixtures covering composition, a native edit ahead of
Rust state, navigation/remount, channel/thread separation and restart. Inspect
every captured PNG. Synthetic composition does not establish actual OS IME
behavior.

## Later gate: realtime ingress and reconciliation

Keep the captured `Source` with its accepted socket generation. Resolve only
existing ownership through an account-scoped server-ID/client-UUID index, then
route to the owning editor or the appropriate offscreen persistence path. Do
not depend on `active_team` or infer ownership from a current composer.

An observation deferred because alternatives or a projection handoff block the
editor must not be silently considered processed. Retain its ordering for
reconciliation or mark reconciliation necessary. Do not coalesce away a
delete-before-create sequence. Define bounded ingress overflow behavior before
enabling the bridge.

Production autosave remains a further gate: shared managed Send/Schedule
ownership, ordinary mutation/readback coverage, persisted cooldowns, explicit
remote-conflict recovery and supported-to-unsupported edit handling are still
required. `client_last_updated_ts` is not a server compare-and-swap guarantee.


## Ordinary positive readback contract recovered (2026-10-14)

Read-only inspection of the retained primary bundles identified a direct
ordinary draft lookup. Bundle aliases/hashes are the same as the source table in
[slack-external-select-protocol.md](slack-external-select-protocol.md); offsets
count decoded JavaScript characters.

- B `UYdH` module at 6816000 invokes `s.oV({draftId:v,
  reason:'resolve_draft_conflict'})` at 6818245 and consumes `response.draft`.
- V `zZt+` at 9756496, with binding at 9756598, resolves that call to
  `drafts.info`.
- S `uGgu` at 1594178 removes instrumentation fields including `reason`, abort
  and timeout before S `LYxw.VA` at 738282 converts remaining argument keys to
  snake case. The proven request field is `draft_id`; reason is not sent.
- B `h/H5` starts at 6792414; active payload fetch at 6794008 uses
  `drafts.list`, default `is_active:true`, limit 25 and optional
  `last_updated_ts`/`next_ts`. Recursion uses the last draft's update timestamp
  and caps payload loading at 200. Its separate active-ID loop at 6794981 uses a
  wrapper absent from the retained bundles, so its endpoint/completeness are
  not established.
- S `Bxxi` at 3724536 handles `is_from_composer` and `client_msg_id` in unified
  ordinary/channel draft flows. Earlier worklog language that active listing
  excluded unscheduled drafts is not an established API contract; the scheduled
  UI's filtering does not prove that exclusion.

Only positive exact-identity readback is supported by this evidence. Missing
responses, partial listings and the 200-item cap cannot prove deletion. The
existing conservative decoder treats `is_from_composer` as unknown; its exact
shape still requires review before broadening support. No production ingress,
new storage format or remote writes were enabled by this audit. The next core
request/decoder implementation and reconciliation lifecycle require their own
tests and ownership review. Update timestamps remain metadata, not server CAS.
