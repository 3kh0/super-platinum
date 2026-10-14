# Global notification defaults and keywords

Status: implemented, unit-tested, and verified through normal/narrow native
fixtures and a live read-only rendering check. Live global preference writes
remain unverified. Channel settings and recurring schedules have separate
implementations and verification ledgers. No live preferences were changed.

The native keyword editor deliberately uses explicit Save/Discard instead of
Slack's debounce/unmount flush. Its seed synchronization preserves newer native
input and composition while allowing an explicit discard to reset the field.
Six JavaScript regressions exercise that helper, including replacement nodes.
Both native fixture runs passed nine simulated writes and twelve reads each;
the resulting PNGs were inspected. Evidence: `tmp/global-notifications-native.log`,
`tmp/global-keywords-tests.log`, and `tmp/agent-ui/global-notifications*.png`.
The bare dev executable also loaded canonical account settings successfully;
`tmp/agent-ui/live-global-notifications.png` was inspected without editing them.

## Source provenance

Offsets are in these exact captured official client assets:

- **B:** `tmp/sb/js/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`
- **S:** `tmp/sb/js/gantry-v2-shared.full-capture.0b81595876785d7a.min.js`

| Source | Contract |
| --- | --- |
| B 261066xx | `setNotificationsMultiGlobalPrefsByApi`: optimistic global key updates, API call, original-value rollback |
| B 261077xx–261096xx | `NotificationsGlobalPlatformSelect`: Everything/Mentions and explicit paired mobile write |
| B 261096xx | `NotificationsGlobalPrefCheckbox`: raw global value, nullish UI default, boolean write |
| B 261099xx–261110xx | `NotificationsKeywordsInput`: mounted text, 100-entry validation, one-second debounce, flush on unmount |
| B 658727 | `Sfh9.validateCSV`: split on `/\s*,\s*/`, compare array length with maximum |
| B 26114115–261184xx | `NotificationsChannelDefaults`: Messaging defaults, device toggles, mobile link state, thread replies |
| B 75936xx | `giaO.S`: single global-pref thunk; rejects nullish values, preserves original for rollback |
| B 75920xx | `BX4c.bL`: updates `all_notifications_prefs.global` locally; preserves channels |
| S 14475xx | `Nu4c.vQ`: single global `users.prefs.setNotifications` adapter |
| S 14489xx | `Nu4c._V`: multi-global `users.prefs.setNotifications` adapter |
| S 1445324 | `Nu4c.jH`: named `users.prefs.get` request |
| S 2960997 | `zN7l`: raw global/channel getters from `all_notifications_prefs` |
| S 38574xx–38592xx | `Fpyc`: calculated global defaults, DM fallback, legacy Nothing normalization, channel overrides |

The offsets ending in `xx` identify a bounded portion of the named module, not a
claim that a function begins at an exact byte there.

## Scope and canonical read

Read `users.prefs.get` with `prefs=all_notifications_prefs`, using the selected
workspace session and normal authenticated request context. Existing
`fetch_notification_preferences` parses the complete object from
`response.prefs.all_notifications_prefs` (object or encoded JSON string).
Global defaults live under its **`global` object**; conversation exceptions live
under `channels[channelId]`.

Here `global=true` means a default in this authenticated user's notification
preferences, as opposed to a channel override. It does not mean an organization
admin setting, every signed-in account, or permission to iterate all workspaces.
The request has no channel ID and no extra account/workspace broadcast parameter.
Native ownership must remain account + authenticated workspace/team + self user
+ credential/transport scope. In an Enterprise Grid session whose ID represents
an organization, the capture alone does not establish propagation to every
underlying workspace. A future live verification must record the actual selected
session scope; do not infer or promise cross-workspace propagation.

Opening the editor must preserve absent keys and explicit null. No defaults are
written merely by showing a fallback. Unknown global and channel keys are never
serialized back as a whole snapshot.

## Keys and projection

| Key in `all_notifications_prefs.global` | Type and surface |
| --- | --- |
| `global_desktop` | String `everything` or `mentions_dms` in the current Messaging defaults selector. Legacy `nothing` is read as Mentions by `Fpyc`. |
| `global_mobile` | String `everything`, `mentions_dms`, or `nothing`; the latter disables mobile push. |
| `global_desktop_push_enabled` | Boolean device preference, independent of `global_desktop` message filter. |
| `global_keywords` | Comma-separated string. Empty string clears the configured keywords. |
| `threads_everything` | Boolean “Thread replies” checkbox in this global defaults component. |
| `global_mpdm_desktop`, `global_mpdm_mobile` | Existing separate DM/group-DM default fields consumed by `Fpyc`; this captured current component does not write them. Preserve them. |

