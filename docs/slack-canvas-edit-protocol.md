# Canvas checklist editing: captured protocol and remaining work

## Evidence boundary

This records source inspection, stock Slack reference captures, and guarded
native API and desktop UI round trips on a task-owned, private, unshared canvas.
The corrected core API toggle and restore passed independent canonical readback,
with target content/parents unchanged and every other section byte-identical.
Actual native desktop clicks subsequently changed the pair to checked/checked
and restored unchecked/checked, verified in stock Slack and by an independent
authenticated canonical/export read. The restored native screenshot was inspected.
The first UI toggle needed manual Refresh after its own realtime event invalidated
the read; the restore reconciled automatically. The bounded post-ACK replacement
read subsequently passed its regression and live gates, recorded below. A later
paragraph-editor pass exposed a separate event arriving after receipt settlement;
that additional visible-reader recovery is tracked in the worklog.

Signed fixtures at 1280×800 and 668×400 passed duplicate-action blocking, toggle
and restore; normal/narrow and saving, uncertain, read-only and rename screenshots
were inspected. These fixture states do not prove live lost-response recovery.
The initial pre-retention API restore was uncertain and was not replayed; stock
Slack restored it before the retention correction was tested. The bounded loader,
title, CRDT retention, transport and ownership contracts are described below.

On 2026-10-13, one existing unchecked item was checked and then restored in
stock Slack. Both writes returned HTTP 200 protobuf acknowledgements; subsequent
fetch-latest responses confirmed the changed section. Independent authenticated
HTML readback confirmed the original two-item state was restored: unchecked,
checked. The capture stopped normally after 11 requests. No document content,
identifiers, credentials, or raw capture payloads belong in this document.

Earlier private probes established that the public `canvases.edit` method rejects
the current desktop token. The HTML export is a read representation, not a write
representation. The unused `ToggleSectionCheckedState` protobuf descriptor is
not a demonstrated alternative endpoint: the captured editor uses collaboration.

## Source references

Offsets are character positions in decoded minified sources, not byte positions
or stable upstream APIs:

- **B**: `gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`.
- **Q**: `gantry-v2-async-quip-collab-webapp.7f6a8bd5f9d5a08b.min.js`.
- **E**: `gantry-v2-async-js_quip-src_collab_controller_editor_editor_tsx.ad4bc845342a0129.min.js`.
- **V**: `gantry-v2-async-gantry-v2-vendors-async.daa946d0953564d3.min.js`.

| Contract | Source |
| --- | --- |
| Controller initialization | B 5288964; V `IGvv` 9793251 |
| Canvas host, form encoding and authentication | B 5286246 |
| Edit HTTP transport | B 5287539 |
| Load-data / fetch-latest binary transport | Q 195535–196584 |
| Checklist mutation / section attribute dirty tracking | Q 3168204 / 1698531 |
| Partial update bundle / original bundle | Q 1471296 / 1295643 |
| Save envelope and acknowledgement decoder | E 257858 / 258317 |
| Editor save delegate | E 564708 |
| Session identity and save sequence | Q `vy7t` 675636 |
| CRDT state serialization | E 272390; Q 1682053 |
| Existing-section local CRDT initialization | E 287010 (`startClientSession_`) |
| Actor identity / default and iframe window sessions | E 280310 / 564426; Q 675938 / 1004612 |
| RTML text and marks / text escaping | Q `GoiO` 937882 / `YLF4` 657793 |
| End-of-text sentinel | Q 939293 (`addEndSpace`) |
| Local session reset and serialization | E 269645–272900 |
| JavaScript default Automerge commit timestamp | Q 3610895 |

The editor chunk is loaded separately from the Quip webapp chunk. Inspecting only
the latter misses the actual save-envelope constructor.

## Initialization and transport

The controller calls `quip.getControllerInitData` with empty application args and
reason `canvas-controller-init`. Its response supplies initialization options,
preserialized data, initializer, Quip user identity, cache metadata and body
classes. Those fields can contain private session data and must not be logged.

