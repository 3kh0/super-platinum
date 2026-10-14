# Channel notification controls

Status: mode, device, mobile override, thread, broadcast, and badge controls are implemented in Channel Details, sharing the existing mute operation and recovery lifecycle. Verification results are recorded below. Preference persistence and native UI checks are separate from OS/mobile notification delivery.

## Local evidence

Offsets below are zero-based UTF-8 byte offsets in these exact local captured assets (the prefixes through these offsets are ASCII):

- **B:** `tmp/sb/js/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`
- **S:** `tmp/sb/js/gantry-v2-shared.full-capture.0b81595876785d7a.min.js`

| Offset | Module/helper | Evidence |
| --- | --- | --- |
| B 2173743 | `pSSe`, exports `qV`, `g2`, `_L` | Mode presets, typed pref array, per-pref/top-level synchronization |
| B 14608380 | `BtIR.ChannelNotificationPrefMenu` | Primary choices: All new posts, Just mentions, Mute; current effective desktop/mobile settings |
| B 14611365 | `BtIR` `el`/`eo`/`ec`/`ed` callbacks | Normalize patch/sync, add `muted:false` when selecting a mode on a muted channel |
| B 14176352 | `R7xz.ChannelNotificationsPrefsModal` | Advanced device switches, mobile overrides, follow-thread/broadcast/badge controls |
| B 14179732 | `R7xz` `ej` | Single-pref changes through `uhjK.My` |
| B 14183142 | `R7xz` `eF` | Mobile off writes `nothing`; on restores current desktop mode, falling back to `mentions_dms` |
| B 7588504 | `uhjK.My` / `Kp` | Single and multi-channel preference thunks and source reason strings |
| S 1443862 | `Nu4c` | Preferences API adapters |
| S 1447898 | `Nu4c.Sq` | Single-pref `users.prefs.setNotifications` arguments |
| S 1448377 | `Nu4c.z0` | Multi-pref `users.prefs.setNotifications` arguments |
| S 3858578 | `Fpyc.getCalculatedDesktopPushNotificationsEnabledSetting` | Channel override, otherwise global push-enabled value |
| S 3858779 | `Fpyc.getCalculatedNotifySetting` | Channel mode/default lookup; legacy `nothing` normalizes to `mentions_dms` |
| B 4844240 / 4849183 / 4861239 | Legacy `ykSe.ChannelNotificationPrefs`, Save, dispatch mapper | Older staged form with desktop/mobile `everything`, `mentions_dms`, `nothing` radios; mobile synchronization on Save |

The older `ykSe` form and newer `BtIR`/`R7xz` surfaces coexist in the capture. Do not treat the older desktop Nothing radio as the sole current contract: the newer primary menu offers mute, while Advanced exposes desktop push separately from the message filter.

## Primary mode changes

`pSSe.qV` + `g2` produce these presets:

- **All new posts:** `desktop="everything"`, `follow_all_threads="false"`, `badge_all_unreads="false"`; leave suppression unchanged.
- **Just mentions:** `desktop="mentions_dms"`, `follow_all_threads="false"`, `suppress_at_channel="false"`; leave badge preference unchanged.
- Selecting either while muted also appends `muted="false"`.
- Desktop sync is the inverse of `hasDifferentMobileSettings`, calculated by comparing the effective mobile and desktop modes. Preserve an existing different mobile mode.
- `_L` sets top-level `sync=true` if a desktop pref has `sync:true`, and adds explicit `sync:false` to other prefs lacking a sync field. Do not accidentally synchronize boolean preferences.

A normalized form example for All new posts with linked mobile settings:

```text
POST users.prefs.setNotifications
global=false
channel_ids=C_SYNTHETIC
sync=true
prefs=[{"name":"desktop","value":"everything","sync":true},
       {"name":"follow_all_threads","value":"false","sync":false},
       {"name":"badge_all_unreads","value":"false","sync":false}]
_x_reason=prefs-store/setMultiChannelNotificationOverride
```

