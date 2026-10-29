# Slack parity continuation - 2026-10-27

The latest Amp checkpoint authorizes useful offline product work despite the
session-0 native blocker. Full user-visible parity remains open. Preserve the
committed polish batch and the historical 5,043/0/7 desktop baseline plus the
later focused reconnect/history results in `parity-resume-20261027`.

## Verified Files/date/recent-search desktop milestone

Focused **17 passed / 0 failed**, then full **5,051 passed / 0 failed / 7 existing
ignored**, using the same actual harness hash `a01afc2784e72f88f0e98f590120135d6581666accb566672ee3999f219999d2`.
All prior 5,043 passing statuses and the ignored set are unchanged; all original
92 failures are `ok`. Affected groups passed: Profile **257**, Canvas **421**,
sharing peer **6**, Home inputs **280**, List comments **330**, group recovery **22**.
No tests/assertions were removed or ignored. The focused rebuild took **631.88 s**,
minimum commit headroom **8.831 GiB**, sampled rustc private peak **17.031 GiB**;
no safety stop. Full gate **250.86 s** (226.94 s tests), unchanged desktop harness.

Mode: dev-fast/one job, explicit process-only `CARGO_INCREMENTAL=0`, same target,
linker and features. Six helper validation cases passed without actual Cargo.
The full source snapshot contains **5,816 inputs**; the green test executable is
retained as `desktop-5051-baseline.exe`. Evidence in
`tmp/parity-search-recovery-20261027/desktop-5051-milestone.json` and adjacent
focused/full result files. Targets **53.158 GiB**, disk **158.612 GiB**, commit
headroom **26.273 GiB** after the full gate; helper remains 56 GiB. Core retains
its independently locked **5,437/0/6 + 1 doctest** evidence. Native/live/layout
acceptance remains unverified; no production rebuild or native probe occurred.
The following milestones implement and verify Search outage recovery and names.

## Active Search outage-recovery slice

Recovery passed **24/24 focused desktop tests** and **5,058/0/7 full desktop**
with the identical harness, every prior 5,051 passing status and the ignored set preserved. A restored connection schedules failed Search recovery before
underlying resource-tab early returns. Admission retries only the failed page,
preserving rows, filter drafts, history visibility/MRU and conversation drafts.
Publication checks account/session/transport, raw query/dialog, kind/sort and page
cursor. A later reconnect does not retire an already admitted REST read; the
request lease quietly releases only its own still-live busy slot on retirement
or cancellation. No polling loop, live call or additional UI surface was added.

The shared message/file loaders use production request/decoder paths. Core
focused `search::` passed **102/102** (104.78 s, minimum commit **16.9 GiB**).
Amp found no concrete source bug and identified two coverage gaps; queued
pre-admission retirement and a held retired API error were added before the
single focused desktop rebuild. Seven new offline tests cover these paths,
page-one message and page-two file recovery, loaded-result preservation, the
actual reconnect entry before its resource return, later restore and disposal/
cancellation. All seven passed. The corrected focused rebuild took **551.02 s**, minimum
commit **8.83 GiB**; the full gate took **248.07 s** and reused the identical hash
`2f4d055994529ead601a36d2975a36c85016f64f4088db6fcdb5713dccc11a61`.
The first 283.12 s compile failed on internal visibility and test toast-field
references; both corrected without weakening assertions. The independent full
core gate passed **5,438/0/6 existing ignored + 1 doctest** in **651.03 s**.
Every prior passing status and the ignored set are unchanged; all 2,055 input
hashes match the frozen source snapshot. Source snapshot:
`recovery-batch-source-before-desktop-11.json` in the current evidence directory.

## Verified cold file-search uploader slice

Source gap and references are recorded in `file-search-uploader-gap.json`.
Names currently fall back to IDs when a user is absent or only partially loaded.
The new desktop-only pipeline reuses the established Edge users/info adapter,
completes file publication before optional metadata and batches unique unresolved
users at 100. Accepted page completion is explicit, and completed search identity/
cursor guards apply before every lookup and publication. Serialized snapshots
preserve intervening user changes; this is not a server-freshness guarantee or a
change-and-revert detector. The existing core partial-user merge remains in use.

Amp reviewed the plan; its refinements cover explicit completion, no-op typed
profile fetchers on injected test paths, no repeated per-user row scans and a
chunk test exceeding 100 unique unresolved IDs. Three new offline journeys passed in **27/27 focused** and **5,061/0/7 full
desktop**, preserving every prior 5,058 pass and the ignored set. Both runs used
hash `a72282d9bad44d3bbf04056eae98f1e8c6cf45bb0654004b8e7ec0d3d71af530`.
Rebuild **533.57 s**, minimum commit **11.014 GiB**; full **224.96 s**. The extra
cached-user display patch preserves admin/deletion flags and omitted status,
avoiding the core full-user merge defaulting sparse boolean fields to false.
Source and retained executable: `uploader-milestone.json`, `desktop-5061-source.json`
and `desktop-5061-baseline.exe` in the evidence directory. No core source change is part of this next slice;
the independently locked core full gate passed with its frozen 2,055-input snapshot.
Metadata errors keep file results usable. A newer page retires old metadata work;
a successful query/page retries all retained unresolved names. A failed newer
page can leave them unresolved until the next successful query/page.

## Verified message-search author slice

The existing loader checks only absent author IDs before awaiting users/info,
then inserts returned users wholesale. A cached ID-only author is skipped; an
intervening realtime user can be replaced by the held response. This is source
evidence. The actual-loader regressions now pass in the focused gate below.
`message-search-author-gap.json` records the source hash and planned proof.

Amp reviewed a desktop-only repair: share only unresolved-name, snapshot and
cached-name patch helpers with uploaders; keep request and completed-file scopes
separate. Inject the existing typed profile lookup at the actual message loader,
forward recovery's existing lookup callback, and keep query/decoder/pager/
projection timing. Four regression paths will inspect published author VMs,
retained roles/status, a held realtime update, query replacement and optional
lookup failure that still finishes the accepted page. No new endpoint/core source
or native/production run is needed.

Focused `bootstrap::search` passed **31/31** (four new actual-loader regressions),
rebuild **545.33 s**, minimum commit headroom **10.681 GiB**. Session/account
identity is mandatory for query publication; dialog/raw-query identity additionally
gates optional name updates and recovery. Ordinary closed-dialog result caching
remains supported. Amp's currentness follow-up is source-clear. The full gate is
running with the identical configuration/harness, SHA256
`38aaa1ab1696a0f09bef269ecc9b1e959acccd701943a3758f6cc30f0a974fc3`.
Full desktop passed **5,065/0/7** in **228.94 s** (220.97 s tests), preserving
every prior 5,061 pass, the ignored set and all original 92 repairs. The focused
and full harness hashes/mtimes are identical. The unchanged core retains its
5,438/0/6 + doctest evidence. Source/binary retained in `author-milestone.json`,
`desktop-5065-source.json` and `desktop-5065-baseline.exe`.
Evidence: `author-desktop-focused-17.*`, `author-desktop-full-18.*` and
`amp-message-author-currentness.review.md` in the evidence directory.

Next source-grounded slice: attachment-only
message-search preview text. `attachment-search-preview-gap.json` and Amp's plan
review rule out top-level files (already represented in body nodes), and isolate
the missing visible AttachmentVm text. Keep body precedence, typed extraction,
suppressed fallback placeholders and navigation identity. Implementation is in
focused compilation; no attachment-preview verification is claimed yet.

## Verified attachment-only Search preview slice

Search now falls back to visible typed attachment pretext/title/body/fields/files
only when the projected body is blank after highlight removal. Existing nonblank
body and top-level file/Canvas previews remain unchanged. Image/thumb-only cards
receive a neutral English label; author/service/footer metadata alone does not
become snippet content. Raw fallback strings and hidden URLs are not consulted.
The shared body-only extraction used by copy/transcript paths is unchanged.