Canvas calls target `https://<workspace-host>/canvas`, with `_x_version_ts` in the
URL. Bodies are `application/x-www-form-urlencoded`; encoding applies
`encodeURIComponent(String(value))` to each supplied value. Cookies accompany the
request. Captured source uses Bearer authorization for same-origin calls and a
form token for cross-origin calls. Implementations must retain the existing
workspace credential boundary rather than accepting an arbitrary canvas host.

`/-/load-data` (including `/editor/<counter>` suffixes) and `/-/fetch-latest` send
`request_binary`, a base64-encoded protobuf. They return binary protobuf, not the
ordinary Slack API JSON `ok` envelope. The existing generic JSON executor cannot
decode these responses. Capture tooling must preserve CDP's `body` together with
`base64Encoded`; replacing binary content with a placeholder loses the evidence.

## Canonical loader and completeness boundary

The native read path is `api::fetch_canvas_collab` in
`crates/super-platinum-core/src/slack/api/canvas_collab.rs`:

1. Fetch `files.info` and require the requested Canvas file identity/type. Take
   the collaboration thread ID from `quip_thread_id`; a Slack file ID is not a
   collaboration thread or document ID.
2. Fetch controller initialization. Decode its protobuf options and bind the
   Quip user, Slack user and team identities to the authenticated workspace.
3. Send a load-data request containing the thread object plus a `THREAD` bulk
   load. Use the authenticated workspace host and the exact-path binary read
   executor; it neither holds an offline request for later dispatch nor adds
   application-level retries. Initial `files.info` uses the existing normal API
   transport; callers still discard results from obsolete visits or accounts.
4. Decode and validate the complete result before exposing a canonical snapshot.

`CanvasCollabSnapshot::from_responses` requires a successful load with no error or
follow-up/incomplete markers, exactly one matching Thread and Document, matching
thread/document relationships, and present canonical sequences. Every returned
section must identify that same thread/document and have a sequence. Exactly one
`Section/<documentID>` index must exist, with unique entries accounting for every
returned non-deleted section and no missing or extra active section. The observed
complete index has `chunked=true`; that flag alone is not rejected. Nonempty
preserialized response payloads are currently unsupported rather than silently
accepted as a complete document. Unknown protobuf fields remain in the snapshot.
A valid snapshot establishes identity and completeness, **not edit permission**;
file, thread, document and section eligibility remain separate builder checks.

A read-only probe on the task-owned private canvas succeeded through this path
and matched the file/thread identities. It returned six sections, including both
checklist items. Neither checklist section initially carried Section field 38
(`CrdtSessionState`). The then-current server-session-only intent correctly
refused them. This explains why copying a CRDT field from initial load cannot be
the sole editing strategy; it does not authorize fabricating server state.
The structural receipt is `tmp/canvas-collab-live-read.log` (local, uncommitted).
No payload or credential needs to be copied into this document.

## Exact edit envelope

`POST /canvas/-/edit-document` uses these form fields:

| Field | Captured constructor |
| --- | --- |
| `thread` | Canonical editor thread ID |
| `document` | Canonical editor document ID |
| `retry_count` | Editor's consecutive-save-error count |
| `secret_path` | Thread's canonical secret path |
| `session` | Client session identity |
| `sequence` | Client session save sequence |
| `nav_action_id` | Navigation action identity |
| `search_session_id` | Search session identity |
| `data_binary` | Base64 protobuf `syncer.DocumentData` partial update bundle |
| `title` | Included when the title argument is non-null |
| `activity_log` | Set to false only when every updated section is programmatic |

The stock desktop delegate ignores the higher-level `originalBundle` argument;
it is **not sent** in this HTTP envelope. Do not invent an `original_bundle`
parameter. The captured writes included title and title rich-text metadata;
omitting those for a checkbox-only native operation remains to be verified.

Three sequence domains must stay separate:

1. Form `session` identifies the client session. Form `sequence` starts at zero
   in that session and increments before save dispatch. It is not a server
   revision or a demonstrated idempotency key.
2. `Section.sequence` is the canonical server sequence. In the captured round
   trip the edited section advanced from 10000 to 12000, then 13000.
