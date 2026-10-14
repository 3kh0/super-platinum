# Notification sound, preview, badge, and attention protocol

Implementation and research handoff, 2026-10-13. Message/VIP sound, preview,
and messaging mute controls passed unit and normal/narrow native verification,
including completed WebView audio playback and live read-only rendering. Live
preference writes, audible OS notification delivery, visible dock behavior, and
cross-platform notification parity remain unproven. Dock bounce preference/UI
and guarded native attention are now implemented; their desktop/native gates
are running at the follow-up checkpoint below. No huddle
calls are part of this slice.

## Evidence and scope

Offsets below are character offsets in these local captured official bundles:

- **B**: `tmp/sb/js/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`.
- **S**: `tmp/sb/js/gantry-v2-shared.full-capture.0b81595876785d7a.min.js`.

`SoundAppearancePref` is B261240xx–261317xx. `MacAppearancePref` is
B261989xx–262006xx. `WindowsAppearancePref` is B26207227–262101xx. These are
three distinct persistence families: authenticated user's notification defaults,
authenticated user's ordinary preferences, and desktop-local settings. The word
“global” in notification defaults does not authorize changing other users or all
workspace members. Propagation between underlying Grid workspaces is not proved
by these component paths; retain the authenticated account/team/user boundary.

## Server-backed fields

| Control | Store / key | Source value and display semantics | Setter |
| --- | --- | --- | --- |
| Message notification sound | `all_notifications_prefs.global.desktop_sound` | Catalog filename or `"none"`; UI selects exact raw match. Selection's empty/falsy value becomes `"none"`. | Global scalar, reason `""` |
| VIP message sound | `all_notifications_prefs.global.priority_desktop_sound` | Same catalog; `"none"` means **same as messages**, not silence. UI requires VIP availability and a nonempty VIP list. | Global scalar, reason `select-new-vip-notification-sound` |
| Include message preview | `all_notifications_prefs.global.no_text_in_notifications` | Stored boolean is inverted checkbox state; raw truthiness becomes the hidden-preview flag. | Global scalar, reason `""` |
| Mute all messaging sounds | User pref `mute_sounds` | Boolean, checkbox `!!raw`; affects sounds, not banner eligibility. | Ordinary user scalar |
| Include thread replies in Activity badge | User pref `activity_include_threads_in_badge_count` | Boolean, checkbox `!!raw`; not the dock badge switch and not `threads_everything`. | Ordinary user scalar |
| macOS activity badge | User pref `mac_ssb_bullet` | Boolean; checkbox `raw ?? true`. Represents activity bullet behavior, not a promise that all numeric badges disappear. | Ordinary user scalar |
| macOS bounce | User pref `mac_ssb_bounce` | `""` off, `"short"` once, `"long"` persistent. Raw falsy becomes off. Local duration starts `short` when off; enabling uses the mounted duration. | Ordinary user scalar |

Sound/preview source handlers are B261253xx–261270xx; thread badge handler is
B261271xx. Native desktop-only appearance controls are mounted through the
platform predicates at B262116xx. The preview and bounce controls are disabled
when `Fpyc.Wv` says global desktop notifications are disabled. That selector is
exactly raw `global_desktop === "nothing"` (S3857269), not an OS authorization
query. Message sound choices are hidden while `mute_sounds`, that legacy disabled
mode, or the browser permission-button predicate applies (B261312xx).

`Fpyc.shouldShowPermissionButton` at S3855084 depends on browser notification
capability/permission; `checkPermission` at S3855006 accepts desktop integration
or granted browser permission. Keep OS authorization state separate from stored
Slack preference state. A successful preference write does not prove that macOS
will display or sound a notification.

### Exact wire families

`giaO.S` (B7593569) calls `Nu4c.vQ` (S14475xx), with default `sync=false`:

```text
POST users.prefs.setNotifications
name=desktop_sound
value=b2.mp3
global=true
sync=false
_x_reason=
```

The same shape applies to preview and VIP sound with their own key/value/reason.
There is no `channel_id` or channel list. The value is the scalar, not a JSON
object containing all notification preferences. Confirm through fresh
`users.prefs.get` → `prefs.all_notifications_prefs.global`, preserving all
unrelated global keys and channel overrides.

`N0Kh.AZ` (S5326205) calls `Nu4c.CI`, the ordinary scalar setter at
S14464xx–14469xx:

```text
POST users.prefs.set
name=mute_sounds
value=true
_x_reason=prefs-api/setUserPrefByApi
```

The same setter handles the Activity badge and macOS appearance user prefs.
`3l3p.Vc` (S5313863) only JSON-serializes its special JSON-pref allowlist; these
ordinary booleans/strings remain scalars. Fresh read is `users.prefs.get` with
`prefs` a comma-separated list of the exact names (`Nu4c.jH`, S14455xx), returning
flat `prefs` keys. Preserve absence/null rather than writing UI defaults back.
Boot and `pref_change` are existing hydration paths, not proof a prior write
succeeded.

The captured client optimistically updates its store. Native implementation
should retain the established one-attempt/no-replay, fresh preflight, explicit
unknown-outcome reconciliation and shared authenticated prefs cooldown used by
channel/global/schedule editors. The dedicated presentation transport narrowly admits
these presentation keys, without exposing an arbitrary users.prefs writer. Keep server global and ordinary user intents separate so partial failure
cannot be presented as an atomic combined save.

