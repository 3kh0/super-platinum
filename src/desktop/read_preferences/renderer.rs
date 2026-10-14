//! Converts native attention and exact DOM observations into read policy inputs.
use dioxus::prelude::*;
use super_platinum_core::state::{ReadObservation, cmp_ts};

use super::presentation::{Conversation, Observation};
use crate::state::ShellState;

pub(crate) fn conversation(shell: &ShellState) -> Option<Conversation> {
    Some(Conversation {
        epoch: shell.core.account_epoch,
        account: shell.core.active_account.clone()?,
        team: shell.core.active_team.clone()?,
        channel: shell.core.active_channel.clone()?,
    })
}

/// A changed cursor, preference, loaded history, or manual hold invalidates old
/// DOM attention even before Dioxus commits its next set of data attributes.
pub(crate) fn key(shell: &ShellState) -> String {
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|id| shell.core.workspaces.get(id));
    let bag = workspace.and_then(|w| {
        shell
            .core
            .active_channel
            .as_ref()
            .and_then(|id| w.messages.get(id))
    });
    serde_json::json!([
        workspace.map(|w| &w.read_preferences),
        workspace
            .and_then(|w| shell
                .core
                .active_channel
                .as_ref()
                .and_then(|id| w.channels.get(id)))
            .map(|channel| (
                channel.unread_count,
                channel.unread_count_display,
                channel.extra.get("is_member")
            )),
        bag.map(|b| (
            &b.last_read,
            b.read_revision,
            b.unread_count,
            b.loaded,
            b.history_refreshing,
            b.history_failed,
            &b.latest
        )),
        bag.and_then(|b| b.messages.last())
            .and_then(|m| m.ts.as_deref()),
        shell.loading,
        shell.loading_older,
        crate::unread::blocks_auto_read(shell),
        shell.transcript_visible(),
        shell.overlay.is_some(),
        format!("{:?}", shell.core.pending_scroll_to),
    ])
    .to_string()
}

pub(crate) fn ghost(shell: &ShellState) -> bool {
    let Some(channel) = shell.core.active_channel.as_ref() else {
        return true;
    };
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team));
    let current = workspace.and_then(|w| w.read_cursor(channel));
    match (shell.divider_at(channel), current) {
        (Some(pinned), Some(cursor)) => cmp_ts(Some(pinned), Some(cursor)).is_lt(),
        _ => true,
    }
}

pub(crate) fn eligible(shell: &ShellState, focused: bool) -> bool {
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
    else {
        return false;
    };
    let Some(channel) = shell
        .core
        .active_channel
        .as_ref()
        .and_then(|id| workspace.channels.get(id))
    else {
        return false;
    };
    // DM/MPDM membership is their open-conversation presence; public/private
    // previews must not acquire read authority from merely having message rows.
    let member = channel.is_im
        || channel.is_mpim
        || channel
            .extra
            .get("is_member")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
    focused
        && !crate::read_shortcuts::busy_channel(shell)
        && !crate::read_shortcuts::bulk::blocks_reads(shell)
        && member
        && shell.transcript_visible()
        && shell.overlay.is_none()
        && shell.core.pending_scroll_to.is_none()
        && !shell.loading
        && !crate::unread::blocks_auto_read(shell)
}

pub(crate) fn can_read(shell: &ShellState, focused: bool, opening: bool) -> bool {
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
    else {
        return false;
    };
    let Some(channel) = shell.core.active_channel.as_ref() else {
        return false;
    };
    let Some(bag) = workspace.messages.get(channel) else {
        return false;
    };
    let Some(latest) = latest_message(bag) else {
        return false;
    };
    let loaded = bag.loaded && !bag.history_refreshing && !bag.history_failed;
    let eligible = eligible(shell, focused) && loaded;
    if opening && workspace.read_preferences.mark_on_open(eligible, false) {
        return true;
    }
    let scope = shell.timeline_scope();
    let key = key(shell);
    let Some(seen) = shell.read_presentation.observed(&scope, &key) else {
        return false;
    };
    let latest_ts = latest.ts.as_deref();
    let cursor = workspace.read_cursor(channel);
    let unread_count = workspace
        .channels
        .get(channel)
        .filter(|channel| channel.unread_count.is_some() || channel.unread_count_display.is_some())
        .map(|_| u64::from(bag.unread_count));
    let reached_end = bag
        .latest
        .as_deref()
        .is_some_and(|known| !cmp_ts(latest_ts, Some(known)).is_lt());
    workspace
        .read_preferences
        .should_auto_mark(ReadObservation {
            eligible,
            loading: !loaded || shell.loading_older,
            divider_in_viewport: seen.divider_visible,
            ghost_divider: seen.ghost || ghost(shell),
            unread_count,
            reached_end,
            at_bottom: seen.at_bottom,
            cursor_covers_latest: cursor.is_some() && !cmp_ts(cursor, latest_ts).is_lt(),
            channel_read: unread_count == Some(0),
            latest_from_self: latest.user.as_deref() == Some(workspace.self_user_id.as_str()),
        })
}