Amp's source review found no blocker; its title+image, thumb-only, field-only
ordering counterexample is included before compilation. Three new regression
tests include the actual successful-query loader with two overlapping pages,
hidden-data sentinels, body/Block Kit precedence and preserved thread/navigation
identities. Normal/narrow fixtures are registered, not rendered or accepted.
Formatting caught a malformed test-only Unicode escape before compilation; it
is corrected. Expected ordinary-body whitespace was reconciled with the existing
core `message_text` trim before any test run. Focused `bootstrap::search` passed
**34/34** in **539.32 s**, minimum commit headroom **10.879 GiB**, no safety stop.
Full desktop passed **5,068/0/7** in **226.96 s** (218.90 s tests), same
configuration/harness, SHA256
`7ef2fa4d10baa1db3891ba3f5b60d9d70ba48ef01c1d382b6c70171373ad624b`;
every prior 5,065 pass, original 92 repair and ignored status is preserved.
Targets **53.156 GiB**, disk **157.356 GiB**, commit headroom **28.550 GiB** after
the full command. Evidence: `attachment-milestone.json`, focused19/full20 results,
`desktop-5068-source.json`, retained `desktop-5068-baseline.exe`.
Primary field reference: [Slack secondary attachments](https://docs.slack.dev/legacy/legacy-messaging/legacy-secondary-message-attachments/).
This supports message content, not exact current Slack Search rendering.

The following safe slice is planned in `search-conversation-gap.json` and
`amp-search-conversation-plan.review.md`: DM/MPIM Search headers should not have
a channel hash, and explicit unresolved IM peers should share the existing bounded
name lookup. Explicit result classification wins over cached fallback; decoded
false flags do not prove absence. No peer is inferred from a channel name, ID
prefix, author or guessed membership. Current workspace metadata is consulted
separately for lookup and final projection, without canonical channel writes.
The attachment gate passed; implementation of this next slice is now active.

## Verified direct-conversation Search slice

Direct conversation headings now omit the channel hash. Explicit unresolved IM
peers share the established author lookup, user snapshots/name-only merge and
100-ID batching. Canonical channel records are not mutated. Explicit MPIM wins
over conflicting cached IM; decoded false flags only permit cached classification
fallback, without claiming that missing and explicit false were distinguished.
Empty result items/blank channel IDs do not add peer lookup work. Name-only and
other ambiguous metadata is preserved without inferred users or membership.

One actual-loader regression covers cold peer hydration, channels/group DMs,
cached fallback, conflicting explicit classification and ambiguous/empty items,
with exact navigation/thread/body/pager/draft and channel-map assertions. A second
test prepares normal/narrow fixture headings. Existing tests only gain the new
VM classification field with their prior channel semantics. Amp's implementation
review found no source blocker; formatting passes. Focused **36/36 passed** in
**538.29 s**, minimum commit headroom **10.520 GiB**, no stop. Full desktop passed
**5,070/0/7** in **227.21 s** (218.97 s tests), identical configuration/harness SHA256
`6e81e1e5bad06bac8c8d1c7e8de6d918f3191500c3b33b0fc4607353e07e4305`.
Every prior 5,068 pass, original 92 repair and ignored status is preserved. The
5,832-input source snapshot and binary are retained: `conversation-milestone.json`,
`desktop-5070-source.json`, `desktop-5070-baseline.exe`. Targets **53.153 GiB**,
disk **156.731 GiB**, commit **28.545 GiB** after full execution. Native/live and
full-parity acceptance remain open.

Next candidate is bounded People Search, reviewed in
`amp-people-search-boundary-plan.review.md`. Existing `users/search` requests 30
rows and exposes no total/cursor contract. It can support name/email lookup in
returned order with explicit limits, not fabricated complete paging/ranking.
The Profile audit confirms an uncached identity has no existing pane projection
without canonical-user insertion. The next integration plan therefore uses
profile-scoped read-only snapshots, preserves cached Profile behavior and all
canonical membership/action guards, and does not seed users from sparse search
records. This plan is now implemented in the active batch, with twelve registered
normal/narrow fixtures. It remains unverified at the desktop boundary. Core
focused passed **147/147**, then full **5,444/0/6 + 1 doctest**, preserving all
prior passes and ignored tests. A later correction restored the existing typed
envelope validation; corrected-source core focus again passed **147/147**, and
its full core gate now passed **5,444/0/6 + 1 doctest** in **659.72 s**,
minimum commit **24.237 GiB**, identical corrected focused harness and all prior
statuses preserved. All 2,057 core inputs match the final snapshot. Evidence:
`people-core-full-28.results.json`, retained `core-5444-corrected-baseline.exe`,
complete `people-milestone.json`. Desktop focus passed **89/89** in
**543.04 s**, minimum commit **10.900 GiB**, no safety stop. The first attempt
failed compilation on renderer re-export visibility; the one-line correction
changes no test or build mode, and the failed evidence is preserved. Full desktop
passed **5,079/0/7** in **245.56 s** (223.60 s tests) on the identical harness SHA256
`e14215ee10153ce4885dec96f8665ff75b57179a09b4b9d7b59622c5df4189e2`.
All 5,843 frozen inputs match. Every prior 5,070 pass, original 92 repair and
ignored status is preserved. Affected Profile257, Canvas421, sharing6, Home280,
List330 and usergroup22 groups remain green. Retained evidence:
`people-desktop-milestone.json`, `desktop-5079-source.json`,
`desktop-5079-baseline.exe`. Targets **53.166 GiB**, disk **154.925 GiB**, commit
headroom **28.425 GiB** after the full command. Native/live acceptance is still open.

## Verified person/thread Search controls

Message filters now expose With person and In threads through the existing
Apply/Clear/Enter and request paths. The pure editor owns positive `with:` and
exact `is:thread` tokens, preserving unchanged queries, exclusions, quoted
literals and other modifier values. Cached person suggestions are independent
of From; explicit checkbox routing keeps file/link/thread flags separate.
[Slack's search guide](https://slack.com/help/articles/202528808-How-to-search-in-Slack)
supports the modifier syntax; it does not establish exact mounted layout or live
endpoint interpretation.

Core focus **24/24** and receipt checks **26/26** passed. Desktop focus first
ran **278/2** because the new tests used the fixture-variant fallback and assumed
mutation order. The corrected tests use the actual Search installer, verify
initial state and address controls by explicit accessible name. No assertions or
tests were removed/ignored. Corrected focus **280/280** took **547.34 s**, minimum
commit **10.762 GiB**, no stop. Full desktop **5,082/0/7** took **246.27 s**
(224.23 s tests), same SHA256
`f7eb18ca17a422ac050b0962fe3e7a71e0f3f123c597676c200994f6f86b47d8`.
All prior 5,079 passes, original92 and ignored statuses are preserved. The
5,848-input source and executable are retained as `desktop-5082-source.json` and
`desktop-5082-baseline.exe`; evidence is `context-filter-desktop-milestone.json`.
Targets **53.129 GiB**, disk **154.065 GiB**, commit **28.441 GiB** after full
execution. The changed-core full gate passed **5,449/0/6 + 1 doctest** in
**652.17 s**, minimum commit **24.858 GiB**, identical focused hash
`8623668710cc2230ee456e9f738d5655498cba647c2e2bd8e231f1e5d6779664`.
Every prior core pass and ignored status is preserved, with five added tests;
all 2,058 input hashes match. Retained evidence: `context-filter-milestone.json`,
`context-filter-core-full-32.results.json`, `core-5449-baseline.exe`. Targets
**53.119 GiB**, disk **153.888 GiB**, commit **28.425 GiB** after the core command.
New normal/narrow fixtures are
registered, not rendered; generated-control tests do not prove trusted native
input or the Clear button's own click callback. No production rebuild or native
probe occurred.

## Current product batch

| Journey | Implementation and scoped evidence | Remaining acceptance |
| --- | --- | --- |
| Workspace Files search | Documented `search.files` request/strict decoding, Messages/Files selector, shared query/sort, ordered paged results, failed-page retry, ID-based selection into the existing preview/Canvas/List readers. Verified file display updates reconcile retained/in-flight results without changing media/access fields. Core focused Files gate: **9 passed**; combined desktop focused **17/17** and full **5,051/0/7** now passed. Earlier compiler/resource failures remain preserved. | Rendered normal/narrow and live endpoint/session compatibility remain unverified; desktop runtime gate passed. |
| Find results from a known day | Shared **On date** filter emits `on:YYYY-MM-DD`, preserves unedited raw syntax and validates edited dates. Unfinished quoted queries refuse adding a modifier. Apply/Clear/Enter reuse the existing adapter. Core focused date gate passed **20/20** after Amp's mixed clear/add finding was corrected. Desktop runtime gate now passed; normal/narrow acceptance remains pending. This is search filtering, not direct timeline jumping. | Rendered interaction and live date semantics remain unverified; desktop runtime gate passed. |
| Local recent searches | Local persisted query/kind/sort history and replay are implemented. Exact retained-record ownership, shared admission and stored-user cache isolation are verified at the core/compile boundaries. **140/140 focused core search tests** and the **5,437-pass full core gate** passed, including five new MRU/cache cases. Raw-text edits retire pending hit-opening while equivalent trimmed queries retain result/pager state. | Desktop runtime/replay assertions passed in 17/17 and 5,051/0/7 gates. Rendered normal/narrow acceptance remains open; no Slack-synced history claim. |
| Search after an outage | Failed first/append pages retry without replacing account/query/kind/sort/page ownership or loaded results. Seven new offline recovery journeys passed; full desktop **5,058/0/7**, core **5,438/0/6 + doctest**. | Physical network transition, mounted and live acceptance remain open. |
| Search identity names | Cold Files uploader names and message authors resolve through the established typed user lookup, preserving observed intervening user/profile updates and optional-failure fallback. Latest author focused **31/31**, full **5,065/0/7**. | Server freshness, change-and-revert history and live session equivalence are unproved. |
| Attachment-only message previews | Typed visible attachment content now supplies blank-body snippets; body/file precedence, hidden-data sentinels and overlapping pages passed **34/34 focused**, then **5,068/0/7 full** on the unchanged harness. | Normal/narrow native and live acceptance remain pending. |
| DM/group-DM Search labels | Direct-conversation headings and explicit cold peer hydration passed **36/36 focused**, then **5,070/0/7 full**, preserving the prior passing and ignored sets. | Rendered/live acceptance remains pending. |
| Bounded People Search | Existing 30-row lookup, returned-order results and profile-scoped read-only details are implemented without canonical membership writes or invented paging totals. Final core **147/147 focused**, **5,444/0/6 + doctest**; desktop **89/89 focused**, **5,079/0/7 full**, identical harnesses and prior statuses preserved. | Rendered and live acceptance remain pending; bounded discovery is not complete People Search ranking/paging parity. |
| Find threads with a person | Message-only With person / In threads controls passed **24/24 core focus**, **5,449/0/6 + doctest**, **280/280 desktop focus**, then **5,082/0/7 full desktop**, identical harnesses and prior statuses preserved. Formatting, whitespace and 26 receipt checks pass. First desktop **278/2** failed harness and correction evidence retained. | Rendered and live modifier semantics remain pending. |
| Find saved/pinned messages | Exact `is:saved` / `has:pin` controls passed formatting, whitespace, 26 receipt checks, **26/26 core focus**, **5,451/0/6 + doctest**, **281/281 desktop focus**, then **5,083/0/7 full desktop**, identical harnesses and prior/original92/ignored statuses preserved. Clear deliberately owns positive pin syntax; unmanaged-token and all receipt guards remain. No save/pin mutation is added. | Native/live acceptance remains pending. |
| Open listed cold/closed DMs | Account-owned feed/info read and canonical InSurface navigation batch is applied. Three actual core factory/decoder tests passed. Corrected desktop focus **226/226**, then **5,097/0/7 full desktop** on the identical binary, prior/original92/ignored statuses preserved. Amp findings and the failed225/1 fixture correction remain recorded. | Final core **5,454/0/6 + 1 doctest** passed on identical focused harness and all prior statuses preserved. Normal/narrow fixtures registered only; native/live acceptance open. |
| Reach older filtered DMs | Explicit continuation button and original rendered frame/cursor/sequence handoff batch is applied. Five new actual-action/integration/generated-panel/click tests passed in **231/231 focused**, then **5,102/0/7 full desktop**, identical binary and all prior/original92/ignored statuses preserved. | Offline gates passed. Core5,454 full carries only on exact2,061 unchanged inputs. Native/live/layout/focus acceptance remains open. |
| Recover an initial DM read | Explicit first-page source/button and scoped optional names batch is applied. Formatting/whitespace, **235/235 DM focus**, **42/42 Search focus** and **5,106/0/7 full desktop** passed on the identical binary, prior/original92/ignored statuses preserved. Four new tests include110-name batching and actual initial callbacks. | Offline gates passed. Core5454 exact2061-input carry; native/live/focus/layout unverified. |
| Find unread cold DMs and renamed groups | Matching canonical names and existing bag counts now drive row labels/filters. Authoritative zero counts override stale feed badges; matching sparse identities remain fallback. Four new regressions passed in **239/239 DM focus**. | Same-binary full47 passed **5,110/0/7**, every prior/original92/ignored status preserved. Core5,454 exact2,061-input carry; native/live/layout unverified. |
| Original native three-fix gate | Zero new native passes. History callback after Signal disposal, Home removed dirty-control ownership, and mounted List file acquisition across socket generations remain open. | Authorized interactive execution host. Do not repeat session-0 probes or attempt OS/security workarounds. |
| Surface metadata account/retry recovery | Owner-scoped successful completion and pointer-owned pending leases; cancellation, coalescing, token/account replacement and multi-chunk omitted/error semantics passed11/11 focused, then5,116/0/7 full49 on the identical harness; all prior/original92/ignored statuses preserved. | Core5454 exact2061-input carry. Native/live acceptance, generic media byte/cache and Later internal lifecycle remain open. |
| Recover Later read pages | Exact failed append cursor, retained successful list/continuation, original rendered source and app-owned read dispatch passed19/19 focused then5,123/0/7 full51 on the identical harness; all prior/original92/ignored statuses preserved. Seven new tests passed. Earlier compile/test-harness failures retained. | Core5454 exact2061-input carry. Native/live/layout, actual LaterPanel unmount and held-per-hydration-stage proof remain open; arbitrary borrow-held abort cleanup is best effort. |
| Account-owned queued pictures | Current-incarnation authenticated sources/jobs, synchronous capture, wait/result retirement, passive protocol compatibility, bounded resident reuse and cancellation-safe cache writers passed30/30 focused then5,127/0/7 full53 on the identical binary; all previous/original92/ignored statuses preserved. Four new production-loader cases passed. | Core5454 exact2061-input carry. Admitted HTTP/write work cannot be physically retracted. Native/live acceptance and offline restart disk-hydration scheduling remain open. |
| Cached pictures while offline | Local-only scheduling, independent cache eligibility and one revision-bound attempt per actual demand passed33/33 media focus then5,130/0/7 full55, identical binary and prior/original92/ignored statuses preserved. Three new production-loader journeys passed, including network-to-offline hydration and post-permit HTTP refusal. | Core5454 exact2061-input carry. Native/live/layout remains open; historical warm-boot reference is not current exact Slack equivalence. |

Sources: [Slack search guide](https://slack.com/help/articles/202528808-How-to-search-in-Slack)
and [documented Files search API](https://docs.slack.dev/reference/methods/search.files/).
The existing message-search endpoint remains unchanged; no private Files endpoint
was guessed. No live Slack action was taken. Normal/narrow fixture registration
is not rendered/native acceptance.

## Evidence and supervision

The implementation/build progression below predates the verified 5,051 milestone.
Its pending-state descriptions are historical; the latest evidence is above.

All new logs/reviews are preserved under `tmp/parity-file-search-20261027/`.
Amp plan review set the paging/type/file-opening acceptance bar. Its implementation
review raised an extra canonical-preview-read requirement; the follow-up reviewed
the existing preview, credential and mutation boundaries and withdrew that
requirement. No source-level opening blocker remained in that review. Amp's date
plan review accepted the bounded known-day search workflow. All reviews ran with
tools rejected; they are review evidence, not executed checks.

Core Files focused command: locked `CoreTest -Jobs 8 -Filter file_search`,
**9 passed / 0 failed**, 208.65 s. First desktop focused compile: exit 101,
310.63 s, one borrow error; **no desktop pass claimed**. Resources after that
attempt: combined targets 45.601 GiB, free disk 168.141 GiB, commit headroom
26.161 GiB. Helper gate remains 48 GiB; user ceiling is 96 GiB only as needed.
Retain the 50 GiB free-disk floor and 4 GiB commit margin.

The second desktop attempt (483.49 s, exit 101) was deliberately stopped when
observed commit headroom reached **3.441 GiB**. Its compiler PID was verified
against the owned desktop test command before termination. No tests ran and no
artifacts were deleted. After stopping, targets were **47.061 GiB**, disk free
**167.609 GiB**, and commit headroom **26.154 GiB**. Amp subsequently authorized
four jobs or fewer for this large test build while retaining ordinary eight-job builds.
Do not repeat the failed eight-job build without enough headroom. The existing
production executable remains the prior baseline and does not include this
product batch.

Final date-focused core command: **20 passed / 0 failed**, 99.40 s. The earlier
20-pass result preceded the mixed clear/add correction and is preserved separately.
Amp confirmed that correction at source level. The independently locked Files/date full core gate completed in **710.16 s**:
**5,432 passed / 0 failed / 6 existing ignored**, plus **1 compile-fail doctest**.
Its exact statuses, test executable hash and pre-history source snapshot are in
`core-full-results.json` and `core-files-date-source.json`. This gate predates the
local-history cache changes. The current focused search gate passed **140/140**
in **165.08 s**, including all five new MRU/cache tests; no newer full-core count
is claimed yet.

The current complete desktop source and test code passed locked dev-fast/eight-job
**CheckTests** in **247.35 s**, after correcting a test assertion to parse the
request URL string. This executes no tests. Cargo's initial concurrent normal/test
check reached **5.214 GiB** commit headroom; the owned test frontend was stopped
at the safety buffer while normal checking finished. Reusing that completed
normal artifact let the test frontend finish alone with eight jobs and stable
caches. Preserve both failed-attempt logs and the successful `desktop-check-tests-03`
log. Amp confirmed the history raw-edit and asymmetric-admission corrections at
source level. No desktop runtime pass or native/layout/live claim is earned.

After the successful check: combined targets **52.073 GiB**, free disk
**161.071 GiB**, commit headroom **26.167 GiB**. The helper is now **56 GiB** after
the measured full-core growth to 48.889 GiB, within the existing conditional
96 GiB authorization. Its eight-job default and 50 GiB/4 GiB guards are unchanged.

The changed local-history full core gate then passed in **686.75 s**:
**5,437 passed / 0 failed / 6 existing ignored**, plus **1 compile-fail doctest**.
All five added MRU/cache tests passed; every prior passing test remained `ok`,
and the ignored set is unchanged. Exact comparison, executable hash and source
snapshot: `core-local-history-full-results.json` and `core-local-history-source.json`.
Root/core formatting and helper PowerShell parsing passed. The changed developer
build passed in **476.47 s**, dev-fast/eight jobs, with minimum observed commit
headroom **6.008 GiB**, no safety-stop and no native launch. Executable:
`target/desktop/dev-fast/super-platinum.exe`, **278,875,136 bytes**, SHA256
`6dc4d92eb251491c817702d55f200edc0c2bf8ce89ad331c98d5db72b009e2a4`.
The prior baseline copy is retained as `developer-baseline-before-search.exe`.
Final measured targets **52.097 GiB**, disk free **160.385 GiB**, commit headroom
**26.162 GiB**; no Cargo/compiler pipeline remains. The helper stays at 56 GiB.

## Guarded desktop build blocker (latest)

Amp's four-job authorization was used, followed autonomously by two and one job.
All seven attempts in `tmp/parity-search-recovery-20261027/` stopped before tests;
none is a new desktop pass or assertion failure. Four/two jobs used a 6 GiB stop
buffer; one job used 5 GiB, preserving the unchanged 4 GiB hard floor. The final
info-only diagnostic stopped at **4.969 GiB** (413.33 s). The subsequent lower
Files `load_with` erased-future experiment reached code generation but stopped at
**4.993 GiB** (367.76 s), before linking/test execution. These measurements do not
establish a working source-footprint repair. Both boxing experiments are reverted;
all **43 non-documentation inputs** match `source-manifest-local-history.json`.
The old test executable remains unchanged and was not executed as a new pass.
Formatting passed; assertions and ignored tests were not changed.

Post-command targets **52.018 GiB**, disk free **160.373 GiB**, commit headroom
**26.338 GiB**. Helper gate stays 56 GiB. No production executable rebuild,
native/session-0 probe, cleanup or OS/security action occurred. Exact attempts,
source comparison and pending gate order are in
`desktop-build-blocker-checkpoint.json` in the new evidence directory.

Amp reviewed `test-build-configuration-proposal.md` and clarified that stable
configuration was operational guidance, rather than a user prohibition. One
bounded reversible experiment is now explicitly authorized. The applied helper
flag `-DesktopTestNoIncremental` requires DesktopTest/dev-fast/one job, sets
`CARGO_INCREMENTAL=0` only around Cargo and restores its previous process value
in `finally`. Six isolated checks passed with mocked Cargo; no Rust build ran in
those checks. Ordinary defaults, manifests, profiles and lockfiles are unchanged.

The focused `bootstrap::search` attempt is running with the same target/linker/
features, one job and a 5 GiB runtime stop guard. Disabling incremental also
affects path dependencies and changes the codegen-unit default (16 rather than
256 here, absent an explicit override); existing cache files are retained. The
runtime guard now samples owned compiler private memory and resolves the actual
executed test executable from Cargo output. Only on a real focused pass may one
full gate reuse the identical configuration/binary. If this attempt fails, do not
cycle configurations: report measured memory/bottleneck and the minimal required
human action. No production rebuild or native probe is authorized by this trial.
Native acceptance remains independently blocked on an interactive execution host.

## Next safe work

The combined Files/date/history, outage recovery, uploader, author, attachment,
DM/MPIM headings and bounded People batches passed focused and full desktop
verification. The latest saved/pinned full desktop baseline is **5,083/0/7**;
its core passed **26/26 focused**, then **5,451/0/6 + 1 doctest** with the same
core harness. Desktop focus **281/281** and full gate used the identical harness
`13342253cb679becebaf666d1ebd3d52cb35bce35a14799dde503842613c2d49`.
All previous statuses, original 92 repairs and ignored sets are preserved.
Rebuild 617.39 s, minimum commit headroom 10.459 GiB; full gate 244.30 s.
Final core 649.09 s, minimum commit 24.891 GiB. Retained evidence:
`saved-pinned-milestone.json`, desktop/core full35/full36 and source snapshots.
After the last heavy gate targets were 53.115 GiB, disk free 152.985 GiB and
commit headroom 28.392 GiB. No production rebuild or native probe occurred.

The listed cold/closed DM repair is verified: actual core focus **3/3**,
corrected desktop focus **226/226**, full desktop **5,097/0/7** on the identical
harness `290871288a1e763e3dedde8483b78521bcfca760a961a460cd6c705951457d46`,
and final core **5,454/0/6 + 1 doctest**, identical core focused harness.
Every previous pass, original92 and ignored set remains. Desktop rebuild542.23s,
minimum commit10.693GiB; full243.55s. Core full665.99s, minimum commit23.853GiB.
Retained evidence: `dm-cold-row-milestone.json`, `desktop-5097-source.json`,
`desktop-5097-baseline.exe`, `core-5454-baseline.exe`. Targets53.125GiB,
free disk151.177GiB and commit27.691GiB after the final core gate. Compile38's
import collision and focused38b225/1 failure/binary remain preserved. The latter
was a fixture authority error: corrected38c establishes existing bag counts,
returns contradictory metadata and keeps/adds strict read-target assertions.
No production executable rebuild, native/session0 probe or live action occurred.

Accessible older-page work passed **231/231 focused** and **5,102/0/7 full
desktop** on identical harness `087f5ea3...8a0a6566`, all prior/original92/ignored
statuses preserved. Rebuild549.67s, minimum commit10.719GiB; full224.98s.
Actual generated panel and serialized disconnected button retries passed.
Core5,454 + doctest carries on2,061 unchanged inputs. Evidence:
`dm-accessible-paging-milestone.json`, source/binary desktop5102. Targets53.127GiB,
free150.574GiB and commit28.174GiB after full42. Native/live acceptance remains open.

Initial-read recovery passed **235/235 DM focus**, **42/42 Search focus** and
**5,106/0/7 full desktop** on identical harness `373420a7...936ddae9`. All prior
passes/original92/ignored sets remain. Rebuild541.32s, minimum commit10.540GiB;
Search8.05s reused the binary; full225.19s. Core5,454 + doctest carries on all
2,061 unchanged inputs. Four new tests cover explicit initial retry, published-
only optional names, cold labels/optional-error retention,110-name batching,
same-account token/account replacement and real serialized panel callbacks.
Source/binary/evidence: `dm-initial-recovery-milestone.json`, desktop5106 files.
Targets53.129GiB, free154.098GiB, commit28.271GiB after full45. No production or
native/session0 run; no live writes or layout/focus acceptance claim.

The row-fact slice passed239/239 DM focus and5,110/0/7 full desktop on the
identical harness `03e250e0...119d892a`, preserving all prior/original92/ignored
statuses. Rebuild544.47s, minimum commit10.624GiB; full231.95s. Source/binary
retained as desktop5110; evidence `dm-row-facts-milestone.json`. It preserves existing
bag counts while metadata is absent, including authoritative zero against stale
feed badges; matching canonical usable peer/name facts drive labels and query
filtering, with same-ID sparse fallback. Blank explicit IM peers cannot borrow
a name that core would never use. Four pure/generated-panel/actual-info tests
passed, including mention maximum and an explicit conflicting cursor/count
response. Core count/name semantics and navigation
currentness are unchanged. Before-images and review/fix evidence are retained
under `dm-row-facts-*`. The independent generic surface-user/emoji account/
reservation ownership audit is now in Amp source-plan review; no further product
changes are applied before that next boundary is coherently reviewed.

Channels discovery/navigation is independently audited in
`channel-search-source-audit.json`, `amp-channel-search-boundary-plan.review.md`
and `channel-search-boundary-followup.json`; xws/record-channel context and
canonical lifecycle admission need original reference evidence. Do not infer
ordinary active-workspace routes from sparse results or complete results from
the existing bounded 30-row helpers. Direct conversation date navigation needs
an original first-message/date history contract; the public update confirms the
feature, not exact selection/pagination semantics. Do not rebuild unchanged
production executables or repeat native session-0 probes.

Daily conversation/compose/files/Later/navigation, cold history/outage, rich
edit/mention, scheduled-forward, modal-refresh, uncertain delivery and cross-surface
acceptance remain open. Notifications/calls/device/native OS integration require
separate authorization and are not substitutes for safe product work.

## Concrete verification blockers

1. **Next product verification:** row-fact full47 passed5,110/0/7 as above.
   Generic surface-user/emoji account/marker ownership is the next independent
   source review; the product source remains at that verified milestone.
   Channels Search still needs its original cross-workspace/record-channel
   contract; no existing browser reference is attached.
2. **Native acceptance:** provide an authorized interactive execution host.
   Session 0 remains blocked. No further launch probe, impersonation, task/package
   registration, trust/security change or OS input is authorized. The original
   three exact scenarios and new search fixtures require retained capture guards
   and inspected PNGs; none has a new native pass.
3. **Live search equivalence:** the documented Files endpoint's compatibility with
   the actual client session and real date-search semantics remain unverified.
   Local decoded responses and compile/build gates do not establish live parity.


Surface metadata milestone49: 11/11 focused and5,116/0/7 full desktop passed;
same `ebb77f60...df9fa221` binary/source, six new tests passed. Core5454/0/6
+1doctest exact2061-input carry. See `surface-metadata-milestone.json`.
Next: source-reviewed Later failed-read recovery; preserve successful rows and
continuation, exact failed cursor, original read source and app task lifetime.
The public October2026 Saved/To-dos rollout is documented but does not authorize
an inferred migration of the original captured target or write journals.


Later read recovery is applied after source review; current focus50c is pending.
Build50's one borrow error and focus50b's17/2 result are preserved. All12 prior
Later cases plus five new driver/lifetime cases passed50b; two new renderer
lookups missed Dioxus static template IDs/text. The corrected test-only harness
retains all callback/cursor/row/mode/generation assertions. No full-gate or native
pass is inferred. Media account-queue ownership is next source-only audit; both
source ownership and original-job authority are required, preserving caches.


Latest milestone51: Later focus50c19/19 and full51 **5,123/0/7** passed on the
identical `a8032a44...e057958d` harness; all earlier/original92/ignored statuses
preserved. Core5454/0/6 +1doctest exact2061-input carry. See
`later-read-recovery-milestone.json`; seven new cases actually passed. Build50
and focus50b17/2 remain historical failures; no guard/test was removed or ignored.
Next safe batch: media source/job account ownership, reviewed source-only in
`amp-media-account-queue-plan.review.md`. Old jobs can select new queued URLs;
a valid new job can also select old authenticated sources. Preserve all caches,
sources, stable slots and public/exact/provisional reuse; source eligibility and
job authority belong together. No media changes yet; audit install/store boundaries.


Media account-queue ownership is applied after the complete-plan and draft Amp
reviews. Nine files bind queued jobs/authenticated sources to a unique shell
incarnation, retire real installs, fence waits/results, and let admitted cache
writers retain the shared owned lock even after their awaiter is cancelled.
Existing protocol access/public IDs and exact/provisional disk cache semantics
remain; resident reuse is bounded by the asset cache and never copies unslotted
private-file bytes. Four new controlled production-loader cases accompany the
unchanged original media corpus. Focus52 is pending; no new pass is claimed.
Root formatting and whitespace checks passed, and all2,061 Core inputs match
its5,454/0/6 +doctest baseline. Draft/before-images/review/source receipts are
retained under `media-account-queue-*`; no production/native/live action ran.


Media focus52 compiled successfully, then passed29/30 with one new-test oracle
failure. `ready_bytes` deliberately refuses provisional images for download;
the existing original regression already proves that contract. Correction52
changes only the new test to inspect actual opaque protocol bytes/no-store,
assert paintability and explicitly retain download refusal. No product/config
change, removed assertion or ignored test. Failed binary/log/source/results
retained. Rebuild587.56s, minimum commit10.146GiB, peak owned17.478GiB; targets
53.156GiB/free151.332GiB afterward. Focus52b is pending on unchanged configuration;
no new full-gate pass yet. Next offline-picture source plan is reviewed, with
implementation gated on this batch's focused/full verification.


Corrected media focus52b passed30/30, including all four new production-loader
cases and unchanged originals. Rebuild557.15s, minimum commit10.574GiB, peak
owned17.383GiB. Targets53.156GiB/free150.934GiB after command. Full53 now runs
with identical configuration and harness `263d62e4...62ce7d07`; its result is
pending. The failed52 oracle and binary remain retained. Offline picture-cache
plan review and source/lock audits are retained, without product edits yet.


Media milestone53 passed **5,127/0/7** on the identical focused52b harness
`263d62e4...62ce7d07`,225,920,000bytes and unchanged mtime. Every prior5,123 pass,
all original92 repairs and ignored set are preserved. Profile257/Canvas421/
sharing6/Home280/List330/group recovery22 all passed. Full230.72s (222.90s tests),
no application compilation. Core5454 +doctest carries on exact2061 inputs.
Source/binary retained as desktop5127; `media-account-queue-milestone.json`.
Targets53.156GiB/free150.685GiB/commit28.044GiB after full. Failed52 and corrected
new-oracle evidence remain. No production/native/live action, commit or cleanup.
Next: implement the reviewed offline persistent-picture slice, with independent
disk eligibility, original demand revision, bounded lookup attempts and known-
offline HTTP refusal. Primary historical Slack warm-boot reference and exact
scope limits are in `offline-picture-cache-source-audit.json`; no current live
or complete parity claim follows. Full parity remains ACTIVE.


Offline persistent-picture batch is applied after Amp draft review. Eight files
add cache-only scheduling on known Offline health, independent local eligibility,
one completed lookup per unique actual demand, revision-checked publication,
and known-offline refusal after hydration and semaphore acquisition. Normal
network source/cache selection and HTTP backoff are preserved. Three controlled
production-driver journeys count actual isolated store lookups and exercise
Network-to-Offline disk completion plus the next backoff-independent local sweep.
Amp's unbounded queued-test await is now a5s timeout. Formatting/whitespace pass,
Core2061 inputs remain exact; source5879 snapshot frozen. Focus54 is pending;
no new pass or native/live acceptance is claimed. Before-images and applied/review
receipts are retained under `offline-picture-cache-*`.


Offline picture build54 ended before tests with one E0603 private-module import.
Core publicly re-exports CachedImage from its root; correction54 changes only
that import, no tests/config/domain source. Failed command/source/import before-
image retained.210.43s, minimum commit14.745GiB; targets53.009GiB/free150.595GiB.
Formatting and whitespace pass; focus54b is running on identical one-job/noinc/
dev-fast settings.5,127/0/7 remains the verified full baseline. Independent next
source plan: Files/Links page recovery. Both begin methods clear loaded rows and
paging metadata before a later page succeeds; the proposed batch separates the
committed page from an exact pending/failed target. No such edit is applied yet.


Offline picture milestone55 passed **5,130/0/7** on the identical54b harness
`bdfb3164...97961a08`,226,679,296bytes/unchanged mtime. All prior5,127 passes,
original92 and ignored statuses remain; all affected groups remain green.
Focus33/33, rebuild558.78s/min commit9.944GiB; full227.74s/220.22s tests,
no application compilation. Core5454 +doctest exact2061-input carry. Source/binary
retained as desktop5130; `offline-picture-cache-milestone.json`. Targets53.163GiB/
free150.016GiB/commit28.221GiB after full. Build54 import failure remains retained.
No production/native/live action or cleanup. Next accepted source plan is
Files/channel Files/Links retained-page and exact-retry recovery; no next product
pass yet. Full parity remains ACTIVE and native/live acceptance stays open.


Resource-page Core draft was source-approved and four files applied: committed
page retention, public transient requested_page and generation-terminal failure/
cancel, plus seven new state regressions. Original Core test bodies preserved.
Files focus56 passed50/50 in116.54s/min commit18.867GiB; Links57 passed10/10 in
3.28s on the identical201,777,152byte `85113c7b...70d6e54` binary. Full Core58
runs now; no fresh full-Core or resource desktop pass yet. Targets53.175GiB/
free149.729GiB after57. Desktop caller audit found the Canvas tab picker shares
Files loading under its own Visit; preserve that path explicitly rather than
restricting every channel Files read to the Files tab. No production/native/live
run, cleanup, commit or source reset.5,130 desktop remains prior-source evidence.


Fresh resource Core full58 passed5,461/0/6 +1doctest on the identical focused
`85113c7b...70d6e54` binary. All5454 prior passes and ignored set remain; seven
new cases passed.2063 inputs stayed exact. Full717.77s/min commit24.114GiB;
unit tests655.47s. Core5461 source/binary and exact results retained. Targets
53.195GiB/free149.502GiB after58. Desktop adaptations are still unapplied draft;
no current resource desktop/native/full-product pass. The shared context draft
preserves ordinary Files/Links and explicit Canvas-picker Visit; global Files
is not diverted by reconnect's detail_open branch because that helper explicitly
excludes MainView::Files. Planned three fixed ticket slots grant only exact local
busy cleanup, while full owner/route/reconnect checks remain required for data.
Rendered source stamps also include pending/committed page state, since success
keeps generation while changing requested target. No production/native/live or
cleanup action;5,130 desktop remains its retained prior-source milestone.


The21-file resource desktop batch is applied after complete Amp review, with
8new reader/actual-component/serialized-listener tests. Read contexts retain
full ReadOwner+active team+original route/visit; three fixed private ticket slots
authorize only owned busy cleanup. Result authority is unchanged/strict. Actual
controls distinguish committed pagination from failed target, retain results,
and refuse stale source stamps. Picker reads preserve the original Visit and
fixture refusal; reconnect carries its original Visit. Names are post-success,
independently guarded; helper bodies unchanged. Four closed normal/narrow aliases
registered only. Reviewer found no definite source blocker; accepted limits and
stronger loaded1/failed3 Retry discriminator are recorded in the resolution.
Root formatting/whitespace passed; Core2063 inputs remain exact at5461+doctest.
Desktop5889 inputs frozen; focus59 Files runs now, then unchanged-binary Links60
and full61 on success. No new resource desktop or native/live acceptance yet.
Before-images/draft/review/source receipts retained; no production build/probe,
cleanup, commit or live Slack action.5,130 remains prior-source desktop evidence.


Resource desktop compile59 failed before tests:20 repeated diagnostics reduce
to three synthetic Offline constructor calls missing String and one File.user
fixture field (the decoder retains user in File.extra). Both new test files are
corrected using Offline(String) and the actual File serde decoder. No product,
assertion, ignored-set or configuration change. Failed source/command and both
before-images retained.303.85s/min commit14.697GiB, targets53.046GiB/free149.433GiB
after command. Formatting/whitespace pass; Core2063 inputs still exact. Focus59b
runs on unchanged one-job/noinc/dev-fast settings; no desktop tests have executed
for the new resource batch yet.5,130 desktop remains prior-source evidence.


Corrected resource Files focus59b passed102/102; Links focus60 passed3/3,
both on identical227,756,032byte `cae3fbec...5a978364` harness/mtime. All eight
new desktop cases are included across those groups. Rebuild575.21s/min commit
10.169GiB/peak owned17.448GiB, no safety stop. Full61 runs on that same binary;
no full-result claim yet. Targets53.200GiB/free148.913GiB after60. Core5461
+doctest remains exact2063-input carry. Compile59 failure/correction preserved.
Next Activity source plan is Amp-reviewed, with no product edits before full61:
transactional cursor history and exact failed refresh/append retry. Native/live/
layout and full parity remain open; no production/native/live action ran.


Resource milestone61 passed **5,138/0/7** on the identical focused59b/60
`cae3fbec...5a978364` harness/mtime,227,756,032bytes. Every prior5130 pass,
all original92 repairs and ignored set remain. Profile257/Canvas421/sharing6/
Home280/List330/group recovery22 all green. Full247.07s (226.03s tests), no
application rebuild; ancillary package test targets compiled as Cargo required.
Core5461 +doctest carries on exact2063 inputs. Source5889/binary retained as
desktop5138; `resource-page-recovery-milestone.json`. Targets53.200GiB/free148.655/
commit27.672GiB after61. Failed59 and corrections remain retained. No production/
native/live action, commit or cleanup. Full parity ACTIVE; native/live open.
Next safe slice is Amp-reviewed Activity failed-read fidelity. Existing rows
already survive failed reads; concrete gaps are premature cursor-history reset
and lost failed refresh/append identity, wrong footer copy and reconnect page1.
A successful cycle stop must not create retry intent. Core/native drafts remain
unapplied; new initial refresh retains max5-page unread fill, exact failure retry
will read one page only. Source review is not compilation or native acceptance.


Activity domain draft applied after Amp review; both suggested test additions
are included. Original test bodies remain byte-equivalent. Three new cases
passed within26/26 Core focus62 (157.23s/min commit17.826GiB, peak owned10.381).
Fresh full63 runs on that identical `ae184629...fdab75cc`201,778,176byte binary;
no fresh full result yet.2064 Core inputs frozen. Targets53.242GiB/free147.790GiB
after62. Eight desktop/native/control files are unapplied draft under Amp review;
4driver+1actual generated-panel tests planned. Source-gap/reference/policy limits
recorded in `activity-failed-read-source-gap.json`. No native/live/layout or full
parity claim; previous desktop5138 milestone/source/binary remains retained.


Activity desktop batch applied as9files after complete Amp review. Initial
review01 was truncated before panel/routing; follow-up02 includes the full
remainder and finds no definite blocker. The existing new generated-listener
test also now clicks ordinary Load older, distinguishing it from page1 Retry.
Initial/unloaded failure copy avoids claiming retained rows exist. Core2064
inputs remain frozen/exact; full63 still running. Four named Activity aliases
registered; both narrow variants plus two prior resource recovery variants now
reach the existing minimum-window branch. This corrects source registration
only, not measured widths/layout/native acceptance. Bash syntax/whitespace pass;
5893 desktop inputs frozen. Root fmt then Desktop64/65 focused and same-binary
full66 follow Core63; no current Activity desktop pass is claimed.


Activity fresh Core full63 passed **5,464/0/6 +1doctest** on the identical62
`ae184629...fdab75cc`201,778,176byte harness/mtime. All prior5461 passes/ignored
statuses preserved;2064 inputs exact. Full710.41s/640.81s tests/min commit23.168.
Core5464 source/binary retained. Targets53.253GiB/free147.423GiB after63.
Before Desktop compile, root found repeated-restore liveness risk: strict old
reconnect serial could retire the admitted request while replacement skipped
busy. Amp approved a narrow three-file correction: original Visit.current stays
byte-equivalent for queued admission; admitted_current substitutes only serial,
retaining every other original field and full ReadOwner/exact Core ticket.
Existing failed-page driver test now holds that read through a real later restore
and duplicate refusal, then releases success. No added custody-test count.
Ten desktop files applied; final5893 inputs frozen. Root fmt20.56s/Bash syntax/
whitespace passed. Focus64 runs now, then panel65/full66 same binary on success.
No Activity desktop/native/live/layout pass is claimed yet. Metadata handoff
remains owner/visit+sequence, not exact ticket/all-held-stage proof. No production
build/probe/live action/cleanup/commit. Long-term full parity remains ACTIVE.


Activity Desktop64 passed12/12 (four new actual driver cases), and generated
panel65 passed1/1 on identical228,187,648byte `07d58d03...be3e1429` harness/mtime.
The panel executes actual serialized Retry/Refresh/Load older listeners across
four aliases, filtered-empty/initial/busy states; no browser/OS/layout acceptance.
Rebuild637.28s/min commit10.168GiB/peak owned17.580GiB; no safety stop. Full66
runs with unchanged configuration/binary. Targets53.256GiB/free146.880GiB after65.
Core5464 +doctest carries on2064 exact inputs. No new full desktop result yet.
Next followed-Threads source plan/Core draft source-approved, still unapplied and
gated on66. Public FeedRequest shape stays two fields; settlement deliberately
tightens. Native must check accepts before report side effects, retain CursorOwner
rt_generation, and quietly cancel only exact original shell/slot work.


Activity milestone66 passed **5,143/0/7** on the identical64/65
`07d58d03...be3e1429`228,187,648byte harness/mtime. Every prior5138 pass,
original92 and ignored status preserved; affected groups all green. Full240.52s/
219.57s tests, no application rebuild. Core5464 +doctest exact2064-input carry.
Source5893/binary retained as desktop5143; `activity-failed-read-milestone.json`.
Targets53.256GiB/free146.633GiB/commit28.251GiB after66. Source-grounded remaining
Activity resource-detail reconnect early return is recorded separately, not
claimed fixed by driver/control tests. Native/live/layout/full parity stay open.
Next safe Threads domain slice applied as2reviewed files with3regressions,
original test bodies byte-equivalent. Public FeedRequest fields unchanged;
settlement intentionally tightens to admitted loading+sequence+cursor.2065 Core
inputs frozen; fmt/whitespace passed. Focus67 runs, then full68 on success. Batch
Activity's parent-route recovery with next Threads native/controller integration;
no production/native/live action, cleanup, commit or push.


Threads Core67 passed8/8 (three new cases),156.49s/min commit17.697GiB.
Fresh full68 passed **5,467/0/6 +1doctest** on identical201,847,808byte
`9d60ea9f...aad4da2f` harness/mtime, every prior5464 pass/ignored status retained.
2065 inputs exact;733.38s total/650.24s tests/min commit23.299GiB. Core5467 source/
binary retained. Targets53.305GiB/free146.272GiB after68. Fourteen desktop files
are unapplied draft, with3actual driver+1panel+1parent-route tests. Native keeps
original CursorOwner/rt_generation and exact shell-root/pending-slot ownership.
Parent resource branch must require conversation_visible (Threads/Unreads have
no resource pane even when detail_open is true), and schedule only Activity's
exact failed feed inside that returning branch. No duplicate normal discovery
refresh. Full native/control review and desktop gates pending; no pass claim.


Threads native/reconnect batch applied as14files after two complete Amp reviews,
no concrete blocker found. Original native reconcile/test bodies and original
Core test bytes retained. Review refinements add no test cases: busy Retry
absence matches begin_retry clearing intent; existing held driver now supplies
threads_state and checks current publication plus intervening live count
protection. Three driver+one actual panel+one real parent-route tests are added.
Parent branch uses conversation_visible, correcting stale Files-tab Threads
routing; only Activity schedules failed idle scoped intent in the returning
resource branch. No normal-discovery duplicate. Six fixture aliases registered;
three narrow names verified inside actual size expression, media diff only3lines.
Root fmt20.70s/Bash syntax/whitespace pass; Core2065 inputs exact at5467+doctest.
Desktop5898 inputs frozen. Focus69 running, then70panel/71reconnect/full72 on
same binary if green. No current Threads desktop/native acceptance. RT guard
unchanged; quiet release on settlement/drop, no immediate auto-handoff claim.
Parent regression measures admissions, not physical HTTP count/real rail input.


Threads69 compiled and passed5/5; generated panel70 passed1/1 on identical
228,731,904byte `9a506c2c...437a64d3` harness/mtime. Rebuild639.51s/min commit
10.020GiB/peak owned17.567GiB. Reconnect71 FAILED4/1, only new real-parent test;
all four original reconnect cases pass. Same-binary child-only backtrace finds
history.mark_channel_read calls native focus before rejecting hidden Threads
transcript; VirtualDom has no DesktopService. No production/native launch or
config change. Failed binary/source/log retained; full72 held. Minimal two-file
correction under review: short-circuit existing mandatory transcript visibility
before platform lookup, keeping all original focus/pref/scope checks; test also
asserts retained active channel/read cursor unchanged. No ignored/removed test.
Targets53.309GiB/free145.524GiB after71; Core5467+doc exact2065 inputs.


Reconnect71 correction source-approved by Amp and applied: hidden transcripts
short-circuit the existing mandatory read eligibility before native focus lookup.
Visible focus/policy/owner guards remain. The real parent test now retains the
active conversation, checks the hidden read cursor and waits whole reconnect
completion plus feed settlement. Core5467+doc remains exact2065 inputs; root
format/whitespace passed. Corrected5898 inputs frozen before71b; failed71 source,
binary and backtrace retained. Same-config1job/noinc/dev-fast reconnect71b
rebuild is active;69bdriver/70bpanel/71c-read-prefs/71d-read-visibility follow on
identical binary only if green, then one full72. No corrected pass claimed yet.
Native/session0 remains blocked; no production/native/live action or cleanup.


Corrected reconnect71b passed5/5 including the real parent/read-cursor case.
Rebuild566.97s/min commit9.779GiB/peak owned17.595GiB, no stop. Driver69b5/5,
panel70b1/1, read preferences71c37/37 and visibility71d2/2 passed on identical
228,736,512byte e89e3ce5...d6284b92 harness/mtime; no unchanged recompile.
Core5467+doc remains2065 exact inputs, desktop5898 frozen. One full72 is active
with original statuses compared to5143; no new full pass yet. Failed71 evidence
remains intact. Targets53.309GiB/free144.398GiB after71d. Amp source-approved
unapplied six-file next draft: visible resource side-thread continuation with
renderer visibility and stricter reconnect-only completion visibility; two real
closed-parent cases. It stays gated on72. Native/live/layout and full parity open.


Threads milestone72 VERIFIED:5,148/0/7 full desktop, identical corrected71b/
69b/70b/71c/71d binary e89e3ce5...d6284b92,228,736,512bytes/mtime. All prior5143,
original92 and ignored statuses retained. Full243.3s;
rebuild566.97s/min commit9.779GiB/peak owned17.595GiB. Core5467+doctest exact2065
inputs. Desktop5898 source/binary retained as5148; threads-failed-read-milestone.json.
Failed71 and minimal correction receipts retained; no test removed/ignored.
Targets53.309GiB/free144.161GiB after72.
Current checklist: Threads exact Retry/retained rows/generated controls + actual
Activity/resource/Threads parent routing are verified offline. Hidden channel
read cursor stays unchanged. Native/live/layout/OS input and full parity OPEN.
Next safe source-grounded journey: visible retained side-thread beside resource
panel. Amp reviewed six-file draft; apply after this milestone, then focused
closed-parent/thread/workflow groups and one new full gate. No production/native
probe/cleanup/live action/commit or push; goal stays ACTIVE.


Next resource-side-thread slice applied as6Amp-reviewed files after5148 milestone.
Shared renderer visibility preserves aside mounting/display and blocks workflow-
hidden reconnect completion. Resource branch now reaches existing owned thread
loader; current target/scope captured after refresh, all original serial/transport/
CursorOwner/file-read guards retained. Ordinary opens unchanged. Two actual
closed-parent regressions added: visible/no-thread Files, and workflow-hidden
thread-origin Files. Root fmt20.61s/whitespace PASS;2065 Core inputs exact.
Desktop5899 inputs frozen before73. Focus73 reconnect runs next; thread/workflow
focus then full76 on identical binary if green. No new pass/native/live claim.


Side-thread compile73 FAILED before test execution (214.72s/min commit14.764GiB):
one new-test Owner import shadow and four unsupported ChannelMessages Serialize
uses. Only the two new test files corrected: explicit workflow Owner, full Debug
bag before/after snapshots retain all fields. No product/config/original-test or
asserted journey change. Failed source/command/log retained. Same-config73b is
next;74/75/full76 remain held. Latest verified desktop milestone stays5148.
Targets53.159GiB/free144.053GiB after73; no safety stop or native/production run.


Side-thread corrected73b PASS6/6,568.48s/min commit9.850GiB/peak owned17.562GiB.
Thread74 PASS2/2 and workflow entry75 PASS13/13, including real hidden parent,
on identical228,867,072byte8c29f41b...dd471e9f harness/mtime. Core5467+doc exact
2065 inputs;5899 desktop inputs frozen. Full76 active; no5150 full pass yet.
Compile73 failed before execution, only two new test files corrected; all failed
evidence retained. Targets53.309GiB/free143.365GiB after75. No production rebuild
or native/session0 probe. Next canonical-profile source gap supported by Amp;
constructor source binding closes queued same-ID retargeting. Draft03 rejected
for missing same-pane credential retirement at new entry fences; corrected04
keeps requested-user mismatch quiet, uses existing guarded retire at both fences,
and adds one token/transport rotation branch within the new regression. All
profile drafts remain unapplied, gated on full76 and final review. No parity claim.


Resource-side-thread milestone76 VERIFIED5,150/0/7, identical73b/74/75 binary
8c29f41b...dd471e9f228,867,072bytes/mtime. All prior5148/original92/ignored statuses
retained. Focus6reconnect/2thread/13workflow; full226.71s, no
unchanged recompile. Core5467+doc exact2065 inputs;5899 desktop inputs/binary
retained as5150. Rebuild568.48s/min commit9.850GiB/peak owned17.562GiB. Compile73
new-test errors and two-file-only correction preserved. HEAD/production unchanged;
no manifest/profile/lockfile change. Targets53.309GiB/free
143.116GiB after76. Checklist: resource-side-thread continuation
verified at actual offline closed-transport parent/error scope, visible/no-thread/
workflow-hidden cases. Drafts/cached replies/read revision/navigation retained;
original loader guards strict. Native/live/layout/success API and held-success
visibility OPEN. Next canonical profile failed-idle recovery source-approved in
corrected04; original03 retirement blocker retained and corrected via guarded
same-pane retirement plus one branch within new regression. Apply exact four-file
draft, then focused and one full gate. Goal ACTIVE/full parity unachieved. No
production/native probe/cleanup/live action/commit/push.


Known canonical profile reconnect corrected04 applied as4reviewed files after
verified5150. Full original ProfileVisit is captured before future queueing;
requested-user mismatch quiet, same-owner credential retirement guarded at both
entry fences. Failed-idle canonical scheduler pins original ProfileVisit+restore,
atomically sets busy then awaits existing reader without another handoff. Search
hints/Canvas-owner/workflow-hidden panes excluded; original response/cache guards
retained. Two regression cases include actual parent beside Files, queued valid/
busy/reopen/restore and constructor pre-poll replacement/credential retirement.
No cache DB or realtime connection in test fixtures; closed HTTP transport only.
Root formatting/whitespace PASS, Core2065 exact;5901 desktop inputs frozen before
77. Focus77 runs, then existing profile navigation/search/People groups and one
full80 if green. No new-profile pass/API/native claim yet; rejected03 preserved.
Existing child-task cancellation remains outside failed-idle scope.


Known profile77 PASS2/2,585.32s/min commit9.995GiB/peak owned17.625GiB.
Navigation78 PASS10/10, pane-only search79 PASS4/4, People79b PASS4/4 on identical
228,867,584bytefc2e597a...f1314fc5 harness/mtime. No changed-binary recompile
for focused groups; Core5467+doc exact2065 inputs. Full80 active, no5152 full
claim yet;5901 desktop inputs frozen. Targets53.309GiB/free142.171GiB after79b.
Next Thread status/visibility four-file draft source-approved, still unapplied.
Terminal-helper test is policy evidence, not async HTTP success proof; hidden
mark test extends real workflow parent with positive eligibility and cursor/anchor
preservation. Review wording repeats80; actual next new milestone gate is84
after current80 clears. Native/live/layout/full parity stay OPEN.


Known-profile milestone80 VERIFIED5,152/0/7 on identicalfc2e597a...f1314fc5
228,867,584byte harness/mtime; focused2new/10navigation/4search/4People all passed.
All prior5150/original92/ignored statuses retained. Rebuild585.32s/min commit9.995;
full228.96s, unchanged binary. Core5467+doc exact2065 inputs;
5901 source/binary retained as5152. Targets53.309GiB/free
141.931GiB after80. Checklist: normal canonical failed-idle
profile re-drive, source-bound queue/refusal and same-owner credential retirement
verified at closed-native-reader/error scope. Success API, previous child-task
cancellation, native/live/layout OPEN. Next Thread status/visibility draft approved,
apply only after80; status helper policy is not successful async HTTP proof.
No production/native probe/cleanup/live action/commit/push; full parity ACTIVE.


Next Thread accepted-status/visibility draft02 applied as4reviewed files after
verified5152. Found accepted replies now clear old error; missing-target error/
toast preserved. Shared reading_thread requires actual thread_visible, and
mark_thread_read denies hidden pane before native focus. Original read/request/
merge/hydration guards retained. Request factory seam keeps same native API and
shared driver; two new tests cover status policy and decoded success through
real continuation. Existing workflow parent extends positive/negative visibility
and actual hidden mark before reconnect, preserving cursor/revision/cache/drafts/
anchor. Root fmt/whitespace PASS;Core2065 exact,5901 desktop inputs frozen before
81. Focus81 then82visibility/83workflow/83bunread/full84 on same binary if green.
No new pass or physical HTTP/native/live claim; detached subscription settlement
unproved. Original01 retained, improved02 source-approved by Amp.


Thread status/visibility milestone84 VERIFIED5,154/0/7, identical980126cf...568bf83c
228,971,008byte harness/mtime. Focus4loader/2visibility/13workflow/3unread passed;
full226.52s, all prior5152/original92/ignored retained. Rebuild
563.69s/min commit10.277GiB/peak owned17.676GiB. Core5467+doc exact2065 inputs;
5901 source/binary retained as5154. Decoded controlled-success continuation clears
old error; missing-target/unrelated-toast semantics remain. Actual workflow-hidden
thread invocation stops before focus with cursor/anchor/cache/composer text intact.
No physical HTTP/native-focus/cancellation or all detached-task proof. Targets
53.31GiB/free140.745GiB after84. Next reviewed
channel transcript/workflow visibility two-file draft may now apply. Modal email/
number source gap additionally needs deliberate Home-only parser boundary review;
not just renderer controls. No new modal edits. Native/live/layout/full parity OPEN;
no production probe/rebuild/cleanup/live action/commit/push.


Workflow channel visibility exact2file draft applied after5154. Added only current
entry.showing exclusion to transcript_visible, preserving all existing read guards.
Acyclic Timeline ownership path source-approved by Amp. One actual Timeline-origin
regression supplies visible eligible(true) witness, restores current workflow and
checks hidden/ineligible state, then actual mark_visible_read before native focus
with full bag/cursor/revision/pending marks/composer text preserved. Root fmt/
whitespace PASS; Core2065 exact,5901 desktop inputs frozen before85. Focus85 entry,
86read prefs/87visibility/full88 if green. No current pass/native/live claim;
both composer text fields are checked, not every composer-state field.


Channel/workflow milestone88 VERIFIED5,155/0/7, identical85/86/87 harness
bf2c81f919e9242094f9a993c7eb2c08a8811bc2a62fbea06295b558eeb42b8b,229,071,872bytes/mtime. All prior5154/original92/ignored retained;
focus14entry/37prefs/2visibility; full224.25s. Rebuild564.72s/min
commit10.371GiB/peak owned17.663GiB. Core5467+doc exact2065 inputs;5901 desktop
inputs/binary retained as5155. Targets53.31GiB/free
139.74GiB after88. Actual Timeline workflow eligibility/hidden
mark denial verified with cached unread state and composer text retained. Native/
live/layout/open-mark cancellation unproved. Next Modal scalar source investigation
uses REA skill/static factory evidence; exact public B2 transport decoded to
recorded7cbc5803 SHA, original encoded response preserved. No execution/session
extraction. Component overrides recovered; reset/validation still being traced.
No new Modal source change or parity claim. No production rebuild/probe/cleanup/
commit/push/live action. Full parity ACTIVE.

Next submit-only normal-Modal email batch is applied, UNVERIFIED. Scope is absent/
empty initial values; fresh nonempty saved email, prefilled email, number inputs,
outward dispatch/confirmation/visibility rules remain closed. Generic parser
defaults and original tests unchanged. Exact public B2/S2/R decoded SHAs match
recorded originals. Fifty-nine source factories, provenance, callback contract,
REA static evidence export retained; no target code executed. Initial omission
and explicit empty callback remain distinct, shared rhb validator reused without
Home state changes, retained values/server errors survive updates, captured input
bindings recheck editable presentation/field/full native credential ownership.
Amp plan01 approved; source02 found permanent disabled-at-mount binding, corrected
before compilation and source03 approved. Generated regression includes disabled
mount -> production supported update -> same ElementId accepted edit. Seven new
Core/four desktop tests and four normal/narrow/error fixture names (registration
only). Core focus89 running, then independently locked full+doc, desktop focused/
full on one rebuilt identical binary. Prior5155desktop/5467Core results remain
baselines, not passes for changed source. No production/native/live/layout claim.

Core89 focused307/307 PASS including all7 new email cases;115.8s rebuild,
minimum commit17.997GiB/peak owned9.127GiB. Targets53.264/free140.148GiB after89.
Core90 full/doc active on identical2067 source inputs. Before desktop compilation,
root added a mounted binding Weak guard for removed/recreated same-ID fields;
strong Rc belongs only to Control hook, ordinary updates retain it, unmount
retires old callback. Existing new credential regression isolates binding drop.
Amp exact3file source04 approved; no extra test count/Core source change. Desktop
remains unverified. Read-only next-candidate URL inventory located exact Modal
registry, URL initial projection and literal callback/URL-parse validation route;
no URL application edit or native/runtime predicate execution. Email gates first.

Core90 VERIFIED5,474/0/6 plus1doctest, identical89 harness
11ca3d9f238849cc8606cf025926eb4ec60424f63719619ec48cbdf5c6394292,
201,922,560bytes/mtime. All prior5467 passes/ignored retained,2067 exact inputs,
679.48s total/632.30s tests, min commit23.789GiB. Retained core5474 binary/source.
Targets53.234/free140.330GiB after90. Desktop91 compilation failed before tests
with one E0603 private reads-module import (288.58s/min14.172GiB/peak13.371GiB).
Existing public CursorOwner re-export fixes the import; no visibility/Core/config
change. Failed log/source retained; root fmt passed and same-config91b rebuilds.
Pre-start inventory assertion found an older ignored target/tests.rs under actual
Profile source, mtime2026-10-19,5 cases already passing in5155. It is untracked/
ignored by **/target/, not a new batch edit or HEAD file. Current5907 snapshot
includes it; earlier5901 hash inventories omitted it. Old binary results remain
actual; no retroactive complete hash attribution. First snapshot captured after
91 startup (no application source edits during build); corrected before91b
snapshot frozen. All9 changed/5 new batch source files retained, Core2067 exact.
No new desktop/runtime/native acceptance or production rebuild/probe/cleanup.


Modal email milestone93 VERIFIED5,159/0/7, same f7da9b21...f32cb212
231,783,936byte/mtime harness as focus91b76/76 and92Home34/34. Prior5155,
original92 and ignored7 retained; Profile257/Canvas421/sharing6/Home280/List330
remain green. Rebuild563.01s/min9.861GiB/peak17.648GiB; full244.62s. Fresh
Core5474/0/6+1doctest,2067 exact inputs, desktop5907 source/binary retained.
Targets53.245/free139.473GiB after93. One compile-only E0603 in91 retained and
corrected with existing public import, no configuration change. Coverage correction
for older ignored source target/tests.rs is explicit; five tests already passed
in5155, no retroactive full hash claim for old5901 inventories. Product PARTIAL:
blank-initial submit-only normal-modal email, exact omission/empty and validator,
retained rows/errors, owned generated callbacks. Prefilled/restored-nonempty email,
number, outward dispatch/confirm/visibility still unsupported; native input/focus/
layout/session0/live/full parity OPEN. No production rebuild/probe/cleanup/live
write/commit/push. Next URL static source32factories and Amp conditional plan01
approved;93 condition now met. No URL source edits before verified closure.

Next URL batch is APPLIED UNVERIFIED after5159 closure. Exact32factories and
OL35 required-message source retained; own pure Node-host predicate59 cases
(48accept/11reject), not downloaded factory/native/Slack execution. Blank-initial
normal-modal scalar constructor adds typed URL omission/clear/literal validation;
generic/email-only/Home defaults and other unsupported guards retained. Shared
binding captures Email/URL kind plus every prior full credential/presentation/
field/Weak-lifetime guard; Email facade and original tests unchanged. Shared
native control sets form="" only on new URL fields, leaving existing controls'
association intact; no Enter or form-wide validation bypass. Five Core/two
desktop tests and4 fixture names, registration only. Amp draft01 identified one
new success-test literal contradicting oracle rejection; corrected before compile,
adding retained-invalid/submission-refusal assertion without changing predicate/
corpus. Conditional focused/full gate approval applies. Core2070 exact inputs
frozen before focus94, currently compiling. Fresh Core full/doc then desktop
focused/full required. Prior5159/5474 baselines do not pass new source. Source/
evidence/cache/limits/restrictions/native0 blocker remain unchanged.

URL core focus94 VERIFIED312/312, all5 new cases and59 Rust/Node predicate
comparisons,119.56s/min18.086GiB/peak9.499GiB. Actual Home/default/email-only
admission rejection passes; original Email tests byte-identical to5159. Core
full95 active,2070 exact frozen inputs. Targets53.238/free138.987GiB after94.
No URL desktop compilation/pass/native acceptance yet; Root5159 remains baseline.

URL Core95 VERIFIED5,479/0/6 plus1doctest, unchanged94 harness18543063...326400b,
202,003,456bytes/mtime. All prior5474 passes/ignored retained,2070 source inputs
unchanged;690.22s total/633.04s tests, min22.908GiB/peak4.223GiB. Retained5479
binary/source. Targets53.264/free138.617GiB after95. Full5914 desktop inputs
frozen before first focused96 app-modal rebuild;11 changed/7 new inputs relative
to5159. Single devfast1/processnoinc/samefeatures/linker/target and all stop/floor
limits preserved. Source review conditional correction is verified by312focus,
including independent59case predicate comparisons; no original Email tests changed.
No URL desktop/runtime/native acceptance or production rebuild/probe/cleanup.


URL milestone98 VERIFIED5,161/0/7, identical000338d2...a27e1264
231,844,864byte/mtime harness to96app-modal78/78 and97Home34/34. All prior5159,
original92 and ignored7 retained; Profile257/Canvas421/sharing6/Home280/List330
remain green. Rebuild644.27s/min9.457GiB/peak17.717GiB; full240.57s. Fresh
Core5479/0/6+1doc,2070 exact inputs; desktop5914 source inventory/binary retained.
Targets53.263/free137.806GiB after98. Actual generated callbacks preserve raw
schemeless URL through typed submission and invalid/required refusal, URL-only
form attribute, mixed Email/plain behavior; owned realtime draft remains intact.
59 bounded Node/Rust predicate comparisons; no target/native/Slack execution.
PARTIAL blank-initial normal-modal quiet controls,4 fixtures registration only.
Prefilled/restored URL and outward dispatch/confirm/visibility stay closed.
Source assertion contradiction caught/repaired before compilation; no old test
changed or actual URL test failure. Native ownership/Enter/focus/sanitization/
layout/live/full parity OPEN; no production rebuild/probe/cleanup/commit/push.
Next fresh-prefill proof18+7source records shows literal writes commute with no
raw saved/owned URL row; getInitialViewState attribution corrected as baseline
only. Conditional Amp prefill plan01 approved;98 milestone condition now met.
No prefill edits before verified closure; preserve old constructors/blank-origin
guards via explicit per-field provenance, no original passing-test rewrite.

Fresh URL prefill draft applied after5161 closure, UNVERIFIED desktop. Scope
refined before first compile: initial owned form only, no raw matching saved row;
active grants retained matchingURL, bounded initial-key history makes recreated
prefill slots sticky unsupported even blank/repeated updates. No grant for newly
introduced prefills in existing form or blank-origin initial changes. A missing
model key alone cannot prove widget disposal before batched render. Old modes/
tests unchanged. Four newCore/one actual generatedDesktop cases,2 named fixtures
registration only. Tests include no-input typed prefill, literal updates, saved/
interaction/foreign-view guards, pristine/dirty retention, batched optional blank
recreation refusal and fresh-owned-parent recovery. Scope drafts/source archived.
Amp draft01 approved source gates; retained-value lifecycle merge still needs
tests, parent/controller lifetime source audit saved without native claim.
Core99 focus316/316 PASS,113.03s/min17.626GiB/peak9.444GiB; all4 newcases. Full100
active on2071 frozen inputs. Targets53.285/free137.669GiB after99. Source snapshots
5161/5479 remain baselines across this new source; no prefill desktop pass/native
acceptance/production rebuild/probe/cleanup/commit/push/live action.


Fresh URL prefill Core100 VERIFIED5483/0/6 plus1doctest, identical99
b7274e8f...f6c34 harness/202081792bytes/mtime. All prior5479 passes and ignored6
retained on2071 exact inputs;685.21s total/635.54s tests, min22.579GiB/peak4.721GiB.
Targets53.300/free137.650GiB after100. Core5483 binary/source retained. Root
formatting PASS;5916 desktop inputs frozen before app-modal101 rebuild, exact
8 changed/2 new files relative to5161; final10-file draft03 retained. Desktop
focused Home102 and one full103 remain pending. No new desktop/native/parity
claim, unchanged production executable/configuration/HEAD and all restrictions.


Fresh URL prefill milestone103 VERIFIED5162/0/7, identical36eebff3cad2619470c6b9b915ceb65d936fb43168034908247eaab90e85d484
231920128byte/mtime harness to101app-modal79/79 and102Home34/34. All prior5161,
original92 and ignored7 retained; affected Profile257/Canvas421/sharing6/Home280/
List330 green. Rebuild659.73s/min9.162GiB/
peak17.626GiB; full242.04s. FreshCore5483/0/6
plus1doc,2071 exact inputs; desktop5916/binaries/source retained. Targets53.3
/free136.851GiB after103. Four newCore/one actual generated/realtime Desktop
cases passed; no-input literal typed prefill, owned pristine/dirty updates and
batched optional blank recreation refusal/fresh-parent recovery. PARTIAL initial
owned normal-modal only/no matching saved row; later introduced/restored/remount
cases excluded. All old passing tests/constructor guards retained. Two fixtures
registered only; native/live/visual/Enter/IME/full parity OPEN, session0 unchanged.
No production rebuild/probe/cleanup/commit/push/live action. Next REQUIRED canonical
integer plan Amp01 condition now met; no number edits yet. Email prefill genuinely
blocked: exact25records/23edges show type-only/value whole-row writes do not commute;
actual effect ordering needed, no URL analogy. Goal ACTIVE and all limits unchanged.


Required integer batch APPLIED UNVERIFIED after recorded5162 closure. Fresh
owned initial normal-modal required canonical0..999999999, false decimals/no
bounds/no matching saved/no dispatch/confirm/visibility. Private ScalarInputs
groups original flags plus explicit new mode; legacy entry points unchanged.
Integer active grants can only shrink after first parse; no blank fallback or
retained new grants, so removed/config-changed/recreated/ungranted slots stay
unsupported without history eviction. Existing URL history unchanged. Shared
Kind/credential/presentation/Weak native scalar control gains number/required
with no icon/form override/step/min/max/pattern/Enter/JS. Home numeric source
unchanged; specialized Home error route retained. Invalid literals replace owned
value and block, clearNone required; exact OL35 required string. Five newCore/two
actual generated-realtime Desktop cases,2 fixture aliases registration only.
Dualfmt PASS; exact15-file draft01 under Amp review. Own Node32cases+1024 sampled
Number identity PASS, no downloaded execution. Rust/native integer unverified.
No source/config/manifest/lock/dependency/default cleanup or production/native
probe/commit/push/live action; all limits/restrictions and goal ACTIVE unchanged.


Amp exact integer draft01 source-approved staged offline gates; no blocking defect
identified, Home checks specifically required because shared values::error routing
changed. Actual dualfmt0/0 recorded,15-source archive exact before Core104. Core104
focused block_actions::form compiling on2074 frozen inputs, expected321 unverified.
Prepared full105 freezer compares5483 prior passes/ignored6 and same104binary;
expected5488+doc is not a result. Desktop10681/107Home34/full1085164 pending.
No new pass/native/parity claim;5162desktop/5483core+doc remain prior verified
baselines. Initial-only grant set cannot grow/regrant in retained forms, no blank
number fallback; URL history preserved. No production/native probe or limit change.


Integer Core104 VERIFIED321/321, all5 new cases and32 bounded corpus comparisons.
186.40s/min15.431GiB/peak owned12.099GiB, no safety stop. Targets53.316/free136.028
GiB after104. Full105 now active on2074 frozen inputs; expected5488/0/6+doc is
unverified until actual completion and comparison to5483/ignored6. Desktop106/107/
108 remain pending, no new desktop/native claim. Prepared closure108 NOT executed.
Goal ACTIVE; next never-seen introduced-number static investigation is read-only,
not new product edits. All source/cache/evidence/safety/action restrictions remain.


Integer Core105 VERIFIED5488/0/6+1doc, identical10429e684f3...c99fb5d
201991168byte/mtime harness.5483 prior passes/ignored6 retained on2074 exact
inputs.722.79s total/640.82s tests,min21.838GiB/peak5.772GiB. Core5488 binary/
source retained. Targets53.267/free135.746GiB after105. Desktop5921 inputs
frozen before106 onejobdevfast/process-noinc app-modal rebuild, expected81
unverified;107Home34/full1085164 pending. Exact15-file source draft01 unchanged;
no new desktop/native/parity claim. Next introduced-number audit records fresh
INPUT vs reused container distinction and untraced final y4OD expansion edge;
no grant based on raw unseen block ID. All four retained JavaScript asset candidates
are existing B2/S2/R/encodedB2, no extra vendor bundle in bounded filename inventory.
No source/production/native probe/configuration cycle/cleanup/commit/push/live action.


Required integer milestone108 VERIFIED5164/0/7, identical93b913d88e79b010f83fe51788ac379a818eb6884021bd5bad0ded4af267974e
231959552byte/mtime harness to106app-modal81/81 and107Home34/34. Prior5162,
original92 and ignored7 retained; Profile257/Canvas421/sharing6/Home280/List330
green. Rebuild657.19s/min9.788GiB/peak
17.669GiB; full240.54s reused MAINbinary.
FreshCore5488/0/6+1doc,2074 exactinputs; desktop5921/binaries/source retained.
Targets53.268/free134.938GiB after108. Five newCore/two actual
generated/realtime Desktop cases passed; no-input0/1literal payload, invalid/clear
blocking, owned pristine/draft/error retention and batched recreation/oldOwner
refusal.32 Rust/Node corpus/1024 own Node identity checks; no downloaded execution.
PARTIAL initial-owned required0..999999999 only/no bounds/saved/dispatch/optional;
old constructors/tests/Home number behavior and URL history retained. Two named
fixtures registered only; native sanitation/IME/Enter/association/layout/live/full
parity OPEN, session0 unchanged. No production rebuild/probe/cleanup/commit/push/
live action. Next introduced-number source audit distinguishes fresh INPUT parent
from reused existing container; no next product edit at closure. Goal ACTIVE.


Signed-integer batch APPLIED UNVERIFIED after recorded5164 milestone108. New
explicit signed constructor, immutable private mode copied by parse_update; old
nonnegative constructor/domain/canonical function/32corpus/fiveCore/twoDesktop
tests and native shared Owner/view pins unchanged (hash checks). Default values::
error body unchanged; new error_in scopes all11 modal parse/saved/edit/validate/
reconcile OLD comparison +NEW fallback/server/submitted paths. Canonical signed
reuses unsigned magnitude after one minus, excludes-0/leadingzero/decimal/exponent/
space/out-of-range. Same initial-only shrink-only grants/quiet required profiles;
no saved/optional/bounds/dispatch/confirm/visibility/newslot grant. Native ctor
selection and2 fixture aliases only; no new renderer/style/icon/API/JS/Enter.
Two newCore/one actual generated Desktop method, separate30case corpus; own
Node30/2048 sampled identity PASS, no downloaded execution. Dualfmt0/0; Amp exact
draft01 approved staged gates, accepts prior108 closure as supplied evidence.
Core109 focus compiling on2076 frozen inputs; expected323 unverified, full110
expected5490+doc then desktop11182/112Home34/full1135165 pending. Signed source
archive12files/preimages retained. Prior5164/5488+doc are verified baselines across
this new source, no new Rust/native pass. Source/cache/evidence/limits/restrictions
unchanged; no production rebuild/native probe/cleanup/commit/push/live action.


Signed Core109 VERIFIED323/323, both new semantic cases and30 signed-corpus
comparisons; old32/nonnegative cases retained.107.50s/min17.727GiB/peak9.371GiB,
no stop. Targets53.251/free135.441GiB after109. Full110 active on2076 frozen
inputs; expected5490/0/6+doc remains unverified until actual exit/old5488+ignored
comparison/same109binary. Desktop111/112/113 pending; no signed desktop/native
claim. All source/pins and safety/action restrictions unchanged; goal ACTIVE.


Signed integer milestone113 VERIFIED5165/0/7, identical98dbe1edf7dfadac938c4f4685731b40b22061c50d73140c5c80d38f13efdfc5
231979520byte/mtime harness to111app-modal82/82 and112Home34/34. Prior5164,
original92 and ignored7 retained; Profile257/Canvas421/sharing6/Home280/List330
green. Rebuild662.33s/min9.62GiB/peak
17.66GiB;full237.04s reused MAINbinary.
FreshCore5490/0/6+1doc,2076 exactinputs;desktop5924/binaries/source retained.
Targets53.234/free134.74GiB after113. Two newCore/one actual
generated Desktop methods passed: initial-1 readiness withoutcallback, literal
negative edits, invalid replacement/clear refusal, scoped derived/server-error/
retained/accepted-step validation. Old nonnegative ctor/canonical/corpus/tests/
controller/view pins unchanged; default error body and Home specialized path
retained.30 Rust/Node signed corpus+2048 own Node identities;old32retained.
PARTIAL initial-owned quiet required signed range only;optional/bounds/decimal/
saved/newslot/remount guards remain. Two fixtures registered only;native sign
entry/buffers/IME/Enter/association/layout/live/full parity OPEN,session0 unchanged.
No production/native probe/cleanup/commit/push/live action. Next source references
installed static resources (cohort separation) and actual People continuation
contract;no new product edits at closure. Goal ACTIVE and all limits unchanged.


Post113 reference triage: globalPeople source38+59edges shows numbered cached
search.modules.people pages, sort=relevant (not score), hit/highlight carriers
(not fullUser snapshots), canonical/unknown member lookup and independent profile
sections. Actual prefs/features, LH2X/header andgenericrequest helpers remain
unresolved; no forced20 or guessedUser hydration/GraphQL contract. Installed4.52.178
staticASAR450files lacksB2/S2/R linkage/y4OD/q1tI; old vendor9j8p numeric identity
recovered with qualifiedhistorical provenance. Nativeuntrustedpayloads unused; all
REA references leftlive, no close/reset/evidencecleanup. PrimaryJSONledgers retained.
Next boundedEdge continuation reference23edges:repeatoriginal30/top-levelmarker,
empty+truthycontinues, rawstop>=30/rejectionabort. Amp plan01 now reviewing explicit
compatibility: preserve39raw->30unique legacy renderer, departSDK finalrawslice
openly; add each-await ownership/clientconfigretirement and localmarker/request
bounds. No next product edits yet;5165/5490+doc remain latest. No globalPeople
paging/live/native/fullparityclaim and no buildconfiguration/actionlimit changes.


BoundedEdge continuation batch APPLIED UNVERIFIED after5165 closure.6 changed/
4 newfiles; same originalcount30/top-levelopaquemarker/returnedraworder, empty+
truthycontinues, no requests afterraw>=30. Wholefinaldecodedbatch retained for
unchanged firstunique/nonemptymax30 finalizer; explicit localcompatdeparture from
SDKrawslice preserves39raw->30unique regression, no refillafterfiltering. Typed
ok/results/failed_ids behavior sharedexactly with legacyFnOnce API. NewCoreNone
outcome forretiredowner; checkbeforeeachrequest/afterawait BEFORE errorpropagation.
Localstring<=4096/cycle/30requestsafety bounds onlywhencontinuationneeded, no cursor
logging/total/UIpaging. StableclientconfigEq (notgeneratedUUIDs) strengthensSearch
identity; normal rawquery/dialog guard tightened. Normal+recovery actualproduction
continuedpaths share owner/preflight/finalizer; legacyFnOnce wrappers explicitsingle
mode/no fallbacktransport/no syntheticHTTPenvelope. ExactrequestLease now covers
normal cancellation/successor protection aswell asrecovery. Three newCore/four new
Desktop methods; alloriginalAPI/People/recovery/pager testbodies/pins unchanged.
Dualfmt0/0 and Amp exactdraft01 approved sequential offlinegates. Core114 focused
users_search compiling on2078frozeninputs, expected6unverified;115full5493+doc and
desktop116search/117profiles/118full5169pending. Prior5165/5490+doc remainverified
baselines, no newpass/native/live/globalPeople/fullparityclaim. Source/caches/limits/
restrictedactions unchanged; no production rebuild/nativeprobe/cleanup/commit/push.


**Latest verified desktop milestone: 5,169 passed / 0 failed / 7 existing ignored.**
Bounded Edge People continuation passed46 Search and4 profile-search focus, then
full118 on the identical 232833024-byte harness, SHA256 `2ab18eb87403bd7aaf03af94302e7ec1ee3f0d2ce34128f53edb677714fc19c5`,
with unchanged modification time. All prior5165 passes, original92 repairs and
ignored7 retained; affected Profile/Canvas/sharing/Home/List groups remain green.
Rebuild 658.77s, minimum commit 9.317GiB,
sampled owned peak 17.666GiB; full 242.33s.
Fresh independently locked Core5493/0/6 plus1doctest, same114focus harness,
2078 exact inputs. All prior5490 Core passes and ignored6 retained. Desktop5928
selected inputs and both baseline binaries retained. Evidence:
`tmp/parity-search-recovery-20261027/people-edge-continuation-milestone.json`.

Normal and outage-recovery People lookups now follow top-level Edge next_marker
until accumulated raw count reaches30 or the marker is falsy. Every request keeps
count30 and the original flags. Empty results with a marker continue. API, rate,
decode and continuation errors publish no partial success. The final decoded batch
stays whole for the existing first-unique/nonempty/max30 projection: this is an
explicit compatibility departure from Slack SDK raw slicing, not a parity claim.
No refill after deduplication, global People numbered pages, total or sort changes.
Each request/await/successor/publication checks the original account, credentials,
transport, stable client configuration, raw dialog, query/kind/sort/page and exact
request/presentation owner. Retired results and errors are quiet. Normal/recovery
share finalization and exact-request cancellation leases; legacy FnOnce seams,
original tests and pager remain intact. Opaque string <=4096 bytes, cycle detection
and max30 requests are explicit local resource bounds, not server guarantees.
Transient results never insert canonical membership or mutate history/composer.

Signed integer5165, nonnegative5164 and freshURL5162 retain their scoped evidence.
Email prefill remains blocked by actual parent/leaf ordering. Introduced-number
final expansion/key preservation is untraced; unseen IDs do not grant fresh life.
Native sign entry/buffers/IME/Enter/association/layout/live/full parity remain OPEN.
The session0 executor blocker is unchanged; no repeat native probe or workaround.
Targets 53.275GiB, free disk 132.59GiB after118. Production
executable and HEAD unchanged. Historical source-inventory coverage correction for
ignored target/tests.rs remains explicit. Goal ACTIVE. Next safe source-grounded
slice: assess shared List user/channel directory continuation with exact query
routes, preserving eligibility, ranking, caches and all existing owner guards.
Desktop dev-fast/one job/process-only no-incremental, Core/ordinary8; single Cargo
pipeline, stable caches/linker/features. Stop5/hard4GiB commit, free50GiB disk,
helper56GiB under conditional96GiB ceiling. No cleanup/commit/push/live action,
production rebuild/native probes/device/audio/OS input/security/registration.



**Latest verified desktop milestone: 5,173 passed / 0 failed / 7 existing ignored.**
List directory continuation passed17 cell/11 creation/46 Search focus, then full125
on the identical 233212416-byte harness, SHA256 `60ca7b41c9dae13e565776121049b76bfbc1c326c06fa3082a191619aecf3a3b`,
with unchanged modification time. All prior5169 passes, original92 repairs and
ignored7 retained. Profile257/Canvas421/sharing6/Home280/List comments330 and
usergroup recovery22 remain green. Rebuild 661.29s, minimum commit
8.894GiB, sampled owned peak 17.904GiB;
full 244.09s. Independently locked Core5496/0/6 plus1doctest,
same119 Channel3/120 User6 harness,2081 exact inputs; all5493 prior passes and
ignored6 retained. Desktop5934 selected inputs and both baseline binaries retained.
Evidence: `tmp/parity-search-recovery-20261027/list-directory-continuation-milestone.json`.

Existing List User/MultiUser and Channel/MultiChannel cell pickers plus People/
Channel field-default pickers now complete their bounded Edge query across short
service batches. Original count30 repeats with top-level marker; empty+marker
continues; raw>=30 stops before unused marker validation. Whole final batch stays
available to unchanged ranking/eligibility/cache veto and visible limit. This is
the explicit SDK raw-slice compatibility departure, with no refill after filtering.
Shared raw-page collector preserves User continuation behavior and endpoint errors;
Channel has its own strict continued decoder. Legacy single-call APIs/decoder,
thirteen domain/owner/test pins and global Scope/Context remain unchanged. The
separate channel check_membership flag discrepancy is excluded.

Original Request and captured stable client configuration guard each request,
await/error/decode/successor and final ingress/success/failure. Quiet retirement
publishes no partial results or stale errors. Query-matched loading cleanup keeps
cache/error/cooldown and cannot clear a successor spinner; creation notifies the
shell after admitted registry cleanup. Cell channel ingress remains final-success
only; tests inject a callback into that exact branch without disk/network writes.
Creation has no ingress. Three Core/four actual Desktop methods cover both domains
and owners, later eligible matches, raw30 with fewer choices, current rate failure
cache/cooldown, stale first error/second success and newer loading. No selection,
default-setting, save, membership or live mutation is added. Local marker/cycle/
30request bounds remain explicit resource policy, not server guarantees.

People continuation5169, signed integer5165, integer5164 and freshURL5162 retain
their original scope. Global People pages, email-prefill effect ordering and
introduced-number final expansion/key preservation remain unresolved. Native
sign/buffer/IME/Enter/association/layout/live/full parity remain OPEN. Session0
executor blocker is unchanged; no repeated probe or workaround. Targets
53.303GiB, free disk 130.71GiB after125. Production executable
and HEAD unchanged. Historical ignored target/tests.rs inventory correction stays
explicit. Goal ACTIVE. Next safe work: separate24-result user selector reference,
preserving raw-first malformed-row handling and dispatched429 admission; do not
generalize the People/List decoder or erase stale-request cooldown ownership.
Desktop1 dev-fast/process-only noincremental, Core/ordinary8; single Cargo pipeline,
stable caches/target/linker/features; stop5/hard4GiB commit/free50GiB disk/helper56
under conditional96GiB ceiling. No cleanup/commit/push/live action, production
rebuild/native probes/device/audio/OS input/security/registration changes.



**Latest verified desktop milestone: 5,177 passed / 0 failed / 7 existing ignored.**
Users-selector continuation passed29 Home/33 Modal/37 message/46 Search/17 List
cell/11 List creation focus, then full138 on identical 233584640-byte harness
SHA256 `1fede7a9934359c269035964ee1dcc81024a91b0c56efbcfc2b4b3891674d861`, unchanged modification time.
All prior5173 passes, original92 repairs and ignored7 retained. Affected groups
retain their exact counts in the milestone record. Rebuild 591.86s,
minimum commit 9.413GiB/owned peak
17.724GiB; full 241.55s. Core5502/0/6
plus1doctest passed on identical126–130 focused harness,2085 exact inputs;
all5496 prior passes and ignored6 retained. Desktop5946 inputs/both baselines
archived. Evidence: `tmp/parity-search-recovery-20261027/users-selector-continuation-milestone.json`.

Existing Home/Modal/message User/MultiUser lookups complete short scalar Edge
batches to24 raw rows, then clip before permissive per-item decoding. Invalid,
duplicate and ineligible rows consume raw budget; no eligibility refill. Existing
flags/query preprocessing/array+metadata acceptance/filters/ranking/selected values
and canonical membership remain unchanged. Bulk queries and filtered conversation
fanout stay separate. Builder/executor None discards accumulated data and bypasses
completion. Dispatch admission occurs after link readiness; known429 account-budget
observation occurs before original result retirement; publication retains original
owner/query/presentation guards. Current-query429 is not masked by its own cooldown.
Home fixtures/injected reads use the same callbacks. Cancellation may drop a
dispatched read before observation; no detached or unconditional settlement claim.
Lookup and app-action budgets stay separate; queries grant no selection/send/replay.

Legacy transport current-check sequence/retry loop/classification/delays/errors
match the prior body except explicit hooks/Option wrapper. Original transport6,
User6/Channel3 tests,14 original source/test pins and embedded discovery tests
remain unchanged. Raw-policy3/admission3 Core methods and four actual desktop
methods passed. Local4096byte/cycle/max30-request bounds remain resource policy,
not server guarantees. Root132 compile failed before tests on3new-test private
map reads and2handle borrows; failed source/log/metrics retained. Draft02 corrected
only4new test/support files via cfgtest immutable accessors/copy handles; assertions
retained/strengthened.132b Home28/1 and134 message36/1 exposed new-test setup
failures; Modal33/Search46/List17+11 passed on that same failed harness. Draft03
changes only2new tests: retained captured Home form plus current-access denial,
and actual message display projection before capture. Failed source/binary/logs
remain retained. Same-config132c and133b?137b establish the corrected focused result.
No Core rerun after test-only correction; exact2085 inputs carry5502. Source review
preparation oversize69263>64000 was retained and split before any CLI invocation.

Prior List5173, People5169 and signed-integer5165 retain their scoped evidence.
Optional-integer expansion remains BLOCKED on native invalid/transient/unseen
buffer proof despite Slack optional-null state proof; current required-only
protection remains. Email-prefill ordering, introduced-number expansion/key edge,
global People data/preferences and native sign/IME/Enter/layout/live/full parity
remain OPEN. Session0 executor blocker unchanged; no probe or workaround.
Targets53.314GiB/free128.973GiB after138; production executable
and HEAD unchanged. Historical target/tests.rs source-coverage correction remains
explicit. Goal ACTIVE. Next safe candidate: source-proven ordinary query flags
and Home Channel/MultiChannel raw24 completion, keeping channel query-gated429,
filtered aggregates/generic30/Profile25 and sidecar hydration separate. No next
product edit is implied by source investigation alone.
Desktop1 dev-fast/process-only noincremental, Core/ordinary8; single pipeline,
stable caches/target/linker/features; stop5/hard4GiB commit/free50GiB disk/helper56
under conditional96GiB ceiling. No cleanup/commit/push/live action/production
rebuild/native probe/device/audio/OS input/security/signing/trust/registration.



**Latest verified desktop milestone: 5,180 passed / 0 failed / 7 existing ignored.**
Home channel/ordinary flags passed60 channel/29 HomeUser/33 Modal/38 message/
46 Search/17 List cell/11 creation focus, then full152 on identical
233743360-byte harness SHA256 `38d2a5cb281cc04d5777c1e06630331c2e661158cfdad633933a3f82760f73c4`,
with unchanged modification time. All prior5177 passes, original92 repairs and
ignored7 retained. Exact affected-group counts are in the milestone. Rebuild
650.21s, minimum commit 9.561GiB,
sampled owned peak 17.739GiB; full
237.28s. Independently locked Core5503/0/6 plus1doctest passed
on identical139-143 focused harness,2086 exact inputs; all5502 prior/ignored6
retained. Desktop5949 input snapshot and both baselines archived. Evidence:
`tmp/parity-search-recovery-20261027/home-channel-selector-milestone.json`.

Ordinary User/MultiUser requests now explicitly include_profile_only_users:true
across existing Home/Modal/message owners and markers. Ordinary Home Channel/
MultiChannel requests explicitly check_membership:true and complete short/empty
service pages to24 raw records, then clip before permissive item decoding.
Malformed/duplicate/ineligible rows consume raw budget; no refill. Captured query,
team policy and account route survive every page. No selection, form, canonical
workspace membership or app-action budget authority is added by lookup.

Channel429 observation retains BOTH captured owner and original query currentness;
stale channel queries cannot update budget. Users retain their separate stale-
account observation contract. Dispatch admission additionally checks cooldown at
builder/fixture and Transport after link readiness; refusal discards partial rows,
bypasses ordinary completion and does not invent a fallback deadline. Current
own429 still reaches error completion. One cancellation races the whole ordinary
read, including fixture waiting, then returns before filtered-path completion;
no detached or unconditional late settlement.

Private filtered builders/aggregate fanout remain byte-exact. Bulk/MPIM/Profile25/
generic30/List whole-terminal policy, eligibility/ranking/team policy and sidecar
hydration stay outside this batch. User raw API/signature/tests unchanged; private
raw24 endpoint factor supplies Channel continuation error identity. New Core1 and
Root3 methods passed. Existing wire tests intentionally gain exact source-proven
flag expectations, retaining route/auth/count/filter/team/decode assertions.
Fourteen legacy API/transport/domain/owner/test pins remain unchanged. Core first
format check required explicit Core formatting. Draft02 strengthens malformed-
input error assertions after source review; Draft03 adds helper reexports caught
before desktop compile. Earlier132/132b/134 failures and all evidence remain.

Ordinary include_record_channels remains UNPROVEN: declared default propagation
depends on missing qChh. Optional integer expansion remains BLOCKED on native
invalid/transient/unseen-buffer distinction; required-only protection remains.
Email-prefill order, introduced-number expansion/key edge, global People data/
preferences and native sign/IME/Enter/layout/live/full parity remain OPEN.
Session0 executor blocker unchanged; no probes or workaround. Prior5177/List5173/
People5169/signed5165 retain scoped evidence, not a full parity claim.
Targets53.319GiB/free127.991GiB after152; production binary
and HEAD unchanged. Historical target/tests.rs inventory correction stays explicit.
Goal ACTIVE. Next source investigation: filtered Conversation/MultiConversation
short-page completion under exactly proven masks, excluding remote MPIM and
feature-gated team filters unless their actual caller/semantics become established.
Desktop1 dev-fast/process-only noincremental, Core/ordinary8; one pipeline, stable
caches/target/linker/features, stop5/hard4GiB commit/free50GiB disk/helper56 under
conditional96GiB ceiling. Preserve source/caches/evidence. No cleanup/commit/push/
live action/production rebuild/native probe/device/audio/notification/OS input/
security/signing/trust/registration.



**Latest verified desktop milestone: 5,183 passed / 0 failed / 7 existing ignored.**
Ordinary Modal/message Channel completion passed34 Modal/40 message/60 Homechannel/
29 HomeUser/46 Search/17 List cell/11 creation focus, then full160 on identical
233940992-byte harness SHA256 `a5f8df9e1758fd42d134f1e89d9ef3d2330f48ef0d3e01aa0c14a7a1ad6b6a7c`,
unchanged mtime. All prior5180 passes, original92 repairs and ignored7 retained.
Exact affected-group counts are in the milestone. Rebuild582.92s,
minimum commit9.572GiB/owned peak
17.795GiB; full226.26s. Core5503/0/6+
1doctest carries from144 on exact2086 unchanged inputs, no unchanged Core rerun.
Desktop5951 inputs/binary and verified8-file source archived. Evidence:
`tmp/parity-search-recovery-20261027/modal-message-channel-milestone.json`.

Ordinary Modal/message Channel/MultiChannel now complete short service pages to
raw24, then clip before permissive item decode, without eligibility refill or
partial publication. BOTH actual ordinary control identity and exact public-only/
default filter are required. Equivalent explicit Conversations, broader masks and
only-team filters stay on the legacy aggregate route. Existing Enterprise refusal,
metadata policy/ranking, form/selection and action guards remain unchanged.
Original filtered request fields/auth/cache route including include_record_channels
remain; check_membership:true is explicitly aligned only for these four controls.
No Home internal-team policy transfer or canonical membership authority is added.

Owner-specific account-before-query429 scopes are byte-exact. Currentness excludes
cooldown; current successor429 reaches normal error finalization while a stale
query can update only its original account budget. Builder/executor None discards
partial data and bypasses error fallback/ingress. PostingIngress is captured before
fetch and called synchronously exactly once only after terminal accepted success,
including empty results; never partial/stale/error. Existing finalizers are reused.
No detached settlement or app-action replay.

All prior regression bodies remain unchanged;3 new methods verify actual captured positive/
negative dispatch, short pages/raw20-invalid overflow, current successor429, stale
success/rate, empty-ingress and narrow wire/filter refusal. Private aggregate
builders,14 legacy source/test pins and owner read constructors remain exact.
Source extraction punctuation and67919 oversized review preparation were retained
and corrected without source/test/config changes; the oversize never invoked CLI.
No Rust gate failed in this batch. Earlier132/132b/134 and all prior evidence stay
retained. Scope beyond these tests/native execution is not established.

Prior Home-channel5180/User5177/List5173/People5169/signed5165 evidence carries.
Broader Conversation/MultiConversation fanout/MPIM/team-filter and request-queue
parity remain OPEN. Source per-provider clipping/fanout does not approve a native
shared429 admission policy. Ordinary include_record_channels default propagation
still depends on missing qChh. Optional integer invalid/transient/unseen buffer,
email-prefill order, introduced-number key/expansion, global People data/preferences
and native sign/IME/Enter/layout/live/full parity remain OPEN. Session0 executor
blocker unchanged; no probes or workaround. Historical target/tests.rs inventory
correction stays explicit; production binary and HEAD unchanged.
Targets53.32GiB/free126.227GiB after160. Goal ACTIVE. Next:
source-grounded explicit Conversation member/channel completion only after precise
fan-in/admission/error precedence is reviewed; no inferred MPIM/default expansion.
Desktop1 dev-fast/process-only noincremental, ordinary8; one pipeline, stable caches/
target/linker/features, stop5/hard4GiB commit/free50GiB disk/helper56 under conditional
96GiB ceiling. Preserve source/caches/evidence. No cleanup/commit/push/live action/
production rebuild/native probe/device/audio/notification/OS input/security/signing/
trust/registration.



**Latest verified desktop milestone: 5,193 passed / 0 failed / 7 existing ignored.**
Explicit Conversation fan-in passed3 shared/3 Home adapter/35 Modal/45 message/
64 Homechannel/29 HomeUser/46 Search/17 List cell/11 creation focuses, then full170
on identical 234670080-byte harness SHA256 `bf0b46a649b49dda07ab159f94246598848d96caf85bc77752ba1464daefcd2a`,
unchanged mtime. All prior5183 passes, original92 repairs and ignored7 retained.
Rebuild598.75s/min commit9.269GiB/owned peak
17.766GiB; full223.3s. Core5503/0/6+
1doctest carries exact2086 unchanged inputs from144, no unchanged Core rerun.
Root5959 inputs/binary and verified21-file source archived. Evidence:
`tmp/parity-search-recovery-20261027/conversation-fanin-milestone.json`.

Actual Conversation/MultiConversation identity and explicit nonempty unique
im/public/private include subset use independent member/channel raw24 marker
completion before permissive decode. No eligibility refill or partial publication.
MPIM/default/empty/unknown/duplicate/team masks remain legacy. Existing native
finalizers/ranking remain; source canonical/ranking/A2TO equivalence is unresolved.
Native fan-in policy settles dispatched siblings then fences currentness: maximum
known429 wins before siblingNone; otherwise None bypasses fallback, then ordinary
Channel-before-Member errors. Shared cooldown admission has no own-query exception.
Modal/message account-owned and Home owner+query observer bodies are exact.
PostingIngress remains one synchronous callback only terminal accepted full success,
including empty; no partial/stale/error ingress, detached work or app-action replay.

Home registrations source-route through one Harness lane/nonce. Receiver-first
destruction and matching ownership cleanup preserve replacement slots; closed sender
tombstones retain old pending_closed evidence. Bundled initial original Result is
observed once, cached and current-fenced; its completed exchange releases before
successors. Initial barrier proves both enabled collector entry boundaries precede
consumption, not simultaneous underlying transport dispatch. First builder admission
remains; no cached-response retroactive admission or fixture live fallback.

Ten new methods verify raw budgets/error/currentness, original combined observation,
already-ready429 gating, both direct closed registrar refusals, held lane lifecycle
and all3 owner drivers/predicates. They do not execute production dispatchers or
establish native acceptance. Draft02 strengthens existing fixture methods after
review; 74039 oversized prompt preparation rejected beforeCLI and all drafts retained.
No Rust gate failed in this batch.14 legacy pins/private builders/owner scope bodies
remain exact. Earlier failures and qualified source-inventory correction stay retained.

Goal ACTIVE. Native/live/layout/IME/Enter/full parity remain OPEN; session0 executor
blocker unchanged, no probes or workaround. MPIM/default/team-filter/queue/ranking,
optional integer invalid/transient/unseen buffer, email-prefill order, introduced
number key/expansion and global People data/preferences gaps remain open. Calendar/
Profile independent source audits identify next safe product work, not parity claims.
Production executable and HEAD unchanged. Targets53.325GiB/free
124.054GiB after170. Desktop1 dev-fast/process-only noincremental, ordinary8;
one pipeline/stable target/linker/features/caches; stop5/hard4GiB commit/free50GiB disk,
helper56 under conditional96GiB ceiling. Preserve source/caches/evidence. No cleanup/
commit/push/live action/production rebuild/native probe/device/audio/notification/
OS input/security/signing/trust/registration.



**Latest verified desktop milestone: 5,196 passed / 0 failed / 7 existing ignored.**
Calendar Guests passed3 new-driver/48 Calendar/35 Modal/45 message/64 Homechannel/
29 HomeUser/46 Search/17 List cell/11 creation focuses, then full180 on identical
234973184-byte harness SHA256 `e1a5ebcec2f22944d149a0db6986d5cdeb9e4f1cf1425a39d91388b486b484a8`,
unchanged mtime. All prior5193 passes, original92 repairs and ignored7 retained.
Rebuild601.02s/min commit9.419GiB/owned peak
17.946GiB; full224.0s. Core5503/0/6+
1doctest carries exact2086 inputs from144, no unchanged Core rerun. Root5962 inputs/
binary and verified5-file source archived. Evidence:
`tmp/parity-search-recovery-20261027/calendar-guests-milestone.json`.

Calendar Guests now completes short/empty users/search marker pages to raw24 before
permissive User decode, dedupe or eligibility; every array retains the512 bound.
The actual shipped guest picker proves include_profile_only_users:true; this wire
flag grants no guest eligibility. Observed endpoint credential/TTL routing remains,
with exact Context pointer/dialog/guest-generation publication fences and separate
shortcut cooldown/endpoint admission before build and after readiness. Known expired
endpoints refuse before request construction. Already-admitted current responses
remain publishable despite later TTL/cooldown changes. Existing account-owned
retain_limit/default60/insertion behavior is exact, not a max-directory-budget port.

No partial publication/hydration or eligible refill. Current refusal preserves
installed choices, selected values and agent facts, skips metadata and clears only
its own live loading slot. Retirement does nothing. Ordinary failures retain cached
fallback/hydration; cache-first64/current-workspace precedence/eligibility remain.
Exact outer Context/query fences surround hydration; its original internal Facts
assignment remains query-fenced. No new full internal Context fencing or physical
cancellation claim. Search results are not globally upserted; Create validation,
sharing100, initial guests, freebusy/event/room/preference paths and old tests stay.

Three new grouped methods verify wire/marker/raw clipping/512vs513, profile-only and
duplicate raw-budget consumption, generation/Context/foreign-account retirement,
bounded37 deadline/key custody, TTL/cooldown admission predicates, current refusal,
cache/remote/workspace display precedence and exact selected values. Injected
readiness is not production Transport/native acceptance. Draft02/03 strengthen only
the new tests; four production files stay exact since draft01. Initial review
context truncated ReadScope at a test-module declaration; successors supplied full
helpers/eligibility/visibility. Two missing assertions were added before build.
No Rust gate failed in this batch.23 Calendar pins/15 old function bodies preserved.

Prior Conversation5193/Home5180/User5177/List5173/People5169/signed5165 carry.
Goal ACTIVE. Next separate review: Profile request/category discrepancies in
`profile-rich-completion-reference-audit-01.json`; existing intentional absence/
category guards/tests remain unchanged. No Profile edit or inferred broadening.
Native/live/layout/IME/Enter/full parity remain OPEN; session0 blocker unchanged,
no probes or workaround. MPIM/default/team/queue/ranking, optional integer buffers,
email-prefill order, introduced-number key/expansion and global People gaps remain.
Production executable and HEAD unchanged. Targets53.326GiB/free
121.898GiB after180. Desktop1 dev-fast/process-only noincremental, ordinary8;
one pipeline/stable target/linker/features/caches, stop5/hard4GiB commit/free50GiB disk,
helper56 under conditional96GiB ceiling. Preserve source/caches/evidence. No cleanup/
commit/push/live action/production rebuild/native probe/device/audio/notification/
OS input/security/signing/trust/registration.



Profile category applicability correction after verified180.

Further Profile category audit supersedes the preliminary expansion candidate for
reviewed OrdinaryClientV2/unified=true: exact source locality production and native
projection both force fromAnotherTeam=false. The framework default is not a live
mode observation. No useful category gap is established for that reviewed mode;
retain current Profile refusals/List behavior. Evidence:
`tmp/parity-search-recovery-20261027/profile-rich-other-team-observation-reference-01.json`
(SHA25642c78cffb564a0f59bcc2efb3857df43ac11cdef9c8586d67ee5391e18404280).
Next candidate is separately scoped final-SDK Profile long_text member wire true,
with legacy Query/transport/List/fixture contracts preserved; plan review running.


**Latest verified desktop milestone: 5,198 passed / 0 failed / 7 existing ignored.**
Profile long_text final-SDK member recipe passed1 new fixture/302 Profile sections/
330 List comments/46 Search focuses, then full191 on identical 235058176-byte
harness SHA256 `8ca8aaf73fccdbadda274791ca2ffb2fb05cfe3f426b9116cf764a3fa22fa298`, unchanged mtime.
All prior5196 passes, original92 repairs and ignored7 retained. Rebuild
657.25s/min commit9.408GiB/owned peak
17.704GiB; full237.29s. Root5969 inputs/
binary and verified13-file source archived. Evidence:
`tmp/parity-search-recovery-20261027/profile-long-text-member-wire-milestone.json`.

Changed Core freshly passed5506/0/6+1doctest in186 on identical focused binary,
preserving5503/ignored6.31 focused checks covered new factory/validator, original
Profile contracts and shared Activity reader.2090 inputs frozen/retained as
core-5506-source/baseline; old5503 carry ended with these source edits.

Actual sealed mounted Profile LongText lease selects one immutable final-SDK member
recipe for factory/executor/fixture. It adds ONLY include_profile_only_users:true,
directly proven at euu1 after WNgx intermediate arguments. Legacy Query factory,
transport grammar/tests and default/List callers retain absence/rejection. Dedicated
transport requires exact Bool(true), strips only that field from a validation clone,
then uses unchanged legacy grammar and admitted same-byte ordered decoder on the
ORIGINAL request. No refused SDK fallback or payload-supplied authority.

Transport validates structural grammar, not equality against a captured credential
owner. Factory/endpoint/currentness retain owner/expiry; closed fixtures compare
exact private auth/headers and correlation URL to a fresh SDK factory plan BEFORE
legacy-clone validation, then retain ORIGINAL redacted SDK args. Two named fixture
entries share unchanged handle/owner/start/wait lifecycle and terminal accounting.
Original source receipt, pre-page/after-await/endpoint/cancellation/publication,
can_dispatch/failure cooldown, raw25 Pages/canonical intake, eligibility/ranking/
matching/selection and category guards remain.14 old function bodies/9 pins and all
1420 existing test files exact. Core3/Root2 new methods encode distinct wire/grammar,
owner/TTL/auth layers, actual mounted Profile opt-in and redaction. They do not
establish end-to-end native acquisition/cancellation. Pin-selection bookkeeping7vs9
was corrected without source/test/config change; no Rust gate failed in this batch.

Profile channel25 audit finds check_membership:true already present and required.
include_record_channels default propagation still depends on qChh; no new omission
established. Category correction42c78cff controls reviewed OrdinaryClientV2/unified
true: source and native locality producers force false, so preliminary another-team
expansion is excluded. Framework default is not live mode evidence. Current wire
flag's server-visible effect is unobserved. Native/live/layout/IME/Enter/full parity
remain OPEN; session0 blocker unchanged, no probes/workaround.

Goal ACTIVE. Next source-grounded Calendar SINGLE sharing channel final-wire audit;
member/profile-only and broader100 continuation need separate eligibility/failure
review. MPIM/default/team/queue/ranking, optional integer buffers, email-prefill
order, introduced-number key/expansion and global People gaps remain open. Prior
Calendar5196/Conversation5193 and all evidence carry. Production/HEAD unchanged.
Targets53.327GiB/free119.438GiB after191. OrdinaryCore8/
DesktopTest1 dev-fast process-only noincremental; one pipeline/stable target/linker/
features/caches, stop5/hard4GiB commit/free50GiB disk/helper56 conditional96 ceiling.
Preserve source/cache/evidence; no cleanup/commit/push/live action/production/native
rebuild/probe/device/audio/notification/OS input/security/signing/trust/registration.



**Latest verified desktop milestone: 5,200 passed / 0 failed / 7 existing ignored.**
Calendar SINGLE channel SDK wire/raw100 completion passed2 new sharing tests,
50 Calendar/302 Profile sections/330 List comments/46 Search focuses, then full202
on identical 235134976-byte harness SHA256 `ac2f072f80e753005bd62c8595bc2403cbf5bb22d96daf2e57f99f766af3e6cb`, unchanged mtime.
All prior5198 passes, original92 repairs and ignored7 retained. Rebuild
690.97s/min commit9.37GiB/owned peak
17.82GiB; full240.31s. Root5973 inputs/
binary and verified6-file source archived. Evidence:
`tmp/parity-search-recovery-20261027/calendar-sharing-channel-milestone.json`.

Changed Core freshly passed5508/0/6+1doctest in196 on identical focused binary,
preserving5506/ignored6.12 focused checks covered raw100 collection, old selector,
admission and channel factory contracts.2092 inputs frozen/retained as
core-5508-source/baseline; historical5506 carry ended with these source edits.

Actual Calendar ShareEventInChannel SINGLE/rfHj source establishes channel
check_membership:true and provider-local raw100 completion. Only channel provider
changes. Each results array must be<=512 before accumulation; first100 raw rows
before permissive decode/dedupe/eligibility, no refill. Budget precedes unused
marker inspection; no overflow-marker retention/object/auxiliary/outcome claim.
Core helper collects rows; it does not authorize transport. Original endpoint or
discovery is captured before collection. Expired unfired pages refuse without
rediscovery/fallback. Factory and endpoint share fresh now; scope admission keeps
owner/query/cooldown/TTL after readiness, while publication excludes TTL/cooldown
for admitted responses. Original attempt observer preserves account/default60/
insertion policy before retirement. Refused work is not an attempt.

PostingIngress is captured before channel discovery. Terminal clipped/decode/current
success invokes it once including empty, preserving decoded duplicate order. No
partial/error/refused/retired ingress. Current None/error marks provider unavailable
and continues original partial-provider metadata/posting/warning lifecycle for
accepted siblings/cache/selected destinations. Retired outcomes exit quietly.
Exact outer Context/query fences follow every provider await, precede metadata,
follow metadata/posting, and guard final write. Those outer fences do not prove
internal metadata Ticket Context custody or physical cancellation; unchanged
posting Query itself includes Context pointer equality. Injected seam tests do not
establish actual Transport readiness or full parent dispatcher/native acceptance.

Original8 sharing function bodies/27 Calendar pins/1424 existing test files exact.
Membership request flag grants no posting authority: unobserved posting still blocks
membership-only rows. Native aggregate warning behavior remains; source ledger
8f7f9808 proves per-query reset without authority to clear unrelated warnings.
Members/MPIM, canonical eligibility/ranking/mode/default/queue behavior remain open.
Three read-only source reviews cleared their assigned boundaries; no Rust gate
failed in this batch. Previous Profile5198/Calendar5196/Conversation5193 carry.

Goal ACTIVE. Independent retained-static SINGLE member eligibility and MPIM
continuation audits are next; implement only a concrete supported journey. Native/
live/layout/IME/Enter/full parity remain OPEN. Session0 blocker unchanged, no probes
or workaround. Profile another-team/category correction42c78cff remains controlling
for reviewed OrdinaryClientV2/unified=true; no category widening. Optional integer
buffers, email-prefill order, introduced-number keys and global People gaps remain.
Production/HEAD unchanged. Targets53.348GiB/free117.957GiB
after202. OrdinaryCore8/DesktopTest1 dev-fast process-only noincremental; one
pipeline/stable target/linker/features/caches, stop5/hard4GiB commit/free50GiB disk/
helper56 conditional96 ceiling. Preserve source/cache/evidence; no cleanup/commit/
push/live action/production/native rebuild/probe/device/audio/notification/OS input/
security/signing/trust/registration.



**Latest verified desktop milestone: 5,202 passed / 0 failed / 7 existing ignored.**
Calendar SINGLE new-member discovery exclusion passed2 new publication/cached
tests,52 Calendar/302 Profile sections/330 List comments/46 Search focuses, then
full211 on identical 235187712-byte harness SHA256 `d1928f9bfdbf172e76208bef4371510136bfe81e00ad991a7aecc87262f3337b`, unchanged
mtime. All prior5200 passes, original92 repairs and ignored7 retained. Rebuild
660.78s/min commit9.364GiB/owned peak
17.799GiB; full239.56s. Root5975 inputs/
binary and verified5-file source archived. Evidence:
`tmp/parity-search-recovery-20261027/calendar-sharing-profile-only-milestone.json`.

Changed Core freshly passed5509/0/6+1doctest in205 on identical focused unit binary,
preserving5508/ignored6. New predicate focus1 and Calendar group10 passed (the new
test appears in both executions).2093 inputs retained as core-5509-source/baseline;
historical5508 carry ended with these source edits.

Exact recorded-digest public vendor assets recovered qChh and its complete56
literal-import helper closure. Inert AST reasoning establishes actual rfHj SINGLE
deep defaults: explicit includeSelf:true/matchEmailField:true survive, inherited
includeProfileOnlyUsers:false applies. Source excludes profile-only NEW member
discoveries. Ledger8acbc572 supersedes earlier bounded missing-qChh blocker for
this actual recipe; earlier negative searches retain their original boundaries.

Core contributes only the profile-field discovery predicate: missing/Bool(false)
passes; Bool(true) and every explicit nonboolean fail closed through preserved
User.extra. Malformed null/zero/empty string refusal is declared conservative native
policy, distinct from JavaScript falsey acceptance. No complete eligibility,
canonical receipt, normalization or membership authority claim.

Cached suggestions retain deletion/identifier checks and selected-ID exemption.
Remote discovery requires original available FIRST, then selected-ID exemption or
passing BOTH raw and current same-ID cached profile observations. Channels keep
their original policy. A separately eligible cached identity may remain when its
flagged remote row is excluded. Original available/resolve/Create/requests and
independent selected-option retention stay exact. Native selected-across-query
compatibility remains distinct from source empty-query prepend; no new selection
veto, wire/raw100/category/canonical expansion or posting authority is introduced.

Final serial publication moved into production publish with the same original
owned_current guard under write and selected-option re-add. Two Root regressions
inspect resulting choices/remote destinations: raw/cache disagreements, original
availability before selected exception, selected/draft/notice/cache preservation,
and remote-only retained selection absent from cache/new results under a nonempty
nonmatching query. They exercise the actual publication helper, not the full async
dispatcher/native UI. Draft02 only strengthens that new test; other4 files exact.
Core file remains exact old prefix;10 sharing functions/26 previous Calendar pins
and2088 pre-existing Rust files containing test attributes stay exact. The prior
27th pin load.rs is the declared changed target. Atomic patch-context rejection and
overbroad27-pin preparation assertion were corrected before gates; evidence kept.
No Rust gate failed. Source review02 closed the one retention-coverage gap.

Goal ACTIVE. Next safe source audit: newly introduced required canonical-integer
INPUT under a never-used top-level key, now that exact y4OD expansion/JSX helpers
are recovered (ledger99d67325). All-key lifetime ownership/admission still needs
review. Preserve same-key/recreated/saved/raw_state/hidden/missed-update/optional
buffer exclusions and original initial-only APIs/tests. Member wire/raw100 still
needs owned canonical/interaction/order/failure review; MPIM normalization/visibility,
global People, ranking/default/queue gaps remain separate. Prior channel5200,
Profile5198/Guests5196/Conversation5193 and all evidence carry.

Native/live/layout/IME/Enter/full parity remain OPEN. Session0 blocker unchanged,
no probes/workaround. Production/HEAD unchanged. Targets53.301GiB/free
116.565GiB after211. OrdinaryCore8/DesktopTest1 dev-fast process-only
noincremental; one pipeline/stable target/linker/features/caches, stop5/hard4GiB
commit/free50GiB disk/helper56 conditional96 ceiling. Preserve source/cache/evidence;
no cleanup/commit/push/live action/production/native rebuild/probe/device/audio/
notification/OS input/security/signing/trust/registration.



**Latest verified desktop milestone: 5,204 passed / 0 failed / 7 existing ignored.**

Required canonical integer under a never-used top-level INPUT key passed2 new
controller journeys,87 App modal/288 Home inputs/102 message inputs/46 Search
focuses, then full221 on identical 235182080-byte harness SHA256
`2c7af659810e09a3a1f951c933e631b406ad7d04cb4ec7092b5834fa161cb402` and unchanged mtime. All prior5202 passes,
original92 repairs and ignored7 retained. Rebuild670.33s/min commit
9.431GiB/owned peak17.807GiB;
full239.34s. Root5979/source9 and binary archived. Evidence:
`tmp/parity-search-recovery-20261027/modal-new-top-key-milestone.json`.

Changed Core freshly passed5513/0/6+1doctest in215 on identical focused unit binary,
preserving5509/ignored6. New4/Modal26/Form323 focuses passed.2096 inputs retained
as core-5513-source/baseline; historical5509 carry ended with these edits.

Scope: actual action-correlated full RT root, single root, strict text/number-only
all-INPUT view, required canonical signed integer in prior bounded domain under
never-used top-level block ID. Exact y4OD expansion/key helpers static proof is
retained; fresh React key mounting remains explicit inference, not runtime/native
or Slack React-version binding. Optional/decimal/bounds/saved/metadata/same-key/
recreated/hidden/missed/HTTP expansion remains excluded. Original initial-only
parse APIs/fixtures retain their scope; no direct-root fixture arms the new mode.

Owner consumes all raw IDs before parse into bounded4096 history; private linear
one-time root custody is not cloned. Read-only clones keep data/expected Weak
atomic epoch, cannot refresh or consume actual custody. Epoch0 is absorbing,
checked increment cannot wrap and Drop synchronously revokes. Immutable whole
introduced INPUT profiles remain distinct from initial integer provenance.
Retired numeric drafts freeze before destructive reconciliation without restoring
absent fields. Same-key replacement text keeps its original dirty-conflict guard.
A tombstone prevents submission revival after field removal/conflict reset.

Serial accepted dispatch consumes only extra authority, leaves original revision/
Session generation/reset/Ticket unchanged, and never rearms through response.
Pending requests avoid premature failure; definite outcomes retain cause and
explain close/reopen; uncertain delivery retains original no-repeat guidance.
Initial-only forms and legacy children retain policy, including actual child RT
update. Tests use nonexecuting transport seam, model acknowledgements and full
account/token/transport/RT context retirement; they are not native/UI acceptance.

Source-review Draft01/02 defects were corrected before Cargo. Final followup03
cleared quoted corrections/integration. Earlier desktop E0603 finding was an
excerpt bug dropping pub(super), withdrawn against exact declaration; no privacy
source change or observed compiler failure. A60728char bounded final prompt
replaced oversized preparation before invocation; all evidence retained. All20
protected function bodies and2090 existing Rust test-attribute files stay exact.

Goal ACTIVE. Next safe source work: ordinary modal read-only content alongside
fresh integer INPUT, and Calendar SINGLE member discovery wire/intake review.
Member raw100/canonical/interaction/order/failure, MPIM normalization/visibility,
global People/ranking/default/queue and optional/native buffers remain separate.
Prior Calendar5202/channel5200/Profile5198/Guests5196/Conversation5193 carry.

Native/live/layout/IME/badInput/Enter/full parity remain OPEN. Session0 unchanged,
no probes/workaround. Production/HEAD unchanged. Targets53.381GiB/free
113.255GiB after221. Core8/DesktopTest1 dev-fast process-only noincremental;
one pipeline/stable target/linker/features/caches stop5/hard4GiB commit/free50GiB
disk/helper56 conditional96 ceiling. Preserve source/cache/evidence; no cleanup/
commit/push/live action/production/native rebuild/probe/device/audio/notification/
OS input/security/signing/trust/registration.



**Latest verified desktop milestone: 5,205 passed / 0 failed / 7 existing ignored.**

Mixed-content required-integer admission passed1 new controller journey,
88 App modal/288 Home inputs/102 message inputs/46 Search focuses, then full231
on identical 235179008-byte harness SHA256 `47ea4fc726837fcf3b224ce7c3f5854618b8c7483285e6b946c9814ca07ba092`
and unchanged mtime. Every prior5204 pass, original92 and ignored7 retained.
Rebuild679.21s/min commit10.077GiB/owned
peak17.8GiB; full244.29s.
Root5982/source5 and binary archived. Evidence:
`tmp/parity-search-recovery-20261027/modal-mixed-readonly-milestone.json`.

Fresh Core5515/0/6+1doctest passed in225b on identical focused binary after
new2/Modal28/Form323. All prior5513 passes/ignored6 preserved;2098 inputs and
binary retained. Original225 was interrupted during reported model capacity:
no final summary/exit record, not a pass or observed test failure. Resume audit
verified no Cargo process and exact2098Core/5982Root inputs/focused binary before
repeating only that incomplete full gate. Retain225 log and225b result separately.

Concrete product: an actual correlated full-RT modal may introduce a required
canonical integer under a never-used explicit INPUT ID beside strict read-only
header/section/context/divider instructions. Original INPUT branch, parser,
renderer/styles/scalar handlers and ownership/currentness remain unchanged.
Every block needs explicit nonblank unique ID; display and unknown IDs enter
history before classification, including after earlier shape failure. Display-ID
reuse remains unsupported/revoked/non-submittable; the unchanged parser can still
retain that visible unsupported field/literal. No absent-row claim or rearm.

Closed composition is plain_text with emoji absent ortrue. False/null/nonboolean,
mrkdwn/accessories/fields/images/actions/conditions/unknown metadata are excluded.
3000 UTF8 bytes/text and1..10 context entries are local defensive budgets, not
Slack schema/codepoint limits. Empty text remains allowed. Exact retained static
expansion/key/default/prune proof supports the bounded shape; generic fresh-key
mounting remains explicit inference, not native/runtime/React-version binding.

Two Core methods cover mixed RT/edit/required-clear/exact input-only wire,
reused display key refusal and strict shapes/UTF8/entry boundaries. One Root
method covers real token/root/update/controller and existing typed display nodes,
then nonexecuting serial submission acceptance. Structural projection is not
native rendering/layout acceptance. All2092 prior Rust test files,23 owner
functions outside classification and original INPUT branch exact; native module
only adds test wiring. Draft01 source review caught new-test shell Unicode and
wrong removal-oracle issues, corrected only in the new Core test before gates.
Other4files stayed exact and followup02 cleared source corrections.

Goal work resumed under renewed Amp supervision. Next highest-value candidate:
Calendar SINGLE member raw100 completion with actual owned intake and new-choice
interaction facts. Source4c585cd3 supports candidate; receipt/cache/selection
integration must be resolved in a concrete reviewed plan before implementation.
Retained selection/resolve/Create, sibling providers and original guards stay.
Global People/ranking/default/queue, MPIM normalization and native buffers remain
separate unresolved journeys. Full parity remains unachieved.

Native/live/layout/IME/badInput/Enter remain OPEN, session0 no probes/workarounds.
Production/HEAD unchanged. Targets53.407GiB/free111.84GiB.
Core8/DesktopTest1 dev-fast process-only noincremental, one pipeline/stable caches,
target/linker/features stop5/hard4GiBcommit/free50GiB disk/helper56 conditional96.
No cleanup/commit/push/live action/production/native rebuild/probe/device/audio/
notification/OS input/security/signing/trust/registration. Mixed-encoding worklog
is preserved. Goal API still reported blocked at resume and exposes no active
reset; explicit user resume authorizes continuing work, documented in resume audit.


## Pending Calendar SINGLE member raw100 applied batch

Pending Calendar SINGLE member raw100 batch: 13 source files are formatted/frozen
as draft03 after split Amp acquisition/admission/offline review and closure03.
Actual later-page acquisition now uses owned canonical intake, bounded dialog
origin witnesses and route-bearing serial selection. Two new-test atomicity gaps
were corrected before compilation; all2094 old test files/58 protected bodies
remain exact. Core232-235 and desktop236-241 are planned, NOT results. Current
changed source requires both gates; historical5205/5515+doc is preserved only.
Native/live/JS/Unicode/layout/full-parity acceptance remains open. One pipeline,
Core8/DesktopTest1 dev-fast processincrementaloff, stop5/hard4/free50/helper56
conditional96; no production/native rebuild/probe or restricted live action.
Evidence: `calendar-single-member100-draft-03.json`,
`calendar-single-member100-source-review-03-assessment.json` in the current
evidence directory. No test result or native acceptance is inferred from source review.


**Latest verified desktop milestone: 5,209 passed / 0 failed / 7 existing ignored.**

Calendar SINGLE member raw100 completion passed4 new actual-reader/selection
journeys,56 Calendar/337 Profile/330 List comments/46 Search/57 directory-member/
6 sharing-peer focuses, then full241 on identical 235877376-byte harness
SHA256 `34bf06e172a18129836e40f89ca931b461526c77b73916b5e4be4de47128f220` and unchanged mtime.
Every prior5205 pass, original92 and ignored7 retained. Rebuild
595.22s/min commit9.264GiB/owned peak
18.252GiB; full242.85s.
Root5989/source13 and binary archived. Evidence:
`tmp/parity-search-recovery-20261027/calendar-single-member100-milestone.json`.

Fresh Core5517/0/6+1doctest passed235 on the identical focused binary after
new2/continuation3/Calendar10. All prior5515 passes/ignored6 preserved;2100 inputs
and binary retained. The earlier mixed-modal225 interrupted log remains INCOMPLETE;
its completed225b5515+doc and desktop2315205 are separate preserved milestones.
No expected count or missing summary was counted as a result.

Concrete product: Calendar sharing can acquire a later-page member within the
first100 raw entries, pass original canonical intake/flags/source/renderer and
publish/select its current native value. Count100 and final SDKflagtrue remain on
every page, with raw budget before item admission/filter/dedup and unused marker
parsing. Unsupported object/auxiliary nonempty envelopes refuse. No refill or
permission/category expansion. Endpoint/TTL/cooldown/account429 and every outer
owner/query guard remain. Failure/retirement keeps original cache/selection/sibling
provider and warning behavior; no hidden DM open or Slack write.

A bounded first-only dialog origin registry prevents provider-installed native
cache from bypassing canonical interaction proof. Independent original cached
eligibility/label facts remain available. Predicate precedes original cached cap;
actual Snapshot currentness is checked once per serial operation with an ephemeral
index. Fresh query may certify actual LOCAL cache facts, never remote hit
membership. All acquired IDs need guarded/canonical/native/raw-profile entries
before any append publication. Serial selection chooses current canonical native
User or independent cache route, clearing stale remote object for the latter.
Retained selection/resolve/Create and channel/posting/metadata rules remain separate.

Registry local limits4096IDs/255byteIDs/4096byte witness names are defensive policy,
not Slack limits or permanent global cache provenance; no eviction/query reset.
Budget/invalid ID refuses the entire provider before intake. Two Core/four Root
methods cover real page hold/intake/selection, late failures/429/missingflags,
renderer and postadoption retirement, pre-cap filtering and discriminating atomic
refusal after a valid prefix. All2094 old Rust test files/58 protected bodies exact.
Source review removed a draft test-only legacy publication shim before compilation;
all old/new tests use production publish. Two new atomicity-test gaps were fixed
before gates; prior drafts and reviews retained. Compile236 failed before tests with3wrapper/fixture type/privacy errors. Corrected
in3Rootfiles with unchanged Core/policy/oldtests/assertions;236b passed after
explicit Arc coercion, owning-module cfg(test) retirement helper and actual
optional-profile mutation. Failed236 log/draft retained.
Focus236b then compiled1pass/3fail: two URLoracles and short-ID hydration
failed. Source audit also found a normalization-fixture mismatch before those
later assertions executed. All affected original cases passed
unchangedbinary diagnostics. Corrected only2newfixture files: exactpath URLoracle,
valid IDs, explicitoff negative and actualon canonicalpositive case. Production/
Core/oldtests/assertions unchanged. Failed236b binary/log/draft preserved;236c
then passed all4. No expected count was used as a result.

Test-only Dioxus evaluator models exact original ASCII lowering through genuine
Eval/join and real fixture initialization/drain. It does not establish native Wry,
general JS/Unicode/layout equivalence. Native/live/layout/IME/badInput/Enter/full
parity remain OPEN. Session0 no probes/workarounds; production/HEAD unchanged.
Goal ACTIVE by explicit renewed supervision; API still has stale blocked status
and no supported active reset. Next implement the reviewed bounded renamed-open-group MPIM journey,
including guarded canonical refresh (proposals01+02+03/Ampplans01+02). Global People preference/response,
ranking/default/queue and native-buffer gaps remain separate. Worklog untouched.
Targets53.366GiB/free107.939GiB. Core8/DesktopTest1
dev-fast process-only incrementaloff, one pipeline/stable target/linker/features/
caches, stop5/hard4GiBcommit/free50GiBdisk/helper56 conditional96. No cleanup,
commits/pushes/live action/production rebuild/native probe/device/audio/notification/
OS input/security/signing/trust/registration.



**Latest verified desktop milestone: 5,212 passed / 0 failed / 7 existing ignored.**

Renamed/open group-DM acquisition passed3 new reader/selection journeys, then
59 Calendar/337 Profile/330 List comments/46 Search/57 member-directory/6 sharing-
peer/82 channel-fetch focuses and full251 on identical 236745728-byte harness
SHA256 `3afc20ae03f1dcffbf0fae1275ff79775e498b2276d0c74ba25fb53801d368ea` and unchanged mtime. Every prior5209 pass, original92
and ignored7 retained. Actual terminal summary/exit0 verified before updater.
Rebuild602.36s/min commit9.139GiB/owned
peak17.986GiB; full241.58s.
Root5999 inputs/source20/binary archived. Evidence:
`tmp/parity-search-recovery-20261027/calendar-renamed-open-mpim-milestone.json`.

Fresh Core5520/0/6+1doctest passed245 after3new/3continuation/10Calendar/16projection
focuses on the same binary; prior5517/ignored6 preserved.2104 inputs/core5520
binary retained, exact through later Root-only fixture corrections. Core225 still
has no completion; actual225b/231 and member235/241 milestones remain preserved.

Concrete product: a current known literal-open MPIM can adopt a later-page custom
name through actual shared intake/renderer/drain, obtain independent current
posting permission and publish/select its current native value. A new query can
canonically refresh the guarded ID even when firstpage is legacy-decodable and
updated unchanged. First-only cache origins are never recaptured. Failed/retired
acquisition preserves genuine independent legacy firstpage; guarded IDs cannot
regain raw fallback. One actual first request, first100 raw results-then-mpims,
30 total attempts, original marker/TTL/cooldown/account429 and every outer guard.
Objects/nonempty huddles/unsupported shapes refuse. No permission/category expansion.

Original consuming Read guards acceptance; returned Canonical guards publication
after own installation. Shared accepted cache work keeps its lifetime. All IDs
register before intake and preflight before atomic append. Current native
re-selection and pre-cap filtering prevent stale raw/cache bypass. Original
independent cache labels/base contributions and retained selection remain separate.
Defensive registry4096/ID255/witness-name4096 and shape bounds are native policy,
not Slack limits or full closed/star/unread/latest/unknown visibility/ranking.

Three coherent tests cover actual two renames/selection, raw100/unused suffix,
mandatory posting, legacy latefailure/retired429, no partial intake, original and
permanentNone origins, pre-cap currentness and atomic refusal. All2096 existing
Rust test files exact; no assertions/guards weakened.246compile101 beforetests
was one newfixture signal-handle borrow;246bcompiled0pass/3newsetupfail, with all
oldaffectedgroups passing samebinary. Failedsource/log/binary retained. Source
correctly rejected stale startup auth. DefaultNoOp fixture composition establishes
final synthetic auth/disconnectedtransport, actualstartup and acceptedconnection
BEFORE owner capture; actualInit/Channels/Extras establish lifecycle readiness.
No owner rebind/fakeReady. Bothoriginaldefaultfixture bodies preserved.246c passes3.

Native/live/IME/generalJS/Unicode/layout/fullparity OPEN. Evaluator models ASCII
lowering through genuine Eval/join, not Wry acceptance. No production rebuild or
Session0 probe. HEAD/production unchanged. Goal ACTIVE by renewed24h supervision;
API still stale blocked/noactive reset. Worklog untouched. Targets
53.403GiB/free103.278GiB afterfull251. Core8/
DesktopTest1 dev-fast processincOff; onepipeline/stablecaches/profile/target/linker/
features, stop5/hard4 commit/free50 disk/helper56 conditional96. No cleanup/commit/
push/livewrite/call/upload/reaction/notification/device/audio/OSinput/security/
trust/signing/registration. Full parity remains a goal, not a fixture claim.

Next safe slice: participant-name discovery when an open custom group's label
misses query, using actual member Snapshot.source/existing filter_mpims and
independent posting. Source/plan01+02 reviewed; aggregate rejection, publication
retirement and queued-selection retirement need independent phases. No next result.
Strict source-equivalent global People transport has a restricted fp/audio/device/
cookie producer acceptance gap. Functional native metadata reuse remains a separate
qualified protocol-policy option; it cannot claim source-equivalent metadata.
Actual prefs/promoted history/Primary context/auth mode/session ordering are pinned.


Current checklist additions:

| Journey | Verified offline behavior | Remaining acceptance |
| --- | --- | --- |
| Rename an open group DM in Calendar sharing | Actual raw100 acquisition/shared intake/same-updated guarded refresh/independent posting/current serial selection; Core5520+doc and Desktop5212 full. | Native/live/layout and broader matching/visibility/ranking; participant-name slice next, not implemented. |
| Global People numbered pages | Source state/context/auth/FormData/metadata producers pinned; bounded lookup preserved. | Producer/controller implementation and service compatibility; strict fp collection remains restricted. No metadata/permission substitution claim. |


**Latest verified desktop milestone: 5,214 passed / 0 failed / 7 existing ignored.**

Calendar sharing can find an open custom-named group DM by a participant's name
when its displayed label misses the query. Actual current member source, complete
canonical roster and the existing source-derived matcher establish this additional
posting-acquisition candidate. Independent actual posting authority remains
mandatory. Current channel and retained member proofs guard publication and queued
selection; neither a server hit nor raw metadata alone supplies permission.

New focus252 passed2/0; Calendar253 passed61, Profile254337, List255330,
Search25646, member256b57, sharing-peer256c6 and channel-fetch256d82.
Full257 ACTUAL5214/0/7 on identical 236945408-byte harness
SHA256 `63357f2565aa1b038d59c4c10613ea3da4017e1436605b4be85d420c0c0c1067`, bytes/mtime unchanged. Every prior5212 pass,
original92 and ignored7 retained. Actual terminal summary and exit0 inspected
before updater. Rebuild601.81s/min commit8.974GiB/
owned peak17.863GiB; full228.26s.
Root6002 inputs/8changed source files/binary archived. Evidence:
`tmp/parity-search-recovery-20261027/calendar-mpim-participant-milestone.json`.

Core5520/0/6+1doctest245 carries on all2104 exact input hashes; no Core change
or repeat full core gate. Existing2099 Rust test-attribute files are exact.
Earlier mixed-content modal, member100 and renamed-MPIM milestones and failures
remain retained; Core225 still has no final completion, never inferred.

Supported participant positive is one lowercase ASCII token<=64bytes and complete
current canonical groups<=64members with actual source fields<=4096bytes. Bounds
are defensive native policy, not Slack backend limits. No native User-to-source
reconstruction, remote fuzzy passthrough, fabricated missing-member absence,
permission/category expansion, recaptured channel receipt or source-ready default.
Matching sums ALL finite peer scores: exact0+four1 refuses4; identical three1
countercheck accepts3. Actual source self excluded. Label/legacy/cache/retained
selection paths remain independent. Two coherent tests also cover real posting,
current metadata without query witness refusal, accepted-match publication
retirement and separate authorized queued-choice retirement preserving draft/cache.
Test-only suspension is BEFORE real authority/channel acquisition, not an in-flight
HTTP response race. Actual subsequent permission remains current while member proof
retires; publication/selection refuse. Native/live/Unicode/general-query/ranking/
layout/IME/full parity remain OPEN; no Session0 probe or production rebuild.

Next active safe product slice: numbered People page1 -> page2 -> cached page1,
Amp plan01 approved offline after this closure. Inspect account/destination,
parameter-producer/rate-observer and independent profile seams before implementation.
Use actual preference/assignment admission; unknown facts keep bounded30 lookup.
Closed text-only multipart/partial Hit/Page/controller must preserve order,
duplicate row identity, pagination, query/account/page lifetime and independent
profile hydration. Existing native opaque request metadata/routing are explicitly
qualified protocol policy, NOT source JavaScript fingerprint/session/enterprise
observations or live compatibility. No device/audio/cookie probes authorized.

Goal ACTIVE by renewed24h user supervision; API remains stale blocked/noactive
reset supported. HEAD/production unchanged, worklog untouched. Targets
53.404GiB/free101.286GiB afterfull257. DesktopTest1 dev-fast
processincOff/Core8/onepipeline/stablecaches; stop5/hard4 commit/free50 disk/helper56
conditional96. Preserve allsource/cache/evidence. No cleanup/commit/push/livewrite/
call/upload/reaction/notification/device/audio/OSinput/security/trust/signing/
registration. Full parity is unachieved.


Current checklist additions:

| Journey | Verified offline behavior | Remaining acceptance |
| --- | --- | --- |
| Find a custom-named group DM by participant in Calendar sharing | Genuine complete member source match, independent actual posting, current canonical selection; new2 and full5214/0/7, allprior preserved. | Native/live/layout, general query/Unicode/ranking/visibility; no HTTP-response-race claim. |
| Global People numbered pages | Source/controller/closed-multipart and qualified native protocol-policy plan approved by Amp; bounded lookup preserved. | Implementation, real offline gates and native/live acceptance pending. No source-equivalent metadata or enterprise-mode claim. |


## Verified numbered People pages and independent profile journey

**Latest verified desktop milestone: 5,217 passed / 0 failed / 7 existing ignored.**

People search now supports numbered pages, cached return to an earlier page,
exact failed-page retry, detached card metadata and an independently owned
read-only profile. Genuine current preference and assignment receipts admit the
new path; unknown or unsupported inputs retain the bounded 30-row lookup.
Results preserve returned order, duplicate IDs, row identity and raw carriers.
Partial hits never become canonical Users or workspace membership. The original
authenticated account/client/token/cookie/transport/destination remain confined.
Native opaque metadata/routing are qualified protocol policy, not JavaScript
fingerprint/enterprise-mode observations or real Slack compatibility.

Actual focus262c passed3/0; Search26349, Profile-read2644, channel-fetch26582,
Calendar26661, Profile267337, List268330, Canvas269421, sharing-peer269b6,
reconnect269c6, Search-view269d3 and directory-member269e57 passed.
Full270 ACTUAL5217/0/7 on the same 237671936-byte harness,
SHA256 `b88d7df43bbd74fe0bede50496eca2ee157680f77b22129d32b13300f3068bb8`, bytes/mtime unchanged.
Every prior5214 pass, original92 repair and existing7ignored status is retained.
The actual terminal summary and exit0 were inspected before this update.
Rebuild603.07s, minimum commit8.572GiB,
owned peak17.914GiB; full244.86s.
Root6015 inputs/34changed source or script files/binary are archived in
`tmp/parity-search-recovery-20261027/people-numbered-milestone.json`.

Fresh independently locked Core5528/0/6 plus1doctest passed full261 on its
unchanged focused258b harness, preserving all earlier passing/ignored statuses.
All2111 Core input hashes still match. Actual Core focuses: new8, oldlookup6,
Search119. Cookie-oracle2587/1 and runner260 count-selector error remain retained;
neither is erased. Desktop262 compile101 ran zero tests: stored Rc cooldown
violated existing Send/Sync requirements. Arc/Mutex correction preserves old
Send assertions, brief lock scope, monotonic cooling and fail-closed poisoning.
Failed source/logs are retained; no failed262 binary was produced.
262b compiled but all3 new tests failed at shared fixture setup. Its exact source,
log and failed binary remain. Fixture262c composes final synthetic URL/client mode
before the actual boot and connection, preserving original source ownership.
Stage diagnostics add no readiness or permission; production reader unchanged.

Amp A04/B05/UI02 source clearances remain scoped. Narrow cooldown correction
reviews06 and one bounded06b retry both failed ECONNRESET with no review result;
root reviewed the exact two-file storage correction and ran the authorized
offline gates. No Amp success is fabricated. No further correction-review retry.

Root scripted executor tests verify real admission, factory construction, page,
cache, retry, publication and independent profile transitions. Actual multipart
HTTP fields/boundary/cookie/redirect/retired429 behavior is separately covered by
Core loopback tests. The held metadata phase proves navigation retirement; a
separate held-page phase proves equal-preference retirement and account cooling.
These are not live network or held-metadata-across-preference proofs. Two named
normal/narrow paging fixtures are registered as presentation-only, not rendered.
Native Session0/layout/focus/IME/live backend/general Unicode/full parity remain
OPEN. No production rebuild or native Session0 probe occurred. Historical
Core225 still lacks a final summary and remains incomplete, never inferred.

Next active meaningful product slice: People own relevant/name/name_desc server
sort controls. Static source shows genuine falsy stick_user_sort permits local
module sort plus page1 fetch without preference writes; truthy sticky invokes
actual users.prefs.set and must not be redirected through the local branch.
Exact source ranges are independently verified. Amp sort-plan01 cleared offline
implementation after actual full270 closure; that condition is now met. No next
implementation, result or source-ready default is claimed.

Goal ACTIVE by renewed24h user supervision; persistent API remains stale blocked
without a supported active reset. HEAD/production unchanged, worklog untouched.
Targets53.478GiB/free96.965GiB afterfull270.
DesktopTest1 dev-fast process-scoped incremental off; ordinary/Core8; single
pipeline and stable caches. Stop5/hard4GiB commit, free50GiB disk, helper56GiB
sufficient with conditional96GiB ceiling. Preserve all source/cache/evidence;
no cleanup/commits/pushes/live writes/calls/uploads/reactions/notifications/device/
audio/OS input/security/trust/signing/registration. Full parity is unachieved.


Current checklist additions:

| Journey | Verified offline behavior | Remaining acceptance |
| --- | --- | --- |
| Navigate numbered People results and retry an exact failed page | Actual accepted parameter/account admission, closed multipart Core5528+doc, real Root reader/controller and independent profile, cached page1 return, retained failed-page data; new3 and full5217/0/7, all prior preserved. | Native/live/backend/full response coverage/layout/focus/IME; metadata/routing remain qualified native policy. |
| Change People alphabetical server sort | Exact source falsy-sticky local branch and truthy preference-write boundary pinned; Core already verifies all3 wire variants. | Next Root control/transition implementation and focused/full gates; no permission to save a live preference. |


## Verified People local server-sort journey

**Latest verified desktop milestone: 5,218 passed / 0 failed / 7 existing ignored.**

People results now have Most relevant (default), A to Z and Z to A server-sort
controls for genuinely accepted falsy stick_user_sort. Each choice starts page1
with its own cache; page2 and cached return stay in that generation. Returned
order and duplicate IDs are preserved. Original account/parameter/control/row
ownership retires old requests and queued selections, while account cooldown
and normal session getters survive. Source preferences, membership and draft
remain unchanged. Actual sticky-true saved sort remains read-only in this slice;
Edit default, preference persistence and coachmark behavior remain unfinished.

New focus271 ACTUAL1/0; Search27250, view2733, profile-read2744,
channel-fetch27582 and reconnect2766 passed on the identical harness.
Full277 ACTUAL5218/0/7, same 237746688-byte executable,
SHA256 `b64de4cdea2d032c988eda287477c44e53ca13b6dbd17a60280f1eff78f83661`, bytes/mtime unchanged.
Every prior5217 pass, original92 repair and existing7ignored status is retained.
Actual terminal summary and exit0 inspected before updater. Rebuild
609.32s/min commit9.102GiB/
owned peak17.904GiB; full228.53s.
Root6015 inputs/7changed files/binary archived in
`tmp/parity-search-recovery-20261027/people-sort-milestone.json`.
All2104 prior Rust test files are protected; the entire earlier three-test file
is an exact prefix. Amp plan01/factory02/applied03 are scoped source clearances.

Core5528/0/6 plus1doctest full261 carries on all2111 exact inputs; no repeat.
Desktop exercises the actual control, effective Frame::input projection and real
closed factory. The unchanged Core assertions verify private form mappings
relevant/desc, name/asc and name/desc. This is composed evidence, not a Desktop
HTTP emission capture. No closed request API or generic transport permission is
widened. Native opaque metadata/routing remain qualified protocol policy, with
no JavaScript fingerprint/enterprise-mode or live compatibility claim.

The earlier numbered-page/profile5217, participant5214 and mixed-modal milestones
and all failed artifacts remain archived. Historical Core225 still lacks a final
summary and is incomplete, never inferred. Immediate old-sort bag retirement is
explicit stronger native policy. Raw whitespace edits retain the loaded PageArc,
but the existing dialog guard retires its display; visible preservation is an
open product gap. Existing named normal/narrow presentation fixtures include a
disabled sort; no new PNG/layout/focus/IME/native acceptance. Session0 remains
blocked; no probe or production rebuild. Live/backend/full parity remain OPEN.

Next source-grounded journey: saved People sorting when sticky is true.
Original optimistic setUserPref is a real local producer, distinct from server
observations. users.prefs.set uses raw enum strings; source persisted/offline
fulfillment can precede transmission, so its synthetic pref_change is not a
server acknowledgement. Exact state35/wire43 ranges and dependencies are root
verified in people-sticky-sort-parent-verification-20261029-01.json. A closed
native writer/echo/currentness design requires Amp review before implementation;
no live write is authorized or executed. No next implementation/result claimed.

Goal ACTIVE by renewed24h user supervision; API stays stale blocked without an
active reset. HEAD/production unchanged, worklog untouched. Targets
53.477GiB/free95.148GiB afterfull277.
DesktopTest1 dev-fast process-scoped incremental off; ordinary/Core8; one Cargo
pipeline/stable caches. Stop5/hard4GiB commit, free50GiB disk, helper56GiB
sufficient with conditional96GiB ceiling. Preserve all source/cache/evidence;
no cleanup/commits/pushes/live writes/calls/uploads/reactions/notifications/device/
audio/OS input/security/trust/signing/registration. Full parity is unachieved.


Current checklist additions:

| Journey | Verified offline behavior | Remaining acceptance |
| --- | --- | --- |
| Sort numbered People and navigate the new sorted bag | Genuine falsy-sticky source, real production projection/factory, page1 reset/page2/cached1, asymmetric returned order/duplicates, retired older completion, unchanged prefs/membership; new1 and full5218/0/7. | Sticky saved-sort write/Edit default/coachmarks, native/live/layout/focus/IME; metadata/routing qualified native policy. |
| Change saved People sort when sticky is true | Original local producer plus exact setter/wire/echo/offline contracts pinned and parent-verified. | Bounded native implementation/guards/loopback/runtime gates; no live write executed. |

## Saved People sort: actual Core5530, desktop focus282 pending

Core focus278 ACTUAL2/0 (including real30s deadline), People2798/0, List-pref2802/0; full281 ACTUAL5530/0/6 plus1doctest. Same focused binary SHA72f423b3a248cec595d5fd57321a0c5f90f1a8d1917d98349281b9113ad60b4f /205606400bytes/mtime exact, all prior5528passes and6ignored preserved;2,116 frozen inputs. Rebuild171.56s/mincommit17.657GiB/peakowned9.209GiB, full688.27s/min23.154GiB. Targets53.482GiB/free94.285GiB. Actual summaries/exit inspected before closure and archive. Earlier Core225 remains incomplete.

Root draft01 contains four new regressions;6,023 inputs frozen,2,104 old test files protected and entire old four-test file exact prefix. Amp04 source found no concrete blocker and held desktop until actualCore281; hold now satisfied, focus282 compiling. No desktop pass claim. runner01 preflight failed before Cargo due accidental baseline filename prefix replacement; runner02 changes that path only. No product/config change. No live Slack write/native Session0 probe/production rebuild/restricted action. Source persistence/retry, native/layout/IME/backend/fullparity remain open.

### Desktop282 actual compile failure, narrow Root correction02

282 exited101 before any test/new harness. Two error classes: failure type imported at slack root instead of public slack::transport; five new assert_eq uses require intentionally absent Sort Debug (11compiler diagnostics total). Only saved_sort.rs/newtests/saved.rs change: exact import path and SAME equality expectations as bool assertions. No Core/public API/production behavior/config/oldtest/currentness change. Draft01/log retained, no old harness falsely archived as new failed binary. Draft02 frozen6023inputs; Core5530 exact2116 carry. fmt--all--check exit0. Build307.2s/mincommit13.651GiB/peakowned13.601GiB; targets53.325/free94.223GiB after command. Amp05 review pending; focus282b then affected283-287/full288 remain required, no desktop pass claim.

## Verified saved People-sort journey: Desktop5222

Saved People sorting now supports the genuine sticky-true branch: a current
Most relevant / A to Z / Z to A choice is an optimistic local producer, starts
a fresh page1 read immediately and makes one closed bounded preference attempt.
The account/control save reservation survives query/read replacement until the
attempt ends. Exact local-choice/submitted-echo/socket receipts stay distinct;
equal/ABA socket events retire old authority and may create a NEW read, never
acknowledge a pending save or revive old frames. A current success echoes only
the originally submitted value, ignoring unproved response prefs. NotRequested
and Unconfirmed have distinct copy; ambiguous saves retain the choice and require
a fresh explicit Retry save. Account-current rate outcomes are observed before
retired publication is discarded. Other preference/assignment retirement stays.

New focus282b ACTUAL4/0; Search28354, view2843, profile-read2854,
channel-fetch28682 and reconnect2876 passed on the identical harness.
Full288 ACTUAL5222/0/7, identical 237993472-byte executable,
SHA256 `8765cccc51166c6392754ac0bb4bfbac73a5dc9cdf0c51b3885c21019446d736`, bytes/mtime exact.
Every prior5218 pass, original92 repair and existing7ignored status is retained.
Actual terminal summary/exit inspected BEFORE milestone updater. Rebuild
608.87s/min commit8.72GiB/owned peak
17.973GiB; full238.82s.
Root6023 inputs/22source-script files and binary archived in
`tmp/parity-search-recovery-20261027/people-sticky-sort-milestone.json`.
All2104 old Rust test files protected; entire previous4test file exact prefix.

Core full281 ACTUAL5530/0/6 plus1doctest, same focus278 binary and2,116 inputs;
all earlier5528 passes/ignored preserved. Closed multipart writer uses only fixed
search_people_sort and raw relevant/name/name_desc with original credentials,
destination and literal _x_reason. Three actual synthetic loopback forms,
retired429, offline/stale refusal, redirect rejection, lost TCP/503 and real30s
deadline are verified. Root injected tests cover actual controller/factory flow.
No live Slack preference write or backend/native compatibility claim follows.

Native one30s attempt/no redirect/queue/replay/automatic retry is qualified
policy: Source persists/queues/retries and synthetic echo can precede transmission.
Source queue/persistence parity, Edit default/coachmarks, richer filters and
persisted UI remain unfinished. Four saving/unconfirmed normal/narrow fixtures
are registered only; no PNG/layout/focus/IME/native acceptance. Raw whitespace
edit still retires display through existing dialog guards despite retained
PageArc. Session0 remains blocked with no probes or production rebuild. Full
parity/live acceptance remains OPEN.

Failed282 compiled no tests: wrong failure-enum imports and new non-Debug Sort
assert_eq uses. Draft02 corrects ONLY actual public import paths and the same
five equality expectations. Old source/log retained, no false failed-binary
archive. Amp04 scoped source review and actual Core closure underlie execution;
Amp05 reported absent tools, not a new verification/approval. Root mechanically
verified the correction. Earlier Core225 still lacks final summary, incomplete.
All prior mixed-modal/participant/page/sort milestones and failures retained.

Next safe source-grounded journey: Account type common3 (Any account type,
Regular members, Deactivated), server filtering, explicit new-query producer and
fresh page1. Parent verifies43 source ranges/26newAST nodes and literal main/
reset dependencies. Amp plan01 is scoped clear; its prior-milestone hold is now
satisfied by ACTUAL288. No next product implementation/result yet. Enterprise
roles/Guests/Frontline and hidden eager all-module lifecycle remain outside it.
Visible-only native fetching must remain explicitly qualified.

Goal ACTIVE by renewed24h user supervision; persistent API remains stale blocked
without active-reset support, never marked complete. HEAD/production unchanged,
worklog untouched. Targets53.484GiB/free92.681GiB afterfull288.
DesktopTest1 dev-fast process-scoped incremental off; ordinary/Core8; one Cargo
pipeline/stable caches. Stop5/hard4GiB commit, free50GiB disk, helper56GiB remains
sufficient with conditional96GiB ceiling. Preserve source/caches/evidence; no
cleanup/commits/pushes/live writes/calls/uploads/reactions/notifications/device/
audio/OS input/security/trust/signing/registration. Full parity is unachieved.


## Next Account type common3: Core draft01, unverified

Prior saved-sort full288 actuallyclosed5222/0/7, Core5530+doc281 archived; Amp next-plan hold satisfied. Parent independently verifies43source ranges/26ASTnodes plus4snapshot excerpts and5carried ledgers. Full reference snapshot files are separately preserved before docs/source move. Exactcommon choices Any omits account_type, Regular members sends0,1,2,3,7, Deactivated6; selected team/hide preferences unchanged. Source new query producer +first-all UUID/query/search order distinct from module sort; native visible-People-only fetching remains explicit adaptation, enterprise/Guests/Frontline blocked.

Core draft01 applies8files/3new methods/2120 frozeninputs, formatted/checkexit0; UNCOMPILED/UNVERIFIED. Existing Input/default request keeps exact old fields/order, new closed typed wrapper and optional exact2CSV validation. Explicit query id->two clock touches and first-all latest/query/search order share unchanged ordinary getters. Original transport test file exactprefix, other old tests untouched. Amp appliedCore02 review pending; freshfocused/onefull required. Root filter actor/UI not yet applied, no next result/live/native/prod/restricted action.

## Account type Core5533 ACTUAL; desktop294 pending

Core289accounttype2/290explicitquery1/291People11/292savedpref2 ACTUALpass; full293 ACTUAL5533/0/6 plusONEdoctest, identical205739520-byte focus289 harness SHAad9934285b06c2227359ee12485e3e6bc6857fc532179d0f963f72102dc26f38 bytes/mtime exact. All5530earlier passes/6ignored preserved;2120 inputs. Rebuild189.15s/mincommit14.903GiB/ownedpeak11.977GiB; full722.28s/min21.255GiB, actualunit639.42s. Targets53.563GiB/free91.537GiB. ActualclosedCore5533source/binary archived before updater; oldCore225 remains incomplete. Amp02 source-clear typo plus281doc corrected:281isgatenumber, actualONEdoc.

Root common3 controller/UI/factory/first-all mode and fourregressions frozen6029inputs,2107oldtestfiles protected; full priornumbered andtransporttestfile prefixes exact. Amp03 scopedsource-clear conditionalactualCore293; Corehold nowmet, rootfmtcheck running beforefresh294focus. No Desktop result/visual/native/livewrite/prod/restrictedaction claimed. Sourcevisible-only adaptation and blockedenterprise/Guests/Frontline/contact/default-settings dependencies retained.

## Verified common3 Account type journey: Desktop5226

People Account type now has the three source-backed common choices: Any account
type, Regular members and Deactivated. They fetch server-filtered page1 and keep
server order/duplicate IDs; they do not filter the loaded roster locally. Any
omits account_type; the other choices send exact0,1,2,3,7 and6. The current
rendered raw-query/dialog/account/cursor/page activation must still match before
a choice clears all retained native Search result bags/queued opens, replaces
the control identity and explicitly starts a query session. First page uses
latest-API UUID then query then search getters; captured query survives cold
search rotation. Paging/sort/retry keep the normal ordering. Exact old work,
page/cache ABA, retries and account changes cannot reclaim the new generation.

Accepted preferences/hide-deactivated, owned empty team, other cold controls,
account cooldown and saved-sort reservation remain intact. Saved-sort completion
releases its original reservation and refreshes only the current filtered read.
Native visible-only People fetching after clearing native bags is an explicit
adaptation of Source eager all-module lifecycle; no hidden results/telemetry/URL/
restored-state equivalence. Source same-value handler refetches; native refusal
is a deliberate stronger policy. Enterprise roles/Guests/Frontline and contact
mapping remain outside the three supported choices.

Focus294 ACTUAL4/0; Search29558, view2963, profile-read2974,
channel-fetch29882 and reconnect2996 passed on the identical harness.
Full300 ACTUAL5226/0/7, same 238076928-byte binary,
SHA256 `a47b4f70744dafa93b61fcf8658bbc9775821e1c189e6c69a87d15d6ec50f085`, bytes/mtime exact.
All prior5222 passes, original92 repairs and7ignored statuses preserved. Actual
terminal summaries and exit0 inspected BEFORE updater. Rebuild704.77s,
minimum commit9.078GiB/owned peak18.114GiB;
full235.89s. Root6029inputs/18changed source-script files and binary
archived in `tmp/parity-search-recovery-20261027/people-account-type-milestone.json`.
All2107 earlier Rust test files protected; entire previous numbered/transport test
files are exact prefixes. Amp plan01/Core02/Root03 are scoped source clearances.

Core full293 ACTUAL5533/0/6 plusONEdoctest, identical289focus binary, all5530
prior passes and6ignored preserved across2,120 frozen inputs. New account-type
mapping/multipart2 and explicit-query-order1, affectedPeople11/savedpref2 passed.
Existing Input/default request fields/order and generic API/transport restrictions
remain unchanged; only the dedicated optional account_type values are accepted.
These Core real synthetic-loopback and Root actual shared-factory/controller
checks compose evidence; no raw public request getter or Root emitted-HTTP claim.
No current Source or cache/evidence/profile/lockfile/dependency cleanup.

Four normal/narrow filtered presentation fixtures are registered only, no PNG/
layout/focus/IME/native/backend acceptance. Session0 stays blocked without probes
or production rebuild. Source queue/persistence and editable defaults, richer
filters, Source File-Newest private mapping and broader search semantics remain
OPEN. Full parity is unachieved. Earlier Core225 still lacks a final summary;
all earlier failures and saved-sort5222/mixed-modal milestones remain preserved.

Next actionable source-grounded proposal: separate native explicit-user Messages/
Files sort memory on current user tab changes. Parent verifies25Source ranges,
12new AST nodes,8immutable full snapshots/10excerpts. Source locality is proven,
but unknown prefs never mean false, private File Newest differs from the native
endpoint, and existing core/history/People/default contracts must stay intact.
Editable default settings require broader per-module/cohort/fixed-key-writer work;
a People-only boolean setter would omit Source behavior. Its42source ranges/
30AST nodes are independently verified; historical document D1 is replayed from
an exact fullSHA reconstruction, never claimed equal to the changed current doc.
No next product changes/result yet; actual300 closes its prior-milestone hold.

Goal ACTIVE by renewed24h user supervision; persistent API remains stale blocked
without active reset, never marked complete. HEAD/production unchanged; worklog
untouched. Targets53.562GiB/free89.942GiB afterfull300.
DesktopTest1 dev-fast process-scoped incremental off, ordinary/Core8, one Cargo
pipeline/stable caches; stop5/hard4GiB commit/free50GiB disk, helper56 sufficient
under conditional96GiB ceiling. Preserve source/caches/evidence. No commits/pushes/
cleanup/live writes/calls/uploads/reactions/notifications/device/audio/OS input/
security/trust/signing/registration. Native/live/full acceptance remains OPEN.


## Next explicit Messages/Files sort memory: Root301 pending

Actual account-filter300 closed5226/0/7, Core5533+ONEdoc exact2120 inputs carries. Parent validates memory25source ranges/12newAST/8full immutable snapshots/10excerpts; Edit-default42ranges/30AST with exactD1 historicalSHA replay are separate broader blockers. Amp next-plan01 and applied02 scoped clear; root7files/fourmethods/6031inputs frozen UNCOMPILED/UNVERIFIED,2111oldtestfiles andoriginalbootstrap/viewSearch testtails exact.

Memory records only explicit different Messages/Files choices. UI kind restoresknown targetonly; legacyPrimitive/history/default/People paths andwirefactories remain exact. Original accountCredential owner versus current rendered/post-action visits; raw/dialog/Cursor/pagerquery/loading/failed/history exact. Ownerretirementlazy, not immediateerasure; stale controls cannotclear successor records. Localdataowner onlypairedauthabsence and neverpromoted; realexistingSearch credential gate returnswithoutrequest underabsence. No falseSourcepref/sticky/FileNewest/defaultsettings/backendoverride equivalence claim.

One missing outerbrace in syntheticJSON was caughtbyfmt beforecompile; failedsource preserved, correctedonlydelimiter. Protectionpreflight initiallyassumedwrongproductiontestfile; correctedboth exacttesttails, nooldtestchange. First memoryreviewpacket66093 locallyrefusedbeforeAmp, smaller58822 plan reviewed, nocapacityevent. Applied02 source39722chars clear. Fmtall/check actual0, Root301nowcompiles, thenfocused/affected/onefull307required. Coreunchanged meansno repeatCoresuite; no production/nativeSession0/livewrite/device/OS/security/cleanup/commit action.

### Memory301 focused PASS; Source02 original scoped task correction

**Work in progress after archived5,226:** Root-only Messages/Files explicit
sort-memory Source02 is frozen, current source UNVERIFIED. Source01 focus301
ACTUALLY passed4/0; binary/source/log archived,619.02s/mincommit9.009GiB/
ownedpeak17.842GiB. Watcher waited completedmetadata then stopped ONLY Python
orchestration before latergates; no Cargo/compiler targeted, all idle confirmed.
No superseded full run or fullmilestone claimed. Target53.563GiB/free89.090GiB.

Independent review found schedule used spawn_forever instead of original scoped
spawn. Pinned Dioxus57d6794 docs/code prove lifetime AND global-context difference;
restore exact originalAPI, no assumption of caller equivalence/no native failure
claim. Source02 changes ONE call; allother6030inputs/tests andCore2120 exact.
Fmtcheck0 and Amp03 scoped source-clear; fresh301b is compiling, then affected/
one identical-binary full307 required. Old4/0 never validates changed Source02.
7files/fourmethods/6031inputs,2111oldtestfiles protected, originalbootstrap/view
Search testtails exact. Core5533+ONEdoc carries unchanged. Owners/visit fences,
paired-absence localdata/no promotion, lazyretirement versus immediate erasure,
unchangedhistory/default/People/Core/transports and allrestrictions remain.
No current fresh pass/native/live/prod/parity claim. Latest verified5226 below.


PinnedAPI evidence message-file-sort-memory-task-lifetime-source-01.json; official0.7 async URL inaccessible viawebtool, exact57d6794 checkout docs/code used. spawn=currentScope, spawn_forever=ROOT anddifferentcontext; no actualnativebug asserted. Holdrecord verifiesonlyorchestratorPID2596 stopped aftercompleted301 metadata. No full/affected supersededrun. Frozen01 Source4passes/archive238308864B SHA0fb667ca9ad0974e4ae0cf66b5975114f0292118e0efda50cffde5c78a64ad6f remains attributedto01. Source02 onecallrestore requiresnew301b; Amp03 clear23.88s/8840chars. Stableconfiguration/no profile/caches/deps changes.

## Verified explicit Messages/Files sort memory: Desktop5230

Messages and Files retain their own explicit native sort choice while switching
Search types. Returning to a type restores its last user-selected sort before
starting the read. A type without a remembered choice keeps the existing carry
or default behavior. History replay, People controls and core pager semantics
remain unchanged. Memory belongs to the original window/account credential;
current owner, raw query, dialog, cursor, pager query/loading/error/history fences
are checked before changes, and the fresh visit is checked inside the original
scoped task before Search admission. Stale callbacks cannot clear successor
memory. Retirement is lazy on the next admitted action, with isolation before use;
this is not immediate credential erasure or preference persistence.

Source02 focus301b ACTUAL4/0; Search30262, view3033, profile3044,
channel30582 and reconnect3066 passed. ONE full307 ACTUAL5230/0/7 reused
that identical 238309888-byte harness, SHA256 `54a792d473b88e6ace6f4d7dd313c7c2a04bf1a86ef3077072f2bee2f371b83c`
with bytes/mtime exact. All prior5226 passing tests, original92 repairs and
7ignored statuses are preserved. Actual terminal summaries, child exit0 and
source manifests were checked before the milestone updater. Rebuild
652.42s, minimum commit8.531GiB,
owned peak18.032GiB; full229.1s.
Seven changed/new files,6031 frozen Root inputs,2111 earlier Rust test files
protected and original bootstrap/view Search test tails exact. Exact2120 Core
inputs carry actual Core5533/0/6 plusONEdoctest293; no redundant Core rebuild.
Evidence and binary: `tmp/parity-search-recovery-20261027/message-file-sort-memory-milestone.json`.

Source01 focus301 ACTUAL4/0 belongs only to its archived source/binary. Static
review found its spawn_forever differed from the original scoped spawn in both
lifetime and global context at pinned Dioxus57d6794. Orchestration was stopped
only after that Cargo job completed, before later gates. Source02 restores the
single original spawn call; the fresh301b-307 results above verify Source02.
No failed native behavior or superseded full run is inferred. All old evidence,
including incomplete Core225, failed282 and the earlier new-test format error,
is preserved. Amp plan01/applied02/scope03 are scoped source reviews.

This is bounded native explicit-choice behavior, not Slack saved-default or
unknown-preference equivalence. Source private File Newest is created/desc;
the unchanged native search.files path is timestamp/desc. Editable defaults,
cohort-dependent settings, queue/persistence, forced Message/backend override,
URL/analytics/restored and multiwindow behavior remain open. No layout or new
screen was introduced; no native/PNG/focus/IME/live backend acceptance is claimed.
Session0 remains blocked without more probes or a production rebuild.

Next source-grounded journey under review: real server-page navigation for
Search. Private Files requires a distinct endpoint/Hit/Page protocol, genuine
parameter admission and an owned metadata/preview authority bridge; passing a
Hit into the existing action-capable viewer is not admissible. Parent independently
verified33 inert-parsed factories/59 AST nodes/19 native excerpts and26 immutable
native metadata-path snapshots. Messages paging on the existing backend is also
being checked before selecting the next coherent product slice. No next source
change or result is claimed. The original mixed-encoding worklog remains untouched.

Goal ACTIVE by renewed24h user supervision; API remains stale blocked without
an active reset, never marked complete. Full parity remains open. HEAD and
production executable are unchanged. Targets53.564GiB/free
87.703GiB afterfull307. DesktopTest1 dev-fast process-scoped
incremental off; ordinary/Core8, one Cargo pipeline, stable caches. Stop5/hard4GiB
commit, free50GiB disk and helper56 sufficient under conditional96GiB ceiling.
Preserve all source/caches/evidence. No commits/pushes/cleanup/live writes/calls/
uploads/reactions/notifications/device/audio/OS input/security/trust/signing/
registration. Native/live/full acceptance remains open.


## Verified Messages selected server-page journey: Desktop5238

Messages Search now selects one server page and supports Previous/Next and a
clamped five-number range. Returning to a cached page, including an empty page,
uses retained data without another search or author lookup. Initial explicit
server page bounds remain the navigation bound; the latest accepted known total,
including zero, survives cached navigation. Unknown first-page metadata retains
the existing legacy path; an owned later-page metadata error preserves the loaded
page and cannot silently become append mode.

A failed target is separate from the displayed page. Retry and outage recovery
request that exact target (tested with shown1/failed4), retaining loaded rows and
label. Account/session/client/transport/workspace-self/raw-query/kind/sort/request
and non-reusable Arc frame/activation identities fence the book. Manual accepted
cache reads can finish while Search is closed without reopening it. Rendered
controls/rows and recovery retain separate stricter dialog/Cursor/activation and
admitted-Visit guards. Cached1->2->1 cannot revive old callbacks. Legacy append,
generic retry/recovery and ID-only row opening cannot bypass an owned book.

The actual row opener checks the retained occurrence before its one owned cancel
successor and retains original credentials through metadata and navigation guards.
Existing conversation-info/posting/channel/thread/Canvas departure paths stay in
place. New tests cover actual entry plus metadata and final Shell navigation
helpers; they do not execute the whole async history/thread/live workflow.

Focus311b ACTUAL5/0, actual rendered controls3122/0, opener3136/0,
Search31468/0, profile3154/0, channel31682/0 and reconnect3176/0 passed.
ONE full318 ACTUAL5238/0/7 reused that identical 239020032-byte harness,
SHA256 `5af94d62d69e64914927ec62e97a25bb54532101b0f19d7cf01199e62df1f0a7`, bytes/mtime exact. All prior5230 passes,
original92 repairs and7ignored statuses preserved. Actual terminal summaries,
child exit0 and frozen inputs were checked BEFORE milestone closure/updater.
Rebuild597.56s/mincommit8.159GiB/
ownedpeak17.984GiB; full244.4s.
6040 Root inputs/21 changed or new files archived, all3170 old test-or-cfg Rust
files protected unchanged or exact whole original test modules/declarations.
Evidence: `tmp/parity-search-recovery-20261027/message-page-journey-milestone.json`.

Core310 ACTUAL5536/0/6 plusONEdoctest reused focus3083/0 binary; affected309
ACTUAL82/0 and all prior5533/ignored6 preserved across2122inputs. Exact-target
admission is additive; legacy begin/append/People/Files/retry contracts and their
original tests are unchanged. That verified Core result carries on exact inputs;
no redundant Core or production rebuild. Original Core225 remains incomplete.

Actual311 failed compilation101 before tests: new Rc violated original Send
hydration bound, key form/helper visibility/Offline payload were wrong. Draft03
uses Arc with no unsafe Send, the required key/visibility and real Offline(String)
constructors; fresh311b-318 above verifies it. Original old tests were not weakened.
All failed source/logs retained. First309 count guard expected9 prefix matches,
while Cargo substring selected82 prior passing tests;313 expected4 missed2existing
opener tests and all6 passed. Actual registered5245 names preserved every prior
name and added8. Guards corrected without repeated tests, rebuild or configuration
cycle. Source plan01/A02/B04/correction05 are scoped reviews, not native acceptance.

This is qualified native page navigation. Existing count20/per-request UUIDs and
within-page flatten/filter/dedupe are retained; source raw grouped/rich/AI order,
query/search session IDs, URL/analytics, concurrent inactive cache fill and scroll/
roving focus remain open. One selected page is rendered; total retained page-cache
memory is not claimed bounded. Six named normal/narrow ready/page2/failed fixtures
are registered only. No PNG/layout/focus/IME/native/live backend acceptance.
Session0 remains blocked without repeat probes. Full parity is unachieved.

Next source-grounded candidate is the private Files sort/page journey. Parent
verified32factory/29typedAST/45ranges/25snapshots for parameter acquisition (5path
spellings dedupe to3physicalassets), and6distinctassets/46factories/59AST for the
singleton/canonical/imagePDF chain and policy producers. Initial narrower missing-
producer finding is withdrawn after expanded retained-asset discovery. Genuine
native parameter/policy/IP/download/canonical receipts and an explicit no-action
preview boundary remain prerequisites. Existing unconditional DownloadButton and
permissive File model cannot grant this authority; snippet/text is a different
Source route. No private File permission or readiness has been manufactured.

Goal ACTIVE by renewed24h supervision; API stale blocked lacks active reset,
never marked complete. HEAD/production unchanged; mixed worklog untouched.
Targets53.544GiB/free86.68GiB afterfull318.
Stable Root1job process-scoped noinc dev-fast, ordinary/Core8, one Cargo pipeline;
stop5/hard4GiB commit/free50GiB disk/helper56 sufficient under conditional96GiB.
Preserve source/caches/evidence. No commits/pushes/cleanup/live writes/calls/uploads/
reactions/notifications/device/audio/OS input/security/trust/signing/registration.



## Verified private Files find/sort/selected-page journey: Desktop5243

Established Files search offers the actual five server sorts: Most relevant(score),
Oldest(timestamp_asc), Newest(created), A to Z(name), Z to A(name_desc). Genuinely
current original count/sticky/File-flag receipts and original socket carriers admit
only the local-sort plain-term branch. Missing carriers, unsupported values and
sticky true are not manufactured into readiness. People contracts remain separate.

One selected server page preserves order/duplicate occurrences. Previous/Next/
numbered pages use an eight-page native cache, including present empty pages.
Initial navigation bounds and latest totals survive cache reads. Pending rows hide;
failed targets retain the shown page and exact retry (shown1/failed4). Reconnect
retains original admitted Visit. Full original account/session/client/transport/
query/kind/sort/channel/tab/thread/dialog/frame/control/receipt identity guards
asynchronous work, cancellation and equal/ABA transitions. History/Filters cannot
revive old authority. An owned private scope never falls back into legacy work.

Hits are informational: no File/URL/canonical/media/preview/download/write authority.
Tombstone and newer file-event vetoes remain. Closed search.modules.files multipart,
cold Home Primary mapping and native routing/auth/metadata are qualified native
policy, not Source Gantry/browser/private-service or full rich-search equivalence.

ACTUAL UI3441/0/controller3454/0/Search34672/0/channel34782/0/
realtime34878/0/2originalignored/reconnect3496/0/view3506/0/
file-events35113/0/profile-read3524/0 passed. ONE full354 ACTUAL5243/0/7
reused that identical 239777792-byte harness, SHA256
`ea712dfbac9c9be9cd4bce9b08b385868b6006d008dd39a2b77e58982259b63f`, bytes/mtime exact. Allprior5238/original92/ignored7
preserved. Terminal summaries/child exit0/passsets/frozen inputs checked BEFORE
closure/updater. Core3216/0,3221/0 andONEfull323 ACTUAL5543unit/0/6+ONEdoc
reused one Core binary; allprior5536+doc/ignored6 preserved. Root6055/Core2130
inputs and34source files archived;3175old Rust test/cfg files protected.
Evidence: `tmp/parity-search-recovery-20261027/private-file-pages-milestone.json`.

UI3250/1 lookup and3340/1 harness timeout retained with source/binaries/logs.
Draft08 corrects only new-test mounted targets/task polling under unchanged3s
deadline and assertions; product/Core unchanged from draft05. Existing16MiB stack
was restored for a diagnostic that initially overflowed before the assertion.
Realtime guard corrected80expected passes to78+2originalignored without rerunning
passed tests. Original tests unchanged; historical Core225 remains incomplete.

Rebuild627.52s/mincommit8.343GiB/
ownedpeak18.105GiB; full243.95s.
Targets53.553GiB/free82.032GiB after full354.
Root1job process-scoped noincremental dev-fast, ordinary/Core8; one pipeline,
stop5/hard4GiB commit/free50GiB disk/helper56 within conditional96GiB.
HEAD/production/caches/profiles/manifests/locks/dependencies unchanged.

Native/session0/live/PNG/layout/focus/IME/scroll acceptance remains OPEN. Four
normal/narrow fixtures are registered only. Full parity is unachieved. Next safe
slice: genuine saved Files order/five-choice saving under scoped Amp plan01,
strict separate admission/account-wide reservation/honest SubmittedOutcome;
no live preference writes. Source proof4assets/42factories/75ownAST/31snapshots.
Static JPEG canonical/policy/content-only media proofs retained for later preview.
Goal ACTIVE renewed24h; stale blocked API lacks active reset. Mixed worklog untouched.
No commits/pushes/cleanup/live writes/calls/uploads/reactions/notifications/device/
audio/OS input/security/trust/signing/registration.


## Verified saved Files order and independent save journey: Desktop5247

Genuinely current Boolean stick_user_sort=true and one of the exact five raw
search_file_sort strings restore ordinary Files order without a write. Separate
saved observations use actual responsible Init/post-store and original socket
carriers; equal/ABA events retire old authority. Local Files KEYS4 and People
contracts remain unchanged. Missing/invalid/nonmenu/exotic modes stay unsupported.

An explicit current choice adopts only named LocalChoice and independently starts
a fresh page1 read plus one bounded fixed-key multipart save. Both completion
orders work. Success reports SubmittedOutcome, without canonical persistence,
response-echo rebinding, receipt replacement or acknowledgment-only refetch.
Failed save retains the choice/results and distinguishes not-sent from unconfirmed.
Retry saving is explicit/current and performs no read. Page retry/reconnect and
cached empty/selected-page behavior retain their original full currentness guards.

A synchronous unique non-Clone reservation blocks overlapping saves by stable
native account/team/self across query/scene/receipt/epoch/credential retirement.
The private root driver drains the bounded request; only its token releases its
physical slot. Original full owner/frame/parameters still gate send/publication.
An Arc/Mutex/AtomicBool sidecar preserves ShellState Send and carries only nonsecret
coordination at both fresh-shell replacement paths. Old UI/receipts are not carried.
Legacy loading refuses structural private ownership before/after await; owned
refresh remains handled if fresh admission fails. Hits grant no File/URL/preview/
download/canonical/action authority.

ACTUAL controller3683/0/generated3691/0/Search37075/0/channel37182/0/
realtime37278/0/2originalignored/reconnect3736/0/view3747/0/
file-events37513/0/profile-read3764/0/session37711/0 passed.
ONE full378 ACTUAL5247/0/7 reused the identical 240334848-byte harness,
SHA256 `e226d316d3754ae76d46e7f0822161a820b8d9d7615fa02835c32fc10b76a7b9`, bytes/mtime exact. Allprior5243/original92/
ignored7 preserved. Actual terminal summaries/child exit0/passsets/frozen inputs
checked BEFORE closure/updater. Core3552/0 andONEfull356 ACTUAL5545unit+
ONEdoctest/0/6unchangedignored reuse one binary and preserve allprior5543+doc.
Root6068/Core2135 inputs,28batchfiles archived;3187old test/cfg files protected.
The prior generated test is an exact byte prefix with only an additive child module.
Evidence: `tmp/parity-search-recovery-20261027/file-saved-sort-milestone.json`.

Earlier3572/1fresh-fixture T1/TINIT0001 mismatch and unchanged357UI1/0
diagnostic remain preserved. New test setup corrected with matching fresh fixture;
all assertions/product/Core byteexact. Scoped fixture-fix06 source review clear;
fresh368+gates above supply execution. Scoped Amp plan01/core02/write04 clear;
read03 evidence gap withdrawn05 after
complete callers/guards and explicit legacy refusal. Static Rc recommendation
corrected immutably from the exact Tokio Send bound; no unsafe Send or original
test weakening. Source-only reviews do not establish executed/native acceptance.
Rebuild627.49s/mincommit7.969GiB/
ownedpeak18.116GiB; full240.86s.
The earlier357 build compiled the changed core path dependency and desktop
harness before its2/1fixture failure. Corrective368 reuses the unchanged Core
library and recompiles the harness. Incremental-off applies process-wide, but
cached codegen trace replay is not evidence of a fresh dependency compilation.
Focused/affected/full use one unchanged binary/config; helper restores environment.
Targets53.586GiB/free78.303GiB afterfull378.
Root1job processINC0 dev-fast/ordinaryCore8/onepipeline/stop5hard4/free50/helper56
within conditional96GiB; profiles/manifests/locks/deps/caches/HEAD/production unchanged.

Qualified native cold/main-tab/routing/auth/metadata/single-attempt policy, not
Source browser/Gantry/private-service/persistence equivalence. Four saving/failure
normal/narrow fixtures are registered only. Native/session0/live/PNG/layout/focus/
IME/scroll and full parity remain OPEN. No production rebuild/native probes/live
preference writes. Next safe slice: Documents/Images/PDFs staged query-only editor under scoped
Amp file-type plan01, after this actual milestone. Independent proof27factories/
71ownAST/25snapshots. JPEG preview stays blocked on missing native canonical
preimage-or-hydration/search-raw/publication/readback boundary (46snapshots/
12factories/43ownAST/64priorEdges). This is a native data prerequisite; no immediate
human approval/live probe is requested. New empty namespaces, typed File absence
or conditional explicit-field algebra cannot mint image authority.
Goal ACTIVE renewed24h; stale blocked API lacks active reset. Mixed worklog untouched.
No commits/pushes/cleanup/livewrites/calls/uploads/reactions/notifications/device/
audio/OS input/security/trust/signing/registration.

Checklist delta: Files local/saved sort/page/read-save is offline verified here;
Search three-type editor is source-plan clear and unimplemented; JPEG canonical
data prerequisite and native/live/visual acceptance remain open. Original brief/
checklist attribution stays in the preserved mixed worklog.


## Verified staged Files type-query editor: Desktop5251

Files filters now stage Documents, Images and PDFs alongside channel, sender and
calendar fields. Apply validates the entire draft before changing the query;
Clear stages removal and Cancel discards the draft. Canonical plural clauses and
explicit singular aliases are recognized. No-op selections preserve original
bytes; edits preserve negative, unknown, quoted and localized type syntax.
An unfinished original quote refuses appended clauses, including a removal-only
change that must re-append another retained category. Generic Messages syntax
and its original filter behavior remain unchanged.

Every Files field, checkbox, Clear, Apply and Enter retains its original owned
draft. Original account/epoch/team/self, session/cookie/transport/client, query,
kind/sort/page revision, channel/thread/tab and unique Arc identity gate edits
and queued submission. Clear rotates identity and keyed inputs; cancellation
retires the draft. Credential-free Files presentation controls stay disabled and
inert even across a queued kind change. No fixture identity is manufactured.

An unchanged query retains private Files ownership and its read path. A changed
trimmed query clears that scene and uses the existing legacy Files request,
sort, pagination and labels. Private colon/quote refusal and pre/post-await
scope checks remain. This query editor grants no File, URL, canonical, preview,
download, media, action or global preference-write authority.

Actual controller392:3/0 and generated controls393:1/0 passed. Affected Search394:
75/0, channel395:82/0, realtime396:78/0 with 2 original ignored, reconnect397:6/0,
view398:8/0, file-events399:13/0, profile-read400:4/0 and session401:11/0 passed.
ONE full402 completed 5251/0/7 on the identical 240564736-byte harness,
SHA256 `23d9cccd6e02e1b2c9a780f1b77f6ccaaca41a8cb4f8fce8ab948c1b546a3a65`, with byte/mtime identity and every prior5247/original92/
ignored7 preserved. Actual child exits, final summaries, pass names, ignored set
and frozen inputs were checked before closure/updater. Core379:31/0 and ONE
full380:5549 unit tests plus one doctest passed, same focused binary, preserving
prior5545+doc and six original ignored. Historical Core225 still lacks a final
summary and is not retroactively closed by any successor.

Root6075/Core2137 inputs and all17 batch files are archived;3195 old test/cfg
files are protected. The original generated-control test file remains an exact
byte prefix with only an additive child module. Evidence:
`tmp/parity-search-recovery-20261027/file-type-editor-milestone.json`.

Amp Core02 caught the open-quote removal-only bug before compilation; corrected
CoreQuote03 cleared it. Root04/05 caught the unowned Files generic fallback;
SourceCorrection06 cleared the original-kind dispatch correction before compile.
All prior reviews, uncompiled snapshots and actual results remain preserved.
A new test mutable binding was corrected before first compilation. Failed381
controller2/1 and unchanged381generatedUI0/1 remain preserved: the new test
executor incorrectly inspected URL query pairs, while the existing builder uses
a POST form. Source07 cleared endpoint+POST+Form+exactly-one-body-query+exactbytes
correction. Only that new cfgtest executor changed; product/Core/original tests
and assertions remain unchanged. Fresh392-402 gates above supply execution.

Rebuild 656.99s; minimum commit 8.07GiB;
sampled owned peak 18.036GiB. Full 240.13s.
First381 compiled the changed Core path dependency and Root. Corrected392 reused
the unchanged Core library and recompiled Root. Incremental-off remains process
scoped; cached codegen log replay alone does not prove fresh compilation.
Later gates use identical configuration/binary. Targets 53.647GiB,
free disk 74.157GiB after full. Desktop1job/dev-fast/processINC0,
ordinary Core8, single pipeline, stop5/hard4/free50/helper56 within conditional96.
Profiles/manifests/locks/dependencies/defaults/caches/HEAD/production unchanged.

This is offline native query-editor verification, not Source browser/Gantry or
backend multi-type union/intersection/ranking equivalence. Four presentation-only
normal/narrow selected/clear fixtures are registered. Native Session0, live Slack,
PNG/layout/focus/IME/scroll and full parity remain open; no native probes or
production rebuild. JPEG preview remains genuinely blocked on native canonical
preimage/hydration and accepted raw-search publication/readback prerequisites.
No empty namespace or typed File absence supplies that authority.

Checklist delta: three-category Files query editing is implemented and offline
verified within the limits above. Next safe journey: source-grounded multi-channel
and multi-sender Messages filtering under source-plan01/composition02 clearance; the existing
search.modules.messages route remains unchanged. Original mixed worklog untouched.
Goal ACTIVE under renewed24h supervision; stale blocked goal API has no active reset.
No commits/pushes/cleanup/live writes/calls/uploads/reactions/notifications/device/
audio/OS input/security/trust/signing/registration.


## Verified known-channel and author Messages filtering: Desktop5256

Messages filters now stage multiple already-known ordinary channels and cached
nondeleted authors together. Local find renders at most30 matching choices while
retaining the full selection; the generated40-author journey verifies that this
window is not a selection cap. Duplicate labels keep distinct IDs. Current native
map-key/object-ID agreement, projected-channel visibility and existing author
eligibility are required; bots are not silently excluded and remote lookup is
not introduced. No parsing or choice creates a canonical record or permission.

Apply composes repeated embedded-ID in:/from: clauses with the existing native
With/date/flags in one transaction. Original angle-aware byte spans preserve
retained occurrences, labels, ordering, whitespace and unknown/name/negative/
quoted/localized scopes. Text inside opaque reference labels cannot become a
separate modifier. Every addition validates original quote/reference completeness
before any removal can hide it. New text values with angle brackets refuse in
this additive native path; retained raw text and the old String API remain exact.
Scoped Clear stages one scope; full Clear stages scopes/other fields; Cancel
retains the query and loaded results.

Original account/epoch/team/self, session/cookie/transport/client, query/input/
dialog/kind/sort/page, view/channel/thread/tab and unique origin gate the draft
and queued submission. Relevant original bank entries are revalidated before
publication. Shared Generic/Files/Messages callbacks retain their original branch;
typed missing/stale sources are disabled/inert and never fall back. Clear rotates
identity and input keys; cancellation/departure retires the draft. An additional
File Owner/Toggle page guard covers accepted coarse-pager completion without a
revision bump; the new regression uses actual begin/complete. Existing private
Files parameters/grammar/read/currentness/no-fallback contracts stay intact.

Byte-identical Messages Apply uses the admitted existing-page read, retaining the
Book, visible rows and other cached pages while the response is held. Success
replaces only the read page; failure retains the cache. Existing exact failed-target
retry is reused; loading/retired owned Books stay handled without initial fallback.
Cold no-Book fallback rechecks the original submission owner. Changed queries
retain the original reset/dispatcher. Endpoint search.modules.messages, POST form,
count20, flags/auth/sort/decoder and manual-versus-recovery lifetime stay unchanged.
Old navigation/cache/retry/recovery use the unchanged non-refresh planner behavior.

ACTUAL controller405:3/0 and generated406:2/0 passed, then Search407:75/0,
channel408:82/0, realtime409:78/0 with2originalignored, reconnect410:6/0,
view411:10/0, fileevents412:13/0, profile-read413:4/0 and session414:11/0.
ONE full415 completed5256/0/7 on the identical 241104896-byte harness,
SHA256 `e2d6a473fa1f39d56d31b09e4c152c1b28f8d20b66c44d5c4d5909f39b29aee3`, bytes/mtime exact. Allprior5251/original92/ignored7
preserved. Actual terminal summaries/childexit/passsets/ignored/source checks
preceded closure/updater. Core403:37/0 and ONEfull404:5555unit+ONEdoctest/0/6
used one binary and preserved allprior5549+doc/sixignored. Root6088/Core2141
inputs and27batchfiles archived;3201old test/cfg files protected; old UIhelper
entire file remains an exact prefix. Evidence:
`tmp/parity-search-recovery-20261027/message-multi-scope-milestone.json`.

Amp core03 caught opaque-quote and generated-with-reference absorption bugs;
correction04 cleared both before compilation, followed by actual Core gates.
State05/UI06 source cleared their supplied areas. Read07 caught a no-op cache
loss after spawned general-search dispatch; correction08 cleared a qualified
current-page refresh. The NEW generated regression now pumps to the third request,
holds its response, checks Book/loaded rows, then verifies cached2 return without
another read. Prior reviews/drafts remain preserved; no old test was weakened.

Rebuild723.47s/mincommit8.308GiB/
ownedpeak18.145GiB; full239.38s.
Changed Core path dependency participates in processINC0 desktop compilation;
cached codegen replay alone is not a new compilation. Later gates reuse identical
configuration and binary. Targets53.683GiB/free71.003GiB.
Desktop1job processINC0/dev-fast, ordinaryCore8, singlepipeline, stop5/hard4/free50/
helper56 withinconditional96. No profile/manifest/lock/dependency/default/cache/
HEAD/production changes. Historical Core225 remains incomplete on its own source.

This is native known-cache query editing, not Source browser/Gantry/provider/
name/locale/sentinel-system or backend OR/AND/ranking equivalence. Four disabled
normal/narrow selected/clear presentation fixtures are registered only. Native
Session0/live/PNG/layout/focus/IME/scroll/keyboard and full parity remain OPEN;
no production rebuild/native probes or live writes. JPEG still needs actual native
canonical preimage/hydration/accepted raw-search publication/readback proof;
empty namespaces or typed absence supply no readiness.

Checklist delta: this known-channel/author Messages journey is implemented and
offline verified within the bounds above. Next static candidate concerns a channel
Threads tab, distinct from global Threads; actual source flag observation and
channel-scoped canonical/read prerequisites must be closed before implementation.
Original brief/checklist and mixed worklog remain preserved. Goal ACTIVE renewed24h;
stale blocked goal API has no active reset. No commits/pushes/cleanup/livewrites/
calls/uploads/reactions/notifications/device/audio/OSinput/security/trust/signing/
registration. Safe work continues under the original goal.