The adjacent Activity, reminders, VIP, huddle, idle-delay, sound and email
preferences are separate controls, with some using `users.prefs.set` instead.
They must not be silently included in these notification-default patches.

Device and mode distinctions:

- The global desktop checkbox renders `!!(rawValue ?? true)`. This means null and
  missing display enabled here. `Fpyc`'s calculated delivery fallback is slightly
  different: `rawValue === true || rawValue === undefined`, so explicit null is
  false there. Preserve the difference rather than reuse one projection blindly.
- Global mobile checkbox is enabled whenever raw `global_mobile !== "nothing"`.
  Turning it off writes `nothing`; turning it on writes raw `global_desktop` if
  truthy, otherwise `mentions_dms`.
- The current global mode selector uses calculated `Fpyc.YY`, so legacy `nothing`
  displays as Mentions. Its available new choices are Everything and Mentions,
  not a new desktop Nothing option. For these global keys, `Fpyc.YY` otherwise
  returns the raw value: missing, null and empty string do **not** become
  Mentions. The selector's `options.find` then has no selected option. Keep an
  explicit placeholder rather than borrowing the channel-mode fallback.
- Channel explicit overrides still win. A global mode change must not erase
  them. DM/group-DM calculated fallback uses the separate `global_mpdm_*` keys;
  an absent/falsy DM-specific value normally defaults to Everything. Do not
  describe changing `global_desktop` as forcing all DMs to the selected mode.
- Preference persistence does not establish OS permission, active pause state,
  device connectivity, banner display, or mobile delivery.

## Mobile linking is mounted UI state

`NotificationsChannelDefaults` initializes local `d` with strict raw equality
`global_desktop === global_mobile`. It does not continuously derive that local
choice after every store update. “Set up mobile overrides” displays `!d` and is
disabled when mobile is off.

- Selecting a desktop mode writes `global_desktop` and **also `global_mobile`**
  only when mobile is enabled and local `d` is true.
- Turning on separate overrides changes local `d` only; it does not write a
  preference just to reveal the selector.
- Turning separate overrides off sets local `d` true and writes
  `global_mobile = raw global_desktop` immediately.
- Turning mobile off also resets local `d` true. Turning it back on restores
  desktop or the captured Mentions fallback.
- Selecting an independent mobile mode writes only `global_mobile`.

This is not the channel-control `sync` algorithm. In particular, do not replace
this mounted state with normalized desktop/mobile equality or silently apply
channel-style `sync=true`. Preserve strict absent/null distinctions when
initializing the link state; reject an unrepresentable outgoing value instead
of inventing a mode. A native controller needs generation-bound mounted link
state and a fresh dependency check when a pending selection reaches dispatch.

The captured dependency boundaries are concrete: desktop-mode selection consults
raw mobile-on state and mounted link state to decide whether its patch includes
mobile. Turning mobile on and relinking consult raw desktop; turning mobile off
does not need a desktop fallback and also sets mounted linking on. An independent
mobile selection writes only mobile. Device/thread toggles and keyword edits
must not acquire channel preset side effects. Canonical refresh may replace
preference values but must not silently recompute a surviving mounted link choice.
The nullish guard in `giaO` (B 7593569) rejects missing/null relink values, while
an empty string is a non-nullish scalar and is passed through by stock.

## Exact writes

Global mode selector, linked mobile example:

```text
POST users.prefs.setNotifications
global=true
prefs=[{"name":"global_desktop","value":"everything"},
       {"name":"global_mobile","value":"everything"}]
_x_reason=NotificationsChannelDefaults
```

`Nu4c._V` sends **no top-level sync argument** for this multi-global call. The
array explicitly names every affected global key. Independent mobile mode uses
the same adapter with a one-element array. There is no `channel_id` or
`channel_ids`, and no `follow_all_threads`/badge/suppression preset side effect.

Single desktop device toggle:

```text
POST users.prefs.setNotifications
name=global_desktop_push_enabled
value=false
global=true
sync=false
_x_reason=NotificationsGlobalPrefCheckbox
```

