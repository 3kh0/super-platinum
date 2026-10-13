# Slack Lists: captured protocol and implementation boundary

Investigated 2026-10-13 using captured Slack frontend source, native Slack UI,
and a guarded read-only API probe of an existing List in a revalidated private
channel. File metadata, three records, and enterprise editing permission parsed
successfully. Subsequent testing used a newly created private verification List:
native checkbox, rich-text, number, and single-select writes were read back from
Slack. A stock-Slack checkbox change also reached the native client without
manual refresh. Live schema rename and number precision changes also reached the
native client without Refresh. Conflict/ambiguous-response behavior and schema
event ordering remain open; offline fixtures cover recovery with synthetic responses.
CDP ports 9222/9223 were unavailable, so native
Slack inspection used the computer-use accessibility surface.

## Sources and locating evidence

Files are under `tmp/sb/js/` (local evidence, not committed dependencies):

| Alias | Exact bundle |
| --- | --- |
| S | `gantry-v2-shared.0b81595876785d7a.min.js` |
| B | `gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js` |
| V | `gantry-v2-async-gantry-v2-vendors-async.daa946d0953564d3.min.js` |
| P | `gantry-v2-vendors.522a064003a98b05.min.js` |
| L | `gantry-v2-async-client-list-view.6cdad2f8b390adb5.min.js` |

Offsets below are zero-based **Unicode character offsets**, obtained using
Python `Path.read_text()`, not byte offsets. Read bounded slices, for example
`source[4611438:4613200]`; do not print entire bundles or capture envelopes.

The captured document in
`captures/slack-2026-10-09T04-26-47-827Z/requests.ndjson` identifies the public
asset base and webpack mapping for L: `client-list-view` maps to
`6cdad2f8b390adb5`, with the `gantry-v2-async-<name>.<hash>.min.js` filename
construction. Only that identified public asset was fetched, without credentials.
L is a small wrapper importing `CvyQ` from B. Both the decoder and main table UI
are already in S/B; earlier exploratory notes calling them missing were incorrect.

## Verified request construction

| Method | Wrapper evidence | Caller evidence |
| --- | --- | --- |
| `lists.open` | V `yY35`, 9787845 | B 7297948 |
| `lists.records.list` | V `84hR`, 9785799 | B `l2X+`, 7286616 |
| `lists.cells.update` | V `+L74`, 9783665 | B `sr7+`, 7266092 |

S `uGgu` at 1594178 defaults `camelCaseInput=true`, `camelCaseOutput=false`.
It removes `reason`, `abortSignal`, and `timeout` from method arguments before
calling the shared API dispatcher. S `LYxw` at 738282 recursively snake-cases
object and array keys. Thus nested `rowId`, `columnId`, and `richText` become
`row_id`, `column_id`, and `rich_text`; this mapping is observed, not assumed.

S `IlU1` at 1570369 converts `reason` to `_x_reason` and selects the active
workspace authentication/routing context. S `GGwr` at 1602234 uses POST
`/api/<method>` and the shared API response/error handling. S `qVKj` at 4551808
form-encodes arguments, JSON-encoding object-valued fields. A nonempty array of
cell objects is consequently a JSON string in the `cells` form field.

Observed arguments, after that conversion:

```text
lists.open:
  list_id; _x_reason=open-list

lists.records.list:
  list_id; include_subtasks=true; archived=false; include_suggested=false
  _x_reason=get-list-records

lists.cells.update (example existing checkbox cell):
  list_id
  cells=[{"row_id":"<record-id>","column_id":"<column-id>","checkbox":true}]
  _x_reason=bulk-update-cells
```

**`lists.open` is not a read-only metadata lookup.** The caller consumes
`last_read_timestamp` and `resolved_default_view` and integrates opening/read
state. Treat it as a read-tracking mutation; do not call it during protocol-only
inspection. Record loading separately fetches file metadata and rejects deleted
or non-List files (B 7287101–7287819).

## File, schema, records, and ordering

S `nxvY` at 3794808 reads `file.list_metadata.schema` and `.views`, converting
their keys to camelCase for frontend use. Preserve raw schema and view data in
the native model. Column `id` and `key` have different roles: incoming cells use
`column_id`; the reducer resolves that against schema `id`, then stores the value
under schema `key` (B 7229049).

Primary-column precedence is `is_primary_column`, a text/rich-text column whose
key uppercases to `NAME`, the first text/rich-text column, then `"name"`.

S `iLfW` at 4610722 contains the decoder. `getUpdatedRecord` at 4612345 consumes:

- `id` and `fields`, where each array entry is addressed by `field.key`;
- `field.text`, preserved separately as a display fallback;
- `date_created`, `created_by`, `thread_ts`, `position`, `parent_record_id`;
- `updated_timestamp`, `updated_by`, `view_positions`, `platform_refs`;
- `is_subscribed`, `saved`, `saved_fields`, `archived`, `suggested`, `date_due`,
  and `assigned_at`.

Keep unknown fields rather than discard them. `getTypedFieldValue` at 4611438
recognizes checkbox booleans including `false`, numeric array values including
zero, `rich_text`/`richText`, typed arrays, links, references, and timestamp/date
compatibility. An absent value resolves to null; an empty rich-text array is
preserved. Date ranges may be joined with `:`. Do not flatten every field into a
string or conflate missing values with `false`/zero.

`sortRecordsByDefault` at 4614478 uses the selected view's `view_positions` entry
(case-insensitive view ID lookup), otherwise `record.position`, and breaks ties
with `date_created`. Preserve position types and ordering semantics; don't sort
record IDs or coerce every position to an integer.

Rich-cell presentation uses the same typed fields. B `FieldRenderer` at
18171200/18172206 resolves select IDs through `options.choices` entries with
`value`, `label`, and `color`, in schema order. B `HSOl` at 18192302 passes
`rich_text` arrays to the block renderer. The link renderer at 18164480 makes
`display_as_url` override `display_name`. Native rendering reuses its restricted
content renderer and URL policy; app actions, embeds, and arbitrary CSS colors
are not introduced through List values.

Parent decoding is at S 4612673; B 7236453/7241932 establish child indexes and
selectors. Stock main-grid selection at B 17874252 excludes parented/archived
rows. The native expandable tree is therefore a presentation choice. It preserves
default sibling order, cuts cyclic parent edges, promotes unavailable-parent
children to reachable roots, and keeps expansion local to the current dialog.

## Number display precision and locale

B `LlTF` at 18251467 supplies the Lists number formatter. Its options adapter
uses `precision: options?.precision ?? 0`; the formatter passes that precision
as **both** `minimumFractionDigits` and `maximumFractionDigits` to
`H0Nw.formatNumber`, with the current Slack user locale. Null/NaN values render
empty. B `NumberColumnEditor` at 18110283 offers precision 0 through 5, labeled
None, .0, .00, .000, .0000, .00000, and writes the selected `precision`.
`options.format` is not read by this number formatter.

