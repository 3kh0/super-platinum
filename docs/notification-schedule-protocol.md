# Recurring notification schedule: protocol and verification

Status: implemented; normal/narrow native interaction fixtures, canonical live
reads and the stock route are verified. **No recurring schedule write has been live verified.**
This records the captured recurring DND schedule, separate from temporary snooze
and per-channel notification settings. No profile, schedule, or global preference
was changed while researching it.

## Current native boundary

- `src/desktop/view/settings.rs` exposes Appearance, Notifications and Storage.
  Notifications mounts the recurring working-hours editor; opening it performs
  canonical reads without changing preferences.
- `src/desktop/overlays.rs` exposes temporary 30-minute, one-hour, two-hour and
  until-tomorrow pauses. `bootstrap/self_menu.rs` uses `dnd.setSnooze` and
  `dnd.endSnooze`; these do not edit recurring schedules.
- Existing until-tomorrow self-menu calculation uses a cached user UTC offset and
  fixed 08:00. That is a separate existing behavior, not a reference algorithm for
  recurring schedules or the newer channel temporary mute.
- `core/src/state/notifications.rs` (under `crates/super-platinum-core/src/`)
  now hydrates the complete recurring schedule, including `dnd_enabled`,
  `dnd_start_hour`, and `dnd_after_<day>`. Raw representations remain intact;
  older cache entries retain migration behavior and unknown prefs are preserved.
- `core/src/slack/api/notifications/temporary.rs` reads tomorrow's schedule for
  temporary channel mute. Keep its behavior and cache migration tests intact.
- `src/desktop/channel_mute/` provides useful stable-owner, one-shot, cooldown,
  uncertainty and canonical-readback patterns. Schedule writes use distinct
  transport admission for the exact named keys and one-shot `users.prefs.set`
  requests, separate from per-channel notification writes. Shared preference
  cooldown ownership survives account navigation.

## Captured source map

Offsets below are UTF-8 byte offsets, verified equal to character offsets in these
specific local files. They are public application source, not private data.

- **B**: `tmp/sb/js/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`
- **S**: `tmp/sb/js/gantry-v2-shared.full-capture.0b81595876785d7a.min.js`

| Source | Behavior |
| --- | --- |
| B 26097662 `DndSchedule` | Every day / Weekdays / Custom; allowed-notification hours, immediate callbacks |
| B 26095346 `DndCustomPreferences` | Seven ordered days, None, disabled/all-day/partial projection, six-None restriction |
| B 26099920 `DndPreferences` | Exact patch construction and disabled-schedule preview |
| B 26100261 `setDndPrefs` | Every action adds `dnd_enabled: true` |
| B 26100561 / 26100992 | Global/custom allowed-start changes can also move allowed-end |
| B 2303140 `SN+s` | Exact preference keys and enum values |
| B 15064366 `JiU7` | Start/end selector order, interval and wrapping rules, retained irregular times |
| B 2302834 `kKOK` | Conflict check and next half-hour fallback |
| B 1042212 `qn32` | 30-minute time options |
| S 5326205 `N0Kh`, export `Hj` at 5329125 | Multi-user-pref thunk, optimistic update and rollback in stock |
| S 1446781 `Nu4c` multi-pref fetcher | `users.prefs.set`, JSON object in form `prefs`, fixed reason |
| S 1445324 `getUserPrefsByApi` | `users.prefs.get`, comma-separated requested names, canonical response prefs |
| B 26104254 `NotificationSchedulePref` | Preferences route and `availability_prefs_section` feature-dependent relocation |

## Exact types, presentation and changed-key patches

The displayed hours are when notifications are **allowed**. Stored names are
opposite-facing quiet-hour bounds: displayed Start maps to `dnd_end_hour` or
`dnd_before_<day>`; displayed End maps to `dnd_start_hour` or `dnd_after_<day>`.
Never swap them based on their names.

