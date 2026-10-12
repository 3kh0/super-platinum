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
| Compose and reply, formatting, drafts, edits, deletes | Formatting controls and shortcuts implemented; native Unicode/selection checks and bold private round trip passed | Remaining draft/edit/delete/list/link/mention workflows |
| Mentions, emoji, custom emoji, new reactions | Picker, full catalog, aliases, private custom reaction add/remove passed across both clients and thread root | Mentions, composer emoji insertion, tone/recents, arbitrary reply reaction checks |
| Search, filters, results navigation | Existing; audit pending | Live search and pagination against Slack |
| Files, upload/download, previews, channel files | Native browser, pagination, private upload/search/image preview and byte-identical download verified | File actions, PDF/text previews, large original downloads, native canvas/list editing |
| Later, save/remind, completed items | Native surface, pagination, message actions, personal reminder creation, and cross-client realtime verified privately | Cold target navigation, full saved-index coverage, live preview edits/deletes, narrow-window audit |
| Pins, bookmarks, channel tabs | Pins and channel Files & links implemented; private bidirectional pin round trip verified | Bookmarks/custom tabs, older thread targets, large lists, offline and permission cases |
| Forwarding and message links | Rich notes, known conversation selection, and private forward round trip implemented | New DM discovery, scheduled forwarding, multiple destinations/partial errors |
| Channels, membership, details, management | Native create, About/Members, profile navigation, name/topic/description editing and archive/unarchive verified privately | Join/leave, member administration; large lists, permission/offline cases |
| DMs, group DMs, profiles, presence, status | Existing partial surface; audit pending | Private navigation; safe status/draft checks |
| Threads, Activity, read state, subscriptions | Existing; audit pending | Both-client realtime and read-state reconciliation |
| Huddles, camera, sharing, device selection | Existing; live media unproven | Controlled private call and media checks |
| Canvases, lists, app interactions, workflows | Audit pending | Reference/API discovery; private create/edit/reopen |
| Notifications, DND, settings, accounts | Existing; audit pending | Native bundle tests, persistence, isolation |
| Keyboard, focus, accessibility, narrow windows | Partial | Native interaction, labels, reduced motion, layout sweeps |
| Geist visual system | In progress | Native screenshots of all surfaces and states |
| Speed, memory, idle work, large workspaces | Baseline pending | Reproducible before/after startup, interaction, and resource measurements |

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