3. `Section.local_sequence` increments when building a local update bundle;
   the two captured writes used 1 and 2. CRDT base/last-saved sequences are
   additional canonical synchronization fields, not substitutes for either.

## Binary schema and partial semantics

Loaded module `Sxm7` exports `Er(bytes, Type)` for protobuf decoding,
`zf(base64, Type)` for base64 protobuf decoding, `aB(message)` for named-field
JSON, and `Ax(message)` for base64 protobuf encoding. Module `p+Jj` exports:

| Export | Type |
| --- | --- |
| `t4.Request` / `t4.ResponseWrapper` | Load-data request / response |
| `IP.Request` / `IP.Response` | Fetch-latest request / response |
| `EL` | `syncer.DocumentData` |
| `WQ` | `syncer.Section` |
| `G7` | `syncer.EditDocumentResponseWrapper` |

`DocumentData.sections` is repeated field 1; document ID is field 15. The
captured requests also include title rich-text field 14. A section update contains
only selected fields; absent fields must not erase unrelated canonical content.

Relevant Section tags: ID 1, canonical sequence 2, type 9, parents 13, status 15,
attributes 16, modified 19, local sequence 20, trace ID 33, read-only 35, CRDT
session 38, activity-stack update 39. `Attributes.checked` is field 2. Full
canonical Section snapshots additionally include document/thread identity and
content. Unknown fields must survive any representation reused for writes.

The checkbox caller checks document and section editability and skips locked
sections. Changing the checked attribute marks status and attributes dirty;
bundle construction then includes required identity/sequence/parent/synchronization
fields. The captured writes each contained one partial section; neither sent
section text content or status field 15. That observed status omission is not
evidence that all checkbox writes can omit status. Attributes retained indentation,
vertical margin and vertical alignment alongside checked; they were not replaced
with a checked-only object. Their activity-stack values were 2 and 3 respectively.
Do not generalize that one-section shape to arbitrary nested lists or selection
ranges: the stock toggle can operate on multiple selected sections.

## CRDT requirements

Both captured checkbox requests carried a `CrdtSessionState`, despite changing
only an attribute. It contained section ID, base sequence, last-saved sequence,
Automerge document bytes, processed heads, and base head. Acknowledgements updated
last-saved sequence and supplied an expiry field. Automerge bytes, processed heads
and base head were byte-identical across both requests and both acknowledgements.
That proves unchanged text CRDT state for this observed attribute-only round trip;
it does not establish that arbitrary cached sessions can be reused.

The source creates a **new** CRDT wire message from its live session. It writes
base sequence, last-saved sequence (or base rounded down to a multiple of 1000),
`Automerge.save(localDocument)`, processed heads and base head. Merge, split and
sequence-head metadata are included when present. It does not simply clone an
acknowledgement: expiry and server-validation fields are not copied into the next
request. Section serialization can add current save heads when there are pending
CRDT changes. Session compatibility, expiry, locks and remote rebases therefore
remain relevant even for a checkbox-only implementation.

Parent metadata also differs between requests and acknowledgements. The captured
request parents protobuf was 39 bytes; the canonical reply's was 76 bytes. Source
Q 1682053 reconstructs request parents from current ancestor IDs and their
container style rather than copying the entire canonical parent message. Preserve
the distinction between canonical read state and the supported write projection.

### Existing sections without a CRDT session

Captured `startClientSession_` initializes a local session when needed. For an
existing clean section, it parses canonical RTML into text plus marks, makes one
Automerge change setting `text` and `deleted`, and applies the parsed marks. The
text is an **escaped RTML representation**, not merely visible text: `GoiO`
passes text leaves through `YLF4.escapeText`, while line breaks and controls have
explicit encoded representations and formatting is represented by marks. The
`addEndSpace` helper appends U+FEFF, including when the input already ends in that
character. Flattening exported HTML would lose these distinctions.