pub(crate) fn latest_message(
    bag: &super_platinum_core::state::ChannelMessages,
) -> Option<&super_platinum_core::slack::models::Message> {
    bag.messages
        .iter()
        .rev()
        .find(|message| message.ts.as_deref().is_some_and(|ts| !bag.is_pending(ts)))
}

/// Keep the evaluation alive while its event service is mounted. Scope/key
/// checks reject anything queued by a replaced timeline, account or preference.
pub(crate) async fn observe(state: Signal<ShellState>) {
    let mut eval = dioxus::document::eval(include_str!("observe.js"));
    while let Ok(Some(mut sample)) = eval.recv::<Option<Observation>>().await {
        let changed = {
            // Pixel offsets and attention bookkeeping do not alter DOM. Avoid
            // repainting the entire shell for each scroll sample.
            #[allow(deprecated)]
            let mut shell = state.write_silent();
            let Some(owner) = conversation(&shell) else {
                continue;
            };
            let scope = shell.timeline_scope();
            let key = key(&shell);
            if sample
                .first_id
                .as_deref()
                .is_some_and(|id| !canonical_scrollmark(&shell, id))
            {
                sample.first_id = None;
                sample.offset = None;
            }
            shell.read_presentation.accept(&owner, &scope, &key, sample)
        };
        if changed && can_read(&state.peek(), crate::read_visibility::focused(), false) {
            spawn(crate::bootstrap::mark_visible_read(state));
        }
    }
}

fn canonical_scrollmark(shell: &ShellState, id: &str) -> bool {
    if !shell.channel_window_is_canonical() {
        return false;
    }
    let Some(row) = shell
        .channel_rows
        .get(shell.timeline_start..shell.timeline_end)
        .and_then(|rows| rows.iter().find(|row| row.id == id && !row.pending))
    else {
        return false;
    };
    shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
        .and_then(|w| {
            shell
                .core
                .active_channel
                .as_ref()
                .and_then(|channel| w.messages.get(channel))
        })
        .is_some_and(|bag| {
            !bag.is_pending(id)
                && bag
                    .messages
                    .get(row.source_index)
                    .and_then(|message| message.ts.as_deref())
                    == Some(id)
        })
}