The single adapter carries a scalar form value. Boolean values become form
strings; canonical values remain booleans. The mobile on/off checkbox uses this
same adapter/reason with `name=global_mobile`, and the relink action uses reason
`NotificationsChannelDefaults`. Thread replies uses
`name=threads_everything` and the global checkbox reason.

Keyword update:

```text
POST users.prefs.setNotifications
name=global_keywords
value=<captured normalized keyword string; may be empty>
global=true
sync=false
_x_reason=NotificationsKeywordsInput
```

These are **not** ordinary `users.prefs.set` writes. Do not confuse them with
recurring schedules or the separate user-level VIP/huddle toggles.

## Exact keyword editing behavior

`NotificationsKeywordsInput` seeds local text once with `raw ?? ""`, then
`addKeywordWhitespace` replaces every comma with comma + one ASCII space.
Before writing, `stripKeywordWhitespace` replaces each literal `", "` with
`","`. It does **not** trim every token, collapse arbitrary whitespace, sort,
case-fold, remove duplicates, or implement quoted CSV.

Validation computes `csv.split(/\s*,\s*/).length <= 100`. Empty and trailing
entries count: empty input has one entry and is permitted; a trailing comma
adds an entry. There is no source maximum string length or individual keyword
length in this validator. No invented length cap should masquerade as a Slack
rule.

The concrete validator function begins at B 659223. Because this separator
contains exactly one literal comma and no capturing groups, its entry count is
also exactly the literal comma count plus one; Unicode whitespace changes the
split contents but not that count. This equivalence does not permit trimming or
normalizing the stored keyword text beyond the separate literal `", "` rule.

Valid edits schedule a one-second debounce. Unmount flushes pending work. An
invalid edit does not invoke the debounced writer, but it also does not cancel
an already queued valid edit. That subtlety needs an explicit native decision
and tests: never let an old debounced value cross account/workspace/visit
boundaries. If native uses explicit Save instead, identify that interaction
choice rather than claim identical stock autosave behavior.

Example: display input `alpha,  beta` serializes as `alpha, beta`, retaining one
space. A tab/newline after a comma is not stripped by the wire normalizer. The
storage string must remain separate from notification matching normalization.

## Existing core consumer and implementation invariants

`core/src/state/notifications.rs` retains the full global map through
`NotificationConfig`, while the complete state is workspace-owned and cached.
Its delivery path checks channel overrides before globals, applies desktop
push separately, consumes `threads_everything`, and searches `global_keywords`
for eligible non-thread, unmuted messages. `matches_keywords` currently splits
commas, trims empty terms, folds case/smart quotes, checks word boundaries and
avoids matching inside Slack entities/mentions. Those are delivery rules, not
authority to normalize the stored keyword preference when editing it.

The next implementation should provide:

1. A typed global snapshot/projection and immutable action intent. Pin owner,
   source baseline, raw modes and mounted link state before the one-shot write.
2. Exact scalar versus multi-pref request builders, narrowly admitting only these
   global keys. Reuse no-hold/no-retry transport and the shared authenticated
   preferences cooldown; never retry uncertain writes automatically.
3. Read-only reconciliation requiring explicit canonical changed keys, preserving
   channel overrides and unrelated global/user preferences. Account/navigation
   rotation retains uncertain receipts without adopting old data into a new
   scope.
4. Keyword draft/debounce or explicit-Save semantics with cancellation, invalid
   draft behavior, raw-string preservation and duplicate-send tests. Include
   empty, trailing/repeated commas, 100/101 entries, Unicode whitespace and
   delayed old-scope callbacks.
5. Linked/separate/off mobile tests covering absent/null/legacy modes, relink
   writes and local-only disclosure changes. Native normal/narrow fixtures must
   independently prove controls, focus and no-write opening/closing.

Live global writes need explicit authorization and an exact recoverable baseline.
The prior private-channel test authorization does not cover workspace-wide
notification-default or keyword changes. No global reset/clear mechanism for
restoring absent individual keys has been established here; do not reuse the
whole-channel reset contract.


## Read-only stock UI confirmation

The current signed-in client exposes these controls under Preferences >
Notifications > Messaging defaults. Root inspected
`tmp/agent-ui/slack-global-notifications-reference.png`; desktop/mobile toggles,
Mentions and direct messages, thread replies and mobile overrides match the
source map. Channel keywords appear below Activity settings. The dialog was
closed and the existing channel route preserved without preference changes.