The actor comes from the editor delegate's window session ID. The base delegate
calls `generateWindowSessionId()` without a window argument, producing the
12-hex client session followed by eight zeros: `session + "00000000"`. The stock
iframe editor overrides that default with `generateWindowSessionId(frameName)`:
its suffix is the frame-name hash in hexadecimal, padded to eight characters.
The native single-editor implementation uses the source-defined base default;
it does not claim to reproduce the stock iframe's suffix. Repeated bootstrap
uses the same actor from the owning session; the source does not add a new
per-visit or per-bootstrap nonce.

For an existing clean section, the single initial head is already processed, the
snapshot queue is empty, the base head is that initial head, and base sequence is
the canonical section sequence. Local reset leaves last-saved sequence unset;
serialization falls back to base sequence rounded down to a multiple of 1000. It
serializes the real saved Automerge document rather than a made-up byte envelope.
An exact cross-runtime head comparison must also control commit metadata: the
first captured-JavaScript comparison differed because JavaScript used current
Unix seconds for the commit while Rust's default was zero. That mismatch is not
by itself evidence of different text or actor encoding. The preparation clock
must supply explicit commit seconds and the oracle must use that same timestamp
before an exact-head result can be interpreted.

A newly initialized local session has **no server expiry**. The source's local
expiration manager starts/reset with no expiration date, and `toPb` does not
manufacture the acknowledgement's expiry or validation fields. This is a local
provenance distinction, not permission to ignore an expired or malformed server
session. A present server CRDT field must continue through the server-session
validator; it must not fall back to local bootstrap to bypass its rejection.

The implementation in `canvas/edit/bootstrap.rs` deliberately supports a smaller
subset than the stock parser: an existing ordinary, non-deleted section with
canonical plain, unmarked Unicode RTML of at most 64 KiB. Tags, entities, control
characters, rich/unknown content shapes, and an already-present CRDT session are
rejected. For this subset, escaped RTML text equals the original canonical text;
there is no HTML stripping or lossy conversion. The builder creates genuine
Automerge text plus U+FEFF and the deleted flag in one change, retains the original
section unchanged, and binds the result to the exact section snapshot and client
session. Local provenance cannot be supplied by decoding an arbitrary wire
message. The checkbox partial update still omits text replacement.

Local interoperability is now verified for six synthetic bootstrap cases,
including CJK, emoji, combining text, RTL, NBSP and FEFF. The independently locked
core suite passed **834 tests, with 2 ignored**, including the exact matched-clock
head comparison against captured-JavaScript fixtures. The explicit Rust export
also passed. In the reverse direction, an isolated harness using 16 captured
library modules and the actual `092942ae1cb66d62` Automerge WASM loaded all six
Rust exports. All six matched text plus U+FEFF, deleted state, the entire decoded
change (including timestamp and hash), and JavaScript/native heads exactly.
The final local receipt is `tmp/canvas-automerge-roundtrip.json`.

The harness loaded libraries only: it ran no Slack application entry point,
network request or real document content. This is two-way interoperability proof
for the supported plain, unmarked subset with matched actor and commit metadata.
It does not extend support to markup, entities or controls or establish arbitrary
CRDT-session reuse. This synthetic proof is separate from the corrected private
native API round trip recorded below.

## Title metadata construction

The earlier requirement for canonical Document field 69 was incorrect and has
been replaced with title-section synthesis in `canvas/edit/title.rs`. Stock `save` calls `extractTitleMetadata(true)` (Q 1113267 / 1297155),
converts the non-deleted static title section with `IRZY`, and adds the resulting
RichTextApi value to DocumentData field 14 (Q 1295618). It does not copy Document
field 69.

The canonical static title is Section type 64 / style 48. Its authoritative text
is Content field 58 (`ContentTitle`), field 1, rather than Content field 1. In both
captured requests, field 14 decodes to `rich_text` → `rich_text_section` → `text`,
whose leaf text equals the canonical ContentTitle text. The root and section
messages use field 1 for type and repeated field 2 for children; the leaf uses
field 1 for type and field 2 for text. Comparison inspected structure and equality
without printing private title content.