## Sound catalog and local preview

`LWDq.$O` at S4751978 supplies these exact message choices:

| Value | Label |
| --- | --- |
| `b2.mp3` | Ding |
| `animal_stick.mp3` | Boing |
| `been_tree.mp3` | Drop |
| `complete_quest_requirement.mp3` | Ta-da |
| `confirm_delivery.mp3` | Plink |
| `flitterbug.mp3` | Wow |
| `here_you_go_lighter.mp3` | Here you go |
| `hi_flowers_hit.mp3` | Hi |
| `knock_brush.mp3` | Knock Brush |
| `save_and_checkout.mp3` | Whoa! |
| `item_pickup.mp3` | Yoink |
| `hummus.mp3` | Hummus |
| `boop.mp3` | Boop |

Normal message choices prepend `none` (“None”); VIP choices prepend `none`
(“Same as messages sound”). `boop_remix.mp3` (“Boop Plus”) is the separate
`LWDq.N7` huddle catalog extension and should not be added to the message dropdown
by inference. Selecting a non-none message/VIP sound plays a local preview through
`Ofe8.ZH` and writes the preference; these are separate effects. `Ofe8` at
S47535xx uses cached HTML Audio, restarts at zero and defaults volume to one.
This is not a Slack message, huddle, or notification API call.

`Fpyc.getNewMsgSoundPath` at S3862441 requires a truthy raw `desktop_sound`, then
selects the VIP or ordinary catalog URL. `canPlaySound` at S3863147 also checks
native OS permission, `mute_sounds`, DND/VIP override and a feature-gated
per-conversation sound debounce. Do not claim all this is reproduced solely
because a saved filename is played by UserNotifications.

Adjacent controls intentionally outside this initial handoff: huddle sound/mute,
calendar sound/mute, and focused-conversation send/arrival sounds. In particular,
B6353974 `NJaK` persists send/arrival choices in desktop client local storage,
under `conversationSentMessageSound` / `conversationIncomingMessageSound`
(S1882743), not `users.prefs.set`. Their source defaults are item pickup and
complete quest respectively (`W+TA`, S4556886). Do not turn those local settings
into guessed Slack preference keys.

## Badge, bounce, and platform-specific boundaries

`maybeBounceDockIcon` (B30530xx) checks desktop integration and nonempty
`mac_ssb_bounce`; `long` invokes the continuous-attention bridge, any other
truthy value invokes the once bridge. The UI only authors empty/short/long.
Native attention retains and retires the exact AppKit request on the
focus/lifecycle boundaries described below; the captured helper alone does not
define that additional cleanup policy.

The desktop unread watcher `3Y5k` at B6467xx computes `showBullet`,
`unreadHighlights`, `hasUnreads`, and `unreads` separately, forwarding them to the
Slack desktop bridge. `mac_ssb_bullet` is not a replacement for Slack's unread
highlight computation. Current native core unread/sidebar data should be used;
full dock aggregation and attention cancellation need a further focused source
trace before an implementation claims exact parity.

Windows uses desktop-local `windowFlashBehavior` (`never`, `idle`, `always`),
with `idle` meaning inactive at least 10 seconds. B26207227 reads
`settings.win32.windowFlashBehavior ?? "idle"` and writes via `YJ4z.cGU`, not a
Slack server endpoint. The same component's notification method/position
controls are desktop-local too. Native Windows platform work must not send these
names through users.prefs APIs or pretend the macOS bounce setting controls them.

## Current native implementation and concrete gaps

- `core/src/state/notifications.rs` hydrates the complete global/channel object,
  plus a restricted ordinary-user allowlist containing `mute_sounds`. It projects
  privacy and normal/VIP sound, including VIP `none` fallback. It does not
  currently hydrate `mac_ssb_bullet` or the Activity-thread badge preference.
  These require cache-compatible hydration and realtime adoption.
  `mac_ssb_bounce` now belongs to the hydrated/cache-compatible ordinary-user
  allowlist and follows the existing preference-change adoption path.
- `src/desktop/notification.rs` replaces hidden preview bodies with “New message”
  while retaining author/channel title. Existing pure tests cover that and silent
  banners. `notification-preview` is a deliberately silent local test banner;
  it does not prove selected sound playback.
- `notification/sounds.rs` allowlists bundled assets. macOS maps approved MP3
  names to CAF resource names; unknown names are silent and cannot access files
  or remote URLs. Missing normal sound now stays silent, matching the captured
  `getNewMsgSoundPath` truthy guard. VIP empty/none inheritance and raw truthy
  privacy/mute values have delivery regressions.
- `notification/macos.rs` uses UserNotifications under bundle identifier
  `com.echonet.superplatinum`, requests Alert/Sound authorization and attaches
  named sounds. It does not implement dock badges, and authorization requested
  here does not include Badge. Dock attention is separately implemented in
  `notification/attention.rs` through AppKit rather than UserNotifications.