#[cfg(test)]
pub(crate) fn record_fixture_observation(shell: &mut ShellState, divider: bool, at_bottom: bool) {
    let owner = conversation(shell).expect("test has an explicit account and conversation");
    let scope = shell.timeline_scope();
    let key = key(shell);
    let sample = Observation {
        scope: scope.clone(),
        key: key.clone(),
        stable: true,
        visible: true,
        divider_visible: divider,
        ghost: ghost(shell),
        at_bottom,
        pending: false,
        first_id: None,
        offset: None,
    };
    shell.read_presentation.accept(&owner, &scope, &key, sample);
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{
        slack::models::Message,
        state::{ReadMode, ReadPreferences},
    };

    fn ready(mode: ReadMode) -> ShellState {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        s.core.active_account = Some("isolated-read-test".into());
        s.core.pending_scroll_to = None;
        s.loading = false;
        s.stick_to_bottom = true;
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        let w = s.core.workspaces.get_mut(&team).unwrap();
        w.read_preferences = ReadPreferences::from_prefs(&mode.patch());
        w.channels
            .get_mut(&channel)
            .unwrap()
            .extra
            .insert("is_member".into(), true.into());
        let b = w.messages.get_mut(&channel).unwrap();
        b.messages = vec![Message {
            ts: Some("20.000001".into()),
            user: Some("OTHER".into()),
            ..Default::default()
        }];
        b.loaded = true;
        b.latest = Some("20.000001".into());
        b.last_read = Some("10.000001".into());
        b.unread_count = 10;
        s.unread_anchor = Some((channel, "10.000001".into()));
        s
    }
    #[test]
    fn saved_positions_reject_optimistic_and_unmapped_dom_row_ids() {
        let mut s = ready(ReadMode::LeftOffAndRead);
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        let b = s
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap();
        b.messages.push(Message {
            ts: Some("99.000001".into()),
            ..Default::default()
        });
        b.pending.push("99.000001".into());
        s.refresh_from_core();
        assert!(canonical_scrollmark(&s, "20.000001"));
        assert!(!canonical_scrollmark(&s, "99.000001"));
        assert!(!canonical_scrollmark(&s, "unknown-dom-row"));
    }
    #[test]
    fn absent_counts_and_cursor_are_unknown_even_with_loaded_history_and_self_tail() {
        let mut s = ready(ReadMode::NewestKeepUnseen);
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        let w = s.core.workspaces.get_mut(&team).unwrap();
        let c = w.channels.get_mut(&channel).unwrap();
        c.unread_count = None;
        c.unread_count_display = None;
        c.last_read = None;
        let b = w.messages.get_mut(&channel).unwrap();
        b.unread_count = 0;
        b.last_read = None;
        b.messages.last_mut().unwrap().user = Some(w.self_user_id.clone());
        s.unread_anchor = None;
        record_fixture_observation(&mut s, false, true);
        assert!(!can_read(&s, true, false));
    }
    #[test]
    fn admission_and_dispatch_use_the_same_latest_nonpending_message() {
        let mut s = ready(ReadMode::NewestAndRead);
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        let b = s
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap();
        b.messages.push(Message {
            ts: Some("99.000001".into()),
            ..Default::default()
        });
        b.pending.push("99.000001".into());
        assert_eq!(
            latest_message(b).and_then(|m| m.ts.as_deref()),
            Some("20.000001")
        );
        record_fixture_observation(&mut s, false, true);
        assert!(can_read(&s, true, false));
    }
    #[test]
    fn unseen_bottom_and_focus_never_substitute_for_the_unread_divider() {
        let mut s = ready(ReadMode::NewestKeepUnseen);
        record_fixture_observation(&mut s, false, true);
        assert!(!can_read(&s, true, false));
        assert!(!can_read(&s, true, true));
        record_fixture_observation(&mut s, true, false);
        assert!(can_read(&s, true, false));
        assert!(!can_read(&s, false, false));
    }
    #[test]
    fn pinned_ghost_and_changed_cursor_invalidate_previously_visible_divider() {
        let mut s = ready(ReadMode::NewestKeepUnseen);
        record_fixture_observation(&mut s, true, false);
        assert!(can_read(&s, true, false));
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap()
            .last_read = Some("15.000001".into());
        assert!(!can_read(&s, true, false));
        record_fixture_observation(&mut s, true, false);
        assert!(!can_read(&s, true, false));
    }
    #[test]
    fn preferences_loading_and_membership_are_current_dispatch_guards() {
        let mut s = ready(ReadMode::NewestAndRead);
        record_fixture_observation(&mut s, false, true);
        assert!(can_read(&s, true, true));
        assert!(can_read(&s, true, false));
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        s.core.workspaces.get_mut(&team).unwrap().read_preferences = ReadPreferences::default();
        assert!(!can_read(&s, true, true));
        assert!(!can_read(&s, true, false));
        s.core.workspaces.get_mut(&team).unwrap().read_preferences =
            ReadPreferences::from_prefs(&ReadMode::NewestAndRead.patch());
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap()
            .history_refreshing = true;
        assert!(!can_read(&s, true, true));
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap()
            .history_refreshing = false;
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .get_mut(&channel)
            .unwrap()
            .extra
            .insert("is_member".into(), false.into());
        assert!(!can_read(&s, true, true));
    }
    #[test]
    fn both_immediate_modes_mark_on_open_but_debounced_background_rules_differ() {
        let mut s = ready(ReadMode::LeftOffAndRead);
        record_fixture_observation(&mut s, false, true);
        assert!(can_read(&s, true, true));
        assert!(!can_read(&s, true, false));
        let team = s.core.active_team.clone().unwrap();
        s.core.workspaces.get_mut(&team).unwrap().read_preferences =
            ReadPreferences::from_prefs(&ReadMode::NewestAndRead.patch());
        assert!(
            !can_read(&s, true, false),
            "old DOM sample must not grant a changed policy"
        );
        record_fixture_observation(&mut s, false, true);
        assert!(can_read(&s, true, false));
    }
}