The stock converter changes NBSP to space, removes U+200B/U+FEFF, normalizes
TAB/LF/CR to spaces and strips other C0/C1 controls. Its text-mode escaping does
not encode ampersands or angle brackets after RTML parsing. Rich output is bounded
to 2000 JavaScript UTF-16 units. The separate form title is DOM-extracted, trimmed
and bounded to 255 UTF-16 units; these two fields can intentionally differ. Empty
titles use localized Untitled text. Native synthesis handles nonempty plain
canonical titles, the observed normalization, and UTF-16 limits. Markup/entities,
localized empty-title fallback and a cutoff splitting a surrogate remain
unsupported. It validates a unique canonical title section and binds its exact
source to the intent and fresh preflight. Both generated title fields matched the
captured stock requests; detached Chromium checks cover normalization. Document
field 69 is neither required nor copied.

## Retained CRDT state and explicit client reset

A new local bootstrap is not interchangeable with an existing actor's accepted
history. An isolated captured-JavaScript reproduction proved that independently
initializing the same text under the same actor at different commit times yields
different sequence-one changes that cannot merge. This explains a concrete local
compatibility defect, but does not establish the exact server cause of the first
native restore's uncertain outcome.

`EditSession::remember_accepted(&AcceptedCheckbox)` adopts only a sealed receipt
for its own HTTP session, reserved form sequence and full workspace/user/file/
thread/document/section key. A successful prepare retains the initial genuine
local seed too, so a new explicit attempt after a known-not-sent result reuses
that history rather than bootstrapping the same actor again. Desktop ownership
still prevents replay of a potentially sent or uncertain operation.

The bounded attribute-only reducer accepts the server's unchanged Automerge
bytes, base sequence, processed heads and base head, together with its canonical
last-saved sequence and expiry. Unsupported session fields, absent CRDT metadata,
or unsupported head changes make the retained history unusable; they do not
invalidate an otherwise independently proven accepted checkbox change. A true
`server_validation_failed` field rejects sealed acceptance itself, regardless of
the returned checked boolean.

When a fresh load omits Section38, reuse requires the exact accepted canonical
section and parent association. The comparison removes only Section38 and the
observed ACK-only fields18,103,104. Both captured ACK/fetch pairs differed in those
three response fields; all other raw fields and canonical revisions are retained
and compared, including unknown fields. No more general full-load omission policy
is assumed. A retained server expiry is checked during preparation and remains
private provenance in `PreparedCheckboxEdit`, so final API validation checks it
again even though outgoing CRDT4 and fresh load CRDT38 are absent.

Retention is limited to 64 section identities and 8 MiB per client session. A
**new, already-fresh explicit intent** may reset the whole HTTP session and actor
when absent-38 state cannot reuse retained history because of drift, expiry,
unsupported acknowledgement metadata or capacity. This is a native conservative
new-client policy: source `vy7t` creates a new session identity and zero save
counter locally, and `startClientSession_` initializes its actor from canonical
state without server registration. It does not claim to reproduce stock's
same-window expiry reducer. It never evicts a history and silently reuses its old
actor. A present expired server CRDT field still rejects preparation.

Intent-versus-fresh validation runs before a reset is planned. The new identity,
cache and counters commit only after preparation succeeds. Old prepared requests
and sealed receipts retain their original identities; they may settle their
original UI operation, but a late old acknowledgement cannot replace the new
session's retained state. No uncertain request is automatically retried.

## Acknowledgement and failure reconciliation

`EditDocumentResponseWrapper` fields are changed 1, final data 2, cell data 3,
import sections 4, mount points 5, and is-new-edit-session 6. The editor decodes
mount-point updates before invoking its save callback. On successful changed
responses it applies returned canonical sections through its model update path;
it does not replace the whole document with the partial update.

Both observed acknowledgements had `changed=true` and one final section with the
requested checked state. Fetch-latest independently confirmed each update. This
does not prove no-op, permission-revocation, concurrent-write, rate-limit,
malformed-response or lost-response behavior. Source distinguishes HTTP 400,
403, 409, 413, 429 and 503; those classifications alone do not justify copying
the full stock retry controller into a narrower implementation.