- `dnd_days`: `every_day`, `weekdays`, `custom`.
- `dnd_enabled`: boolean; every captured schedule action sets it to `true`.
- Global times: `dnd_start_hour`, `dnd_end_hour`, strings in local wall-clock
  `HH:mm` form. The write contains no UTC timestamp or timezone conversion.
- Custom day names: lowercase Monday through Sunday. Each has
  `dnd_enabled_<day>` = `all_day`, `disabled`, or `partial`, plus
  `dnd_before_<day>` and `dnd_after_<day>` time strings.
- `all_day` means DND all day: UI displays **None**, hiding the end selector.
  `disabled` means no DND that day: UI previews allowed `00:00`–`23:59`.
  When six other days are already `all_day`, stock does not offer None on the
  remaining day. Existing None days can still be changed.
- A globally disabled schedule previews Every day, `00:00`–`23:59`; it does not
  write defaults merely by opening. Active missing/null/falsy time values render
  empty `--:--` placeholders, not invented 09:00/17:00 values.
- Options normally use half-hours; final `23:59` is labeled Midnight. Start
  excludes that final value. Canonical non-interval times (e.g. `08:15`) are
  inserted so opening does not round or lose them. `time24` controls labels only.
- Global allowed-end options may wrap; custom options do not. When a changed
  allowed-start is greater than or equal to the existing allowed-end (after
  five-character zero-padding), stock also advances allowed-end to the next
  strictly later half-hour, or `23:59` when none exists (`kKOK`). Preserve this
  exact side effect instead of applying a generic start-before-end validator.

Examples of the **JSON object serialized into form field `prefs`**:

```json
{"dnd_days":"weekdays","dnd_enabled":true}
```

```json
{"dnd_end_hour":"09:00","dnd_enabled":true}
```

```json
{"dnd_before_monday":"09:00","dnd_enabled_monday":"partial","dnd_enabled":true}
```

```json
{"dnd_enabled_sunday":"all_day","dnd_enabled":true}
```

The time example assumes no adjustment of its paired bound. If adjustment is
needed, include only the evidenced paired key as well. Changing a custom time
always sets that day's enabled mode to `partial`. None changes only the day's
mode plus `dnd_enabled`; retained times must not be erased.

`Nu4c` sends `users.prefs.set` with `prefs=JSON.stringify(patch)` and reason
`prefs-api/setMultipleUserPrefsByApi`. This is a JSON **object**, not the
`[{name,value}]` array used by channel mode changes. Booleans remain JSON
booleans. No channel ID, title, or entire preference snapshot belongs in this
write. The captured UI applies each valid selection immediately; no separate
Save/confirmation dialog is present in this component. Opening, previewing and
closing perform no writes.

## Safety and evidence still needed

Use a new typed renderer-neutral schedule intent and fresh named-pref read. Pin
owner/account/team/user/credentials, relevant canonical schedule fields, and the
chosen patch. One explicit selection sends once; do not automatically retry an
ambiguous write. Retain its ownership/lock across navigation, offer a read-only
status check, and settle only when all expected patched keys are explicitly
present and canonical. Preserve unknown/user/global/channel preferences and
concurrent unrelated changes. Coordinate shared `users.prefs.get` cooldown and
any mutations touching the same schedule; preserve temporary snooze as a separate
operation rather than clearing it implicitly.

The read-only live probe (`tmp/schedule-read-probe.log`) verified a flat
`{ok:true,prefs:{...}}` named-pref response, including canonical unpadded `8:00`
times. The requested `tz` preference was absent while `set_tz_automatically` was
true; a separate canonical `users.info` read supplied `America/New_York`,
`Eastern Daylight Time`, and the current offset. Absence is not an empty timezone
or permission to substitute the operating system's timezone. Boot cache remains
preview, not a write receipt. Preserve absent/null distinctions and do not invent
schedule defaults from temporary snooze or channel-mute behavior.