B grid column construction at 17994074 routes both Number and Currency through
this same formatter. This verified Lists table path does **not** supply
`style: "currency"`, a currency code, or a cents conversion. Do not reuse the
unrelated billing currency formatter or add a currency symbol by inference.
FieldRenderer's Number display at 18174077 and filter labels at 17831332 also
use `LlTF`; raw numeric editor and wire values remain unrounded.

S `H0Nw` at 854386 delegates rounding/grouping to `Intl.NumberFormat` and applies
Slack locale adjustments: Spanish four-digit rounded values disable grouping;
`es-LA` maps to `es-419`; `es-ES` replaces grouping periods with U+202F; `es-LA`
replaces grouping commas with U+202F and the decimal point with a comma;
Italian explicitly enables grouping when unspecified. S `YlrH` at 4577437
resolves the active team/enterprise's user locale. Do not silently substitute
the operating-system locale and claim exact Slack parity.

The native cached cell projection now applies these rules through the core
`slack_lists::numbers` formatter. Stored 12.5 displays as 13 at default precision
and 12.50 at precision 2; the editor seed and outgoing `number` remain 12.5.
Decimal digits are rounded without converting integer values to `f64`, retaining
exact i64/u64 values. Currency follows the same decimal display path.

Locale comes from the workspace's Slack `prefs.locale`, restored from the warm
sidebar cache and updated by generation-checked realtime preference events.
Missing boot preferences preserve the cached value; explicit null clears it.
The captured supported keys are de-DE, en-GB, en-US, es-ES, es-LA, fr-FR, it-IT,
ja-JP, ko-KR, pt-BR, zh-CN, and zh-TW. Missing or unsupported keys use en-US;
this native surface has no Slack URL locale override. Locale changes reproject
loaded rows without a List API request, preserving expansion and editor drafts.
Missing/null precision uses zero; malformed precision or values outside 0–5
retain the unformatted value rather than invent a schema interpretation.

The focused formatter suite passed 1,188 comparisons against a captured browser
`Intl.NumberFormat` oracle, including all 12 locales, precision 0–5, negative
zero, ties, carries, exponent values, and exact large integers. Additional tests
cover cache/boot/realtime locale ownership and draft-preserving reprojection.
The native checker now asserts rounded table values with exact editor seeds,
schema precision changes, and locale changes during an unsaved numeric draft;
those new native assertions await the next root-owned build/check run.

## Typed writes and edit lifetime

B `uDYD` at 7312962 converts editor values into typed cell arguments:

| Column kind | Wire value |
| --- | --- |
| text / rich_text | `rich_text: [...]`, not a plain string |
| checkbox / completed / todo_completed | `checkbox: boolean` |
| number / currency | `number: [number]`, or `[]` to clear |
| select / multi_select | `select: [option IDs]` |
| date / due_date | `date: [value]` |
| todo_due_date | `date` or `timestamp`, selected by the caller's timestamp mode |
| user / assignee / todo_assignee / vote | `user: [IDs]` |
| attachment / canvas | `attachment: [IDs]` |
| channel | `channel: [IDs]` |
| email / phone / rating | corresponding typed array |
| link | `link: [{original_url, display_as_url, display_name?}]` |

Empty arrays/false differ by kind. Some frontend intermediate values use `|` or
`,` delimiters; a native typed model need not reproduce those intermediate strings.

Current select cardinality is also encoded by `type: "select"` with
`options.format: "single_select" | "multi_select"`: B `SelectColumnEditor`
at 18112511 changes this option, and `FieldRenderer` at 18176701 reads it.
P `jHPH` at 1853976 additionally defines the older `multi_select` schema type.
Do not infer cardinality from the type string alone or invent an
`allow_multiple`/`is_multiselect` option.

B `sr7+` at 7266092 deduplicates row/column edits, skips equal values unless
explicitly overridden, snapshots previous values, applies optimistic state, and
submits cells. Existing cells carry `row_id` plus schema `column_id`. Creation
uses distinct `row_id_to_create`/`column_id_to_create` paths: exclude these from a
first existing-cell editor. The caller uses response `cells` when reconciling
creation identities and undo history. It handles
`slack_connect_file_upload_sharing_blocked` explicitly; generic failures produce
a FailedSaving banner.

## Captured record creation contract

B `t0Mq` at 7276635 is `Create list record`; V `9StH` at 9785110 wraps
`lists.records.create`. The stock header `NewItemButton` at B 179168xx passes
only `listId` and `viewId`, then opens the returned record. The thunk's ordinary
request contains `list_id`, `_x_reason: "create-record"`, `position: ""` when
no caller position is supplied, and `updated_timestamp`. Optional caller values
become `fields`, `initial_fields`, `parent_record_id`, and
`duplicated_record_id`; `view_id` is not forwarded. No placeholder record ID is
submitted. The service supplies `response.record.id`.

The create thunk resolves the supplied view and finds the first schema column
whose type is `todo_completed`. If the view is a Completed-items view, it appends
`{column_id: column.id, checkbox: true}` to the caller's copied initial fields.
B `5QWf` at 2838985 defines that condition as
`view.defaultViewKey === "completed_items"` (wire `default_view_key`). This
branch does not additionally test the column key or `todo_mode`. The header
otherwise supplies no group/filter defaults. Board lane creation at B 17964742
separately constructs typed initial fields from the lane and selected filter
families; it must not be inferred for the header action.

`initial_fields` uses schema column IDs plus the same typed value conversion as
cell updates. For example, B 17964742 maps `{columnId, ...uDYD({type,value})}`,
and `uDYD` maps text/rich-text to `rich_text` blocks. Thus a native name draft can
be included in one create request without first creating a blank row and then
sending a second mutation. This is a native interaction choice; the observed
stock header itself creates blank before opening details. A newly created item
may fail the current saved filter and still be opened using its returned
canonical record; creation is not evidence that the filter should be changed.

The native header now follows the blank-item interaction: one explicit Add item
starts one guarded create request, and confirmed canonical readback opens the
new item's details. It does not add a name form or synthesize filter/lane values.
The accepted-create authority can open that returned item even if the current
saved filter excludes it. Changing the visit/view before completion prevents
an unsolicited detail switch and leaves an explicit Open item action instead.
Unknown outcomes retain a warning and block further creates until the user
explicitly acknowledges checking Slack; that acknowledgment sends no create
request. A refresh cannot identify a lost-response blank by its title or by
comparing row counts. The disconnected fixture covers these boundaries, but
execution receipts and any live server verification are recorded separately.

### Creation clock and limits

B `AZAI` at 7210909 initializes a mutable Lists clock from a timestamp with
counter zero and a generated node. `PHBi` at 8752648 obtains that node by
removing hyphens from a UUID and retaining its last 16 characters; S `Rb/W` at
893820 produces lowercase-hex UUIDv4 strings. The timestamp constructor replaces
zero milliseconds with the current time. The Lists store at B 7225709 persists
the clock; its serialized fallback reads `_milliseconds`, `_counter`, `_node`.