The authenticated HTML export lagged the first accepted edit: an immediate export
still showed the original state, while a later export caught up. An unchanged
HTML export immediately after dispatch is therefore not definitive write failure.
Use the canonical protobuf acknowledgement/readback to reconcile a pending edit.

A native first slice should use one attempt per explicit toggle, distinguish
known-offline pre-dispatch failure from an uncertain post-dispatch outcome, and
reconcile canonical section identity/sequence/state before offering another
write. No captured evidence establishes an idempotency key. Concurrent changes
and successful writes followed by later opposite changes need explicit handling;
matching the requested boolean alone is not an immutable receipt.

## Implementation status and remaining proof

The current source contains these boundaries:

- `canvas/wire`: bounded protobuf decoding with unknown-field preservation and
  typed section/load/acknowledgement accessors.
- `slack/api/canvas_collab.rs`: account-bound initialization and complete canonical
  loading, verified through the private read-only probe.
- `canvas/edit.rs` and its `bootstrap`, `title`, `batch`, and `retained` modules:
  source-backed intent/preflight, title synthesis, genuine plain-text Automerge,
  shared batch eligibility projection, and session retention/reset. Preparations
  and accepted receipts remain sealed to their canonical and authenticated owner.
- `slack/api/canvas_collab/edit.rs`: `execute_canvas_checkbox_edit` consumes the
  preparation, validates canonical inputs and expiry immediately before sending,
  and returns sealed acceptance or `CanvasEditFailure::{NotSent,Uncertain}`.
- `slack/transport/canvas_edit.rs`: exact mutation path, bounded request/response,
  total timeout, no offline hold, redirects disabled, and a dedicated lazy shared
  client with application and HTTP-library protocol retries disabled. Every
  failure after dispatch stays uncertain, including invalid acknowledgements.
- `src/desktop/canvas/checklist*`: exact exported/canonical ID mapping, cached
  intents, immutable dispatch scope, visit/read generations, and stable-owner
  pending/uncertain receipts across navigation or credentials/transport rotation.
  Old accepted receipts can settle through current authorized canonical reads;
  only the original full-scope edit session adopts their CRDT state.

The first native endpoint toggle passed, but its initial restore remained
uncertain and was not replayed. Stock Slack restored the original state before
the retention correction. The corrected private native API round trip then
passed: the first ACK confirmed checked=true at canonical sequence16000, and the
second ACK confirmed checked=false at sequence17000. Both ACKs carried supported
CRDT fields1,2,3,4,5,6,8; `remember_accepted` succeeded after each. Independent
canonical reads confirmed each accepted change, target content and parents stayed
unchanged, and every other section was byte-identical. Independent authenticated
HTML export confirmed unchecked/checked restored. Structural-only evidence:
`tmp/canvas-retained-private-roundtrip.log`.

The independently locked full core suite passed **857 tests, two ignored**;
the final desktop suite passed **706 tests, three ignored** after the readback
correction. The final signed app passed strict code-signature verification.
Logs: `tmp/canvas-retention-core-final.log`, `tmp/canvas-readback-final-tests.log`
and `tmp/canvas-readback-final-build.log`.

Signed native fixtures passed at 1280×800 and true narrow 668×400, including
blocking duplicate actions, toggle and restore. Their screenshots were inspected,
as were saving, uncertain, read-only and rename states. The initial live mapping
rejection exposed a real export shape: `li[id] > span[id]` repeats the same ID.
The reviewed exception is restricted to that exact bounded wrapper shape;
ambiguous duplicate identities remain blocked, and fixtures include the real shape.

Actual native UI clicks then passed an accepted toggle and restore. Stock Slack
showed checked/checked after the first toggle and unchecked/checked after restore.
The first acceptance required manual Refresh because its own realtime event
invalidated readback; restore completed automatic readback. A subsequent
independent read rechecked private ownership and empty shares, six canonical
sections, two exact exported/canonical matches, two fresh preparations, matching
title bytes and the original unchecked/checked pair, with zero writes. Evidence:
`tmp/canvas-native-ui-restored-read.log`; the inspected restored UI capture is
`tmp/agent-ui/canvas-live-restored.png`.

