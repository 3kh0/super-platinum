# Slack parity and Geist redesign

This is an implementation and verification ledger, not a claim of parity.
The requested outcome is full Slack feature parity, a premium Geist-inspired
interface, and measured performance improvements. Do not mark this project
complete while any required workflow below is missing or unverified.

## Testing boundary

Use the user's open Slack browser as the reference. Message sends are authorized
only in the private `#echotestchamber`; other write testing must also stay in
private channels. Never expose credentials in logs or commit real Slack captures.
Use isolated native fixtures for UI iteration. Preserve real drafts and session
data. A passing unit suite is not evidence of live Slack behavior.

## Current evidence, 2026-10-12

- Working repository: `snack`, Rust/Dioxus Desktop. Initially clean.
- Playwright reference: signed-in Hack Club Slack, Home/announcements. Reference
  visibly exposes Home, DMs, Activity, Files, Later, Agents & tools; channel tabs
  include Messages, Files & links, and Pins.
- Native baseline: `tmp/agent-ui-before/` captures conversation, multiline
  composer, thread/profile panes, DMs, and settings.
- Baseline rail exposed Home, DMs, Activity. Later, Files, and composer formatting now
  have native surfaces; Files browsing and a private upload/search/preview/download round trip are verified.
- Design reference: https://vercel.com/geist/introduction and its colors,
  typography, and materials pages. Fonts are vendored with the upstream license.

## Workstreams and acceptance evidence

| Workflow | Current status | Required verification |
| --- | --- | --- |
| Conversation reading, unread markers, history, navigation | Existing; audit pending | Live compare; cold/warm history, pagination, workspace switches |
| Compose and reply, formatting, drafts, edits, deletes | Formatting controls and shortcuts implemented; native Unicode/selection checks and bold private round trip passed; local draft resume resolves missing conversation metadata with scoped completion | Live cold-destination draft resume; remaining draft/edit/delete/list/link/mention workflows |
| Mentions, emoji, custom emoji, new reactions | Picker, full catalog, aliases, private custom reaction add/remove passed across both clients and thread root | Mentions, composer emoji insertion, tone/recents, arbitrary reply reaction checks |
| Search, filters, results navigation | Channel/sender/date controls, sorting, scoped paging, sidebar and conversation shortcuts, exact thread replies, and archived private conversation navigation verified in the signed client; missing metadata resolution has guarded regression coverage | Cross-client comparison, additional filter types, live cold-metadata resolution, and live multi-account/outage races |
| Files, upload/download, previews, channel files | Native browser, pagination, text/code/audio and searchable PDF previews, file links and confirmed own-file deletion/renaming in Files and attachment viewers implemented; private upload/download/delete/rename API round trips verified | Cross-client file metadata refresh, broader file actions, Office previews, richer PDF workflows, large original downloads, native canvas/list editing |
| Later, save/remind, completed items | Native surface, pagination, message actions, personal reminder creation, and message realtime verified privately; saved files, metadata/viewer navigation and durable uncertain-operation recovery passed signed offline normal/narrow fixtures | Live saved-file mutation/readback/realtime, full saved-index coverage and cold message targets |
| Pins, bookmarks, channel tabs | Pins, channel Files & links, link/folder bookmark management and explicit ordering implemented; private pin and bookmark API round trips verified | Custom tabs, bookmark emoji/non-link actions, cross-client bookmark realtime, older thread targets, large lists, offline and permission cases |
| Forwarding and message links | Rich notes, people/directory discovery, deferred DM opening, multiple destinations, per-destination receipts and explicit uncertain resends | Scheduled forwarding, full note controls, live new-DM/multi-destination/fault recovery |
| Channels, membership, details, management | Native create, About/Members, metadata editing, archive/unarchive, join/leave, people invitations/removal; open member lists refresh on membership events/reconnect; live private discovery, missed-create/rename recovery after socket outage, and people search verified | Successful live add/remove round trip, nonsingleton self join/leave, app/agent invitations; large lists and permission/offline cases |
| DMs, group DMs, profiles, presence, status | Quick switcher retains people without loaded DMs; dedicated recipient picker and DM/group-DM opening implemented; remote recipient discovery verified in the signed client; account-scoped status text/emoji/expiry editor and uncertain-result readback passed native offline checks | Live first-DM/group-creation API round trip, live remote-recipient selection, live status set/get responses and broader draft checks |
| Threads, Activity, read state, subscriptions | Shared conversation/thread surfaces; Activity preserves exact reply targets with parsed-shape/request regressions and normal/narrow native click verification; agent navigation now uses the same reply destination | Both-client realtime and live read-state reconciliation |
| Huddles, camera, sharing, device selection | Existing; live media unproven | Controlled private call and media checks |
| Canvases, lists, app interactions, workflows | Native canvas reader/create/rename and bounded checkbox/paragraph editing with private cross-client save/restore proof and paired first-load recovery; Lists support saved tables/boards, record details, guarded rich-text/number/select/boolean/calendar-date and ordinary User and Channel edits, item creation, conditional deletion confirmation, and same-ID deletion Undo/Redo; private live writes/readback verified; typed app modals and ordinary message buttons/static-select/radio/checkbox/static-multi-select/datepicker/timepicker/users-select/conversations-select controls have signed offline checks, including normal/narrow multi-picker interactions and full-state wire regressions | Live app receipt/responses remain unverified; broader app controls; List continuation, saved-view writes, remaining field types and item actions; richer canvas authoring/collaboration, comments, custom tabs and workflow discovery/execution |
| Notifications, DND, settings, accounts | Existing; audit pending | Native bundle tests, persistence, isolation |
| Keyboard, focus, accessibility, narrow windows | Shortcut handoff and IME event guards tested through the native WebView; partial overall | Real OS input-method composition, remaining native interaction, labels, reduced motion, layout sweeps |
| Geist visual system | In progress | Native screenshots of all surfaces and states |
| Speed, memory, idle work, large workspaces | Paired quick-switcher, channel-geometry and thread-index CPU benchmarks show lower calculation cost; visible projections remain bounded; signed native channel/thread scrolling and selection checks pass | End-to-end live startup, scrolling, interaction, memory/idle measurements and broader workloads; thread CPU probes do not establish native latency gains |

## Changes underway

1. Neutral default palette, shared selection/hover colors, readable secondary
   labels, restrained borders/elevation, locally served variable Geist fonts.
2. Formatting controls and shortcuts for channel and thread composers; matching
   local textarea autosizing. DOM text/selection and navigation guards prevent
   stale formatting work from replacing newer input.
3. Reaction picker with keyboard search/navigation, common emoji first, bounded
   120-item rendering, and workspace catalog loading on demand. Catalog sorting
   runs off the UI thread; account-scoped cache lives for five minutes. Alias
   resolution supports custom images and standard glyphs, rejecting cycles and
   unsafe URLs. Search results are memoized independently of media arrivals.
4. Scroll performance observer now schedules animation frames only during a
   scroll burst, stops after inactivity/hidden state, and restarts on scroll.
5. Later has in-progress/completed/archived tabs, cursor pagination, message
   previews, save/remove, complete/restore, archive/unarchive, custom local-time
   reminders, and personal reminder creation. Requests and page generations are
   account/workspace scoped; mutations survive their originating view closing.
6. Saved-message navigation keeps an explicit target inside the bounded render
   window, including after history projection, and does not discard it when an
   empty/new timeline reports its bottom. Scroll acknowledgement happens after
   the row is queried in an animation frame. Later thread-opening callbacks also
   check the channel generation.

## Verified evidence and open failures

- Native formatting: selected Unicode text with an astral rocket character;
  bold toggle on/off, italic shortcut, multiline quotes, and independent thread
  formatting all checked in the actual Dioxus fixture.
- Private Slack round trip: the test message beginning `Super Platinum parity
  check · 2026-10-12` arrived with a bold DOM node in Slack. No messages were sent
  outside `#echotestchamber`.
  A reply beginning `Super Platinum thread parity check` was composed with the
  native italic shortcut and a Unicode rocket, sent from the thread composer,
  and independently verified as an italic DOM node in Slack.
- The initial live reaction selection closed the picker but did not save a
  reaction. Found scope-owned request cancellation on unmount; changed the
  mutation to root-scope lifetime, with account/target guards. Live retest passed:
  native catalog search found `ohneheart`, Enter added it, Slack showed it, native
  thread root showed it too, and removing it from the thread removed it in Slack.
  Automation temporarily timed out/no window; restarting the test app restored it.
- Read-only live `emoji.list` probe succeeded with 64,135 custom entries,
  including `ohneheart`. Slack's own search uses `emojis/search` with query/count;
  observed 26 results for `partyparrot`. No credentials were emitted or saved.
- In an isolated browser page, old scroll observer scheduled 90 callbacks over
  1.5 seconds idle; new observer scheduled zero. Scroll resumed sampling and
  stopped after the burst. This measures the observer, not whole-app CPU/startup.
- Core suite after catalog/alias/sidebar changes: 266 passed. Node observer
  suite: three passed. Desktop suite after sidebar optimization: 111 passed.
  Formatting and diff whitespace checks passed.
- Geist native captures inspected at `tmp/agent-ui-geist/`: conversation,
  composer, profile/thread, DMs, settings, alternate appearance, and reaction
  picker. The catalog fixture at `tmp/agent-ui-catalog/reaction-picker.png` was
  also rendered and visually inspected; the live custom reaction check passed.
- `/Users/sh/Documents/GitHub/multiSlack` is an additional read-only reference.
  Its demo handlers are useful examples, not proof of live Slack API behavior.
- Later reference round trip: Slack `saved.add` and `saved.delete` both succeeded
  for the private test message; it is no longer saved. `saved.list` uses
  `limit=15`, `filter=saved`, `include_tombstones=true` and cursor pagination.
  Response has `saved_items`, `counts`, `response_metadata.next_cursor`; each item
  includes item_id/type, ts, date_created/due/completed/updated/snoozed_until,
  is_archived, state, todo_state. A live active item had `state=in_progress`,
  `todo_state=saved`, false is_archived, and zero due/completed/snoozed dates.
  Counts include uncompleted_count, uncompleted_overdue_count, archived_count,
  completed_count, total_count. Add/update/reminder schemas were subsequently
  captured through the private test items before implementing them.
- A process sample caught realtime projection in `channel_recency/latest_ts`.
  `examples/sidebar_benchmark.rs` reproduces projection with 1,000 synthetic
  channels and 200,000 messages, 25 runs with unchanged ordering assertions.
  Baseline median/p95 ms: alphabetical 110.271/151.803; recent 100.717/123.220;
  priority 85.721/162.800. Fix computes recency only for recent sort and borrows
  timestamps while scanning, cloning only the selected result. After medians
  in the first run: alphabetical 0.453 ms, recent 171.419 ms, priority 1.040 ms.
  Repeated alphabetical medians were 1.218, 1.022, 1.361 ms; priority 1.056 and
  1.121, 1.456 ms in the repeats. Recent medians 90.018, 111.957, 150.359 ms confirm
  substantial timing noise and no reliable recent-sort speedup. Alphabetical
  and priority no longer scan histories and show a large repeatable improvement.
  This is synthetic projection timing, not whole-app latency.
- Latest source passed `cargo check --locked`, formatting and whitespace checks;
  signed debug bundle was rebuilt and relaunched in the private test channel.

## Later verification, 01:31 local

- Native complete/restore, one-hour reminder/clear, archive/unarchive on the
  private parity root were independently observed in Slack. Completed pagination
  loaded 50, then 50, then the remaining four items without modifying unrelated
  items. Warm navigation opened the private saved message beside Later and the
  target was visible in a native screenshot.
- A first early-startup navigation showed older history. Added cold-anchor and
  out-of-window target regressions; cold native retest remains required.
- Standalone native reminder creation preserved Unicode and two lines in Slack,
  with the correct one-hour due time. Both personal test reminders were marked
  complete so they will not notify later. One was created in Slack for protocol
  discovery and one in Super Platinum for the round trip.
- `saved.add` creates a reminder with item_type=reminder, rich-text description,
  UUID client_id and date_due. Existing descriptions are preserved as blocks;
  native description editing and basic rich-text controls were verified below.
- Captured `saved_added`, `saved_updated`, `saved_deleted` WebSocket frames from
  the private root. They contain saved, counts, event_ts. The reducer rejects
  replayed frames and stale connection/workspace events, invalidates older pages,
  updates counts/items immediately, and hydrates new visible message previews.
  Reconnect refreshes Later. Live add and delete in Slack immediately updated
  native Later's row, preview, and count without any refresh. Native toolbar
  Save/Remove both round-tripped to Slack. The private root was left unsaved.
- Native captures read: `tmp/agent-ui-later/{later,later-empty,later-error,
  later-reminder}.png` and `tmp/agent-ui-reminder/{later-new-reminder,
  later-reminder}.png`. Current-time input initialization and multiline creation
  layout are verified. Latest locked desktop suite: 114 passed; independently
  locked core suite: 273 passed. Signed debug bundle rebuilt and relaunched with
  the agent socket for targeted diagnostics.

## Rich reminder editing and Files, 01:52 local

- Native create/edit reminder dialog now uses a structured rich-text editor with
  bold, italic, and strike controls. Existing links, mentions, emoji, lists,
  quotes, code, and unknown blocks survive adjacent edits. Paste is plain text;
  five real-browser checks cover round trips, Unicode selections, multiline
  input, clipboard handling, and focus containment/restoration.
- On the completed private personal test reminder, native bold formatting of
  only the first line appeared in Slack. A second native edit replaced the
  second line while preserving that bold first line and completed status.
  Neither operation scheduled a notification. The native modal screenshot at
  `tmp/agent-ui-rich-reminder/later-new-reminder.png` was inspected.
- Later previews show multiple lines and merge realtime message edits/deletes.
  A pending history response cannot restore content from before an edit/delete;
  the regression also covers delayed edits after a tombstone. This race is
  unit-verified; live message edit/delete preview verification remains open.
- Files reference: `search.modules.files` returns files directly in `items`
  with page/page_count/per_page/total_count pagination. Page size is 50. Sorts
  observed are last_engaged, timestamp, score. Ownership uses
  `creator:<@USER>` or shared_with_you=1. File type filters are query terms.
- New native Files surface has explicit search submission, type/ownership/sort
  controls, bounded pages, result counts, error/retry/empty states, thumbnails,
  and native viewer opening. Canvases/lists currently open their Slack link;
  native editing is not implemented. Stale query/account responses are rejected.
- Thumbnail credential routing now parses HTTPS hostnames and checks Slack
  domain boundaries, rejecting lookalike hosts, paths, userinfo, and HTTP.
- One diagnostic request capture incorrectly treated multipart input as a URL
  form and exposed authentication fields to tool output. Subsequent captures use
  positive field allowlists and return no authentication values. No credentials
  are recorded in this ledger or source fixtures.
- Core tests after Files: 278 passed. Desktop suite before the final Files
  navigation/thumbnail hardening refinements: 114 passed. The first native Files
  captures exposed a grid-span bug (unused right column), now corrected; a
  refreshed full-width, empty, error, minimum-size (668×400), and viewer captures
  were rendered and inspected at `tmp/agent-ui-files/`.

## Files live verification, 02:02 local

- Empty query returns `no_query`; a read-only probe confirmed `*` works for an
  unfiltered browser. Initial All types load now succeeds with no error.
- Native image filter/pagination loaded page one and page two. Slack's
  last_engaged index ignored `in:echotestchamber` (998 images), while timestamp
  and score returned 26. Text and ownership filters now force a compatible sort,
  and Recently viewed is unavailable for those filtered searches.
- Uploaded `super-platinum-private-file-check.png` (a 326-byte synthetic checker)
  from native Home in private `#echotestchamber`; Slack independently showed it.
  Native Files searched the exact name with `in:echotestchamber`, returned one
  result, and rendered the same checker in its viewer. No write testing outside
  the private channel occurred.
- The old viewer download anchor did nothing in WKWebView. Replaced it with a
  native asynchronous Save dialog using the already-transitive rfd 0.17.2,
  sanitized suggested filenames, cache-held original bytes, background disk
  writes, and cancel/error/success handling. This fixes the shared viewer in
  conversations as well as Files.
- Live Save dialog wrote `tmp/private-file-check/super-platinum-private-file-check-downloaded.png`.
  It was byte-identical to the 326-byte source; SHA-256
  `3d4736ba240cc0331a2fffa14e6a1ff662a11d4c5ef708ce886463e3256df1c1`.
- Ownership probe found creator:<@USER> excludes ordinary image uploads;
  from:<@USER> includes the private test upload. Native ownership now uses from
  for uploads and creator for explicit canvas/list searches. Final native retest
  passed: Created by you + the exact private search returned one item,
  F0C8L3T7CLC, with no error.
- Latest desktop tests after native download: 114 passed. Core: 278 passed
  including the final ownership modifier refinement. Formatting and whitespace
  checks passed. The latest signed debug bundle is running with the agent socket.
- Temporary read-only probes printed only aggregate counts, safe API error
  codes, and user identity fields; the probe source was removed afterward.
- Remaining file gaps include channel Files & links, share/rename/delete/copy
  actions, PDF/text preview, large downloads beyond the media cache limit, and
  original-versus-transcoded video download handling. Native canvas/list editing
  still needs its own implementation; an external Slack link is not parity.

## Next verification

Finish Later full saved-index coverage beyond page one,
native cold/out-of-window navigation, and live message preview edit/delete checks.
Continue the remaining message/channel workflows and full parity audit above.
Keep the full objective active; no full-parity claim is justified.

## Channel Files & links and navigation, 02:25 local

- Added conversation Messages / Files & links navigation. Channel files and
  links have independent search, refresh, bounded pages, loading/error/empty
  states, and native source-message navigation. Channel Files reuses the global
  viewer and download path without overwriting global search state.
- Slack reference verified `conversations.searchLinks` with channel_id, query,
  page, sort=timestamp and sort_dir=desc; pages contain 20 links. Some titles
  are null even when URLs are present. The typed parser now accepts those and
  renders the URL as fallback. Scope/query/page generations reject stale results.
- Files uses `in:<#CHANNEL>` and desktop_files_channel_tab. Live private-channel
  load returned 39 files, matching Slack's 36 media + 3 other files. Searching
  super-platinum-private-file-check returned the existing synthetic upload.
- Native Links returned 104 results, matching the Slack reference, and page two
  loaded without error. Searching ft.com returned one result and reset page to
  one. Global Files, channel Files, and Links retained independent query values.
- Source-message navigation was tested on the private channel's older message
  1788956025.252269. Repetition exposed a real history-merge race: an early cached
  scroll consumed the anchor before older rows were merged. Explicit message
  anchors now survive through the fetch and hydration; manual wheel scrolling
  can cancel a pending message jump. The final native retry stayed on the target
  after history increased to 147 messages.
- Switching away from the transcript originally recreated a blank virtual
  spacer on return. The channel tab transition now captures Latest or the center
  visible message before unmount and invalidates queued old-DOM measurements.
  Native latest-message and older-message round trips both returned visible
  messages. This restores the message context, not an exact pixel offset.
- Resource views do not consume new channel unread counts or suppress their
  notifications; mark-visible-read now checks actual transcript visibility.
  A realtime regression covers incoming mentions while resources hide the chat.
  Reconnect refreshes resources without resetting the selected tab; workspace
  changes reset to Messages and invalidate pending conversation work.
- Native fixtures rendered and inspected under tmp/agent-ui-channel-resources:
  channel-files, channel-files-narrow, channel-links, channel-links-narrow,
  channel-links-empty, channel-links-error, and channel. Removed redundant
  headings and changed channel ownership to a compact dropdown so a full row
  remains usable at the 668x400 minimum window size. Live screenshots inspected
  through native UI control; no new Slack writes were needed in this batch.
- Latest gates: 281 core tests, 116 desktop tests, locked check/build, both
  formatting checks, and git diff --check passed. Signed bundle rebuilt and
  running on the real private channel.
- multiSlack's pinned-message component and API helper were read as reference;
  no files in that repository were modified. Pins implementation is next.
- Remaining channel resource gaps: Slack's combined media/files/links overview,
  sort variants for links, realtime resource invalidation, custom channel tabs,
  native canvas/list editing, and thread-reply source resolution. Later-specific
  cold navigation and full saved-index coverage still require their own checks;
  the shared anchor fix alone does not prove those workflows.

## Conversation pins, 02:39 local

- Added a native Pins conversation tab with rich message previews, author/date,
  refresh/retry, empty/error states, bounded rendering (50 then Show more), unpin,
  and source-conversation navigation. Message hover tools now expose pin/unpin;
  pinned messages have a compact Material Symbols Rounded label.
- Added renderer-neutral pins.list/add/remove requests and scoped PinsState.
  Account/channel generations reject late lists. Realtime pin events reject old
  versions and invalidate in-flight lists; delayed mutation acknowledgements do
  not undo a newer event. Full list refresh reconciles pin metadata in cached
  transcript and thread copies. Message edits, deletions, and reactions reach
  the pinned preview; in-flight list overlays preserve those newer changes.
- Fixed overlay lifetime: edit/reaction overlays belong to one pending fetch,
  so an old edit cannot keep overriding later authoritative snapshots.
- Legacy file/file-comment pins remain readable. Thread-reply source navigation
  now anchors the parent before opening its thread; exact older-reply scrolling
  still needs implementation and live verification.
- Slack reference: pins.add accepted channel C0AQDL3V5GU and timestamp
  1791780358.080789. pins.list returned one message with channel, created,
  created_by, message.ts, pinned_to and rich blocks. Event contract checked
  against https://docs.slack.dev/reference/events/pin_added/ and
  https://docs.slack.dev/reference/events/pin_removed/.
- Live private-channel proof on the existing synthetic parity message:
  Slack-created pin appeared in native Pins; native unpin removed it from
  Slack's open Pins tab; native toolbar pin reappeared in Slack immediately;
  Show in conversation returned to the source (pending anchor consumed, target
  in rendered history). Slack unpin and pin both updated the already-open native
  Pins tab without a manual refresh. Native final unpin restored the original
  state, independently checked in Slack. No new messages or files were sent.
- Inspected real native screenshots for pins, pins-narrow (668x400), pins-empty,
  pins-error, and channel-pinned-message under tmp/agent-ui-pins, plus live native
  Pins. The signed bundle was rebuilt and is running with the agent enabled.
- Gates passed: 288 independently locked core tests, 117 desktop tests, locked
  build/check, both formatting checks, and git diff --check.
- Remaining Pins verification: older thread-reply positioning, large lists,
  permission failures and reconnect/offline behavior. Legacy pin rendering is
  parser/unit-covered but has no live legacy-file fixture. Full parity remains
  incomplete; previous channel resources and Later gaps remain outstanding.

## Forwarding, message links, and selection stability, 03:04 local

- Added Forward to message tools, with searchable known conversations, keyboard
  selection, multiple destination chips, a shared rich-text note editor, original
  message/attachment preview, and explicit private-message copy semantics.
  The destination list is bounded to 30 rendered matches. Forward and personal
  reminder creation now use the same rich-editor component.
- Matched Slack's observed chat.shareMessage request: channel is the source,
  timestamp is the source message, share_channel is the destination, and blocks
  carry the note. Each destination retains a client_msg_id for an unchanged
  retry. Writes are not automatically retried; successful destinations are
  removed while failed destinations and the note remain. Work survives the
  originating component closing and checks account, team, and transport identity.
- Private live evidence: Slack reference forward 1791787262.724109 and native
  forward 1791787872.938599 both quote original synthetic root
  1791780358.080789 in C0AQDL3V5GU. The native note's bold text, rocket emoji,
  original quote and thread source URL were independently checked in Slack.
  No sends occurred outside #echotestchamber. Multi-destination/partial-failure
  behavior is implemented but has not yet been exercised against Slack.
- Copy link initially failed because chat.getPermalink rejects this enterprise
  web context. Canonical archive links preserve all six timestamp decimal
  digits and thread_ts/cid. The real imported session has an enterprise host
  but no enterprise_id: host detection now covers that shape, with a regression
  test. In the rebuilt native app, Copy link showed success and pasting into
  the unsent forwarding note matched the original source's exact thread URL.
  The modal was closed without sending the clipboard-test note.
- Live clicking exposed a separate transcript jump: selectstart expanded the
  virtual window by 80 rows on either side, inserting estimated-height history
  above the pointer before native text selection began. Selection now freezes
  the existing rendered neighborhood. A 200-row regression verifies its bounds
  stay unchanged. Native clicking and reopening Forward subsequently retained
  the intended original synthetic message. Drag selection beyond the rendered
  overscan still needs a deliberate scrolling/selection design and verification.
- Native forward, forward-narrow (668x400), forward-error and the refactored
  later-new-reminder fixtures were captured and inspected under
  tmp/agent-ui-forward. Fixed the original narrow-dialog clipping with a
  scrollable body and fixed header/footer. The narrow body interaction still
  needs a keyboard/scroll pass, beyond the inspected initial screenshot.
- Latest suites: 292 independently locked core tests and 119 desktop tests pass.
  The signed app bundle was rebuilt and relaunched with the live agent. Locked cargo check, both
  formatting checks and git diff --check pass. No temporary credential probes
  remain in examples; multiSlack was read only.
- A prior UI diagnostic printed unrelated signed links from the transcript.
  Subsequent UI diagnostics are restricted to the current dialog or exact
  synthetic test labels; link values/credentials are not recorded in this log.
- Remaining forwarding parity: new-person/DM discovery, scheduling, full note
  formatting/mention/emoji controls, live multiple destinations, ambiguous
  delivery retry and permission/offline behavior. Other parity gaps above
  remain open. During this check an older failed image also showed a native
  broken-image glyph; the attachment fallback path needs a dedicated audit.

## Failed image presentation, 03:12 local

- Added a shared native MediaImage component for message images, video posters,
  Block Kit images, unfurl images/icons, custom emoji, reaction/reply faces,
  message avatars, Files thumbnails and the image viewer. Undecodable bytes
  now produce an accessible Image unavailable surface, initials, shortcode,
  or the surface's existing file/icon fallback instead of the WebView error glyph.
- MediaRegistry tracks image decoding failure separately from downloaded bytes.
  Original bytes remain available for Download. An asset revision rejects a late
  decoder callback after newer bytes replace it; replacement data clears the
  failed state. Failure invalidates the current component immediately and other
  copies through the normal media dirty tick. Permanent HTTP failures also leave
  the loading state; transient failures retain existing retry behavior.
- Media fetch logs now contain only a host, opaque local asset ID and failure
  category. Raw transport error strings can embed signed URLs, so they are no
  longer printed in this path.
- Two desktop regressions cover stale decode callbacks, original-byte retention,
  shared failure state and permanent-failure renderer notification. All 121
  desktop tests pass, as do locked check/build, formatting and git diff --check.
  Core code did not change in this batch (previous locked core suite: 292).
- Captured and inspected media-failed, media-loading-state and image-viewer-upload
  under tmp/agent-ui-media-fallback. The failed fixture sends invalid bytes through
  the actual native decoder, with a valid control image alongside them; the
  unavailable panel, avatar initials and emoji shortcode all rendered correctly.
- Rebuilt and relaunched the signed bundle. In the real private channel, an
  unavailable October 2 GIF rendered the fallback instead of the broken-image
  glyph. The existing synthetic private PNG thumbnail and original viewer both
  rendered correctly; the viewer was closed afterward. No Slack writes occurred.
- Remaining media work: remaining standalone avatar/status surfaces should adopt
  the shared component; video decode failures need their own treatment. UI retry
  for expired media URLs and supported-format coverage remain unverified.
- Next concrete navigation failure: while scrolling through the older private
  transcript, the viewport still sometimes jumps backward when the virtual
  window changes or the transcript refreshes. The preceding selectstart fix
  handles its specific click trigger only. Inspect full message-block measurement
  (date separators are currently excluded), spacer estimates, and unconditional
  reset_timeline_window in refresh_from_core; preserve a visible row/offset across
  updates and verify ordinary scroll stability against the live long transcript.

## Timeline anchoring and history navigation

- Refreshes retain the current rendered message identities instead of resetting
  history readers to the newest 100 rows. Full message blocks, including date
  and unread separators, now supply measured heights. Unmeasured rows share a
  stable 66px estimate between virtual spacers and fast-scroll lookup; a jump
  wholly inside a spacer renders its estimated neighborhood directly.
- Added a scoped DOM anchor that keeps visible message identities and offsets
  through virtual-window replacement, spacer changes, late image resizing,
  deletion and history prepend. It follows the bottom only while already there,
  gives user input priority and disconnects its observers after unmount. No
  animation frames run while this anchor is idle.
- Removed the old second scrollHeight correction from load_older. Completion
  now checks account epoch, transport identity, channel and navigation generation
  before clearing the loading flag or hydrating the active surface. Channel and
  workspace navigation release the old loading flag. An old near-top DOM sample
  cannot begin pagination for a newer visit.
- Native verification in #echotestchamber: ordinary forward scrolling and 20-page
  jumps in both directions remained populated. A local sidebar refresh retained
  the same visible position. A subsequent 40-page upward gesture triggered real
  pagination, increasing loaded messages from 149 to 248; the resulting reading
  position remained unchanged in the next screenshot. The rendered window then
  held 51 rows (86..137), rather than all 248. No message/file writes occurred.
- Returning to latest exposed another race: scrolling could report success while
  old virtual rows were still mounted. Navigation now waits, for at most eight
  frames, for the captured conversation scope and actual target row. A missing
  target leaves the reading position alone. Intervening user input cancels the
  queued request. The app effect and first-measurement path use this same helper;
  stale effects cannot consume another navigation's pending target. Explicit
  message targets survive until their surrounding history fetch completes.
- In the latest signed bundle, explicit channel reopening twice returned from
  history to the synthetic newest messages, with stick_to_bottom=true and no
  pending anchor. Startup itself still sometimes opens older history before an
  explicit channel open; warm-start anchoring remains an unresolved next check.
- Browser evidence: nine timeline-anchor checks and three navigation checks pass.
  The earlier no-anchor control shifted a visible row by 40px; the anchor checks
  require less than 1px drift. The three existing scroll-performance Node tests
  also pass. Desktop suite: 125 tests; locked check/build, formatting and
  git diff --check pass. Core code was unchanged in this batch (prior core suite:
  292). Native variable-height/channel/unread fixtures were inspected under
  tmp/agent-ui-timeline-anchor earlier in this batch.
- Remaining navigation coverage includes warm-start anchoring, cold Later/Pins
  targets and exact older thread-reply positions, selection beyond overscan,
  and live delayed-response/account-switch races. Full parity remains incomplete.

## Startup anchor follow-up

- Integrated the navigation and reading-anchor browser harnesses. This reproduced
  a failure that their separate checks missed: a successful latest/message jump
  could be undone by a ResizeObserver before the ensuing scroll event. Navigation
  now captures its new anchor synchronously after moving the viewport.
- A second regression dispatches a layout-induced scroll before resize delivery.
  Previously this dropped bottom following; the anchor now compares content
  height and restores its existing reading position before accepting that event.
  Wheel input retains priority. Both regressions failed before their respective
  fixes and pass afterward. Ten anchor checks and four integrated navigation
  checks currently pass, as do the three scroll-performance Node tests.
- Two rebuilt native startup checks now retained the newest synthetic private
  messages through 30 seconds of background history/media hydration. The second
  run held stick_to_bottom=true and no pending anchor throughout the loaded
  portion of the check; screenshots confirmed the newest messages afterward.
- The first check exposed repeated 18-to-100-row expansions on unrelated core
  refreshes. Bottom readers now keep their existing measured neighborhood,
  including new arrivals but bounding large merges to 100 rows. A regression
  covers unchanged refresh, one arrival and a 300-message merge. The second
  native check held 18 rows (182..200 of 200) through startup and a controlled
  sidebar collapse/expand refresh; the sidebar was restored afterward.
- Current verification: 126 desktop tests, locked check and signed bundle build,
  formatting and git diff --check passed. The running bundle includes these
  changes. Core code was unchanged. No Slack message/file writes occurred.
- The reproduced warm-start/late-media jump is fixed in these live checks.
  Remaining navigation work includes cold saved/pinned targets, exact thread
  reply positioning, scrollbar-drag/selection coverage and cross-account delayed
  responses. These results do not establish full Slack parity.

## Saved and pinned thread targets

- Thread opening now carries an optional exact reply timestamp. Saved and pinned
  replies anchor the channel on their parent and open the selected reply in the
  thread pane. Targeted reply requests start inclusively at that timestamp;
  opening an older target does not mark all newer replies read. Missing replies
  and failed loads have a retryable thread notice.
- Thread visits have their own generation and scoped DOM navigation. Closing,
  reopening, switching accounts or replacing the transport invalidates old work.
  User scrolling cancels the pending target. Profile opening now uses the same
  thread-close transition so target/loading/error state is cleared consistently.
- Native fixtures under tmp/agent-ui-thread-target cover a 40-message thread
  positioned at reply 7, a failed thread load and the existing unread divider.
  These were captured and inspected in the preceding implementation batch.
- Live #echotestchamber checks opened the synthetic saved reply from Later and
  the same pinned reply through Show in conversation. The Pins check confirmed
  the correct parent and visible reply, with target consumed, loading=false and
  failed=false. The temporary save was removed in the preceding batch; the pin
  was removed in this batch and the refreshed Pins panel showed No pins yet.
- A follow-up code audit found that missing Later preview metadata was treated
  as a channel message. Later now resolves the selected message and, if needed,
  its conversation before choosing the route. Exact channel/timestamp matching
  avoids opening another message from a batch response. A newer saved-item click,
  surface change, thread change or list generation invalidates pending metadata.
  Failed or missing metadata leaves navigation alone and shows an actionable
  error instead of guessing a channel target.
- Current checks: 130 desktop tests, locked cargo check, formatting and
  git diff --check pass. All ten anchor browser checks, six integrated navigation
  checks and three Node scroll-performance checks pass. No core code changed in
  this batch. Cold routing has focused parsing/cancellation regression coverage;
  an explicitly delayed live cold-preview request remains unverified.
- The signed bundle was rebuilt and restarted successfully. A newly saved
  synthetic private reply opened from Later after restart with the correct
  parent, both messages visible and no pending target or load error. Its preview
  had already hydrated before the click, so this verifies the restarted normal
  route rather than the missing-preview fallback. The temporary save was removed
  afterward; In progress returned from 2 to 1. The app was returned to the private
  channel. No message or file was sent during this batch.
- Remaining navigation work includes live targets outside a 200-reply page,
  thread pagination and resize anchoring, scrollbar-drag/selection coverage and
  delayed cross-account responses. These checks do not establish full parity.

## Thread pagination and reading position

- Threads now expose earlier/newer page controls instead of silently stopping
  after one 200-reply request. Renderer-neutral ThreadHistory tracks independent
  page boundaries, retry state and cursor progress. Repeated/cyclic cursors and
  nonadvancing pages remain retryable rather than spinning or pretending to end.
  Native requests retain the existing account/transport/thread-visit guard.
- A read-only probe in #echotestchamber compared small pages with the complete
  13-message thread. The latest request returned the parent plus reply ranks
  10..12; its cursor returned 7..9. Starting at rank 5 returned 5..7, but combining
  its forward cursor with oldest skipped ranks 8 and 12 on subsequent pages.
  Forward paging now uses an exclusive boundary at the last received reply.
  The corrected bidirectional model recovered all 13 messages in five pages,
  including both skipped boundaries, and exhausted both directions. The probe
  printed only counts, rank indexes and pagination flags; its temporary source
  was removed. No message, file, pin or saved-item writes were made by the probe.
- Cached replies outside an unfilled interval are hidden; the parent remains
  visible. Replies arriving during the current visit, including optimistic sends,
  remain visible after the newer-page control so an unloaded gap stays explicit.
  Empty parent-only pages do not resurrect stale cached replies.
- The existing DOM reading anchor now attaches independently to channel and
  thread containers. Paging holds the current row/offset; later image resizing
  preserves it. Ordinary arrivals still follow the bottom, and closing a thread
  does not dispose the channel anchor. Native paging/error/target/unread fixtures
  were inspected under tmp/agent-ui-thread-paging.
- Browser checks: ten channel-anchor, six navigation and six thread-anchor tests
  pass. Three Node scroll-performance tests also pass. The thread harness covers
  earlier and newer insertions, media resize, manual paging at the bottom,
  ordinary arrivals and channel isolation. The live small-page probe establishes
  API/model completeness for that thread, not a native 200-plus-reply UI run.
- Remaining work includes bounded rendering after loading many thread pages,
  live delayed-response/account-switch tests and a native multi-page thread run
  with more than 200 replies. Full feature parity is still incomplete.
- Final gates pass: 298 core tests, 130 desktop tests, locked cargo check, both
  format checks, git diff --check and signed macOS bundle build. The rebuilt app
  opened the existing private 13-message thread with no pending page or error.
  Opening the synthetic saved reply exposed Load earlier replies; clicking it
  exhausted that boundary, retained both messages and left no loading/error state.
  The temporary save was removed and In progress returned to 1. The app is back
  in #echotestchamber. No message or file was sent in this batch.

## Compact thread groups and accessible message actions

- The real Slack private thread renders its parent and first reply with author
  headers, then eleven adjacent replies with compact gutters. Its measured DOM
  rows were 64px for the parent, 52px for the first reply and 30px for compact
  replies. Super Platinum previously forced every thread message to expand.
- A shared thread projection now applies the existing same-author/five-minute
  grouping rule. The parent, first reply, changed authors, time gaps and the first
  live reply below an unloaded page gap retain headers. Both thread refresh paths
  use this projection, without cloning the complete raw message bag first.
- Pending sends now read pending state from their thread bag rather than scanning
  channel bags. They retain their full Sending/Uploading status until confirmed.
  Compact edited messages keep their edited label. Message rows expose their
  accessible author/time and reveal the timestamp and action toolbar on keyboard
  focus, without changing height.
- Rebuilt and inspected thread-grouped, thread-grouped-pending, thread-paging and
  thread-target native fixtures under tmp/agent-ui-thread-grouping. The edited
  label and pending status remain visible. In the signed live app, the same
  private thread contains 13 messages, 11 compact rows, two author headers and
  no load/page error. A compact reply still exposes reaction, save, pin, forward,
  edit and delete actions. No Slack message/file/save/pin writes occurred.
- Verification: 132 desktop tests, locked check, formatting, git diff --check
  and signed bundle build pass. Three focused browser checks verify keyboard
  toolbar/timestamp visibility, stable row height and retained edited/author
  metadata. Core code was unchanged in this batch (previous suite: 298).
- Remaining thread work still includes bounded rendering after many pages,
  native 200-plus-reply pagination and delayed response/account-switch coverage.
  Full Slack feature parity remains incomplete.

## Scheduled messages: native creation and queue management

- Added a clock action to channel and thread composers, a rich-text scheduling
  dialog with a tomorrow-at-09:00 preset and local custom date/time, and a
  Scheduled sidebar surface. The queue supports refresh, loading more active
  drafts, rich previews, editing/rescheduling in place, and confirmed deletion.
  The dialog names the destination and preserves existing files and thread data.
- A live check rejected the public chat scheduling methods with
  `not_allowed_token_type`. The read-only multiSlack reference documents Slack's
  session-token `drafts.*` flow. Verified the real web client's `drafts.list`
  request uses `is_active=true` and `limit=25`; the native queue uses that active
  filter with an initial limit of 100. Sent, deleted and unscheduled drafts are
  excluded. Increasing the requested limit loads more; large active queues and
  server-side limit caps still need live coverage.
- `drafts.create` and `drafts.update` retain rich blocks, file IDs and complete
  destinations, including thread timestamps. Updates are atomic instead of
  deleting/recreating the old schedule. Mutation timestamps use current time;
  writes do not retry automatically. Queue generations prevent a stale list
  response from resurrecting a completed deletion, and account/transport guards
  keep completions out of replacement sessions.
- Scheduling files shares the existing upload transfer stage, then finalizes
  files without a channel. They are attached to the draft without creating an
  immediate channel message. A failed scheduling request retains the composed
  content and finalized IDs for retry while the dialog stays open. Successful
  scheduling clears only unchanged original composer text and original
  attachments; newer text/files and another thread's draft survive.
- Live API verification in #echotestchamber created, listed, updated and deleted
  a rich scheduled thread reply with the existing synthetic file attached.
  The update retained its ID, blocks, destination and file ID. Both the browser
  reference schedule and API test were cleaned up.
- In the signed native app, scheduled a bold Unicode test plus a newly uploaded
  synthetic PNG for tomorrow 09:00. Slack's own Scheduled screen showed the
  content and attachment. Native editing appended text and moved it to 10:00;
  Slack confirmed both changes with bold formatting and the file retained.
  Native deletion removed the schedule. Final API verification found zero
  pending schedules, the original three active drafts, and no synthetic draft.
  The newly uploaded private test file was deleted separately. The temporary
  probe source was removed; sanitized results remain in tmp/scheduled-*.log.
- Inspected four native captures in tmp/agent-ui-scheduled: populated, empty,
  load-error and edit-dialog states. Also inspected the signed app's compose
  dialog live. It remains usable above the composer with an explicit destination,
  attachment count and local send time. Shared editor focus containment applies.
- Remaining scheduling work: send now, cancel-to-draft and the full drafts
  surface; attachment previews/removal within the editor; persistent upload
  recovery after closing a failed dialog; queue realtime updates, remote-edit
  conflict coverage, large-queue completeness and a native scheduled delivery
  timing test. Full Slack feature parity is still incomplete.
- Final gates pass: 135 desktop tests, 303 independently locked core tests,
  locked cargo check, both format checks, git diff --check and the signed macOS
  bundle build. No commits, pushes or deployments were made.

## Draft lifecycle, immediate sending, and concurrent edits

- The queue now retains all active drafts and provides Drafts and Scheduled
  tabs. Drafts sort by the latest edit; schedules sort by send time. Both support
  rich previews, editing and deletion. The compose dialog can save without a
  schedule, schedule an existing draft, or cancel a schedule in place. Existing
  rich blocks, file IDs and thread destinations survive these transitions.
- Verified the web client's actual immediate-send requests: text uses
  chat.postMessage with draft_id; attached drafts use files.share with draft_id
  and comma-separated file IDs. Slack retires the draft atomically. Native
  sending follows these shapes, requires a message timestamp receipt, retains a
  stable client_msg_id for retry, and never automatically retries the write.
  Cancellation needs the full draft payload with date_scheduled=0; a minimal
  time-only update was rejected live. Mutation timestamps now use milliseconds
  as observed in the web client.
- Before editing or sending, fetch the active server draft and compare its
  revision. A conflict preserves the open editor and replaces the queue's cached
  revision so reopening shows the current server content. That replacement also
  invalidates older in-flight list responses. Missing/deleted/sent drafts and
  incomplete loaded queues return explicit errors. This is a preflight check;
  it does not prove server-side compare-and-swap for a change during the write.
- Native live verification in #echotestchamber saved a rich draft with its file,
  scheduled it, canceled it back to drafts, and sent it immediately. The real
  Slack browser displayed the resulting message and synthetic attachment. An
  external edit during an open native editor was rejected while preserving its
  text; closing then showed the remote revision without a manual refresh.
- Native cold-launch testing exposed an intermittent first-save failure:
  Eval::recv returned Finished. In the pinned Dioxus desktop bridge, JavaScript
  finalization disposes the eval owner. The scheduling snapshot now keeps its
  channel alive with an acknowledgement until Rust consumes the result. Two
  separate fresh launches then saved edited rich drafts on the first attempt.
  The final native text draft was sent to the original private test thread;
  conversations.replies confirmed exactly one matching reply, the original
  thread timestamp, and retained italic formatting. The other one-shot eval
  consumers still merit an audit for this same lifetime risk.
- API roundtrips additionally verified cancel-in-place and send-now for both
  text and file thread drafts, with content/destination/file retention and
  automatic draft retirement. Final active count is the original three drafts,
  has_more=false, with no own synthetic draft or pending schedule. Original
  user drafts and the reusable synthetic attachment were preserved. Temporary
  probe source was removed; sanitized evidence is in tmp/draft-lifecycle-*.log.
- Inspected native fixtures for populated/empty drafts, rich draft editing,
  destinationless drafts, send confirmation, and scheduled editing under
  tmp/agent-ui-drafts. Controls remain compact and stable. Missing/multiple
  destinations explicitly disable sending until supported.
- Gates: 135 desktop tests, 308 independently locked core tests, locked check,
  both format checks, git diff --check, signed macOS build, and five browser
  rich-editor checks pass. The rebuilt signed app is running. No commits,
  pushes, deployments, or writes outside the private test channel were made.
- Remaining draft/scheduling work: destination selection and multi-destination
  sending; automatic per-conversation draft persistence; attachment preview and
  removal; persistent failed-upload recovery; realtime queue synchronization;
  large-queue completeness and actual timed-delivery verification. Full Slack
  parity, design review across all surfaces, and broader performance validation
  remain active work.

## Automatic local composer recovery

- Channel and thread composers now bind to separate workspace/channel/thread
  draft keys. Text and local attachment references stay with their destination
  when navigating, closing a thread, or moving between surfaces. The textarea
  remount key includes account and destination; delayed input events are checked
  against that identity. A sent/cleared/scheduled draft cannot reappear from an
  older stored copy, and scheduling after navigation clears only unchanged
  source content rather than the new conversation's composer.
- Drafts are stored incrementally in per-account drafts-UUID.sqlite files,
  separate from the disposable transcript cache. A single background worker
  coalesces pending edits by destination and uses transactions for disk writes.
  Startup loads are ordered behind queued writes; unsaved updates remain queued
  for retry and are merged into an account reload. Storage failures produce a
  visible notice without signing the user out or deleting stored data. Transient
  upload state is excluded from serialization, and attachment IDs resume above
  the highest recovered ID.
- Automatically saved composers appear in the Drafts tab with an On this device
  label, rich previews, attachment counts, Continue writing, and confirmed
  deletion. Continue writing restores the original conversation/thread and
  focuses its composer. Its async navigation survives the Drafts view unmounting
  and checks the current account/navigation generation before opening a thread.
- Failed uploads return their files and caption to the original draft, including
  when it is no longer visible. Newer typed content is preserved. Domain/state
  tests cover this behavior; a forced live upload failure was not exercised.
- Native testing exposed the pinned Dioxus file picker's literal wildcard
  extension filter on macOS: every ordinary file was disabled. Composer attach
  buttons now use the existing rfd async dialog without that filter, retaining
  account/destination guards. Verified the synthetic PNG could be selected and
  appeared as a 326-byte attachment chip. Send now has an explicit primary-action
  CSS class, so adding a clock action no longer gives scheduling its styling.
- Offline native verification kept distinct drafts in #ship and #general and
  restored a thread draft after closing/reopening it. Live #echotestchamber
  verification created separate synthetic channel and thread drafts, restarted
  the signed process, and recovered both. A second restart recovered the channel
  text plus attachment. The Drafts overview displayed both, and Continue writing
  reopened the correct private thread with its text and keyboard focus. Inspected
  the native screenshot of both restored composers and their action styling.
- Confirmed deletion of only the two synthetic local drafts through the native
  overview. The local store then had zero rows; the original three Slack server
  drafts remained present. No test messages were sent or files uploaded in this
  batch. The reference image on disk was preserved. The rebuilt signed app is
  running, with changes uncommitted and undeployed.
- Gates pass: 138 desktop tests, 313 independently locked core tests, locked
  cargo check, both format checks, git diff --check and the signed bundle build.
  Logs are tmp/local-drafts-*.log. The agent screenshot command failed to capture
  this session, so visual inspection used the native computer-use screenshot.
- Remaining: automatic synchronization/merging with Slack server drafts, local
  draft ordering and large-list rendering, upload-in-flight crash recovery,
  missing/moved attachment guidance, and a live account-switch/disk-failure audit.
  This adds local recovery rather than claiming complete cross-client drafts.
  The broader parity/design/performance goal remains incomplete.

## Live server drafts and conflict recovery

- Observed Slack's automatic composer lifecycle in #echotestchamber: typing
  creates a drafts.create request after roughly ten seconds, subsequent edits
  use drafts.update on the same ID, and clearing uses drafts.delete with
  skip_file_deletion=true. The web autosave requests used is_from_composer=false,
  rich-text blocks, destinations, file_ids, a stable client_msg_id on create,
  and a millisecond client_last_updated_ts on update. No credentials or raw
  authenticated captures were written to the repository.
- A controlled replay of an older update to our own synthetic draft returned
  ok=true and replaced the newer synthetic text. client_last_updated_ts is not
  a compare-and-swap guarantee. Existing edit/send checks and the new delete
  check are revision preflights; a remote edit between preflight and mutation
  remains a server race. Do not describe these checks as atomic conflict safety.
- Captured actual draft_create and draft_update frames containing full draft
  objects, and draft_delete containing draft_id/client_msg_id/event_ts without
  a draft object. The socket event timestamp and the draft's last_updated_ts
  use distinct clocks. New scheduled/realtime.rs applies these events directly
  to the server Drafts/Scheduled queue, orders socket events per draft, rejects
  stale payload revisions, and retains deletion tombstones. Changes arriving
  during a list request overlay its response without canceling the load or
  discarding pagination state. Malformed/partial frames cannot replace text,
  destinations, or files with empty defaults.
- Desktop event handling retains the existing workspace and connection-generation
  guards. Draft-only frames avoid rebuilding the conversation projection and
  introduce no polling or per-event API fetch. A reconnect refreshes a previously
  loaded or visible server draft queue; a forced live reconnect was not exercised.
  Late mutation responses cannot resurrect a retired draft or replace a newer
  socket revision with an older HTTP response.
- Delete now fetches the current server revision before mutating. Send and Delete
  confirmations retain the version that was present when confirmation opened,
  including when the underlying row updates in realtime. The shared revision
  check also rejects a stale HTTP preflight when a newer socket revision is
  already known. Conflicts retain the latest queue content and present a review
  notice instead of executing the old action.
- The draft editor preserves unsaved local text when the server version changes.
  On conflict, it offers Save my edits as a separate, unscheduled draft. The copy
  keeps blocks, destinations, thread/broadcast settings, and file IDs, receives
  a distinct stable message identity, and cannot inherit a send schedule. The
  same option is available when the source draft was sent or deleted. Added the
  drafts-conflict fixture to the native UI harness. Copying after remote deletion
  and copying a file-bearing draft were not exercised live in this batch.
- Native verification: browser-created draft appeared without Refresh; subsequent
  edits updated its row; old Delete and Send confirmations were rejected while
  keeping the newer draft. An open editor retained its own unsaved text, and
  Save rejected the remote conflict. Inspected native screenshots of the conflict
  notice and copy option. Save copy created a second unscheduled draft containing
  the native Unicode edits, while the browser composer retained the different
  remote edits. Both versions remained after an explicit server refresh. Browser
  deletion removed a row without Refresh, including while its native editor was
  open. A freshly reviewed native Delete successfully removed only the test copy.
- All synthetic drafts from this batch were cleaned up; the three original Slack
  drafts remain. No messages were sent or files uploaded. The signed app is
  running; work remains uncommitted and undeployed.
- Gates pass: 139 desktop tests, 322 independently locked core tests, locked
  cargo check, both format checks, git diff --check, and the signed macOS build.
  Final logs: tmp/draft-copy-{check,tests,core-tests,build}.log. Earlier protocol,
  realtime, and revision checks have tmp/draft-live-*.log and
  tmp/draft-revision-*.log. Native screenshots were inspected through computer
  use rather than the unavailable agent screenshot command.
- Remaining: local channel/thread composer autosave still saves only on this
  device. Automatically uploading those local drafts, persisting server identity
  and revision across restarts, handling uncertain creates without duplicates,
  and merging/recovering simultaneous edits remain unfinished. The live server
  queue and explicit copy recovery are groundwork for that integration, not full
  bidirectional composer synchronization. Broader parity/design/performance work
  remains active.

## Channel details, members, topic and description

- Channel headers now open a native About/Members dialog with channel type,
  archive state, creation date, ID, topic and description. Values use the typed
  rich-text renderer, including emoji and mentions. The dialog shares the compact
  neutral surface and selection styles; channel-details is in the fixture sweep.
- About fetches authoritative conversations.info metadata before enabling edits.
  Topic and description use conversations.setTopic / conversations.setPurpose,
  including explicit empty values to clear a field. The save preflight compares
  only the edited field and retains local input on remote conflict. This is a
  revision preflight, not atomic compare-and-swap: a remote change after the read
  can still race the write. Reads retry safely; writes do not automatically retry.
- Slack canonicalizes Unicode emoji to shortcode syntax, so successful saves
  reread channel details before the next edit. If that follow-up read fails, the
  submitted value is retained; a later conservative conflict is still possible.
  Metadata merges preserve sidebar preferences, read state and Connect fields.
  Async work checks account/workspace scope; mutation tasks survive modal closure.
- Enterprise conversations.members returned enterprise_is_restricted in the live
  session. Inspected Slack's actual users/list channel-filter request and verified
  Edge marker pagination in read-only probes. The member loader uses Edge when
  enterprise_id is set or the team ID starts with E (this session has only the
  latter), with filter=people. Normal workspaces use conversations.members.
  Hydrated Edge users avoid an extra user lookup; missing REST users are hydrated.
  IDs deduplicate across pages; repeated cursors stop instead of looping. Members
  load on demand, display at most 100 rows at a time, and have loaded-member search.
- Native verification in #echotestchamber: the final Enterprise route loaded John
  and Rowan, matching Slack's two members. Clicking John opened his native profile.
  Inspected the native member dialog screenshot. Escape closed details and returned
  focus to the channel header. With authoritative loading complete, Shift+Tab
  wrapped from Close to Edit Description and Tab wrapped back to Close.
- Earlier in this batch, native topic and description writes were independently
  visible in Slack, including rocket emoji. A browser topic change then caused
  native Save to reject the stale edit while retaining its input. Canceled that
  synthetic edit and restored both fields to their original empty values; Slack
  independently confirmed both empty. Four metadata system messages remain in
  the private test channel. No explicit chat messages or files were sent in this
  channel-details batch. Original server drafts and existing files were preserved.
- Follow-up polish retains unsaved edits across About/Members switching, disables
  starting another field edit until the current edit is finished, focuses the
  editor when opened, and adds cached member avatars with initials fallbacks.
  Native verification confirmed the Unicode edit survived About/Members/About,
  the other Edit button was disabled, and editor focus was automatic. Canceled
  the synthetic edit without saving; both fields stayed empty. Inspected settled
  native About and Members screenshots, including avatars and aligned actions.
  The final signed build returns focus to About after Cancel; Escape then closes
  details and focuses the channel header. Successful Save uses the same focus
  helper but was not repeated after that final keyboard-only change. The rebuilt
  client is running on Home/#echotestchamber with an empty composer.
- Verified gates: 139 desktop tests, 323 independently
  locked core tests, locked cargo check, both format checks, git diff --check,
  and signed macOS build. Final UI logs: tmp/channel-details-polish-tests.log,
  tmp/channel-details-polish-check.log, tmp/channel-details-polish-build.log.
  Core log: tmp/channel-details-final-core-tests.log.
- Remaining: channel creation/join/leave, rename/archive, invite/remove members,
  permissions/settings, full-server member search, large-channel live pagination,
  account-switch and offline/permission failure checks. Full Slack parity and the
  broader design/performance goal remain unfinished.


## Channel creation, rename and archive lifecycle

- Added a compact Create channel dialog beside the workspace heading. It previews
  the normalized name, shows the destination workspace, and explicitly offers
  Private (initial selection) or Public with visibility descriptions. It opens the
  returned channel after success if the originating account and navigation are
  still current. The channel-create fixture is included in the native sweep.
- Live Enterprise discovery: the signed-in session uses E09V59WQY1E, while Slack's
  create request sends team_id=T0266FRGM. The native form fetches the signed-in
  user's enterprise_user.teams and resolves those with team.info. It never infers
  a creation workspace from an arbitrary Slack Connect channel. Multiple teams
  require a selection; normal T-prefixed sessions use their own workspace.
- Slack's web create request included name, validate_name=true, is_private, and
  team_id. The inspection initially aborted a request, but Slack automatically
  retried it after the route was removed and created the authorized private test
  channel C0C8MRDCCMS (super-platinum-channel-check-20261012). This was detected
  from the network response and UI; it is included in cleanup, not treated as a
  read-only inspection. No credentials were printed or saved in repo captures.
- Added typed core create/archive/unarchive requests and naming validation. Reads
  can retry; create, rename, archive and unarchive never automatically retry.
  Name edits share the channel details preflight and preserve input on conflicts.
  Names render as literal text rather than message formatting; server canonical
  names are reread after success. The existing remote-change race between read
  and write remains; these checks are not atomic compare-and-swap.
- Archive/unarchive show a named confirmation with the effect before mutating.
  They reread archive state before writing and reject an already-changed state.
  Successful changes refresh metadata, sidebar projection and persisted cache.
  Archived channels hide both composers and block both native send entry points
  and optimistic queues, preserving existing text. Known nonmembership similarly
  blocks sends. Joining is still a Slack handoff until native joining is added.
- Live native round trip: created private C0C8CT6K6J1 as
  super-platinum-native-admin-20261012, then renamed it to
  super-platinum-renamed-admin-20261012. Slack's browser independently showed the
  same ID/name. Native Archive produced read-only guidance and disabled metadata
  editors; reopening that ID in Slack showed its archived banner. Native Unarchive
  restored the composer; Slack independently removed the archived banner and
  showed the unarchive system event. No chat text or files were sent in these
  test channels; channel lifecycle actions generated Slack system messages.
- Inspected the native creation screenshot with Private selected and Hack Club
  loaded. Native Save returned keyboard focus to About after rename. Final
  confirmation-focus checks passed in the final signed build: opening archive
  focuses Cancel, canceling returns to About, and successful archive also returns
  to About. Restart retained the renamed channel. Dialogs now reserve vertical
  margins and scroll internally; disabled controls have a visible dimmed state.
  Inspected the final signed build's creation screenshot after those CSS changes;
  Escape closes it and returns focus to Create channel. No test input remains.
- Cleanup: both C0C8CT6K6J1 and C0C8MRDCCMS were archived through the native
  confirmation flow, then independently reopened in Slack to verify their
  archived banners. Both remain recoverable. Returned both clients to
  #echotestchamber. Original drafts/files and that channel's name/topic/description
  were untouched in this batch.
- Gates: 140 desktop tests, 326 independently locked core tests, locked
  cargo check, both format checks, git diff --check and signed macOS build.
  Logs: tmp/channel-admin-{check,tests,core-tests,build}.log. Final UI polish
  validation uses tmp/channel-admin-final-*.log.
- API references: [create](https://docs.slack.dev/reference/methods/conversations.create/),
  [rename](https://docs.slack.dev/reference/methods/conversations.rename/),
  [archive](https://docs.slack.dev/reference/methods/conversations.archive/),
  [unarchive](https://docs.slack.dev/reference/methods/conversations.unarchive/).
  multiSlack channelAdmin.ts was a read-only naming/API reference.
- Remaining: native join/leave, invitations/member removal, more settings and
  permissions, archived-channel discovery, external-client lifecycle realtime
  synchronization, public-create live checks, multi-workspace creation and offline
  uncertain-outcome recovery. Full parity/design/performance remains active.

### Live channel metadata synchronization — 2026-10-12

- Captured private-channel `group_rename`, `group_archive`, `group_unarchive`
  frames and the accompanying `channel_name`, `channel_topic`,
  `channel_purpose`, `channel_archive`, `channel_unarchive` system messages in
  the existing disposable private channel. Topic and purpose can arrive solely
  as message subtypes; ignoring those kept an open native details dialog stale.
- Added a core partial-update reducer with independent timestamps for name,
  topic, purpose and archived state. Older/duplicate field events and malformed
  values are ignored. Sparse events cannot invent unknown channels or replace
  membership, privacy, read state, stars or Slack Connect metadata.
- Details reads and generic-info refreshes capture a revision before requesting
  data, then preserve socket updates received while the request was in flight.
  Later authoritative reads can reconcile missed events. The reconnect path
  now refetches open-channel details with account and connection-generation
  guards. Metadata changes use the existing asynchronous cache persistence.
- An already-open details dialog observes per-channel revisions without fetching
  on every event or replacing typed editor text. A remote change to the edited
  field displays a conflict notice; save preflight refuses to overwrite it.
  A remote archive disables saving and both composer send paths immediately.
- Live signed-bundle checks: browser rename updated the native header, composer
  placeholder and open details heading. Browser description clearing changed
  the open native value to Not set. Browser topic changes preserved exact
  native unsaved text; the attempted save was refused and Slack retained the
  remote value. Native typing after refusal appended to the same editor,
  verifying restored focus. Inspected the final focused-editor screenshot.
  Browser archive disabled native Save and replaced the composer with archived
  guidance; browser unarchive restored controls without losing the edit.
- Fixed duplicate conflict notices and moved focus restoration into the mounted
  component's post-render effect so it runs after controls are re-enabled or
  replaced. The detached mutation task only requests focus while its signals
  remain alive.
- Restored `C0C8CT6K6J1` to `super-platinum-renamed-admin-20261012`, cleared the
  temporary topic/description and archived it again. The second disposable
  channel remains archived. No explicit chat messages or uploads in this batch;
  private channel administration generated Slack's system messages. Removed
  both temporary browser socket listeners and reloaded #echotestchamber.
- Gates: 330 independently locked core tests; 141 desktop tests; both format
  checks; git diff --check; signed macOS builds. Logs under
  `tmp/channel-live-{core-tests,desktop-tests,final-tests,build}.log` and
  `tmp/channel-live-focus*-build.log`. On the last signed build, opened the
  #echotestchamber topic editor without changing its content, canceled, and
  verified Escape closed details afterward. Both clients finished at
  #echotestchamber with the native composer empty and details closed.
- Limits: this does not implement realtime create/join/membership discovery or
  prove forced-outage recovery live. Ordering protection covers socket field
  clocks and reads in flight, not a server-provided transactional version for
  every HTTP snapshot. Broader boot/account-replacement races and metadata
  hydration paths remain an audit item. Full parity/design/performance is active.

### System-message text contrast — 2026-10-12

- Matched the supplied stock Slack reference: channel activity bodies use the
  existing `--muted` color; author names retain their normal emphasis. The
  shared timeline/thread renderer applies the style from an explicit subtype
  allowlist, covering public/private channel activity and pin notices. Ordinary
  chat, bot posts, files, and broadcast replies keep the normal body color.
  Subtype reference: <https://docs.slack.dev/reference/events/message/>.
- Added a regression test for activity versus authored-message subtypes and a
  native `system-messages` fixture. Inspected
  `tmp/system-messages-ui/system-messages.png`: gray activity, bright chat/app
  text, and a readable wrapped archive notice.
- Validation: 143 desktop tests passed (`tmp/system-message-tests.log`),
  `cargo fmt --check`, `git diff --check`, and native fixture capture.

### Realtime channel discovery — 2026-10-12

- Reproduced missing discovery with a new private channel in Slack:
  `C0C8JGDM202` / `super-platinum-discovery-before-20261012`. Slack sent an
  unstamped `channel_joined` object followed by timestamped
  `member_joined_channel`; the native channel count stayed at 2,120.
- Self-join hints now schedule a deduplicated authoritative details read.
  Reads are scoped to account, transport, workspace, and socket generation,
  while allowing the user to navigate elsewhere. Pending discovery retains
  rename/archive/membership field clocks even before the channel is cached,
  so an older response cannot resurrect a channel left during the fetch.
  Unrelated channel-created/other-user join events do not invent membership.
  References: <https://docs.slack.dev/reference/events/channel_joined/> and
  <https://docs.slack.dev/reference/events/member_joined_channel/>.
- Live signed-bundle retest: Slack-created private `C0C8NCPNE5S` /
  `super-platinum-discovery-after-20261012` increased the native channel count
  to 2,121 without a refresh or restart. The native quick switcher found its
  exact name with a lock icon; opening it loaded its join message and enabled
  the composer. Inspected both native screenshots. Browser archiving then
  immediately replaced the native composer with archived-channel guidance.
- Both disposable channels are archived, confirmed by their server socket
  archive events. No explicit chat messages, invitations, or uploads. Removed
  the temporary socket observer and restored both clients to #echotestchamber.
- Gates: 337 independently locked core tests, 143 desktop tests, locked Cargo
  check, both format checks, diff whitespace check, and signed macOS build.
  Logs: `tmp/channel-discovery-{core-tests,tests,check,build}.log`.
- Additional focused native orchestration test passed for stale socket,
  account/transport replacement, deduplication, and background-workspace
  navigation (`tmp/channel-discovery-scope-tests.log`). The first test run
  lacked a synthetic session in its fixture; supplying that isolated session
  fixed the harness. No production change was needed after the live retest.
- Limits at this checkpoint: reconnect recovery was not yet implemented.
  The later reconnect inventory entry below supersedes that gap; external
  nonsingleton join/leave remains unproven.


### Channel member administration — 2026-10-12

- Members now has a compact Add people picker with debounced workspace search,
  name/handle results, removable selections, private-history disclosure, and
  per-person results. Removal has an inline confirmation and private/public
  rejoin guidance. Self-removal stays under Leave channel; general-channel
  removal is disabled. Slack remains the authority for workspace policy.
- Invites run one person per request, with a fresh channel preflight and account
  guard. Confirmed successes leave the selection; failed/unattempted people stay
  selected. Uncertain, rate, offline, or global errors stop the batch. Requests
  are not automatically retried. Canonical channel metadata is refreshed after
  mutations. References: <https://docs.slack.dev/reference/methods/conversations.invite/>
  and <https://docs.slack.dev/reference/methods/conversations.kick/>.
- Isolated signed native flow verified: search Jules, select, add, immediate
  three-person list, preview/cancel removal, then confirmed removal back to two
  members with focus restored. Mock HTTP tests verify invite/kick request fields,
  already-present, policy failure, wrong-channel success, and partial error
  responses. These are fixture/mock checks, not successful live memberships.
- Read-only live check in private #echotestchamber: John/Rowan loaded; no self
  removal action; John removal preview showed the private rejoin warning and
  Cancel preserved both members. Add people searched Maya remotely and returned
  real names/handles. No result was selected and no invitation/removal submitted.
  Canceled the picker and closed details; native composer stayed empty.
- Native captures inspected under `tmp/member-admin-ui-final/`, including add
  error and narrow add/remove layouts. At the 668×400 minimum window size, native
  Tab navigation scrolled the removal confirmation and both actions into view.
- Limits: no successful real add/remove membership round trip. Apps/agents and
  cross-workspace invite administration remain outside this people picker. Live
  policy denial, rate limits, and large-channel pagination still need coverage.

### Member-list realtime refresh — 2026-10-12

- Other people's join/leave events now invalidate cached member pages without
  changing our own channel membership. The open Members pane refreshes on those
  events and socket-generation changes; About does no member fetch until needed.
  Refreshes coalesce for 250 ms and wait for pending member mutations. The picker
  stays mounted so query/selections survive a refresh.
- A page fetched before a later membership event or reconnect cannot restore
  stale rows or overwrite the current error state. Failed refreshes retain the
  existing list for retry. Their old pagination cursor is not reused for a new
  roster. Successful refresh starts again at the first page, preserving the
  loaded-member search text. The existing 100-row display limit remains.
- Added core invalidation coverage for other-user joins/leaves, private/public
  system messages, unstamped/out-of-order events, malformed events, and unknown
  channels. Native reducer tests cover stale responses, reconnects, refresh
  failure, and subsequent pagination. The `member-live-refresh` native fixture
  opens with Maya/You, injects a synthetic remote leave, and shows Jules/You
  after authoritative fixture refresh; inspected the actual captured window.
- Gates: 341 independently locked core tests, 148 desktop tests, locked Cargo
  check, plus a focused lifecycle recheck after adding the integration assertion.
  Logs: `tmp/member-refresh-{core-tests,core-scope,tests,final-tests,check}.log`.
  Native captures: `tmp/member-refresh-ui/{member-live-refresh,member-add,member-remove-narrow}.png`.
- Built the signed macOS bundle (`tmp/member-refresh-build.log`) and relaunched
  the real session. #echotestchamber Members loaded John/Rowan; manual Reload
  members returned the same two rows. Inspected the live screenshot, closed
  details, and restored header focus. Browser also remains in #echotestchamber
  with no dialogs. Both format checks and diff whitespace checks pass.
- Limits: event-driven refresh is native-fixture proven, not a live other-person
  membership change. A failed read waits for another event or explicit retry;
  refresh returns large lists to the first server page. No polling loop or
  background fetch is added for closed member panes. Reconnect discovery of
  entirely missing channels remains a separate open gap. Full feature parity,
  design, and performance work remains active.

### Reconnect channel inventory and suspended-frame navigation — 2026-10-12

- A reconnected workspace now fetches `client.counts` and hydrates missing or
  changed channel metadata in batches of 250. Reads retain account, transport,
  socket-generation, and channel-revision guards, including for background
  workspaces. Failures preserve cached data and use the safe error presentation.
- Counts no longer advance cached metadata versions before full details arrive.
  Private-only membership inventories reconcile correctly, archived member
  channels can receive unarchive updates, and membership events newer than a
  counts request survive its response. Count-confirmed membership also protects
  against older in-flight detail responses.
- Core regressions cover missed create/rename/unarchive, private-only counts,
  stale leave/join responses and unread state, older detail responses, and
  501-channel batching. 345 independently locked core tests passed:
  `tmp/reconnect-channel-core-tests.log`.
- Live socket outage: a temporary loopback HTTP CONNECT proxy passed encrypted
  traffic unchanged and interrupted only the native Slack websocket. While
  native was disconnected, browser Slack created singleton private channel
  `C0C9D9ETVA4` (`super-platinum-reconnect-20261012`). Native retained 2,120
  channels and could not find it. Unblocking recovered 2,121 channels; the
  switcher found the private channel and its history/composer opened without
  restarting. A second interruption covered a browser rename to
  `super-platinum-reconnect-renamed-20261012`; native recovered the new name
  while its selected conversation remained #echotestchamber. These checks
  prove background inventory recovery rather than only active-channel reads.
- The live switch exposed another bug: navigation awaited animation frames
  indefinitely while WebKit reported `document.hidden = true`. This also held
  up the history refresh behind that await. Navigation now races each frame
  with a 50 ms timer, retains target/scope and user-input checks, and awaits
  the evaluation result. Both channel and thread navigation use this path.
  Latest navigation keeps its target mounted until acknowledgement and clears
  the pending state after success; equal-length history changes also retrigger
  when the newest message ID changes.
- Temporary native diagnostics confirmed hidden-window state and were removed.
  The signed `timeline-latest-opening` fixture visibly reached row 359 and
  cleared its pending target. After the final timer change, two live short-to-
  long channel switches reached the latest #echotestchamber rows with
  `pending_scroll_to = null` and a 180..200 mounted window. Inspected the native
  screenshots, including gray system activity beside bright authored messages.
- Seven Playwright navigation checks passed, including deliberately suspended
  animation frames, stale scope, explicit targets, input cancellation, and
  thread/channel isolation. 149 desktop tests, both Cargo format checks,
  `git diff --check`, and the signed macOS build passed. Latest logs:
  `tmp/navigation-fallback-{tests,build}.log`.
- Cleanup: the singleton test channel was archived, briefly unarchived for
  navigation verification, then archived again. No explicit messages or
  invitations were sent. Both clients returned to #echotestchamber. The proxy
  processes were stopped and the app relaunched without proxy or fixture
  variables; no system proxy settings changed.
- Limits: the controlled outage interrupted the websocket, not the entire
  machine's network. Full-network recovery, account replacement during a live
  recovery read, missed nonsingleton membership changes, and permission-error
  recovery still need separate live coverage.

### Native attention and automatic read marks — 2026-10-12

- Realtime message counting now requires the selected transcript to be focused,
  at its latest messages, and finished navigating before treating an incoming
  message as viewed. Background windows, resource panels, pending navigation,
  and readers in older history retain incoming unread/mention counts.
- Automatic channel read calls require native focus. Initial conversation
  opening retains the existing mark-on-open behavior; subsequent history
  refreshes require the reader to be at the bottom. Returning native focus
  acknowledges the visible latest conversation and eligible open thread.
  Thread read calls also check the current channel/root, loaded state, bottom
  position, and absence of a pending older-reply target.
- Automatic channel/thread responses are guarded by account epoch and transport
  identity. Renderer-neutral read acknowledgement bookkeeping rejects changed
  cursors, including a later manual unread mark, and preserves unread/mention/
  VIP counters when newer messages arrived while the HTTP request was pending.
  In that case counts remain conservative until the authoritative server mark
  reconciles them, rather than clearing unseen arrivals.
- Added read-only agent diagnostics for native focus, eligibility, current read
  cursor, and active-channel unread/mention counts. Realtime tests moved into
  their own module so the implementation stays below 600 lines.
- Private live check: browser Slack marked the 04:58 test message unread in
  #echotestchamber. Native, hidden and unfocused, received cursor
  `1791794800.210959` with one unread. Reopening/refetching the same native
  conversation finished navigation but preserved that cursor and count.
  Native focus return advanced to `1791799527.003389` and zero unreads. A
  temporary browser websocket observer independently captured both
  `channel_marked` events. Removed that observer after testing.
- Native scrollback check: scrolling up set `stick_to_bottom = false` and
  `reading_latest = false`. Another private manual unread mark remained intact.
  CUA did not reliably restore native focus on that second check, so focused
  scrollback retention is covered by reducer tests, not claimed as live proof.
- Gates: 347 independently locked core tests and 152 desktop tests passed,
  including new acknowledgement-race and attention-state cases. Locked check,
  both format checks, diff whitespace check, and signed macOS build passed.
  Logs: `tmp/read-focus-{core-tests,tests,check,build}.log`.
- Limits: no other person was asked to send messages, so background incoming
  messages and new thread replies have reducer coverage rather than a new
  two-person live round trip. Full Mark as Read preference synchronization,
  explicit mark-unread UI, and comprehensive Activity/read reconciliation are
  still pending. Slack documents the opening preferences at
  <https://slack.com/help/articles/360043037853-Manage-your-Mark-as-Read-preference>.


### 2026-10-12 — Explicit conversation unread state

- Added a Material Symbols “Mark unread” action to channel message toolbars.
  It fetches the preceding message from Slack history before moving the read
  cursor, including a zero-cursor fallback for the first message. Threads retain
  their separate read model; this does not claim thread-unread parity.
- A per-visit hold prevents focus, history refresh, and incoming messages from
  immediately undoing an explicit unread action. A compact status above the
  composer offers “Mark as read”; reopening the conversation ends the hold.
- Explicit requests wait for automatic marks already in flight. Account and
  transport changes invalidate responses; server counts outrank local estimates,
  and a different intervening cursor supersedes delayed HTTP completion. Failed
  requests restore the preceding hold without affecting a newer visit.
- Bulk read acknowledgements now preserve later arrivals and intervening cursors,
  and coordinate with explicit unread requests. This was code-reviewed and
  compile-tested; bulk marking was not run on the user's real workspace.
- Live signed-app proof in private #echotestchamber: marking the existing 04:58
  test message unread set cursor `1791794243.766559`, unread count 1, and kept the
  hold while native focus and bottom position were true. The native “Mark as
  read” button advanced to `1791799527.003389`, count 0, and cleared the hold.
  Repeated the unread action, then reopened the same native channel: the hold
  cleared and Slack again confirmed count 0. A temporary browser websocket
  observer independently captured both unread/read round trips; removed it and
  the previous request observer after testing. No new messages were sent.
- Stock Slack chose `1791794800.210959` as its cursor for this same message;
  native history returned `1791794243.766559`. Both produced the same one-unread
  boundary. Exact cursor identity for hidden/deleted messages is not established.
- Visually inspected real native UI and offline `message-marked-unread` and
  `message-marked-unread-narrow` captures, including the 668×400 minimum client
  size. The status and composer remain separate and the divider precedes the
  selected message. Agent live screencapture failed on this display; CUA's native
  screenshot succeeded and was inspected.
- Gates: 349 independently locked core tests and 157 desktop tests passed;
  locked check, both format checks, diff whitespace check, and signed macOS build
  passed. Logs are under `tmp/manual-unread-*`; fixture PNGs under
  `tmp/agent-ui/message-marked-unread*.png`.
- Remaining: thread “Mark unread,” keyboard shortcut parity, opening/read
  preference synchronization, and comprehensive Activity/read reconciliation.
  First-message zero-cursor behavior and failure/stale-response isolation have
  automated coverage, but no additional live first-message or outage test.


### 2026-10-12 — Thread unread actions and cross-client read state

- Stock Slack reference in private #echotestchamber: thread “Mark unread” sends
  `subscriptions.thread.mark` with the selected reply timestamp and `read=0`.
  The `thread_marked.subscription.last_read` event is one microsecond before that
  reply. Marking the thread parent uses the same request with the parent timestamp;
  it does not mark the channel unread. Reading sends `read=1` with the latest reply.
- Native thread rows now offer “Mark unread,” a New divider, and the same compact
  “Marked unread / Mark as read” status as channel conversations. Thread holds
  are scoped to the pane visit, account, workspace, channel, and root. Pending
  operations block automatic marks and wait for earlier requests to finish.
- Added typed `thread_marked` decoding and renderer-neutral thread read reducers.
  Integer microsecond arithmetic preserves timestamp precision. Read revisions
  prevent a delayed HTTP response from overwriting intervening events even when
  the cursor changed away and back to the same value. Realtime generations and
  account/transport guards reject stale work.
- Thread feed rows display unread counts and update their unread reply sets.
  Feed requests now carry account, workspace, transport, and request-generation
  guards and reconcile marks received during the request. Cached thread counts
  are recomputed on paging and reply arrival without changing cursor ordering.
- Automatic thread reads use displayed, acknowledged replies and require no
  unfilled newer-history gap; unseen cached replies and local pending sends do
  not advance the cursor. Parent `last_read` metadata, when supplied by replies,
  initializes the read position without superseding newer realtime state.
- Live verification found and fixed a cross-client issue: browser Slack, with
  the same thread open, immediately read a native unread mark. The native read
  cursor updated but the hold banner remained. Realtime reconciliation now
  removes that stale status and updates the divider.
- Final signed-app proof: native marking reply `1791795731.106029` unread set
  thread cursor `1791795731.106028`, count 1, and retained the hold while native
  focus was true. Native “Mark as read” restored cursor `1791795731.106029`,
  count 0, and cleared the hold. Repeated unread, opened that thread in browser
  Slack, and independently observed its `read=1` request and realtime event;
  native count, hold, and divider cleared. Channel cursor remained
  `1791799527.003389` with zero unreads throughout. No messages were sent.
  Restored the test thread to read, closed the browser thread, and removed every
  temporary request, response, and websocket observer after verification.
- Inspected native live screenshot and three actual Dioxus fixture captures:
  `thread-marked-unread`, `thread-marked-unread-narrow` (668×400), and
  `thread-list-unread`. Controls, divider, and composer remain distinct and
  readable. The thread feed count uses existing typography and unread fill.
- Gates: 355 independently locked core tests and 161 desktop tests passed;
  locked check, both format checks, diff whitespace check, and signed macOS build
  passed. Logs: `tmp/thread-unread-*`; screenshots: `tmp/agent-ui/thread-*.png`.
- Remaining parity work includes keyboard shortcuts, synchronized opening/read
  preferences, global thread badge state, comprehensive Activity reconciliation,
  and broader thread-feed pagination/live-arrival behavior. New-reply and stale
  response behavior have reducer coverage; no new two-person reply or outage
  test was performed. The full Slack parity goal remains open.

### 2026-10-12 — Workspace thread totals and sidebar attention

- Threads now shows Slack's workspace mention total, capped visually at 99+,
  or a dot for unread activity without mentions. Its accessible name retains
  the full count. These totals come from `client.counts.threads` and
  `update_global_thread_state`, independently of the paginated thread feed.
- Counts survive warm boot. An absent counts field preserves existing knowledge;
  an explicit cleared snapshot replaces the channel maps. Local revisions reject
  HTTP snapshots overtaken by realtime, and existing socket generation guards
  reject old connections. Slack's event `timestamp` may be an older manually
  selected read cursor, so it is not used to order these events.
- Realtime totals skip transcript/sidebar reprojection and persist through the
  existing background cache writer only when the totals change. Added coverage
  for backward cursors, workspace/connection isolation, stale HTTP snapshots,
  clearing, old sidebar JSON, and warm-cache round trips.
- Live signed-app verification in private `#echotestchamber`: initial native
  totals matched Slack (8 mentions, 17 VIPs). Marking an existing reply unread
  in browser Slack changed native per-channel thread unreads 0 → 1 while the
  native app stayed on Home with no thread open. Closing/reopening that thread
  in Slack restored native count 0. Other workspace unread totals remained
  intact. No messages were sent; the private thread was restored to read, the
  browser thread closed, and the temporary observer detached.
- Inspected four actual native fixtures: mention badge, unread dot, cleared
  state, and narrow sidebar. Rechecked the `system-messages` fixture and live
  native screen: activity text is gray and author names remain bright. Captures
  are in `tmp/thread-counts-ui/`; live inspection used CUA because the agent's
  `screencapture` returned exit status 1.
- Gates: 359 independently locked core tests, 162 desktop tests, both format
  checks, diff whitespace check, and signed macOS bundle build passed. Logs are
  `tmp/thread-counts-{core-tests,tests,build,ui}.log`.
- Remaining: keyboard shortcuts/read preferences, Activity reconciliation,
  broader thread-feed pagination/live-arrival behavior, and two-person incoming
  reply/outage verification. The overall Slack parity goal remains open.

### 2026-10-12 — Thread-feed pagination and live previews

- Found that the previous Threads view silently stopped at its first page and
  displayed the count of preview messages instead of the parent's full
  `reply_count`. Added explicit load-more, retry, loading, empty, and refresh
  states, conversation names, and the full server reply count.
- Corrected the request parameter from `max_ts` to `current_ts`. Slack's shipped
  `getView` fetcher uses `current_ts`; its `processThreads` derives the cursor
  from the last thread's latest returned reply, with unread threads ordered
  first. Its response `max_ts` is not the continuation cursor. Read-only live
  probe: sending `max_ts` repeated all three rows; sending `current_ts` returned
  three different rows with zero overlap. Reference bundles:
  `gantry-v2-shared.595441bc725828a9.min.js` and
  `gantry-v2-async-gantry-v2-shared-boot-async.2aa16ec7da5b5085.min.js`, loaded from
  Slack's current `a.slack-edge.com` resources.
- Extracted renderer-neutral feed state and native loading into focused modules.
  Feed state is account/workspace scoped; workspace switches release pending
  loads. Duplicate boundaries merge by channel/root, failed requests retain
  loaded rows and cursor, and repeated/cyclic cursors stop without an automatic
  request loop. Feed-provided thread totals also respect realtime revisions.
- Known feed roots/replies receive realtime partial edits, arrivals, deletions,
  and unsubscriptions. Changes arriving during an HTTP request replay after its
  snapshot so stale pages cannot undo them. Read marks remain authoritative.
  New subscription/count activity offers refresh without abruptly replacing the
  reader's current list. Reconnect reloads a visible Threads feed.
- Native live proof: the signed app's “Load more threads” expanded 10 rows to
  20 and advanced the cursor. The private echo thread was shown with five
  replies and one unread; opening it in browser Slack changed its native feed
  unread count to zero without a refresh. Sent one authorized test reply in
  `#echotestchamber`, text “Super Platinum thread feed live-update check ·
  2026-10-12” (`1791812865.621769`), and verified both Slack's six-reply thread
  and the native feed's six-reply row without reloading the native list.
- Inspected six real native fixtures in `tmp/thread-feed-ui/`: normal, empty,
  failed, loading, new activity, and narrow. The full-count fixture displays
  147 replies despite holding only a small preview. Final gates: 364 locked
  core tests, 164 desktop tests, both format checks, diff whitespace check,
  signed bundle build and signature verification passed. Logs:
  `tmp/thread-feed-{core-tests,tests,check,build,ui}.log`.
- Removed temporary browser request/bundle probes. Left the private test thread
  read and closed in browser Slack. No messages were sent outside the authorized
  private channel. The single test reply remains as verification evidence.
- Remaining thread work includes unread/opening preferences, keyboard controls,
  broad two-person/outage tests, subscription controls in the native UI, and
  large-feed rendering measurements. Activity reconciliation and the broader
  Slack parity/design/performance ledger remain open.

### 2026-10-12 — Native thread follow/unfollow

- Added a compact Material Symbols bell in the thread header, with Follow /
  Unfollow labels, pressed state, and a disabled pending state. The action is
  captured against its account and thread before spawning, and finishes if its
  source pane closes. It controls reply notifications independently of read
  marks. Initial state is checked with `subscriptions.thread.get`; its live
  response is a `subscriptions` array of thread timestamps.
- Verified Slack's shipped add/remove fetchers in
  `gantry-v2-shared.595441bc725828a9.min.js`: both send `channel`, `thread_ts`,
  and the existing `last_read`. They return only `ok`, so authoritative realtime
  `thread_subscribed` / `thread_unsubscribed` events update local state and
  message copies. Revisions prevent old reads/HTTP acknowledgements from
  reversing a newer change, including a change away and back to the same value.
  Failed writes preserve the prior setting and release the pending control.
- Unfollowing removes the item from the workspace Threads feed and journals the
  change during an in-flight feed fetch; following makes newly available feed
  activity refreshable. No subscription action locally moves a read cursor.
- Live signed-app proof in the existing private `#echotestchamber` thread:
  native Unfollow made Slack offer “Get notified about new replies”; native
  Follow made Slack offer “Turn off notifications for replies”. Then changed
  the setting in browser Slack in both directions and observed the open native
  toggle update without reload. Restored the original followed state. The
  thread cursor remained `1791812865.621769` with zero unreads throughout.
  No messages were sent in this pass.
- Inspected actual native on/off/pending/narrow fixtures in
  `tmp/thread-subscriptions-ui/` plus the live native screenshot and AX toggle.
  The bell and close control remain distinct at narrow width. Gates: 368 core
  tests, 165 desktop tests, locked check, both format checks, diff whitespace
  check, signed bundle build and signature verification passed. Logs:
  `tmp/thread-subscriptions-{core-tests,tests,check,build,ui}.log`.
- Reference observers were removed and the browser thread closed after testing.
  Remaining: synchronized read/opening preferences and shortcuts, Activity
  reconciliation, comprehensive outage/multi-person coverage, large-list
  performance, and the other workflows in the parity ledger.

### 2026-10-12 — Dependency refresh

- Updated both independently resolved Cargo lockfiles to current compatible
  releases, including Tokio 1.53.2, Rustls 0.23.45, and objc2 0.6.5.
- Diagnosed PR #22: Windows imported Tao traits from a different version than
  Dioxus's window; its deny job also retained vulnerable Rustls 0.23.44.
  Removed the direct Tao dependency and used Dioxus's Tao/Wry reexports in
  authentication and Windows capture. Preserved Wry's X11 feature for the
  standalone Linux login window and pinned it to Dioxus's 0.53.5 version.
  Dioxus's git revision is unchanged; the graph has one Tao and one Wry.
- Validation: 368 core and 165 desktop tests passed, both format checks,
  locked build, diff check, cargo-deny (advisories/licenses/bans/sources),
  and Clippy passed (Clippy emits style warnings). Signed bundle and signature
  verification passed. Relaunched signed in to private #echotestchamber;
  opened and dismissed the native quick switcher, visually verified it, and
  left the composer empty. Logs: tmp/dependencies-*.log.
- Changes remain local; no PR was merged or pushed. Windows/Linux runtime and
  build verification still need CI. Activity audit found unguarded page and
  hydration responses; no Activity code changes were made before the user
  redirected this pass to dependencies.

### 2026-10-12 — Build artifact cleanup

- At the user's request, removed desktop deps/incremental/build/fingerprint/
  examples caches, flycheck output, and the independent core target directory.
  No compiler jobs were running. Preserved the current executable and signed
  app; checked its dynamic dependencies were system libraries/frameworks.
- Disk free space increased from 2.4 GiB to 68 GiB. Desktop target decreased
  from 49 GiB to 88 MiB; the separate 16 GiB core target was removed.
  Source, lockfiles, session data, and validation logs remain intact.
- Signature verification and the live agent ping passed. Subsequent compilation
  will rebuild these caches; avoid redundant full builds while continuing UI work.

### 2026-10-12 — Activity pagination and refresh reconciliation

- Added explicit older-page loading, refresh, retry, loading, and empty states.
  Initial loading still fetches up to five pages to fill the unread list;
  additional history is now reachable. Activity filters keep a single scrollable
  row at narrow widths, with accessible pressed states and stable header height.
- Extracted scoped Activity domain/loading modules. Account/workspace/sequence
  checks and native transport identity checks reject stale pages and previews;
  workspace switching clears the old feed and its remembered conversation.
  Duplicate requests are blocked; failed pages preserve rows/cursors for retry;
  repeated cursor cycles stop with refresh available.
- Journaled accepted realtime replacements/inserts and local reads during each
  fetch, then replayed them over its snapshot. Pages deduplicate and sort once,
  selection follows bundled item key changes, and unused hydrated previews are
  pruned. Local reads now subtract their actual row delta from the workspace
  unread total instead of replacing that total with a partial page count.
- Regression evidence: 377 independently locked core tests and 166 desktop tests
  passed; both format checks and diff checks passed. Inspected native paging,
  loading, error, retry, empty, and narrow fixtures in tmp/activity-feed-ui.
  After the header refinement, recaptured paging/narrow. Signed bundle build and
  strict signature verification passed. Logs: tmp/activity-feed-*.log.
- Live proof: opened Activity without selecting any notification. The initial
  five-page load returned 99 unique items/127 hydrated previews; clicking
  Load older activity returned 119 items/148 previews. The Activity badge stayed
  at 40 and private #echotestchamber's cursor stayed 1791812865.621769. Refresh
  completed with 100 items, no failure, and another page available. Verified
  keyboard focus reveals the offscreen Reactions filter, activated it, restored
  All, then returned Home to #echotestchamber. No messages or read-mark tests on
  public notifications were performed.
- Limits: stale-response and cross-workspace races are regression-tested, not a
  live multi-account/outage reproduction. Playwright's transport was closed in
  this pass, so independent Slack browser read-state comparison remains open.
  Broader Activity realtime/read preference behavior and large-list performance
  remain in scope for the overall parity goal.
- Validation used CARGO_INCREMENTAL=0, CARGO_PROFILE_DEV_DEBUG=0,
  CARGO_PROFILE_TEST_DEBUG=0, CARGO_BUILD_JOBS=4. Build output is about 1.7 GiB
  desktop plus 825 MiB core; disk still has roughly 84 GiB free. Use these same
  environment overrides for subsequent validation to avoid duplicate profiles.


### 2026-10-12 — Search paging, isolation, and thread targets

- Replaced the first-page-only loader with explicit paging. Search retains loaded
  rows on a later-page failure, retries the same page, removes overlapping hits
  by channel/timestamp, and exposes loaded/total counts and a terminal empty state.
- Added renderer-neutral request identity for account epoch, workspace, query,
  page, and sequence. Query edits and workspace changes invalidate pending work
  and clear stale rows. Native transport identity is checked before hydration
  writes and result application; stale failures cannot overwrite a newer search.
- Search preserves Slack syntax (including in:, from:, and has:), currently sorted
  newest first. Result snippets show up to three lines. Paging/retry controls stay
  outside the scrolling results, and the dialog is bounded by the window height.
- Search hits retain thread roots. Selecting a reply anchors its parent in Home
  and opens the exact reply through the existing scoped thread loader. Navigating
  away while channel history loads cancels the pending thread open.
- Native keyboard testing caught a controlled-input regression (rapid keystrokes
  were overwritten). Kept the input native/uncontrolled and remount it only for
  programmatic query replacement, leaving per-keystroke edits uninterrupted.
- Validation: 382 core tests and 169 desktop tests passed (551 total), including
  five new paging/request regressions and three desktop merge/query/workspace
  regressions. Both format checks, diff checks, and strict signed-bundle
  verification passed. The enlarged agent snapshot hit the JSON macro recursion
  limit during development; extracting its search payload resolved the compile
  failure without increasing the limit.
- Inspected native search, loading, empty, error, later-page error, and narrow
  fixtures under tmp/search-ui. The first narrow capture clipped paging controls;
  recaptured and verified the bounded layout, visible footer, and thread labels.
- Live private search in:echotestchamber returned 20 of 367 results, then 40 unique
  results on page two with the original 20 preserved in order. Search/paging left
  the channel read cursor at 1791812865.621769. Rapid native typing preserved the
  exact query; Enter produced a verified zero-result state. Programmatic query
  replacement updated the visible field. Tab/Enter opened reply
  1791812865.621769 under root 1791780358.080789; inspected the actual thread view
  with that reply visible and no loading/error state. No messages were sent.
- Limits: Playwright reference transport remains closed, so this is native live
  Slack API proof, not a current both-client comparison. Advanced visual filters,
  selectable sorting, discovery of search-hit channels absent from the sidebar,
  and live account/outage race testing remain open. Full Slack parity is unproven.
- Builds retained CARGO_INCREMENTAL=0 and DEV/TEST_DEBUG=0 with four jobs; disk
  free space remained about 84 GiB.


### 2026-10-12 — Search filter editor and sorting

- Added native channel, sender, after-date, and before-date fields with Apply and
  Clear controls. Reopening the editor restores explicit modifiers from the query.
  Loaded workspace channel/user suggestions are limited to 30 rendered options.
  Dates validate locally before submission; invalid edits preserve existing hits.
- Modifier editing preserves quoted phrases, negative modifiers, other operators,
  Unicode byte boundaries, and unchanged relative dates/multiple values. Changed
  field values are quoted when necessary, and embedded quote/escape injection is
  rejected. The query remains visible and editable as Slack syntax.
- Added relevance, newest, and oldest sorting to the message search request.
  Changing sort invalidates pending results and restarts at page one; subsequent
  pages retain the sort. Query edits preserve the chosen sort. API fields follow
  Slack's documented score/timestamp and asc/desc values:
  https://docs.slack.dev/reference/methods/search.messages/
  Modifier reference: https://slack.com/help/articles/202528808-Search-in-Slack
- Inspected normal and narrow native fixtures under tmp/search-controls-ui.
  The first narrow capture squeezed the filter fields; now opening filters gives
  the editor the available dialog space, with results retained in state and
  restored when the editor closes. All four fields and Apply are visible in the
  final narrow capture. Fixed the search footer's undefined radius token.
- Validation: 388 core tests and 169 desktop tests passed (557 total). Added five
  modifier-editing regressions and a sort/request invalidation regression; the
  request-shape test now checks oldest-first API arguments. Both fmt checks,
  diff checks, and strict signed-bundle verification passed.
- Live checks used only searches scoped to private #echotestchamber. Oldest-first
  returned 20 ascending timestamps, then 40 ascending timestamps with the first
  page unchanged. Newest-first restarted at page one with 20 descending results.
  Applying from:me after:2026-10-11 before:2026-10-13 alongside the channel filter
  returned 16 results, all authored by Rowan and dated 2026-10-12 UTC. Reopening
  filters restored all four values. Invalid 2026-02-30 showed validation without
  changing the query; correction and Enter submitted successfully. Relevance sort
  also returned the 16 filtered results without an API failure (ranking quality
  was not independently assessed). Restored newest-first and closed search.
- No messages were sent and no search hit was opened in this pass; the echo read
  cursor remained 1791812865.621769. Live multi-account/outage races, broader filter
  types, unknown-channel targets, and current Slack web comparison remain open.


### 2026-10-12 — Search entry points and conversation navigation

- Added a sidebar Search messages action, Cmd/Ctrl+G workspace search, and
  Cmd/Ctrl+F conversation search. Conversation search inserts the channel ID scope
  and places the cursor at its end; workspace search selects the existing query.
  Escape and Close invalidate pending search navigation. Shortcut reference:
  https://slack.com/help/articles/201374536-slack-keyboard-shortcuts
- Search hits can resolve missing conversation metadata before navigation without
  joining the conversation. Account, transport, workspace, query, navigation, and
  thread generations reject stale completions. Lifecycle-aware metadata merging
  preserves newer realtime updates. Errors retain search results for recovery.
- Cached archived conversations can be projected for navigation while preserving
  the outgoing composer draft. Opening a reply refreshes channel history and then
  opens the exact thread target only if the navigation is still current.
- Validation: 388 core and 174 desktop tests passed (562 total; logs under
  tmp/search-navigation-{core,tests}.log). Five desktop regressions cover closed
  and replaced searches, stale account/workspace/thread navigation, missing
  metadata resolution, archived navigation/draft binding, and shortcut scope.
  Both fmt checks, git diff --check, and strict signed-app verification passed.
  Inspected channel and pending-navigation fixtures under tmp/search-navigation-ui.
- Relaunched the signed app and verified sidebar search, Escape, Cmd+G, and
  Cmd+F through native UI. Typing retained in:<#C0AQDL3V5GU>; the known private
  reply query returned one result. Opening it displayed reply 1791812865.621769
  under root 1791780358.080789 with no thread loading/error. Inspected the live
  screenshot. An unsent draft survived opening/dismissing search, then the test
  draft was removed without sending.
- Live search also opened private archived channel C0C8MRDCCMS from its archive
  event. History was visible and the composer was replaced by an archived notice.
  This proves archived navigation, not a cold metadata fetch. Restored Home to
  private #echotestchamber with search and thread closed and the composer empty.
- Limits: missing-cache network resolution and live account/outage races remain
  unverified; current both-client comparison and full Slack parity remain open.
  Native macOS typing substituted the opening straight quote in the test query
  with a smart quote; the query still returned the expected hit, but literal
  search syntax input needs a dedicated follow-up.


### 2026-10-12 — Literal search input and dialog keyboard focus

- Reproduced two native defects: typing an ASCII quote in search triggered macOS
  smart-quote substitution, and Shift+Tab from the dialog Close button focused
  the obscured channel Send button. Search/palette/filter inputs now disable
  correction, capitalization, spell checking, and completion. This preserves
  literal syntax without rewriting the query or changing composer preferences.
- Added shared shell-dialog Tab/Shift+Tab containment and focus return on removal.
  Hidden, inert, disabled, and negative-tabindex controls are skipped; the set of
  controls is read on each Tab so expanding filters or receiving results stays
  correct. Pointer tracking preserves the opener on macOS, where clicking a
  button does not always focus it. Removed openers and replacement dialogs do not
  receive stale focus. Removal observers disconnect; no periodic work is added.
- Named the thread close button "Close thread" for assistive tools.
- Five JavaScript regression tests passed via
  node --test scripts/overlay-keyboard.test.mjs. The desktop suite passed all
  174 tests; no core code changed in this pass. Rust fmt, JS syntax, diff checks,
  and strict signed-bundle verification passed. Logs are under
  tmp/search-keyboard-{tests,bundle}.log. The rebuild used incremental/debug-info
  overrides and four jobs; target is about 1.3 GiB with about 93 GiB free.
- Live signed-app checks in private #echotestchamber preserved ASCII quotes,
  negative modifiers and date hyphens exactly. A quoted known-reply query returned
  one result. Tab and Shift+Tab wrapped between Close and the result, and between
  Close and Apply with filters expanded. Escape/Close returned focus to the
  sidebar opener or the original composer. Jump-palette input/results wrapped
  correctly. The thread close button appeared by name and closed the thread.
  Inspected the final live search screenshot; no messages were sent.
- Restored Home/private echo with the search and thread closed and composer empty.
  Other custom dialogs own separate focus behavior and are not covered by this
  shared-shell change. Full assistive-technology and cross-platform audits remain
  open, as do the broader feature-parity and performance requirements.


### 2026-10-12 — Bounded quick-switcher ranking with paired measurement

- Replaced full sorting/retention of every matched person and channel with a
  sorted top-12 accumulator. Score, unread, label ordering and stable encounter
  order for exact ties are preserved. Candidate retention now stays at twelve
  entries after each insertion (one transient candidate), instead of growing with
  all matches. This does not bound the input workspace or DM lookup map.
- Added an independent stable-sort regression over every prefix of 512 candidates,
  including ties, late winners, unread counts, and Unicode labels. Existing rank
  tests continue to cover deleted/bot/self exclusion, previous channel names,
  existing DMs, remote channels, recents, and frecency.
- Added scripts/bench-palette.sh and an ignored core benchmark using synthetic
  10,000/50,000 users plus one channel per ten users. Both algorithms run in the
  same process against the same workspace. Five warmups precede 40 alternating
  pairs per query; identical final entries are asserted. Core test optimization
  is level 3, matching the shipped dependency optimization. No credentials or
  live messages enter the benchmark.
- Separate-process initial timings were noisy, including large changes on the
  unchanged no-match path; they are not the basis for the improvement claim.
  The two paired runs below are the accepted broad-query evidence (milliseconds):

  | Users | Query | Full-sort median, run 1 | Bounded median, run 1 | Full-sort median, run 2 | Bounded median, run 2 |
  | --- | --- | --- | --- | --- | --- |
  | 10,000 | person | 3.530 | 2.501 | 3.834 | 2.710 |
  | 10,000 | 東京 | 3.903 | 3.083 | 4.098 | 3.212 |
  | 50,000 | person | 23.598 | 15.439 | 25.128 | 16.510 |
  | 50,000 | 東京 | 26.190 | 18.880 | 46.270 | 33.083 |

- Broad ASCII median cost fell about 34–35% at 50,000 users; Unicode about 28%.
  No speedup is claimed for narrow/no-match queries. In particular, 50,000-user
  no-match medians were 16.596 vs 16.712 in run 1 and 35.443 vs 40.449 in run 2;
  load/timing variability remains unresolved. Full logs are
  tmp/palette-performance-paired.log and tmp/palette-performance-paired-repeat.log.
  These are focused CPU measurements, not end-to-end keyboard or frame-time proof.
- Validation: 389 core tests and 174 desktop tests passed (563 total); the ignored
  benchmark was explicitly run twice. Both fmt checks, diff checks, script syntax,
  and strict signed-bundle verification passed. Logs:
  tmp/palette-core-tests.log, tmp/palette-desktop-tests.log, tmp/palette-bundle.log.
- Relaunched the signed client. Native Cmd+K found private #echotestchamber;
  Down/Enter selected it correctly, and an unmatched query showed the empty state.
  Restored echo with palette/search/thread closed and composer empty. No messages
  were sent. Broader startup/scrolling/resource measurements and full parity remain
  open; native discovery of people without an existing DM remains a feature gap.


### 2026-10-12 — Quick-switcher people without existing DMs

- Fixed native projection discarding ranked User targets without an existing
  channel index. The palette now retains typed channel/person targets and shows
  names, handles, avatars/presence, and existing unread badges. Unopened-person
  rows are view projections only; lookup creates no conversation metadata.
- Clicks bind the rendered target identity, while Enter and agent selection use
  the selected ranked target. Existing conversations open immediately. New people
  use conversations.open, project the returned DM, preserve the outgoing draft,
  and navigate to Home before loading history. The UI reports pending opening
  and recoverable errors. Selecting agent entry zero now honors that index.
- Account epoch, transport identity, workspace, palette request generation,
  channel/thread generations, and active surface guard asynchronous completion.
  Closing/reopening, query ABA edits, workspace switches, and newer selections
  invalidate old opens; old cleanup cannot clear the newer pending selection.
  This guard is for palette navigation; the separate profile DM flow is unchanged
  and still needs an equivalent stale-response audit.
- Added four desktop regressions: unknown-DM person projection, successful
  completion plus outgoing draft restoration, stale completion without metadata
  insertion, and replacement-request cleanup. All 178 desktop tests passed;
  core code was unchanged in this pass. Rust fmt, diff checks, and strict signed
  app verification passed. Inspected palette-people and normal recent-results
  fixtures under tmp/palette-people-ui; log tmp/palette-people-tests.log.
- Relaunched the signed build. A live name lookup returned twelve results,
  including four person targets with no loaded DM. No target was opened during
  that lookup; the active conversation stayed private #echotestchamber. Native
  typing/Down and then a private-channel query/Enter worked through the unified
  selection path. No messages were sent or new real DMs created. Restored echo
  with the palette closed and composer empty.
- Limits: actual first-DM API completion is covered by state regression tests,
  not a live creation round trip. Lookup covers loaded workspace people; remote
  people discovery and broader group-DM workflows remain open. Full parity,
  design and performance acceptance remain unfinished.


### 2026-10-12 — Self-DM quick-switcher discovery

- The quick switcher now includes the signed-in user in name/handle lookup,
  labels that row “(you)”, and reuses an existing self-DM when present. Deleted
  people and bots remain excluded from person search. Decorative palette avatars
  are hidden from accessibility so row names are not spoken twice.
- Added a core regression for self lookup reusing the existing notes conversation
  and updated the exclusion test to retain self. All 390 core tests and 178
  desktop tests passed (568 total); one opt-in benchmark remains ignored by the
  ordinary core suite. Both fmt checks, diff checks, and strict signed-bundle
  verification passed. Logs: tmp/palette-self-core.log,
  tmp/palette-self-desktop.log, tmp/palette-self-bundle.log.
- In the rebuilt signed client, native Cmd+K/name lookup displayed “Rowan (you)”
  with the current handle. Enter opened the existing self-DM D08000BEGJV in Home
  and dismissed the palette. This was a cached-conversation route; it did not
  exercise conversations.open or establish first-DM/self-DM creation support.
  No messages were sent. Restored private #echotestchamber with an empty composer
  and closed palette/thread.
- The live quick-switcher screenshot was inspected for layout. Initial typing
  immediately after opening was not captured twice during automation; explicitly
  focusing the input worked. Keyboard opening/focus timing needs a follow-up.
- Storage check after the compact rebuild reports 94 GiB available. Full feature
  parity and broader design/performance acceptance remain unfinished.


### 2026-10-12 — Preserve typing while shortcut dialogs open

- Reproduced dropped prefix characters in the signed native client: rapid
  Escape / Cmd+K / `echotestchamber` produced only `chamber`. Removing the delayed
  autofocus/RAF selection alone still produced `otestchamber`, demonstrating a
  second gap before the native render round trip creates the input.
- Initial focus and selection now run once per inserted input in a MutationObserver
  scoped to the persistent overlay slot. Result rerenders do not reselect text or
  steal focus from a result. The observer does not watch timeline mutations and
  does not poll. Conversation search retains its caret after the channel prefix.
- Cmd+K/F/G arm a bounded, short-lived queue for input arriving before the new
  field exists. It replays text, paste, select-all, deletion and Up/Down/Enter into
  that field, with input updates before navigation keys. Enter cannot reach the
  obscured composer. Escape, another shortcut, pointer interaction, root
  replacement, or the 1.5-second expiry clears pending input. Repeated Cmd+K in
  an already mounted palette selects its query synchronously.
- Added nine JS regressions (14 overlay tests total), including selection stability,
  prefix caret, removed input, queued text/paste/Unicode deletion, early Enter
  delivery order, cancellation, and unrelated-dialog isolation. All 17 JS tests
  (including three existing scrolling tests) and 178 desktop Rust tests passed.
  Core behavior was unchanged in this pass. Rust fmt, diff checks, and strict
  signed-bundle verification passed. Logs: tmp/palette-focus-js.log,
  tmp/palette-focus-tests.log, tmp/palette-focus-final-build.log.
- Live native checks preserved the complete immediate Cmd+K query and replacement
  query, Cmd+F channel prefix plus new text, and Cmd+G literal quoted text. Rapid
  Cmd+K/query/Enter selected private echo. Inspected the palette screenshot;
  Shift+Tab focused its result and Tab returned to its editable query. Removed
  temporary focus tracing, rebuilt/relaunched the final signed app, and repeated
  the immediate-query/Enter check. No messages were sent; restored private echo
  with palette/thread closed, composer empty, and its read cursor unchanged.
- Live checks cover the macOS keyboard path and ordinary text/punctuation; IME
  composition, alternate platforms, and assistive-technology combinations remain
  unverified. The queue leaves composition events to the native input system.
  Full feature parity, design and broad performance acceptance remain open.


### 2026-10-12 — Guard profile refresh and Message navigation

- Profile hydration previously checked only the selected user ID after network
  work. It now checks account epoch, authenticated transport identity, workspace,
  profile generation and selected profile after both asynchronous hydration
  stages. Closing/reopening the same profile or switching workspaces retires the
  old visit. Workspace switching now closes its old profile pane. Missing
  credentials finish the current loading state with actionable text; profile
  errors no longer expose raw transport errors in the pane.
- Moved profile Message navigation into bootstrap/profile_navigation.rs. Intent
  is captured synchronously at the click, duplicate pending opens are ignored,
  and completion checks the account/profile visit, request generation, channel,
  thread, rail surface and overlay/search/palette navigation. Stale completion
  cannot insert channel metadata, navigate, or clear a replacement request.
  Escape and subsequent navigation cancel the local pending action.
- A successful open retains existing metadata, preserves the outgoing draft,
  opens the DM in Home and closes the profile/hover card. Pending Message buttons
  display “Opening…” with disabled and aria-busy states. Both pane and hover
  entry points use the same path. Error text is recoverable and generic.
- Seven new regressions cover account/profile/transport changes, workspace ABA,
  missing-credential cleanup isolation, draft preservation on successful DM
  completion, stale completions after navigation, duplicate clicks and replacement
  cleanup. All 185 desktop tests passed; core code was unchanged in this pass.
  Rust fmt, diff checks and strict signed-bundle verification passed. Logs:
  tmp/profile-navigation-tests.log, tmp/profile-navigation-build.log.
- Captured and inspected normal and pending profile fixtures under
  tmp/profile-navigation-ui. The pending label preserves button dimensions and
  surrounding layout. Relaunched the final signed app, refreshed the signed-in
  user's profile, and clicked Message: the existing self-DM opened in Home and
  the profile closed. Restored private echo using Cmd+K, with an empty composer,
  closed palette/profile/thread and unchanged read cursor. No messages sent.
- Limits: first-DM API creation and stale network completions are not live-proven;
  the former remains a gap and the latter is covered by deterministic state tests.
  Separate VIP mutation completion and recent-profile channel metadata merge
  behavior still need their own audits. Full feature parity, design and broad
  performance acceptance remain unfinished.


### 2026-10-12 — VIP write isolation and profile channel metadata

- Replaced one global profile VIP loading flag with pending changes keyed by
  account epoch, workspace and person, scoped to the authenticated transport.
  Closing/reopening a profile retains its outstanding write and rejects duplicate
  clicks, while other people remain independently actionable. Captured click
  intent runs outside the profile component lifetime.
- A successful VIP mutation updates and persists its original workspace even
  after switching away, but cannot mutate another account or renewed transport.
  Completion and failure feedback are limited to the originating profile visit;
  old cleanup cannot clear a replacement request. Active-workspace projections
  refresh after success. Failures use recoverable text without raw URLs.
- Workspace VIP snapshots now record per-person membership revisions. A delayed
  HTTP success cannot reverse newer realtime changes, including add/remove ABA
  cycles. Changes for unrelated people and repeated identical snapshots do not
  invalidate the request. Boot and realtime preference snapshots use this path;
  cache format is unchanged and revision counters are session-local.
- Profile recent-conversation hydration captures the channel lifecycle revision
  before fetching and uses the existing lifecycle-aware refresh merge rather
  than wholesale map insertion. This reuses the tested metadata merge behavior
  already used by search and reconnect discovery.
- Added five desktop regressions for pending-request isolation, profile closure,
  workspace switching, account/transport replacement, and realtime precedence,
  plus two core VIP revision regressions. All 190 desktop tests and 392 core tests
  passed (582 total; one opt-in core benchmark ignored). Both fmt checks, diff
  checks, and strict signed-bundle verification passed. Logs:
  tmp/profile-vip-tests.log, tmp/profile-vip-core.log, tmp/profile-vip-build.log.
- Inspected tmp/profile-vip-ui/profile-vip-pending.png. The affected VIP button
  is disabled with aria-busy and an updating tooltip; Message remains available
  and layout stays stable. Relaunched the signed app on private echo, with no
  profile, palette or thread open and the same read cursor. No live VIP preference
  was changed and no message was sent. A live VIP mutation round trip and profile
  metadata overlap remain unverified; state tests do not establish those claims.
- Remaining DM feature gap confirmed in code: existing group conversations can
  open, but there is no recipient picker for starting a new group DM. Full parity,
  design and broad performance acceptance remain open.


### 2026-10-12 — New DM and group-DM recipient picker

- The Direct messages compose button now opens a dedicated recipient picker.
  Search matches loaded people by display name, real name, and handle, keeps a
  bounded top 30, excludes self/deactivated/already-selected people, and supports
  removable recipients, arrow/Enter selection, empty-field Backspace, and the
  shared dialog focus trap/restoration.
- Core `Recipients` validates a nonempty set of up to eight other people,
  removes duplicates/caller while preserving selection order, and rejects
  malformed IDs. The new `conversations.open` path requests full metadata and
  supports both one-to-one DMs and MPIMs. Reference:
  <https://docs.slack.dev/reference/methods/conversations.open/>.
- Opening is guarded by account epoch, workspace, transport identity, dialog
  generation, conversation/thread generation, and originating surface. Dialog
  tasks are canceled on unmount; late responses cannot redirect a later visit.
  Navigation lands in Home, retains the outgoing draft, and preserves existing
  conversation metadata/read state when Slack returns a sparse object.
- Pending state locks recipients and the submit action, leaves cancellation
  available, and provides safe permission/network/error copy with retry. Native
  captures caught a clipped footer at minimum window size; the dialog now has
  a bounded, scrollable body and fixed footer, plus visible disabled styling.
- Live selection exposed a native input bug: adding `alex` with Enter and
  immediately typing `maya` produced `alexmaya`. Recipient input reset now
  happens synchronously in the WebView before Rust processes the selection;
  it preserves the existing input and does not dispatch a premature input event.
  IME confirmation and disabled/no-result options do not reset the field.
- Validation: 195 desktop tests, 395 core tests (one benchmark ignored), 17
  overlay keyboard tests; full group metadata decode also checked after its
  fixture expansion. Formatting, diff whitespace, signed bundle build and strict
  signature verification passed. Five native captures inspected under
  `tmp/new-dm-ui/`; logs `tmp/new-dm-{core,final-tests,keyboard-tests,metadata-test,final-build}.log`.
- Final signed-build live recheck passed: `alex` + Enter + immediate `maya`
  preserved the fresh `maya` query, selected two recipients, removed the last
  with Backspace, removed the remaining chip by click, disabled the empty submit
  action, canceled via Tab/Enter, and reopened with an empty selection. No
  messages were sent and no conversations with other people were created.
  Server-side creation/permissions and remote people discovery remain unverified
  or incomplete; this does not establish full Slack parity.

- Follow-up observations: one background launch initially presented a blank
  WebView until a shortcut activated it; the final signed relaunch rendered
  normally. A rapid Escape from New message followed immediately by Cmd+K and
  typing `echotestchamber` once lost the `ech` prefix (the matching private
  channel still opened). These intermittent foreground/shortcut handoff cases
  were not resolved by the recipient-input fix; the shortcut follow-up below
  addresses the typing loss. The intermittent background-launch observation
  remains a separate investigation.


### 2026-10-12 — Preserve typing across closing and reused shortcut dialogs

- Reproduced the rapid close/reopen failure in the signed app: Escape followed
  immediately by Cmd+K and `Rowan` produced `owan`. The existing mounted-palette
  fast path could focus the input that Escape was still removing.
- The WebView now marks an escaping dialog as closing and buffers subsequent
  shortcut typing until the new visit is acknowledged. A dedicated
  `shortcut_generation` advances only on opening palette/search, so a delayed
  old query render cannot acknowledge the new visit. The overlay observer also
  handles coalesced close/reopen updates that retain the same input node.
- The acknowledged visit applies its requested query/prefix and then replays
  queued edits. Normal result renders and native typing keep their existing
  focus/selection behavior. Observation remains scoped to the overlay slot;
  only the dialog kind/generation attributes were added to the mutation filter.
- Validation: 21 keyboard tests and 196 desktop tests passed; new cases cover
  a closing input, a reused input, delayed old-query updates, and repeated
  conversation-search prefixes. `cargo fmt --check`, `git diff --check`, signed
  bundle build and strict codesign verification passed. Core domain code did
  not change in this pass. Logs: `tmp/shortcut-handoff-{tests,rust,build}.log`.
- Signed live checks: initial immediate typing, four rapid Escape/Cmd+K cycles
  alternating `Rowan` and `echotestchamber`, Escape from the recipient picker
  followed by Cmd+K/`echotestchamber`, Cmd+F/`focuscheck`, repeated Cmd+F/
  `secondcheck`, and Cmd+G/`"literal phrase"` all retained the expected full
  input. Conversation searches retained `in:<#C0AQDL3V5GU> ` exactly. Native
  screenshot visually inspected. No searches submitted, messages sent, or new
  conversations created; restored Home on the private test channel.
- These checks establish the reproduced shortcut fix, not full Slack parity
  or exhaustive IME/platform behavior. Remote people discovery and live group
  creation remain outstanding from the recipient-picker work.


### 2026-10-12 — Remote recipient discovery and stable directory requests

- New message now searches Slack's people directory after a 250 ms debounce,
  keeping cached suggestions immediately available. Remote results append without
  moving the current local selection; duplicate, self, selected, empty-ID and
  deactivated people are excluded. Fuzzy/email matches from Slack remain usable.
  Suggestions are bounded to 30 local plus 30 remote; refine the query for more.
- Queries, account/team/transport changes and dialog/navigation generations scope
  every completion. Only an accepted recipient selection hydrates the workspace,
  preserving newer cached/realtime profiles. Loading, empty, offline, rate-limit
  and other failure states retain local matches and provide a Retry action.
  People without a handle no longer display a stray `@`.
- The first signed live check uncovered a reactive subscription bug: credentials
  capture subscribed the search effect to the entire shell, so media/realtime
  updates repeatedly canceled/restarted the same search and cleared results.
  A real Dioxus VirtualDom test reproduced this before the fix. Directory context
  capture now peeks at a snapshot; only the request-key memo and Retry subscribe
  the effect. The existing Context::get subscription behavior remains unchanged.
- Validation: 202 desktop tests passed, including the failing-before/passing-after
  runtime regression and stale query/account/transport/deactivation/merge tests.
  The runtime regression also checks query changes and explicit Retry still work.
  Formatting, diff whitespace, signed build, and strict codesign verification
  passed. No core domain changes in this pass. Logs:
  `tmp/dm-directory-runtime-red.log`, `tmp/dm-directory-final-tests.log`,
  `tmp/dm-directory-final-build.log`.
- Six native captures inspected under `tmp/dm-directory-ui/`: directory results,
  loading, error, empty, minimum-window error and selected recipients. Footer,
  retry and input remain visible at minimum size. Final signed live screenshot
  also inspected: searching `charlotte` displayed remote matches and cleared the
  loading indicator. Opt-in count-only trace recorded one completed request,
  30 returned users and 28 uncached users, without subsequent automatic requests
  in the observed log (`tmp/dm-directory-final-live.stderr`).
- Native selection verification was interrupted by an external UI change: the
  picker closed and the app switched to an existing DM. Automation stopped there
  rather than restoring another view. No messages sent or conversations created.
  Actual remote-recipient selection and server-side first/group-DM creation remain
  unverified; full Slack parity remains incomplete. Storage cleanup remains stopped.


### 2026-10-12 — Recover local drafts whose conversation metadata is missing

- `Continue writing` previously stopped when a saved draft's conversation was
  absent from the workspace cache. It now resolves that conversation through
  the existing Slack info API, merges it with lifecycle revision protection,
  then navigates to the original channel or thread. Cached destinations still
  open without a metadata request. Archived destinations are projected before
  navigation so outgoing composer text remains bound to its original channel.
- The draft-list component owns pending lookup tasks. Completion checks the
  request revision, account epoch, team, transport identity, channel/thread
  generations, current surface and overlay visit, and continued draft existence.
  Choosing another draft, deleting it, or navigating away cannot let an old lookup
  hydrate metadata or move the user. The history task survives the deliberate
  draft-list unmount only after successful navigation.
- The row disables duplicate opens and shows `Opening…` in a button with stable
  width. Missing credentials, offline and lookup failures leave the draft text
  in place with a retryable error. Added four offline fixtures for the normal,
  loading, error and minimum-window error states.
- Validation: 207 desktop tests passed, including five new tests covering cold
  metadata resolution, archived destinations and outgoing draft preservation,
  channel/thread draft separation, stale account/transport/navigation/deletion,
  wrong response IDs, duplicate clicks and A/B/A selection. Formatting and diff
  whitespace passed. Signed bundle build and strict codesign verification passed.
  Logs: `tmp/draft-resume-tests.log`, `tmp/draft-resume-build.log` and
  `tmp/draft-resume-ui.log`. All four native screenshots under
  `tmp/draft-resume-ui/` were visually inspected.
- No core domain changes, messages, Slack mutations or storage cleanup in this
  pass. The signed bundle was rebuilt but the user's running client was not
  restarted. A real uncached-destination lookup and end-to-end thread loading
  from this entry point remain unverified; the goal remains incomplete.
- Follow-up audit finding: channel/thread composer Enter handlers currently lack
  an explicit IME-composition guard. Check native composition/send behavior in a
  subsequent pass; do not treat existing formatting tests as proof of IME safety.


### 2026-10-12 — Keep IME confirmation out of send and shortcut handlers

- The composer audit found that both channel and thread Enter handlers could
  send on an IME confirmation key. Escape, focus trapping, formatting, and
  recipient selection also needed to leave composition keys alone. WebKit can
  dispatch the final keydown after compositionend with `isComposing=false`;
  see https://bugs.webkit.org/show_bug.cgi?id=165004 and
  https://developer.mozilla.org/en-US/docs/Web/API/Element/keydown_event.
- The shared native keyboard layer now stops propagation of composing keydowns
  and legacy IME key code 229 before Dioxus's bubbling bridge or app shortcuts.
  It deliberately does not prevent default behavior, preserving native text
  composition. Both Rust composer handlers also reject `is_composing` events.
  Ordinary Enter, Escape, Tab, formatting and shortcut buffering retain their
  existing paths; no timing-based suppression window was introduced.
- Three new Node regressions failed before the change and passed afterward:
  composition keys reaching application handlers, WebKit final Enter clearing
  or replaying recipient input, and IME Escape marking a dialog as closing.
  All 24 keyboard tests and 207 desktop tests passed. Formatting and whitespace
  checks passed. Logs: `tmp/ime-keyboard-red.log`,
  `tmp/ime-keyboard-tests.log`, `tmp/ime-desktop-tests.log`.
- Added `scripts/check-ime-native.sh` and an opt-in disconnected-fixture probe.
  It exercises synthetic compositionstart/compositionend/key events through the
  actual WebKit/Dioxus bridge for channel and thread composers. Twelve guarded
  Enter/Escape/format cases preserved DOM text, Rust composer text, thread and
  overlay state, and native default handling. Two ordinary Enter positive
  controls reached Rust and cleared/queued the offline composers.
- The native probe passed for the built executable and signed app bundle;
  strict codesign verification passed. Logs: `tmp/ime-native-run.log`,
  `tmp/ime-signed-native-run.log`, `tmp/ime-native-check.log`, `tmp/ime-build.log`.
  The initial harness run used the wrong fixture name (`thread` instead of
  `threads`) and failed its mount check; corrected before the successful runs.
- These are real WebView event-delivery checks with synthetic events, not proof
  of an actual macOS input-method candidate session. Manual OS IME coverage and
  wider Slack parity remain incomplete. No Slack messages or other remote
  mutations occurred. The user's running signed-in client was not restarted,
  and storage cleanup remains stopped.


### 2026-10-12 — Start native mention and channel completion

- Audit confirmed the main/thread composers have no `@`/`#` completion yet.
  Reviewed the read-only multiSlack `composerAutocomplete.ts` and
  `composerSuggestions.ts` resources and Slack's formatting reference:
  https://docs.slack.dev/messaging/formatting-message-text/.
- Added renderer-neutral `composer_completion`: caret-aware UTF-8 triggers,
  bounded eight-result ranking, explicit typed user/channel/group/broadcast
  targets, and insertion into the exact selected occurrence. Duplicate display
  names remain separate identities; other literal occurrences are not rewritten.
  Empty/deactivated identities and archived/DM channel-link candidates are
  excluded. Missing profile fields do not gain an empty-query ranking bonus.
- Triggers skip emails, URLs, existing Slack references, selections, invalid byte
  offsets and code spans/fences. Insertions check that query/caret intent remains
  current, reject malformed IDs/wrong target kinds, retain Unicode and suffix
  text, and avoid extra whitespace before punctuation or existing whitespace.
- This is the domain layer only: it is not connected to the running composer and
  does not establish user-visible mention completion. Required next work remains
  the native suggestion popup, readable reference presentation that preserves
  IDs through editing/draft restoration, scoped remote people lookup, keyboard
  and IME interaction, and a private send/render round trip. Keep these as one
  unfinished feature; do not replace the requested workflow with a raw-ID input.
- Validation: 402 core tests passed (one benchmark ignored), including seven
  completion tests; all 207 desktop tests passed against the new core module.
  Both workspace format checks and diff whitespace checks passed. Logs:
  `tmp/mention-core-final-tests.log`, `tmp/mention-desktop-final-tests.log`.
  No UI/runtime claims, native rebuild, Slack mutations or cleanup in this pass.

### 2026-10-12 — Wire native mention completion into both composers

- Added a shared channel/thread controller with compact Geist suggestion rows,
  arrow navigation, Enter/Tab selection, pointer selection, Escape dismissal,
  ARIA listbox state, and IME protection. Native synchronous key cancellation
  prevents selecting a mention from also invoking Rust's send handler.
- Added occurrence-based display references: names remain readable, while the
  composer/draft/send model keeps exact Slack user, channel, group and broadcast
  tokens. Equal display names keep separate identities. Editing a label removes
  only that occurrence's identity; typing a matching name never guesses an ID.
  Wire/display selection mapping handles UTF-16 browser offsets, Unicode,
  formatting and code spans. Restored drafts resolve only referenced identities,
  without copying the workspace directory into JavaScript.
- Nonempty people queries use a cancellable 250 ms directory lookup. Results are
  guarded by account epoch, destination, navigation generations and transport;
  native sequence/text/caret checks reject stale replies. Cached profiles win
  over stale directory profiles and only a selected remote person is hydrated.
  Exact remote matches can displace cached prefix matches in the bounded list;
  an explicitly keyboard-selected row stays available while results arrive.
- Reference tests caught and fixed two identity bugs: an ambiguous string diff
  after deleting the first of two identical names, and redo restoring a selected
  mention as plain text. Beforeinput selection hints disambiguate occurrences;
  attaching identity updates the native insertion's metadata snapshot.
- Native testing also exposed invalid undo ranges after direct focused `.value`
  replacement. Focused programmatic writes now use escaped text fragments through
  WebKit's replacement command, with a text-command fallback. Internal input
  events are suppressed to preserve the setter's event contract. References:
  https://github.com/WebKit/WebKit/blob/main/Source/WebCore/editing/EditorCommand.cpp
  (ReplaceSelectionCommand versus TypingCommand) and
  https://developer.mozilla.org/en-US/docs/Web/API/Document/execCommand.
- Validation: 210 desktop tests and 39 JavaScript tests pass. The
  native harness covers real WebView/Rust state and requests actual macOS
  Enter/Delete/undo/redo keystrokes against its own disconnected fixture PID;
  synthetic script commands alone have unreliable native undo-group boundaries.
  The normal executable passed all 12 model receipts and actual macOS keyboard
  actions. Signed app keyboard attempts stopped on the foreground guard (both
  the ordinary bundle and an isolated fixture bundle); that check is unverified,
  and further real-key automation was stopped rather than risk another window.
  The final signed bundle passed strict codesign verification, the passive
  picker/toast geometry capture, and the existing IME regression (12 guarded
  synthetic keys and two offline send controls). Channel and narrow thread
  captures were inspected; notices move below the header while a picker is open
  so they cannot cover its choices. No signed-in client restart occurred.
  Logs: `tmp/completion-desktop-tests.log`, `tmp/completion-js-tests.log`,
  `tmp/completion-native-run.log`, `tmp/completion-signed-capture.log`,
  `tmp/completion-signed-ime.log`, `tmp/completion-bundle.log`.
  Screenshots: `tmp/completion-channel.png`, `tmp/completion-native.png`.
- Still unfinished: live directory completion and a private send/render round
  trip; clipboard identity preservation; long undo histories beyond the bounded
  reference snapshots; unfocused programmatic replacement undo behavior; cold
  draft metadata hydration; broadcast eligibility and member/recent ordering.
  These remain part of the original Slack parity goal. No live Slack messages,
  commits, pushes or storage cleanup were performed in this pass.

### 2026-10-12 — Preserve mention identities through composer clipboard events

- Added a versioned custom clipboard representation alongside readable
  `text/plain`. Full selected reference occurrences retain their exact Slack
  tokens when pasted between channel and thread composers in the same workspace;
  matching literal names and partially selected labels remain ordinary text.
  Cut preserves the exact unselected characters and uses a native edit command.
- The receiver validates workspace, visible-text agreement, token shape and
  ordered in-bounds UTF-16 spans. Foreign-workspace or malformed metadata falls
  back to the browser's normal paste. Imported references in code/escaped text
  stay readable literal text. Shift+Cmd/Ctrl+V bypasses reference metadata.
  Native truncation/normalization cannot attach IDs at invalid display offsets.
- The native harness creates private DataTransfer objects and dispatches
  clipboard events through the real WebView, then verifies Rust composer state.
  It never reads or changes the system clipboard and sends no OS keys. Nine
  signed-app model receipts passed: both copy directions, partial cut/paste,
  plain pasted names, plain-text shortcut, foreign-workspace fallback, code
  paste and partial-label copy. The ordinary executable passed the first eight
  cases before adding the explicit plain-text shortcut case.
- Validation: 210 desktop tests, 45 JavaScript tests, format and diff checks
  passed. Rebuilt the signed application; strict codesign verification and the
  existing signed IME check passed (12 guarded synthetic keys, two offline send
  controls). Logs: `tmp/clipboard-{desktop-tests,js-tests,native-run,
  signed-native,signed-ime,bundle}.log`.
- A focused Node measurement with 40,639 visible characters and 250 references
  over 100 measured copy-import samples took 0.28 ms median / 0.66 ms p95.
  `tmp/clipboard-benchmark.log` measures reference metadata only; it excludes
  native clipboard access, DOM editing and Rust IPC, and is not end-to-end UX
  performance evidence.
- WebKit custom MIME behavior was checked against
  https://webkit.org/blog/8170/clipboard-api-improvements/ and paste-event
  handling against https://developer.mozilla.org/en-US/docs/Web/API/Element/paste_event.
  Actual system-pasteboard transfer, cross-window/relaunch persistence and stock
  Slack rich clipboard interoperability remain unverified. This closes the
  in-app event/model gap, not full clipboard parity. Other mention gaps from the
  preceding entry and the full Slack parity objective remain active. The live
  client was not restarted; no Slack messages or storage cleanup occurred.


### 2026-10-12 — Native canvas reading and desktop protocol verification

- Canvas files now open a native reader from both Files results and message
  attachments. The reader retains formatted headings, inline emphasis, code,
  links, tables, lists, checklists and images, with refresh/retry states and
  an independently scrolling document. It renders the first 80 top-level nodes
  and exposes more on demand. Editing is not implemented by this change.
- Renderer-neutral parsing produces an inert typed tree; server HTML, styles,
  event attributes and executable elements are never injected into the WebView.
  Parsing runs off the UI thread and rejects oversized/deep/wide exports.
  Authenticated content requests accept only the exact HTTPS Slack file host;
  images use the existing registry's host-specific credential policy.
- Reader requests are scoped to account, team, transport and dialog lifetime.
  Refresh generations reject stale responses, and a failed refresh retains the
  previously loaded document. Replacing the transport remounts the reader.
- Live protocol evidence: both documented `canvases.create` and
  `conversations.canvases.create` rejected the desktop session with
  `not_allowed_token_type`; neither created a canvas. Slack's cached desktop
  bundle calls `files.createCanvas` with title/markdown. That call created one
  private standalone test canvas; it was not shared to a channel. The probe
  verified the private #echotestchamber destination before testing. No Slack
  messages were sent.
- `canvases.getContent` also rejected this session. `files.info` followed by an
  authenticated fetch of `url_private` returned the canvas HTML export. The new
  production `fetch_canvas` path successfully parsed the task-owned document's
  heading, Unicode and completed/incomplete checklist entries. Logs:
  `tmp/canvas-protocol-{probe,channel,desktop,read}.log` and
  `tmp/canvas-native-protocol.log`. The task-owned receipt records that the
  canvas is private and unshared. The temporary credential-loading probe source
  was removed after verification.
- Full parity remains unproven: native create/edit controls, concurrent editing,
  canvas comments, custom channel tabs, lists, workflow/app interactions and the
  other ledger gaps remain. Reading via authenticated API and rendering a native
  fixture are separate evidence; the existing signed-in client was not restarted.
  Storage cleanup remains stopped.
- Validation: 211 desktop tests and 406 independent core tests passed (one
  pre-existing ignored core test). Signed-app native checks passed at normal and
  minimum window sizes: Files open routing, headings/emphasis, Unicode,
  checklists, table/code rendering, bounds, horizontal overflow, independent
  scrolling and dismissal. These use passive DOM events in disconnected fixtures,
  never OS keys. Logs: `tmp/canvas-{desktop-tests-final,core-tests-final,
  signed-native}.log`. Four signed native captures were inspected:
  `tmp/agent-ui/canvas{,-narrow,-loading,-error}.png`. The check caught and fixed
  inherited modal sizing; loading placeholders received an explicit contrasting
  fill. `scripts/agent-ui-check.sh` now accepts `SUPER_PLATINUM_BINARY` so signed
  bundles can use the same capture workflow. Format/diff checks and strict
  codesign verification passed. Final signed bundle: `tmp/canvas-bundle-final.log`.


### 2026-10-12 — Native canvas rename and collaboration protocol findings

- The canvas reader now offers Rename after fresh metadata confirms edit
  permission. The inline form rejects empty names, preserves drafts on failures,
  keeps the confirmed header unchanged until success and supports cancellation.
  Successful renames update cached Files results and refresh the document.
- Before writing, the client re-reads file metadata and rejects a draft whose
  original name has already changed, or whose permission has been revoked.
  Account, workspace, transport, canvas ID and dialog visit are checked after
  awaits and before sending the mutation. Only the `file` and `title` fields
  (plus authentication) go to `files.edit`; body and sharing are untouched.
  Mutations do not automatically retry. A matching latest title satisfies a
  retry without writing it again. Slack supplies no atomic revision precondition
  here, so simultaneous renames can still race after the preflight read.
- The task-owned canvas was freshly verified as owned by the signed-in user,
  nonpublic, editable, with zero channel/group/DM shares and
  `org_or_workspace_access: none`. The public `canvases.edit` endpoint rejected
  this desktop token with `not_allowed_token_type`. The desktop `files.edit`
  request renamed the canvas and persisted the new title; the original title
  was restored and read back. The production request builder and metadata
  validator also passed this live round trip (`tmp/canvas-rename-live.log`).
  The temporary gated probe source was removed after it exited successfully.
- The cached Slack desktop source routes body edits to
  `/canvas/-/edit-document` using form-encoded collaboration requests and binary
  responses through its collaboration controller. Related reads use `/-/load-data` and
  `/-/fetch-latest`. The HTML export is not a write representation. A compatible
  section-edit and concurrent-change implementation remains open; native
  create/edit controls, comments and live collaboration are not completed by
  title editing. No messages or changes to shared/public canvases were made.
- Validation: 211 desktop tests and 407 independent core tests passed
  (one existing ignored test); the new API test checks title-only request fields,
  blank-name rejection, stale names, revoked permissions and no automatic
  mutation retries. Format and diff checks passed. Logs:
  `tmp/canvas-rename-{desktop-tests,core-tests,live}.log`.
- Signed native checks passed at both default and minimum window sizes, including
  opening Rename, blank-name disablement, failed-save draft retention, unchanged
  unconfirmed title and cancellation. Three signed screenshots were inspected:
  `tmp/agent-ui/canvas-{rename,rename-error,readonly}.png`; the readonly fixture
  hides Rename. Strict codesign verification passed. Evidence:
  `tmp/canvas-rename-{signed-native,captures,bundle}.log`.
  The signed-in client was not restarted and storage cleanup stayed stopped.


## Native standalone canvas creation

- Files now exposes New canvas with a compact name/content form. Markdown is
  submitted through the desktop `files.createCanvas` protocol. The core builder
  trims the name, preserves the content, bounds input and sends no channel or
  sharing parameters; automatic mutation retries are disabled.
- Drafts survive closing/reopening within the current account/workspace. The
  root-scoped request survives dialog dismissal. A late success retains its
  canvas ID for reopening and only automatically opens the reader if the same
  dialog visit, account, workspace and transport are still current.
- A lost response, missing created ID or connection loss blocks another submit
  and directs the user to Files before explicitly discarding the draft. This
  endpoint has no verified idempotency key. A definitive API rejection retains
  the draft for a manual retry. No raw Slack error or credentials reach the UI.
- This adds creation with initial Markdown content. Editing an existing body,
  rich visual authoring, sharing controls, comments, collaboration, and durable
  canvas drafts across an application restart remain open.

- Live production-builder probes created one populated canvas and one blank
  canvas. Both were verified owned by the signed-in user, `is_public: false`,
  `org_or_workspace_access: none`, and zero channel/group/DM shares. Populated
  content preserved heading, bold text, Unicode and checklist states; both
  reopened through the native parser. The newest/owned/canvas Files query found
  the created document. Task-owned receipts are in
  `tmp/canvas-create-{live,empty}-receipt.json`, with sanitized evidence in
  `tmp/canvas-create-live.log`. The temporary probe source was removed after its
  successful exit. No messages were sent.
- 213 desktop tests and 408 core tests passed (one existing ignored core test).
  Tests cover title/content requests, nonretry behavior, malformed created IDs,
  account/workspace isolation and uncertain creation outcomes. See
  `tmp/canvas-create-final-tests.log` and `tmp/canvas-create-core-tests.log`.

- Final signed native checks passed at normal and minimum widths: empty-name
  validation, Unicode title/body retention, offline failure, close/reopen and
  explicit draft discard. Captures of the form, actual 668x400 minimum window,
  uncertain-response state and Files entry point were inspected under
  `tmp/agent-ui/canvas-create*.png` and `tmp/agent-ui/files.png`. The minimum
  window keeps actions visible with a scrolling form body. Fixed inherited
  primary-button margin and removed the outdated Open in Slack canvas-row hint.
  Evidence: `tmp/canvas-create-{signed-native,captures,bundle-final}.log`.
  Strict codesign verification, formatting and diff whitespace checks passed.
- Signed-in process 27205 remained running throughout; the build was not
  installed into or used to restart that live session. No storage cleanup ran.
  End-to-end signed-in UI submission and live multi-account/network races remain
  separate from the verified private API and disconnected native UI checks.


## Channel link bookmarks

- Added a native Bookmarks conversation tab with scoped loading, refreshing,
  opening links, adding/editing link names and URLs, and a named removal
  confirmation. Existing folders and other bookmark types remain visible; link
  children retain their parent label. Folder management, reordering, emoji
  customization and non-link resource actions remain open.
- The desktop protocol uses `channel` and `bookmark` parameters, as seen in
  Slack's cached shipped client. `bookmarks.list` reads include folders and
  legacy workflow entries. Read retries are enabled; mutations never retry
  automatically. URL validation permits credential-free HTTP/HTTPS links.
- Edits/removals reread the server list before writing and reject changed or
  missing originals. The endpoint has no atomic compare-and-set, so a change
  simultaneous with the final write can still race. Emoji/icon/folder parameters
  are omitted from link edits. Writes run at root scope and are guarded by the
  account, workspace, transport, channel state and operation generation.
- A failed submit retains the draft. Uncertain write outcomes require reviewing
  the refreshed list and cancelling the old draft before submitting another
  mutation. Reconnect and `bookmark_added`/`bookmark_updated`/`bookmark_removed`
  events refresh the visible channel; old realtime connections and unrelated
  channels cannot trigger the refresh. Bookmark events do not rebuild the
  visible transcript. Realtime behavior is covered by scoped tests and protocol
  evidence; cross-client live rendering remains to be verified.
- Private production-API add/list/edit/remove round trip succeeded in freshly
  verified `#echotestchamber` (C0AQDL3V5GU). The test bookmark was removed and
  absence read back; original entries were checked for preservation. No messages
  were sent. Evidence: `tmp/bookmark-probe.log` and
  `tmp/bookmark-live-receipt.json`.
- While reviewing mutation errors, corrected canvas creation's treatment of
  `internal_error`, `fatal_error` and `service_unavailable`: these can indicate
  partial success, so another creation is blocked pending inspection. Added
  regressions to the existing uncertainty test. Slack documents possible partial
  success under [bookmark method errors](https://docs.slack.dev/reference/methods/bookmarks.add/).

- A second private probe verified that editing a link without emoji parameters
  preserves its assigned emoji. The test link was removed and the prior list
  restored (`tmp/bookmark-metadata-probe.log`). An initial folder probe used
  `folder` and failed; a subsequent read verified no matching folder remained.
  Slack's cached `keka` module identifies the desktop folder type as
  `local_folder`; the renderer and fixtures now recognize the real type.
- 214 desktop tests and 412 core tests passed (one existing ignored core test).
  Signed native checks passed at default and minimum sizes for list rendering,
  invalid/unsafe URL rejection, failed-submit draft retention, populated edit
  fields and named removal cancellation. All five screenshots were inspected:
  `tmp/agent-ui/bookmarks{,-narrow,-edit,-remove,-error}.png`.
  Logs: `tmp/bookmarks-{final-tests,core-tests,signed-native,captures}.log`.

- The corrected `local_folder` private probe succeeded. A task-owned child link
  kept both its emoji and folder parent after a production-builder title/URL
  edit. The child and folder were removed, absence was read back, and original
  entries were unchanged. Evidence: `tmp/bookmark-folder-probe.log` and
  `tmp/bookmark-folder-receipt.json`. Temporary probe sources were removed after
  confirmed successful exits. Folder creation/reordering controls remain absent
  from the native UI even though the desktop folder protocol is now verified.

- Rebuilt and strictly verified the signed bundle after the folder-type fix.
  Native interaction checks passed again at normal and minimum widths; the final
  normal/minimum captures were inspected (`tmp/bookmarks-final-captures.log`).
  Focused core folder/ordering/scope tests passed after the fixture correction.
  Formatting and diff whitespace checks passed. Live signed-in process 27205
  was left running, no real OS keys were sent, and no storage cleanup ran.
  Signed-in end-to-end bookmark UI writes and cross-client live events remain
  unverified; private API evidence and disconnected native UI evidence are
  separate checks.


## Bookmark folders and ordering

- Added folder creation/rename, hierarchical rows with collapse/expand, link moves
  into/out of folders, and explicit before/end placement for links and folders.
  Folder nesting is not offered because that write behavior is unverified.
- Verified the desktop `bookmarks.reorder` protocol in freshly checked private
  `#echotestchamber`: `center`, `bookmark_type`, optional `parent_id`, `left` and
  `right`. Root moves must omit `parent_id`; an empty value is rejected. Moves
  into folders, before another child, back to root, folder ordering and folder
  rename all read back successfully. All three task artifacts were removed and
  absence verified. Evidence: `tmp/bookmark-order-{probe,inspect,resume}.log` and
  `tmp/bookmark-order-receipt.json`. Production request builders have parameter
  tests; these protocol probes used direct desktop requests.
- Removing a folder cascades to its contained bookmarks. The confirmation shows
  the count, and the preflight checks the exact confirmed descendants as well as
  the folder. A new, moved, renamed or missing child blocks deletion. This is a
  preflight safeguard, not an atomic server transaction; concurrent writes
  after the read can still race. Linked documents/websites are not deleted.
- Move preflights derive current sibling anchors and reject a changed source,
  missing destination or missing position anchor. Disconnected failures retain
  the selected folder and position. API writes remain nonretrying and guarded
  by the account/workspace/channel/transport generation.
- Before the final removal regression, 214 desktop tests and 415 core tests
  passed (one existing ignored test). All six focused bookmark regressions then
  passed, including the new cascade-deletion safeguard
  (`tmp/bookmarks-organize-regression.log`).

- Signed native interaction checks passed at normal and minimum window sizes:
  list and URL validation, link/folder draft retention while offline, folder
  rename/collapse/expand, destination/position selection and reset, root-only
  folder moves, and cancelling cascading removal. The initial harness read the
  confirmation before its render completed; it now waits for the editor. Final
  evidence: `tmp/bookmarks-organize-native-final.log`. Strict code-signature
  verification passed after rebuilding.
- Inspected all five native captures: `tmp/agent-ui/bookmarks.png`,
  `bookmarks-narrow.png`, `bookmarks-folder.png`, `bookmarks-folder-remove.png`
  and `bookmarks-move.png`. The minimum layout retains readable rows and action
  controls without horizontal overflow. Capture log:
  `tmp/bookmarks-organize-captures.log`.
- Formatting and diff whitespace checks passed. Existing signed-in PID 27205
  remained running and its canonical agent answered ping. No storage cleanup,
  OS key events or message sends were performed. Signed-in end-to-end UI writes,
  cross-client realtime, emoji controls and non-link resource actions remain
  open; full Slack parity is not established.


## Text, code and audio file previews

- Extracted the shared conversation/Files attachment viewer into
  `src/desktop/file_preview.rs`. Text files, source snippets, JSON, XML, YAML and
  related text MIME types render as inert text with preserved whitespace, Geist
  Mono, scrolling and optional line wrapping. The dialog names the file and
  keeps its close control visible for long names.
- The core decoder accepts UTF-8 and BOM-marked UTF-16, rejects binary controls
  and malformed encodings, and bounds previews to 256 KiB / 2,000 lines with a
  visible truncation notice. Original download bytes remain untouched. Empty
  and unreadable files have explicit states. No server HTML is injected.
- Audio files have native playback controls. Audio/video sources remain stable
  when unrelated assets arrive, avoiding playback resets on global media epoch
  changes. Preview loading uses the existing authenticated media registry and
  account isolation. The existing 64 MiB media-download limit remains; this does
  not establish large-file download parity.
- Full suites passed: 214 desktop tests and 418 core tests, with one existing
  ignored core test (`tmp/file-preview-{desktop,core}-tests.log`). New decoder
  coverage includes literal hostile markup, Unicode/UTF-16, invalid/binary
  content and truncation without splitting Unicode.

- A private `.txt` upload in freshly verified `#echotestchamber` exposed Slack
  returning `application/octet-stream` / `binary` metadata despite valid UTF-8
  content. Added a conservative known-text-extension fallback in both Files and
  message projections, still validating the bytes before rendering. Four focused
  file-preview regressions passed (`tmp/file-preview-regression.log`).
- The same private file downloaded byte-identically (125 bytes), preserved
  Japanese/emoji and literal script markup through the production decoder, and
  was deleted. Slack confirms deletion via `file_deleted`, not `file_not_found`.
  Receipt: `tmp/file-preview-private-receipt.json` (F0C9HCV3JQG,
  `removed_and_verified`); logs: `tmp/file-preview-private-{final,cleanup}.log`.
  No duplicate upload was created. Temporary probe source removed after exit.
- All seven signed native checks passed: text, minimum-width text, long file,
  UTF-16, empty file, binary rejection and audio. The audio check decoded a WAV,
  played/paused it, and verified playback survives an unrelated media epoch
  increment. Inert markup, wrapping, untouched downloads and bounded layout were
  checked. The first empty-file eval returned no receipt; the focused rerun and
  full final sweep passed. That original harness failure is retained in
  `tmp/file-preview-native.log`; final evidence is
  `tmp/file-preview-native-final.log`.
- All seven `tmp/agent-ui/file-preview-*.png` captures were inspected. Strict
  signed-bundle verification passed. PDF/office previews, syntax highlighting,
  additional encodings/codecs, signed-in UI clicks and large original downloads
  remain unverified or missing; this is not full file-preview parity.

- Final full suites after the MIME fallback passed: 214 desktop tests and 419
  core tests, one existing ignored test. Logs:
  `tmp/file-preview-{desktop,core}-final-tests.log`. The existing image-viewer
  capture also passed visual review after extraction
  (`tmp/file-preview-image-regression.log`). Formatting and diff whitespace
  checks passed. Canonical agent ping succeeded for the still-running signed-in
  client, and discovery was restored after fixture captures. No storage cleanup
  or OS key events ran.


## Local PDF viewer

- Bundled Mozilla PDF.js 6.4.299's legacy display/worker build, fonts, CMaps,
  profiles and image decoders (4.9 MB), with upstream licenses and the npm SHA-512
  integrity pin in `assets/pdfjs/SOURCE.txt`. `scripts/vendor-pdfjs.py` reproduces
  the assets without npm install or package scripts. Source:
  https://mozilla.github.io/pdf.js/getting_started/ and the pinned npm release.
  This version is newer than the 6.2.108 fix listed in
  https://github.com/mozilla/pdf.js/security/advisories/GHSA-hq66-cqwq-w95j.
- PDFs load on demand from the app's own asset handler. Original bytes are
  available only for the currently open PDF viewer's opaque media ID. Requests
  for closed/other viewers and traversal paths are rejected. Document scripts,
  action links and scripting sandbox/QuickJS resources are not activated or
  bundled. Eval is disabled in PDF.js document parsing.
- Added page navigation/jumps, fit-width and zoom controls, rotation, selectable
  text and a password-entry path. One page renders at a time; superseded renders
  are cancelled, raster resolution is bounded, and closing releases the loading
  task, worker, canvas and listeners. The renderer is platform-neutral; native
  verification in this pass is macOS only.
- The first signed native pass verified real worker parsing, three-page
  rendering, selectable text nodes, page boundaries/jumps, zoom, rotation,
  invalid page input and disposal at normal/minimum window sizes
  (`tmp/pdf-preview-native.log`). The fixtures contain distinct page content and
  a PDF script action which is never invoked by the display-only renderer.
- A private upload in freshly verified `#echotestchamber` downloaded identically
  to the three-page fixture and was removed; `files.info` verified deletion.
  Receipt: `tmp/pdf-preview-private-receipt.json`, file F0C87NG4U4X;
  log: `tmp/pdf-preview-private.log`. The temporary probe source was removed
  after a successful exit. No duplicate artifact or other-channel write occurred.
- 216 desktop and 419 core tests passed, one existing ignored core test
  (`tmp/pdf-preview-{tests,core-tests}.log`). Added isolation/path tests and PDF
  extension fallback for generic MIME metadata. The existing authenticated media
  download limit still applies. Password handling, complex font/image codecs,
  PDF search, links/annotations/forms, printing, Office documents and signed-in
  UI clicks remain unverified or incomplete. This does not establish full PDF
  or Slack parity.

- The expanded native harness exposed a lifecycle race: an unawaited async IIFE
  could finish the Dioxus eval before it sent its receipt (`Err(Finished)`). PDF,
  text/audio and bookmark harnesses now explicitly await their async checks.
  Initial capture review also found the minimum PDF window hiding Download and
  malformed PDFs leaving an empty canvas; the PDF scroll region now leaves room
  for actions and hides failed page placeholders. The final sweep follows these
  fixes and includes actual text selection, rapid page changes and close during
  rendering.
- During this pass, previously live PID 27205 was found absent; `pgrep` found no
  running Super Platinum process and the canonical agent socket refused
  connections. No matching crash report was found. The exit cause is unknown;
  this run did not issue a stop or restart of that signed-in process. Native
  verification continues with task-owned disconnected fixtures.
- Final signed sweep passed all 12 native checks: three PDF cases, seven
  text/audio cases and both bookmark layouts. Logs:
  `tmp/pdf-preview-native-final.log`, `tmp/pdf-related-file-checks.log` and
  `tmp/pdf-related-bookmark-checks.log`. Inspected the final normal, minimum-size
  and malformed PDF captures under `tmp/agent-ui/pdf-preview*.png`; Download
  remains visible at minimum size and malformed files show the fallback without
  a blank page. Strict bundle signature verification, both formatting checks and
  `git diff --check` passed. Password-protected and complex documents remain
  outside the verified coverage above. The canonical agent still refuses
  connections; the signed build is ready, but no signed-in relaunch was performed.

## Original filenames through file previews

- Fixed both Files and message attachments dropping Slack's original `name` in
  favor of its editable display `title`. The rich media node and viewer now keep
  `download_name` separately; Download receives the original sanitized filename,
  including its extension and Unicode, while headings retain the Slack title.
  Blank message-attachment titles now use the same fallback as Files.
- Native fixtures open a file titled `Release notes`, originally named
  `release-notes-日本語.html`, through the actual Files row and message attachment
  handlers. Both verify the displayed title, original contents and Download's
  filename. The OS Save dialog itself was not automated in this pass.
- 216 desktop tests passed (`tmp/file-filename-tests.log`). Nine file-preview,
  three PDF and two bookmark native checks passed in the signed macOS bundle
  (`tmp/file-filename-native-final.log`, `tmp/file-filename-pdf-native.log`,
  `tmp/file-filename-bookmarks-native.log`). No core code changed.
- The first sweep still exposed `EvalError::Finished` in a fast binary-file case,
  despite the prior top-level await fix. The pinned Dioxus desktop evaluator is
  owned by its query slot; completion can release the channel before Rust reads
  the final receipt. File, PDF and bookmark checks now wait for an explicit Rust
  acknowledgement after sending their final result. The audio check also waits
  for acknowledgement of the media-epoch update. This fixes the test protocol
  rather than retrying a lost result. Original failure is retained in
  `tmp/file-filename-native.log`; the final sweep above passed.
- Final capture review also caught and corrected `1 lines` to `1 line` in text
  previews. No Slack messages, uploads, other external writes or storage cleanup
  were performed in this pass. Full file/Slack parity remains incomplete.
- Rebuilt after the singular-label correction, reran both native filename paths,
  and inspected both final `tmp/agent-ui/file-preview-renamed-*.png` captures.
  Final receipt log: `tmp/file-filename-captures-final.log`. Strict signature,
  formatting and diff-whitespace checks passed. Capture-owned processes and
  discovery markers were released; no signed-in client was relaunched.

## Search within PDF previews

- Added a document-wide, case-insensitive literal search field with match count,
  next/previous navigation and wraparound. Phrases span PDF text items and line
  breaks; regex characters remain literal. Current-page matches and the active
  match have distinct highlights that retain the underlying selectable text
  through zoom and rotation. Match navigation scrolls only the PDF pane.
- Cmd/Ctrl+F focuses PDF search. Enter/Shift+Enter and Cmd/Ctrl+G navigate matches;
  Escape in a nonempty field clears it without dismissing the file. IME
  confirmation does not advance results. Closing the viewer releases these
  shortcuts and restores conversation search.
- Text extraction uses the bundled PDF.js worker, one page at a time. Input is
  debounced for 180 ms; only per-page counts are retained for the whole document,
  with match offsets and highlights computed for the displayed page. Query
  revisions discard superseded work, and closing cancels queued work and releases
  listeners. Pages whose text extraction fails are disclosed as unavailable,
  rather than presenting an unqualified no-match result.
- 216 desktop tests passed (`tmp/pdf-search-tests.log`). Three Node matching
  tests passed (`scripts/check-pdf-search.mjs`, `tmp/pdf-search-js.log`), covering
  literal regex characters, Unicode, case, line boundaries and source offsets.
  Core code was unchanged. The native signed sweep passed normal/minimum windows,
  invalid PDFs, and a 120-page document (`tmp/pdf-search-native-verified.log`).
  It checks same-page matches, cross-page wraparound, multi-line highlighting,
  rotation/text preservation, no-match/clear states, keyboard/IME behavior,
  replacement searches, close during extraction, and restored global shortcuts.
- The first expanded long-document sweep timed out on a transient progress
  predicate (`tmp/pdf-search-native-final.log`). The harness now observes the
  search-start DOM transition before replacing/closing, rather than polling that
  short-lived state. The corrected full sweep and two additional long-document
  runs all passed. The 120-page replacement search measured 255, 254 and 254 ms
  including debounce and test polling (`tmp/pdf-search-long-repeat.log` for the
  two repeats). These are simple owned-text fixtures, not general PDF performance
  or a comparison with Slack.
- Search is local and has no external writes. OCR for scanned-only PDFs,
  dehyphenation/accent normalization, case/whole-word search options, complex
  font/image documents, partial-page extraction failures and password-protected
  documents remain missing or outside native coverage. Full PDF/Slack parity is
  still unproven.
- Final visual review reclaimed the old Download margin for the PDF pane and
  standardized its button to 32 px. Multi-line matches now scroll their combined
  bounds into view when they fit; larger matches begin at the first fragment.
  The minimum-window native check verifies the entire match is visible.
- Final validation after the geometry changes: 216 desktop tests and all four
  native PDF cases passed (`tmp/pdf-search-tests-final.log`,
  `tmp/pdf-search-native-final-geometry.log`). The final 120-page search measured
  254 ms with the same fixture/polling caveats above. All four final captures
  (`tmp/agent-ui/pdf-search{,-narrow,-empty,-error}.png`) were inspected, including
  both highlighted lines visible in the minimum window. Capture receipts are in
  `tmp/pdf-search-captures-final.log`. Three JavaScript tests, formatting,
  whitespace and strict bundle signature checks passed. No signed-in app was
  relaunched, and task-owned fixture processes/discovery markers were released.


## File row actions and deletion reconciliation

- Main Files and channel Files now offer Copy link, Open in Slack, and deletion
  of files uploaded by the signed-in user. Deletion names the file and explains
  its scope before confirmation. Cancel and Escape preserve it; Escape restores
  focus to the menu trigger. Menus use sibling buttons, never nested buttons.
- Link actions accept credential-free HTTPS Slack permalinks only and never
  enable public sharing. Clipboard success requires an acknowledged WebView
  receipt. A failed clipboard write reports failure rather than success.
- A confirmed delete first fetches `files.info` and verifies the exact file ID,
  current uploader and displayed title. Account/workspace/transport guards reject
  stale work. The mutation outlives row unmounting; writes to disposed component
  state are guarded. Ambiguous failures retain the row and offer Check status;
  `file_not_found` alone is not treated as proof of deletion. Server permissions
  remain authoritative; admin deletion of other uploaders' files is not exposed.
- Request/event references: [files.delete](https://docs.slack.dev/reference/methods/files.delete/),
  [files.info](https://docs.slack.dev/reference/methods/files.info/), and
  [file_deleted](https://docs.slack.dev/reference/events/file_deleted/). Both
  documented ID keys (`file_id` and RTM `file`) are handled under the existing
  realtime generation guard. Delete requests are not automatically retried.
- Confirmed deletion removes both scoped Files entries, replaces cached file
  attachments with a Deleted file marker, and preserves surrounding message
  text. Workspace deletion IDs mask late history/search projections and persist
  through warm boot. In-flight or lagging Files searches filter these IDs rather
  than resurrecting rows. Conversation/thread/search/pin/thread-feed/activity/DM
  message copies are reconciled where cached. A matching file or canvas preview
  closes, and old rich nodes cannot reopen a confirmed deleted file.
- 217 desktop tests and 422 core tests passed (one ignored benchmark):
  `tmp/file-actions-tests-final.log`, `tmp/file-actions-core-tests.log`. Coverage
  includes ownership/link policy, message/thread reconciliation, stale searches,
  account isolation, cache compatibility, mutation request shape and stale socket
  generations. The first core run caught a missing field in a test workspace
  fixture; that fixture was corrected before the full passing run.
- All three signed native action checks passed: main Files, minimum-window Files,
  and minimum-window channel Files. They verify ownership controls, unsafe-link
  rejection, stubbed clipboard success/failure, confirmation/cancel, offline
  retention, Escape/focus, account reset and closing the deleted file's preview.
  The OS clipboard, external browser and real keyboard were not used.
- Visual review caught the channel confirmation falling below its small results
  pane. Channel spacing is now tighter, opening/changing the action panel focuses
  its first available control and scrolls the file list, and the native check
  requires the whole fixture confirmation to be visible. Final screenshots
  `tmp/agent-ui/file-actions{,-narrow,-channel-narrow}.png` were all inspected;
  final receipts are `tmp/file-actions-captures-final.log`. The earlier channel
  fixture failure in `tmp/file-actions-native.log` used the pre-fixture bundle;
  final captures ran against the rebuilt signed bundle.
- Two signed filename-preview regressions and the normal PDF search/preview
  regression passed (`tmp/file-actions-preview-regression.log`,
  `tmp/file-actions-pdf-regression.log`). Formatting, diff whitespace and strict
  bundle signature checks passed. The bundle is built, but a signed-in client
  was not restarted. Playwright still reports Transport closed.
- Live API proof used only newly uploaded task file F0C880499NK in freshly
  verified private #echotestchamber. Its immediate file-info response initially
  lacked the expected private-share entry, so the first probe stopped before
  deletion. The follow-up read verified the same file's owner/title, private
  channel share and lack of public shares, then used the production delete
  request and verified Slack's deleted state. Receipt:
  `tmp/file-actions-private-receipt.json` (`removed_and_verified`); proof log:
  `tmp/file-actions-private-verification.log`. The temporary probe source was
  removed. No existing user files were deleted and no storage cleanup ran.
- Remaining evidence gaps: signed-in UI mutation, actual cross-client deletion
  delivery, live permission-denial and uncertain-response paths, and account
  changes while network requests are in flight. File rename/sharing/save actions,
  message/viewer action menus, Office previews and larger downloads remain beyond
  this slice. Full file/Slack parity is not established.


## File actions from attachment viewers

- The shared attachment viewer now exposes the same Copy link, Open in Slack,
  and confirmed own-file deletion controls as Files rows. Both Files-opened and
  message-opened attachments carry a small typed `FileIdentity` (ID, validated
  permalink, uploader) through the rich node and viewer projection. Inline media
  without a Slack file identity retains Download without unsupported Slack file
  actions. Original download names remain separate from display titles.
- Opening a preview does not add a metadata request: actions use the attachment
  metadata already loaded. Deletion still uses the existing fresh-info ownership
  and title preflight. The menu preserves the preview DOM/canvas while opening,
  cancelling and copying, rather than remounting the content.
- Viewer and row controls have distinct element IDs. Action panels retain their
  account/workspace/file key. An account epoch change discards the old local
  confirmation; confirmed deletion closes the matching viewer. The actions keep
  the same offline failure and uncertain-write handling as the Files browser.
- Escape in the action panel closes only that panel and restores the More button;
  the next Escape can close the viewer. The native shortcut bridge and keyboard
  coordinator recognize the local Escape marker, so the parent dialog is not
  dismissed or marked as closing by a nested menu's Escape. Optional event targets
  also remain valid in the standalone keyboard harness.
- The footer uses compact 32 px controls and preserves original filename delivery
  to Download. Screenshot review found that expanding PDF actions at minimum
  height scrolled away the title and Close control. The viewer is now a bounded
  column with a fixed header and independently scrolling body. The menu scrolls
  within that body, and the native check requires the complete confirmation and
  Close control to remain visible.
- Final validation: 218 desktop tests (`tmp/viewer-actions-tests-final.log`),
  422 core tests with one ignored benchmark (`tmp/viewer-actions-core-tests.log`),
  and 25 keyboard JavaScript tests (`tmp/viewer-actions-keyboard-tests.log`) passed.
  Tests cover identity extraction and link rejection, current workspace ownership,
  account epoch changes and tombstoned viewer targets. The initial keyboard run
  caught target-less synthetic Escape events; optional target access fixed that
  regression, and the new local-Escape case passes with the existing suite.
- Four signed native viewer checks passed: open from Files, open from a message,
  minimum-window message viewer, and minimum-window PDF viewer. They check exact
  copied permalinks with a stub clipboard, original download filenames, distinct
  IDs, preserved preview nodes, confirmation/cancel, both Escape levels, offline
  retention, scope reset, deletion closure and geometry. Final receipts:
  `tmp/viewer-actions-captures-final.log`. All four final
  `tmp/agent-ui/viewer-actions-{files,message,narrow,pdf}.png` captures were inspected.
- Nine existing file-preview checks, all four PDF checks, and all three Files-menu
  checks also passed against the same signed bundle (20 native checks total).
  Logs: `tmp/viewer-actions-preview-regression.log`,
  `tmp/viewer-actions-pdf-regression.log`, and
  `tmp/viewer-actions-file-menu-regression.log`. Formatting, diff-whitespace and
  strict signature checks passed. No new Slack writes, OS clipboard changes,
  storage cleanup or signed-in process restart occurred in this pass.
- Remaining gaps include direct attachment hover/context menus, rename/share/save
  file actions, signed-in UI mutation, actual cross-client deletion delivery and
  live uncertain-write/permission cases. Office previews, larger originals and
  the broader parity ledger are still open. This does not establish full Slack
  feature parity or a general performance comparison with Slack.


## Forwarding delivery receipts and retry safety

- Multi-destination forwarding already existed. It now tracks each destination as
  sending, forwarded, rejected, or unconfirmed. Confirmed and uncertain recipients
  are excluded from ordinary submission and search results, including after the
  note changes. Definite failures retain their selection and can be retried.
- Added renderer-neutral delivery state with original request IDs and rich blocks.
  Known rejections may be retried; changed notes receive a new ID only after a
  definite rejection. Ambiguous outcomes require an explicit duplicate warning
  and Resend original action. An earlier ambiguous attempt stays ambiguous when
  a later resend is rejected. This does not assume Slack deduplicates request IDs.
- Response channel and timestamp are validated before marking success. Late
  receipts with a different request ID are ignored. Retry-After is enforced for
  write retries. Offline includes timeouts and connection resets, so a failed
  in-flight write is treated as uncertain; a known offline link is rejected before
  starting a batch. Corrected misleading transport documentation accordingly.
- Check delivery reads up to three recent history pages in the destination and
  requires the exact request ID, current sender and valid timestamp. No match,
  inaccessible history, or a truncated window leaves delivery unconfirmed.
  The original note remains frozen until uncertainty is resolved. This state is
  local to the dialog; persistent recovery after closing/restarting remains open.
- Async batches keep a local ledger even if the original dialog disappears, with
  account/team/transport guards and a completion toast. Successful destinations
  still refresh the visible conversation. Missing credentials now show an
  actionable error while retaining the note and recipients.
- Added shared keyboard containment and local Escape handling to the forward
  dialog. Rich-note reads and clipboard receipts now keep the WebView eval alive
  until Rust acknowledges receipt. Partial results use compact rows with a
  scrollable body, fixed header/footer and focused resend confirmation.
- Read-only Slack evidence: the existing task-authored forward
  1791787872.938599 was found in freshly verified private #echotestchamber
  (C0AQDL3V5GU). Its current-user ownership and a nonempty client_msg_id were
  verified without logging message content or credentials. This establishes that
  history exposes that field, not server deduplication or a newly simulated lost
  response. Receipt: tmp/forward-private-read.log. The temporary probe was removed.
- Rust validation: 218 desktop tests (tmp/forward-tests-final.log), 426 core tests
  with one ignored benchmark (tmp/forward-core-tests.log), and 25 keyboard tests
  (tmp/forward-keyboard-tests.log) passed. Core regressions cover partial batches,
  changed notes after rejection, explicit resend identity, prior uncertainty,
  response mismatch, stale IDs, Retry-After and positive history evidence.
- All four final signed native checks passed: forward, forward-narrow,
  forward-partial and forward-partial-narrow (tmp/forward-captures-final.log).
  Each exercises 12 acknowledged rich-note reads, focus wrapping, multi-recipient
  selection or mixed delivery results, note/selection retention, offline errors,
  Escape and geometry. Partial cases also exercise resend confirmation/cancel,
  frozen notes and offline delivery checks/resends. An initial check used an
  overly broad destination selector; scoping it to result rows fixed the harness.
  All four captures in tmp/agent-ui/forward*.png were inspected, including the
  complete resend confirmation at minimum height. Final bundle log:
  tmp/forward-build-verified.log. Strict signature, both formatting checks and
  git diff whitespace validation passed.
- Remaining gaps: signed-in UI mutation and real multi-destination/ambiguous
  response recovery, persistent receipts, new-person DM discovery, scheduled
  forwarding and full rich-note controls. No new Slack writes, real clipboard
  changes, storage cleanup or signed-in process restarts occurred in this pass.
  Full Slack parity and a general performance comparison remain unproven.


## Forwarding to people outside the current DM list

- The forward picker now searches cached people and the workspace directory as
  well as conversations. It reuses the new-message picker's debounced lookup,
  cancellation, account/team/transport scope, local ranking and selection-only
  hydration. Forward-dialog identity is part of the scope, so reopening a forward
  cannot accept an earlier visit's result. Ordinary shell ticks still do not
  restart searches. Search also supports @people and #channels.
- A known DM is reused and deduplicated against directory hits. Selecting a new
  person adds a chip and hydrates their profile metadata without opening a DM or
  navigating. Forward resolves those people through conversations.open, validates
  the returned DM identity, remembers the channel and then submits the original
  message and note. A newly resolved channel replaces the pending person chip.
- Existing message receipt state still suppresses confirmed/uncertain destination
  retries. Resolution failures retain the affected recipients and note while
  other prepared destinations can finish. Rate limits stop further DM opens and
  gate subsequent opening attempts until Retry-After. Account/transport changes
  stop the batch. Request/note snapshots survive the originating dialog closing.
- Directory loading and retry states are exposed in the picker. Result handles
  distinguish new people. Existing channel/DM navigation and the source message
  remain unchanged during resolution. Sparse DM responses hydrate the known
  participant while mismatched users, non-DM IDs and archived responses are
  rejected before forwarding.
- 221 desktop tests passed (tmp/forward-people-tests-final.log), including new
  regressions for uncached users, known-DM deduplication, channel-only filtering,
  deactivated users, DM response validation, preserved source navigation and
  stale directory visits. Existing directory runtime subscription tests passed.
  25 keyboard tests passed (tmp/forward-people-keyboard-tests.log).
- An earlier partial-result run opened a responsive native window with a blank
  WebView. The same fixture rendered without the check enabled. The forwarding
  check now starts once from the mounted dialog instead of a root startup future,
  keeping its long evaluation after the interpreter's initial DOM handshake.
  All six final signed checks passed, including initial/reopened recipient focus
  (tmp/forward-people-captures-final.log): standard/narrow forwarding, partial
  results at both sizes, and directory people at both sizes. All six final PNGs
  in tmp/agent-ui/forward*.png were inspected. The new checks verify uncached
  selection, hydration without DM creation, mixed person/channel recipients,
  @/# filtering, known-DM deduplication and offline retention. The four existing
  forwarding regressions also remain green. Signed build:
  tmp/forward-people-build-final.log; strict signature and both formatting checks
  passed, along with git diff whitespace validation.
- Screenshot review exposed the optional note editor stealing focus and scrolling
  the recipient field out of view at minimum height. RichEditor now accepts an
  autofocus option, enabled by default for its existing consumers and disabled
  for forwarding. Six real-browser rich-editor tests passed, including the new
  opt-out and existing default focus/formatting/serialization/paste behavior
  (tmp/forward-people-rich-editor-tests.json). The native check now also requires
  recipient focus on first open and after reopening.
- No Slack messages or new DMs were created during this pass. Real new-person DM
  forwarding, directory permission/rate-limit responses and mixed network
  failures remain unverified. Scheduled forwarding, full rich-note controls and
  persistent recovery remain open; this is progress toward the broad parity goal.

## Uploaded file renaming

- Added Rename to owned uploaded-file menus in Files, channel Files and attachment
  previews. The compact inline form labels the field Name in Slack, preserves
  Unicode and rejects blank/control-character names. Canvas/list and external-file
  editors remain separate. The form explains that the download filename may change.
- The title-only files.edit request uses exactly file and title, as observed in
  Slack's shipped filesEditFetcher. It is not automatically retried. Fresh
  files.info verifies ownership and the original display title before writing;
  a different remote title requires reopening the form. A second files.info
  confirms the exact requested title before reporting success. This preflight is
  not an atomic compare-and-swap against simultaneous external edits.
- Lost/unknown responses retain a frozen draft with Check status. A read showing
  the old title does not authorize another write. Known rejections retain the
  editable draft, and rate-limit responses gate further attempts in that form.
  The existing delete path now also treats Offline responses as uncertain:
  transport timeouts and connection resets can follow completed writes.
- Confirmed title metadata is reconciled through the shared cache visitor for
  messages, threads, search, pins, thread/activity feeds, DMs and file lists.
  In-flight file searches are invalidated; deleted files cannot be resurrected.
  The viewer retains its media identity and uses Slack's returned download name.
- Native interaction checks cover owned/other-uploader menus, blank/unchanged
  names, Unicode input, offline draft retention, uncertain status without resend,
  Cancel/Escape/focus, viewer title reconciliation and stable preview DOM, with
  normal and minimum-size channel/global Files fixtures.
- Live private proof exposed two assumptions during verification: upload shares can
  appear after files.completeUploadExternal returns, and files.edit title also
  changes name. In private #echotestchamber (C0AQDL3V5GU), the owned task upload
  F0C8RHR2KBK returned the exact title Renamed verification · 日本語 and the
  server-normalized filename Renamed_verification_____________.txt. The client
  now reconciles both returned fields instead of retaining the original name.
  Readback also verified current-user ownership. Both task uploads from this
  pass (F0C8MLDE19C and F0C8RHR2KBK) were deleted and confirmed tombstoned or
  file_deleted. Receipts: tmp/file-rename-private-attempt1-receipt.json and
  tmp/file-rename-private-receipt.json; final cleanup log:
  tmp/file-rename-private-cleanup-final.log. The temporary probe was removed.
- 223 desktop tests passed (tmp/file-rename-tests-verified.log); 427 core tests
  passed with one ignored benchmark (tmp/file-rename-core-tests-final.log).
  Coverage includes title validation, title-only/non-retry request fields,
  conflicting/unchanged/confirmed titles, ambiguous failures, scope isolation,
  download-name reconciliation, preserved media fields and stale file searches.
- All five final rename captures passed (tmp/file-rename-captures-complete.log):
  standard/narrow/channel-narrow previews, plus global and narrow channel inline
  forms. Seven existing file/viewer action checks also passed, including the PDF
  viewer (tmp/file-rename-regression-captures.log). All 12 final PNGs were inspected.
  The shortest channel Files viewport initially clipped the rename label; a
  compact grid for short-window inline forms keeps the complete form visible.
  A capture-only menu-reopen race was corrected by waiting for closure.
- Blank fixture windows also affected the existing forward and file fixtures.
  Direct fixture-only WebView diagnostics showed a loaded page/interpreter;
  activating the task-owned native process restored rendering. The rename runner
  waits for launch/activation and reasserts activation while awaiting its receipt.
  No OS keyboard events or signed-in process restarts were used. Optional
  SUPER_PLATINUM_UI_DOM_TRACE diagnostics are restricted to fixture runs.
- Final signed build: tmp/file-rename-build-layout.log. Both format checks,
  whitespace validation and strict bundle signature verification passed.
  Storage cleanup remains stopped; no commits or pushes were made.
- Remaining: file_change realtime metadata refresh, auditing Later/scheduled-file
  copies and late history/search-index results, persistent uncertain-operation
  recovery after closing/restarting, and live permission/rate-limit/lost-response
  scenarios. Preflight does not lock out concurrent external edits. Full Slack
  parity and broad performance claims remain unproven.

## Cross-client file names and Later reconciliation

- Split implementation, cache reconciliation and independent review across three
  agents; root owns integration and native verification. The session supports
  four simultaneous agents including root, within the user's maximum of 20.
- Slack's documented file_change event may contain only an ID; it requires a
  fresh files.info read for metadata:
  https://docs.slack.dev/reference/events/file_change/.
- Added a scoped, coalesced metadata refresh path for file_change. It guards
  account epoch, transport identity, workspace socket generation and per-file
  request ordering. Cached display/download names are updated without replacing
  preview media, and inaccessible files retain their warm cached state.
- Extended the shared file visitor to Later's hydrated and pending previews.
  Hydration snapshots distinguish later partial events and file edits from older
  cached patches, while workspace deletion tombstones also apply when Later is
  opened after a deletion. Scheduled drafts hold file IDs rather than duplicated
  File metadata, so they need no equivalent metadata visitor.
- Independent review focused on deletion before Later scope creation, concurrent
  history responses, partial message-field ordering, and remote events arriving
  during local rename verification. Local canvas rename also invalidates older
  metadata reads; canvas reader title synchronization remains a separate gap.
- Next audit priorities are persistent uncertain-operation receipts, measured
  thread windowing, ordinary composer draft synchronization, file share/save
  entry points, and researched canvas body editing. These are implementation
  gaps; live first-DM/group-DM, bookmark, forwarding failure and human-assisted
  huddle checks remain distinct verification gaps.
- File-sync checkpoint validation: 231 desktop tests passed
  (tmp/file-realtime-desktop-tests.log); 431 independently locked core tests
  passed with one ignored benchmark
  (tmp/later-file-reconciliation-core-tests-final.log). Both format checks and
  whitespace validation passed. Signed build and strict signature verification
  passed (tmp/file-realtime-native-build.log).
- Five signed native rename cases passed and all five PNGs were inspected
  (tmp/file-realtime-native-captures.log; tmp/agent-ui/file-rename*.png). The
  standard/narrow/channel-narrow viewer checks inject remote metadata while an
  unsent draft remains open, assert the draft and preview DOM survive, then
  verify reopening uses the remote title and download filename. This is
  disconnected native reconciliation proof, not live cross-client event proof.
- Remaining file-sync gaps include non-name metadata/content refresh, late
  history/search results outside Later, and canvas reader title updates. No
  live Slack mutations, signed-in restarts, commits, pushes or storage cleanup
  occurred in this pass. Subsequent thread-performance and operation-recovery
  implementation began after the validated signed file-sync snapshot.

## Thread windowing, file recovery and draft synchronization groundwork

- Thread projection now builds at most 100 regular message views plus the root
  and pending-message exceptions. Lightweight timestamp rows and measured
  spacers preserve automatic scrolling through loaded replies, exact targets,
  unread boundaries, author grouping and existing server pagination gaps. The
  measurement cache is bounded; selected text pins the rendered neighborhood.
- Synthetic optimized-dev projection medians over ten iterations changed from
  0.729/8.052/39.275 ms for 200/2,000/10,000 replies to
  0.340/0.434/0.897 ms. The after samples project 101 views. Evidence:
  tmp/thread-projection-before.log and tmp/thread-projection-after-final.log.
  These are projection measurements with observed run-to-run variance, not
  end-to-end frame latency or a broad client performance claim.
- File rename/delete now commit a scoped SQLite recovery receipt before dispatch.
  Account/workspace/user/file identity and an operation UUID guard resolution;
  the UUID is local bookkeeping, not a claim of Slack idempotency. Unknown
  outcomes block another write, survive menu closure/restart and retain read
  Retry-After deadlines. Exact terminal responses are idempotent across surfaces.
  Immutable receipt handles outlive their originating row and account view.
- Files exposes an Unconfirmed file changes inventory so a pending deletion
  remains accessible after its file disappears from the normal list. Status
  checks never infer failure from an unchanged title or still-visible file.
  Explicit Stop tracking records retirement, not server success/failure; it
  preserves existing cooldowns. All journal I/O runs on blocking workers and
  must succeed before dispatch. Fixtures require injected task-owned storage.
- Review additionally closed stale-write windows during journal commits,
  disposed-signal callbacks, mounted recovery rows left busy after deletion,
  stale terminal receipts, and simultaneous confirmations from multiple views.
  A preflight-only read cooldown without an operation receipt is retained through
  menu transitions but is not persisted across a full app restart.
- Added a pure draft-sync reducer preserving raw blocks, destinations, file IDs,
  stable create identity, remote revision, pending operations, conflicts and
  deletion intent. Eleven focused tests cover rapid edits, ambiguous responses,
  restart normalization and ordering races including deletion before adoption.
  It is not connected to persistence, networking or ordinary composer autosave;
  those remain required implementation work, with remote conflict semantics
  and normalization still needing real protocol evidence.
- Integrated suites passed 241 desktop tests (one ignored timing benchmark) and
  452 core tests (one ignored benchmark): tmp/recovery-thread-desktop-tests-final.log
  and tmp/recovery-draft-core-tests-final.log. Native validation follows below.
- The signed recovery runner completed separate seed, narrow-window restart and
  explicit-retirement processes; its final SQLite check retained the Unicode
  rename intent and recorded only the deletion as retired. All three captures
  were inspected. This first checkpoint exposed clipped narrow-window feedback
  and misleading pending-deletion copy; the subsequent UI changes require a
  fresh signed run with visibility assertions before being considered verified.
- Stronger thread checks exposed scroll anchoring failures that the initial
  bounded-row checks missed. The shared anchor browser suite passed eleven
  channel and six thread cases after pointer-input handling was added, but the
  signed 10,000-reply jump-to-top check still failed. Native thread scrolling is
  therefore still under investigation; the earlier receipt passes are not a
  completed scrolling gate.
- The draft adapter now uses the existing account SQLite database and one
  persistence worker. Atomic paired saves use storage revisions; preparation
  returns an operation only after commit, and restart loading durably turns
  in-flight work into uncertain work without replacing IDs. Explicit enrollment
  requires a matching durable local draft and an authoritative snapshot.
  Legacy edits targeting an enrolled draft are retained separately with exact
  settlement tokens; they cannot overwrite synchronized state or silently
  authorize an older dispatch. This remains disabled groundwork: ordinary
  composer autosave, lossless block editing and draft-aware send are not wired.
- Thirteen focused persistence tests and the independent core suite passed
  (463 passed, one ignored): tmp/draft-persistence-tests.log and
  tmp/draft-persistence-core-tests.log. The integrated desktop suite passed
  242 tests with one ignored: tmp/parallel-desktop-tests.log. Cross-view file
  status checks now refresh their durable receipt and cooldown before reading
  Slack, and uncertain rename conflicts remain checkable.
- Next draft integration must include send ownership, not only an autosave timer:
  ordinary send currently clears/checkpoints the composer and sends without a
  draft ID. A durable send intent must preserve the submitted snapshot while
  newer input remains independent; uncertain creates/sends cannot be replayed.
  Start with one top-level text-only destination and a strict reversible codec.
  Keep unsupported remote blocks raw and available through the existing rich
  editor. Foreign remote drafts need explicit validated adoption, and scheduling
  must share draft ownership to avoid creating duplicate copies. Capture ordinary
  create/update normalization and send/retirement ordering before enabling writes.
- Independent next feature: persist forwarding receipts before dispatch and
  expose unresolved deliveries after dialog closure/restart. The current forward
  dialog owns an in-memory ledger; an account switch can skip outcome updates.
  Existing immutable requests and positive history receipts provide a stronger
  starting point than the currently incomplete saved-file navigation protocol.
  Keep forwarding storage separate from ordinary composer draft ownership.
- Final signed file sweep passed all 15 cases: three recovery processes, five
  rename cases and seven file/viewer action cases. All 15 PNGs were inspected.
  The short-window recovery workspace now scrolls as a whole, keeping feedback
  and ordinary file actions reachable; native assertions require visible status
  and next action, preserved recovery across restart, unique controls, explicit
  retirement and focus on retirement/cancel. Pending deletion copy is accurate
  and its status-check button is neutral. Logs: tmp/file-recovery-native-final.log,
  tmp/file-rename-native-final.log and tmp/file-actions-native-final.log.
- The shared browser suites passed 26 checks (12 anchor, six thread, eight
  navigation). A regression reproduces pending navigation overriding pointer
  input when pointer cancellation is removed. Trace-off native repetition still
  reproduced an unexpected bottom jump, so thread measurements and queued
  navigation now carry a user-input generation to reject older work. The updated
  desktop suite passed 243 tests plus one ignored. Final native validation for
  that generation fix remains pending at this checkpoint.
- Follow-up input review: channel and thread selection pinning currently releases
  on the scroll container's mouseup handler. Check a selection drag released
  outside that container and preserve an explicit release/cancel path before
  claiming complete selection behavior for virtualized threads.
- Final thread checkpoint: queued thread navigation now also carries the native
  input generation into JavaScript and compares it with a synchronous,
  scope-resetting input counter before moving the viewport. This closes input
  arriving before the queued eval installs its event listeners. The new browser
  regression fails with that check removed and passes with it; all 28 browser
  cases pass. The final desktop suite passed 243 tests plus one ignored, and
  the rebuilt app passed strict deep signature verification.
- With tracing disabled, the final signed app passed a focused 10,000-reply case
  and two full five-case sweeps. Every top jump reached the root at scrollTop 0;
  every final bottom jump kept the exact pending/latest reply visible with
  bounded mounted rows. All five final PNGs were inspected. Evidence:
  tmp/thread-window-dom-generation-tests.log,
  tmp/thread-navigation-before-generation.log,
  tmp/thread-window-dom-generation-sweep1/ and
  tmp/thread-window-dom-generation-sweep2/.
  One intervening 200-reply run closed its pane during the check and remains
  unexplained; its failure log is retained rather than counted as a pass. The
  original intermittent offset was not uniquely attributed by a failing stack
  trace, so the proof is regression coverage and repeated native behavior, not
  a claim that every possible scroll race is eliminated. Full Slack parity and
  ordinary cross-client composer autosave remain unfinished.

### Durable forwarding and selection lifecycle (2026-10-12 continuation)

- Three parallel workers handled the forwarding journal, strict draft projection,
  and virtualized selection lifecycle; root integrated forwarding UI and reviewed
  cross-account/outcome ownership. The runtime permits three workers alongside
  root, below the user's maximum of 20.
- Forwarding now commits an immutable account/team/user/source/destination record
  before dispatch. Stored uncertainty survives restart; each explicit resend
  rotates a local attempt identity while retaining the original Slack client ID,
  rich note and history boundary. Responses persist against the original account
  before UI guards. Opening a dialog or inventory never retries a write.
- Drafts & scheduled exposes unconfirmed forwards even when their source message
  or destination is absent from cache. Review restores original rich notes,
  positive receipt checks, explicit duplicate-warning resend and explicit stop
  tracking. Retirement never claims Slack delivery. Completed earlier forwards
  do not permanently exclude a destination when a new dialog is opened.
- Read and write Retry-After deadlines remain durable; a failed response write
  also retains an in-memory delay. Every check reloads the current receipt;
  stale attempts and cross-view cooldowns cannot resolve or resend replacements.
  Review fixes added retry after loading failure, captured account/epoch identity
  for inventory actions, and a latch preventing mixed recovered notes from being
  sent through an editor still displaying a different frozen note.
- Draft projection accepts only an exact reversible subset of top-level,
  text-only, single-destination rich text. Unknown fields, entities, files,
  threads and ambiguous/combined styles remain opaque. Validated foreign remote
  adoption retains the exact raw snapshot and identity; callers still must
  establish authenticated ownership and active/unscheduled lifecycle. No new
  draft enrollment, network synchronization or ordinary autosave is enabled.
- Selection now uses one ordered document bridge for both transcripts. Mouseup
  outside the container, pointer cancellation and window blur release the pin;
  scope/gesture ownership prevents an old release from affecting a new selection.
  Navigation resets retained channel selection. Synthetic browser evidence
  reproduces six failures in the old handlers and passes seven fixed cases,
  alongside all 28 existing anchor/navigation cases. Shared anchor and navigation
  helpers are unchanged in this continuation.
- Validation so far: desktop suite 252 passed, one ignored; standalone core suite
  486 passed, one ignored; final forwarding-only core rerun 16 passed including
  the subsequently added monotonic defer helper. Signed rebuild and strict deep
  codesign verification passed. Native selection and forwarding recovery checks
  are running next; they are not counted as passed at this intermediate entry.
  Logs: tmp/forward-recovery-final-tests.log, tmp/draft-codec-core-tests.log,
  tmp/forwarding-persistence-tests.log, tmp/forward-recovery-build.log,
  tmp/selection-lifecycle-before-probes.log and tmp/selection-anchor-regressions.log.
- Next draft boundary: add immutable Send/Schedule consumption intent to the same
  draft reducer/SQLite transaction, then one desktop controller per account UUID
  and DraftKey owns checkpoint/clear/send/schedule/realtime observation. Do not
  enable a textarea debounce writer first. queue_composer currently clears via
  the legacy path; send_pending omits server draft identity and needs captured
  account/transport outcome ownership; ScheduleDialog independently creates or
  updates and then clears its source. Those paths must share the owner before
  autosave is safe. Send is not edit(None) followed by drafts.delete.
  Fake-dispatcher/temporary-SQLite acceptance should cover send during create,
  uncertain restart, newer typing during consumption, exact draft/client IDs and
  raw content, schedule reusing owned identity, no delete-before-send, stale CAS
  and disk failure causing zero dispatches, and late outcomes saved to the
  original account. Remote deletion alone must not prove a send succeeded.
  Ordinary autosave request shape/list coverage/server normalization still need
  protocol proof; existing scheduled builders hardcode a different composer flag.
- Final native forwarding recovery passed all three disconnected signed processes:
  seed, narrow restart, and explicit retirement. SQLite verification retained the
  original request/client ID/attempt ID through restart, offline actions and
  cancellation; retirement changed tracking only. The original source is absent
  from the inventory projection and the destination is deliberately uncached.
  Notes preserve bold Unicode text and newlines; both confirmations focus Cancel
  and restore the opener; narrow feedback and actions must actually be visible.
  The first fixture used a non-Slack synthetic timestamp and was corrected to a
  valid timestamp. Narrow validation then exposed clipped error feedback; the
  dialog now scrolls new errors into view. Both failures are resolved by the
  final run. All three final recovery PNGs were inspected. Evidence:
  tmp/forward-recovery-native.log and tmp/agent-ui/forward-recovery-{seed,restart,retire}.
- The six preexisting signed forwarding workflows also passed against the final
  build: ordinary, narrow, partial outcomes, narrow partial outcomes, people, and
  narrow people. Evidence: tmp/forward-recovery-regression-native.log. The signed
  selection check passed with selected text retained, both native selection pins
  false after terminal events, bounded rows, and the latest pending reply visible;
  its PNG was inspected. Evidence: tmp/selection-lifecycle-native.log and
  tmp/agent-ui/selection-lifecycle.{log,png}. These are disconnected fixture checks;
  this continuation sent no Slack messages and did not restart the signed-in app.
- Final gate after the UI fixes: all 252 desktop tests passed (one ignored),
  desktop/core formatting checks and git diff --check passed, and strict deep
  codesign verification passed. A separate worker inspected all six ordinary
  forwarding PNGs: both widths retain close/footer actions, and partial outcomes
  keep Cancel/Resend original visible with clear focus. Together with the three
  recovery captures and selection capture, all ten final native workflows have
  inspected screenshots. All task-owned fixture processes exited; the signed-in
  client was not restarted. Full parity, ordinary autosave, and the prior
  unexplained isolated thread-pane closure remain open rather than claimed done.

### Upload delivery receipt boundary (2026-10-12, source checkpoint)

- Slack's [files.completeUploadExternal reference](https://docs.slack.dev/reference/methods/files.completeUploadExternal/) documents a successful `ok` response with file IDs, not a message timestamp. It also explicitly warns that `internal_error` and `fatal_error` can occur after part of the operation succeeded.
- The upload API now distinguishes failures before conversation delivery (local file reads, staging, cancellation) from failures during the final completion request. Only the latter can leave delivery uncertain. A completion response must positively return every exact staged file ID.
- Native upload readback uses replies for a thread and channel history otherwise. It retires the optimistic payload only after finding a valid message timestamp with the exact returned file IDs, sender and destination. An unrelated, empty or partial page retains the payload. There is no evidence here for combining separate messages into one upload receipt, so split-file results remain unconfirmed instead of being inferred.
- Known failed uploads restore the original caption/files when their draft is empty. If newer typing exists, the original payload remains separately visible; it is not merged into or substituted for the newer draft. Rejected text sends also retain their optimistic payload. No automatic retry was added.
- Regression sources cover stage-specific errors, malformed/partial completion responses, exact file readback, account ownership, and preserving newer drafts. The final standalone core suite passed 511 tests (one ignored), and doctests passed; evidence is `tmp/draft-consumption-core-tests-final.log`. Scoped formatting/diff validation passed; the coordinated desktop suite is still pending. No live Slack write was performed.


### Shared draft ownership and send outcomes (2026-10-12 continuation)

- Three workers remained active alongside root, the runtime's four-agent limit.
  Work was divided among core consumption/storage, protocol builders and upload
  receipts, and account-safe messaging/status presentation; root integrated the
  desktop draft owner, cancellation-safe initialization and acceptance tests.
- Draft consumption freezes Send/Schedule intent and its exact raw payload
  separately from later typing. Existing create/update work settles first;
  never-published content can send/schedule directly. Successful consumption
  retires its draft identity and rotates the next composer UUID. Remote deletion
  is not send confirmation. Rejection requires explicit retry; uncertainty never
  authorizes replay. Store writes use format 2 and still read format 1.
- Pure ordinary-draft builders preserve operation/client/draft identity and
  validate account/workspace/destination before constructing requests. Recorded
  traffic and Slack's shipped source establish ordinary is_from_composer=false,
  millisecond update time, full scheduled payload, and files.share's broadcast
  field. The existing scheduled-file request now uses that observed field.
  These are protocol/source checks, not fresh live-write verification; CDP was
  unavailable and the signed-in application was not restarted.
- The new disabled desktop boundary owns one account and destination at a time:
  lossless enrollment, strict revision checkpoint/clear/consume, durable prepare
  before dispatch, pure receipt CAS retries, scoped remote observation and
  explicit delete/retry/cancel controls. I/O runs on blocking workers. A retained
  initialization task prevents a canceled view's restart normalization from
  running again after a replacement view starts dispatching. Late outcomes save
  against the captured owner rather than whichever account is visible.
- Temporary-SQLite/fake-dispatch tests cover persistence before dispatch, send
  during create, exact frozen content and owned draft ID, new typing, supported
  remote edits, opaque response conflicts, strict receipt validation, account
  separation, stale revisions, concurrent views, clear-then-delete, cancellation,
  process restart, disk failure before dispatch and after receipt, explicit
  rejection recovery, and zero automatic replay. No production enrollment,
  ordinary draft autosave, background runner or network adapter is enabled yet.
- Active messaging captures account, visit epoch and transport before I/O;
  late text/upload/history outcomes cannot mutate a different account visit.
  Optimistic timestamps use one monotonic microsecond allocator across channel,
  thread and uploads, eliminating same-second row collisions. Definite failures
  retain text, and uploads preserve original caption/files separately from newer
  drafts. Pending rows distinguish "Not sent" from "Delivery unconfirmed";
  stopped transfers no longer retain progress spinners. These labels and legacy
  send payloads remain transient, not the new durable draft recovery queue.
- Text success additionally requires explicit boolean ok:true, a valid Slack
  timestamp and matching destination/explicit message fields. Malformed positive
  responses remain unconfirmed. Local text client IDs are not compared against
  server-generated IDs because the legacy sender does not transmit them yet.
  The managed sender must use the new request builder's original client UUID.
- Upload readback merges only the exact matching uploaded message and never
  replaces an existing cache row. It cannot roll unrelated cached edits back or
  resurrect unrelated deleted messages from its bounded page. A matching uploaded
  row deleted during the read can still be reinserted: general realtime message
  deletion has no retained tombstone. Scoped event ordering/tombstones remain a
  concrete next boundary; this is not proof of all history/realtime ordering.
- Next integration sequence: reroute managed keys in state/drafts.rs before any
  enrollment (legacy writes to managed keys correctly become blocked alternates);
  restore managed metadata and empty-local consumption intents at boot; make
  composer Send and ScheduleDialog await the same paired consume transaction;
  persist their outcomes before account/UI guards; then route scoped draft events
  through the owner. Coalescing may combine only one view's edits. Retry-After
  deadlines, blocked-alternate settlement and authenticated remote ownership must
  be wired before a real runner. Arbitrary blocks/files/threads remain unmanaged
  until their projection is lossless. Full parity is still in progress.
- Current gates: final standalone core suite 511 passed, one ignored; doctests
  passed. Desktop suite 294 passed, one ignored. Signed native outcome fixtures
  are being finalized. Core and desktop logs are
  tmp/draft-consumption-core-tests-final.log and
  tmp/draft-controller-desktop-tests-final.log. Final native evidence follows.

- Final native gate: signed rebuild and strict deep codesign verification passed;
  all three disconnected send-outcomes fixtures passed (normal, narrow, thread).
  Root inspected all three final PNGs. Each has accurate failed/unconfirmed text,
  preserved message/file content, no active-transfer label on stopped rows, and
  the newer composer unchanged. Narrow verification scrolls each real row into
  view and checks its complete bounds. The first checker incorrectly required
  four tall rows to be visible simultaneously; the failure PNG showed normal
  scrolling, not a layout defect. The corrected reachability check passed.
  Evidence: tmp/draft-controller-native-build.log, tmp/send-outcomes-native.log,
  and tmp/agent-ui/send-outcomes{,-narrow,-thread}.{log,png}.
- Final formatting checks for desktop/core and git diff --check passed; the
  native capture driver passed Python compilation. Core 511/desktop 294 tests
  passed (one ignored in each suite), with core doctests passing. Only the
  fixture-native scrolling assertion changed after that desktop test run; the
  signed rebuild/native run verifies that final checker. All fixture processes
  exited. No Slack messages were sent, no signed-in client was restarted, and
  no further storage cleanup was performed. The parity goal remains active.

### Durable channel-text delivery and local recovery (2026-10-12 continuation)

- Three workers ran alongside root, the runtime's four-agent maximum. They owned
  upload/realtime ordering and local persistence, delivery readback/recovery UI,
  and release/typing handoff. Root integrated the production send path and gates.
- Supported top-level text sends now enroll the exact durable local draft, freeze
  a Send intent and clear its paired local value before clearing the composer.
  One captured account/transport dispatch sends the original UUID. Preparation
  CAS may retry; the HTTP write never replays. Outcomes persist to the captured
  owner before visible-account guards. Thread/file/unsupported rich sends still
  use their existing paths; ownership guards prevent them or scheduling from
  bypassing an outstanding managed action.
- A detached local writer coalesces one editor's input and survives navigation.
  Mentions and attachments typed beside a frozen direct Send remain full local
  content, independent of the strict remote projection. Positive delivery can
  return the composer to ordinary local editing without losing newer typing.
  Released writes enter the worker synchronously and compare the exact local
  predecessor. SQLite contention does not hold the UI enqueue mutex. Settled
  writers are retired from polling instead of accumulating idle work.
- Rejected competing edits retain one latest SQLite version per editor with a
  monotonic serial, plus the first blocked compatibility version. They survive
  queue disposal/account switches/restart without replacing another editor's
  content. Pending/blocked versions remain visible independently of the live
  composer. Dispatch and new consumption fail closed while versions conflict;
  explicit version settlement remains further parity work.
- Boot restores managed metadata and frozen empty-local sends before exposing
  the shell. Drafts shows saved delivery, remote conflicts, retained local edits,
  and clean interrupted preparation. Continue editing locally releases only a
  clean, unowned record; aborting a fresh preparation attempts the same guarded
  handoff. Account/transport identity guards prevent stale refresh/UI effects.
- Check delivery reads one bounded history/replies page and requires exact UUID,
  authenticated sender, original destination/thread and a valid timestamp.
  Absence, malformed data and duplicates remain uncertain. It never resends.
  Checks reserve durable spacing and persist Retry-After deadlines; format 3
  reads formats 1/2/3. A failed deadline write blocks checks within the process;
  after a crash only the successfully persisted spacing/deadline can be known.
- Realtime deletions now leave session-scoped channel/thread tombstones even
  when the original row was absent. Upload readback and managed send receipts
  preserve intervening realtime edits/deletions rather than resurrecting a row.
  This is not a general persisted history/cache tombstone implementation.
- Native composer clearing also checks the actual DOM wire text, unique shell
  visit/destination and IME ownership. Newer native input can precede the Rust
  signal after a storage await; it must not be replaced by a delayed clear.
- Ordinary remote draft create/update autosave stays disabled pending complete
  response protocol proof and event integration. General raw blocks, files,
  threads and scheduling do not yet share this durable direct-send workflow.
  No live Slack request, signed-in app restart, commit/push or storage cleanup
  was performed. Playwright MCP still returned Transport closed.
- Final locked suites passed: desktop 364 and core 527 tests, with one ignored
  in each suite and doctests passing. All 22 composer JavaScript tests passed.
  Desktop/core formatting, git diff --check and both native drivers' Python
  compilation passed. Evidence: tmp/managed-send-desktop-tests-final.log and
  tmp/managed-send-core-tests-final.log.
- The signed native build and strict deep codesign verification passed. All
  five disconnected native fixtures passed: recovery normal/narrow and send
  outcomes normal/narrow/thread. Root and the native worker inspected all five
  PNGs. Narrow content scrolls normally, with every recovery/status label checked
  for reachability and bounds. Each outcome fixture exercised the production
  DOM clear helper against wrong scope, newer native text, active composition,
  and matching text after composition, restoring the textarea afterward.
  Recovery fixtures retained four recovery kinds and the frozen empty local;
  disconnected Check delivery explained the connection requirement and preserved
  saved content. Evidence: tmp/managed-send-native-build.log,
  tmp/managed-draft-recovery-native.log, tmp/managed-send-outcomes-native.log,
  and tmp/agent-ui/{managed-draft-recovery,managed-draft-recovery-narrow,
  send-outcomes,send-outcomes-narrow,send-outcomes-thread}.{log,png}.
  All task-owned fixture processes exited. These are offline checks, not live
  Slack delivery proof. Remote autosave, broader durable delivery workflows and
  explicit conflict settlement remain open; the parity goal remains active.

### Local version review and late edit responses (2026-10-12 continuation)

- The preceding goal turn made verified implementation and native-test progress.
  This continuation reused three workers alongside root for core persistence,
  recovery UI, and edit-response/native verification work. Full parity remains
  active; local version settlement is one additional completed workflow boundary.
- Drafts now offers one Review local versions control per destination, with
  current text, the first retained local version, and each editor's latest saved
  version. Native radio choices expose exact text and attachment names. The
  final action explicitly says it removes the other reviewed local versions;
  Cancel and Escape leave them intact. Frozen sends and remote Slack conflicts
  remain separate, and choosing local content never sends a Slack request.
- SQLite compares the entire reviewed inventory, paired local value and managed
  state in one transaction. Unseen edits make the choice stale. Settled writer
  revision floors prevent delayed old callbacks from recreating discarded
  versions or regaining ownership merely because their predecessor text matches.
  Later input from an old writer remains a separate version for another review.
- Editor settlement survives a canceled foreground waiter and serializes with
  checkpoints and release. It resumes on the chosen base using a fresh writer
  identity, preserves newer input arriving during disk I/O, and refuses to
  discard memory-only input absent from the durable review. Released input is
  queued against the selected predecessor after the transaction completes.
- Projection compares both the draft book and bound composer. Navigating during
  a save cannot recapture unchanged old content over the selected version. Native
  replacement compares exact wire text, field/account visit identity and IME
  state; newer DOM input remains intact. Pre-settlement metadata refreshes are
  invalidated, and the review component is scoped to the account and shell visit.
- Message edit receipts now capture account/transport, edit visit and exact input.
  Late success/failure cannot close a newer editor or affect another account.
  Receipt patches only update original channel/thread copies whose editable
  content still matches the captured baseline. Newer HTTP/realtime edits,
  reactions, deletion tombstones and unrelated thread bags are preserved.
- Unsupported chosen content on a remotely managed draft remains explicitly
  blocked unless the existing direct-send local capability permits it; every
  saved version is retained. Remote conflict resolution/autosave and general
  durable thread/file/scheduled delivery remain further parity work.
- Next concrete feature audit: saved files in Later currently render a generic
  label and cannot open. Slack's cached shared bundle proves file saved.add,
  saved.delete and saved.update identities (`item_type=file`, file ID, no ts).
  Implement file metadata hydration, native opening and saved-state controls;
  verify file-specific readback semantics before interpreting list omissions as
  successful removal. This audit made no live requests or source changes.
- Validation in progress: core locked suite passed 535 tests (one ignored) and
  doctests; five focused editor settlement regressions and 24 composer JavaScript
  tests passed. Final desktop and signed native results follow below.
- Final validation passed: 382 desktop tests and 535 core tests (one ignored in
  each suite), doctests, all 24 composer JavaScript tests, desktop/core formatting,
  Python driver compilation and git diff --check. An older fixture test assumed
  synchronous isolated writes; it now enqueues both visits through the ordered
  asynchronous backend and verifies that the newer input remains authoritative.
  Logs: tmp/draft-settlement-desktop-tests-final.log and
  tmp/draft-settlement-core-tests.log.
- The signed build completed and strict deep codesign verification passed. All
  five signed disconnected native fixtures passed: two recovery layouts and
  normal/narrow/thread send outcomes. Root and the native worker inspected all
  seven PNGs (including the two open-review captures). Review controls and copy
  are reachable in the narrow scroll container. Cancel/reopen/Escape, initial
  selection and success focus passed. SQLite readback proved selected local
  content, removal of reviewed alternatives, and preserved raw remote conflict
  plus frozen uncertain send. A fresh database open after both fixture processes
  exited independently confirmed those records and reviewed-writer tombstones.
  Both channel and thread native fields passed scope/newer-input/composition
  guards for clear and replacement, then restored their original fixture values.
  Evidence: tmp/draft-settlement-native-build.log,
  tmp/draft-settlement-recovery-native.log,
  tmp/draft-settlement-outcomes-native.log, and
  tmp/agent-ui/managed-draft-recovery{,-narrow}{,-review}.png plus
  tmp/agent-ui/send-outcomes{,-narrow,-thread}.png.
- All task-owned fixture processes exited. No live Slack requests, signed-in
  client restart, commit, push or storage cleanup occurred. Playwright MCP still
  returned Transport closed, so native verification used the signed offline
  fixtures. Full Slack parity remains unproven and the goal remains active.

### Saved files in Later (2026-10-12 continuation)

- Shared File Actions now saves/removes files through `saved.*` using exact file
  identities. Later hydrates title/type/uploader metadata and opens the existing
  native file/canvas/list viewer. Deleted and unavailable files keep their saved
  row and management actions; deleted files cannot open stale cached content.
- Saved membership is independent of filters and pagination. List omission is
  never proof of removal. Per-item request tokens, scope visits and list
  generations protect later observations; beginning an item request invalidates
  preceding list requests. Completion also recognizes `todo_state=completed`.
- File save/remove/complete/archive/reminder changes persist an account-owned
  SQLite receipt before one dispatch. Exact request/response identities and
  explicit contradictory states are validated. Uncertain results never replay
  automatically; bounded positive-only status checks cannot prove removal from
  a missing row. Missing negative fields cannot confirm restore/unarchive or a
  cleared reminder. Explicit Stop tracking records retirement without claiming
  success or undoing the Slack change.
- A process-wide file-operation guard prevents another mounted control from
  retiring an in-flight request and issuing the opposite write. Cooldowns are
  durable, with a process fallback if saving an observed deadline fails. Receipt
  persistence precedes account/transport/view projection guards. This guard does
  not claim coordination across simultaneous independent app processes.
- Recovery inventory remains available for files absent from the current page
  or deleted from Slack. Controller completion invalidates the mounted inventory;
  terminal receipts with active cooldowns remain visible. Failed journal loads
  fail closed with an explicit reload action. Recovered rows are refreshed after
  checking/retirement, and every control remains inside the scrollable list.
- Primary wire evidence is the already-captured Slack shared bundle, module
  e6np (file saved.add/delete/update) and GDwF (saved-file identity/state). No
  unverified saved.get omission semantics were introduced.
- Validation: final Rust/native gates are in progress; exact results follow.
- Final validation passed: 387 desktop tests and 552 independently locked core
  tests (one ignored in each), doctests, desktop/core formatting, Python driver
  compilation and git diff --check. Logs: tmp/later-files-desktop-tests-final.log
  and tmp/later-files-core-tests.log. The final signed build completed in 45.15s;
  strict deep codesign verification passed (tmp/later-files-native-build.log).
- Both signed disconnected native fixtures passed at 1280x800 and 668x400.
  They use the real shared File Actions control for Save, Later native reopening
  and Remove, then create an uncertain removal while the same inventory stays
  mounted. Cold/background metadata, missing-file retry, deleted-file opening
  guard, positive-only recovery check, canceled retirement and explicit
  retirement passed. Fresh journal reads proved FSTART Confirmed Remove,
  FCOLDOPEN Uncertain Remove and FRECOVERY Retired with its cooldown retained.
  Root independently reopened both SQLite databases after the fixture apps
  exited and verified all three results again.
- Root and native worker inspected all six final PNGs. Initial screenshot review
  caught a clipped Completed tab in the narrow panel; tab labels/counts now wrap
  within all three bounded controls and native geometry assertions pass. Fixture
  counts reflect four seeded files, and transient notifications are dismissed
  before captures. Production confirmed file changes separately refresh counts
  through the existing scoped Later loader. Captures:
  tmp/agent-ui/later-files{,-narrow}{,-viewer,-recovery}.png;
  runner log: tmp/later-files-native.log.
- No live Slack requests, signed-in client restart, commit, push or storage
  cleanup occurred. Native evidence is from isolated fixtures, not live Slack
  server roundtrips. Full Slack parity remains ongoing; especially actual
  saved-file mutation/readback/realtime interoperability needs live validation.

### Channel refresh projection cache removal (2026-10-12 continuation)

- Repository-wide reference inspection found `ShellState.messages_by_channel`
  was write-only: refresh and navigation cloned rich message projections into
  it, while the UI read `ShellState.messages` and navigation projected core
  history again. Removed the field, its writes/resets and obsolete fixture/test
  setup. Core cached history, the actual full message projection, unread-divider
  visit state, selection and timeline-window logic are unchanged.
- Added the ignored `channel_vm::benchmark::mixed_history_refresh` benchmark in
  `src/desktop/channel_vm/benchmark.rs`. It uses a disconnected fixture shell and
  the existing mixed-content source bag with 201, 2,001 and 10,001 total messages.
  It measures rich projection, a standalone deep clone, unchanged full shell
  refresh, and full refresh after changing the final message's text/rich blocks.
  The logical timeline window contains 100 rows; this is not a native DOM count.
- Method: compiled before and after with locked Cargo and
  `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
  CARGO_BUILD_JOBS=2`. Each metric runs 11 times, discards the first, and reports
  median/min/max of the remaining ten. Preserved the before test executable and
  alternated before/after execution across five pairs (AB, BA, AB, BA, AB), with
  no concurrent Cargo work. Figures below are the median of the five per-process
  medians, in milliseconds. The optimized test profile is the repository's dev
  opt-level 1, not a release/native frame benchmark.

  | Cached messages | Unchanged before | Unchanged after | Edited before | Edited after |
  | ---: | ---: | ---: | ---: | ---: |
  | 201 | 0.591 | 0.507 | 0.584 | 0.496 |
  | 2,001 | 6.742 | 5.409 | 6.794 | 5.369 |
  | 10,001 | 34.839 | 29.100 | 34.266 | 29.330 |

- At 10,001 messages unchanged refresh decreased 16.5% and edited refresh 14.4%.
  Standalone rich projection remained 28.891 -> 29.025 ms, while the removed
  duplicate clone cost 5.461 ms in the before runs. This removes an unused
  retained copy and its repeated allocation work; resident-memory savings were
  not measured. Full rich projection is still unbounded and remains the main
  measured cost. Host scheduling produced outliers, retained in the raw paired
  results rather than excluded; these timings do not prove native frame latency,
  first paint or live-workspace responsiveness.
- Validation: focused ignored benchmark passed before and after; 28 existing
  state tests, seven file-event tests and five file-action tests passed using the
  freshly compiled test executable. Those cover selection pinning, channel
  navigation, unread-divider preservation, saved targets, cache trimming, file
  rename/delete ordering and viewer identity. Scoped formatting and
  `git diff --check` passed. No new signed/native run was performed for this
  cleanup. Root owns the subsequent full integration and signed build gates.
- Evidence: `tmp/channel-refresh-baseline.log`,
  `tmp/channel-refresh-after.log`, `tmp/channel-refresh-paired.json` and
  `tmp/channel-refresh-focused-tests.log`. Reproduce the benchmark with
  `cargo test --locked channel_vm::benchmark::mixed_history_refresh -- --ignored
  --exact --nocapture --test-threads=1` and the resource environment above.
- Suggested existing native smoke: `thread-window-2000` with
  `SUPER_PLATINUM_SELECTION_CHECK=1`, driven by
  `tmp/selection-lifecycle-capture.py`. Its real channel/thread DOM checks cover
  outside release, selection retention, virtual jumps and return to latest;
  channel navigation/unread behavior is covered by the state tests above. Run
  only against the coordinated signed offline build, not the signed-in client.

### Return definitely rejected sends to drafts (2026-10-12 continuation)

- A definitely rejected managed direct send now offers Return to draft. The
  action restores the frozen message to ordinary local editing without HTTP or
  another send. Uncertain delivery, scheduled sends, threads, remote ownership,
  mismatched receipts and unsupported lossy content cannot take this path.
- Restoration validates the exact account, destination, revision, operation and
  consumption identity. Newer local input and independent saved versions remain
  separate. SQLite checks for alternates again in the committing transaction,
  closing the gap between reviewing local versions and restoring the message.
- An unchanged failed optimistic copy is removed only when its unique client
  identity, sender, pending failure, text and blocks all match the frozen send.
  Confirmed, uncertain, edited and ambiguous copies remain. Composer projection
  precedes refreshing the timeline so the old empty field cannot recapture the
  returned draft.
- Independent review found a completion-to-projection typing race. The editor
  now stays parked after the durable restoration until the foreground handoff
  proves the original scope and edit serial. Input before that handoff is saved
  as a separate version. Interrupted recovery is verified alongside the ordinary
  one-click return flow. Native evidence uses signed disconnected fixtures and
  does not establish live Slack interoperability.
- The interrupted-return path has an explicit serial/revision marker. A later
  Continue editing locally action revalidates the durable restored content and
  absence of alternate versions before projecting it while Drafts is visible.
  Only then can normal editing resume. Settlement and release clear the marker.
  Native coverage includes a parked return with no foreground projection.
- Final Rust gates passed: 396 desktop tests (two ignored), 552 independently
  locked core tests (one ignored), doctests, both formatting checks, Python
  driver syntax and git diff --check. The final desktop suite explicitly ran
  the post-receipt typing, interrupted-return rearm, durable-alternate refusal
  and navigation/re-entry regressions. An earlier focused command matched zero
  tests; it is compilation evidence only. Logs:
  tmp/rejected-send-desktop-tests-final.log and tmp/rejected-send-core-tests.log.
- Final signed native build completed in 41.49s and passed strict deep codesign
  verification. Build log: tmp/rejected-send-native-build-final.log.
- Both signed native recovery fixtures passed at 1280x800 and 668x400 using
  actual Return to draft, Continue editing locally and Continue writing buttons.
  They verified focus, editable restored composer text, explicit rejection of
  overwriting newer input, absence of return for uncertain delivery, and recovery
  of a parked handoff without restarting. Root and the native worker inspected
  all six captures: tmp/agent-ui/rejected-send{,-narrow}{,-review,-restored}.png.
  Transient toasts overlay some lower content in narrow captures; target controls
  remained reachable and the layout/interaction assertions passed.
- Root and worker independently reopened both fixture databases after process
  exit. C1/C4 contain exact ordinary local drafts and no managed rows; C2 retains
  uncertain delivery with no local text; C3 retains its rejected frozen intent
  and newer paired draft. No alternate versions were created. Evidence:
  tmp/rejected-send-native.log, tmp/rejected-send-sqlite-verification.txt and
  tmp/agent-ui/rejected-send{,-narrow}-settlement-25198/draft-settlement.sqlite.
- The final signed build also passed the existing thread-window-2000 native
  selection lifecycle check after the unused channel cache removal: selected
  text survives outside release; channel/thread virtual jumps and thread latest
  restoration pass; both selection pins release. Root and worker inspected
  tmp/agent-ui/selection-lifecycle.png. Logs:
  tmp/channel-cache-selection-native.log and tmp/agent-ui/selection-lifecycle.log.
- All task-owned fixture processes exited. No live Slack request, signed-in
  client restart, commit, push or storage cleanup occurred. Full parity remains
  active; the new native flows still need live server interoperability evidence.

### Next channel performance step: read-only audit

- No bounded channel projection change has been implemented yet. The smallest
  complete follow-up is a lightweight full-history row index plus rich views
  only for the visible window. Geometry, target lookup, unread position, latest
  identity and measurement guards must use the complete index. Scrolling must
  project newly selected rows, rather than merely changing vector indices.
- Annotating a slice directly would invent date/author boundaries at its edge
  and move the unread divider. Preserve original predecessors, full-history
  unread placement and the existing timestamp/client-ID/fallback identities.
  Current-window reactions, edits, profiles, files and media must still refresh.
  Required proof includes annotation equivalence, pending identity replacement,
  prepend/delete anchors, selection retention and variable-height native jumps.
- This still requires an O(N) metadata scan. Native behavior and performance of
  the proposed bounded-projection change have not been validated.
- Subsequent coordinated signed native smoke passed: the existing disconnected
  `thread-window-2000` selection driver verified channel text-selection
  retention/outside release, channel virtual jump, thread pointercancel/blur
  release and return to latest. Both native selection pins ended false; ten
  channel rows and eight thread rows were visible, with fourteen thread rows
  mounted. Inspected `tmp/agent-ui/selection-lifecycle.png`: history and latest
  pending reply rendered with both composers intact and no blank spacer/layout
  defect. Driver exited zero and task-owned PID 26170 exited (verified absent).
  Logs: `tmp/channel-cache-selection-native.log` and
  `tmp/agent-ui/selection-lifecycle.log`. This verifies the completed cache
  removal's existing native behavior, not the proposed bounded projection.

### Bounded channel rich projection (2026-10-13 continuation)

- Implemented the preceding audit: channels retain a lightweight row index for
  every timeline-visible cached source message, while rich text, attachments,
  reactions and media registration are projected only for the current window.
  Startup, refresh, channel selection and scroll window changes use this path.
  Ordinary windows contain at most 100 rich rows. A pinned selection temporarily
  retains its complete old span when messages are inserted inside it; releasing
  selection restores the ordinary bound. A regression covers 100 selected rows
  becoming 101 without evicting the selected endpoint.
- Full metadata owns absolute DOM/measurement indexes, spacers, held IDs, unread
  cursors and latest/target navigation. Original predecessor/date/unread context
  remains independent of window edges, including hidden replies, missing or
  malformed timestamps, pending IDs and client-ID/text identity fallbacks.
  Stale source positions reject the complete projection rather than shifting
  absolute indexes by omitting an interior row. Production refresh/navigation
  rebuild metadata synchronously and never fall back to synthetic fixture rows.
- Picker reaction ownership now reads its exact channel/thread source so a
  target scrolling outside the window cannot turn Remove into Add. Agent
  diagnostics distinguish total metadata from projected rows and identify their
  recent-message output as the rendered window. Explicit synthetic fixture
  backing preserves existing visual fixtures without entering production paths.
- All 411 desktop tests passed (two ignored), including full-versus-windowed
  MessageVm equivalence, far edits/reactions/file rename/delete, pending
  confirmation, unread/search/date targets, held-row prepend/delete and selection
  insertion. Doctests, formatting and git diff --check passed. No core-domain
  sources changed in this batch. Log: tmp/channel-window-desktop-tests.log.
- Before/after measurements used the preserved immediately preceding test
  executable and the current executable, five separate-process alternating
  AB/BA pairs with no simultaneous build/native workload. Each process discards
  one warmup and reports the median of ten samples; figures below are medians
  of the five process medians, in milliseconds. Both use the same optimized dev
  test profile and low-artifact Cargo environment used by the previous benchmark.

  | Cached rows | Refresh before | Refresh after | Edited before | Edited after |
  | ---: | ---: | ---: | ---: | ---: |
  | 201 | 0.503 | 0.304 | 0.500 | 0.299 |
  | 2,001 | 6.047 | 0.867 | 5.900 | 0.864 |
  | 10,001 | 30.506 | 4.112 | 30.696 | 3.963 |

- At 10,001 rows the unchanged refresh decreased 86.5%, edited refresh 87.1%.
  Projected rich rows decreased from 10,001 to 100 while all 10,001 metadata rows
  remained. Bounded rich projection alone measured 0.215ms; the retained full
  reference projection remained 30.071 -> 29.939ms as a comparison check.
  Metadata scanning is still O(N). These are synthetic CPU measurements, not
  native frame latency, live-workspace responsiveness or measured RSS savings.
  Exact executable hashes, individual samples and host outliers are retained in
  tmp/channel-window-paired-benchmark.json and its .log counterpart.
- Signed native build completed in 47.77s and passed strict deep codesign
  verification (tmp/channel-window-native-build.log). Native checks follow.
- The initial far-scroll native check passed gestures but sampled inconsistent
  final DOM/Rust windows. Investigation found its bare navigation call left the
  pending target active. The checker now uses complete production fixture
  navigation, requires anchor retirement and three consecutive exact DOM/index/
  projected-row agreements after animation frames, and rechecks the final receipt.
  Initial native output is diagnostic evidence, not the final acceptance gate.
- Hardened measurement completion within a conversation: its captured bounds,
  DOM window bounds and current bounds must agree before applying heights/window
  changes. Late row-ID or window mismatches request a fresh sample. Conversation
  changes still reject the sample outright. Reference full-history projection
  and annotation helpers are now test-only. This does not change the measured
  synchronous projection path above.
- Final desktop suite again passed all 411 tests and doctests; formatting and
  diff checks passed. Final signed build completed in 44.28s and passed strict
  deep codesign verification. Logs: tmp/channel-window-desktop-tests-final.log
  and tmp/channel-window-native-build-final.log.
- Final signed channel-window native acceptance passed at 10,001 cached rows:
  five far-scroll ranges remained bounded (21/34/34/35/23 mounted rows), actual
  DOM selection retained its text and row IDs while pinned, and release restored
  window movement. Date and unread navigation each achieved three consecutive
  DOM/metadata/projected-row agreements in three attempts with zero mismatches.
  Final state held 34 projected rows at indexes 4983..5017, target visible,
  pending anchor retired and selection unpinned. Inspected the final PNG.
  Evidence: tmp/channel-window-native-final.log and
  tmp/agent-ui/channel-window-10000.log / .png.
- Final signed selection-lifecycle rerun also passed all four channel/thread
  lifecycle checks. Both native selection pins cleared, channel visible rows 10,
  thread visible rows 8, thread mounted rows 14. Inspected the fresh PNG and
  verified the task-owned fixture process exited. Evidence:
  tmp/channel-window-selection-native-final.log and
  tmp/agent-ui/selection-lifecycle.log / .png. Both fixture processes exited;
  no signed-in session was restarted, no live Slack requests were made, and no
  storage cleanup was performed. Full parity and live interoperability remain
  ongoing; these results prove the isolated native fixture paths.

### Parallel follow-up audits (2026-10-13)

- Three additional workers reviewed Block Kit control values, requested dependency
  updates/system-message styling, and consumers of bounded channel projections.
  The projection audit found no additional off-window regression: mutation and
  navigation paths use canonical source bags or complete row metadata.
- Read-only dependency verification confirmed Tokio 1.53.2, objc2 0.6.5 and
  Rustls 0.23.45 in the appropriate current dependency graphs, with the pinned
  Dioxus revision and a single aligned Tao 0.34.8/Wry 0.53.5 retained. Rustls is
  in the desktop lockfile, not the independently resolved core graph. GitHub
  PRs 21 and 22 are closed without merging; the requested changes are already
  present locally. No further dependency edits were needed. A subsequent Oct 13
  read-only audit verified all ten jobs of GitHub Actions run 38261872738 passed
  for committed main 620ae0a9f53752d9ada4716f5cbedd1444c73cdd, including macOS
  Intel/ARM, Windows x64/ARM and Linux x64/ARM. This remote CI evidence applies
  only to that committed dependency baseline, not the current local parity work.
  https://github.com/3kh0/super-platinum/actions/runs/38261872738
- System activity retains the explicit lifecycle/pin subtype allowlist, muted
  shared timeline/thread body styling, bright author text and ordinary-message
  exclusions. No additional styling change was needed.
- Implemented initial-value display for existing inert Block Kit controls:
  selected options/users/channels/conversations, date/time/text values, and the
  previously omitted multi_external_select. Empty/malformed values retain
  placeholders. Option URLs remain inactive. Supplied timepicker zones stay
  visible; datetime values use existing local timestamp helpers. Composition
  labels decode Slack's three escaped characters once and remain plain text.
- Independent review corrected Slack's differing conversation default rules:
  explicit single selections take precedence, whereas the multi-select current-
  conversation default overrides its array. Without an owning conversation in
  BlockCtx, that default does not borrow the unrelated active channel. Seven
  regressions cover these rules and malformed/type/epoch fallback. References:
  https://docs.slack.dev/reference/block-kit/block-elements/select-menu-element/
  https://docs.slack.dev/reference/block-kit/block-elements/multi-select-menu-element/
  https://docs.slack.dev/reference/block-kit/block-elements/time-picker-element/
  https://docs.slack.dev/messaging/formatting-message-text/
- Registered disconnected block-control-values and block-control-values-narrow
  fixtures. This displays supplied initial state only; application action
  dispatch, dynamic options, modal state/submission and remote current state
  remain unsupported. No Slack mutation protocol was guessed or called.
- Validation passed: 418 desktop tests (two ignored), doctests, cargo fmt --check,
  git diff --check and capture-script syntax checks. Signed native build completed
  in 4m02s and strict deep signature verification passed. Normal 1280x800 and
  narrow 668x400 fixture captures were inspected: all supplied-value groups
  appear normally; visible narrow date/time/check/text controls stay within the
  conversation width. Narrow capture starts at the latest rows, so it is not
  evidence of every offscreen group at that width. Both task-owned disconnected
  fixture processes exited. No core sources changed in this batch.
  Logs: tmp/block-controls-desktop-tests.log, tmp/block-controls-native-build.log,
  tmp/block-controls-native-capture.log. PNGs and state snapshots:
  tmp/agent-ui/block-control-values{,-narrow}.{png,log} and matching -state.json.
- Follow-up read-only app-button audit located the captured Slack bundle's
  createFetcherSendActions (module urb3) and viewOpened handler (NGtd) in
  tmp/sb/js/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js.
  Static code supplies action/container JSON, app/service identity, correlation
  token and optional state.values. Modal open/update handling correlates pending
  tokens, expiry and previous-view stack; native reducers for that lifecycle are
  absent. Captured request-index matches were bundle downloads, not observed
  blocks.actions round trips. Next testable step is fixture-backed identity/state
  modeling and a controlled private action capture before enabling dispatch.
  Existing transport can encode the form and does not classify blocks.actions
  as retry-safe. This remains static evidence, not live interoperability proof.

### App interaction identity and modal correlation (2026-10-13 continuation)

- Previous goal turn made concrete progress (initial control values plus 418
  passing desktop tests/native captures). Full parity remains active. Rechecked
  Playwright and CUA: both return Transport closed; no CDP listener was present
  on 9222/9223. No signed-in session restart or live Slack action was attempted.
- Traced the ordinary message-button path in captured primary Slack code rather
  than relying on the work-object container. MessageBlocks (lvR2, character
  12806217) builds a message container using the message timestamp/channel and
  explicit is_ephemeral flag; thread_ts is included only for ephemeral threaded
  messages. ButtonElement (j1cO), Te7m confirmation wrapping and urb3 sendActions
  establish button text/identity/value/style, original confirm object, service/
  app identity, optional function ID/state.values, and JSON form serialization.
  These offsets refer to the captured shared-boot bundle named in the preceding
  section. w9uO in the captured shared bundle checks only message.is_ephemeral.
- Typed projection preserves parent block/action IDs, value, confirmation state
  (missing, malformed, parsed), typed text flags/style and accessibility label.
  Button metadata is boxed so ordinary text nodes do not grow with confirmation
  payloads. Parent identity cannot be spoofed by an element's own block_id;
  missing and duplicate IDs remain explicit rather than being synthesized.
- Canonical message-button capture/revalidation now binds account epoch, viewer,
  team, channel and server timestamp; rejects ambiguous blocks/actions, changed
  blocks/app context, malformed confirmation, special auth/agent prompt paths,
  recipient mismatch and attachment substitution. Reactions alone do not stale
  the intent. A pure prepared-request builder accepts only revalidated captures,
  checks workspace/viewer, preserves the observed optional fields and remains
  non-retryable. Strict endpoint response decoding preserves only plain error
  feedback. These APIs do not dispatch requests or look up canonical bags.
- A renderer-neutral modal reducer models explicit account/team/reconnect scope,
  ten pending tokens, expiry/replay guards, parent-stack matching, opaque view
  updates and local close. It does not invent view_closed events or treat opaque
  received content as a validated/submittable form. Same-generation updates use
  socket order; no sortable server version was established. Realtime integration,
  actual modal rendering/input submission and dispatch remain outstanding.
- Recipient-restricted buttons now render only for their declared viewer;
  malformed audiences remain hidden. Supplied accessible names reach buttons,
  with visible text as fallback. URL opening behavior is unchanged and ordinary
  app buttons remain disabled while full interaction orchestration is unfinished.
- Independently locked core suite passed all 574 tests (one ignored) and doctests,
  including 22 new source-validation, request and modal lifecycle regressions.
  Log: tmp/block-actions-core-tests.log. Desktop/native gates follow below.
- Desktop suite passed all 427 tests (two ignored) and doctests. Nine new
  parser regressions cover block identity, malformed confirmation, audience
  restriction and source distinctions. Both independently checked Rust format
  gates and diff whitespace checks passed after sorting the new API module.
  Log: tmp/block-actions-desktop-tests.log.
- Added an exact named-fixture/env-gated read-only native DOM checker plus
  scripts/check-block-buttons-native.py. It checks five expected control names,
  accessible names and disabled states, absence of restricted/malformed-audience
  controls, and horizontal fit. It never clicks or dispatches an action; retries
  for initial DOM readiness are bounded. Native build/capture results follow.
- Initial signed native build passed in 40.98s, and both named fixtures passed
  the DOM identity/audience/accessibility/overflow checker. PNG review exposed a
  pre-existing styling collision: global .primary imposed an 18px login margin
  on Block Kit buttons, stretching the adjacent link button. Those first images
  are not the final visual acceptance gate. Scoped Block Kit sizing now resets
  margins, uses the shared 32px control token, wraps long labels, and keeps
  disabled primary/danger controls neutral and readable. The native checker now
  also requires zero top margins and token-matched heights for these short labels.
- Next actual-modal integration evidence: captured shared-bundle FGXB near
  characters 1340750..1342700 prepares views.get(view_id), views.submit(client_token,
  view_id,state JSON), and views.close(client_token,view_id,optional root_view_id).
  Captured submit handlers distinguish update/push/clear/absent response_action;
  unknown nonempty actions need explicit handling. Rejections retain field errors
  and view_error, which generic Transport currently discards. A narrowly scoped
  structured response path plus typed input state and lifecycle-aware dismissal
  are needed before enabling modal submission. Per-control initial conversion
  and remote state reconciliation still need tracing; live proof remains absent.
- Final accepted signed build completed in 44.64s with strict deep codesign
  verification. Native button-metadata checks passed on the first sample at
  1280x800 and 668x400: all five expected controls had correct accessible names
  and enabled/disabled states, restricted labels were absent, both action groups
  fit horizontally, and every short button measured 32px high with zero top
  margin and the shared 8px corner radius. Personally inspected both final PNGs.
  Logs: tmp/block-actions-native-build-accepted.log and
  tmp/block-actions-native-capture-accepted.log; detailed receipts and PNGs are
  tmp/agent-ui/button-metadata{,-narrow}.{log,png}. Rechecked and inspected the
  preceding initial-control-value normal/narrow fixtures after shared sizing
  changes (tmp/block-controls-style-native.log). The final radius-only adjustment
  was verified by the button native checks. No Rust domain logic changed after
  the passing core/desktop suites. Both format gates, JS/Python syntax checks and
  git diff --check passed. Task-owned fixture processes exited; no live Slack
  requests or messages, signed-in session restart, storage cleanup, commit or
  push occurred. Full parity and live app-action interoperability remain ongoing.

### App modal forms and lifecycle (2026-10-13, in progress)

- Added renderer-neutral typed form parsing, validation and exact `{values}`
  submission state for text/multiline, static single/multiple selection,
  checkbox/radio, date, time and local datetime controls. Raw option objects
  survive serialization. Stateful controls not yet implemented (including
  selectors outside input blocks and legacy password controls) explicitly block
  submission. Native parse errors and dirty values survive remote updates.
- Captured cWek/cxJV/AM6q establish per-control state keys; AgTQ/4Z6N/9tv2 show
  that accepted same-view updates retain matching field state and prune removed
  identities. Added a distinct acknowledged-update reconciliation path instead
  of clearing all submitted fields. Unsolicited incompatible updates retain
  writing and block submission rather than silently dropping it.
- Added narrowly scoped structured transport for blocks.actions/views.get/
  views.submit/views.close with one attempt, exact endpoint allowlist, health
  handling, retained field/view errors and no response-content logging. Typed
  decoders distinguish absent/update/push/clear and reject unknown actions.
- Added native modal controller, input renderer, overlay and realtime wiring,
  scoped asynchronous results, close queue, and modal stack hydration/submission
  transitions. Capture evidence distinguishes Back (one view; root only for
  clear_on_close) from full Close (root and whole stack). Interrupted/ambiguous
  submissions remain locked without automatic replay. Generic overlay dismissal
  and replacement participate in close lifecycle.
- Disconnected normal/narrow fixtures exercise supported inputs and synthetic
  app feedback. Initial core suite: 602 passed, one ignored; initial desktop
  suite: 433 passed, two ignored. Later protocol/reconciliation changes require
  the final desktop and native gates below. Final core run now 603 passed, one
  ignored (tmp/app-modal-core-tests-final.log). No live app submissions occurred.
- Playwright transport recovered: browser_tabs list succeeded with an existing
  about:blank tab. This proves tool access, not signed-in Slack interoperability.
  Production message-button dispatch remains disabled pending canonical message
  owner plumbing, confirmation and exact bot/app metadata handling.
- Further review fixed restoring an invalidated parent after Back or an accepted
  child pop: the revealed view now schedules guarded views.get hydration. A
  pure regression covers both paths, stale revision/scope rejection and retained
  writing. Realtime now retires displaced overlay state before processing an
  event so a delayed update cannot replace a newer overlay.
- Known-offline mutations now fail before dispatch instead of parking until link
  recovery; views.get remains a safe read that can wait. The internal unsent
  marker is distinct from Error::Offline after a send attempt: only the former
  unlocks for retry, while ambiguous network failures retain the lock. Added
  localhost tests for all three write endpoints staying unsent after recovery
  and for no automatic retry. Latest core suite: 605 passed, one ignored;
  tmp/app-modal-core-tests-accepted.log.
- Next production button slice has a concrete audited path: explicit message
  owner/bag at view/message.rs, exact canonical source capture/revalidation,
  plain confirmation, metadata from message.bot_profile (Emf2/getBotByMsg gives
  this profile precedence over the bot registry), app_id and bot team from its
  preserved extra fields, and token registration before one dispatch. rIIG
  establishes bot/profile team precedence but absent metadata needs a real bot
  lookup implementation rather than an invented fallback. Specialized Slackbot
  actions, ownerless modal/attachment controls, and message controls with
  unmodeled state must not be routed through ordinary button dispatch. Captured
  j1cO dispatches before opening a URL after confirmation; actual roundtrip proof
  remains outstanding.
- Final desktop suite passed 442 tests, two ignored, with no new compiler
  warnings (tmp/app-modal-desktop-tests-accepted.log). Signed native build
  completed in 50.74s and strict deep codesign verification passed. Both format
  gates and git diff --check passed.
- All four signed disconnected native fixtures passed at 1280x800 and 668x400.
  Real DOM input/change/submit events verified nine fields, required validation,
  exact core choice state (including comma/empty option values), text/caret
  retention through remote updates, server field-error association, bidirectional
  focus trapping, retained options after removal/reintroduction, Push/Back/Clear,
  Escape, and reachable Close while busy/uncertain. Busy/uncertain are injected
  fixture states; this is not real Slack timeout proof. No horizontal overflow;
  footers remain visible. Personally inspected all four initial PNGs. Follow-up
  captures below cover lower date/time controls and remove an old synthetic
  success toast from the final uncertainty screenshot.
- Final capture-only fixture adjustments passed all 15 focused modal tests
  (tmp/app-modal-focused-tests-final.log), followed by a signed rebuild in 50.52s
  and strict deep codesign verification. All six capture runs passed:
  app-modal-form, app-modal-form-narrow, app-modal-lifecycle,
  app-modal-lifecycle-narrow, and form/-narrow lower-control captures. Logs:
  tmp/app-modal-native-build-final.log and tmp/app-modal-native-capture-final.log.
  Receipts/PNGs: tmp/agent-ui/app-modal*.{log,png}. Inspected upper forms,
  both final uncertain states and both lower captures. Normal lower screenshot
  shows checkbox/radio/date/time/datetime; narrow final screenshot shows time
  and datetime, with each date/time control separately scrolled into view and
  checked by the native checker. No horizontal overflow or obscured footer.
- Final format gates and diff whitespace checks passed. Native fixture processes
  all exited through the driver's owned-process cleanup. No live Slack requests
  or messages, signed-in restart, storage cleanup, commit or push occurred.
  Production app-button dispatch and live interoperability remain next; the
  overall parity/design/performance goal remains active.


### Ordinary message buttons and correlated app forms (2026-10-13)

- Added exact rendered message ownership for timeline, thread and pins bodies.
  Canonical action identity, app metadata, viewer restrictions and confirmation
  are checked at render/click/confirmation. Edited, deleted, pending, ambiguous,
  wrong-account and stale-visit sources cannot dispatch. Inline bot profiles
  retain Slack's precedence; message.bot_id remains the service identity.
- Supported stateless ordinary buttons prepare one blocks.actions request with
  a registered client token before dispatch. Correctly correlated realtime
  view_opened opens the existing typed app form workflow. Missing app routing,
  stateful message controls, specialized calendar/widget/Slackbot actions and
  ownerless attachment/modal controls remain gated instead of guessing payloads.
- Confirmation uses typed plain/mrkdwn text, initial Cancel focus, shared dialog
  focus trapping and exact-source revalidation. Cancel schedules no request or
  URL. Confirmed URL intent follows captured Slack's schedule-before-open order,
  without waiting for an HTTP acknowledgement. A later account/source change
  can cancel a still-unsent request even after its separately requested URL opens.
- Pending and ambiguous outcomes remain locked against duplicate dispatch;
  proven preflight offline failures and explicit rejection release the action.
  Frozen wire identity keeps an uncertain action locked when its original bag
  is evicted and the same button appears in a thread or another visit. Operation
  and completion generations prevent stale tasks from clearing newer attempts.
- Parallel review added a pre-send account/transport/canonical-source guard and
  immediate cleanup of displaced modal stacks. Accepted clicks survive ordinary
  channel navigation. Ordinary rows now skip canonical source capture entirely
  unless their rendered body contains a real app button; no elapsed-time
  performance improvement is claimed from this structural fast path alone.
- Added disconnected signed-native fixtures for same-label/different-block
  buttons, confirmation cancellation, duplicate clicks, action-to-modal token
  correlation, form submit/push/clear, stale-source cancellation and uncertain
  outcomes at normal/narrow sizes. They prepare synthetic requests without a
  Transport and record URL intent without opening a browser.
- Initial core suite: 613 passed, one ignored
  (tmp/message-buttons-core-tests.log). Initial desktop suite: 465 passed, two
  ignored (tmp/message-buttons-desktop-tests.log). Additional review fixes,
  bot lookup groundwork and final native gates are recorded below when complete.
- Playwright browser_tabs still succeeds with a usable about:blank tab. CUA
  also read the open native Slack window and accessibility tree without input
  or navigation. This is live reference access, not button interoperability.
- Further parallel review fixed hidden-parent modal updates invalidating an
  active child's request. Modal revision now tracks active-view/stack changes;
  hidden ancestors retain new payload/hydration state without touching the
  child's pending state. Parent drafts reconcile when revealed. Current-view
  updates still invalidate stale responses. Four new regressions and an
  expanded submission regression cover both inline updates and later hydration.
- Added the renderer-neutral bots.info request/decoder groundwork from captured
  hZPA: plural bots/app_ids request fields and explicit-ok bots array response.
  One exact requested bot identity is accepted; absent app/team metadata stays
  absent. Three pure tests cover shape, preservation and malformed/mismatched
  identities. This API is not yet wired to a desktop bot cache or resolver.
  Captured Emf2 requires any inline bot_profile to win; fetch is a fallback only
  when inline metadata is absent, not a way to fill incomplete inline profiles.
- Final independently locked core suite passed 618 tests, one ignored
  (tmp/message-buttons-core-tests-final.log). Desktop/native gates follow.
- Final full desktop suite passed 472 tests, two ignored
  (tmp/message-buttons-desktop-tests-final.log). The first non-test build exposed
  an unused duplicate action snapshot; removed it and its immediate redundant
  revalidation after core capture. All 25 message-button tests passed afterward
  (tmp/message-buttons-focused-tests-final.log). The signed rebuild/native
  interaction gates below use this final source.
- Follow-up performance candidate: share immutable Source revision snapshots
  with Arc while preserving exact equality and canonical resolution. App-heavy
  rows still rebuild revisions/stateless validation per button. This remains a
  measured-performance task for the next pass, not an implemented speed claim.
- Final signed build completed in 40.57s; strict deep codesign verification
  passed. No new compiler warnings remain (only preexisting block 0.1.6 future
  incompatibility). Log: tmp/message-buttons-native-build-final.log.
- Both native button fixtures passed at 1280x800 and 668x400. Receipts verify
  exact synthetic wire identities, zero request/URL intents on Cancel, repeated
  confirmation producing one action, rejection of wrong modal tokens, matching
  modal submit/push/clear, stale-source cancellation, pending/uncertain locks,
  hidden-recipient exclusion and ambiguous-identity disabling. Forward/reverse
  focus traps, formatted confirmation copy, visible footers and no overflow all
  passed. Personally inspected both final confirmation PNGs.
  tmp/message-buttons-native-capture.log and
  tmp/agent-ui/message-button-actions{,-narrow}.{log,png}.
- Shared modal-controller regression run passed all six signed native captures:
  form/lifecycle at normal+narrow sizes, plus lower-control captures at both
  sizes. Inspected all six PNGs. Input/error association, retained drafts,
  Push/Back/Clear/Escape, choices and date/time controls, pending/uncertain locks,
  reachable Close and fitting footers remain intact.
  Log: tmp/message-buttons-modal-regression-capture-final.log.
- The first regression harness attempt read a partial JSON receipt while it was
  being written; the completed receipt itself reported success. Fixed both
  native drivers to parse newline-complete receipts only, checked Python syntax,
  and reran the modal sweep successfully. No application assertion was hidden.
- Final format/whitespace checks passed. All task-owned fixture processes exited.
  No live Slack request/message, signed-in restart, storage cleanup, commit or
  push was performed. Real app interoperability and the missing-profile desktop
  resolver remain open; the full parity/design/performance goal stays active.


### Bot metadata fallback and snapshot sharing (2026-10-13)

- Continued with three parallel workers: core bot-cache bookkeeping, immutable
  snapshot sharing/rendered hydration, and captured service/client-action rules.
  The previous goal turn was verified implementation progress; the complete
  Slack parity/design/performance objective remains active.
- Added per-workspace bot lookup cache with exact request tickets, eight pending
  reads, 256 positive records and 64 negative keys. Automatic admission reserves
  capacity and never evicts; explicit hover retry can replace an entry while
  respecting cooldown. Negative overflow uses one bounded global deadline,
  preventing completion-triggered sweeps from endlessly refetching evicted misses.
  Expired requests cannot replace newer results; abandoned requests cool down
  only until their original deadline plus cooldown. These are local resource
  policies, not claimed Slack endpoint limits.
- Rendered ordinary buttons start guarded asynchronous bots.info reads only when
  inline bot_profile is absent. No timeline-wide tick scan or blocking render
  work is introduced. A completion wakes remaining rendered candidates that hit
  the concurrency bound, and never automatically dispatches an action. Reads
  and results are guarded by account, workspace/user, transport and credentials.
- Pure service resolver now follows captured Emf2/rIIG/uVRc precedence: inline
  profile wins as a complete record; absent inline profile uses cached bot data;
  valid bot team wins, otherwise canonical member/enterprise/current-team rules
  resolve service_team_id. Missing app identity still cannot correlate a modal.
  Source freezes resolved service routing independently of raw message revision;
  refreshed cache/member routing invalidates stale clicks and confirmations
  without injecting metadata into canonical messages.
- Snapshot storage now shares immutable Arc<Value> in Source and core captured
  buttons; exact equality/freshness checks remain. Isolated clone benchmark:
  representative 13-block/5-button ~7 KB snapshots, 10,000 clones. Source pair:
  7,330,000 allocations/927,490,000 requested bytes before versus zero after;
  core revision: 3,670,000/463,380,000 versus zero. One-run timing was ~119ms/58ms
  versus ~35us each. This measures snapshot cloning only, not whole Source
  cloning, message rendering, native frames or overall app performance.
  Reproducer: scripts/bench-message-snapshots.rs; retained output:
  tmp/agent-ui/message-snapshot-bench.jsonl.
- Added exact captured reserved client-action gates so Slack-owned channel,
  feedback, domain-authorization and preference actions cannot accidentally go
  through ordinary blocks.actions. Their specialized UI and slack:// button
  routing remain parity gaps; external scheme allowlists were not loosened.
- Initial cargo check passed. Final independently locked core suite passed 629
  tests, one ignored (tmp/message-button-metadata-core-tests.log). Desktop and
  native metadata/action gates follow below. Dioxus effect/prop handling was
  checked against 0.7 documentation and the pinned checkout implementation:
  https://docs.rs/dioxus/latest/dioxus/prelude/macro.use_reactive.html
- First desktop run passed 484 tests and failed three new service fixtures:
  their invented app IDs contained underscores, which the actual bots.info
  decoder correctly rejects. Replaced those synthetic IDs with valid uppercase
  identifiers; did not loosen production identity validation.
- Final desktop rerun passed 487 tests, two ignored; the signed native build
  completed in 39.27s and strict deep codesign verification passed. Logs:
  tmp/message-button-metadata-desktop-tests-final.log and
  tmp/message-button-metadata-native-build.log.
- All four native button captures passed: actions and metadata at normal and
  narrow sizes. Personally inspected all four screenshots. Confirmations fit
  both viewports with visible actions and focus; metadata controls remain
  individually reachable with no horizontal overflow. Metadata receipts assert
  ten completed lookups, maximum eight concurrent, drained queue, incomplete
  inline profile not fetched, and zero prepared actions, URL intents or transport
  use. Log: tmp/message-button-metadata-native-capture.log; images and receipts:
  tmp/agent-ui/message-button-{actions,metadata}{,-narrow}.{png,log}.
- Final root/core format gates and git diff --check passed. Task-owned native
  fixture processes exited. No live Slack request/write, signed-in restart,
  cleanup, commit or push was performed. Real app interoperability, member bot-ID
  fallback, stateful message controls and specialized client actions remain open.
  Three workers now cover typed Slack URL parsing, guarded navigation and a
  focused rendering performance measurement; the broader goal remains active.

### Typed Slack links and parallel performance review (2026-10-13)

- Reused three workers in parallel for the pure URL parser, guarded navigation,
  and isolated validation measurements. Root integrated message-button links,
  built native link fixtures, and retained central build/test ownership.
- Added typed channel/workspace and workspace-host permalink navigation, keeping
  message/thread timestamps as exact strings. Routing rejects ambiguous query
  parameters, unsupported Slack commands and arbitrary schemes; ordinary HTTP(S)
  links retain the external opener. A permalink must resolve to a unique signed-in
  workspace. Enterprise hostname normalization matches the existing URL writer.
- Known/archived/filtered conversations reuse global channel/message navigation;
  missing conversation metadata uses guarded conversations.genericInfo reads.
  Dedicated link generations supersede older requests without invalidating the
  current history load merely because a link was clicked. Account, credentials,
  transport, channel/thread/profile, search/palette/dialog and modal guards stop
  stale completions from taking over a newer surface.
- Ordinary rich links and same-workspace app-button URLs now share that router.
  App actions still schedule before URL handling without awaiting acknowledgement.
  Cross-workspace app-button URLs report unsupported feedback, because changing
  active workspace would currently cancel the unsent action/modal correlation.
  User deep links remain unsupported until DM-plus-profile sequencing is wired;
  profile-only navigation would lose the captured Slack behavior.
- Added disconnected native fixtures at both window sizes. They exercise real
  rich-link handlers for channel changes, exact replies, an older offscreen
  permalink target, and unsupported-command feedback. Validation follows below.
- The independent performance worker added scripts/bench-message-validation.rs,
  retaining tmp/agent-ui/message-validation-bench.jsonl. On 13 blocks/25 buttons,
  2,000 source revision checks measured median 54.889ms with JSON reconstruction
  versus 9.428ms in a borrowed-field prototype; allocations fell from 2,410,000
  to 42,000 (remaining exact bot-profile serialization). The composite validation
  portion (1,000 cached messages, 200 x 25 buttons) measured 284.870ms versus
  179.353ms, about 37 percent lower. All 24 mutation/null equivalence cases passed.
  Production validation code was not changed. These are isolated measurements,
  not native frame or whole-app gains. Next optimization is borrowed revision
  comparison while preserving all owner/bag/service/stateless/identity checks.
- Initial full desktop suite passed 502 tests, two ignored. The first native run
  exposed mrkdwn Slack links rendered as literal text; extended the existing link
  tokenizer for slack:// and added an exact-label/destination regression, keeping
  arbitrary schemes inert. The next full suite passed 503 tests, two ignored
  (tmp/slack-links-desktop-tests-final.log).
- A subsequent native assertion incorrectly expected thread_target to remain set
  after scrolling; the main view intentionally consumes that target on success.
  Strengthened the fixture with 31 later replies and checks for the exact older
  reply inside the thread's clipped viewport. The permalink similarly targets an
  older offscreen message, so channel-only navigation cannot satisfy its check.
- The narrow fixture correctly virtualized the newer link out after opening old
  history; added a return-to-reply link on the visible older fixture message so
  the harness follows a reachable interaction. Both final native link captures
  passed at 1280x800 and 668x400. Personally inspected both PNGs: exact reply is
  visible inside its scroll pane, composer/close remain reachable, and no
  horizontal overflow occurs. Both receipts prove channel navigation, older
  permalink targeting, exact reply targeting, unsupported feedback, no transport.
- All four native button regressions also passed (actions and metadata at both
  sizes). Final signed build took 18.82s; strict deep codesign verification,
  root format gate and git diff --check passed. Evidence:
  tmp/slack-links-native-build-final.log,
  tmp/slack-links-native-capture-final.log,
  tmp/slack-links-buttons-regression.log,
  tmp/agent-ui/slack-links{,-narrow}.{png,log}.
- Playwright MCP successfully listed its live blank tab. No real Slack action,
  message, account restart, storage cleanup, commit or push was performed. All
  task-owned native processes exited. These changes cover in-app link handlers;
  user-DM/profile deep links, OS-delivered navigation, cross-workspace app-button
  ownership and live app interoperability remain open. The full goal stays active.

### Borrowed revision comparisons (2026-10-13)

- Previous goal turn was verified progress: typed links plus 503 desktop tests
  and six native fixtures. Continued with three workers owning core comparison,
  desktop comparison/measurement, and an independent serialization audit.
- Corrected an earlier suspected gap before widening service identity: captured
  ordinary message actions require message.bot_id. Boot lvR2 at 12805721 gates
  Emf2 metadata lookup on a string bot ID; at 12806614 ordinary dispatch returns
  when it is absent. The broader shared selector's member.profile.bot_id fallback
  does not override that renderer guard. Existing explicit message.bot_id checks
  stay; no ordinary member fallback implementation is needed.
- Production core revalidation and all five desktop Source freshness checks now
  compare stored snapshots to borrowed block/scalar fields. Frozen Arc<Value>
  capture, raw/wire revision boundaries, exact owner/bag/service checks, stateless
  gating and action identity validation remain unchanged. Bot-profile comparison
  intentionally retains Serde serialization, including nested unknown fields and
  flattened-key override behavior. Missing/explicit-null equivalence is preserved.
- Extracted pure capture/matches helpers into core validation/revision.rs and
  desktop message_buttons/source/revision.rs. Eight focused regression tests cover
  every tracked field, ordered/raw nested action content, nullable fields,
  flattened profile/icon collisions and malformed snapshots. Independent review
  found no mismatch with the previous seven-key/core and twelve-key/source JSON.
- Independently locked core suite passed 633 tests, one ignored; desktop suite
  passed 507 tests, two ignored. Signed native build completed in 23.83s; strict
  deep codesign verification passed. All four action/metadata native checks passed
  at normal/narrow sizes; inspected the narrow confirmation and metadata PNGs.
  Both formatting gates and git diff --check passed. Logs:
  tmp/borrowed-revision-{core-tests,desktop-tests,native-build,native-capture}.log.
- Updated scripts/bench-message-validation.rs to include the actual production
  helpers rather than prototype copies; 24 mutation/null equivalence checks pass.
  Retained an initial run during compilation, then repeated once after the build
  and native processes exited. Final same-run three-round medians: 2,000 source
  comparisons 58.735ms -> 10.539ms; allocation count 2,410,000 -> 42,000. Core
  comparison 59.694ms -> 10.163ms. Composite validation portion, 200 x 25 buttons
  over 1,000 cached messages: 308.907ms -> 187.968ms (39.2 percent lower), allocations
  12,260,000 -> 6,340,000. Receipt:
  tmp/agent-ui/message-validation-production-idle-bench.jsonl. These are isolated
  validation measurements, not native frame or overall app timing; profile
  serialization remains an intentional allocation source.
- No live Slack requests/writes, signed-in app restart, cleanup, commit or push.
  All task-owned build/native/benchmark processes exited. Next protocol work is
  stateful message controls; captured message state initialization differs from
  modal submission, so existing modal validation cannot be reused wholesale.
  The full parity/design/performance goal remains active.
- Next message-control slice, from captured shared-boot bundle: static_select in
  section accessories/actions, confirmation, and full state on later button clicks.
  kwSp1748637 keys state by channel-message; 7u/z12821423 lazily initializes on
  first interaction; Te7m13143116 updates local state before dispatch; 9tv2
  6535852 retains state across remounts and prunes removed block/action IDs.
  M0Sk1611609 emits type-only section/input entries without initial values but
  omits uninitialized action-row entries. urb3 at4144700 sends
  state=JSON.stringify({values:state}) through existing blocks.actions. z3Rn
  13305352/13307800 defines static-select raw selected_option, optional initial
  option/placeholder; confirmation cancellation preserves the previous selection
  and stale/removed options must not commit. Reuse form parsing/value types, not
  whole-modal submission/required-field validation. Keep unsupported sibling
  control types gated until their state and actions are owned.

### Ordinary message static selects and shared input state (2026-10-13)

- Continued with three concurrent workers on typed protocol/source validation,
  message input state, and native rendering/checks; root integrated the controller.
  Runtime allows four agents including root, within the user's maximum of twenty.
- Added canonical static_select controls in section accessories and action rows,
  grouped native options, explicit clears, confirmation/cancellation, stale-source
  rejection, and full message state on subsequent button actions. State belongs to
  account/viewer/workspace/channel/message, survives remount/navigation, initializes
  on first interaction (including a button), and is never persisted as a modal form.
- Kept component display separate from retained raw wire state. Captured z3Rn
  resets display on option removal/new initial values without committing state;
  9tv2 prunes removed stateful IDs only. A later button preserves retained raw
  selected_option, while each new select must match a current canonical option.
  Static-select-to-button replacement prunes the old stateful identity.
- Transactions prepare a validated request before publishing local state; pending
  confirmations also freeze local input revision. Navigation-only source refresh
  does not alter that revision. Account replacement prunes the input cache.
  Pending writes disable sibling message controls; uncertain locks remain attached
  to the original action. Unsupported stateful siblings still gate the container.
- Cache is bounded at 256 retained input containers without silently discarding
  choices; stateless buttons do not consume that capacity. Rendering borrows cached
  input/eligibility state and still checks current canonical ownership. Heavy
  action snapshots belong to interactions, not select rendering.
- First verification: independently locked core suite 650 passed/one ignored;
  desktop suite 521 passed/two ignored. Both signed native select fixtures passed
  confirmation cancel/duplicate submit, complete state, navigation unmount/remount,
  stale option edit, and normal/narrow reachability/overflow checks. Root inspected
  both PNGs. Final post-review verification is recorded below.
- No live Slack write, signed-in app restart, cleanup, commit or push. Playwright
  MCP tab listing succeeded (tab 0 about:blank). Real app acknowledgement and other
  stateful control types remain outside this verified slice; full goal stays active.
- Final review removed per-field action snapshot/full-input parsing during cache
  eligibility via borrowed canonical select metadata validation, and reduced DOM
  handler captures to option value strings. Added capacity and cross-bag pending
  confirmation regressions; actual dispatch retains full strict validation.
- Final verification: core 652 passed/one ignored; desktop 523 passed/two ignored;
  both cargo fmt gates and git diff --check passed. Signed native build finished
  in 32.31s including lock wait; strict deep codesign verification passed. Both
  select fixtures and all four existing action/metadata fixtures passed. Inspected
  final narrow select and button-confirmation captures, plus both select sizes
  from the first run. Receipts/logs:
  tmp/message-select-{core-tests-final,desktop-tests-final,native-build-final,
  native-capture-final,buttons-regression}.log. No whole-app performance claim
  follows from these correctness checks.
- Next narrow protocol slice: radio_buttons. Captured gMsa begins at13301581 in
  the shared-boot bundle; it resolves current raw option by value, emits selected
  option/null plus optional initial option, and commits confirmation only onGo.
  It has no static-select componentDidUpdate reset; model display reconciliation
  must distinguish the two controls. All state, recipient, stale-source, and
  later-button full-state requirements still apply.

### Ordinary message radio groups and self-menu response ownership (2026-10-13)

- Previous turn made verified progress (static selects and six native checks).
  Continued with three workers on radio protocol/state, native rendering, and
  desktop regression coverage, then reassigned free workers to broader parity and
  performance audits. Full Slack/design/performance objective remains active.
- Added typed radio_buttons actions, canonical raw option/description preservation,
  native accessible radio groups, shared message state, confirmation and later
  button submission. Captured gMsa differs from static select: an existing radio's
  saved choice survives new initial/options data; if that option disappears there
  is no checked item until it returns, without changing retained wire state.
- Radio and select controls share container state and account/navigation ownership,
  but retain separate action validation and display reconciliation. Unknown sibling
  controls remain unsupported. A group gets a unique DOM-only name so separately
  rendered timeline/thread copies cannot uncheck each other's browser inputs.
- First independently locked core run passed 668 tests/one ignored; desktop run
  passed 531 tests/two ignored. Native and final post-review gates follow below.
- Broader audit identified missing own-status/profile editing, native Lists (current
  list files open externally), and Apps/Workflows discovery. Canvas content editing
  requires the captured Quip collaboration path; existing reader/create/rename do
  not establish editor parity. These remain requirements, not external-link substitutes.
- The audit also found delayed self-menu responses updating whichever account was
  active after await, plus older pause/resume results overwriting newer local
  intent. A focused ownership/latest-intent fix is underway in this batch.
- Follow-up core fix: same-ID static-select/radio type replacement mounts from
  retained selected_option, not the old component's reset display. Null/missing
  falls back to new initial; radio empty saved values use gMsa truthiness fallback,
  while select preserves them. Three regressions added; final core suite 671 passed.
- Known display-lifetime gap: MessageInputs currently stores shared display state.
  Captured component constructors re-read savedOption on a new mount; mounted radio
  instances do not follow changed initial values. Empty saved radio values and
  multiple rendered copies can therefore need separate per-component display
  lifetimes. Ordinary nonempty navigation retention fixtures do not prove those
  edge cases. Preserve shared wire state while separating mounted display state
  in follow-up; do not call the current controls complete parity.
- Self-menu completion guards now bind account epoch, workspace/viewer, transport,
  session token/host, and independent presence/DND/status intent counters. Pause
  and resume share a counter. Clear-status responses merge only status fields and
  preserve intervening realtime status/profile updates. Existing optimistic error
  rollback is still absent. Final desktop suite after these changes: 533 passed.
- Native first run caught a real radio restore failure behind confirmation: pinned
  Dioxus checked is not volatile, so the browser selection remained on the unconfirmed
  option. Fixing controlled checked writes without remounting the input/group;
  native results below must supersede this failed first capture.
- Next measured performance target: view/mod.rs sums all offscreen channel heights
  on shell renders; state/timeline.rs scans those heights for viewport lookup;
  thread_window rendering also walks metadata. Composer writes global ShellState,
  so profile these scans during typing/scrolling before claiming a bottleneck.
  Existing channel-window native checks establish correctness, not frame latency.
  Measurement itself needs repair first: performance.rs drops samples >=250ms;
  scroll_performance.js checks the 180ms active deadline before recording a pending
  frame delta, potentially dropping long stalls too, and excludes thread scrolling.
  Add unfiltered visible-frame/long-stall reporting and command-to-settled-target
  timings, capture a baseline, then evaluate cached cumulative-height geometry.
- Final native fix explicitly supplies a volatile checked Attribute on existing
  radio inputs (pinned Dioxus html/elements.rs declares checked DEFAULT); fixed
  RSX spread ordering after a compile-only diagnostic. Normal/narrow checks now
  pass cancellation with unchanged group/input nodes, pending duplicate rejection,
  full raw state, nonempty navigation retention, hidden/restored selected option,
  changed initial, and stale confirmation cancellation. Final checks dismiss the
  validated stale-action notice, then use elementFromPoint to prove each input and
  button is reachable; normal/narrow PNGs inspected, including the final unobscured
  narrow capture. Existing two select and four button/metadata fixtures also pass.
- Final gates: core 671 passed/one ignored; desktop 533 passed/two ignored; both
  formatting gates and git diff --check passed. Final signed build 19.81s and strict
  deep codesign verification passed. Eight native fixtures passed in total (final
  radio pair strengthened and repeated after checker-only reachability changes).
  Logs: tmp/message-radio-{core-tests-final,desktop-tests-final,native-build-final,
  native-capture-final,select-regression,buttons-regression}.log.
- No live Slack writes, signed-in app restart, cleanup, commit or push. Radio
  component-lifetime gaps, real app acknowledgement, optimistic self-menu failure
  rollback, and the broader matrix remain open; goal is not complete.

### Status editing, mount-local controls, and honest scroll measurements (2026-10-13)

- Continued with at most three workers alongside root: status controller, native
  status/picker UI, control mount lifetimes, then an independent geometry audit.
  Root serialized builds and native launches. No live profile/status writes,
  signed-in app restart, cleanup, commits, or pushes in this batch.
- Added account-menu status editing with text, the shared bounded emoji picker,
  six expiration choices (30 minutes, 1 hour, 4 hours, Today, Don't clear,
  Custom), local datetime validation, and retained draft/error states. Captured
  Slack boot source establishes Today as local end-of-day, text-only
  speech_balloon fallback, and UTF-16 maxLength 100. Existing exact custom expiry
  seconds survive an untouched field. New editor uses the existing profile
  request builders and one-attempt transport semantics.
- Save and existing Clear status share latest-intent ownership; account, viewer,
  session, transport, and realtime status guards protect completion. Ambiguous
  responses retain a locked draft and offer read-only Check status before a new
  explicit save. Missing/malformed profile fields cannot masquerade as a clear.
  Rate-limit expiry requests one repaint so Save unlocks without further input.
  Live users.profile.set/get endpoint responses remain unverified.
- Native selects/radios now have component-local display handles independent of
  retained container wire state. Confirmation commits only the originating live
  mount; stale/unmounted handles cannot prepare. Empty radio saved values fall
  back to the current initial option on remount, while existing mounts keep their
  own displayed value. Static-select reset rules remain distinct.
- Native radio checks now prove channel/thread copies can display different
  values, actual empty-value unmount/remount fallback, and subsequent button
  submission retaining the empty wire value. Five prepared requests per fixture;
  no transport. Normal/narrow status checks prove Cancel/Escape discard, text/caret
  retention, actual emoji-picker selection, invalid custom date, duplicate-save
  lock, rejected-save retention, accepted merge, and uncertain readback without
  resend (three prepared requests and one readback per fixture).
- Repaired scroll sampling: independent channel/thread streams, RAF intervals
  separate from event-to-first-frame delay, visible long stalls retained even
  past the 180ms active deadline, no idle RAF loop, hidden-time exclusion, and
  bounded 600-sample windows with p50/p95/max/stall counts in agent state.
  Eight Node observer regressions include 500ms and 1800ms synthetic stalls.
- Validation: independently locked core suite 671 passed / 1 ignored; desktop
  552 passed / 2 ignored; scoped/full formatter checks and git diff --check pass.
  Signed native build passed (40.21s); deep strict codesign verification passed.
  Status, radio and static-select native pairs passed. Status normal/narrow and
  radio narrow screenshots inspected. Logs: tmp/status-mount-{core-tests,
  desktop-tests,native-build,radio-native,select-native}.log and
  tmp/status-native-check.log; artifacts under tmp/agent-ui/.
- Fresh disconnected 10,001-row auto-scroll baseline, three launches, 58 retained
  RAF intervals each: p50 17ms each, p95 18/18/20ms, max 19/30/23ms, no >50ms
  samples. First-frame delays read 0ms at the native timer's resolution; this is
  not zero end-to-end latency. No thread workload was measured. Artifacts:
  tmp/agent-ui/channel-scroll-baseline-{1,2,3}.json; runner:
  scripts/measure-channel-scroll-native.py. These are baselines, not a measured
  optimization or live-workspace proof.
- Native bounded-window navigation/selection gate passed. Command-to-first
  verified DOM/native target agreement: date 103ms, unread 101ms; three-sample
  confirmation 234/236ms. The check includes two RAFs plus a 40ms polling interval,
  so these are conservative observation timings, not exact paint latency.
  Evidence: tmp/channel-window-timing-baseline.log and
  tmp/agent-ui/channel-window-10000.log/state JSON.
- Next performance candidate remains channel cumulative-height offsets: O(1)
  spacer sums and binary-search jumps. Audit identified row replacement, accepted
  measurement batches (including early returns), and height pruning as necessary
  invalidation boundaries. No geometry optimization implemented in this batch;
  measure rebuild cost alongside read/jump savings before adopting it.
- Shared reaction-picker regression capture also passed and was inspected; this
  confirms layout/opening only, not a live reaction write. The native bounded
  channel screenshot was inspected after its date/unread navigation gate.

### Cached channel geometry and paired performance evidence (2026-10-13)

- Previous goal turn made verified progress. Continued with three workers on
  channel geometry, independent mutation-path regressions/CPU measurement, and
  captured Slack Lists protocol evidence. Full Slack parity remains open.
- Added cumulative channel row boundaries, replacing per-render full-transcript
  spacer scans with O(1) reads and spacer-jump linear scans with binary searches.
  Rebuilds occur after row replacement/clear and changed accepted measurements,
  including pending-navigation/selection-pinned early returns and height eviction.
  Fixed 66px unknown-height estimates and existing boundary semantics remain.
  Range-only changes and unchanged measurement batches do not rebuild.
- Two pure tests compare cached behavior with the previous linear algorithm
  across mixed/empty/large/exact/nonfinite boundaries. Seven independent shell
  regressions exercise canonical and fixture replacement, same-length reorder,
  prepend/delete, channel switches with matching message IDs, measurement changes,
  pinned selections, pending anchors, and eviction.
- Validation: cargo check --locked passed (9.14s); full desktop suite 561 passed,
  3 ignored; final focused integration run 7 passed, 1 ignored. Formatter and
  diff checks passed. Signed native build passed (37.02s); deep strict codesign
  verification passed. No core behavior changed in this batch.
- Paired CPU benchmark uses 512 read/jump queries per sample, one warmup and eight
  alternating old/new samples in the same process. At 200/2,000/10,000 rows,
  median spacer scans were 0.7694/10.9354/57.8459ms versus cached
  0.000438/0.000459/0.000876ms; jump scans were 0.6445/6.1415/29.8293ms versus
  0.005417/0.008479/0.012292ms. Full cache rebuild medians:
  0.00260/0.02285/0.11911ms. Production acceptance of a changed 100-row measurement
  batch plus rebuild: 0.00569/0.02583/0.12147ms. This last measurement pins selection
  and excludes batch construction, projection, eviction, and WebView work.
  Raw samples: tmp/timeline-geometry-cpu.json and .log. These isolated helper
  improvements do not establish whole-app or live-workspace speedups.
- Preserved the prior signed app in tmp/timeline-before.app. Alternated three
  before/after disconnected native 10,001-row scroll launches using the same
  harness. Before p50 17/17/17ms, p95 18/19/20ms, max 24/20/32ms;
  after p50 17/17/17ms, p95 19/19/19ms, max 30/25/31ms. No sample exceeded50ms.
  Sample counts before 59/48/58, after58/58/58. Native results show similar frame
  timing, not a demonstrated frame-rate improvement. No thread workload measured.
  Artifacts: tmp/agent-ui/channel-scroll-geometry-{before,after}-{1,2,3}.json;
  runner now accepts --binary, --label, --runs, --run-start for reproducible pairs.
- Both native navigation gates passed with identical final DOM/native target
  agreement and no mismatches. Date/unread first verified agreement before99/100ms,
  after90/99ms; three-sample confirmation before232/232ms, after222/232ms. Single
  pairs with two RAFs +40ms polling are insufficient to claim a latency trend.
  Final new-build screenshot inspected. Logs:
  tmp/timeline-geometry-{check,tests,integration-tests,native-build,native-pairs,
  navigation-pairs}.log and tmp/agent-ui/channel-window-10000-geometry-*.log.
- No live Slack writes, signed-in app restart, cleanup, commit, or push. Lists
  protocol investigation found the record decoder and main UI in existing
  captured source after correcting a module-header search. CDP9222 is unavailable;
  no restart attempted. Native Lists implementation is the next feature slice;
  pagination continuation and live response/concurrency proof remain open.
- Corrected Lists findings and implementation boundaries are recorded in
  docs/slack-lists-protocol.md with exact captured bundle character offsets.
  The decoder and main table UI were present in the original captures. The actual
  traced List loader fetches once; it observes next_cursor but supplies no cursor
  or limit. A native reader must report incomplete results rather than inventing
  continuation behavior. Request/schema/record/permission/realtime consumer
  evidence is ready for implementation; live endpoint compatibility is not proven.


## 2026-10-13 — Native Lists reader and guarded checkbox edits

- Parallel implementation used three workers plus root, the session concurrency
  limit, within the requested maximum of 20. Separate protocol, controller, and native
  UI ownership was followed by independent lifecycle and performance reviews.
- Existing ordinary Lists now open in a native table. Raw schema/record/field
  objects survive parsing; false/zero/unsupported fallback values remain visible.
  Only 50 loaded rows mount per local page. Arrow/Home/End cell movement, Space
  checkbox editing, both-axis scrolling, and keyed-row focus recovery are covered.
  Initial loading, read-only/locked, incomplete, failed-read, and retained-content
  recovery have explicit states. Local paging does not imply server continuation.
- Existing ordinary checkbox writes capture raw identity/content, re-read file,
  records, and team editing permission, then dispatch one nonretry request. Missing
  permission leaves readable content read-only. Canonical file tombstones reject
  parsing/edit/readback. Scope checks include account/epoch/workspace/viewer/session,
  transport, visit, request and realtime revision. Duplicate edits lock; uncertain
  outcomes survive close/reopen and require concrete readback, never automatic replay.
- Known List/file events invalidate the reader. Realtime bursts retain one active
  read and request one replacement on completion; reconnect explicitly retires
  older reads. Retry-After is respected even for obsolete responses. Base row
  projections stay in an Arc across checkbox phases; only a small mutation target
  changes. Tests prove these work bounds; no measured whole-app speed claim is made.
- Independent review caught and fixed permission-failure read availability,
  tombstone preflight, reconnect read races, full-row mutation projection work,
  and realtime request amplification. The first native run then exposed focus loss
  during keyed-row moves and a global 32px button width clipping Refresh. Both were
  fixed and the native interaction gate passed at 1280×800 and 668×400 viewports.
- Validation: locked core 683 passed / 1 ignored; locked desktop 581 passed / 3 ignored
  (12 new core and 20 new desktop regressions). Initial cargo check passed; final
  signed app build, strict deep codesign verification, cargo fmt --check, Python
  driver syntax and git diff --check passed. Logs: tmp/slack-list-core-tests.log,
  tmp/slack-list-desktop-tests.log, tmp/slack-list-check.log,
  tmp/slack-list-native-build.log and tmp/slack-list-fmt.log.
- Native receipts assert exactly two prepared nonretry checkbox writes, duplicate
  suppression, canonical pending checkbox display, successful and lost-reply
  readback without replay, read-only/incomplete/error recovery, paging boundaries,
  a single keyboard tab stop, focus preservation/removal recovery, header control
  nonoverlap, bounded dialog and scroll reachability. Six PNG states inspected:
  tmp/agent-ui/slack-list{,-narrow,-readonly,-incomplete,-error,-loading}.png.
  Interaction logs: tmp/agent-ui/slack-list{,-narrow}.log; runners:
  scripts/check-slack-list-native.py and scripts/agent-ui-check.sh.
- This is verified offline progress, not full Lists parity. Live request/response
  compatibility and actual successful cell response shape remain unverified;
  fixtures use synthetic responses. Server continuation, saved-view filtering/
  grouping, parent/subtask hierarchy, rich cell presentation/reference resolution,
  other cell edits and row/schema creation remain open. Protocol evidence and
  boundaries: docs/slack-lists-protocol.md. No live Slack writes, signed-in restart,
  cleanup, commit or push in this batch. Full parity goal remains active.

## 2026-10-13 — Rich Lists, hierarchy, and verified enterprise reads

- Used three parallel workers plus root for protocol/policy, native interaction
  checks, and controller/projection review. Rich cells reuse the safe content
  renderer; select options resolve schema labels, user/channel references resolve
  cached names, and links follow existing URL rules. Display traversal is bounded
  before cloning (32 levels, 2,048 nodes, 64 KiB strings); oversized cells show an
  explicit notice while preserving raw data.
- Parent/subtask navigation uses a locally expandable tree with stable row/cell
  identities and 50 mounted rows. Missing/archived/deleted/cyclic parents do not
  hide children. A display-only first-column fallback keeps expansion reachable
  when no primary text column exists. This native presentation does not implement
  Slack saved-view filtering/grouping or prove stock main-grid hierarchy parity.
- Checkbox gestures reject obsolete dialog generations and collapsed children.
  An explicit edit already in preflight survives local collapse. Completion
  variants require authenticated personal-task preferences proving ordinary List
  identity, checked again during fresh preflight; personal To-do completion stays
  unsupported because Slack may also mutate a referenced original List record.
- Name-changing realtime events reproject dependent rows only, including hidden
  children and unknown-to-known references. Unaffected row Arcs and expansion
  survive; status-only updates do no projection work. Independent review caught
  changing DM peer dependencies, including equal-name peers, and added coverage.
  Later HTTP hydration and group-membership styling remain separate refresh gaps.
- Revalidated the private test channel, inspected its existing List through native
  Slack, and ran a structural read-only probe. Its four columns and three complete
  records parsed successfully. This exposed the old E-prefixed workspace
  `team.prefs.get` failure. The corrected enterprise branch uses
  `enterprise.prefs.get` and `locked_prefs.allow_lists`, returning `full` live.
  Selective `users.prefs.get` also parsed and classified this List as non-personal.
  Evidence: tmp/slack-list-live-read-fixed.log and
  tmp/slack-list-live-read-policy.log; reusable probe:
  examples/slack_list_read_probe.rs. No List cells or messages were sent. A transient
  native Slack navigation query entered an empty DM composer, was detected, and
  was cleared with empty/Send-disabled/draft-count restoration verified.
- Locked core suite: 698 passed / 1 ignored; locked desktop suite: 595 passed /
  3 ignored. Signed build and strict deep codesign verification passed. Logs:
  tmp/slack-list-rich-{core-tests,desktop-tests}.log and
  tmp/slack-list-rich-native-build-final.log. The first native normal window
  passed all interactions and rich/reference/hierarchy captures. The narrow run
  exposed header/help text leaving too little row space; the short-height layout
  was revised before final verification rather than weakening the visibility check.
- Final native interaction checks passed at 1280×800 and 668×400, with overview,
  rich-cell, reference-label and expanded-hierarchy captures. The compact layout
  keeps three hierarchy rows fully visible and offers keyboard help through a
  disclosure. Read-only, incomplete, error and loading captures also passed and
  were inspected. Artifacts: tmp/agent-ui/slack-list*.png;
  tmp/slack-list-rich-native.log and tmp/slack-list-rich-states.log. Native receipts
  retain the two exact synthetic writes, duplicate locking, pending canonical
  state, uncertain readback without replay, keyboard navigation, local paging,
  stable refresh/reorder focus, rich styles/links, reference names and hierarchy
  expansion/collapse checks. Final formatting and diff checks passed.
- Live proof in this batch covers read compatibility only, not successful cell
  writes or end-to-end native rendering of the signed-in List. Saved views,
  continuation, personal To-dos, other field editors, and creation remain open.
  No signed-in client restart, storage cleanup, commit or push. Full parity goal
  remains active.

## 2026-10-13 — Typed Lists editors and signed-in write verification

- Three parallel workers handled typed domain mutations, editor/controller UI,
  and independent protocol/visual review; root integrated and verified the live
  client. Existing-row rich text, numbers, single-select and multi-select now have
  explicit Save/Cancel editors. Rich trees and reference labels remain intact;
  numeric zero differs from clear, and option IDs/cardinality come from schema.
  Stale drafts stay visible but cannot write; uncertain saves reconcile without
  replay. No row/schema/option creation is implemented by these editors.
- Review caught real Slack multi-select schemas using `type: select` with
  `options.format: multi_select`, delayed rich-editor mounting into a newer draft,
  initial focus landing on Bold instead of the text area, and mismatched footer
  button sizes. These were fixed with focused regression coverage. The shared
  rich-editor browser suite passed all seven checks, including reused-ID mount
  ownership (`tmp/slack-list-edit-browser.log`).
- Created `Super Platinum Lists verification 2026-10-13` in private
  `#echotestchamber`, verified invite-only access, and added two test fields using
  stock Slack. The older June List was untouched. On the new List, a native
  completion false-to-true save appeared in Slack; resetting it in stock Slack
  reached the native client without manual refresh. New schema fields required
  manual refresh, leaving file-view subscriptions as a documented next slice.
- Native signed-in saves were read back for an initially absent Name field,
  explicit Bold formatting, number zero, decimal 12.5, numeric clear, select Ready,
  and select clear. Number and select test values were cleared afterward. Stock
  Slack displays 12.5 as 13 with precision zero; native currently displays raw
  12.5, a separate formatting gap documented in the protocol notes.
- The first rich save succeeded in Slack but exposed a false native conflict:
  Slack added a root `rich_text.block_id`. Receipt comparison now ignores only
  that absent/string root ID; text, styles, links, unknown attributes, nested IDs,
  and block order remain exact. Original data and submitted payloads are unchanged.
  Read-only evidence: `tmp/slack-list-live-rich-readback.log` and
  `tmp/slack-list-live-rich-bold-readback.log`. Synthetic native readbacks now add
  IDs too and require clean editor closure with no unresolved operation.
- Final locked suites passed 707 core tests (1 ignored) and 603 desktop tests
  (3 ignored). Signed build, strict deep signature verification, formatting and
  diff checks passed. Final native checks passed at 1280×800 and 668×400 including
  generated-ID readback, exact text-area focus, aligned footer controls, and all
  editor interactions. Normal/narrow editor PNGs were inspected and readable.
  Final gates are recorded in `tmp/slack-list-edit-*-final.log`.
- Relaunched the final signed build and populated a second previously absent Name
  field with `Native readback normalization verified`. F2 focused the text area;
  immediate typing worked, Save closed the editor without a conflict notice,
  focus returned to its cell, and stock Slack showed the exact saved text. The
  final signed-in build remains open on the private verification List.
  Live multi-select, saved-view projection,
  cursor continuation, personal To-dos, and creation remain open. No messages,
  storage cleanup, commits, or pushes were performed in this batch.

## 2026-10-13 — List subscriptions, numeric display, and saved-view domain

- Three parallel workers implemented file-view protocol/lifecycle, locale-aware
  number display, and saved-view domain projection. Independent cross-reviews
  caught stale request dispatch across close/offline recovery, unnecessary
  per-tick credential/context cloning, permanent-error retrying, writable empty
  groups, and mixed-type sort comparison risks. Root owns builds and live checks.
- File-view leases use captured files.open/ping/close and temporary socket
  subscribe/unsubscribe frames. HTTP operations serialize per file, pings carry
  window focus at 30-second intervals, closing schedules delayed unsubscribe,
  and reconnect replaces the subscription generation. Queue rejection cannot
  count as a successful subscription. Visible-visit dispatch guards and a narrow
  single-attempt transport path prevent an obsolete request waiting offline and
  sending later. Already-sent HTTP timeout ordering remains an unproven server
  boundary; captured last_updated timestamps are preserved.
- Numbers use schema precision and the Slack locale, with exact decimal-string
  rounding and grouping for the 12 captured locales. Currency follows the same
  Lists decimal display path. A live browser Intl oracle supplied 1,188 cases;
  all passed the focused core run. Stored/editor values remain unrounded. Boot,
  warm cache, and generation-guarded preference events retain locale; local
  re-projection preserves expansion and unsaved editor drafts without API reads.
- Saved-view domain code preserves raw metadata and models table column order/
  visibility, supported typed filters, stable sorting and select/checkbox groups.
  It exposes unsupported layouts/rules explicitly and does not write shared views.
  This domain code is not yet connected to the native picker/table; that user
  workflow remains unfinished. Date filtering, local grouped refinements and
  default selection/read tracking need further implementation and verification.
- Read-only metadata from the private verification List confirmed three standard
  table views and a separate record layout (`tmp/slack-list-live-view-metadata.log`).
  Root created the new grouped table view `Native parity verification` in that
  same private List using stock Slack, and observed empty Backlog/Ready groups
  plus three Ungrouped items. This corrected the initial read-only-only grouping
  model. Exact metadata is in `tmp/slack-list-live-grouped-view.log`; stock Slack
  was returned to All items. No other List or message was modified.

- The initial integration gates passed 733 core tests (1 ignored), 619 desktop
  tests (3 ignored), strict deep signature verification, and native List checks
  at 1280×800 and 668×400. Overview and compact rich/select editor PNGs were
  inspected: controls remained visible, scrolling was contained, and focus was
  clear. Logs: `tmp/slack-list-viewing-{core-tests,desktop-tests,native}.log`.
- Signed-in live verification now proves the file-view subscription path delivers
  schema changes: renaming the private test Select field in stock Slack and
  restoring its name updated the native header without Refresh. After closing
  and reopening the native List, numeric 12.5 displayed as 13 in both clients;
  reopening the native editor retained 12.5. Changing schema precision from 0
  to 2 and back updated native display 13 → 12.50 → 13 without Refresh. The
  temporary numeric value was cleared and original field settings restored.
- One first save after schema edits was rejected by fresh preflight comparison
  before any write; refreshing and reopening the editor allowed the save.
  Investigation identified whole-document equality as a possible overbroad
  guard, but did not weaken it without field-difference evidence. Opt-in
  `SUPER_PLATINUM_LIST_EDIT_TRACE` now emits only fixed allowlisted changed-key
  labels and booleans, never values or arbitrary keys, to diagnose this gap.
- A second parallel review caught a close-completion race: closing a newly
  reopened dialog before its renew-close completed could retain the socket
  subscription indefinitely. Close completion now always arms delayed release;
  a currently visible visit cancels it on the next tick. Added a 60-second
  release regression. Saved-view tests now cover stable grouped instance IDs,
  collapse/reopen counts, incomplete empty pages, unsupported rules on empty
  data, and consistent rejection of unknown task completion policy even when
  no rows are loaded. Final gates for these follow-up changes are pending.

- Live trace on a fresh signed app reproduced the first-open rejection and
  isolated it to top-level `last_read`: all record, column, schema, view,
  permission and personal-policy comparisons matched. Typed preflight now
  ignores only that top-level read marker; all other comparisons remain strict.
  An independent reviewer verified the narrow scope and regression coverage.
  Final rebuild/live first-open verification follows this change.

- Final follow-up gates passed: 738 core tests (1 ignored), 621 desktop tests
  (3 ignored), signed build and strict deep code-sign verification, formatting,
  diff checks, and native editor/interaction checks at 1280×800 and 668×400.
  Logs: `tmp/slack-list-last-read-{core,desktop,build,native}.log`. Latest compact
  rich/select editor captures were inspected and remained readable with visible
  focused controls and aligned actions.
- Relaunched the final signed build and saved 12.5 immediately after the first
  open of the private verification List, without Refresh. Save closed cleanly,
  returned focus to the cell, native and stock Slack displayed 13, and reopening
  the native editor preserved 12.5. Clearing then saved cleanly and stock Slack
  confirmed the original blank value. Final trace contains successful open and
  subscribe enqueue with no preflight mismatch. The signed app remains open on
  this List. No messages, other Lists, storage, commits or pushes were changed.
- Next slice remains native saved-view picker/projection and verified default
  selection/read tracking; the full parity/design/performance goal stays active.


## 2026-10-13 — Native saved-view integration

- Previous batch was verified progress: live file-view subscriptions, number
  formatting, and the first-open last-read preflight fix all passed. This batch
  connects the core projector to the native picker/table rather than treating
  the unfiltered reader as saved-view parity. Three workers own the native
  controller/UI, protocol/default helpers, and independent review/fixtures;
  root owns experiments, native verification, builds and live testing.
- Current authenticated experiment lookup confirmed grid-todo filtering and
  personal-default dual write enabled, rollback disabled. These settings come
  from a validated `experiments.getByUser` assignments response; failures remain
  unknown. Root's scoped private-List probe printed only the three policy flags.
- Native regression cases now include exact reordered/hidden columns, filtered
  numeric sorting, a hidden Name column with a genuinely blank numeric cell,
  multi-select duplicate row instances, empty/collapsed groups, canonical writes
  from duplicate instances, incomplete-page notices, and explicit unsupported
  layout fallback. They extend the existing normal/narrow native harness.
- Independent reviews caught semantic-primary promotion when Name is hidden,
  stale grouped projections on reference-label updates, leaking recursive
  reference traversal budgets, and joined default requests that would force
  completion ordering. These fixes are implemented. Subsequent review added null
  completion-filter compatibility, excluded inline-canvas entries from the picker,
  and allowed a recovered experiment lookup to fetch personal defaults without
  repeating open/read marking. Native validation remains pending.
- Offline optimized-development benchmark (`examples/slack_list_projection_bench.rs`)
  measured 20 projections after five warmups: 1,000 synthetic records median
  0.587 ms / p95 0.615 ms; 10,000 records median 11.414 ms / p95 12.886 ms.
  This measures core filter/sort/group projection only, not native paint or
  network latency. The native integration caches projections outside rendering.
  Artifact: `tmp/slack-list-projection-bench.log`. The first core gate passed
  749 tests (1 ignored); subsequent review fixes require another gate.

- Final core suite currently passes 754 tests (1 ignored), including strict
  malformed-filter rejection and null-default compatibility. Desktop compilation
  exposed two narrow test/type errors, now fixed. Its first execution found a
  prior-intent regression when collapsing after beginning a checkbox edit; the
  follow-up separates selection ownership from geometry before native gates.
- Independent next-boundary research recommends ordinary date/due-date filters
  and sorting. Captured B `+E6Z` around 2846400–2848900 and dispatch 2851157
  define date rules; B 2823540 defines relative presets. Implementation needs
  explicit local calendar/current-time/prior-read context, distinct zero versus
  missing dates, and captured multi-value semantics. Personal reminder timestamps
  require an additional verified `todos` experiment. Cursor continuation still
  lacks a verified request shape: B 7286661–7287900 uses the first-page endpoint
  and consumes the next cursor only as a completeness marker. Keep the incomplete
  notice and do not guess a next-page wire contract. No implementation in this batch.
- Desktop follow-up passed 633 tests (3 ignored), signed build/signature passed.
  Native normal-size run caught an additional DOM identity regression: a new
  per-entry RSX match fragment wrapped each keyed row, recreating focused cells
  during reorder. The renderer now emits direct keyed row VNodes; the existing
  retention assertion remains unchanged. Full native rerun is pending.
- Signed-in private-List testing of the saved-view build resolved the saved
  grouped `Native parity verification` view on first open, displaying empty
  Backlog/Ready groups and three Ungrouped rows. The first checkbox save succeeded
  without Refresh and removed the completed row from that view. Completed items
  showed exactly that one row; resetting it produced an empty Completed view and
  three Incomplete items. Stock Slack independently confirmed false restored.
  This verifies current ordinary todo-mode completion filtering/default selection
  and the narrow checkbox last-read exception, not every default-response branch
  or server read-marker outcome. Safe trace has no preflight mismatch.
- Full normal/narrow native suite passed after the row-key fix, including saved
  columns, grouping, duplicate canonical writes, keyboard controls and original
  editor/focus checks. Normal/narrow saved-column/group and narrow select-editor
  PNGs were inspected: controls, focus and footer remain visible and readable.
- Final-build live group edit changed the private first row from blank to Ready,
  moving it from Ungrouped to Ready; stock Slack confirmed Ready. Clearing it
  restored Ready 0 / Ungrouped 3 and stock Slack confirmed the blank value.
  This exposed a focus-return gap when the originating group instance disappears:
  editor fallback selected Close before the replacement canonical cell. A narrow
  canonical-cell fallback and native regression are being added before final gate.
- The new native regression explicitly focuses Save, removes the originating
  Design instance of a multi-group row, and expects the surviving Engineering
  cell. It reproduced an observer conflict: the grid recovered focus from the
  disabled editor Save button before editor teardown. Grid recovery now excludes
  editor-owned focus, leaving canonical return to the editor observer. Paging
  recovery is unchanged. Signed rebuild and native rerun follow this JS-only fix;
  the latest Rust desktop suite still passes 633 tests (3 ignored).
- Final saved-view gates passed: 754 core tests (1 ignored), 633 desktop tests
  (3 ignored), formatting and diff checks, signed app build/strict deep signature
  verification, and full native checks at 1280×800 and 668×400. The final JS-only
  focus adjustment was rebuilt and exercised by both native runs. Native receipts
  include `groupedEditFocus: true`; latest narrow grouped/rich-editor images were
  inspected again. Logs: `tmp/slack-list-saved-views-{core-final,desktop,build,native}.log`.
- Final signed-build live verification explicitly focused Save with Tab and
  activated Return: the first private row moved blank → Ready and retained focus
  on its Verification stage cell. F2 immediately reopened that same cell. Clearing
  the value returned it to Ungrouped and kept focus on the corresponding cell.
  Stock Slack confirmed Ready and then the restored blank value; all three rows
  remain incomplete. The signed app is left open on the grouped verification List.
- This finishes the saved-table integration slice, not full Slack parity. Date
  filters, personal To-dos completion/reminders, non-table layouts, server cursor
  continuation, view/schema/row creation and conflict semantics remain open.
  The full parity/design/performance goal stays active. No messages, unrelated
  Lists, commits, pushes, deployment or storage cleanup were performed.


## 2026-10-13 — Saved-view date projection

- Previous turn made verified progress: saved tables, defaults, completion
  filters, grouping and focus-preserving edits passed domain/desktop/native/live
  gates. This batch adds captured date filtering/sorting and bounded calendar
  invalidation; three parallel workers own core, desktop, and independent
  source/fixture review. Root owns live reference setup, experiments, integration
  gates, protocol probes and documentation.
- Created `Native date verification` (`View0C8X4GLKPW`) only in the existing private
  verification List. Stock Slack saved a Today filter on `todo_due_date` with
  `operator: equal`, empty `typed_values`, and `values: [today]`. The first
  verification row was temporarily assigned 2026-10-13; its wire field contains
  `date: [2026-10-13]`, `timestamp: [-1]`. Today matched only that row.
  Probe: `tmp/slack-list-date-live-source.log`.
- Added 2026-10-14 temporarily to the second row and saved This week plus descending
  Due Date sort. Slack shows second row (14th), then first (13th), excluding the
  third undated row. Wire sorts are `ascending: false`, `column_id: Col02`,
  `key: todo_due_date`; filter values are `[this_week]`. Probe:
  `tmp/slack-list-date-live-week-sort.log`. Both temporary dates require restoration
  after final live verification. Personal default remains Native parity verification.
- Date timestamp preference uses separate `todos` experiment, personal assigned
  or first task-list identity, and rollback off. Added the separate assignment to
  the validated experiment adapter rather than reusing `lists_grid_todos`.
  Implementations and final native checks are in progress.

- Final core suite passes 766 tests (1 ignored). Desktop suite passes 638 tests
  (3 ignored), including independent-review remediation: calendar-dependent
  layouts reproject at bounded minute samples even for same-current-offset
  system timezone changes. Cached rows remain shared between samples; hidden,
  obsolete-account and deterministic-clock guards remain.
- Projection benchmark (20 measured runs after 5 warmups, local dev profile):
  number scenario 1k median 0.605 ms / p95 0.633 ms; 10k median 11.920 ms /
  p95 13.109 ms. Date-filter/sort/group scenario 1k median 0.620 ms / p95
  0.637 ms; 10k median 8.513 ms / p95 9.774 ms. These are synthetic core
  projection measurements, not native frame times or a cross-machine guarantee.
  Evidence: `tmp/slack-list-dates-bench.log`.
- Authenticated experiment probe confirms `todos: false`, `grid_todos: true`,
  rollback false and default-view dual write true for this session. This directly
  supports keeping the date timestamp policy distinct from the grid experiment.
  Evidence: `tmp/slack-list-dates-live-final.log`.
- Final signed bundle passes strict deep codesign verification. Native harness
  passes both normal and narrow fixtures after the timezone fix, including all
  existing editor, grouping, focus, paging and read-only date-view cases.
  Inspected Today and week captures at both sizes; rows and descending dates
  are correctly ordered with usable compact controls. Metadata timestamp text
  still uses the existing raw fallback; relative display is a separate follow-up.
  Evidence: `tmp/slack-list-dates-build-final.log`,
  `tmp/slack-list-dates-native-final.log`,
  `tmp/agent-ui/slack-list{,-narrow}-saved-date-{today,week}.png`.
- Signed-in private live proof: selecting Native date verification in the new
  signed client shows 14th then 13th, 2 items / 3 loaded, matching stock Slack's
  saved This week / descending Due Date view. Saving Today in stock Slack
  automatically changes the native selected view to just Native editor
  verification (13th), 1 item / 3 loaded, without Refresh or reopening.
- Restored both temporary due dates through stock Slack Clear selection. Native
  Today view becomes empty automatically; switching native back to Native parity
  verification shows all three rows with blank dates, number/select cells and
  unchecked completion. Stock remains All items with the same restored rows.
  Final guarded readback: `tmp/slack-list-dates-restored.log`. The new private
  date view remains saved as Today with descending Due Date; the personal default
  remains Native parity verification. No messages were sent.
- Full parity remains unfinished. Independent source audits identify existing
  saved Board rendering and typed relative timestamp display as supported next
  slices. Available captured records.list callers still do not establish a
  continuation cursor request: next_cursor only marks records incomplete. Keep
  the incomplete-page notice; do not invent cursor fields or claim complete data.
  Personal To-dos reminders, full DST/live preset matrix, date edits and creation
  remain separate work.
- Next-slice source pointers (read-only audits, no implementation claim): B
  BoardView 17959600–17962200 promotes primary card field and removes grouping
  field; 5QWf.B_ 2839299 applies typed grouping.order; Q81I 18314600 splits
  multivalue memberships; 18302721/18307336 handles empty Ungrouped and filters.
  Board lane empty/order rules differ from table grouping. Timestamp display:
  B18144165 eoW4.TimestampToken sets relative/alwaysAgo; B19675972 refreshes
  every minute; S813840 CFnj supplies thresholds; S810358 nL6P treats numeric
  below 1e10 as seconds, otherwise milliseconds. Keep display units separate
  from captured date-sort semantics. Canonical created_time reads record
  date_created; last_edited_time uses its field timestamp, with no invented
  updated_timestamp fallback.

## 2026-10-13 — Native Board views

- Previous goal turn made verified progress: date filters/sorts and bounded
  calendar invalidation passed core, desktop, signed native and private live
  gates. This batch adds existing saved Board views using three workers for
  pure projection, native rendering, and independent source/native-fixture
  review. Root owns integration, benchmarks and signed-in private proof.
- Created Native board verification (`View0C8RJFB98W`) in the same private
  verification List `F0C8MDD6B8D`, under #echotestchamber. Stock All items →
  View as board automatically groups by `todo_completed` / `Col00`; saved
  `type: board`, `show_completed_items: true`, original visible columns.
  Open contains all three unchecked records; empty Completed lane remains.
  Task cards retain a header completion checkbox although grouping removes
  that column from normal card fields. No row values changed. Source probe:
  `tmp/slack-list-board-live-source.log`.
- Created a second private saved Board, Native stage board verification, grouped
  by Verification stage. Stock shows Ungrouped with three cards, plus empty
  Backlog and Ready lanes. Screenshot confirms header completion remains and
  the ordinary Completed body field is suppressed (AX card summary mentions
  its value; that is not a duplicate visual field). Source probe:
  `tmp/slack-list-stage-board-live-source.log`. Implementation/gates pending.
- Core Board implementation passes 774 tests (1 ignored); first desktop gate
  passes 640 tests (3 ignored). Source review corrected typed lane order to use
  the first select value and constrained header completion to canonical
  `todo_completed` key+type. Native keyboard review found and is addressing
  empty-card focus, field traversal, skipping empty lanes, and final-page pager
  focus ownership. Final signed native checks remain pending.
- Synthetic pure projection benchmark: Board 1k median 0.621 ms / p95 0.636 ms;
  10k median 12.892 ms / p95 14.095 ms (20 measured runs, 5 warmups, local dev
  profile). Same run table number scenario 10k median 12.684 ms; date scenario
  10k median 8.290 ms. This measures projection, not native layout/paint.
  `tmp/slack-list-board-bench.log`.
- First signed native run caught an actual focus regression while paging
  Ungrouped: cards advanced to 11–20 but focus jumped to Engineering. The
  previous RAF could run before Dioxus replaced the old cards. Remediation
  waits for actual DOM replacement with a scoped pager intent; checks also
  cover all-empty lane pages returning focus to an enabled pager. No passing
  native or live Board claim is made until those cases pass.
- Final signed bundle passes strict deep signature verification and the full
  native List suite at normal and narrow sizes after the pager readiness fixes.
  Receipts cover lane order/membership, empty lanes/cards, horizontal scrolling,
  bounded card/lane paging, field traversal, same-lane final-page focus, empty
  lane page fallback, duplicate canonical checkbox writes, number editor, and
  task-header completion moving a card with focus preserved. Inspected all six
  new Board captures (multi, paging, completion at both sizes); compact controls,
  keyboard disclosure and scroll boundaries are usable. Evidence:
  `tmp/slack-list-board-native-final.log`,
  `tmp/slack-list-board-build-final.log`, `tmp/agent-ui/*-board-*.png`.
- Signed-in live private proof: Native board verification initially matches
  stock Slack Open 3 / Completed 0. Clicking Native editor verification's native
  checkbox moves its card to Completed (Open 2 / Completed 1); stock Slack
  independently shows Completed Yes. Focus follows the canonical completion
  field. Space restores unchecked/Open 3, with focus following back; stock
  confirms all three unchecked. No Refresh was needed.
- Native stage board verification (`View0C8VFXB9CH`) matches stock Ungrouped 3,
  Backlog 0, Ready 0. Setting the first row's Verification stage to Ready through
  stock All items automatically moves the native card to Ready 1 / Ungrouped 2.
  Stock Clear selection restores native Ungrouped 3 / Ready 0. All rows remain
  undated with blank estimate/stage and unchecked completion. No messages sent.
- Keyboard-only live proof: from native view picker, Tab through Refresh/Close
  into first completion field, Tab to Name, F2 opens the existing rich Name
  editor with bold content preserved; Escape closes it and returns focus to
  that Board Name field. No name mutation performed. Signed live app remains
  open on the restored stage Board; stock remains All items. Personal default
  remains Native parity verification.
- Full Slack parity remains unfinished. Existing Board reads and supported cell
  edits now work; dragging cards, lane management, card creation, record details,
  additional grouping types, and unverified records continuation remain separate
  gaps. No claim that these supported Board cases establish complete Board parity.

### Record details continuation

- Three workers cover explicit record-detail authority, native presentation and
  source/fixture verification; root handles integration and signed live proof.
  Stock Slack private List table context menu → Open item and stage Board card
  click both expose the same Item pane. It shows primary Name, Mark Complete,
  Assignee, Due Date, Verification estimate and Verification stage. The grouping
  field is available despite being omitted from the Board card.
- An exploratory stock inline-name interaction normalized the verification name
  to plain text; restored its original bold style through the native rich editor
  toolbar. Read-only probe confirms the original `bfz4L` block, text and bold
  style in `tmp/slack-list-details-restored-name.log`. No messages sent.
- Scope: loaded canonical record details and existing supported cell writes.
  Stock `lists.records.open` tracking, record thread/comments, creation and other
  item actions remain separate gaps. Implementation and verification in progress.
- Desktop gate passes 647 tests (3 ignored), including seven new detail guard
  tests. Core domain files were unchanged in this slice. A first signed native
  run caught `inert="false"` remaining active: the pinned Dioxus interpreter does
  not treat `inert` as a boolean attribute. Optional attributes now remove it
  completely when the layout is active. Subsequent native run passed existing
  table and Board checks, then exposed nested editor focus readiness. Focus
  diagnostics are being collected; no passing native/live detail claim yet.
- Final record-detail signed native suite passed at normal and narrow sizes;
  all six detail captures inspected. Includes hidden-field writes, canonical
  return focus after lane move, 30-field paging, stable panel/page/focus across
  read refresh with fresh action generations, stale draft retention, view-change
  cancellation and removed-record read-only state. The editor readiness helper
  now observes field focus before F2; mount diagnostics remain available.
  Evidence: `tmp/slack-list-details-native-focus.log`,
  `tmp/slack-list-details-build-focus.log`, `tmp/agent-ui/*-detail-*.png`.
- Resumed signed-in native proof: stage Board picker → keyboard Tab to Name →
  Alt+Enter opens detail → Tab/End/F2 opens hidden Verification stage → keyboard
  selects Ready and saves. Fresh guarded server read independently confirms
  `Rec0C8WCXSR1S` / `Col0C8SJ9070S` has `OptQQIK872V`; Back shows Ready 1 /
  Ungrouped 2. Reopen details and No selection restores empty stage. Stock app
  reference view did not give reliable fresh state in this continuation, so
  server readback is the independent confirmation, not a stock realtime claim.
- Restored original first-name bold formatting after observing stock-name
  normalization again. Final guarded snapshot checks all three completion values
  false and stage/estimate/due date empty; first Name is bold. Evidence:
  `tmp/slack-list-detail-live-fields.log`,
  `tmp/slack-list-detail-live-final-baseline.log`. No messages sent.

### Calendar-date editor continuation

- Three workers now implement core calendar-date value guards, native editor,
  and source/fixture checks. Captured ordinary calendar date writes use
  `date:[YYYY-MM-DD]` or `date:[]` on the existing canonical cell endpoint.
  Personal task timestamp/Later editing remains separate; the calendar editor
  must not reinterpret those records. Signed native and live gates pending.
- Implementation adds strict Gregorian calendar set/clear for ordinary date,
  due-date and nonpersonal task-date cells. Timestamp-only/Later values, ranges,
  malformed values and incompatible timestamp siblings stay read-only. Native
  Save reads validity and value atomically, locks the captured input immediately,
  and preserves stale/cancel/account/record guards. Invalid partial input cannot
  become a clear operation. The explicit Clear button changes only the draft.
- Rust gates pass: desktop 649 tests (3 ignored), independently locked core 779
  (1 ignored), formatting and diff checks. Signed bundle passes strict deep
  verification. Native fixtures cover leap dates, clear, invalid recovery,
  pending locks, duplicate clicks, hidden detail fields, stale drafts and focus
  after a date-filtered row disappears. First native attempt stopped at the
  existing first-capture animation-frame wait before date checks; inspected its
  rendered List screenshot and rerunning with display sleep inhibited.
- Runner activation was one-shot before `finishedLaunching`; it now waits for
  the isolated app to be active and maintains activation during native checks.
  Full normal/narrow suites then passed, and all four date captures were read:
  `tmp/slack-list-date-edit-native.log`, `tmp/agent-ui/*-date*editor.png`.
- Live private List proof set first record Due Date to 2026-10-14; fresh server
  read confirmed `date:["2026-10-14"]` with `timestamp:[-1]`. Deleting just the
  native month segment and pressing Save produced a validation error; fresh
  server read confirmed the saved date was preserved. This real segmented-input
  check also caught Clear failing on already-empty serialized invalid input.
  Cancel/reopen then Clear/Save restored baseline; server snapshot confirms all
  three rows unchecked with dates/stage/estimate blank and original Name bold.
  Evidence: `tmp/slack-list-date-edit-live-{set,invalid,restored}.log`.
  WebKit partial-input Clear correction and its final gates are pending.
- Parallel preparation identified the next bounded List gaps. Explicit record
  opening has a captured `lists.records.open` side effect (`list_id`, `id`,
  `_x_reason=open-record`); use the existing no-hold/one-attempt read-state
  transport and stable explicit-open identity, never resend on refresh. Blank
  root creation is source-backed with `position:""` plus the captured HLC
  timestamp, but must reject unknown/over-limit metadata: stock creation can
  archive the oldest item at the row limit. Creation response/defaults and
  uncertain-result reconciliation still need live evidence. These actions are
  not implemented by the calendar-date slice.
- Final Clear correction is verified in the rebuilt signed app. WebKit skips
  updating internal segments when assigning an already-empty serialized value;
  Clear now forces a valid-to-empty transition in the same JS task only for
  empty invalid input. Live sequence: save 2026-10-14, reopen, month Backspace,
  Save rejected, Clear resets month/day/year to empty, Save closes editor and
  returns blank Due Date. Final independent server snapshot confirms the full
  original baseline in `tmp/slack-list-date-edit-live-final-baseline.log`.
- Final gates after the correction: formatting, 649 desktop tests (3 ignored),
  signed bundle and deep verification, complete normal/narrow native List suites,
  four refreshed date captures inspected, Python runner syntax and diff check.
  Core remains at 779 passing tests (1 ignored), unchanged since that passing
  independently locked run. Personal-task timestamp/Later date editing and
  broader List actions remain outside this verified calendar-date scope.

### List item creation continuation

- Previous turn made verified progress: calendar date set/clear passed native
  and live checks with the private baseline restored. Three workers now cover
  creation core/API/clock, native state/UI, and source/fixture verification.
- Source confirms header Add item creates an ordinary blank root in any saved
  view, with only `completed_items` seeding the first `todo_completed` column.
  Group/lane-specific creation is a distinct path. Fresh guarded private List
  metadata reports row count 3, limit 5000 and `over_row_maximum:false` in
  `tmp/slack-list-create-live-limits.log`; no creation dispatched yet.
- Root extracted existing-cell mutation ownership into `mutations.rs` to keep
  the shell controller focused, added creation lifecycle integration and a
  canonical detail opener for accepted created rows hidden by saved filters.
  The opener preserves saved-view selection and will not replace a user-opened
  detail. Builds, fixture captures and live creation proof remain pending.
- Parallel final review fixed two recovery edges: a later Open item preserves
  the original accepted receipt while allowing concurrent canonical field/schema
  changes, and confirmed-but-unavailable items have an explicit Dismiss action.
  Post-dispatch connection loss is now uncertain even when transport health
  becomes offline; only the known-offline pre-dispatch gate is definitive.
  A TCP regression verifies accepted POST plus lost response is never replayed.
- Current locked gates pass: 791 core tests (1 ignored), 658 desktop tests
  (3 ignored), formatting, diff check, signed build and strict deep signature
  verification. Native normal/narrow runs and live creation proof are pending.
  Dismiss stage ownership is unit tested; the native checker currently exercises
  successful readback and uncertain acknowledgment rather than missing-item Dismiss.
- Complete normal/narrow native List suites passed after the final changes:
  `tmp/slack-list-create-native.log`. Inspected all four creation captures
  (`tmp/agent-ui/*-list-created.png` and `*-list-create-uncertain.png`): header
  actions, filtered-item details, recovery copy and acknowledgment fit both widths.
- Signed live app created and named two items in the private verification List
  F0C8MDD6B8D, reached through #echotestchamber Files & links. Ordinary header
  creation produced Rec0C8P37RW8M (Native creation verification 2026-10-13,
  unchecked); Completed-view creation produced Rec0C9NPBM8RE (Native completed
  creation verification 2026-10-13, checked). Both opened canonical details and
  saved their Name through the existing rich-text editor. Independent server
  reads verified exactly those two additions and byte-equivalent projected fields
  for the original three records. Evidence: `tmp/slack-list-create-live-{before,
  first,views,completed,final}.log`. The two named test items remain in this private
  List; no deletion was attempted. Returned UI to Native parity verification.
- Creation is verified progress, not full List parity. Group/lane-specific and
  subitem creation, broader field types/actions and record-open tracking remain.
  Next bounded source-backed slice: one `lists.records.open` request per explicit
  detail opening (`list_id`, `id`, `_x_reason=open-record`), no replay on refresh
  or paging and no failure-driven loss of local detail. Captured B t0NX@7300063
  dispatches wCCe.LO before routing; V wCCe@9785910 owns the request wrapper.

### Explicit List item-open tracking continuation

- Previous goal turn made verified progress: live ordinary/Completed creation
  and canonical name editing passed, with original private records unchanged.
- Captured caller fires `lists.records.open` without awaiting or consuming its
  response before routing locally. Implementing one mount-owned request per
  explicit item opening, guarded by stable detail-open identity and account/List
  scope. Refresh/paging must not resend; failure must not replace canonical
  fields or make details unusable. There is no evidence for additional response
  fields or specific unread-counter effects in the captured caller.
- Core builder and exact-path no-hold transport gates are implemented; the
  independently locked core suite passes 792 tests (1 ignored). Desktop/native/
  live request acceptance verification remains pending.
- Parallel broader audit identifies higher-impact next work after this bounded
  lifecycle addition: ordinary message checkbox/multi-select controls (currently
  unsupported siblings block whole-message actions), existing canvas body editing
  and collaboration, and measured live conversation/thread responsiveness. These
  remain open requirements, not implied complete by the List work.
- Next app-control source handoff: B M0Sk@1611609 and cxJV@1750345 distinguish
  absent `initial_options` from explicit `[]`; section type-only state and actions
  omission must survive that distinction. iGIl@1742940 lets saved empty selections
  override defaults. AM6q@1754021 submits complete raw `selected_options` objects,
  including `[]` for deselection. Checkbox s+nw@132217xx retains mounted options
  and updates/dispatches only after confirmation; do not recanonicalize old raw
  selected siblings against a newly changed option list. Multi etBz@132807xx
  resets display for changed truthy initial options (including empty arrays),
  not arbitrary option-list changes. s3Wf@1598455 excludes ordinary message/thread
  from immediate dispatch: use a picker draft with Apply/Cancel and max checking.
  Existing modal `InputValue::Selected(Vec<String>)` / raw `OptionSpec` can help,
  but modal submission/confirmation policy is not ordinary-message policy.
- Final record-open gates passed: 664 desktop tests (3 ignored), 792 independently
  locked core tests (1 ignored), formatting/diff checks, signed build with strict
  deep verification, and full normal/narrow native List suites. Native receipts
  prove explicit-open counts, no refresh/field-paging replay, and created-item
  mount tracking. Inspected both refreshed detail-table PNGs. Logs:
  `tmp/slack-list-record-open-{desktop,core,build,native}.log`.
- Live signed diagnostic run opened the existing private record Rec0C8P37RW8M
  in F0C8MDD6B8D. Slack accepted one request at opening generation 15. A completed
  canonical Refresh preserved the panel and produced no extra request. Back and
  explicit reopen produced exactly one accepted request at generation 20.
  `tmp/slack-list-record-open-live.log` records only structural tracking fields
  and static success/error classes; no API response bodies or credentials.
  Independent readback in `tmp/slack-list-record-open-live-fields.log` confirms
  all five records have identical fields to the preceding creation checkpoint.
  This proves request acceptance and ownership, not undocumented unread effects.
- The same warm debug launch recorded first paint 1632.2 ms, cache loaded
  1884.3 ms, conversation paint 2436.1 ms, realtime connected 3751.6 ms. These
  are one-run baseline observations, not paired evidence of a speed improvement.
  Diagnostic process exited cleanly; no persistent tracing settings were changed.

### Ordinary message checkbox and multi-select continuation

- Implemented ordinary message checkbox groups and static multi-selects through
  the shared message renderer. Message actions retain raw option objects and
  include sibling button/select/radio/checkbox/multi state. Absent defaults,
  explicit empty arrays, and an untouched absent multi Apply stay distinct on
  the wire. Checkbox confirmation commits only on Go; multi confirmation is
  integrated into a draft picker with Apply/Cancel and an enforced selection cap.
- Account, source, container revision and mounted-control ownership guard queued
  handlers and picker submissions. Changed initial arrays reset multi display;
  option updates do not recanonicalize retained selections. Cancel, navigation
  and account replacement retire uncommitted drafts.
- Independently locked core suite passes 804 tests (1 ignored); desktop suite
  passes 673 tests (3 ignored). Native signed build and normal/narrow interaction
  verification are in progress. Final gates must include the 80-option scrolling,
  filtering/focus stress case and actual inspection of the captured PNGs.
- No verified private app-action target is established yet. Prepared fixture
  requests prove serialization and native behavior only, not live app receipt.
  A read-only history audit is restricted to private #echotestchamber; no arbitrary
  third-party app action may be used as a substitute for an owned test target.
- Source-backed review also found retained multi selections must remain removable
  even when no longer present in the current option list. The picker exposes
  selected tokens separately; removal keeps trusted raw siblings intact and
  unavailable values cannot be re-added. Cancel restores the mounted selection.
- Fresh guarded private-channel history audit completed two pages / 342 messages:
  five messages contain eight buttons, and none contain checkbox or multi-select
  controls. The temporary read-only probe was removed. This confirms the current
  history has no target for this slice; thread replies were not separately scanned.
  Log: `tmp/private-message-controls-read.log` (structural metadata only).
- Next canvas boundary audit: an existing checklist-item toggle is smaller than
  arbitrary body editing, but still needs captured collaboration initialization,
  `/-/edit-document` envelope, binary acknowledgement and canonical readback.
  Current HTML export has section IDs/checked state but not edit sequence/session
  or locks. Cached `ToggleSectionCheckedState` is only an unexported descriptor;
  the real caller mutates section state through collaboration, so it is not a
  safe shortcut. Do not enable the read-only rendered checkboxes without this
  protocol evidence. Public `canvases.edit` remains incompatible with the current
  desktop token as recorded above.
- Final gates passed: 674 desktop tests (3 ignored), 804 independently locked core
  tests (1 ignored), formatting/diff checks, signed bundle and strict deep signature
  verification. Both normal/narrow multi native runs prove 10 prepared requests,
  untouched-absent omission, explicit empty selection, integrated confirmation,
  duplicate rejection, retained selection removal, stale/account/navigation guards,
  and 80-option scrolling/filtering/keyboard focus. Existing radio and static-select
  normal/narrow native suites also pass. Logs: `tmp/message-multi-{desktop,core,
  build,native,radio-regression,select-regression}.log`.
- Inspected final normal/narrow picker PNGs under `tmp/agent-ui/`; removed WebKit's
  redundant native search bevel and retained the standard compact field/focus ring.
  Native harness readiness now polls only before one click, and navigation checks
  actual old-node disconnection rather than a fleeting empty viewport. The initial
  failures were harness timing checks; no controller guards were weakened.
- Reopened the signed normal-session app and verified Messages in private
  #echotestchamber. No live app actions were dispatched. Full Slack parity, canvas
  collaboration and measured performance work remain active beyond this slice.

### Thread viewport indexing and cached spacer projection

Replaced whole-thread selection scans with cached canonical source indices, and cached the history/live render pieces using cumulative heights. Accepted viewport receipts now reproject the selected rows without reconstructing all lightweight thread metadata. Canonical history/realtime refreshes still rebuild the index; selected identity, pending-state, or bag-length drift rejects the indexed projection and the shell falls back to canonical synchronization. Root and pending rows remain selected outside the viewport, and timestamp-bearing omitted messages still break author grouping.

Standalone CPU probe: `scripts/bench-thread-window.rs`, compiled with `rustc --edition 2024 -C opt-level=1`. Baseline executable was captured before the patch; five alternating before/after process pairs each measured 20 batches of 100 calls after warmup. The patched probe also compares old reference walks and new paths within the same process, checks their output equality, and holds the comparison viewport fixed. Results are the median of each process's median per-call time (milliseconds):

| Loaded rows | Selection before / indexed | Render pieces before / cached | Full sync before / after | Viewport measure + align after |
| --- | --- | --- | --- | --- |
| 200 | 0.010517 / 0.000474 | 0.010738 / 0.000434 | 0.014777 / 0.015352 | 0.000289 |
| 2,000 | 0.060149 / 0.000310 | 0.061533 / 0.000417 | 0.123200 / 0.118146 | 0.000272 |
| 10,000 | 0.274447 / 0.000347 | 0.277745 / 0.000470 | 0.592169 / 0.648118 | 0.000338 |

At 10,000 rows, the patched executable's same-process linear references measured 0.234619 ms for selection and 0.264116 ms for render pieces, corroborating the removed scan cost. Full synchronization is approximately 9% slower at that size because it builds the additional numeric geometry caches; the saving is on repeated viewport-only updates and ordinary renders. Additional cache memory is linear in loaded lightweight rows, without duplicating rich message VMs. Height changes rebuild cumulative geometry; estimated offset-to-row lookup remains linear.

Evidence: `tmp/thread-window-cpu-paired.json`, `tmp/thread-window-cpu-{before,after}-{1..5}.log`, and preserved `tmp/bench-thread-window-before` executable. These are synthetic geometry/index CPU measurements, using the production ThreadWindow with small synthetic core adapters. They do not measure full rich-message projection, Dioxus reconciliation, browser layout/paint, native interaction latency, network behavior, or real Slack data. Sub-microsecond timings are batch averages, not individual frame-latency claims. No profile settings changed.

New regressions compare cached paint pieces and actual rich MessageVm output against the former linear reference, including unloaded gaps, missing timestamps, pending/root inclusion, height eviction, viewport alignment, scope changes, same-length reorder, pending changes, and shell fallback after a prepend. Scoped rustfmt, `git diff --check`, and the standalone probe passed. Final desktop/native gates are recorded below.

A focused followup also retires same-visit viewport receipts after a canonical synchronization, even when timestamps and input generation stay unchanged. A separate projection generation, advancing within each window lifetime, is emitted on `#thread-body` and checked against both the captured Rust generation and sampled DOM generation. Rejected samples use the existing queued fresh-sample path. Viewport-only measure/align does not advance this generation, avoiding a self-invalidating measurement loop. Pure regression covers unchanged-ID content edits, stale pre-commit DOM, absent/malformed DOM tokens, valid fresh receipts, viewport-only updates, and scope changes. The final root validation below includes this guard.

- Final locked desktop suite passed: 681 tests, three ignored; doctests, `cargo fmt --check` and `git diff --check` passed. Log: `tmp/thread-index-desktop-final.log`. This slice changes desktop geometry only; the independent core suite was not repeated.
- `scripts/macos-app.sh` built the final bundle and strict deep codesign verification passed. Log: `tmp/thread-index-signed-build.log`.
- Final signed native thread driver passed all five fixtures (200, 2,000, 10,000 replies, exact target and history gap), including five viewport jumps each, root/pending retention, at most 102 mounted rows, exact initial target, page-gap placement and stable latest restoration. Log: `tmp/thread-index-interactions.log`; detailed receipts and inspected final screenshots: `tmp/agent-ui/thread-window-{200,2000,10000,target,gap}.{log,png}`. Screenshots are taken after the final jump to latest; target/gap initial conditions are in the receipts.
- Final signed selection lifecycle driver passed channel outside-release text retention, channel/thread virtual jumps, pointer-cancel/blur release, latest restoration and both selection pins cleared. Root inspected `tmp/agent-ui/selection-lifecycle.png`; log: `tmp/thread-index-selection.log`.
- The indexed CPU timings above predate the final constant-time projection-generation guard. The approximately 9% full-sync difference is an observed paired median with variation, not an isolated exact overhead. Native driver timings include deliberate settling delays and are correctness evidence only. The same-visit stale-receipt race has a pure regression and final native smoke, not a deliberately interleaved native race test.

### Existing-canvas write protocol captured from stock Slack

- Opened the previously created private, owner-only unshared canvas in stock Slack through Playwright CDP. A fresh read-only probe checked account/workspace, private test channel membership, canvas owner/editability, empty channel/group/DM shares and no workspace/org access before inspecting it.
- Toggled one existing checklist item on and restored it off. Both protobuf edit acknowledgements and independent fetch-latest responses confirmed the exact section/state; authenticated HTML export eventually confirmed the original unchecked/checked pair. The immediate HTML export lagged the first accepted edit, so it must not be used alone to infer write failure.
- Captured and inspected the missing editor controller chunk. `docs/slack-canvas-edit-protocol.md` records the verified binary envelope, session/canonical/local sequence distinctions, partial-state and CRDT requirements, acknowledgement handling, and outstanding native implementation. No general canvas editing capability is claimed.
- Sensitive captures remain gitignored. The temporary authenticated probe was removed; the owned capture process stopped normally after 11 requests; stock Slack returned to its original Home conversation. No messages were sent. Full Slack parity remains active.

- Reopened the rebuilt signed client normally and verified Home → #echotestchamber with an empty composer. Task-owned capture and fixture driver processes have exited. No commit, push or storage cleanup occurred.

### Canonical canvas reads and guarded checkbox preparation

- Added bounded protobuf decoding with lossless unknown-field preservation, a canonical collaboration loader, and a one-section checkbox intent builder. The loader checks file/thread/document identity, account initialization, section associations and the complete section index; incomplete or unsupported response shapes fail closed. The binary read transport is restricted to observed read endpoints with bounded request/response sizes and timeouts.
- A freshly guarded read of the task-owned, private unshared canvas succeeded through the production loader: six canonical sections and two existing checklist items. Both items initially lacked CRDT state. Captured stock source shows that this state is created locally with Automerge when the first edit is prepared, rather than fetched by a missing read endpoint. No native mutation was sent.
- Added genuine Automerge initialization for ordinary unmarked canonical text. It preserves Unicode, uses the editor's escaped-text representation and end sentinel, and creates typed local provenance without inventing a server expiry. Existing server sessions retain their expiry/shape validation. Rich or unsupported content stays unavailable for this initial path. The exact pinned dependency and both lockfiles were updated without refreshing unrelated packages.
- Desktop checklist lifecycle state now retains potentially sent operations across close/reopen, workspace changes and credential/transport rotation. Stable account/team/user/file identity prevents duplicate edits; immutable full request scope still guards dispatch and reads. A matching accepted acknowledgement can reconcile through a new authorized canonical read, while an uncertain request never clears merely because its boolean matches. Capacity is explicit (eight per stable owner, 32 total), with no silent eviction. An independent agent reviewed the corrected ownership and capacity boundaries.
- At this foundation checkpoint, the canvas checkbox UI was not enabled. Mutation transport, reader mapping, visible pending/error states and native private toggle-and-restore verification were still outstanding; their subsequent implementation and proof are recorded below. Protocol details and the next verification boundaries are in `docs/slack-canvas-edit-protocol.md`.

- Locked validation passed: 693 desktop tests (three ignored) and 834 independently locked core tests (two ignored), plus both doctest gates. The explicit ignored synthetic-export gate also passed. Logs: `tmp/canvas-bootstrap-desktop-final.log`, `tmp/canvas-bootstrap-core-final.log`, `tmp/canvas-bootstrap-export-final.log`.
- Captured Slack JavaScript Automerge and native Rust interoperate in both directions for all six synthetic cases (plain text, empty text and varied Unicode). Exact decoded changes and heads match when given the same actor and commit time. The first comparison exposed JavaScript's current-Unix-seconds commit default versus Rust's zero default; native preparation now supplies its explicit clock. The isolated oracle loads only library modules and the matching public WASM; it contains no Slack session or real document content. Final proof: `tmp/canvas-automerge-roundtrip.json`; repeatable synthetic fixtures live beside the bootstrap tests.
- A final guarded live canonical read confirmed both real checklist items are within the supported plain-text subset, but preparation is still deliberately rejected: initial canonical Document field 69 is absent, while the captured outgoing edit contained title metadata. Source-backed title construction remains a prerequisite; the guard has not been removed by assumption. Structural-only evidence: `tmp/canvas-bootstrap-live-shape.log`. No native edit endpoint was called, and no UI/native visual claims apply to this foundation.

- Followup source investigation located the missing title path: stock save calls `extractTitleMetadata(true)` and converts the non-deleted static title section to RichTextApi; Document field 69 is not copied. The captured canonical title is Section type 64/style 48, with authoritative text at Content field 58 / ContentTitle field 1. Both captured outgoing DocumentData field 14 values contain rich_text → rich_text_section → text and match that canonical title. Strict source-bound title synthesis is the next small implementation boundary; no guard was relaxed in this checkpoint. The temporary authenticated probe was removed. Final root/core formatting and `git diff --check` passed.

### Native checkbox title mapping and first endpoint verification

- Replaced the incorrect Document.69 dependency with source-bound canonical title-section synthesis. Generated form title and RichTextApi bytes match both captured stock requests exactly. Detached Chromium parser checks verified control/space/Unicode normalization; unsupported rich/empty titles and UTF-16 cuts through a surrogate remain explicit boundaries.
- Added the exact edit-document form constructor, a consumed preparation API and a dedicated, lazily pooled mutation HTTP client with redirects and even library-level protocol retries disabled. Pre-dispatch failures are NotSent; any failure after dispatch remains Uncertain. A separate source review checked delivery classification, scope binding, limits and secret-free errors. Initial core gate passed 847 tests (two ignored); read-only desktop wiring passed 696 tests (three ignored). Later batch/retention/UI changes require the final gates below.
- Fresh private read-only verification proved two exact exported/canonical checkbox ID matches, both independent fresh preparations, and byte-for-byte title metadata equality with captured Slack requests. Evidence: `tmp/canvas-title-private-prepare.log`.
- The first actual native API toggle was accepted and independently read back. Target content/parents stayed unchanged and all other canonical sections were byte-identical. The second attempt, intended to restore the checkbox, returned Uncertain; it was not replayed. Independent canonical and export reads still showed the changed value. Evidence: `tmp/canvas-native-private-roundtrip.log`, `tmp/canvas-native-after-uncertain-read.log`.
- Restored the original unchecked/checked pair through the stock canvas's actual checkbox input, receiving HTTP 200; a new native canonical+HTML read independently verified the restored pair. Evidence: `tmp/canvas-stock-restored-readback.log`. A prior click on the editable text's role=checkbox div only focused the section and made no write.
- An isolated captured-Automerge reproduction then proved that two newly initialized documents using the same actor and different timestamps cannot merge (`duplicate seq 1`). This is a concrete local compatibility issue, not proof of the unrecorded second response's exact cause. Stock retains its accepted CRDT state across ordinary saves. Retained-session integration and a fresh complete native round trip were the next steps at this checkpoint; the first attempt itself did not prove native restore. The corrected API and UI round trips are recorded below.

- Retained-session correction now records genuine local seeds on successful preparation and adopts canonical CRDT state only from an own-session, own-request sealed ACK. Missing Section38 on a fresh load no longer implies a new history. Exact association preserves all fields except observed response-only18/103/104 and omitted38; server expiry is retained privately and checked again immediately before dispatch. A new fresh explicit intent can transactionally rotate the whole client/actor on unusable absent-38 state or bounded-cache exhaustion; old ACKs cannot overwrite the new session. Present expired server metadata still rejects. Seven focused retention regressions were added; scoped formatting and diff checks passed. Full core/desktop validation and corrected private native toggle-and-restore plus signed UI proof were pending at this checkpoint; later results are recorded below. Protocol/API details are updated in `docs/slack-canvas-edit-protocol.md`.

- Corrected private **native core API** toggle-and-restore now passed: checked=true at canonical sequence16000, then checked=false at17000, with valid supported ACK CRDT metadata and successful `remember_accepted` after each. Independent canonical reads confirmed both changes; target content/parents were unchanged and every other section remained byte-identical. Independent HTML export confirmed the original unchecked/checked pair. Evidence: `tmp/canvas-retained-private-roundtrip.log`. Full independent core suite passed856 tests before the final-send expiry patch; affected tests afterward passed30 with one ignored. Signed desktop clicks and captures were still pending at this API checkpoint; their later proof is recorded separately below. This API result alone is not a UI verification claim.

- Independent protocol review found and closed a final-send expiry gap: selected retained server expiry now survives as private preparation provenance and is rechecked immediately before dispatch, even when the new load omits Section38. The final independently locked core suite passed857 tests with two ignored.
- Desktop integration initially exposed three compile failures, all fixed; the next full suite passed700 tests with three ignored. Subsequent independent review closed the file-event invalidation and concurrent Refresh/loading races: validated matching file events retire canonical reads, stale exports release only their own loading flag, dispatch stages block Refresh/Retry while recovery stages remain usable, and rename drafts exclude checkbox actions. Three focused regressions accompany these fixes. The subsequent full desktop gate passed 704 tests with three ignored, and strict code-signature verification passed. Those checks precede the later post-ACK read recovery correction; static gates alone make no visual claim.

### Native canvas checkbox UI verification and read recovery

- Signed fixtures passed at 1280×800 and true narrow 668×400, exercising duplicate-action blocking, toggle and restore. Normal/narrow screenshots were inspected, along with saving, uncertain, read-only and rename states. These fixture states are not evidence of live lost-response recovery.
- Initial live mapping rejected the stock export's repeated ID on `li[id] > span[id]`. A reviewed, bounded exception now accepts that exact wrapper shape; ambiguous duplicates still fail closed. The fixtures include the real shape.
- Actual native desktop clicks accepted the first toggle; stock Slack showed checked/checked. Its own realtime event invalidated the automatic read, requiring manual Refresh. Native restore then succeeded with automatic readback, and stock Slack showed the original unchecked/checked pair. The restored screenshot `tmp/agent-ui/canvas-live-restored.png` was inspected.
- An independent read-only probe then passed private-owner/unshared guards, six canonical sections, two exact exported/canonical checkbox matches, two fresh preparations and title-byte equality; canonical/export state was the original unchecked/checked pair. It made zero writes. Evidence: `tmp/canvas-native-ui-restored-read.log`.
- At this checkpoint, gates were 857 independently locked core tests (two ignored), 704 desktop tests (three ignored), strict signed-app verification and the native fixture/live UI evidence above. Bounded post-ACK read recovery was still pending; its later final validation is recorded below. Uncertain operations remain separately locked and are never replayed or cleared solely by boolean equality.

- Final readback correction passed independent review and the full locked desktop suite: **706 passed, three ignored** (`tmp/canvas-readback-final-tests.log`). Final signed build and strict verification passed. The normal1280×800 and narrow668×400 native checks passed again, and final captures were inspected.
- A fresh signed native UI toggle-and-restore completed automatic confirmation in both directions without manual Refresh; stock Slack independently reflected checked/checked then unchecked/checked. A final zero-write canonical/export probe revalidated private ownership and the restored original pair (`tmp/canvas-final-restored-readback.log`); `tmp/agent-ui/canvas-final-native-restored.png` was inspected. This proves the live workflow, while the specific event-retirement branch is regression-tested rather than claimed to have occurred on that run.
- Removed temporary authenticated probe source and ACK diagnostics. Returned native client to Home/#echotestchamber with destructive agent commands disabled; stock Slack returned to its original channel route. No messages were sent in this slice. Full rich canvas authoring/collaboration remains outside this completed checkbox path; the broader parity goal stays active.


### Activity reply destinations and presence projection

- Activity now preserves the exact reply timestamp for message-specific mention,
  reaction and reply entries. Bundled thread notifications retain latest-thread
  behavior. Three parsed-shape regressions passed through Activity selection,
  thread state and the inclusive history request; a real native Activity click
  and cross-client read-cursor comparison remain unverified.
- A five-second live process sample (`tmp/native-idle-profile.txt`) located
  repeated sidebar projection under the realtime worker. Presence frames now
  update the rail and conversation presence fields without rebuilding messages,
  geometry, avatars or sidebar ordering. Workspace and socket generation checks
  remain in the existing reducer. The change retains rail snooze expiry refresh.
- Initial focused tests passed for equality with full projection, untouched
  timeline/sections, stale sockets and inactive workspaces. A paired disconnected
  2,009-conversation benchmark recorded median 1.1985 ms for full refresh versus
  0.1184 ms for presence projection (p95 1.2482 / 0.1205 ms). This establishes a
  lower projection cost, not an overall idle CPU or native latency improvement.
  Evidence: `tmp/presence-tests.log`, `tmp/presence-benchmark.log`. Final integrated
  checks follow after the parallel date-control and List API changes settle.


### Parallel pass: Activity, presence, datepicker and List deletion API

- Signed ordinary-message datepicker checks passed at exact **1280x800** and
  **668x400**. Both PNGs were inspected: controls remain readable and contained;
  narrow controls wrap vertically and keyboard focus remains on the native date
  segment. The runner prepared four disconnected requests per size and asserted
  untouched/absent date state, complete sibling state, invalid/empty event rejection,
  confirmation Cancel without commit, duplicate/pending rejection, navigation
  remount, and stale-message confirmation retirement. No real Slack action was
  sent. Programmatic invalid-date sanitation is not proof of trusted OS typing.
- The initial native checker incorrectly expected an explicit null for an
  untouched actions date. Its assertion was corrected to require omission, matching
  captured Slack initialization and existing unit evidence; production was unchanged.
- Core List deletion now has exact single-attempt delete/info requests, fresh
  permission and target/topology guards, explicit NotSent/Uncertain outcomes,
  and target-bound record_deleted readback. This is an **API layer only**: a
  native confirmation flow, retained operation state and private live proof are
  still required before exposing Delete item. See `slack-lists-protocol.md`.
- Final independently locked core suite: **875 passed, 2 ignored**. Full desktop
  suite: **714 passed, 4 ignored**, with doctests passing. Both formatting checks,
  diff check, signed build and deep/strict signature verification passed. A later
  checker-only assertion change was rebuilt and exercised by both native runs.
- Final paired presence benchmark: 2,009 conversations, median **1.1905 ms full
  refresh / 0.1108 ms presence-only**, p95 **1.2278 / 0.1158 ms**. This measures
  projection CPU cost only. Overall idle CPU and native interaction latency still
  need controlled before/after workload measurements.
- Evidence: `tmp/list-date-core-tests.log`,
  `tmp/activity-presence-date-tests-final.log`,
  `tmp/message-date-checker-build.log`, `tmp/message-date-native-checks.log`,
  `tmp/presence-benchmark-final.log`, and
  `tmp/agent-ui/message-datepicker{,-narrow}.png`.
- Remaining next steps: private live app response/receipt proof, trusted native
  date entry, other app controls, List delete confirmation/reconciliation UI,
  native Activity exact-reply click/read-cursor proof, and broader parity audits.
  The overall parity/design/performance goal remains active.


### Ordinary-message timepicker: signed offline verification

- Ordinary message timepickers now preserve exact `HH:mm` wall time and optional
  timezone metadata without UTC conversion. Captured Slack initialization is
  retained: absent values remain omitted, a timezone-only initial state contains
  no invented `selected_time`, and committed actions carry explicit selected
  time plus the complete sibling state. Truthy initial-time and timezone fields
  are included in the action exactly as captured.
- The required native time input has no invented Clear action. Empty or invalid
  changes do not dispatch. Confirmation Cancel leaves mounted display and shared
  wire state unchanged; Go commits only after request preparation. Pending and
  duplicate actions, stale message/account owners, navigation, and unmounted
  controls retain the existing action guards. Component-local display and shared
  state remain distinct across prop changes and remounts.
- Normal **1280x800** and narrow **668x400** signed native checks passed, with
  **four prepared requests per fixture** and `noTransport` asserted. They cover
  untouched and timezone-only state, sibling-state preservation, invalid/empty
  rejection, confirmation cancellation, duplicate/pending rejection, navigation
  remount, and stale confirmation retirement. Both final PNGs were inspected by
  the root agent. No real Slack action was dispatched by these fixtures.
- Integrated gates passed: independently locked core **882 passed, 2 ignored**;
  desktop **728 passed, 4 ignored**; signed build and deep/strict signature
  verification. Evidence: `tmp/time-delete-core-tests.log`,
  `tmp/list-delete-time-desktop-tests-final.log`,
  `tmp/list-delete-time-signed-build.log`, `tmp/message-time-native-checks.log`,
  and `tmp/agent-ui/message-timepicker{,-narrow}.png`.
- Live app responses/receipts and trusted OS time-segment entry remain unverified.
  Programmatic input events and invalid-value sanitation establish the tested
  native event boundary, not physical keyboard-entry parity. This checkpoint
  records timepicker completion only: the separate List deletion narrow fixture
  failed on premature detail closure and remains under investigation. Broader
  app controls and the overall parity/design/performance goal remain open.


### List deletion: native and private-live checkpoint

- Normal and narrow native deletion fixtures passed on rerun and again with the
  diagnostic build. Evidence: `tmp/slack-list-delete-native-checks-rerun.log`
  and `tmp/slack-list-delete-native-diagnostics.log`. The first narrow run's
  unexpected detail closure remains unexplained; its evidence is preserved at
  `tmp/slack-list-delete-first-narrow-failure.{log,png}`. Passing later runs do
  not explain that failure, and no speculative product fix is claimed.
- In the private verification List, native Add item created a new disposable
  record. Native Delete → Cancel was followed by exact-target **Present**
  readback; native Delete → Confirm was followed by exact-target **Deleted**
  readback. All five baseline fields remained intact and no extra rows remained.
  Evidence: `tmp/list-delete-native-predelete.log`,
  `tmp/list-delete-native-cancel-readback.log`, and
  `tmp/list-delete-native-final-readback.log`. The root agent inspected
  `tmp/agent-ui/list-delete-private-live.png`.
- The earlier unrelated Name bold-style loss was restored using the exact
  baseline blocks, and readback confirmed all five baseline fields preserved.
  Its cause remains unproven; the captured stock editor normalization path is a
  possible explanation, not attribution to the unrelated deletion.
- Native deletion remains confirmation-only; Undo is unimplemented. Stock's
  conditional immediate-delete/Undo behavior and same-ID undelete contract are
  documented in `slack-lists-protocol.md`.
- Follow-up fixes for a stale creation “Open item” notice and conflicting Refresh
  guidance are in progress. Final integrated gates for those changes are pending.
  This checkpoint establishes the tested deletion/cancellation round trip, not
  completion of all List actions or the broader parity goal.


### 2026-10-13 — Final deletion integration checks

- Exact deletion readback now retires a matching accepted creation notice/receipt,
  removing the stale “Item added / Open item” affordance for that deleted item.
  Other record/list/account identities and non-deleted readbacks preserve their
  notices. Two focused regressions cover these boundaries.
- Pending deletion owns recovery guidance; generic refresh-to-edit/add copy is
  suppressed while its receipt locks mutations. Read-only refresh remains
  available for uncertain outcomes. Native Undo remains unimplemented.
- Final desktop suite: **730 passed, 4 ignored**, with no failures
  (`tmp/list-delete-time-final-tests.log`). Root and independently locked core
  formatting plus `git diff --check` pass. Core remains **882 passed, 2 ignored**;
  no core source changed after that independent run.
- Temporary private read/restore probe source was removed. The stock Slack
  reference returned to its original channel without changing its saved draft.
  Earlier transient narrow-fixture closure remains an unexplained observation,
  not a fixed or disproven bug. Successful subsequent runs and structural failure
  diagnostics are recorded above.
- Final signed build and deep/strict signature verification pass
  (`tmp/list-delete-time-final-build.log`). Both deletion fixture sizes pass on
  that build (`tmp/slack-list-delete-native-final.log`); final normal/narrow
  uncertain-state PNGs were inspected and show the corrected recovery copy.
  This is the third successful normal/narrow run after the preserved first
  narrow failure; that earlier failure's cause remains unproven.
- Final signed live client relaunched in the private channel. Direct background
  launch initially showed a blank window and agent timeout; activating its exact
  app bundle with `open -a` restored the signed-in Dioxus surface. This reproduces
  the earlier background responsiveness observation; it is not a resolved startup
  performance claim. Structural sample: `tmp/final-native-blank-profile.txt`.

### 2026-10-13 — Metadata cache snapshot cost

- A fresh five-second native sample identified user-group hydration spending
  main-thread time cloning the entire workspace before enqueueing a metadata
  save. The path included cached message bodies and their rich JSON. The sample
  was predominantly idle overall; it does not establish a continuously busy
  application or explain the intermittent blank-window launch.
  Evidence: `tmp/native-background-before-undo.txt`.
- User-group hydration and its member-panel refresh now capture only persisted
  workspace metadata. They do not clone message bags, realtime connections, or
  session credentials. Full transcript persistence retains its previous path.
  The single persistence worker still coalesces by account and team: newer
  metadata retains an older queued full transcript snapshot; a newer full
  snapshot supersedes it. Metadata saved after an already-drained full save
  leaves the transcript and channel-state tables untouched.
- The paired CPU-only benchmark used **2,001 channels and 8,001 synthetic rich
  messages**, with five warmup rounds and **40 measured samples per path** in
  alternating order. Full workspace snapshot capture measured **11.1317 ms
  median / 13.0110 ms p95**; metadata capture measured **0.3558 ms median /
  0.8835 ms p95**. This measures capture cost only, excluding destruction, queue
  work, serialization, SQLite I/O, and rendering. It is not a whole-app speedup,
  frame-latency, or startup improvement claim.
  Evidence: `tmp/cache-metadata-benchmark.log`; repeat with the independently
  locked core test `metadata_snapshot_clone_cost --ignored --nocapture`.
- Core regressions verify equivalent persisted metadata, unchanged transcript
  and read-state rows, retained group-removal behavior, same-team snapshot
  merging, cross-team isolation, and later full-snapshot precedence. The
  independently locked core suite passed **895 tests, 3 ignored**, and the
  separately invoked snapshot benchmark passed. Evidence:
  `tmp/undo-cache-core-tests.log` and `tmp/cache-metadata-benchmark.log`.
  Desktop queue coverage was added; its integrated gate is recorded separately
  when completed.
- Limits: metadata capture still clones channels, users, and groups. Full saves
  still clone transcripts, and hydration/realtime can still trigger full shell
  projections. Failed cache writes retain the existing no-retry behavior; a
  later metadata-only save does not incidentally retry a failed drained full
  transcript snapshot. No activation workaround was added, and the intermittent
  blank-window observation remains unresolved.

### 2026-10-13 — List Undo/Redo integration checkpoint

- Completed record-specific same-ID deletion Undo/Redo with fresh access and
  exact-record preflight, stale account/visit/cancellation guards, single-attempt
  dispatch, and exact readback. Verified childless complete Lists delete
  immediately; known children or incomplete pages retain confirmation.
- Private native Delete → Undo → Redo passed for one newly created disposable
  item. Undo restored its original ID and exact fields; Redo left only the five
  unchanged baseline records. Readback logs: `tmp/list-undo-{created,delete,redo}-readback.log`
  and `tmp/list-undo-first-attempt-readback.log`. The restored native window was
  visually inspected through computer-use capture. Subtree restoration is still
  fixture-only; this is not a general List Undo history.
- Stronger normal/narrow signed native fixtures passed at 1280×800 and 668×400:
  duplicate transition clicks, held-request disabled controls, exact restored
  raw-field/ID baseline, expanded child rendering, immediate childless deletion,
  confirmation cancellation, and uncertain result recovery. Restored screenshots
  for both sizes and the narrow uncertain state were inspected.
  Evidence: `tmp/undo-cache-native-checks-final.log` and
  `tmp/agent-ui/slack-list{,-narrow}-delete-restored.png`.
- Desktop **737 passed, 4 ignored** (`tmp/undo-cache-desktop-tests-final.log`);
  independently locked core **895 passed, 3 ignored**; formatting and diff checks
  passed. Final signed build and deep/strict signature verification passed
  (`tmp/undo-cache-signed-build-final.log`). Later harness-only coverage changes
  were compiled and exercised by that signed build.
- Before this checkpoint, five repeated normal/narrow deletion runs passed
  (`tmp/delete-repeat/run-{1..5}.log`). The earlier intermittent detail closure
  remains unexplained. This checkpoint's first fixture failed before actions,
  while native windows could not be captured; a later power-state read reported
  a recent lid-open event. After the display returned, the stronger fixtures
  passed. This does not establish a fix for background responsiveness.
- Two earlier individual startup samples measured direct launch first paint
  444.7 ms/cached conversation 844.6 ms, and standard bundle launch 914.1/1286.6 ms
  (`tmp/launch-{direct,bundle}-before-undo.log`). These are individual observations,
  not a performance trend or proof that blank-window behavior is resolved.
- Next bounded parity candidate: ordinary User-column editing through the
  existing guarded cell-update path, excluding todo-assignee sharing behavior.
  Entity-picker candidate filtering still needs source confirmation. Next
  performance candidate: skip empty user-group hydration commits; frequency of
  empty live responses has not been measured. Neither candidate is implemented.


### 2026-10-13 — Ordinary People fields and empty hydration checkpoint

- Ordinary List `user` fields now support single/multiple people, removal and
  clearing through the existing guarded cell-update path. The picker retains
  uncached selected IDs, limits cached/remote choices to 30, includes self and
  guests, excludes ordinary bots/deactivated/profile-only candidates, and rejects
  stale query/account/visit results. Selection remains local until Save.
  Legacy assignee and todo-assignee sharing behavior remains unsupported.
- Slack bundle references and exact wire/cardinality rules are documented in
  `docs/slack-lists-protocol.md`. Parallel reviews caught fuzzy remote matches
  being filtered a second time and hidden-cell queued queries changing loading
  state; both now have focused regressions.
- Private native verification created a disposable item, selected self, cancelled
  a removal, saved an explicit clear, and deleted the item. Canonical reads and
  stock Slack agreed on selection and clearing. The final read retained all five
  original records with identical fields and no extra live records. A named
  verification People column remains on the task-owned private List. Evidence:
  `tmp/list-users-{selected,cancelled,cleared,cleanup}.log`. Stock Slack's original
  route and existing draft were restored. The temporary read-only probe source
  was removed; no sharing or assignment endpoint was introduced.
- Signed native fixtures passed at normal and narrow sizes, covering single
  replacement/clear, multiple selection, uncached removal, candidate exclusions,
  Cancel with zero writes, duplicate Save locks, keyboard entry/focus recovery,
  and canonical request identity. Both People editor PNGs were inspected, as was
  the live native picker. Evidence: `tmp/list-users-native-checks-final.log` and
  `tmp/agent-ui/slack-list{,-narrow}-user-editor.png`.
- Empty successful user-group hydration batches now skip the Signal write and
  full shell projection. An all-empty run skips metadata persistence; nonempty
  batches still project immediately and preserve loaded group membership.
  Partial success followed by failure retains prior updates and existing retry
  behavior. Tests cover empty, mixed, partial-error and missing-workspace cases.
- The paired synthetic benchmark used 2,009 channels and 40 measured samples:
  full projection median **3.975417 ms**, p95 **7.516625 ms**; empty commit median
  **0.000291 ms**, p95 **0.000708 ms**. Evidence:
  `tmp/usergroup-empty-benchmark.log`. This isolates projection work; live empty
  response frequency, end-to-end startup and frame latency remain unmeasured.
- Final gates: desktop **749 passed, 5 ignored**; independently locked core
  **900 passed, 3 ignored**; separately invoked hydration benchmark passed;
  both formatting checks, diff check, signed build and deep/strict signature
  verification passed. Logs: `tmp/list-users-desktop-tests-final.log`,
  `tmp/list-users-core-tests.log`, `tmp/list-users-signed-build-final.log`.
  Initial validation exposed two fixture issues: independent opaque media IDs
  in equality checks, and an earlier numeric edit filtering out the User target.
  Both were fixed while preserving full equality and exact fixture restoration.
- Limits: live verification selected self only. Single-user and multiple-person
  writes are fixture-proven; live remote directory search and special AI-agent
  eligibility remain unverified. Started directory reads can outlive cancellation,
  although stale results cannot enter a newer draft. Full Slack parity and the
  earlier intermittent blank-window observation remain open.
- Next bounded candidate: ordinary channel-reference List fields through the
  same editor path, after verifying Slack's picker and payload contract. Continue
  measuring visible runtime work before extending performance claims.

### 2026-10-13 — Ordinary Channel fields checkpoint

- Ordinary List Channel fields now use the guarded entity-editor path. Private
  native verification confirmed Cancel preserves the existing selection,
  explicit clear saves `channel: []`, and fuzzy search `echotestchambr` finds
  `echotestchamber`; Save retained the exact intended private channel ID.
  Independent canonical reads verified each result. The disposable record was
  deleted afterward: all five baseline records retained their original fields,
  with no extra live records. Evidence:
  `tmp/list-channels-native-{cancel,clear,selected,cleanup}.log`.
- The original signed binary's normal and narrow fixture runs passed
  (`tmp/list-channels-native-checks-retry.log`). Both Channel editor PNGs and the
  narrow User editor PNG were inspected. Gates for that binary: independently
  locked core **904 passed, 3 ignored**; desktop **759 passed, 5 ignored**; signed
  build passed. This checkpoint does not imply support for every channel-picker
  variant or full Lists parity.
- The shared User/Channel search input now uses `initial_value` as its mount seed
  so delayed cached-result renders do not overwrite browser-owned text or caret.
  New native assertions exercise DOM text and caret retention for both entity
  pickers; query, account, visit, and mutation guards are unchanged. Follow-up
  desktop suite: **759 passed, 5 ignored**
  (`tmp/list-channels-input-desktop-tests.log`). The follow-up signed build and
  deep/strict signature verification passed
  (`tmp/list-channels-input-signed-build.log`). Both normal/narrow native runs
  then passed, with User and Channel `inputSurvivesResultRender` receipts true
  (`tmp/list-channels-input-native-checks.log`). Fresh normal/narrow Channel
  editor PNGs and the narrow User editor PNG were inspected and fit clearly.
  Both Cargo formatting checks and the diff check passed; the temporary read-only
  probe source was removed. These synthetic DOM checks do not establish trusted
  OS input or IME completeness. The earlier overlay/Budget fixture failures remain
  preserved without an asserted cause; the private round-trip evidence above
  belongs to the original signed binary.

### 2026-10-13 — Bounded runtime observability checkpoint

- Added opt-in `SUPER_PLATINUM_RUNTIME_PROBE=1` instrumentation in
  `src/desktop/performance/runtime_probe.rs` and its adjacent JS, with narrow
  `performance.rs`, `media.rs`, and `runtime.rs` wiring. A native watchdog and
  structural JS reports observe the first **20 seconds** independently of the
  Dioxus app-task queue; payloads are whitelisted and capped, with no message
  content, URLs, credentials, or raw IPC. Disabled mode adds no probe thread,
  script, custom protocol, or native callback. See
  `docs/runtime-responsiveness.md` for commands, source reasoning, and limits.
- Two signed offline `slack-list` launches were healthy and remained unfocused:
  direct executable first paint **1632.4 ms**, bundle launch **589.0 ms**. Fresh
  agent checks from approximately 2–20.5 seconds passed at **0.170–1.036 ms** and
  **0.495–1.118 ms**, respectively; earlier socket checks found no socket. Both
  reported `headless=true`, an open edit socket, three root children, completed
  frame callbacks, and advancing runtime heartbeats. These are individual
  observations, not a comparative speedup or focused/background comparison.
- Direct-launch PNG was inspected. Bundle agent capture failed with
  `screencapture` exit 1; a separate CUA screenshot was inspected and showed the
  fully painted fixture with inactive window controls. The artifact name
  `bundle-foreground` is inaccurate as an observed focus label. Evidence:
  `tmp/runtime-probe-{direct-background,bundle-foreground}.{log,json}` and
  `tmp/agent-ui/runtime-probe-direct-background.png`.
- The probe can change scheduling through its own wakeups, logging, and protocol
  requests. Neither healthy launch reproduced the earlier live blank-window
  failure; no activation workaround or hang fix is claimed. A controlled live
  reproduction and an uninstrumented comparison remain open.

### 2026-10-13 — DM pane channel lookup CPU cost

- `view/secondary.rs::dm_list_panel` previously searched all projected channels
  separately for each displayed DM's avatar and presence. The first DM page has
  100 entries and later 100-entry pages accumulate; query changes and shared
  shell repaints repeated those scans. The pane now builds one borrowed index
  per render and performs one lookup per displayed entry. Existing avatar
  eligibility and missing-presence fallbacks are unchanged. The index keeps the
  first duplicate to preserve `iter().find` semantics, assumes no sort order,
  clones no channel rows, and requires no retained-cache invalidation.
- The paired standalone benchmark includes the actual production `ChannelVm`
  layout (184 bytes) and `view/secondary/dm_channels.rs` helper. It uses synthetic
  distributed IDs with 10% misses, five warmup rounds, 40 alternating measured
  samples, and five calls per sample. Index construction and destruction are
  included each time. Final median / p95 milliseconds:

  | Projected channels / loaded DMs | Previous two scans | Borrowed index |
  | --- | --- | --- |
  | 2,000 / 100 | 0.372592 / 0.428108 | 0.025425 / 0.032292 |
  | 2,000 / 500 | 1.959042 / 2.517817 | 0.033617 / 0.036550 |
  | 10,000 / 100 | 1.637908 / 2.726358 | 0.134742 / 0.197300 |
  | 10,000 / 500 | 7.913600 / 11.021083 | 0.142917 / 0.275875 |
  | 10,000 / 2,000 | 31.522158 / 33.131083 | 0.161475 / 0.172375 |

- Reproduce with `bash scripts/bench-dm-channel-lookup.sh` after a normal desktop
  build. It invokes standalone `rustc` at the app's `opt-level=1`, using existing
  core artifacts; it does not invoke Cargo, launch the app, or access a session.
  Evidence: `tmp/dm-channel-lookup-benchmark-final.log`; two earlier paired runs
  are retained in `tmp/dm-channel-lookup-benchmark{,-repeat}.log`. The pure
  regression checks unsorted, duplicate, and missing IDs against the prior
  first-match behavior, including pointer identity. Scoped formatting, shell
  syntax, and diff checks passed. The integrated desktop suite passed 760 tests
  (5 ignored), including the index regression. The signed native DM fixture
  passed and `tmp/agent-ui/dms.png` was inspected: avatar, presence, unread badge,
  and preview remained visible. Evidence: `tmp/dm-lookup-native-checks.log`.
- This measures lookup CPU work only. It excludes label/preview projection,
  Dioxus reconciliation, DOM size, layout/paint, network, and native interaction
  latency. Synthetic sizes are not measurements of the user's live DM count.
  Results assume rows eligible for both old lookups; rows without an available
  avatar previously performed only the presence scan. Building the index can
  cost more for a very selectively filtered list; the empty feed skips index
  construction. Rich-message preview
  generation and unbounded loaded-row rendering remain separate concerns; no
  whole-app or startup speedup is claimed and no profile settings changed.

### 2026-10-13 — Activity reply navigation and compact List chrome

- The Activity control-plane command previously discarded the exact reply
  destination even though the visible row click preserved it. `ActivitySelect`
  now uses `thread_destination` and `open_thread_at`, matching the UI's older
  reply versus bundled-thread distinction.
- Added disconnected `activity-navigation` and `activity-navigation-narrow`
  fixtures with a 32-reply transcript. The native checker clicks a bundled
  thread, a channel post, and an older reply. It verifies latest behavior for
  the bundle, removal of the prior thread for a channel post, preservation of
  the Activity surface, and exact older-reply visibility away from the bottom.
  The Python harness also exercises both agent destinations. Both sizes passed;
  PNGs were inspected and showed the intended reply. Evidence:
  `tmp/activity-navigation-native-checks.log` and
  `tmp/agent-ui/activity-navigation{,-narrow}.png`. All data was synthetic and
  transport remained absent. This does not prove live Slack read-state parity.
- List titles/view controls now share a wrapping header row. Full shortcut
  instructions remain available through accessible descriptions and a collapsed
  disclosure at every size, instead of occupying a persistent paragraph above
  the content. Table, Board, and record-detail disclosure gates passed at both
  sizes, checking accessible descriptions, expansion bounds, content/scroll
  restoration, and focus. Normal/narrow table, Board, and record PNGs were
  inspected. The first assertion incorrectly treated a nonzero paragraph
  rectangle inside closed native details as visible; WebKit reported it as
  invisible. The corrected check uses `checkVisibility()` and retains all
  geometry/restoration assertions. Evidence: `tmp/list-density-final-native.log`
  and `tmp/agent-ui/slack-list{,-narrow}-{rich,board-multi,detail-table}.png`.
  These checks exercise native summary click behavior, not trusted OS keyboard
  activation or assistive-technology output.
- Integrated desktop tests passed **760, with 5 ignored** and the signed build
  passed deep/strict verification (`tmp/activity-density-desktop-tests.log`,
  `tmp/activity-density-signed-build.log`). Core behavior was unchanged in this
  checkpoint; the prior independently locked **904/3 ignored** suite remains
  the latest core result. The final signed rebuild and strict signature check
  passed (`tmp/list-density-final-build.log`); final root/core formatting and
  `git diff --check` passed. List/DM fixture gates and screenshot inspection are
  complete. Broader live read-state parity and intermittent startup blankness
  remain open; this checkpoint does not close the overall parity goal.

- Independent Activity review confirmed the exact-target request and post-await
  guards. It also identified a pre-existing queued-start gap in
  `bootstrap/thread.rs::open_thread_at`: its first channel/thread mutations
  precede ownership capture, so navigation between queuing and first polling
  needs a synchronous admission guard. This remains a follow-up; no unvalidated
  fix was mixed into this checkpoint. The agent-path automated assertion proves
  non-latest state; exact visible reply was additionally checked in its PNG.
- Relaunched the latest signed bundle, confirmed signed-in Home in the private
  test channel and destructive commands disabled, and inspected its live CUA
  screenshot. The channel, composer, avatars, and gray system messages painted.
  The older owned instance had stopped answering its agent ping; it was replaced
  only after PID/socket identity verification. The fresh instance answers. Native
  agent screenshot still returns `screencapture` exit 1 for the live bundle; CUA
  supplied the visual evidence. This does not establish that intermittent
  unresponsiveness is fixed. Private state: `tmp/activity-density-live-final.json`.

- Parallel Link research produced a reproducible offline captured-code oracle:
  48 normalization cases and 10 stored-value converter cases passed, reproduced
  by root (`scripts/slack-list-link-oracle.mjs`,
  `tmp/slack-list-link-oracle.json`). Source hashes and synthetic inputs are
  recorded. This establishes edge cases for the next faithful port; Link editing
  itself and server enrichment/readback remain unimplemented/unverified.

### 2026-10-13 — Activity queued admission and ordinary List Link editing

- This checkpoint supersedes the prior Activity queued-start follow-up and Link
  implementation-pending notes above. Activity now captures a navigation ticket
  synchronously at selection, before its task is queued. First polling checks
  account, viewer, credentials, transport, surface/overlay, channel/thread
  generations, and the exact selected notification before any initial thread
  mutation. Admission consumes the generation; identical queued tickets cannot
  run twice. Read marking and destination loading start together after admission.
  Five pure regressions cover replacement/removal, navigation and away/back,
  account/credential/transport changes, and one-shot exact/bundle/channel paths.
  `tmp/activity-admission-native.log` passed normal and narrow disconnected
  fixtures. Both DOM clicks and subsequent agent commands now have measured
  rendered destinations: bundle bottom gap zero and the exact older reply
  intersecting the thread viewport at 1280×800 and 668×400. This remains synthetic
  navigation proof, not live Slack read-state parity.
- Ordinary Link cells now support an uncontrolled Link/Name draft with explicit
  Save and Cancel, Remove Link, and separate display-as-URL/name actions through
  the existing guarded cell-update lifecycle. Save uses the captured detector
  semantics, including first-match URL/email/phone extraction and original-input
  fallback; it does not substitute generic URL parsing or unconditional trimming.
  Display changes preserve the canonical URL/name and are disabled while the
  draft is dirty. Editable wire values contain only `original_url`, optional
  `display_name`, and `display_as_url`; full record/schema snapshots remain pinned
  for preflight. Receipt comparison permits the known attachment enrichment while
  keeping editable members exact. Storage and safe navigation remain separate:
  unsafe schemes can be stored as inert text and do not become clickable links.
- Normalization runs in `spawn_blocking`, with one shared permit held inside the
  blocking closure even if its editor task is cancelled. Old results cannot save
  into a reopened draft or cross account/revision changes. Save, Display, and
  Remove also check the live pending state, covering clicks queued before the
  disabled render arrives. The normal/narrow native List gates passed
  (`tmp/link-native-final.log`): typing/Cancel without writes; held-normalizer
  Save/Display/Remove duplicate suppression followed by Cancel/reopen; normalized
  set, both display modes, rename, Remove Link, inert unsafe storage, focus,
  bounded controls, and exact fixture cleanup. The inspected
  `tmp/agent-ui/slack-list-link-editor.png` and
  `tmp/agent-ui/slack-list-narrow-link-editor.png` show both labeled inputs,
  display/removal controls, Cancel and Save fully inside the native editor.
  Neither image showed clipped actions or horizontal overflow.
- The independently locked core suite passed **914 tests, 4 ignored**
  (`tmp/link-core-final.log`); the integrated desktop suite passed **768 tests,
  5 ignored** (`tmp/link-activity-desktop-final.log`). The checked-in captured-code
  normalization oracle contains 1,548 synthetic cases and is exercised by the
  core suite. The native-profile timing
  probe (`tmp/link-detector-desktop-final.log`) measured 105.567 ms cold setup,
  5.167 ms for 1,000 warm ordinary calls, and approximately 679–762 ms for the
  measured 10,000-character pathological dotted inputs. This is local synthetic
  CPU evidence, not an arbitrary-input latency bound or a whole-app speed claim.
  Large pathological input can still take time; work stays off the UI thread and
  concurrent normalization is bounded. No arbitrary maximum input length was
  introduced. Split-surrogate results that cannot be represented as Rust strings
  remain rejected rather than silently rewritten.
- **Live Link write/readback and cleanup remain pending at this checkpoint.** A
  disposable private row and existing Link column were prepared before the pause;
  their real identifiers are retained only in private task evidence. This entry
  does not claim that the live row was deleted or that server enrichment has
  been proven by a native roundtrip. Full attachment/unfurl presentation and
  exhaustive browser-detector equivalence remain separate gaps. The broader
  parity goal, live read-state coverage, and intermittent native responsiveness
  are not closed by these gates.

### 2026-10-13 — Link live findings and retained cell-write ownership

- The private live Link check has now proved Cancel without a change, explicit
  Save, display-mode changes, and rename through stock Slack plus independent
  canonical readback. The existing baseline rows remain unchanged. The service
  returned singleton camelCase Link values; editing/receipt parsing already
  accepted those, but display projection did not. The reader now accepts equal
  snake/camel aliases and rejects conflicting URL/name/display aliases instead
  of silently choosing one. The disconnected backend now emits the observed
  camelCase array with attachment metadata, keeping the outgoing request intact.
  **The rendering fix still needs the rebuilt native check/live display proof;
  the disposable live row has not yet been cleaned up.** No real identifiers are
  included here.
- A separate stock-source correction captures the Link modal's initial Name:
  a nonempty explicit name wins (including whitespace), otherwise the exact
  URL-keyed attachment title is used, with nullish fallback to attachment
  fallback text. An empty title remains empty. This is an immutable desktop
  draft seed, separate from canonical `LinkValue`: Cancel and display-mode
  toggles do not invent a stored name; explicit Save can store the seeded title.
  Helper/controller regressions cover precedence, malformed metadata,
  snake/camel storage, Cancel, toggle, stale attachment refresh, reopen, and
  explicit Save. The revised native cancellation check also uses a genuinely
  changed URL/name, so a core no-op rejection cannot substitute for proving that
  a cancelled normalizer cannot affect a reopened editor. These additions are
  implemented; their integrated desktop/native gates remain pending below.
- Independent review found an inherited cell-mutation lifecycle gap: changing
  workspace, account, credentials, or transport erased potentially sent cell
  receipts. The desktop now retains `Saving` and `Uncertain` operations by exact
  attempt serial plus stable account/team/user/List ownership. Only unsent
  `Preparing` work is retired. Returning to the owner restores its locks; another
  owner with the same List ID stays isolated. A sending operation remains locked
  until its original bounded request completes, then a fresh current-scope read
  reconciles the exact record/column identity. No request is replayed. Late
  responses settle only their matching receipt and cannot update a replacement
  visit's UI. Retained receipt scopes discard the session/client; pending copy
  derives from the current receipt phase instead of freezing a Sending notice.
- New cell admission is bounded to eight pending operations per stable owner and
  32 total, without evicting unresolved writes. Restore refuses an ambiguous
  same-List collision rather than overwriting either receipt. Six focused
  regressions were added for switch-away/back while still sending, credential
  rotation and stale reads, another owner with the same List ID, unsent retirement
  and uncertain reconciliation, late duplicate completion, and capacity/collision
  preservation. An independent read-only review found no remaining blocker.
  Scoped formatting and diff checks passed. **These six regressions and the
  integrated rebuild/native gates have not yet been run at this checkpoint.**
- The independently locked core suite now passes **918 tests, 4 ignored**
  (`tmp/link-core-live-followup.log`). A source-equivalent parser scanning change
  reduced the three previously measured pathological dotted cases from
  679.387/682.535/762.409 ms to 2.534/2.598/3.101 ms in the native-profile probe
  (`tmp/link-detector-desktop-offset.log`). These are the same selected synthetic
  cases, not a universal latency bound; the single-worker CPU budget and
  cancellation guards remain in place. The earlier 914-core/768-desktop/native
  results above remain historical gates for the prior source state. Root will
  append the final integrated results and private cleanup evidence separately.

### 2026-10-13 — Integrated Link verification and private cleanup complete

- Final locked gates passed: **918 core tests, 4 ignored** and **777 desktop
  tests, 5 ignored**, both formatting checks, and the signed macOS bundle build
  with strict signature verification. Evidence: `tmp/link-core-live-followup.log`,
  `tmp/link-live-integration-desktop.log`, and `tmp/link-live-final-build.log`.
  Both normal and narrow native List checks passed in
  `tmp/link-live-final-native.log`, including camelCase canonical responses and
  the strengthened changed-draft cancellation case. The operator read both
  generated Link editor PNGs and confirmed visible, bounded inputs/actions.
- The rebuilt client displayed the real canonical Link name correctly in item
  details; `tmp/agent-ui/link-live-after-alias-fix.png` was captured and inspected.
  Native Remove Link then produced an empty canonical Link array and an empty
  stock Slack cell. Native Delete item removed only the disposable row and
  offered Undo. Independent readback confirmed that the remaining five complete
  records exactly match the original baseline, with no extra records and no
  sharing changes. The task-owned Link verification column remains available.
  Stock Slack's original route and existing draft were restored; the temporary
  session-backed probe source was removed. Private snapshots stay untracked.
- After the final build, an isolated CPU probe measured 103.643 ms cold setup,
  5.037 ms for 1,000 warm calls, and 2.588/2.627/2.818 ms for the three selected
  pathological dotted cases (`tmp/link-detector-desktop-integrated-isolated.log`).
  A probe run concurrently with the native harness was slower (8.820–12.996 ms
  for those cases, 1197.895 ms cold); keep that load sensitivity visible rather
  than treating individual runs as a stable latency guarantee. Full-input
  semantics, immutable draft ownership, and the single-worker budget remain.
- Live ordinary Link editing is verified; full attachment/unfurl parity,
  exhaustive detector equivalence, live credential-rotation reconciliation, and
  the intermittent blank startup are still open. Receipt-retention proof here
  is from focused controller tests and review, not a forced real-account outage.

### 2026-10-13 — Parallel user-picker, List layout, and startup follow-up

- Three concurrent workers handled ordinary-message `users_select`, independent
  candidate-policy/fixture work, and List horizontal usability, then independently
  reviewed the picker lifecycle. This fills the runtime's four-agent limit with
  root; no extra agents or unrelated work were started to meet the user's cap.
- The user-picker policy is based on captured ordinary-message member search,
  separate from List User fields: self, ordinary bots, Slackbot/securitybot,
  guests, and external members are included. Deleted, profile-only, workflow,
  huddle-only external guests, and special search/expert/system identities are
  excluded. Agentforce discovery remains unavailable because its app-access
  metadata is not retained; the picker explains that limitation when encountered.
  Confirmation, container state, owner/query revision guards, rate-limit retention,
  and remote-only display labels are part of the implementation/review scope.
- List changes use compact checkbox and numeric widths, pin the visible canonical
  primary column even when reordered, paint opaque matching row states, and
  reserve measured header/primary clearance during keyboard navigation. Hidden
  primary columns pin nothing. The new normal/narrow checks exercise geometry,
  hit testing, and synthetic Home/End/Right navigation; they do not establish
  trusted OS keyboard event delivery. Percent-formatted numeric columns inherit
  numeric sizing; no undocumented percent type was invented.
- Two bounded live startup probes were healthy, including an actually unfocused
  background launch. A CUA screenshot of the background conversation was read
  after the agent screenshot path failed; it showed painted content and muted
  system messages. The earlier blank-window failure remains unresolved. Corrected
  stack interpretation and individual measurements are in
  `docs/runtime-responsiveness.md`; these observations are not a startup fix or
  comparative speedup.
- The opt-in probe now adds passive numeric script/resource/rejection counters,
  without inspecting private error payloads. Root's pure-JS privacy/lifetime
  regression script and independent review passed. No interpreter wrapping,
  ACK replacement, or activation workaround was added.
- First integrated core gate: **924 passed, 4 ignored**
  (`tmp/users-list-core-tests.log`). The initial desktop check caught missing
  `serde_json::Value` imports, now repaired. Final desktop/build/native gates are
  pending at this checkpoint; root will append their actual results below.
- Final frozen-source desktop suite passes **784 tests, 5 ignored**
  (`tmp/users-list-desktop-final-tests.log`); both formatting checks and
  `git diff --check` pass. An independent lifecycle review found no remaining
  blocker after the concurrent Retry-After fix. Captured upstream `iGIl` coalesces
  null saved user values before `HvtU`'s undefined-only fallback, confirming that
  remounting a committed null selection uses `initial_user`; no speculative
  display change was made. Live remote search/app receipt and trusted OS keyboard
  delivery remain unproven by the disconnected fixtures.
- Signed build and strict signature verification passed
  (`tmp/users-list-build.log`). Normal and narrow List native checks passed
  (`tmp/users-list-native-final.log`), including compact-column measurements,
  reordered/hidden primary, focused-cell hit testing, and existing editors/views.
  Root inspected both rich and horizontally scrolled reference captures; primary
  labels remain opaque and visible while the remaining columns scroll.
- The first normal/narrow message-user interaction checks also passed
  (`tmp/users-native-final.log`), but image inspection found a wrapped trigger
  chevron and default platform search styling. A scoped CSS correction plus a
  single-row trigger assertion is being integrated before the final screenshot
  gate. The narrow capture also showed the deliberate stale-action test toast;
  its dismissal belongs to fixture cleanup, not a product toast behavior change.
- Final picker follow-up is verified: shared compact trigger/search styles keep
  both triggers on one 32-pixel row; the search focus outline is inset so its
  scrolling container cannot clip the top edge. The fixture clears its deliberate
  stale-action toast only after proving no extra dispatch. The final signed build
  and strict signature check passed (`tmp/users-focus-final-build.log`), then both
  native sizes passed again (`tmp/users-focus-native-final.log`). Root read both
  final PNGs; controls, availability copy, Cancel, and scrolling result area are
  visible and bounded. Unit logic remains the previously verified 924-core /
  784-desktop state; subsequent edits were scoped CSS and fixture assertions.
- Final real-session background launch accepted all three numeric error counters:
  eight JS samples, zero script errors/rejections and two resource errors in the
  last sample, 99 native-observed app heartbeats by 20 seconds, and five successful
  pings from approximately 1–20.5 seconds. Logs:
  `tmp/runtime-probe-live-counters-bundle-background.{log,json}`. Agent capture
  failed with exit 1; root inspected a CUA capture of the painted private channel.
  The rebuilt live client remains open (owned PID 37398 at this checkpoint,
  socket `/tmp/sp-runtime-live-counters-bundle-background.sock`). Diagnostics
  expired after their bounded run. This remains a healthy counterexample, not
  a resolution of the intermittent blank startup; no Slack messages were sent.

### 2026-10-13 — Existing-paragraph authoring and Files density in parallel

- Three workers split canonical canvas text protocol, desktop editor/lifecycle,
  and compact Files layout plus independent review. Root owns live reference
  captures, integration builds and native validation.
- Stock Slack reference capture used only the existing task-owned private canvas.
  The appended text probe and exact restoration both received HTTP 200, and a
  reload showed the original bold/Unicode paragraph. Before/after canonical loads
  prove all five non-target sections byte-identical and target content/parents
  restored. The original Slack route and empty composer were preserved.
- Files changes reduce rows from 72 to 56 pixels and previews from 44 to 32,
  compact surrounding controls, center row actions and preserve full title and
  metadata tooltips. An isolated normal/narrow checker covers geometry, search
  draft/caret preservation during a result-count render, actions, focus and
  reaching the final row. Native build/capture validation is pending.
- Canvas existing-paragraph support is still in progress. The early integration
  check identified missing scraper parser-error support; enabling its existing
  zero-dependency `errors` feature keeps malformed markup rejection available.
  No native text save, new passing suite or full rich-authoring parity is claimed
  at this checkpoint.

- Frozen domain and desktop suites passed **934 core tests, five ignored**
  (`tmp/canvas-text-core-readback-fixed.log`) and **788 desktop tests, five
  ignored** (`tmp/canvas-files-desktop-final-tests.log`). Both formatting checks
  and diff checks passed. Independent review corrected a post-ACK race: a newer
  legitimate canonical edit settles the prior accepted receipt; an equal revision
  still needs matching content/parents. The regression reconstructs a validated
  other-file snapshot, rather than mutating only its public File metadata.
- The signed build and strict signature check passed
  (`tmp/canvas-files-final-build.log`). Files native normal/narrow gates passed
  (`tmp/files-density-native-final.log`), with ten and three fully visible rows
  respectively, stable 56-pixel rows and intact controls. Root inspected both
  final Files PNGs; full titles remain available and narrow titles ellipsize.
- Canvas normal native interactions passed, but narrow screenshot inspection and
  geometry found clipped Save/Cancel controls. The first captures also showed an
  inherited primary-button margin and a misleading perpetual Saving label for an
  uncertain result. The follow-up gives the editor a bounded text scroll area,
  fixes both actions at 32 pixels, and distinguishes Checking/Unconfirmed. The
  native checker now uses the intersection of modal, scroll-container and window
  bounds in initial, invalid, stale and uncertain states. Final follow-up build
  and native rerun are pending at this checkpoint.

- The bounded editor follow-up passed signed normal/narrow interaction and
  ancestor-clipping gates (`tmp/canvas-text-layout-native-final.log`, repeated
  after the parent readback correction in `tmp/canvas-readback-native-final.log`).
  Root read both final PNGs: formatting controls, scrolling text, helper copy and
  32-pixel actions fit inside the modal at both sizes. These captures show the
  synthetic uncertain state, not a live lost-response injection.
- Core parent normalization is now covered by **936 passed, five ignored**
  (`tmp/canvas-text-retention-core-final.log`). The final retention build and
  strict signature check passed (`tmp/canvas-text-retention-final-build.log`).
  A later focused desktop canvas gate passed **29 tests**
  (`tmp/canvas-text-desktop-last-gate.log`); the full desktop baseline remains
  788 passed / five ignored before this follow-up.
- Native keyboard append and exact restoration were independently visible in
  stock Slack, with bold/Unicode text and unchecked/checked preserved. The first
  pair automatically returned to read view. Canonical baseline/final comparison
  proves six stable section IDs, all five other sections byte-identical, and
  target content/parents restored (`tmp/canvas-text-final-live-comparison.json`).
  That pair exceeded Slack's retained-history expiry and therefore legitimately
  rotated its actor; it does not prove retained reuse.
- An additional rapid live repetition exposed a late-event ordering: a successful
  accepted read can settle the receipt just before a file event marks the view
  stale, preventing the draft-close effect or leaving read view disabled. The
  append was reconciled with read-only Refresh and exact restoration independently
  confirmed. A focused worker is correcting receipt-driven closure and bounded,
  event-driven visible-reader recovery; no mutation replay is permitted.
- Three available workers independently reviewed desktop lifecycle, domain
  parent normalization and evidence. The runtime permits root plus three active
  workers; all three were used, within the user's maximum of twenty.

- The rapid pair additionally proves retained-session reuse: its authoritative
  incremental responses share one trace prefix with counters 0→1, preserve the
  same CRDT base/head and extend processed history from two to three heads.
  Saves were 24.17 seconds apart, within the first history expiry. A subsequent
  complete canonical reload proves all six identities and all five non-target
  sections unchanged, with target content/parents exactly restored. Evidence:
  `tmp/canvas-text-retained-live-comparison.json`. Stock Slack's original DM route
  and empty composer were restored; the private capture was stopped.

- Late-event correction is frozen and independently reviewed. Positive settlement
  records the exact accepted text attempt on its current visit, so later cache
  invalidation cannot strand that draft. Visible loaded canvases coalesce new
  file-event revisions into one pending component-owned read task with a 250ms
  debounce. Reads defer during export/rename/write preparation/dispatch and while
  offline; a failed read latches automatic recovery off until explicit refresh
  or reopen. No background polling or mutation retry was added. Three regression
  tests cover settlement/late invalidation, absent or uncertain receipts, event
  coalescing, errors, write stages, manual recovery and account retirement.
- The full final desktop suite now passes **791 tests, five ignored**
  (`tmp/canvas-late-event-final-tests.log`). The independently locked core result
  remains 936 / five ignored because this correction only changes desktop state.
- Explicit remaining gap: a file event during the first canonical/HTML export
  load, before any snapshot exists, can still require manual Refresh. This batch
  addresses event recovery for an already loaded canvas; it does not invent a
  separate export retry policy. IME and live lost-response/account-rotation fault
  injection also remain outside the verified scope.

- Final signed late-event build and strict signature verification passed
  (`tmp/canvas-late-event-final-build.log`). Both signed native text fixtures
  passed again (`tmp/canvas-late-event-native-final.log`); root read both PNGs.
- A fresh process then completed native keyboard append and exact restoration
  without manual Refresh. Both returned automatically to Read view; after stock
  reload the original bold/Unicode paragraph and unchecked/checked pair remained,
  and native Edit/checkbox controls were enabled with no stale banner. Root
  inspected the final restored CUA screenshot. The agent screenshot command
  failed with screencapture exit 1, so no saved final live PNG is claimed. The
  live pair validates end-to-end completion; deterministic ordering of the late
  event is established by the new regression, not assumed from that successful
  run. Stock Slack was restored to its original route and empty composer.
- Updated client remains running on Home, owned PID 59287 at this checkpoint,
  socket `/tmp/sp-canvas-late-event-live.sock`. No Slack messages or sharing
  changes were sent. Final formatting and diff checks pass. Full Slack parity,
  general canvas authoring and the intermittent blank-startup investigation
  remain open; this is a completed bounded batch, not completion of the goal.


### 2026-10-13 — First-load canvas recovery, conversation app control and runtime evidence

- Prior batch classified as progress: native paragraph save/restore and retained
  history were independently verified; full desktop/core gates passed and the
  loaded-canvas late-event defect was corrected. The overall goal remains open.
- Three workers now own paired first-load canvas recovery, ordinary app-message
  conversations_select, and bounded startup socket observations. Root owns
  integration and all live/native validation. Conversation candidates must cover
  direct/group conversations and source-backed filters, not only channels.
- Two sequential same-build live background launches, with the runtime probe off
  then on, stayed responsive and visibly painted. Their individual paint timings
  and ping samples are recorded in `docs/runtime-responsiveness.md`; they are
  healthy counterexamples, not a blank-startup fix or measured speedup. Sparse
  native accessibility snapshots coexisted with painted WebView screenshots.

- Ordinary app-message `conversations_select` now has typed capture, complete
  sibling form state, guarded confirmation and bounded shared picker discovery.
  It supports channel, group-DM and member results; member IDs pass through
  unchanged without opening a DM. Source-derived include/exclusion defaults and
  metadata freshness have regressions, including a newer rejected remote result
  suppressing stale cached fallback. Internal team-restricted selection on Grid
  is explicitly unavailable because required account-types normalization state
  is not modeled; ordinary filters remain available. Live app receipt remains
  unverified. Public contract: https://docs.slack.dev/reference/block-kit/block-elements/select-menu-element/.
- Paired canonical/export canvas loads now recover from first-load invalidation;
  stale exports can only supply an inert initial preview. Automatic reads remain
  coalesced, owner/visit guarded and latched off after either half fails. A reopened
  view deferred by a retained Saving operation receives its first read once that
  operation settles. No mutation replay was added.
- Final locked desktop suite passed **807 tests, five ignored**
  (`tmp/conversation-canvas-final-tests.log`), including the final remote rejection
  regression. Independently locked core suite passed **944 tests, five ignored**
  (`tmp/conversation-picker-core-tests.log`). Formatting/diff checks and the passive
  socket Node harness passed. Signed build and strict signature verification passed
  (`tmp/conversation-canvas-final-build.log`). Both normal/narrow conversation
  picker fixtures, canvas recovery/text fixtures and existing user picker
  regressions passed; root read all six PNGs. Evidence:
  `tmp/message-conversation-native-final.log`,
  `tmp/canvas-paired-load-native-final.log`,
  `tmp/message-user-conversation-regression.log`. These disconnected fixtures
  verify guarded native interactions and prepared requests, not live app receipts.

- Final signed live launch accepted the new passive socket fields and remained
  responsive for the controlled twenty-second observation; exact noncomparative
  results are in `docs/runtime-responsiveness.md`. The private test canvas opened
  through the paired reader with original bold/Unicode paragraph, unchecked/checked
  pair and enabled edit controls. Root inspected its live screenshot. No live
  canvas writes, Slack messages or sharing changes occurred in this batch. The
  agent screencapture command failed (exit 1), so only the inspected CUA image is
  claimed for this live read; fixture PNGs were captured successfully.
- Updated signed client remains running, owned PID 80845, socket
  `/tmp/sp-runtime-controlled-a3-bundle-background.sock`. Overall Slack parity,
  live app receipt verification, richer canvas authoring and the intermittent
  blank-startup cause remain open. This batch is progress, not goal completion.

### 2026-10-13 — Next everyday interaction slices (source audit only)

No implementation or new live verification is claimed for these priorities:

1. **Channel mute/unmute.** The notification and sidebar models already consume
   mute preferences (`crates/super-platinum-core/src/state/notifications.rs`
   and `sidebar_prefs.rs`),
   and `src/desktop/realtime.rs` applies preference events. Native channel details
   (`src/desktop/view/channel_details.rs`) has no corresponding control. First
   capture the exact stock notification-write protocol; do not infer it from the
   read model. A bounded controller/action should preserve unrelated preferences.
   Validate one explicit write in a task-owned private channel, independent stock
   readback, immediate sidebar/notification changes, account guards, restart
   persistence and restoration of the original setting.
2. **Thread reply “Also send to channel”.** The thread composer in
   `src/desktop/view/composer.rs` lacks the control, and
   `src/desktop/state/composer.rs` plus `messaging.rs` do not retain broadcast
   intent in ordinary pending sends. Core draft/scheduled request builders already
   support `reply_broadcast`; that alone does not establish the native workflow.
   Carry explicit intent through draft, send and scheduling boundaries. Offline
   checks should cover destination changes and duplicate-send protection; private
   validation should prove one canonical reply appears in both thread and channel
   without duplicate rows, with ordinary replies remaining thread-only.
3. **Edit own profile fields.** `src/desktop/profile.rs` displays name, title and
   pronouns but offers no editor. Existing `bootstrap/self_menu.rs` profile writes
   handle status, and the core `users.profile.set` primitive is available. Start
   with a separate guarded display-name/pronouns editor, after checking field
   restrictions in stock Slack. Test Cancel/no write, untouched-field preservation,
   stale-field conflicts and account changes. Any authorized private-account
   verification must compare fresh canonical readback and restore exact original
   values; profile changes are workspace-visible even when testing from a private
   conversation.

These are current code omissions, not newly reproduced live failures. The proposed
acceptance checks remain pending and do not expand this batch's completed scope.

### 2026-10-13 — Composer emoji and mention-search handoff (read-only audit)

- **Composer emoji insertion remains absent.** Channel/thread controls in
  `src/desktop/view/composer.rs` expose formatting, attachments, send and schedule;
  `reaction_picker::EmojiPicker` currently serves reactions and self status only.
  Core `composer_completion::Kind` supports people/channels, not emoji shortcode
  suggestions. Manual shortcode typing remains possible. Reuse the bounded emoji
  picker for an explicit composer action, capturing the exact account, destination,
  field scope, DOM text and selection before opening. Insert through the existing
  reference-aware textarea boundary in `composer_references.js`; do not replace
  a newer draft or recover identity from a display name. Test channel and thread
  insertion at a caret and over a selection, Unicode offsets beside mentions,
  Cancel/no change, focus/caret return, native undo, and navigation/account changes
  while the picker is open. Normal/narrow native checks must verify the selected
  emoji reaches the intended draft without sending a message.
- **Mention directory search can bypass Retry-After when its task is canceled.**
  `src/desktop/view/completion.rs` cancels the previous lookup on every new query
  or dismissal, then permits another lookup after 250 ms. The transport's retry
  delay lives inside that canceled request future; completion retains no endpoint
  cooldown. Thus changing `@query` during a 429 backoff can start a fresh request
  before the deadline. Preserve observed backoff independently of query lifetime,
  scoped to the authenticated owner/workspace, while retaining current query,
  destination and transport guards for installing results. Recheck admission
  after debounce; shorter late errors must not shorten a longer deadline. Use
  `view/completion.rs` and focused helper tests plus
  `scripts/composer-completion.test.mjs` for controller coverage. A controlled
  transport regression should return 429, change/dismiss/reopen the query, and
  prove zero new requests until expiry, cached choices still usable, and stale
  results unable to replace the current selection. Existing message user-picker
  cooldown handling is a reference, not proof that composer search already has it.

These are code-backed findings; no new live reproduction, implementation, or
validation run was performed for this handoff. Work waits until the current
signed/native integration gates finish.

### 2026-10-13 — Composer emoji, thread broadcasts, and retained mention cooldown

The preceding composer/broadcast handoffs are now implemented; live parity is still pending.

- Shared channel/thread formatting controls open the existing bounded emoji picker.
  Insertion preserves UTF-16 selections and mention identities through the existing
  textarea reference boundary; account, destination, visit, text, IME and stale
  picker callbacks are guarded. Standard emoji insert glyphs; custom emoji insert
  workspace shortcodes. Shortcode suggestion completion is still absent.
- Thread replies expose a draft-owned “Also send” checkbox, including stock
  channel, DM and group labels. The choice survives local draft navigation/recovery
  and follows captured destinations into text sends, uploads and schedules.
  Thread DOM scopes now include the visit generation so delayed clearing cannot
  erase an identical draft in a reopened thread.
- Draft/text/broadcast-file writes use a strict one-attempt HTTP boundary.
  Broadcast files finalize privately before one files.share request; explicit API
  rejection differs from uncertain delivery. Canonical upload readback projects
  into both original bags without overriding newer rows or tombstones.
- Mention search retains endpoint admission and Retry-After independently of a
  canceled query receiver; a controlled HTTP regression covers that cancellation.
- Final Rust gates: desktop 823 passed / 5 ignored; independent core 957 passed /
  5 ignored. Composer JavaScript gates: 32 passed. Formatting and diff checks pass.
  Logs: tmp/composer-broadcast-{desktop,core,js}-verified.log.
- Native fixtures and live Slack verification are the next gate. Do not infer
  real broadcast delivery, trusted OS undo, or attachment parity from unit tests.
- Original dependency updates remain present, and system activity uses the muted
  token. Earlier committed CI evidence does not validate the current dirty tree.
- Next separate feature: a scoped Mute/Unmute channel action. Capture stock's
  permanent plus temporary mute writes before implementing; clearing only the
  permanent flag is insufficient when a temporary mute is active.

Native checkpoint: signed bundle and strict deep signature verification passed.
Both thread-broadcast fixtures passed and their PNGs were inspected: visible
checkbox/focus, bounded normal/narrow layout, one offline queued send, captured
destination, retained choice, stale-visit rejection, and schedule choice all pass.
The emoji fixture failed while reopening/focusing its picker after earlier
insertion; its failure PNG was inspected. This is an open native gate, not an
emoji completion claim. Logs: tmp/thread-broadcast-native-verified.log and
tmp/composer-emoji-native-verified.log; captures under tmp/agent-ui/.

Emoji native resolution: a composer-only capture-phase guard now consumes each
original dialog's first choice before the native event bridge; duplicate clicks
or Enter events from that dialog remain blocked after its ticket is retired.
The duplicate-click regression was retained. Both emoji fixtures now pass and
their PNGs were inspected, including selected Unicode text beside a mention,
custom shortcode insertion in threads, Cancel/caret return, newer text rejection,
navigation retirement and bounded 120-choice normal/narrow layout. Final desktop
823 / 5 ignored, core 957 / 5 ignored, JavaScript 33 / 0 failed; signed bundle
and strict signature verification pass. Final logs: tmp/composer-broadcast-
desktop-final-gate.log, tmp/composer-broadcast-js-final-gate.log,
tmp/composer-emoji-guard-build.log, tmp/composer-emoji-native-final.log.
The repeat native run also passed both sizes; both final PNGs were inspected.
Repeat evidence: tmp/composer-emoji-native-repeat.log. The earlier intermittent
symptom did not recur in either post-fix sweep.
Real Slack text/file broadcast delivery and trusted OS undo remain pending.
Stock Slack was restored to its original conversation with an empty composer;
no real Slack messages were sent in this batch.

Mute protocol handoff (captured source, no writes): shared-boot-async.56e901
bundle BtIR at14609200 sends users.prefs.setNotifications via uhjK.Kp at7590700
and full-capture.0b815 Nu4c at1448220. The multi-pref shape is global:false,
channel_ids:[id], prefs:[{name:"muted",value:"true"|"false"}], sync:false.
Temporary mute uses temp_mute_expiration_ts with a Unix-seconds string. Stock
Unmute sends only muted:false; the inspected local merge does not clear expiry.
Do not invent an additional expiry write: canonical server readback must settle
the behavior for an actively temporary-muted channel. Preserve all unrelated
notification preferences and test only the authorized private channel.

### 2026-10-13 — Live composer verification and channel mute integration

Using the signed client in private #echotestchamber (C0AQDL3V5GU), root
inserted a rocket through the thread emoji picker. Real Cmd-Z removed only the
emoji and Cmd-Shift-Z restored it. Broadcast text arrived as canonical
1791926748.668329 in both stock channel and thread; ordinary reply
1791926881.147329 appeared only in the stock thread. Both native pending text
rows retired after acknowledgment.

The inspected synthetic 326-byte PNG was then sent as a broadcast with caption
“Super Platinum file broadcast check · 2026-10-13 · private fixture”. Stock
channel/thread each show one canonical 1791926912.286919, file F0C8TKF2KV1.
Native shows that same canonical file in both views, but retained a duplicate
local Delivery unconfirmed row. Root inspected the live screenshot (the agent
screenshot save failed; do not claim a saved PNG). This exposed strict
files.share envelope parsing plus loss of finalized IDs on uncertainty. The fix
must retain exact IDs for canonical readback without replay; parser relaxation
requires actual response shape evidence. A structural-only env-gated diagnostic
is planned for the next authorized private validation.

Channel Mute/Unmute API, guarded desktop controller, About action and offline
fixtures are being integrated in parallel. Their final Cargo/native/live gates
are still pending.


Post-integration result: desktop 832 passed / 0 failed / 5 ignored; independent
core 964 passed / 0 failed / 5 ignored. Formatting, diff checks, signed bundle
build and strict deep codesign verification passed. Normal and narrow channel
mute fixtures passed; both PNGs were inspected. Live native Mute changed stock
Slack to Muted; native Unmute restored the original Just mentions preference.
Temporary-mute server expiry behavior remains unverified.

A second authorized synthetic file broadcast after the recovery fix arrived as
canonical 1791927594.119769 with file F0C8Y0606NA. Playwright independently found
exactly one matching stock channel row and one matching thread row. The native
pending row retired, both canonical views show the fixture, and both composers
are empty. The live screenshot was inspected through CUA. This proves normal
live recovery for this upload; it does not prove outage/restart recovery. The
opt-in tracing diagnostic emitted no receipt line in the packaged launch, so
no real envelope-shape inference is justified. It is being changed to emit its
already privacy-tested allowlisted structure directly when explicitly enabled.

Parallel follow-up: bounded pagination for upload readback beyond 100 replies,
channel broadcast parent context/jump, and dependency/system-message audit.
Finalized upload IDs still need a retained recovery workflow if immediate
readback fails; current confirmation must not be presented as durable recovery.


Original request audit: requested rustls 0.23.45, Tokio 1.53.2 and objc2 0.6.5
are already locked. PRs https://github.com/3kh0/super-platinum/pull/21 and
https://github.com/3kh0/super-platinum/pull/22 are closed, unmerged. The direct
Tao update was superseded by using pinned Dioxus Tao 0.34.8 / Wry 0.53.5,
avoiding the mixed-version Windows hwnd error from CI 38244163656. Dependency
baseline CI 38261872738 passed all ten jobs. Current remote HEAD d6de69f run
38392261877 fails on List creation E0499/E0382; those exact source errors have
already been repaired in this dirty local tree. Local checks are not proof that
the remote CI is green. No additional dependency change or remote merge was made.
System-message body styling remains muted while authored subtypes stay normal;
the inspected live native channel also displayed its topic/description notices
in gray. Stock Slack was restored to its original DM route after verifying both
private test composers were empty.


### 2026-10-13 — Parallel upload recovery, broadcast context and mute expiry

Upload receipt readback now follows up to ten 100-row history/reply pages under
one 30-second budget, checking owner before/after each request and refusing
cursor cycles/missing continuations. Exhaustion is uncertainty, never permission
to resend. Finalized file IDs are also retained on the in-memory pending upload.
Canonical realtime/history/reply ingestion reconciles exact account, sender,
destination and file IDs, respecting tombstones in both bags. Fresh events use
binary lookup of incoming timestamps; only receipt attachment scans a relevant
bag once for realtime-before-ACK. Four same-session regressions and three page
scan regressions cover both arrival orders and owner/proof rejection. Restart
recovery and an explicit Check delivery control are not implemented.

Channel broadcasts now carry a quiet Replied to a thread context link and an
optional bounded parent preview. The channel projection supplies authoritative
channel identity even when Slack omits it from individual history rows. Deleted
parents stay masked; rich Block Kit parents use label-only context rather than
stale fallback text. Thread rows omit the preamble. Context click, toolbar Reply
and double-click synchronously admit the exact parent/reply destination, then
load that captured thread visit; double-click focus does not wait for network.
Legacy reply_broadcast rows now remain visible alongside thread_broadcast rows.
Native fixtures/checker: message-broadcast and message-broadcast-narrow.

The sidebar now derives permanent and temporary mute state from one canonical
notification projection at boot, realtime updates and readback. A local scalar
expiry deadline avoids repeated preference scans between changes. Loaded warm
caches rebuild once; unloaded fixtures/caches preserve their current mute sets.
Three core and one desktop regression cover expiry, preserved permanent mutes,
canonical preference preservation and retained uncertain mutation ownership.
The independent production review found no outstanding ownership blocker.

Core gate: 968 passed / 0 failed / 5 ignored. Desktop integration initially
caught an RSX partial move of preview; root changed the rendered binding to borrow
it. The post-fix desktop and native gates are pending below.


Final integration gates: desktop 843 passed / 0 failed / 5 ignored; core 968
passed / 0 failed / 5 ignored. cargo fmt --check and git diff --check passed.
Signed macOS bundle build and strict deep signature verification passed. Both
message-broadcast fixtures and both channel-mute fixtures passed on this build;
root inspected all four final PNGs. Broadcast links/parent reply targeting,
thread-only preamble omission, immediate double-click composer focus and bounded
normal/narrow geometry passed. Logs: tmp/parallel-recovery-desktop.log,
tmp/parallel-recovery-core.log, tmp/parallel-recovery-build.log,
tmp/message-broadcast-native.log and tmp/channel-mute-native-final.log. Captures:
tmp/agent-ui/message-broadcast{,-narrow}.png and channel-mute{,-narrow}.png.
No new Slack writes were required for these final offline checks. Full Slack
parity remains an active goal; temporary-mute server duration behavior, durable
upload recovery, and additional feature/live coverage remain open.