B `getTimestamp` at 7275769 chooses the greater of the prior milliseconds and
current wall time. Retaining the prior millisecond increments the counter;
advancing time resets it to zero. It rejects a clock more than 60 seconds ahead
and counters above 65535. `pwnG` at 8753133 serializes decimal milliseconds,
a four-character uppercase-hex counter, and a 16-character node, separated by
hyphens. This value is update-order metadata: the captured source does not
establish it as a server deduplication or retry key.

Creation limits come from top-level `file.list_limits`, via `8A13.Q` at B
7240793 and `Ltli.getAllFiles`; they are not the number of currently loaded rows.
At B 7277971, `over_row_maximum` leads to the confirmation whose text explains
that adding an item archives the oldest item. A remembered dismissal can skip
that dialog in stock Slack. A bounded native creation slice must not infer
permission to autoarchive from ordinary Add-item intent or from missing limits;
verified below-limit metadata is the conservative initial boundary. This is a
native safety restriction, not a claim that Slack rejects absent metadata.

The response consumer reads `record.id` and decodes `record.fields` as an array
using S `iLfW.getUpdatedRecord` at 4612345, retaining the record's creation,
position, parent, update, archive, and other metadata. Source does not prove
that `record.list_id` or the submitted timestamp is echoed. Their absence must
not be confused with a verified conflicting identity. Actual server defaults,
near-limit races, and uncertain-create reconciliation require separate live
verification; no automatic retry is justified by this source.

## Permission and native UI requirements

- File recognition is `(mode == "list" || filetype == "list") && subtype !=
  "poll"` (S 3752937). Native `files.rs` now routes files matching this predicate
  to the Lists dialog.
- Ordinary List editability requires `access` equal to `write`/`owner` and
  `!is_locked` (S 3757572, 3747420), plus Lists editing enabled by team preference
  (S `B4qP`, 3794590; B `getListIsViewOnly`, 7246726).
- The selector has an explicit personal-task-list exception (S `j2uc`, 2614000).
  Do not generalize it to ordinary Lists or treat missing permission data as write
  permission.
- Main UI: B `CvyQ`, 18044960; `BaseListDetail`, 18026473. Grid cells use
  `rowheader`/`gridcell`, `aria-colindex`, and roving focus (B 17718122).
- Filtering, grouping, ordering, hidden columns, saved views, and subtasks are
  separate semantics. A schema-order table is not proof of saved-view parity.
  Keep row and input identities stable during refreshes and retain a local draft
  until an explicit commit/cancel outcome.

## Pagination, response, and realtime limits

**No continuation request has been verified.** B `l2X+` at 7287475 sends no
cursor/limit arguments. It consumes `records` and sets `recordsLoaded` from the
absence of `response_metadata.next_cursor`. The actual `BaseListDetail` mount
effect at 18033162 fetches once, marks its local loading state complete, then
filters/sorts stored records client-side. Do not invent a continuation parameter,
reuse a different API's pagination contract, or label a cursor-bearing response
as complete. Capture a real continuation or authoritative schema before adding it.

Realtime event handlers at B 5815690–5818000 establish these names and fields:

- `list_record_removed`: `list_id`, `id`;
- `list_records_archived`: `list_id`, `ids`, `restore`;
- `list_record_updated`: `list_id`, `record`, or deletion via `deleted`/`id`;
- `list_record_created`: `list_id`, `record`;
- `list_cells_updated`: `list_id`, `cells`, optional `column`;
- `list_contents_merged`: triggers a fresh records fetch.

B 17789197 resubscribes on reconnect and schedules a refresh. These are verified
consumer expectations, not full event schemas or proof of delivery/order. Missing
evidence includes live response shapes, partial batch success, conflict/version
semantics, authorization errors for each method, reconnect delivery, and update
acknowledgment versus realtime-event ordering. Do not copy the captured optional
column handler's truthiness check on its array index: it appears to skip index 0.

### File viewing and realtime subscription lifecycle

The native handler already recognizes `list_cells_updated` and `file_change`.
B 5816607 handles an optional `column` on the former; B `filesChanged` at
5765623 reads the latter's `file_id` and invalidates file metadata. Its
`file_change` registration is at 5775753. No separate schema/column event was
established by this investigation. Native columns remaining stale until a manual
refresh therefore does not justify inventing additional event names.

The native List reader now implements the separate file-viewing lifecycle shown
by the captured client:

| Step | Verified source and behavior |
| --- | --- |
| Open | B `3uCk` at 6906374 calls `files.open` with `fileId`, `supportsReadOnlyCheck: true`, `lastUpdated` from the mount's `Date.now()`, and reason `open-file`. It consumes `viewers` and `should_subscribe_and_ping`. V `ey1N` at 9767933 supplies the endpoint. |
| Subscribe | B `WBU9` at 5595132 sends `{channel_id: fileId, type: "temp_channel_sub"}` through the current connected socket. The open handler invokes it for Lists when `should_subscribe_and_ping` is true. |
| Keep viewing | B `Tze1` ping thunk at 13677466 calls `files.ping` with `fileId`, `isFocused`, and reason `ping-file`; `isFocused` compares the current window's focus/blur timestamps. It consumes `viewers` and `should_subscribe_and_ping`, unsubscribing when the latter becomes false. V `VFi9` at 9768098 supplies the endpoint. |
| Schedule | B `FileOpenSubscriber` at 13678205 starts a 30-second interval only after a successful open returns true and the component remains mounted. It stops on false, `file_not_found`, or unmount. |
| Close | B `Tze1` at 13676740 calls `files.close` with `fileId`, current `lastUpdated`, and reason `close-file`; V `IAj7` at 9763605 supplies the endpoint. The caller schedules unsubscribe after `60000 * !wKdA.W()` milliseconds and checks whether the List was reopened before sending it. The module tracks an open-view count per List. |
| Unsubscribe | B `00Dn` at 5707347 sends `{channel_id: fileId, type: "temp_channel_unsub"}` only while connected. |
| Reconnect | B `useResubscribeListOnReconnect` at 17789197 subscribes again for the active eligible List and schedules a records refresh with up to 10 seconds of random delay. |

The shared request conversion described above makes the HTTP fields `file_id`,
`supports_read_only_check`, `last_updated`, and `is_focused`. These are viewing/
presence operations, not read-only protocol probes. The source also contains a
special open override before checking `should_subscribe_and_ping`; it must not
be generalized to bypass an explicit false response for ordinary Lists.

The implementation lives in core `slack/api/lists/viewing.rs`, realtime frame
helpers, and desktop `slack_lists/lifecycle.rs`. It reuses the connected
workspace's socket and serializes open/ping/close per file. Account, viewer,
visit, and socket-generation guards prevent old work from acquiring a newer
visit's subscription. The runtime tick drives the 30-second ping schedule and
guarded delayed unsubscribe; explicit false responses stop eligibility.
`Connection::try_send` distinguishes accepted, full, and closed queues. Accepted
means locally enqueued, not a server acknowledgment. The dedicated
`Transport::execute_file_viewing` path rejects known-offline work immediately,
uses a narrow endpoint allowlist, and does not hold or replay viewing requests.