The bounded post-ACK readback correction is now verified. A same-owner, same-visit
canonical read retired by a file event can receive one replacement read; errors,
rate limits and uncertain outcomes never retry the mutation. The final signed
normal/narrow fixture checks passed again, with their PNGs inspected. A fresh
live native toggle and restore both confirmed automatically, without manual
Refresh, and stock Slack independently showed both transitions. The final
zero-write canonical/export probe confirmed the original unchecked/checked pair
and private ownership again (`tmp/canvas-final-restored-readback.log`). The final
restored native screenshot was inspected
(`tmp/agent-ui/canvas-final-native-restored.png`). This live run proves the full
workflow; the bounded invalidation branch is covered by regression tests rather
than an assertion that the race occurred in that run.

Accepted-write read recovery is distinct from uncertain-write recovery: an
uncertain send still must not clear from a matching boolean or automatically
retry. Unsupported markup, controls and richer CRDT merges remain outside this
bounded path.

New sections and general rich visual authoring, multiplayer collaboration,
comments and durable offline editing remain separate gaps. Existing paragraph
editing is the bounded path described below.


## Existing-paragraph text reference capture (2026-10-13)

A fresh stock Slack UI check confirmed the task-owned canvas was invite-only,
with only the current user as owner. A real keyboard edit appended a short probe
suffix to its existing bold/Unicode paragraph, then removed that exact suffix.
Both `edit-document` calls returned HTTP 200. Reloading the canvas confirmed the
original text and bold markup; the original Slack conversation was then restored
with its empty composer preserved.

Canonical before/after loads each contain six sections with the same IDs. All
five non-target sections are byte-identical. The target CONTENT12 and PARENTS13
are exactly restored; only target sequence, update timestamp, and trace fields
changed. This is stock reference/protocol evidence, not a native text-save claim.
Raw capture remains private and gitignored at
`captures/canvas-text-private-1791920641400/network.ndjson`; the structural-only
decoder output is `tmp/canvas-text-private-structural.json`.

### Bounded native paragraph path

The new text intent accepts one existing ordinary top-level paragraph, with
bold, italic, strike, inline code, links and soft breaks. It rejects unsupported
markup, nested/structured sections and multi-paragraph editor output instead of
flattening them. Exported HTML supplies only an unambiguous section anchor; the
canonical snapshot supplies the draft, content language, revision and permissions.

`canvas/edit/text` validates a fresh snapshot and prepares a consumed one-attempt
request. The captured projection carries CONTENT12, empty top-level PARENTS13,
local counters and CRDT38; it does not invent style, attributes or activity flags.
Text history uses escaped UTF-16 offsets, a terminal FEFF, current_head12 and
processed_heads6. Removed formatting records false marks; unchanged marks are
not rewritten. A sealed ACK verifies the sent history head, canonical content and
language before retained history becomes reusable. A newer canonical revision
can settle an already accepted receipt; equal revisions still require matching
content/parents. Uncertain writes never settle from matching visible text alone.

The desktop editor shares the checklist file lock, session and retained receipts.
It snapshots and disables the rich editor in one JS task, keeps rejected drafts
intact, and guards async work by draft generation, visit, read and account scope.
Cancel retires only unsent work. Read-only refresh remains available for accepted
or uncertain results without replaying the write.

Synthetic evidence: `scripts/check-canvas-text-marks.mjs` passes seven captured
visitor cases; the Rust suite consumes the synthetic fixtures, including an
actual captured-JavaScript bootstrap history. Captured official JavaScript/WASM
read all four exported native edit cases (formatted Unicode/entity/soft-break
append, link/emoji/style change, formatting removal and empty paragraph). These
checks establish bounded protocol interoperability, not native live-save or full
rich-authoring parity. Final Cargo/native/live results are recorded in the worklog.


### Native paragraph evidence and parent normalization

