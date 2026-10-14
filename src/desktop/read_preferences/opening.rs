use super::{presentation::Scrollmark, renderer};
use crate::state::ShellState;
use super_platinum_core::{
    domain::PendingScrollTarget,
    state::{ReadOpeningTarget, ReadScrollmark, cmp_ts},
};

/// Applies only ordinary conversation opening. Explicit targets are installed
/// by open_message afterwards and clear the renderer restoration separately.
pub(crate) fn apply(shell: &mut ShellState, unread: bool) {
    shell.read_presentation.restore = None;
    shell.read_presentation.unread_loading = None;
    shell.read_presentation.unread_fetching = None;
    shell.read_presentation.unread_continuation = None;
    shell.read_presentation.unread_paused = false;
    let Some(channel) = shell.core.active_channel.clone() else {
        return;
    };
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
    else {
        return;
    };
    let owner = renderer::conversation(shell);
    let saved = owner
        .as_ref()
        .and_then(|owner| shell.read_presentation.saved(owner));
    let target = workspace.read_preferences.opening_target(
        None,
        unread,
        workspace.read_cursor(&channel),
        saved.map(|mark| ReadScrollmark {
            ts: &mark.ts,
            offset: mark.offset,
        }),
    );
    let (target, restore) = match target {
        Ok(ReadOpeningTarget::FirstUnreadAfter(ts)) => {
            (PendingScrollTarget::FirstUnreadAfter(ts.into()), None)
        }
        Ok(ReadOpeningTarget::Saved(mark)) => (
            PendingScrollTarget::Message(mark.ts.into()),
            Some(Scrollmark {
                ts: mark.ts.into(),
                offset: mark.offset,
            }),
        ),
        Ok(ReadOpeningTarget::Explicit(ts)) => (PendingScrollTarget::Message(ts.into()), None),
        // Unknown/malformed preferences confer no read authority. Newest is a
        // safe visual position until a complete snapshot establishes policy.
        Ok(ReadOpeningTarget::Latest) | Err(_) => (PendingScrollTarget::Latest, None),
    };
    if unread && let Some(owner) = owner {
        shell.read_presentation.forget(&owner);
    }
    shell.stick_to_bottom = matches!(target, PendingScrollTarget::Latest);
    shell.read_presentation.restore = restore.map(|mark| (shell.timeline_scope(), mark));
    if let PendingScrollTarget::FirstUnreadAfter(cursor) = &target
        && !crate::fixture::is_fixture()
    {
        shell.read_presentation.unread_loading = Some((shell.timeline_scope(), cursor.clone()));
    }
    shell.core.pending_scroll_to = Some((channel, target));
    shell.reset_timeline_window();
}

pub(crate) fn offset(shell: &ShellState, target: &PendingScrollTarget) -> Option<f64> {
    let PendingScrollTarget::Message(ts) = target else {
        return None;
    };
    shell
        .read_presentation
        .restore
        .as_ref()
        .filter(|(scope, mark)| scope == &shell.timeline_scope() && &mark.ts == ts)
        .map(|(_, mark)| mark.offset)
}

pub(crate) fn first_after(shell: &ShellState, cursor: &str) -> Option<usize> {
    shell
        .channel_rows
        .iter()
        .position(|row| !row.pending && cmp_ts(Some(&row.ts), Some(cursor)).is_gt())
}

pub(crate) fn cancel(shell: &mut ShellState) {
    let scope = shell.timeline_scope();
    if shell
        .read_presentation
        .unread_fetching
        .as_ref()
        .is_some_and(|(visit, _)| visit == &scope)
        && let Some(bag) = shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get_mut(team))
            .and_then(|w| {
                shell
                    .core
                    .active_channel
                    .as_ref()
                    .and_then(|channel| w.messages.get_mut(channel))
            })
    {
        bag.history_refreshing = false;
    }
    shell.read_presentation.unread_fetching = None;
    shell.read_presentation.unread_loading = None;
    shell.read_presentation.restore = None;
    shell.read_presentation.unread_continuation = None;
    shell.read_presentation.unread_paused = false;
    if matches!(
        shell.core.pending_scroll_to,
        Some((
            _,
            PendingScrollTarget::Message(_) | PendingScrollTarget::FirstUnreadAfter(_)
        ))
    ) {
        shell.core.pending_scroll_to = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaRegistry;
    use super_platinum_core::state::{ReadMode, ReadPreferences};

    fn shell(mode: ReadMode) -> ShellState {
        let mut s = ShellState::fixture(MediaRegistry::default());
        let team = s.core.active_team.clone().unwrap();
        s.core.workspaces.get_mut(&team).unwrap().read_preferences =
            ReadPreferences::from_prefs(&mode.patch());
        s
    }
    #[test]
    fn explicit_jump_clears_the_unread_loader_and_preserves_older_history_access() {
        let mut s = shell(ReadMode::LeftOffAndRead);
        let channel = s.core.active_channel.clone().unwrap();
        apply(&mut s, true);
        s.read_presentation.unread_loading = Some((s.timeline_scope(), "1.0".into()));
        s.read_presentation.unread_fetching = s.read_presentation.unread_loading.clone();
        assert!(s.open_later_item(&channel, "1719800060.000200"));
        assert!(s.read_presentation.unread_loading.is_none());
        assert!(s.read_presentation.unread_fetching.is_none());
        assert!(s.read_presentation.restore.is_none());
        assert!(matches!(
            s.core.pending_scroll_to,
            Some((_, PendingScrollTarget::Message(_)))
        ));
    }
    #[test]
    fn newest_modes_do_not_inherit_an_unread_opening_target() {
        for mode in [ReadMode::NewestAndRead, ReadMode::NewestKeepUnseen] {
            let mut s = shell(mode);
            apply(&mut s, true);
            assert!(matches!(
                s.core.pending_scroll_to,
                Some((_, PendingScrollTarget::Latest))
            ));
            assert!(s.stick_to_bottom);
        }
    }
    #[test]
    fn left_off_uses_cursor_boundary_even_when_first_unread_is_not_loaded() {
        let mut s = shell(ReadMode::LeftOffAndRead);
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap()
            .last_read = Some("20.000001".into());
        apply(&mut s, true);
        assert_eq!(
            s.core.pending_scroll_to,
            Some((
                channel,
                PendingScrollTarget::FirstUnreadAfter("20.000001".into())
            ))
        );
        assert!(!s.stick_to_bottom);
    }
    #[test]
    fn stored_offsets_are_owned_by_the_exact_opening_scope_and_target() {
        let mut s = shell(ReadMode::LeftOffAndRead);
        s.read_presentation.restore = Some((
            s.timeline_scope(),
            Scrollmark {
                ts: "20.000001".into(),
                offset: -8.5,
            },
        ));
        assert_eq!(
            offset(&s, &PendingScrollTarget::Message("20.000001".into())),
            Some(-8.5)
        );
        assert_eq!(
            offset(&s, &PendingScrollTarget::Message("21.000001".into())),
            None
        );
        s.channel_generation += 1;
        assert_eq!(
            offset(&s, &PendingScrollTarget::Message("20.000001".into())),
            None
        );
    }
}