Root's live verification on the separate private verification List observed a
stock-Slack schema rename and restoration automatically appear in the open
native reader without Refresh. Changing a number field's precision from 0 to 2
and back to 0 likewise changed its native display from `13` to `12.50` to `13`
without Refresh. This establishes live metadata-driven refresh and reformatting
for those operations. It does not establish every server event shape, frame
acknowledgment, or close/reconnect race. After the latest review fixes, root
reported core 737-test and desktop 621-test suites passing, followed by a signed
build and strict deep signature verification. A subsequent live preflight trace
identified top-level `last_read` as the sole difference rejecting the first typed
save after opening a List. Typed preflight now ignores only that read marker;
all other file metadata, schema, records, columns, permission and routing checks
remain strict. Final validation receipts belong in the parity worklog.

`lists.open`, explicit List read marking, resolved defaults, and the native
saved-view picker remain separate and unimplemented by this file-viewing lease.

## Saved-view projection: captured requirements

The core projector lives under `slack_lists/saved_views`, with a native picker
and cached desktop projection. The following consumer evidence defines its
semantics; it is not a claim of live verification of every view shape.

- **View identity/defaults:** S `nxvY` at 3794808 reads raw
  `list_metadata.views`, camel-cases a copy, and finds views by exact ID. B
  `BaseListDetail` at 18026473 sorts selectable views by `position`, excludes
  record/canvas-inline layouts from the normal picker, and validates selected
  IDs against available views. Its selection logic combines an explicit view,
  template-initial view, server `resolved_default_view`, personal default,
  `list_metadata.default_view`, all-items fallback, user preference, and
  channel-tab/return-navigation state. These are distinct sources, not synonyms.
  B 7297948 stores the `lists.open` response's resolved ID; B 7251175 separates
  resolved and personal default selectors. An asynchronous default must not
  replace a view the user already chose (mount guards at B 18026473–18033162).
- **Columns:** B `czID` at 2816333 iterates the selected view's `columns` array
  in its existing order, resolves entries by schema `key`, and retains known
  columns with truthy effective `visible` (plus a grouping-column exception).
  It does not append every omitted schema column. B 7251430–7253450 derives
  server visibility from entries with explicit `visible`, then overlays local
  refinements. Primary/completed-column promotion is a caller option, not a
  universal sorting rule. Preserve raw width/other column metadata without
  treating it as an arbitrary CSS value.
- **Filter representation:** B 7247470 combines saved `filters` and
  `info_column_filters`. Leaves contain `key`, `operator`, `values`, and optional
  `column_id`; V `s6tE` at 9849994 supplies exact operator strings (`equal`,
  `not_equal`, comparisons, `contains`, `does_not_contain`, `is_empty`,
  `is_not_empty`, `is_me`, `is_not_me`). B `GgRn` at 2824835 models grouped local
  refinements as `{match: "all" | "any", groups: [{filters: [...]}]}`. This is
  evidence for local evaluation, **not proof of a server `filter_set` member**.
- **Filter behavior:** B `+E6Z` at 2849689 applies flat leaves sequentially
  (AND). Its grouped path at 2857601 ANDs leaves within each nonempty applicable
  group, then intersects/unions group record-ID sets for all/any. Empty/not-empty
  run before kind-specific handling: zero is nonempty, unchecked is empty.
  Checkbox/completed controls use these operators rather than guessed boolean
  equality. Numeric comparisons exclude missing values; select membership uses
  current single/multi format; `viewer` and is-me operators require the current
  authenticated user. Date presets use local calendar boundaries, ISO weeks,
  last-read state, and a separate zero-valued “Later” case. Unsupported date or
  operator semantics must be reported, not silently treated as all rows.
- **Sort behavior:** B `applySortToRecords` at 2852950 uses ordered sort keys
  and `ascending`, after default `view_positions`/position ordering. Selects
  sort by schema choice order (minimum matching rank for multiple selections),
  not displayed label; empty/deleted selections sort last in both directions.
  Numeric missing values also sort last while zero remains real data. Text uses
  the first meaningful rich text/name token; user/channel sorts depend on local
  name maps. Stable ties must retain incoming row order. Date and legacy-kind
  edge cases need separate parity fixtures rather than a generic string sort.
- **Grouping:** B `useGetRowItems` at 2833174 builds select groups from schema
  choices; multi-valued fields can place one record in several groups. All-deleted
  selections go to Ungrouped. Collapsed groups keep counts but omit rows. B
  `5QWf` at 2838985 decodes `grouping.order` as typed cell objects (such as
  `{select:[id]}`), not plain IDs, and recognizes `default_view_key` values
  `all_items`, `incomplete_items`, and `completed_items`. Board ordering at
  17962068 is distinct from grid grouping; do not assume the grid applies every
  board ordering rule. Group-row keys must include group identity as well as
  record identity, while edits still target the single canonical record.
- **Pipeline and completion:** B `VBDB` at 2814031 excludes deleted, parented,
  archived (unless archive mode), and suggested (unless opted-in) records before
  projection; then filters, sorts, searches, and applies completion-view rules.
  `show_completed_items` defaults false at B 7260374, with personal-task/feature
  exceptions. Native hierarchy is currently a separate presentation choice, so
  filtering parents/children cannot be claimed equivalent without an explicit
  policy and fixtures.

The implemented boundary is a raw-preserving `SavedView` plus core projection
for **table and bounded Board views**: exact column visibility/order, supported
typed leaf filters, ordered sorts, and the supported grouping families described
below. Unknown operators, layouts, and
identity conflicts produce an explicit unsupported-view reason. “All loaded
items” remains a separately named native fallback. Server defaults resolve only
through the authorized open lifecycle; choosing a view locally does not call
`lists.views.update` or change another user's shared view. Local refinements and
saving/deleting views can follow once their write contract is separately modeled.

Tests should cover resolved-default arrival after a manual selection, removed
view/column IDs, missing versus false/zero, multi-select format, option-order
sorting, stable ties, empty/deleted groups, duplicate projected instances of one
record, and a source edit while its row is filtered out. Maintain incomplete-page
notices: filtering the fetched first page never makes it a complete List.

### Default selection, read state, and task completion audit

The following offsets are characters in the same captured bundles, not line
numbers. These are consumer-side observations; response schemas have not been
exhaustively verified against live requests.