The native editor completed an append and exact restoration on the same private
canvas. Stock Slack independently showed both changes, preserving bold/Unicode
text and both original checkbox values. The first completed pair returned to
read view automatically. Canonical baseline/final loads contain the same six
sections: all five non-target sections are byte-identical, and target CONTENT12
and PARENTS13 are restored. Only target sequence, update time and trace differ
(`tmp/canvas-text-final-live-comparison.json`). No message or sharing mutation
was involved.

Live ACKs explicitly encode empty PARENTS13 while full canonical loads omit it.
Text readback and retained-session association treat only these two zero-parent
representations as equivalent. Nonempty or unknown parent metadata remains
preserved and subject to the original eligibility checks; other association
fields remain exact. Focused regressions cover both representation directions
and retained sequence/head continuity. The independently locked core suite
passed 936 tests with five ignored (`tmp/canvas-text-retention-core-final.log`).

A subsequent live repetition found an intermittent late file-event race: an
accepted write could leave the editor showing Saving after the receipt was
already settled, or make the read-only view stale. Manual read-only Refresh
reconciled it, and the paragraph was restored. Do not treat the earlier successful
pair as proof this race is resolved; final recovery evidence belongs in the
worklog. Synthetic native checks do not establish IME behavior, live lost-response
recovery or in-flight account/transport replacement.


The late-event desktop correction now passes 791 desktop tests (five ignored),
normal/narrow signed native fixtures, and another live native append/restore
without manual Refresh. Settled text attempts carry explicit serial-bound proof
so later cache invalidation cannot hold their draft open. New file revisions
coalesce into one pending read task for the visible loaded canvas; failures latch
automatic reads off until explicit recovery. The final restored view had enabled
controls and no stale banner. The race ordering itself is deterministic regression
coverage; the successful live run is end-to-end workflow evidence. At that
checkpoint, invalidation during the first canonical/export load still required
manual Refresh. The subsequent implementation below addresses that separate
case. Full results and artifact paths are in `docs/parity-worklog.md`.

### Paired first-load recovery (native fixtures verified)

`src/desktop/canvas/loader.rs` now fetches the canonical document and typed HTML
export concurrently within one component-owned task. Neither response installs
independently. Completion checks the authenticated owner, file, visit, current
read ticket and Reader generation. A superseded ticket, deleted file or replaced
account/visit cannot install content or release a replacement Reader's loading
state.

A file event can invalidate this pair before the first canonical snapshot
exists. A successful same-owner export may then supply an initial read-only
preview, but cannot grant canonical edit authority or rename permission. Later
invalidated pairs retain already rendered content. This prevents an event stream
from starving first paint; a current successful pair eventually installs the
latest document and enables eligible controls when the events settle.

The visible-view effect coalesces events for 250 ms and admits one paired task
at a time, with one pending latest revision. A claimed read consumes its event
revision; stale state alone does not schedule work. Preparing/Saving operations,
offline state, an in-flight export and rename drafts block automatic admission.
A visit reopened while a retained write is Saving also receives its one initial
read after that operation settles, even if no file event arrives. Any claimed
read consumes this initial admission. An error in either half, including a
response invalidated while in flight, latches automatic recovery off until
explicit Refresh or reopening. There is no mutation retry.

Focused regressions cover first-load invalidation with no snapshot, repeated
event bursts, independent canonical/export failures, late superseded results,
account/visit/deletion guards, retained rendered content, and reopening during
Saving followed by accepted, uncertain or not-sent outcomes. The existing
`canvas-text` and `canvas-text-narrow` checker now includes controlled completions
through the production install and event-admission helpers: an inert preview,
three bounded synthetic read pairs, visible recovery of the latest paragraph and
its controls, and rejection of a late old pair. It then runs the existing text
save checks. This fixture does not exercise real transport timing or the full
production debounce scheduler. The final locked desktop suite passed 807 tests
(five ignored), followed by a signed build and both native fixtures. Their
receipts require initialLoadRecovery, latestPairWins and exactly three synthetic
read pairs; root inspected both PNGs. Evidence:
`tmp/conversation-canvas-final-tests.log`,
`tmp/canvas-paired-load-native-final.log` and `tmp/agent-ui/canvas-text*.png`.