- Linux now supplies the standard boolean `suppress-sound` hint and Windows
  emits `<audio silent="true"/>` for silent or unknown sounds. Known non-silent
  Windows sounds still use Notification.IM, and Linux still delegates audible
  sound selection to the notification server. Custom sound parity and audible
  platform verification remain open. Wire semantics are documented by
  [Microsoft](https://learn.microsoft.com/en-us/uwp/schemas/tiles/toastschema/element-audio)
  and [freedesktop.org](https://specifications.freedesktop.org/notification/latest/ar01s08.html).
- Settings > Notifications > Sound & appearance now exposes these four
  presentation controls through a separate bounded intent/controller, plus
  macOS-only Dock bounce and duration controls. It shares
  authenticated cooldown with the other editors, keeps canonical values during
  writes, and reconciles uncertain outcomes only through explicit reads.
  VIP eligibility uses actual boot team plan plus VIP membership; plan metadata
  is cache-compatible and unknown does not fabricate availability.
- Explicit Preview and non-none sound selection play bundled local audio, with
  generation-safe replacement, bounded failure cleanup, and stop on view exit.
  This works in bare dev without requesting OS notification permission.

## Build and verification handoff

Use the **bare dev executable** for ordinary UI, controls, fixtures, and ownership
validation. It remains the default; this slice does not require routinely
packaging or launching an app bundle. Use a **signed debug bundle only for
bundle-dependent macOS notification delivery and authorization checks**.
`scripts/macos-app.sh` defaults to `target/debug/Super Platinum.app`, converts
bundled MP3 assets to CAF and ad-hoc signs the bundle; no release build or
replacement of an installed app is needed. Launching that bundle through
LaunchServices is required for this app's native notification path. A bare dev
executable is deliberately rejected before UserNotifications initialization;
ordinary UI remains usable. Do not restore a Script Editor fallback. Offline
fixtures skip notification initialization and cannot prove OS delivery.

The implemented native slice is message/VIP sound + preview + messaging mute,
with exact separate server endpoints, fresh readback and local asset preview.
Both normal/narrow bare-dev fixtures passed six simulated writes/eleven reads;
root read the PNGs. Each native receipt recorded bundled Drop playback reaching
`ended` with duration and current time of 0.535510204 seconds, plus successful
selection-triggered preview. This verifies the WebView playback lifecycle, not
human-audible output or macOS notification delivery. Live read-only values matched
stock (Ding, Knock Brush, previews on, mute off), and Settings was closed without
a real preference write or message send. Evidence is in
`tmp/presentation-native.log` and `tmp/agent-ui/live-notification-presentation.png`.
Those results precede the bounce follow-up below. Badge behavior remains a
distinct, unimplemented platform integration. Verification should separate:
pure wire/hydration tests; normal/narrow
fixture controls and ownership races; bare-dev local preview; signed-bundle
notification delivery/silent behavior; and explicitly authorized preference
mutation/readback with exact restoration.
OS sound settings, Focus, browser permission and notification authorization can
suppress delivery independently. Live read-only rendering and WebView playback
are complete for this slice; real preference mutation/readback, audible OS
delivery and platform badge/bounce verification remain open.

## Follow-up: exact dock aggregation and attention lifecycle

Source follow-up, 2026-10-13. This section supplements the earlier handoff;
its badge research remains unimplemented. The later bounce implementation is
described below without claiming visible OS verification.
Besides B/S above, the installed **Slack 4.52.178** desktop archive was inspected
in memory, without extracting or modifying the app:
`/Applications/Slack.app/Contents/Resources/app.asar`.
The following offsets are characters in its entries:

- `dist/main.bundle.cjs` (**M**), SHA-256
  `895936036452c4aefb324d27e0e54b6bfb94f25edd2e001c60d3b0e6bc3db463`.
- `dist/preload.bundle.js` (**P**), SHA-256
  `0dd7442c53eec29f9b040265bad5d604b14608aaede116eb538bccb344b0d5ed`.

### Numeric count and activity dot are independent

B6467xx `3Y5k` sends each workspace an `unreads` object to desktop, through
`YJ4z.E6y` / `desktop.redux.dispatchUpdate`, only when the projected value changes.
The fields are `showBullet`, `unreadHighlights`, `hasUnreads`, and `unreads`.
`unreads` here is a 0/1 indicator, not a count of every unread message.

The exact desktop aggregation at M9601xx–9603xx sums `unreads` and
`unreadHighlights` across the desktop's `webapp.teams` map. Its aggregate
`showBullet` is true iff at least one workspace has both its `showBullet` enabled
and positive `unreads`. M1218022 `getTrayStateFromUnreadsInfo` then projects:

1. Positive `unreadHighlights`: decimal numeric badge, regardless of
   `showBullet`.
2. Otherwise positive `unreads`: unread state, with an activity dot only when
   `showBullet` is true; otherwise empty badge.
3. Otherwise: empty badge/rest state.

M1217763 applies the changed label with `dock.setBadge`. Development-only badge
label overrides exist separately; they are not user preferences. Therefore
turning off `mac_ssb_bullet` must **not** erase a positive numeric notification
count. Numeric values are notification/highlight totals and may include Activity
and Later, not exclusively literal @mentions.

The frontend aggregate has two source branches. With Activity Inbox enabled
(`SWeb.Lh`, S4894958), define:

- `A`: `fexP.cJ` Activity badge count (S5113086; V2 Activity count when Inbox is on).
- `L`: `fexP.Nq` overdue Later count for app badge (S5114754), only when reminders
  are not already included in Activity and the Later rail surface is enabled.
- `D`: `fexP.wf` V2 DM badge count (S5112591), included separately only when DMs
  are opted out of Activity (`SWeb.Vn` false).
- `U`: `vuTZ.Z6 > 0`, the existing total unread indicator (S3517254).

Then numeric highlights are `A + L + D`, and the unread indicator normally tests
that sum. When bullet is enabled and `A == 0`, the unread indicator instead uses
`U`, allowing ordinary unread activity to show a dot even without Activity
notifications. Without Activity Inbox, highlights use `xbJ9.p` (B8205790):
`JOlo.P` unread highlights plus eligible overdue Later count. `JOlo.P` excludes
suppressed DMs, then chooses totals with/without thread unreads according to the
thread-badge preference and source thread condition. Do not approximate either
branch by summing visible sidebar rows or the currently open Activity page.

### Stock bounce and cancellation boundary

S1313418 `IHvj` binds `desktop.dock.bounceOnce` and
`desktop.dock.bounceIndefinitely`. P693522 maps them respectively to the
`BOUNCE_DOCK` IPC with `informational` and `critical`. M1406215 performs the
native DND check before calling M1393298 `app.dock?.bounce(type)`.
The installed JS shell does not retain the return ID or call `cancelBounce`;
no explicit preference-change cancellation is evidenced in these entries.

Electron documents that critical attention continues until activation or
cancellation, informational animation lasts about a second but its request
remains until activation/cancellation, and a focused application does not start
a bounce. Those platform semantics explain stock focus cancellation without an
invented frontend reset request. The API also provides explicit ID-based
cancellation. [Electron Dock API](https://www.electronjs.org/docs/latest/api/dock)

A native implementation can retain request IDs to cancel on owner retirement,
shutdown, or an explicit disabled preference as a conservative lifecycle policy,
but must label those additional cleanup decisions as native policy rather than
an observed Slack JS callback. Do not replay attention on cache hydration or a
settings read; admit it from the same deduplicated eligible notification event.

### Existing native attachment points and pinned API trap

`core/src/state/unreads.rs` already maintains channel unread/mention counters
and guarded read acknowledgements; `Workspace::apply_counts` retains canonical
Activity unread count, and thread counts have a separate authoritative model.
`src/desktop/view/mod.rs` and `view/secondary.rs` consume the Activity total.
These are useful inputs, but the current data is not yet a proved complete port
of the source's Inbox/Later/DM-exclusion aggregate. A bounded core projection
should expose numeric highlight count and ordinary-unread boolean separately,
with explicit coverage for missing counts, muted/suppressed channels, threads,
Later, and multiple workspaces. Never derive them from a bounded transcript.

`src/desktop/realtime.rs` already passes actual `window.is_focused()` plus visible
conversation/thread context to `NotificationTracker::for_event_in_view`.
`runtime.rs` also samples focus for List viewing. The new attention controller
owns a bounded AppKit request; neither path owns an app-wide unread label. Avoid
an unconditional new scan on every runtime tick. Update the badge on relevant canonical count/preference/
workspace changes and send to the platform only when its label changes.

Pinned `Cargo.lock` has **tao 0.34.8** and **objc2-app-kit 0.3.2**. Local source
inspection establishes:

- `tao::platform::macos::WindowExtMacOS::set_badge_label(Option<String>)`
  forwards an app-wide dock label (`platform/macos.rs:86,166`). It can represent
  both numeric labels and the source activity dot, unlike a count-only method.
- `tao::Window::request_user_attention` accepts Critical/Informational, but its
  macOS implementation **discards the returned AppKit request ID** and does
  nothing for `None` (`platform_impl/macos/window.rs:1427`). The public docs
  explicitly warn `None` has no effect on macOS (`window.rs:1175`). It cannot be
  used as an explicit cancellation API.
- `objc2-app-kit` generated `NSApplication` exposes
  `requestUserAttention(NSRequestUserAttentionType) -> NSInteger` and
  `cancelUserAttentionRequest(NSInteger)` (`NSApplication.rs:665–671`), plus
  `dockTile`; `NSDockTile::setBadgeLabel` is available in its generated bindings.
  Direct use needs a main-thread owner and review of enabled Cargo features
  (the direct manifest currently lists `NSWindow`), not a guessed Dioxus hook.

Bare dev remains the default for ordinary UI, pure projection/lifecycle tests
and controls. Bundle-dependent authorization/delivery or actual dock behavior
must be verified separately when necessary; do not routinely replace the running
dev executable with a signed app just to exercise settings. The source-research
checkpoint above ran no Cargo, native attention request, dock mutation, or Slack
write; the implementation checkpoint follows.

## Implemented bounce and Focus admission checkpoint

The macOS settings path now authors only `mac_ssb_bounce` values `""`, `"short"`
and `"long"` through the ordinary scalar `users.prefs.set` family and its exact
reason above. The checkbox reflects a nonempty raw string; the mounted duration
is retained while off. “Once” and “Until I return” select short and long.
Legacy `global_desktop == "nothing"` disables these controls. The existing
owner/preflight/readback/uncertain-outcome guards apply, and checking an uncertain
result performs a read without repeating the write. Unknown stored strings are
preserved; new authored values are restricted to the known set.

`src/desktop/notification/attention.rs` admits only Message/DesktopNotification
events already accepted and deduplicated by the existing notification tracker.
Huddle invites, local notification previews, cache hydration and preference
reads cannot request messaging attention. Slack delivery/DND/VIP rules still
apply. AppKit's whole-application `isActive` is checked separately from visible
conversation suppression: a focused Settings window must not cause a bounce.
The controller coalesces eligible bursts during a Focus read and rechecks the
latest message's owner, realtime generation, eligibility and read position
before requesting attention. Off-thread Focus work is subject to a two-second
await timeout; timeout/error/unknown is quiet and never queued for replay.

The native request uses Informational for short and Critical for long, retaining
the exact returned AppKit ID. Replacement cancels the old ID; an existing
same-owner long request is not restarted. Focus, owner/credentials retirement,
disabled/changed preference and component shutdown cancel owned attention.
Maintenance uses the existing runtime tick and is inactive when no token or
pending read is owned. These additional cleanup rules are native lifecycle
policy, not a claimed stock JS cancellation callback. AppKit itself documents
activation cancellation and active-app no-op; it does not document automatic
Focus suppression. [AppKit attention API](https://developer.apple.com/documentation/appkit/nsapplication/requestuserattention(_:))

`notification/attention/focus.rs` ports the read-only macOS 12+ algorithm from
the installed Slack dependency **macos-notification-state 2.0.2**, npm git commit
`53d0fc4fc51f1b1133379c0944a44b28efb83c0d`. The module includes its MIT notice.
This is source-backed private-format compatibility, not an Apple-supported
permission-free Focus API. [Pinned upstream source](https://github.com/felixrieseberg/macos-notification-state/tree/53d0fc4fc51f1b1133379c0944a44b28efb83c0d)

- `Assertions.json` first `data` object supplies `storeAssertionRecords`.
  Any nonempty list is active; the first assertion's mode chooses the app
  exception. Upstream does not evaluate assertion expiry and explicitly leaves
  timestamp comparison as a TODO. Native code does not invent an expiry rule.
- Without an active assertion, the fallback schedule looks up mode `NONE` in
  `ModeConfigurations.json`. Enabled triggers use current local hour/minute,
  start inclusive/end exclusive, with overnight intervals. The upstream
  algorithm ignores end-minute and day filters; those limitations remain.
- When Focus is active, `ModeConfigurationsSecure.json` supplies the mode's
  allowed-application keys. Exact bundle matches and upstream's child-bundle
  substring match permit attention. A bare executable without bundle identity
  cannot inherit an exception.
- All filesystem work runs on the blocking worker. Reads are limited to regular
  files and one MiB per file; malformed/unreadable/oversized required state is
  `Unknown`, which suppresses attention. There is no preference mutation,
  permission prompt, steady filesystem poll or configuration-content logging.
  Unlike stock's error-to-false fallback, failure is conservative. Private schema
  changes can therefore disable bounce until support is updated.

Synthetic tests cover active assertions, malformed state, local schedule
boundaries/overnight behavior, bundle exceptions and isolated bounded file reads;
controller tests cover exact request ownership and retirement. The locked core
suite passed **1,054 tests** at this checkpoint. The final desktop suite passed **938 tests / 6 ignored**, and the independently
locked core suite passed **1,054 tests / 5 ignored**. Normal/narrow native bounce
checks passed with ten simulated writes, sixteen reads, and seven reachable
controls each; both PNGs were inspected. These are the extended bounce receipts,
not the earlier sound-only checks. No real global preference
write, visible Dock bounce, OS Focus suppression or audible OS notification
delivery is claimed here. Dock badge aggregation remains unimplemented.

## Badge category and consumer trace

Further read-only source trace, 2026-10-13. This establishes the next count
consumer boundary; it is not a claim that native Dock aggregation is complete.
In addition to B/S, the category enum is **V**:
`tmp/sb/js/gantry-v2-vendors.522a064003a98b05.min.js`, `Cla3` at 1846984.

### Canonical count snapshots and zero-feed handling

B4165296 `OKlm.transformClientCountsOutput` forwards the top-level
`client.counts.activity_v2` object unchanged to `activityV2`. B4474009
`applyCountsToChannelsAndThreads` dispatches `MdxY.Ql` when that field is present.
S5065152 exports Ql as `updateActivityV2BadgeCounts`; the reducer at S5107060
replaces `activityIA4.unreadActivityV2BadgeCounts` with the supplied map. It does
not merge categories. An explicit empty map clears the snapshot; an omitted map
does not authorize clearing it. Keep unknown category keys for forward-compatible
canonical storage even though the current display selector ignores them.

B5922593 `handleBadgeCountsUpdatedRTMEvent` uses the same replacement for an
Inbox-enabled `badge_counts_updated.activity_v2`. It separately sums the raw
map's values, not the filtered display count. A zero sum marks currently unread
root-feed items locally read through `QDAL.m`, with `isLocalOnlyUpdate:true`.
`2zxL.Sv` is `getActivityRootList` (B9508853/9509989), not the filtered or
unread-only feed. No Slack mark-read write follows this branch. Source also
falls back to `{}` for this sum when `activity_v2` is missing, while skipping
map replacement; native code must explicitly distinguish that source quirk from
an authoritative empty snapshot. A reaction-only or unknown-category positive
map can produce a zero display badge without authorizing this zero-feed cleanup.
The legacy event branch updates `counts` and `alerts`; it clears local unread
items only when the combined value list is nonempty and every value is zero.

HTTP count/feed loads require a snapshot revision guard against a newer realtime
replacement or local read transition. Loaded feed size and `total_unread_count`
must not overwrite the category map or substitute for its display projection.

### Exact Inbox category projection

`fexP.cJ` (S5113086) selects its Inbox branch only when the `activity_inbox`
assignment is exactly `on` (`SWeb.Lh`, S4894958). Its category map is canonical
`unreadActivityV2BadgeCounts`; the selected entries are summed once each:

- Always included: `thread_v2`, `channel`, `internal_channel_invite`,
  `list_record_edited`, `bot_dm_bundle`, `at_user`, `at_user_group`, `at_channel`,
  `at_everyone`, `keyword`, `list_record_assigned`, `list_user_mentioned`,
  `external_channel_invite`, `external_dm_invite`, `shared_workspace_invite`,
  `saved_reminder`, `prejoin_dm_welcome_party_alert`.
- `list_todo_notification`: `lists_monster_step == on` (S3791940).
- `list_approval_request` and `list_approval_reviewed`:
  `approvals_in_todos == on` (S3792107).
- `unjoined_channel_mention`: `unjoined_channel_mention_in_activity == on`
  (S4898588).
- `quietly_added_to_channel`: `quietly_added_to_channel_in_activity == on`.
- `generic_system_alert`: `generic_system_user_alert == on` (S1291453).
- `dm`: Inbox enabled and user pref `activity_dms_opt_out` is falsy (`SWeb.Vn`).
- `team_joiner_invite`: `inviter_joiner_first_dm_v2 == treatment`
  (`vtGv.wy`, S3281354 onward).

`message_reaction` is explicitly excluded after the membership filter. `mpdm`,
`bot_dm`, `thread_reply`, `example_user_alert`, `calendar_upcoming_event`,
`file_deleted`, and `code_review_requested` are not selected. Unknown categories
are not implicitly added. `activity_include_threads_in_badge_count` does **not**
gate `thread_v2` in this Inbox branch. Its source checkbox is hidden for Inbox
users (B26117936); applying it indiscriminately would undercount.

`fexP.wf` (S5112591) independently reads raw `dm || 0`. The Dock aggregate adds
this count when DMs are opted out of Activity, so changing that preference moves
DM contribution between components without dropping or double-counting it.

### Later and legacy branches

`fexP.Nq` (S5114754) returns zero unless Inbox is enabled,
`global_reminders_in_activity ?? true` is falsy, and Later is visible. In that
case it uses `pVFH.BK` (S3055782), exactly
`saved.counts.uncompletedOverdueCount`, not a count of loaded Later items.
The raw `client.counts.saved` key is `uncompleted_overdue_count`; B4165296 uses
`ijf8.A` (S4038745) to camel-case saved count keys. Existing captured
`tmp/sb/client.counts.json` confirms that key structurally.

Later visibility comes from `+3b5.b` (S5324982): parse user pref
`tab_rail_customization`, merge defaults (`wfsK.c`, Later defaults true), then
force applicable mandatory tabs visible. Missing/null final Later visibility
defaults true at the Nq call site. A complete consumer must use this effective
visibility policy, not merely inspect whether Later is the active native tab.

Without Inbox, cJ uses canonical `channel_badges` (`vuTZ.xY`, S3520610):
`thread_mentions + app_dms + channels`, plus `thread_unreads` when
`iHqW.Dn || activity_include_threads_in_badge_count` is truthy, plus selected
user-alert category counts. `iHqW.Dn` is `thread_badging_home_sidebar == on`
unless `dual_badging_in_home_sidebar == on` (S1202467). The Dock's legacy numeric
branch instead uses `JOlo.P` (B8206180), its DM-suppression-aware channel totals
and thread policy, with eligible Later count through `xbJ9.p` (B8205790).
These distinct branches cannot be replaced with one sum of visible sidebar
mention counts. Full mandatory-tab and legacy suppression projection remain
separate dependencies before claiming all-account Dock parity.

### Smallest next complete consumer slice

First retain and consume the revisioned category snapshot from counts and
`badge_counts_updated` on the Activity surface, with exact empty/absent handling
and stale HTTP protection. That is the current implementation boundary; no Dock
label should be inferred from it alone. Focused acceptance cases are replacement
removing an old category, explicit zero snapshot, absent map preservation,
unknown/reaction-only positive counts, and realtime arriving during HTTP load.

The next bounded display slice is the complete **Inbox-enabled** count projection
above: preserve only its needed experiment groups from the authenticated
`experiments.getByUser` assignments response (S1028162), hydrate the relevant
user/global prefs, retain canonical overdue counts per workspace, and project
Activity/DM/Dock components separately. Existing `slack/api/experiments.rs` shows
the authenticated assignments shape: `assignments[name].group`; a missing flag
in a successful complete response is off, while an unavailable response is not
proof of a gate value. Do not rely on boot extras to contain this separate read.
Tests should cover every category gate, DM opt-out conservation, thread-pref
non-effect in Inbox, reminders-in-Activity avoiding a second Later contribution,
and account/reconnect/count revision ownership. Only after these inputs are
complete should an app-wide numeric/dot adapter be added; badge preference UI
alone is not a complete consumer.

At this checkpoint the category snapshot/revision and desktop consumer changes
are in progress; their test gates have not yet completed. Root's passive
30-second live WebSocket observation saw 34 message events, 46 typing events and
3 pongs, with no `badge_counts_updated` event. Stock Activity displayed 45.
This proves an active observation window, not delivery or reconciliation of the
new badge event path, and the stock rail value is not a native aggregate oracle
without the same canonical map and gate snapshot. No live write was used to
manufacture an event. Further source exploration stops at the bounded consumer
handoff above; native and live count validation remain explicit follow-ups.


### Final bare-dev verification for bounce

`scripts/check-dock-attention-native.py` launched a disconnected hidden fixture,
read actual Focus as Available, and invoked both informational and critical
AppKit requests. Both returned accepted IDs and their exact cancellation
closures were invoked immediately. Receipt: `tmp/dock-attention-appkit-native.log`.
This proves native request/cancel invocation, not visible animation or real-message
delivery. The first runner attempt could not hide an unregistered bare process
through NSRunningApplication; the verified runner uses System Events with its
exact owned PID. It does not launch a signed bundle or change OS Focus settings.

Settings checks: `tmp/dock-attention-settings-native.log`; captures:
`tmp/agent-ui/notification-presentation{,-narrow}.png`. Live dev PID 75860 loaded
the existing bounce-off preference without changing it, matching stock Slack's
read-only reference. `tmp/agent-ui/live-dock-notifications.png` was inspected and
Settings closed. The client remained signed in and online in the private test
channel with destructive actions disabled. Badge aggregation, live preference
write confirmation, actual-message/visible-bounce checks, and OS Focus-on
suppression remain distinct gaps.


### Activity snapshot ingestion implemented

The typed category map, `badge_counts_updated` ingestion, revision-guarded
HTTP adoption and local zero reconciliation are implemented. Zero reconciliation
keeps an in-flight feed request alive, marks its baseline read and then replays
post-zero socket changes; history and newer unread entries survive. Missing/null
event maps do not trigger the stock empty-fallback quirk. Total-only local reads
invalidate categories until the next authoritative map.

Locked suites passed 945 desktop / 1,062 core tests (6 / 5 ignored); the normal
and narrow dev-native Activity fixtures passed badge transitions and exact reply
navigation, with PNGs inspected. See `tmp/activity-badge-{desktop-tests,core-tests,native}.log`.
Live bare dev PID 78857 booted online with compatibility count47. No real write
was used to provoke a count event. Exact category filtering, gated aggregation
and native Dock badge application remain unimplemented.

### Assignment hydration and optimistic read boundaries

Further captured-source review, 2026-10-13, resolves the inputs for the next
Inbox projection. S1028162 `fetchExperimentsData` makes the authenticated
`experiments.getByUser` read and returns `response.assignments`. Assignments are
objects keyed by experiment name, with a raw string `group`. The existing core
`slack/api/experiments.rs` already uses this request shape for List gates.
Preserve a complete optional snapshot for the Activity gates above: no successful
snapshot means unknown; a missing name in a successful complete snapshot means
not enabled. Preserve unfamiliar group strings rather than treating any
nonempty group as enabled. All listed gates compare exactly `on`, except
`inviter_joiner_first_dm_v2`, which compares exactly `treatment`.

S2401956 `+YqE.po` replaces the assignments store with the supplied truthy map;
`{}` therefore clears it. `+YqE.gf` is a separate explicit local override action
that merges supplied raw group strings. S2402782 `4OuQ` reads the stored group,
returning null for an absent assignment; its logging/nonlogging accessors do not
apply another policy layer. B6875800 `C8PH.bulkAddExperiments` dispatches the
replacement. S2085960 removes `experiment_assignments` from the boot-data store
because assignments belong in their own store. The previously referenced
`tmp/sb/client.userBoot.json` is not present in this checkout, so this trace does
not establish a current userBoot response field. No experiment-change realtime
contract was established in the inspected bundles. Do not infer a gate from an
`activity_v2` payload or invent a realtime assignment event.

Direct item reads and cursor reads have different count behavior. B9546140
`QDAL.m` (`markActivityItemAsRead`) optimistically dispatches `MdxY.WP` with the
item marked read, then calls the `HWsl` activity API wrapper. On failure it
restores the item and invalidates the feed. The WP reducer at S5089544 updates
matching item copies, including the root list; it does **not** decrement
`unreadActivityV2BadgeCounts`. Its other item-read actions (`wZ`, `it`) likewise
update local item/index state without changing that map. A direct native
mark-read must not manufacture a category decrement or discard the canonical
map solely because one feed item changed. A subsequent canonical counts/RTM
snapshot can replace it independently.

Separate channel and thread cursor reducers do make optimistic count changes
when the Inbox gate is enabled. `MdxY.rM` / `E` (S5086450 onward) and `MdxY.Q` /
`w` (S5087900 onward) collect indexed message IDs at or below the read cursor.
The helpers at S5065480 mark matching unread items across lists, but count
**only previously-unread root-list entries**, once per entry's `type`. Other
copies and normalized item maps do not contribute extra units. Thread matching
requires the same thread key and indexed message ID; channel matching excludes
thread items. Each category becomes `max(0, oldCount - matchingRootEntries)`.
The `YfTA.W` mapper (S4902019) sets `isUnread` directly from raw
`is_unread`; `thread_v2.unread_msg_count` is a separate display field, not an
alternative unread predicate. Its indexed `messageTs` is `min_unread_ts ||
latest_ts` (S4904323). This is not a subtraction by loaded child-message count
or an unconditional one-item decrement. Native cursor parity requires the same
root/index evidence;
without it, retain canonical counts rather than approximate.

The earlier total-only local-read invalidation checkpoint describes the prior
implementation, not the exact stock direct-item rule. The next consumer patch
must separate item-read state/revision from category snapshot validity. Focused
regressions should cover direct reads preserving the map, cursor reads counting
one root copy despite duplicate list copies, repeated reads not decrementing
again, and canonical map replacement arriving while a feed read is in flight.

### DM rail is a different consumer from Dock Activity-DM contribution

The actual `tab_rail_dms_button` path is B27888304 → `DmsTabContent`
(B27794602) → `badgeCounts.dms`. Its parent obtains that object from `/58m.X`
(B27879416 / B9444329), which uses `0SNR.e` (B9463286), **not** `fexP.wf`.
`fexP.wf` remains the raw V2 Activity-DM contribution used by the Dock aggregate.
Using it for the DM rail would conflate separate counts and units.

`0SNR.e` iterates `vuTZ.ty`, the canonical
`unreadCounts.countsPerChannel` map (S3517254). For channels whose metadata says
`is_im || is_mpim`, it sums truthy `unreadHighlightCnt`. It separately counts
conversations with a positive highlight; that second number is not the rail
badge. IM exclusions are search-system `USEARCH`, security `USLACKSECURITY`,
Slackbot `USLACKBOT`, system-notification `USLACK`, and a canonical member
classified as bot (`is_bot` or `is_slackbot`, including those special IDs).
References: `pS2E.df/Cx` S3691886, `HhPa.e` S3708188, `xQEw.G` S3712907,
`etUg.po` S3798818 onward, `pE90` S3812598. The captured personal-workspace
exception (`xfeP.R`, S1137002) is hardcoded false. MPIM members are not inspected
for this IM-only bot exclusion. Missing member metadata alone does not exclude
a recognized IM; missing channel metadata cannot establish IM/MPIM membership.

The rail selector has no additional self-user, deleted-user, open-state or
mute condition. Mute and membership matter upstream: the counts initializer
(B4470652) uses `fgSa.F` (S3930986), excluding muted channels, and admitting IMs
otherwise. MPIMs also require membership, non-archive, non-file and effective
inbox visibility (`y0HN.WK`, S3976688: Enterprise context or not from another
team; `tpNE.k` at S3788507 reads the explicit self Enterprise flag or
Enterprise ID). These operate on normalized channels: at S2277553,
`processNewChannelForUpserting` forces `is_member=true` for IM and MPIM, and
`setFromAnotherTeam` explicitly sets `fromAnotherTeam=false` for both types.
Consequently a native DM-only selector must not reject an MPIM because raw API
metadata omits `is_member`, nor apply a fabricated raw `fromAnotherTeam` gate.
Archive, file and mute checks remain relevant. `applyCountsToChannels`
(B44728xx) initializes DM/MPIM unread count from
`mention_count || Number(has_unreads)` and highlight count to zero; the
initializer's `fgSa.v` then uses that unread count as the DM highlight count.
A numeric zero with a forced `has_unreads` therefore contributes one. This
source path uses raw `mention_count`, not an invented `mention_count_display`
replacement. Later canonical unread-model recounts may refine that map.

Finally, `0SNR.e` moves the DM total into Home and returns zero for DMs when
`Zk56.Ty` says the onboarding rail tabs are hidden (B9605758). Native always
visible tabs do not yet implement that separate onboarding visibility policy.
The source distinguishes this gate from Activity Inbox and DM opt-out. A rail
projection must preserve those distinctions rather than infer it from the Dock
aggregate or from the number of loaded DM rows.

The native DM-rail correction is undergoing integration validation. Its
message-bag read state must outrank older channel metadata; a cleared bag must
not regain a badge from a stale `has_unreads` bit. Authoritative read count
snapshots with omitted numeric counts must also clear previous mentions.
Source tracing establishes the calculation and normalization, not live count
agreement: the observed stock rail total and an older native conversation count
are different consumers, and no final paired live result is claimed here.
Root-owned core/desktop suites, normal/narrow fixtures and a same-account live
comparison remain the gates for this correction.