| Concern | Verified captured behavior |
| --- | --- |
| List open | B 7297948 calls `yY35.xW({listId, reason: "open-list"})`; V `yY35` 9787845 maps this to `lists.open`. The shared conversion gives HTTP `list_id` and `_x_reason`. No cursor, view ID, or timestamp is supplied by this caller. |
| Open response | That callback parses `last_read_timestamp` with `parseInt(..., 10)` and stores it separately from a truthy `resolved_default_view`. B 7250864/7251090/7251324 define distinct last-read, resolved-default, and personal-default selectors. The date-filter consumer uses `Dayjs.unix(lastReadTimestamp)` at B 2847963, establishing seconds for this value; do not replace it with the current millisecond clock. |
| Mark read | B 18029040 invokes `X4ex.K` only when `viewContext !== "peek"`, passing `{fileId, timestamp: Date.now(), reason: "mark-list-read"}`. B `X4ex` 6921201 sends only when `file.created * 1000` is nonzero and does not exceed that timestamp. V `WIAG` 9767827 identifies `files.markRead`; wire fields are `file_id`, `timestamp`, `_x_reason`. A successful response removes a locally shown badge. This is distinct from `files.open`/ping/close presence and from merely storing the `lists.open` response's prior read timestamp. |
| Personal default | B 17875864 fetches `lists.getDefaultView` with `listId` and reason `fetch-personal-default-view`, stores `view_id`, and treats only `default_view_not_found` as an empty personal default. V `e87w` 9787523 defines that endpoint/error. BaseListDetail starts this fetch only under `lists_default_view_dual_write`; S 3792932 defines that flag as experiment group `"on"`. This does not authorize `lists.setDefaultView`. |
| Legacy preference | B 7255043 reads the string preference `lists_default_view`, parses it as a JSON map keyed by List ID, and catches invalid JSON as an empty map. This is not the same payload as `resolved_default_view` or `view_id`. |
| View candidates | B 18029133 sorts normal picker candidates by `position` and excludes `record` and `canvas_inline`; the selected-view validity set excludes only `record`, allowing a separately routed canvas-inline view. B 18029465 begins the ordinary candidate expression with a template-initial view, then explicit/local route, valid resolved default, valid personal default, metadata `default_view`, conditional all-items fallback, and legacy preference. The surrounding tab/window/template effects mean this expression is not a universal precedence specification. |
| Manual/async guard | B 18030093/18030212 applies an arriving resolved/personal default only when no manual selection has occurred, no explicit route view exists, the ID is valid, and the one-time async-default guard has not been consumed. The personal effect additionally requires no resolved default. Both share the same guard: a personal response which actually changes the selection first can consume it. Manual selection records the selected view per window/List. |

`list_metadata.todo_mode` is read directly by S `nxvY.getListTodoMode` at
3797516. It must not be inferred merely from a `todo_completed` column or the
List title. The exact captured completion pipeline is:

1. B `VBDB` 2815696 applies its extra completion filtering only when
   `lists_grid_todos` is enabled, the List's `todo_mode` is truthy, a selected
   view exists, and the schema includes a `todo_completed` column. S 3790996
   defines this experiment as group `"on"`.
2. Effective visibility comes from B 7260309: saved
   `show_completed_items ?? false`, overridden by a defined local refinement.
   Before either, a personal task List forces visibility to true when the
   `lists_todos_rollback` experiment is not on. S 3792393 defines that rollback
   flag. S `j2uc` 2614000 identifies a personal List as `assigned_to_list` or
   the first `task_lists` entry, not every List containing task columns.
3. When effective visibility is false, remove completed rows; then, for
   `default_view_key == "completed_items"`, retain only completed rows. The
   captured `5QWf` helper at B 2838985 recognizes `all_items`,
   `incomplete_items`, and `completed_items`, but the pipeline does not replace
   the visibility boolean with a guessed rule based solely on those names.

The experiment selector at S `4OuQ` 2402782 reads
`state.experiments[name].group`; `_Z` delegates to that selector at 2404158.
An absent assignment in a *complete stock state* evaluates to null/not-on.
A native client which did not fetch/preserve that state cannot equate its
missing evidence with a verified off assignment. The current pure projector's
explicit completion-policy input preserves this distinction. Desktop integration
also needs an effective show-completed override for the personal-task case;
reading only the saved boolean is insufficient. If policy evidence is unavailable,
use an explicit unsupported-view result plus the separately named native fallback.

**Proposed native contract (integration policy, not an additional wire schema):**

- Capture account epoch, workspace, authenticated viewer, List ID, and visible
  visit generation for an explicit List-open operation. Issue at most one
  `lists.open` for that visit, with no generic offline hold or replay that could
  mark a closed/replaced visit open. Its actual server-side read-state effects
  are not established by the response consumer alone. Model `files.markRead`
  separately for an authorized non-preview visible read; do not fire either
  operation for background metadata refresh or saved-view picker changes.
- Keep explicit routed selection, local manual selection, and resolved-default
  loading state distinct. A manual/explicit choice always survives a late default
  response. On metadata refresh, validate its ID against the current catalog;
  stale or unsupported selections need a visible reason, not silently unfiltered
  rows under the saved view's name.
- For the ordinary standalone surface, use a valid explicit route first. With
  no route/manual choice, retain template-initial handling and use validated
  server-resolved state when available; metadata default, an explicitly requested
  all-items fallback, legacy preference, and the first selectable view provide
  provisional local candidates. Preserve the chosen candidate's provenance so
  one valid late default may replace only an untouched provisional selection.
  Do not implement channel-tab/window-return precedence without its additional
  context. If fetching personal defaults concurrently, reproduce the shared
  one-time settlement guard rather than allowing repeated asynchronous jumps.
- Parse open result fields independently and conservatively. Missing/empty
  resolved default means no candidate; malformed values or unknown IDs do not
  select a view. Retain the prior read timestamp for date filters independently
  of the current clock and the selected default. Local view selection does
  not call `lists.setDefaultView`, `lists.views.update`, or write user preferences.
- Verify completion feature assignments and personal-task policy in the same
  authenticated scope before claiming task-view parity. Keep incomplete-page
  notices visible after projection. Regressions should cover late defaults after
  a manual choice, old-account/old-visit responses, invalid/deleted view IDs,
  template/explicit-route conflicts, missing assignments, and personal-task
  visibility overriding a saved false value.

## Implemented native boundary

The domain and request code lives in `crates/super-platinum-core/src/slack_lists/`
and `src/slack/api/lists.rs` within that crate. Desktop ownership, reads, and
mutations live in `src/desktop/slack_lists/`; `view/slack_lists.rs` renders the table.

- Opening an existing List fetches `files.info`, the captured first-page
  `lists.records.list` request, workspace editing permission, personal-task policy,
  and scoped experiment assignments. The saved-view integration adds once-per-visit
  `lists.open` and eligible `files.markRead`, plus independently settling personal
  defaults when dual write is enabled. Account, visit, manual selection, editor,
  and mutation guards protect late results. Missing or failed permission lookup
  leaves readable rows available but disables editing.
- Documents, schema, views, records, and typed field objects retain raw JSON.
  The parser rejects duplicate identities and malformed required structures.
  Checkbox false, numeric zero, missing fields, and unsupported values remain
  distinct. The table uses schema-order columns, bounded rich text, safe links,
  select-option labels, and cached user/channel names with raw-ID fallbacks.
  Rich display traversal is limited to 32 levels, 2,048 nodes and 64 KiB of
  strings; oversized cells show an explicit notice without altering raw data.
