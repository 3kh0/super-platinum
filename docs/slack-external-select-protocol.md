# External select protocol

Source audit and bounded implementation record for `external_select` and
`multi_external_select`, recorded on 2026-10-14. Single external core and desktop
support passes the 1,218-core/1,155-desktop gate and all four Windows native
fixtures on `df0017fa...`, with all 38 captures inspected. See the worklog for
the retained initial failures and exact evidence. Multi-external core and
desktop implementation is now under separate verification, described below.
No live Slack session or app
callback was used to establish these contracts.

## Primary sources and provenance

Public authoring and callback contracts:

- [Single-select element](https://docs.slack.dev/reference/block-kit/block-elements/select-menu-element/)
- [Multi-select element](https://docs.slack.dev/reference/block-kit/block-elements/multi-select-menu-element/)
- [Block suggestion payload](https://docs.slack.dev/reference/interaction-payloads/block_suggestion-payload/)

These documents describe an app's Options Load URL and the Slack-to-app
`block_suggestion` payload. That callback is not the desktop client's request
endpoint. The client endpoint below was recovered separately from Slack's public
JavaScript bundles.

| Alias | Public bundle | SHA-256 |
| --- | --- | --- |
| B | [Shared boot](https://a.slack-edge.com/bv1-13/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js) | `1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537` |
| V | [Async vendors](https://a.slack-edge.com/bv1-13/gantry-v2-async-gantry-v2-vendors-async.daa946d0953564d3.min.js) | `8904e661fc5858d74b7e2f11400b2f1f0f0f746cf188a74fa3bc8db4de7a1362` |
| S | [Shared](https://a.slack-edge.com/bv1-13/gantry-v2-shared.0b81595876785d7a.min.js) | `f085df09fb4f938caf425ea66fb0244c8f92a8012eea8eb298e4f629b377dbf7` |

Local audit copies are in `tmp/sb/js/`, under those exact filenames. Offsets
below count decoded JavaScript string characters, not compressed bytes. The
bundles and their temporary excerpts are not repository source files.

| Bundle / module / offset | Observed behavior |
| --- | --- |
| B `IKXd` 13235009 | `ExternalSelectElement`: single selection, query lifecycle, display, confirmation plumbing, action shape. |
| B `JQ84` 13268961 | `MultiExternalSelectElement` and outer `MultiExternalSelectInput`: retained options, query lifecycle, staged ordinary selection, inline input selection. |
| B `urb3` 4145104 | Shared action and external suggestion fetchers; suggestion builder is near the end of this module. |
| V `lebZ` 9731283 | `PJ` resolves to `blocks.suggestions`. |
| S 1571082 | `blocks.suggestions` occurs in the exclusion list consumed by `shouldRetryMethod`. |
| B `ViewBlocks`, around 12690600 | Modal service/app/team and view-container routing. |
| B `hKGv` 1746259, followed by `hA8x` | Option/group formatting preserves `originalOption`, description, label, and value. |
| B 1721118 / 1728320 | External single/multi wrappers pass container action context and input state. |
| B 1743891 | Saved state resolves `selected_option` / `selected_options` into component props. |
| B 1751241 / 1753208 / 1755068 | Initial state, ordinary action state, and full-state selected-option projection. |

## Observed desktop request and response

Both selectors dispatch the shared `urb3` external suggestion fetcher. It
camel-to-snake converts the supplied context and calls `blocks.suggestions` with:

```text
service_id
app_id                 included only when truthy
service_team_id
function_execution_id  when available from the container context
container              JSON string
action_id
block_id
value                   query, without client-side normalization here
```

The fetcher supplies `abortSignal` to the API layer. It does not add `state`,
`client_token`, a page size, cursor, offset, or page number. It does not call the
developer's Options Load URL directly.

The response mapper returns `optionGroups` from a truthy `option_groups` field
first; otherwise it returns `options` from a truthy `options` field; otherwise
it returns an empty options array. Empty JavaScript arrays are truthy. Group
and option order are preserved by the presentation mapping. Selection retains
the original option object rather than reconstructing it from label/value.

The public single-select reference specifies at most 100 options, or at most
100 groups with up to 100 options per group. The multi-select reference also
documents external option groups. No inspected source or public schema supplies
a server-pagination contract for this control. A future implementation should
not invent one.

The S bundle explicitly excludes `blocks.suggestions` from automatic retries.
Suggestions are therefore not safely classified as automatically replayable
merely because they retrieve options. This evidence does not establish server
idempotency, callback side-effect behavior, or a guaranteed cancellation boundary
once Slack has received a request.

### Modal routing

`ViewBlocks` constructs:

```text
serviceId:     view.botId
appId:         view.appId
serviceTeamId: viewContextTeamId || view.appInstalledTeamId
container:     { type: "view", view_id: view.id }
```

That context reaches the same suggestions fetcher. No view hash or full form
state is added by this path. Local stale-response checks should still bind the
current view revision/hash; absence from the wire is not permission to apply
results to a replacement view.

## Observed query and selection behavior

Both components default to `minQueryLength: 3` and a trailing 500ms debounce.
Query length is evaluated with JavaScript string `.length`. Search below the
threshold does not dispatch. The single selector searches without debounce on
focus when no options are loaded; the multi selector uses its debounced search
for that case. Both still enforce their threshold, so opening a default-minimum
control with an empty query need not send a request. A zero minimum allows it.

Each issued search aborts the previous controller; unmount aborts and cancels
the debounce. Accepted results update the options list, not the retained
selection. A selected item need not occur in the newest query result. Errors
clear loading and are caught; this audit does not establish a product retry UI.

Single selector:

- Constructor prefers a defined saved option, otherwise the initial option.
- A changed initial option updates mounted selection, including removal.
- Selection emits `selectedOption` containing the original option or `null`.
- A supplied truthy initial option and placeholder are included in the action.
- Confirmation is passed to the select component. Optional input blocks allow
  clearing; ordinary behavior should not be inferred from modal optionality.

Multi selector:

- Inner constructor uses saved options, initial options, then `[]`.
- Inner mounted state resets for a changed truthy initial array; `[]` is truthy.
- Selected options are retained separately across queries and normalized back
  from presentation wrappers to original option objects.
- Ordinary selection uses the shared multi dialog with submit/cancel and
  confirmation props. Cancel restores the previously selected options.
- Input blocks use inline local changes. Over-maximum input selections are
  forwarded for validation; ordinary submit is blocked while over maximum.
- Action payload carries ordered `selectedOptions`, plus supplied initial
  options and placeholder. Full-state projection uses `selected_options`;
  single selection uses `selected_option`.
- Source defaults ordinary requiredness to false. Modal input-block requiredness
  must be handled separately.

### Multi external outer/inner state trace (resolved source audit)

This section is a read-only source trace, not implemented multi-select support or
native observation. B `JQ84` begins at 13268961. Its two separate state owners
must not be collapsed into the multi-conversation component's update rule.

| Owner / exact offset | Direct source behavior |
| --- | --- |
| Inner `MultiExternalSelectElement` constructor 13271480 | `selectedOptions = savedOptions || initialOptions || []`; `previousSelectedOptions` captures that result once at construction. Empty arrays are truthy. Initial invalid flag is false. |
| Inner `componentDidUpdate` 13272320 | A changed truthy `initialOptions` resets selected options. `[]` resets; null/removal does not. A selected-array change calls `onChangeSelectedOptions`, updates selected count/invalid state, and calls submitAction when inline. Saved-option prop changes alone do not reset it. |
| Inner `setSelectState`, immediately before 13273326 | Converts formatted choices back through `originalOption ?? choice`, preserving original raw options. Query changes independently schedule the existing debounce. |
| Inner `cancelAction` 13273326 | Calls the outer change callback with the fixed opening snapshot and restores the same snapshot locally. It does not invoke submitAction directly. |
| Inner `submitAction` 13274459 | Emits ordered `selectedOptions`, adding truthy supplied `initialOptions` (including []) and placeholder. Over-max blocks ordinary action; input parent still propagates local state for form validation. |
| Outer `MultiExternalSelectInput` 13278069 | Constructor stores only `props.initialOptions`, not savedOptions. Ordinary trigger count uses this outer state. |
| Outer update 13278523 | Every deep change of the original initialOptions prop resets outer state, including null/removal and []. |
| Outer open 13278648 / render 13278860 | Opens a new inner element, passing `onChangeSelectedOptions`, all current props, then overriding inner initialOptions with outer selected state. Outer change callback strips formatted wrappers through originalOption. |
| S `n8uB` 2807150 | openModal clones/stores the supplied React element. Updating that open element requires its returned explicit render handle; this outer opener does not retain/use that handle. Thus ordinary opening captures inner props, rather than subscribing the open picker to future outer renders. |

Combined behavior implied by those directly read methods:

- Fresh ordinary trigger displays original initials only. Opening the picker
  uses saved non-null arrays first, otherwise the outer selection, otherwise [].
  A saved [] outranks nonempty initials; saved null is coalesced to absent by
  B `iGIl` 1743891. Merely opening with a different saved array does not invoke
  the selection-change callback, so trigger and picker can temporarily differ.
- Ordinary staging changes update the outer trigger preview via the callback,
  but do not call the container action/state wrapper. Explicit picker Cancel
  restores the opening snapshot into both owners and sends no action. Applying
  invokes the selected-array action, after any confirmation.
- While an ordinary picker is open, remote initial changes affect the mounted
  outer trigger, not its already-open snapshot element. A native implementation
  should retire that stale visit instead of allowing its old callback to commit
  into a newer canonical element. This is this client's ownership guard, not a
  claim that the captured component enforces that retirement itself.
- Closed ordinary initial updates/removal reset the trigger, while committed
  shared state remains separate. Reopening again gives saved arrays priority.
  With no saved array, removal/null yields [] on the newly constructed inner;
  with saved [] it stays explicitly empty.
- Inline input keeps the inner mounted. A changed supplied array (including [])
  propagates outer -> inner. Removing/nulling initial resets the outer state but
  fails the inner truthy update gate, so the visible inner selection stays put.
  Saved prop changes alone also do not reset that mounted inner. Full remount
  re-runs saved -> outer initial -> [] precedence. Dirty native form retention
  and conflict rejection must remain separate from pristine source resets.
- Max validation is recalculated in the inner when selected state changes; a
  max-only prop update does not itself execute that recalculation. The bounded
  native implementation must validate the current maximum at Apply/submission,
  retain over-limit arrays for correction, and not reproduce an invalid-submit
  opportunity from this source lifecycle detail.

#### Cancellation versus confirmation cancellation

B `+RSw` immediately after JQ84 (13279489) passes submitAction as dialog
onGo and cancelAction as onCancel. B `Te7m` WithConfirmDialog at 13137439 injects
multiConfirmProps whose speedbump appears only for the Go operation.
B `FlDO` renderSpeedBumpMaybe at 15057765 makes confirmation Cancel only hide
that speedbump; the picker stays open with staged choices and no action has been
sent. Confirmation Continue calls onGo, which submits and closes after completion.
The explicit picker Cancel is the separate rollback operation described above.

FlDO's onRequestClose (15052569) and Escape route call onClose/closeModal, not
onCancel. JQ84/+RSw supplies no rollback onClose. Consequently the inspected
Escape/X/overlay-close route can leave the outer preview staged even though no
shared state was committed. A later reopen still prefers saved arrays, or that
preview when saved state is absent. Do not describe every dismissal as source
rollback. A bounded native slice may deliberately route all dismissals through
its local rollback, but must document this safer UI difference and keep it free
of remote writes. Exact OS focus/overlay behavior remains unverified.

#### Modern input wrapper and raw state

B `yIlz` 1728173 maps normal input parents to inline multi selection and reads
savedOptions from input saved state. It maps each saved option through Q1qH.Ag.
B `Q1qH` 1737114 projects saved options to text/value (mrkdwn text becomes plain
text with emoji true; missing value becomes empty string), omitting description.
The generic Te7m/iGIl path passes saved option objects directly. These are distinct
wrapper projections, not evidence that suggestions or committed action options
should be reduced to value-only identities. A first bounded native slice should
retain exact raw initial/accepted option metadata and explicitly document any
saved-state projection difference, rather than guessing which missing metadata
may be reconstructed.

B `cxJV` 1750345 includes initialOptions in selected_options when truthy, so []
is retained. B `AM6q` 1754021 projects selectedOptions directly to selected_options,
including [], with no response URL flag. Full sibling state remains separate
from trigger preview and picker staging. B `we8C` 1751905 filters empty arrays
in its ordinary initial/current helper, while AM6q retains explicit clears;
implementation must use the existing proven container-state ownership path,
not substitute that helper for full committed state.

#### Smallest proposed implementation (not authorized by this document)

Reuse single external routing, bounded suggestions transport, raw ExternalOption,
accepted-response catalogs and stale-query guards. Add a distinct multi external
kind with min query/max count and an ordered Vec<ExternalOption> value; array []
is clear, incoming saved null is absent. Ordinary selections stage locally until
Apply + confirmation; confirmation Cancel returns to the same staging. Retain
outer display versus committed wire separately, invalidate open visits on exact
canonical changes, and explicitly record any normalized dismissal rollback.
Normal modal input is submit-only with block optional authority and over-max
arrays retained for correction; confirmation/dispatch/visibility/specialized
contexts remain unsupported. Preserve the inline removal/null reset distinction
for pristine fields, dirty clear/reconcile rules, and fresh-remount precedence.
No new API endpoint, pagination, response flag, option membership in newest query,
or automatic retry is supported by this trace.

Before enabling multi, focused regressions should cover saved missing/null/[];
closed/open/inline initial changed-array/removal/null/[]; staged preview versus
wire; explicit Cancel and confirmation Cancel independently; remount with and
without committed saved arrays; current-max changes; original option metadata;
and exact full sibling state. This source audit resolves the wrapper-state
question but does not establish real Slack callback or native interaction proof.

## Repository boundaries before the single-select implementation

The original audit found:

- `src/desktop/blocks/controls.rs` preserves unavailable external controls'
  initial labels; `view/rich.rs` has no actionable external-selector branch.
- `core/block_actions/form.rs` has no external kind/value retaining dynamic raw
  options. `InputValue::Selected(Vec<String>)` depends on static options.
- `form/parse.rs` has reusable option-shape parsing, but
  `form/message_multi.rs` requires initial options to belong to a static list.
  That membership requirement cannot be used for remote initial selections.
- `message_buttons/inputs/multi.rs`, `inputs/display.rs`, and `source/action.rs`
  provide staging, display ownership, confirmation and final-dispatch boundaries.
- `app_modal/users.rs` provides query/session guard patterns. Its directory
  ranking, member/channel eligibility and canonical workspace merging are not
  valid policies for arbitrary app-defined option values.
- `slack/api/block_actions.rs` has no suggestions request builder.
  `slack/transport.rs::is_block_interaction_request` does not yet allow
  `/api/blocks.suggestions`. `PreparedRequest::retry_safe` currently does not
  mark this method retry-safe and should retain that property.

Here `core/` abbreviates `crates/super-platinum-core/src/`.

## Implemented bounded selection boundary

Core implements single and multiple external selection in ordinary message
section/actions controls and ordinary modal input blocks, using typed suggestion
requests and exact raw options. Multi-external core tests pass in the 1,235-test
gate. Desktop multi selection is implemented and under verification; its first
full suite has one new assertion-wrapper failure being corrected, and native
fixtures remain pending. Single-select native proof remains separate. Desktop
owns accepted-response provenance, query and credential/transport lifetimes,
rendering bounds, and final eligibility.

Local restrictions:

- Start with plain-text option labels/descriptions and group headings, retaining
  original option JSON. Explicitly reject unsupported rich option shapes.
- Validate bounded arrays and payload size; reject malformed groups and
  conflicting duplicate values. Preserve server order rather than applying
  member-directory ranking or a second local query filter.
- Bound rendered results without implying server pagination.
- Preserve initial/saved selections independently of the latest query result.
  Values are scoped to their app, container and field, not global identities.
- Use bounded requests, explicit retry and a `Retry-After` cooldown. Never
  automatically replay suggestions or final actions after ambiguous completion.

Ownership requirements for this client:

- Capture account epoch, team/self, current credentials and transport, app and
  service routing, exact container/element revision, picker visit and query
  revision before asynchronous work.
- Modal searches additionally bind session generation, view ID and current
  view revision/hash. A same-ID replacement is still a new owner.
- Reject stale results before changing options, selection, errors or cooldown
  presentation. Close, navigation, reopen, account replacement and credential
  rotation retire the captured request authority.
- Revalidate the exact option proof and source before Apply, confirmation and
  dispatch. Query changes may replace visible results without deleting retained
  selected proofs. Another app or field cannot lend a matching value authority.
- There is no documented conversation filter on external selects. Query and
  element-configuration changes are the relevant replacement races; do not
  transplant channel/member filter policy.

## Focused acceptance work still needed

- Exact request fields and response precedence; no state/client token/cursor;
  `retry_safe == false`; endpoint allowlist and bounded failure behavior.
- Minimum zero/default three, UTF-16 query length, debounce, abort and stale
  response guards; deletion back below the threshold.
- Options/groups, empty responses, malformed shapes, conflicting values and
  initial options absent from the current result.
- Selected options across queries; saved null/empty state; initial prop updates,
  Cancel and remount, especially the multi outer/inner distinction.
- Exact single/multi action and full sibling state; confirmation cancellation
  without commit or dispatch; no duplicate final dispatch.
- Modal dirty/pristine updates, optional clear, maximum reduction/correction,
  field replacement, account/transport/view replacement and 429 cooldown.
- Normal/narrow disconnected captures for short-query guidance, loading,
  grouped results, selected/cleared, confirmation, errors and uncertain action
  completion. Synthetic fixture input does not establish trusted OS keyboard,
  IME, live app callback correctness or Slack delivery.

Live response envelopes, undocumented service-specific option shapes, callback
side effects, and server pagination remain unverified. This document does not establish live app delivery or callback correctness.


## Exact normal-modal service routing

Additional public-bundle trace, using the B/S URLs and SHA-256 hashes above:

- B `5Fwt`, `BlockKitModal.renderViewContent`, 12783702 passes the current view,
  its ID, modal container context and action/confirmation callbacks to ViewBlocks.
  It does not supply `viewContextTeamId`.
- B `gEn+`, `ViewBlocks`, 12690765 reads `view.botId`, `view.appId`, and
  `viewContextTeamId || view.appInstalledTeamId`; its container is exactly
  `{type:"view",view_id:view.id}`. The normal modal caller therefore resolves
  service team from the view's installed team, not the active workspace team.
- S `FGXB` fetch at 1341034 converts the returned raw view through `ijf8.A`;
  submission's updated view does the same at 1341889. B `NGtd` view-open handling
  at 5743202 also applies that conversion.
- S `ijf8` module 4038745/function 4038786 shallow-camel-cases top-level keys
  (`maxdepth:0`). Raw `bot_id`, `app_id`, and `app_installed_team_id` become the
  normalized routing fields above. Nested block payloads retain their raw keys.
- S `67Zz` 2060212, `WHV9` 2061351 and `PgoZ` 2080354 store/select the view
  without adding a service-team fallback.

`api::SuggestionsContext::modal` validates the current hydrated ordinary view,
matching view/app IDs and the selected normal input field, and requires raw
`bot_id` and `app_installed_team_id`. Missing routing is unavailable until a new
validated view supplies it. It does not infer routing from private metadata,
conversation origin, payload team ID, current workspace or active channel.
Ordinary modal ViewBlocks does not forward `function_execution_id`; that field
is included only on the proven ordinary-message route when present. Modal hash,
stack generation, credentials and transport freshness remain desktop guards.

## Raw option, state and transport implementation details

- `block_actions::external::ExternalOption` has private validated raw JSON and
  read-only accessors. It accepts plain-text text/description, value and optional
  emoji metadata only; no URL or service-specific rich option extensions.
  [Text compositions](https://docs.slack.dev/reference/block-kit/composition-objects/text-object/)
  require at least one character. Labels/descriptions/group headings are bounded
  to 75 UTF-16 units and placeholder text to 150. Option values are strings of at
  most 150 UTF-16 units; the official option reference gives no minimum, so an
  empty value is a real option object and is never conflated with null clear.
- Direct results are bounded to 100 options; groups to 100 with 100 options each.
  Order and exact original option metadata are retained. Conflicting duplicate
  values within one response reject the response; identical duplicates retain
  their original order. Present empty groups take precedence over options.
  Non-null malformed groups/options, including false/0/empty-string groups, are
  explicitly rejected by this bounded client. This is stricter than JavaScript
  truthiness, not a claim of matching malformed-response coercion.
- `InputKind::ExternalSelect` / `InputValue::External` do not require static
  option-list membership. Core guarantees shape and exact action/full-state raw
  equality; desktop must attest canonical initial/saved or accepted-response
  provenance for each app/container/field and reject conflicting newer metadata.
  An option omitted from a later query remains selected and valid.
- B `iGIl` 1743891 resolves `savedOption ?? state.selected_option ?? undefined`.
  Incoming persisted null therefore falls back to `initial_option`; with no
  initial it is empty. B `IKXd` 13235009 distinguishes undefined from a selected
  original option and resets mounted selection whenever initial changes,
  including removal/null. Native local dirty optional clear remains explicit
  null through compatible modal reconciliation and outgoing full state. A new
  incoming mount again applies the wrapper fallback; local null is not claimed
  to override an initial forever. Message committed wire and mounted display
  remain independent, including changed/removed initial options.
- Ordinary interaction includes full raw selected option or null and supplied
  initial/placeholder/confirmation metadata. Modal input requiredness belongs
  to input.optional. Ordinary UI does not offer Clear; optional modal inputs do.
  Modal confirmation, dispatch-on-change, visibility and specialized view types
  remain unsupported. Service-specific element specializations remain explicit
  unsupported boundaries.
- `api::block_suggestions` preserves the exact query and fields proven above,
  imposes a local 4096-UTF-16-unit query budget, and verifies workspace team/self
  against captured context. It adds no state, client token, hash, cursor or page.
  `decode_external_suggestions` requires an affirmative ok envelope.
- Transport permits the exact POST suggestions path on the bounded interaction
  path, with a 4 MiB streaming cap before JSON decoding (declared-length and
  streamed overflow checked). This is a local safety budget, not a Slack limit.
  Network/body/decode errors do not log option bodies. Automatic retry remains
  disabled, including 429; Retry-After is returned for desktop cooldown.

Focused tests cover UTF-16 limits using explicit Rust astral escapes, empty
value versus malformed empty composition, groups/ordering/conflicts, persisted
null fallback, local dirty clear, required and dynamic raw state, full siblings,
initial removal with retained committed state, current capture/owner/raw equality,
strict modal route, query shape/no retry, 429 single attempt and both byte-cap
paths. Their execution and native visual evidence are recorded in the worklog's
**Single external selector native gate complete** entry. This does not establish
live app callback interoperability. Multi-external implementation and its
separate verification are described below.


## Multi-external implementation and bounded Windows verification

Ordinary controls separate the opening preview, ordered staged selection and
committed wire state. Saved arrays take precedence, including explicit empty
arrays; missing/null use the source-backed fallback. Apply validates current
maximum, requiredness and exact provenance before confirmation/dispatch. Explicit
picker dismissal rolls back opening preview; confirmation Cancel retains staged
choices. Applying an empty optional selection sends `selected_options: []`, with
all sibling state. All dismissals use rollback deliberately; do not infer exact
Slack implicit-dismiss behavior from that local policy.

Modal selection edits the local form inline and does not dispatch block actions.
It retains empty required and over-limit arrays for correction and validates on
submission. Each edit advances field identity to prevent removal/re-add ABA and
retires outstanding query authority while retaining accepted rows and cooldown.
Exact raw saved option metadata is retained deliberately, unlike the modern
Slack wrapper's text/value-only saved projection documented above. This bounded
metadata difference is not claimed as exact wrapper parity.

Core accepts at most 1,000 unique selected values, a local safety budget rather
than a claimed Slack limit. Unsupported rich options, service-specific contexts,
server pagination and live callback interoperability remain required/unverified
boundaries. All four normal/narrow ordinary/modal fixtures pass on executable
`e3cb05e350c98981a914e6313da17cee3a9c87cf130c2dbcd405ce971e4d4122`,
with all 50 PNGs inspected in `windows-multi-external-select-verified/`.
Exact raw requests preserve ordered options, explicit empty arrays and sibling
state; loading is held through capture and saved-empty priority is asserted on
reopen. The narrow uncertainty toast clears the dialog and footer hit tests pass.
Narrow modal maximum/dirty-sibling copy is below the captured fold; native state
and raw receipt checks cover it, not those pixels. Closed-log identity/stages,
owned process exit and pointer restoration pass. This is synthetic disconnected
evidence, not live callback or trusted OS input proof. The subsequent legacy
single-external regression exposed a changed clear-button accessible label.
Restoring its established single-select label fixes the regression: all four
checks pass on `adaa29ff...`, with all 38 PNGs inspected. A subsequent gate on
`dd5a3aa0...` passes all four checks with 38 inspected PNGs in
`windows-single-external-held-loading/`: both narrow loading captions now remain
visible with no rows. Eight actual raw requests independently preserve exact
option metadata, optional null, and full sibling state. Each fixture records
two attempts and five queries. Empty-result preservation remains a native
assertion without a dedicated PNG. The legacy runner still lacks per-process
creation/exit, pointer and binary-SHA receipts; root's unchanged binary check
does not supply those missing lifecycle records. Narrow ordinary uncertainty
copy clears the composer but overlaps disabled background controls.