`Nu4c.z0` receives a channel-ID array before the existing request serializer; the single-channel form is a raw ID in `channel_ids`, as separately observed for existing mute. This example uses a synthetic placeholder, not an admissible production ID. Prepared-request regressions cover the exact mode combination; the isolated private-channel production API verification below also confirmed linked and independent mobile readbacks.

## Advanced settings and single-pref wire

`R7xz` dispatches each Advanced change independently through `uhjK.My` → `Nu4c.Sq`. It is not an atomic multi-control Save form.

```text
POST users.prefs.setNotifications
name=desktop_push_enabled
value=false
channel_id=C_SYNTHETIC
global=false
sync=false
_x_reason=prefs-store/setChannelNotificationOverride
```

- **Desktop notifications:** boolean `desktop_push_enabled`. Changing it does not replace the desktop message filter or mute the channel.
- **Mobile notifications:** `mobile="nothing"` turns off; enabling uses current normalized desktop mode or `mentions_dms`. Do not infer a separate mobile-enabled boolean.
- **Advanced mobile override:** `mobile="everything"` or `mobile="mentions_dms"`; hidden/disabled when mobile notifications are off. The labels can describe matching desktop settings without changing wire semantics.
- **Follow every thread:** boolean `follow_all_threads`.
- **Broadcast mentions:** checkbox display is inverse of `suppress_at_channel`; retain that inversion.
- **Badge every new message:** boolean `badge_all_unreads`, shown for All new posts.

The desktop permission/OS speedbump is a separate surface. A successful preference write does not prove a desktop banner can be delivered. Legacy `nothing` normalization preserves Activity/filter semantics and must not be mistaken for push enablement.

## Native implementation boundary

Current code:

- `crates/super-platinum-core/src/slack/api/notifications.rs`: complete canonical `NotificationPreferences`, guarded `users.prefs.get(all_notifications_prefs)`.
- `.../notifications/temporary.rs`: `ChannelMuteChange::{Permanent,TemporaryUntil}` and narrow mute requests.
- `.../notifications/{settings,intent}.rs`: source-derived effective modes, typed actions, exact key patches, intent revalidation, and canonical confirmation.
- `crates/super-platinum-core/src/state/notifications.rs`: raw global/channel preservation, effective desktop push/filter consumption, realtime preference adoption.
- `src/desktop/channel_mute.rs` and `channel_mute/{model,network}.rs`: stable-owner receipts, fresh read admission, one-shot dispatch, shared authenticated endpoint cooldown, retained uncertainty. The settings submodule reuses that lifecycle and adopts only the confirmed intent's affected keys.
- `src/desktop/view/channel_notifications.rs`: compact Channel Details controls; every activation is a separately guarded change.
- `src/desktop/view/channel_details.rs::MuteControl`: mute actions and the shared pending/recovery status above the settings editor.

Typed mode patches and single-pref device changes require exact changed-key canonical readback before adoption. Mute and settings share owner/cooldown/operation machinery so their mutations cannot race. Never serialize the entire preference snapshot as a mutation. Unknown/concurrent global and channel keys must survive. Partial or ambiguous outcomes remain locked for read-only checking; do not replay.

## Required verification

1. Exact prepared requests for both primary modes, linked/different mobile settings, muted transitions, per-pref sync, and independent Advanced toggles.
2. Missing channel overrides/global fallback and malformed/unknown mode handling; device-off changes preserve the Activity message filter.
3. Cancel/no mutation where a local draft is used; duplicate activation, owner/visit/auth rotation, concurrent realtime preference changes, endpoint Retry-After, and exact changed-key readback after an ambiguous response.
4. Normal/narrow native fixtures with readable compact controls and keyboard/focus behavior; assert unrelated preference keys remain unchanged.
5. Authorized live preference readback is complete for the isolated private-channel API sequence below. Live native-click dispatch and actual desktop/mobile delivery remain separate, unverified boundaries.

## Read-only native Slack reference (2026-10-13)