- Loaded rows use default position/date ordering and exclude archived rows.
  Tables render local pages of 50 with native checkboxes and keyboard cell movement.
  Local pages do not fetch server pages. A nonempty server cursor produces an
  incomplete-results notice. Parents can expand/collapse children in a native
  treegrid. Missing, archived, deleted, and cyclic parent references remain
  reachable with notices. Inline hierarchy is a native design, not proof of
  stock Slack's saved-view behavior. The saved-view picker projects exact visible
  column order, supported typed filters, stable sorting, and select/checkbox
  groups. Multi-group rows retain separate DOM identities and one canonical edit
  identity; empty groups and local collapse state remain visible. Unsupported
  layouts/rules show an explicit notice; All loaded items remains a separate
  hierarchy fallback. Saved date filters and date sorting use the explicit
  calendar and timestamp policy described below. Boards use separate lane rules
  and bounded local paging. Local refinements and other layouts remain unfinished.
  Native/live validation status is recorded in the
  parity worklog.
- Existing ordinary `checkbox` cells with concrete booleans are editable.
  `completed` and `todo_completed` additionally require verified personal-task
  preferences proving the List is ordinary. Personal completion, absent cells,
  other field kinds, and row/column creation stay read-only. The controller captures list/record/column identity and raw
  revisions, then re-reads document, records, and permission before dispatch.
  Changed identity/content, unavailable/tombstoned files, or lost permission reject
  the edit. Top-level `last_read` alone is ignored during captured-document
  comparison; schema, views, records, permissions and unknown fields stay strict.
  It sends one `lists.cells.update` attempt, with no automatic replay or known-offline
  hold. Accepted edit intent survives local collapse; switching saved views, even
  away and back, rejects a still-preparing edit. Focus follows a canonical cell
  when an accepted grouping edit moves its rendered instance.
- A mutation locks further edits for that List. Displayed checkbox state changes
  only after readback. The response validator requires one matching cell with the
  requested boolean; `ok:true` alone is insufficient. Even an accepted response
  triggers a guarded readback. Ambiguous outcomes remain locked until a concrete
  same-identity readback is available; a different observed value requires a new
  explicit user edit. Locks survive closing/reopening the List in the same scope.
- Account, workspace, viewer, session, transport, visit, and read revisions guard
  asynchronous results. Known List/file events invalidate the reader and request
  fresh data rather than patching partial event payloads into cells. Reconnect
  retires older in-flight reads; deletion blocks reads and writes.

## Board projection and native surface

The core Board projector is `slack_lists/saved_views/board.rs`; native rendering
is `view/slack_lists/board.rs`. This section records implemented behavior and
captured evidence. Signed native and private live validation receipts are recorded
in the parity worklog under Native Board views.

B `BoardView` around 17959600 builds lanes independently of grid grouping.
With no grouping field it supplies one Ungrouped lane. For select fields it
starts from schema choices, optionally sorted by the grouping field's saved sort;
checkbox and `todo_completed` groupings supply a true lane. An empty-value lane
is prepended. B `5QWf.B_` at 2838985 applies saved `grouping.order` as typed cell
objects: a select object's first ID, or empty when absent; checkbox true maps to
the true lane and false maps to empty. Unknown/deleted order entries and duplicate
entries are skipped, then remaining lanes retain their original order. Board
order is not copied into table grouping.

B `Q81I` around 18314700 assigns a multi-select record to every matching known
nonempty lane. A record with no matching selection, including deleted choices,
belongs to Ungrouped. Each rendered copy keeps a lane-specific instance identity
and the same canonical record identity for editing. The BoardLane condition
around 18307300 hides empty Ungrouped; named empty lanes remain, including in
read-only mode. An applicable filter on the grouping key can suppress a named
lane even when it contains a record also belonging to another matching lane.
An unrelated filter does not suppress empty named lanes. These rules differ
from the writable-grid empty-group rule.

BoardView requests `forcePrimaryColumnFirst` and then excludes the grouping
field from card fields (B around 17960700). Native cards promote an already
visible primary field without reintroducing hidden fields. When the verified
todo feature path applies, the captured primary renderer reads canonical
`columnMap['todo_completed']` and renders completion beside the primary; the
ordinary completion field is suppressed in the card body. The native header
control retains that separate canonical column identity, existing permission and
personal-task guards, and the same preflight/readback path as other edits. No
completion header is invented when the primary is hidden or the canonical field
is unavailable. Ordinary checkbox grouping remains a separate case.

The native surface supports ungrouped, select/multi-select, checkbox, and
`todo_completed` Boards. It mounts at most four lanes with ten cards per lane,
using local lane/card paging and horizontal scrolling. Page controls and roving
field focus preserve card/column identity; Up/Down navigate cards, Left/Right
navigate nonempty lanes, and Tab/Shift+Tab traverse fields within a card. Existing explicit
field editors and checkbox controls use canonical identities despite duplicate
cards. A card with no visible nonempty fields receives an inert keyboard target
instead of revealing its hidden primary field. Local paging is a native bounded
presentation choice, not a claim to reproduce Slack's virtualization.

Synthetic native scenarios cover saved typed lane order, duplicate membership,
empty/read-only/filter-sensitive lanes, primary/grouping field rules, keyboard
traversal, paging, horizontal overflow, ordinary and heading checkbox writes,
and typed editor readback. Their execution results belong in the parity worklog.
Date, rating, user/channel, and other Board grouping families remain explicitly
unsupported. Drag-and-drop, lane reordering/creation, card creation, saved-view
writes, server cursor continuation, and personal-task completion side effects
are outside this slice. Incomplete loaded-page notices remain visible.

## Local record details

Captured B `t0NX` at 7300063 is the ordinary Open record action. It dispatches
`lists.records.open` with `listId`, `id`, and reason `open-record`, marks the
active record locally, and then routes the item or its thread. Its arguments
carry `viewId`, `highlightColumnId`, `threadTs`, `defaultContentType`, and focus/
navigation options; those are caller context, not additional cell-update fields.
V `wCCe` around 9785910 wraps the endpoint. Under the shared argument conversion
the request carries `list_id`, `id`, and `_x_reason=open-record`.

The detail renderer is lazy-loaded as `client-list-record-details` / module
`Jkle` at B 29925354. That chunk is absent from the inspected cached bundles, so
the open-action source alone does not prove detail field ordering or layout.
Separate stock-Slack inspection of the private verification List showed the item
pane exposing its name and completion control plus the remaining schema fields.
Opening from a Board also exposed the grouping field omitted from the card body.
This establishes field availability in that observed List, not a universal
field-order contract.

The native panel is a local full-schema reader under the existing List overlay.
It opens only from a current, visible table/Board record instance and retains the
canonical record ID independently of the originating view's hidden fields.
Fields use the existing safe rich renderer, identity labels, empty-value display,
and supported editors. Schema order and local pages of 30 fields are native
presentation choices. The underlying bounded layout stays mounted but hidden
and inert, preserving its page and return-focus target. Explicit Open item
buttons and Alt+Enter open details; Back/Escape returns to the list. Nested editor
Escape cancels that draft first.