`TimezonePref` (B 26069080) reads `tz` and `set_tz_automatically`; enabling automatic
timezone writes the detected `O35q.zoneId` to `tz`. `maybeUpdateTimezoneSideEffect`
(B 6317857) similarly updates it when automatic detection is enabled. The generic
named-pref getter (S 1445324) accepts these names through its comma-separated
`prefs` argument. Schedule setters send the selected wall-clock strings unchanged.
Slack's [timezone documentation](https://slack.com/help/articles/219889247-Manage-your-time-zone-preferences)
confirms that changing the Slack timezone keeps notification schedules in local
time. Capture the canonical user timezone as selection context; rejecting a
changed timezone before dispatch is a native stale-intent safeguard, not a claim
that stock performs the same preflight. Do not convert schedule hours using the
current UTC offset. Server behavior at DST gaps/overlaps is not verified here.

The captured preferences route hides this schedule in Notifications when
`availability_prefs_section == "on"` and shows only Reminders there. The flag
adds the Availability tab (B 26290811). `AvailabilitySection` (B 26166861) renders
the same connected `DndPreferences` as **Working hours** (B 26170844), with
`excludeRemindersPreferences:true`. It changes placement, not the write protocol.
Root inspected the live stock Working hours screenshot
(`tmp/agent-ui/slack-working-hours-reference.png`), closed Preferences without
changes, and preserved the prior conversation route.

No schedule-specific permission/plan/offline disable is passed by these schedule
components; `BaseDndTimeSelect` has an optional `isDisabled` prop which these
callers do not set. This absence is not proof that server errors cannot occur.
Slack's [schedule documentation](https://slack.com/help/articles/214908388-Pause-your-Slack-notifications)
states that the feature is available to all members on all plans and that a
personal schedule overrides workspace/org defaults. Do not interpret an admin
default as a write prohibition. VIP exceptions and automatic profile statuses
remain separate preferences and are not modified by schedule edits.
Reminder notification time, mobile idle delay, email preferences, notification
sounds and global message modes remain separate surfaces; do not silently bundle
their writes into this slice. No trusted OS/native schedule interaction or real
schedule mutation has been verified yet.

## Meaningful implementation gates

1. Core oracle tests for each changed-key patch, reversed stored/displayed bound
   mapping, next-half-hour/end-of-day adjustment, custom None/partial/disabled,
   six-None constraint, irregular canonical times, absent/null preview, and
   unknown-key preservation. Test exact endpoint/reason/object/boolean wire.
2. Fresh-preflight tests: changed paired bound, policy/owner/session rotation,
   stale selection, duplicate activation, 429 non-shortening cooldown, failed or
   partial canonical read, unknown outcome with no replay, concurrent unrelated
   prefs, late completion after reopening.
3. Normal/narrow disconnected native fixture: every/custom/weekday paths,
   seven-day scroll and fixed controls, keyboard focus, no-write open/close,
   pending locks, exact prepared requests, accepted and uncertain readback.
   Inspect actual PNGs. Default runner uses the dev executable.
4. Any live write requires an explicitly authorized isolated schedule baseline
   and exact restoration/readback; private-channel write permission does not
   authorize workspace-wide schedule changes.


## Native verification

Both dev-executable fixtures passed after correcting select initialization,
three-tab layout and inherited dialog-top bounds. Root inspected both PNGs.
`tmp/notification-schedule-native.log` records the final pass; fixtures cover
eight exact changed-key writes, ten reads, all seven days, None, pending focus,
uncertain-result check without replay and unknown-pref preservation. The live
read-only API returned Every day, 08:00–20:00, America/New_York, matching stock.
A real schedule write and DST transition remain unverified.


The rebuilt live dev editor was also opened read-only through typed Settings
navigation. `tmp/agent-ui/live-working-hours.png` was inspected and shows the
same Every day / 08:00–20:00 / America/New_York values. The dialog was closed
after capture. This proves live hydration and rendering, not schedule mutation.