Opened the real client in private `#echotestchamber` using its existing CDP
connection, then Edit notifications → More options → Advanced settings. The
primary menu showed All new posts, Just mentions, and Mute and hide. Advanced
showed separate desktop/mobile switches, a mobile override disclosure, Follow
every thread, and Get broadcast messages. No Save button: individual controls
apply independently. No preference was changed; the original conversation route
was restored. The modal PNG was captured and inspected at
`tmp/agent-ui/slack-channel-notifications-reference.png`.

An intentional source quirk: the calculated mobile mode normalizes a raw
`nothing` override to `mentions_dms`. With desktop Mentions, the primary mode
menu therefore considers that mobile mode linked; changing to Everything with
sync enabled can turn mobile notifications back on. Advanced's enabled switch
uses the raw channel override, not the global calculated mode. Preserve these
separate projections instead of assuming that a raw mobile-off setting always
means an independent override.


## Production API verification and cleanup (2026-10-13)

`tmp/channel-notifications-live-actions.log` records a successful gated run in a
new, uniquely named private test channel. The probe verified the new channel's
private status, self membership, and creator from fresh canonical metadata before
any preference mutation. It used the production `ChannelNotificationIntent`,
`set_channel_notification`, and fresh `users.prefs.get` canonical confirmation
for all 16 actions:

- Everything and Mentions; a second Mentions selection preserving different
  mobile settings; Everything again before badge controls.
- Desktop push off/on, mobile off/on, and both mobile override values.
- Follow every thread off/on, broadcast suppression off/on, and badge-all off/on.

Every action confirmed its requested keys and equality of all unrelated global
and other-channel preferences. Create/archive used existing request builders
through a bounded probe-only sender with retries and redirects disabled. The
probe did not post messages or change global preferences. Its temporary source
was removed after verification.

Cleanup reset the new channel's overrides, independently confirmed the channel
entry was absent while unrelated preferences stayed equal, then archived the
same owned channel and confirmed `is_archived` from fresh metadata. All action,
reset, archive, and final completion receipts passed. The archived channel is the
only retained artifact; no real channel IDs or private preference payloads are
included in this document.

This proves the production API request and canonical reconciliation paths. It
does **not** prove live dispatch through native control clicks or OS/mobile banner
delivery. Disconnected native fixtures establish a separate UI boundary.

## Whole-channel reset contract

Captured B 26201872 (`resetChannelNotificationOverrideAndCallApi`) calls
`Nu4c.Sq` with `name:"reset"`, `value:"default"`, and the single channel ID:

```text
POST users.prefs.setNotifications
channel_id=C_SYNTHETIC
name=reset
value=default
global=false
sync=false
_x_reason=prefs-store/resetChannelNotificationOverride
```

`BX4c.Wh` (B 75927xx) optimistically removes the **entire** channel override
object; the thunk restores that object on rejection. The isolated live run
confirmed the server's resulting absent channel entry. There is no evidenced
selective per-key deletion contract. Resetting an existing channel with mute or
unknown override fields is therefore not an exact restoration strategy for that
baseline. The separate global `default_all` reset is outside this feature and
was not called.

## Preference versus OS permission

The stock Advanced desktop switch does not display the preference alone.
`R7xz` reads effective desktop push through `Fpyc.u0`, then combines it with
browser notification permission and its asynchronous macOS-disabled result:
`!macOSDisabled && browserPermissionAllowed && effectiveDesktopPush`.
The live reference's unchecked switch alongside canonical global push enabled
was consistent with this additional permission gate; it was not evidence that
an absent channel override means false. Native settings retain the canonical
preference and disclose the separate OS permission boundary. A persisted true
preference does not guarantee a delivered banner.

Core synchronization also preserves JavaScript's missing-versus-null distinction:
a defined global mobile `null` can differ from desktop and prevent synchronization,
whereas an absent mobile value does not. A null channel override falls back to
globals. Focused regressions cover these cases and the raw-mobile-off quirk above.