Detail editing uses the existing `lists.cells.update` construction and guarded
preflight/readback. It does not broaden field-type or personal-completion
permissions. A separate detail authority admits hidden schema fields without
granting that authority to stale grid/card callbacks. Closing details or selecting
another saved view retires unsent detail work; already sent mutation receipts
remain tracked. Fresh canonical reads retire old callbacks while retaining a
mounted draft for explicit cancellation; stale data disables Save. Removed,
archived, or replaced records retain an unavailable last-loaded snapshot with
editing disabled. Account, List visit, record identity, schema, and permission
checks still apply. Focus returns to the original instance/cell when available,
then to a surviving canonical record when a grouping edit moved its card.

This slice deliberately **does not send `lists.records.open`**. Its open/read
tracking side effects and ownership lifecycle remain unimplemented; local panel
opening must not be described as reproducing those server effects. Threads,
comments, subtask creation, new fields, record creation/deletion, record history,
and standalone/deep-linked records outside the loaded page also remain outside
this panel. Synthetic checks cover table/Board opening, hidden grouping-field
edits, bounded field paging, nested Escape, view-change cancellation, current
event invalidation with guarded readback, and record removal. Build/native/live
results are recorded separately in the parity worklog.

## Existing calendar-date editing

The captured ordinary update path is B `edrK` at 7303763 → `sr7+` at
7266092 → `uDYD` at 7312962. It submits existing canonical `row_id` and
`column_id` through `lists.cells.update`, with `date: ["YYYY-MM-DD"]` or
`date: []` to clear. `date` and `due_date` use this representation;
`todo_due_date` does too when `shouldUseTimestamp` is false. The timestamp
branch is distinct and preserves numeric zero. Do not map Later to a calendar
date or send both representations speculatively.

B `hh3m` at 18140064 (`DateFieldEditor`) forwards the date-picker value directly
to its update callback. S `iLfW` at 4614869/4617391 defines the input format as
`YYYY-MM-DD`; column `options.dateFormat` only changes the displayed format.
Calendar writes therefore require no UTC conversion or midnight timestamp.

The native typed editor adds calendar-only values to the existing Save/Cancel
lifetime. It requires a verified ordinary, non-personal List, current write
permission, an unlocked document, and the same canonical schema, record, and
account/visit authority checks used by other typed edits. Valid singleton ISO
dates and empty values are supported, including the observed ordinary stored
shape with `timestamp: [-1]`. Timestamp-only values, Later, conflicting
timestamps, date ranges/multiple dates, and malformed dates remain read-only.
No personal-task timestamp editor or reminder side effect is introduced.

The native date input preserves incomplete browser input until Save. Save checks
browser validity and then core calendar validation; an incomplete date must not
silently become a Clear operation. The Clear control changes only the local
draft, and explicit Save submits the empty date array. Backend uncertainty still
requires readback without replay. Date edits can change saved-view membership;
the selected view is reprojected and focus returns to a surviving visible cell.
Record details expose an editable date even when its saved table hides that
column, using the separate detail authority.

The disconnected checker covers empty-to-leap-date, Clear, a hidden ordinary
todo-date edit, a stale draft, and a saved-filter disappearance with focus
recovery. Its invalid-input check injects browser validity state to exercise the
Save guard; it does not claim that assigning an invalid string reproduces a
native calendar's segmented typing behavior. Native screenshots capture both a
calendar editor and the hidden-field editor at normal/narrow window sizes.
These additions require the root-owned build and native gates; source inspection
and fixture code alone do not establish live server clear/readback behavior.

## Saved date filters and sorting

The implementation is in core `slack_lists/saved_views/dates.rs` and desktop
`slack_lists/date_context.rs`. It handles `date`, `due_date`, `todo_due_date`,
`created_time`, and `last_edited_time` in saved-view filters and ordered sorts.
This section describes read projection; the calendar-only editor above is a
separate write path. Saved-view projection does not write
saved view definitions. The details below describe source-backed behavior and
implemented boundaries; completed gate receipts and the exact private live scope
are recorded in the parity worklog under Saved-view date projection.

### Cell decoding and timestamp policy

S `iLfW.getTypedFieldValue` at 4611438 consumes `date: ["YYYY-MM-DD"]` and
`timestamp: [number]`. Without timestamp mode, a nonempty date array wins; a
timestamp-only cell retains its number, including `-1`. In timestamp mode a
timestamp wins unless it is `-1`: that sentinel falls back to a supplied date,
or becomes empty when no date is supplied. Numeric zero is the separate Later
value and is nonempty. A date plus `timestamp: [-1]` therefore preserves the
calendar date in both modes.

The mode is not inferred from a column name, `todo_mode`, or the presence of a
timestamp. B `l2X+` at 7287618 passes S `nxvY.yJ` into the record decoder.
S `nxvY` at 3794808 defines that selector as `todos` on, `lists_todos_rollback`
not on, and a personal task List. S `j2uc.P` at 2614000 identifies the latter by
exact equality with `assigned_to_list` or the **first** `task_lists` entry.
Verified ordinary Lists use date-first decoding. Unknown policy rejects cells
whose interpretation depends on that policy; failed metadata reads are not
treated as verified off assignments.

Ordinary table projection at B `VBDB.useGetRecords` around 2815298 spreads
decoded fields and then supplies `created_time` from raw `record.date_created`.
It does not copy the decoder's `assignedToMeReminderTimestamp` into those rows.
The generic filter's reminder override therefore does not authorize applying
raw `record.date_due` to every ordinary date column. Native projection follows
the ordinary table path and the canonical created-time override.

### Filter semantics

B `+E6Z` at 2846400 supplies the comparison/filter helpers; B `LsSA` around
2823190 enumerates presets. Dates use the machine's local calendar, not UTC day
boundaries. Supported presets are `today`, `yesterday`, `tomorrow`, `this_week`,
`next_week`, `last_week`, `upcoming`, `overdue`, `later`, and
`since_i_last_viewed`, alongside explicit ISO calendar dates. Weeks run Monday
through Sunday using `isoWeek`.

- Empty/not-empty operators run before ordinary comparisons. Missing values
  are excluded from other date operators, including negations. Zero remains
  nonempty. A missing or falsy first filter choice (`null`, `false`, `0`, or
  `""`) bypasses the leaf; later falsy choices are skipped.
- Multiple choices use OR, including `not_equal`; they are not interpreted as
  a set to exclude. Ordinary comparisons normalize both sides to local midnight.
  Numeric record timestamps are interpreted as Unix **seconds** for filters.
- Later equal/contains tests numeric zero; its negations test nonzero. Relational
  operators against Later do not match. Upcoming uses a boundary two local days
  after today: equal/contains is strictly after, and its negations strictly
  before. The boundary day matches neither side.
- Overdue compares against the current instant, excluding zero. A date-only
  value for today represents midnight and can be overdue at noon; a numeric
  timestamp later today need not be. Negation uses strictly after now, so exact
  equality matches neither side.
- If the **first** choice is `since_i_last_viewed`, the caller forces strict
  greater-than and uses exact instants rather than day normalization. The
  retained `lists.open.last_read_timestamp` is Unix seconds. Missing prior-read
  evidence produces an explicit unsupported result rather than substituting
  the current time. A later since-viewed choice does not activate that first-choice
  override.
