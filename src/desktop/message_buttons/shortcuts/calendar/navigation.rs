//! Navigation follows a confirmed event only. The original write is never replayed.
use super::*;
use super_platinum_core::block_actions::home::navigation::Link;

pub(super) fn app_home(mut state: Signal<ShellState>, id: u64) {
    let target = {
        let s = state.peek();
        let Some(c) = active(&s, id).filter(|c| c.stage == Stage::Created && !c.fixture) else {
            return;
        };
        let Some(link) = c.app_link.as_deref() else {
            return;
        };
        let Some(ctx) = context(&s, id) else {
            return;
        };
        match Link::parse(link, &ctx.session.team_id) {
            Some(target) => Some(target),
            None if cal::external_url(link) => {
                let link = link.to_owned();
                drop(s);
                let _ = crate::media::open_external(&link);
                return;
            }
            None => None,
        }
    };
    let Some(target) = target else {
        if let Some(c) = active_mut(&mut state.write(), id) {
            c.notice = Some("This app Home link cannot be opened in the current workspace.".into());
        }
        return;
    };
    if !crate::app_home::navigation::open_calendar_link(state, target, id) {
        if let Some(c) = active_mut(&mut state.write(), id) {
            c.notice = Some("The destination workspace is unavailable. Your event was created; you can still open it in your calendar.".into());
        }
    }
}

pub(super) fn current(s: &ShellState, id: u64) -> bool {
    active(s, id).is_some_and(|c| c.stage == Stage::Created && !c.fixture)
}
pub(super) fn close_current(s: &mut ShellState, id: u64) {
    if current(s, id) {
        close(s, id, true);
    }
}

/// 9AoV closes after every confirmed shared event. Only a non-thread share
/// also navigates. The return value tells the caller whether history must load;
/// it is not the dismissal result. Unshared success keeps its confirmation.
pub(super) fn shared(s: &mut ShellState, id: u64, attempt: u64) -> bool {
    let Some(c) = active(s, id)
        .filter(|c| c.stage == Stage::Created && c.attempt == Some(attempt) && !c.fixture)
    else {
        return false;
    };
    let Some(ctx) = context(s, id) else {
        return false;
    };
    let Some(event) = s
        .message_buttons
        .shortcuts
        .calendar_attempts
        .iter()
        .find(|a| {
            a.id == attempt
                && a.epoch == ctx.epoch
                && a.team == ctx.session.team_id
                && matches!(a.result, Settlement::Created(_))
        })
        .and_then(|a| match &a.write {
            Write::Event(e) => Some(e),
            _ => None,
        })
    else {
        return false;
    };
    if event.share_channel.is_empty() {
        return false;
    }
    let channel = event.share_channel.clone();
    let in_thread = !event.share_thread.is_empty();
    let remote = match &c.share_selected {
        Some(sharing::Destination::Channel(c)) if c.id == channel => Some(c.clone()),
        _ => None,
    };
    let navigated = !in_thread
        && (|| {
            let w = s.core.workspaces.get_mut(&ctx.session.team_id)?;
            if !w.channels.contains_key(&channel) {
                w.channels.insert(channel.clone(), remote?);
            }
            s.refresh_from_core();
            let index = s.channels.iter().position(|c| c.id == channel)?;
            super::super::channel_scope(s, &ctx, &channel)?;
            s.select_channel(index, crate::state::ChannelOpen::Global);
            Some(())
        })()
        .is_some();
    // The confirmed write remains in its attempt book. Dismiss only this dialog,
    // even after Global navigation has retired its original editing owner.
    close(s, id, true);
    navigated
}

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod tests;