- Captured week `not_equal` has a distinct branch: it compares week midnights
  with the record instant without normalizing the latter. A numeric noon within
  the week can satisfy both week equal and week not-equal. Native code preserves
  this observed behavior. The week array branch does not use the exact-time
  override of the scalar since-viewed comparison.

`todo_due_date` filtering remains subject to the captured todo-mode gate;
completion visibility additionally keeps its independent personal-task policy.
Date support does not bypass either policy or enable personal completion writes.

### Sort semantics, refresh, and remaining boundary

B `applySortToRecords` at 2852950 uses `new Date(c)` for date sort keys. ISO
calendar strings consequently sort by UTC midnight; numeric values are treated
as **milliseconds**, unlike the filter's seconds interpretation. Native code
preserves this source asymmetry. Ascending puts valid dates before missing
values and Later; descending uses the captured missing-zero/Later-negative-infinity
sentinels. Other sort keys and stable incoming-order ties are preserved.

Desktop projection caches an explicit local calendar and prior-read context.
Only a visible, owned List with a relevant selected filter samples time, at a
60-second cadence. Calendar-dependent views conservatively reproject each sample,
even when the current day and UTC offset are unchanged: two system timezones
can share today's offset while having different historical or future rules.
Overdue also reprojects as the sampled clock advances. Arrival of prior-read
state immediately reprojects dependent views. These local changes do not fetch
records or write Slack state. Deterministic offline fixtures supply a fixed
local offset and exercise Today, ISO week sorting, Later, multi-choice negation,
strict prior-read boundaries, and local-day transitions through the native picker.

The decoder deliberately supports singleton ISO date/integer timestamp cells;
multiple dates/ranges, malformed values, unsupported calendar bounds, and
ambiguous or unavailable local midnights are explicit unsupported cases. Slack's
broader Dayjs parsing/coercion behavior is not claimed. The ordinary table path
does not model My Items reminder overrides or newly inserted `bottomRecordIds`
filter exemptions. Projections still cover only loaded records and preserve the
incomplete-page notice. DST behavior, personal-task reminder surfaces, and the
full preset/operator matrix require separate live verification; fixed-offset
fixtures alone do not establish them. Gate receipts belong in the parity worklog.

## Ordinary completion versus personal To-dos

B `CheckboxFieldInput` at 17692999 and the ordinary table toggle at 18005710
use the common boolean updater: `edrK` at 7303763 → `sr7+` at 7266092 →
`uDYD` at 7312962. The ordinary path adds no date/assignee write. The personal
branch in `Wd1o` at 18322774 uses `c8rH` at 7308059, which can also change a
referenced original List record. Native single-cell completion excludes that path.

S `j2uc` at 2614000 identifies personal Lists using `assigned_to_list` or the
first entry of `task_lists`; `todo_mode` is not the discriminator. S `4iNl`
at 5317004/5318233 reads these authenticated user preferences. S `Nu4c` at
1445350 and P `0WGR` at 1807374 establish `users.prefs.get` with
`prefs=task_lists,assigned_to_list`, consuming `response.prefs`. Native reads and
write preflight fetch this policy afresh. Missing/malformed preferences leave
completion variants read-only; explicit verified empty preferences are distinct.
The live probe successfully parsed this policy and classified the observed private
List as non-personal (`tmp/slack-list-live-read-policy.log`). A later checkbox
round trip on the new private verification List succeeded and restored false.

Offline fixtures exercise synthetic file/record data and synthetic mutation
responses through the domain/controller and native checker. They do not establish
that Slack returns the assumed successful cell response shape. Native normal/narrow
interaction checks and state captures passed offline; separate signed-in checks
verified checkbox, rich-text, number, and single-select readback. Execution receipts
and remaining gaps are recorded in the parity worklog. Remaining protocol gaps
include cursor continuation, partial-success/conflict semantics, schema realtime
ordering, and live multi-select compatibility.

## Workspace permission source

For standalone workspaces the permission is `teamPrefs[active_team].allow_lists`,
rather than user boot preferences. Captured S module `/0DE` at character 5299264 consumes it; S `mL3A`
at 5300600 and P `PAdb` at 1805069 establish `team.prefs.get` with
`targetTeam` (wire `target_team`) and `response.prefs`. Native reads and write
preflight request it independently and require the exact string `full` before
an ordinary list is editable. Missing or failed permission lookup remains read-only. This avoids
assuming that file write access alone grants workspace Lists editing.

Enterprise sessions instead use `enterprise.prefs.get` with `_x_reason` and no
`target_team`, consuming the top-level `locked_prefs.allow_lists` string. S
`nxvY` at 3797739 switches through the enterprise predicate `tpNE` at 3788507
to `CtFl.SI` at 5278326; P `mabH` at 1791845 supplies the endpoint. S
`fetchEnterprisePrefsData` at 1033724 and B `fetchAndUpsertOrgPrefs` at 10055578
establish the request and response shape. No fallback to `org_prefs` or
`unlocked_prefs` grants editing. Both imported E-prefixed identities and explicit
enterprise IDs choose this branch.

The live probe initially exposed `invalid_arguments` from the old
`team.prefs.get` request for an E-prefixed session. The production enterprise
branch then returned `allow_lists: "full"`. The same guarded probe parsed a
four-column List with three complete records, including `todo_completed` boolean
cells. Its reusable entry point is `examples/slack_list_read_probe.rs`; it checks
private-channel identity/membership before reads and normally emits structural
metadata, not credentials or cell contents. An explicit `--first-row-rich` switch
is restricted to the new verification List and emits its first named test record
to investigate normalization. Evidence: `tmp/slack-list-live-read-fixed.log` and
`tmp/slack-list-live-rich-{readback,bold-readback}.log`. Slack assigned a root
`rich_text.block_id` on first save, causing a false mismatch under literal JSON
comparison; the visible content and subsequent bold style were preserved.


### Authenticated experiment policy for native saved views

Captured `fetchExperimentsData` in S at 1028162 POSTs
`experiments.getByUser` and consumes `response.assignments`. The July capture's
complete response has `ok`, `assignments`, and `config_version_ts`; each
assignment carries a `group` string. This is a separate fetch from userBoot.
The native adapter retains only `lists_grid_todos`, `todos`,
`lists_todos_rollback`, and `lists_default_view_dual_write`, comparing each group
to `on`. The separate `todos` assignment controls personal date timestamp mode;
it is not interchangeable with `lists_grid_todos`. A missing flag in
an otherwise valid assignments map is off; a missing map, malformed target
assignment, or failed fetch is unknown. Unrelated experiment payloads and
exposure identifiers are discarded. The native request supplies the captured
platform/release fields, without inventing a Slack web build version.

A current authenticated read against the private verification workflow succeeded
on 2026-10-13: grid todos and default-view dual write were on; rollback was off.
Only these three booleans and the known/unknown outcome were printed in
`tmp/slack-list-saved-view-policy-probe.log`. The native integration must preserve
account/visit ownership and must not interpret a failed lookup as all flags off.
