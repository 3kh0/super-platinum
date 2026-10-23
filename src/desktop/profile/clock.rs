//! A displayed-clock change detector owned by the existing runtime tick.
//! No timer, async owner, cached member data or independent shell mutation.
use crate::state::ShellState;
use std::cell::RefCell;
thread_local! { static DISPLAYED: RefCell<Vec<Face>> = const { RefCell::new(Vec::new()) }; }

pub(crate) mod fixture;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
struct Face {
    slot: &'static str,
    account: Option<String>,
    epoch: u64,
    generation: u64,
    team: String,
    user: String,
    minute: Option<i64>,
}

fn minute(offset: Option<i32>, now: i64) -> Option<i64> {
    super_platinum_core::state::user_local_minute_at(offset, now)
}

fn faces(s: &ShellState, now: i64) -> Vec<Face> {
    if s.profile_user.is_none() && s.profile_hover.is_none() {
        return vec![];
    }
    let mut users = Vec::new();
    let canvas = crate::canvas::comments::profile_active(s);
    let pane_visible = if canvas {
        crate::canvas::comments::profile_visible(s)
    } else {
        !crate::channel_workflows::details::visible(s)
    };
    if pane_visible && let Some(user) = &s.profile_user {
        users.push(("pane", user.as_str()));
    }
    if let Some(hover) = &s.profile_hover
        && !crate::slack_lists::cell_editor_blocks_profile_hover(s)
        && crate::canvas::comments::profile_hover_token(s, hover).is_none_or(|v| v.current(s))
    {
        users.push(("hover", hover.user_id.as_str()));
    }
    if users.is_empty() {
        return vec![];
    }
    let Some(team) = s.core.active_team.as_ref() else {
        return vec![];
    };
    let Some(w) = s.core.workspaces.get(team) else {
        return vec![];
    };
    users
        .into_iter()
        .filter_map(|(slot, id)| {
            let user = w.users.get(id)?;
            Some(Face {
                slot,
                account: s.core.active_account.clone(),
                epoch: s.core.account_epoch,
                generation: if slot == "pane" {
                    s.profile_generation
                } else {
                    s.profile_hover_generation
                },
                team: team.clone(),
                user: id.to_owned(),
                minute: minute(user.tz_offset, now),
            })
        })
        .collect()
}

pub(crate) fn now(s: &ShellState) -> i64 {
    fixture::clock(s).unwrap_or_else(super_platinum_core::state::now_secs)
}

fn same_owner(a: &Face, b: &Face) -> bool {
    a.slot == b.slot
        && a.account == b.account
        && a.epoch == b.epoch
        && a.generation == b.generation
        && a.team == b.team
        && a.user == b.user
}

/// Record only a label that the current Profile renderer is actually producing.
/// Runtime observations never act as a substitute for this render observation.
pub(crate) fn display(s: &ShellState, user: &str, slot: &'static str) -> String {
    let now = now(s);
    let face = faces(s, now)
        .into_iter()
        .find(|v| v.slot == slot && v.user == user);
    if let Some(face) = face {
        DISPLAYED.with(|v| {
            let mut v = v.borrow_mut();
            v.retain(|old| old.slot != slot);
            v.push(face.clone());
        });
        if fixture::enabled(s) {
            fixture::render(now, &face);
        }
    }
    s.core
        .active_team
        .as_ref()
        .and_then(|t| s.core.workspaces.get(t))
        .and_then(|w| w.users.get(user))
        .and_then(|u| super_platinum_core::state::format_user_local_time_at(u.tz_offset, now))
        .unwrap_or_default()
}

#[derive(Default)]
pub(crate) struct Clock {
    pending: Vec<Face>,
}
impl Clock {
    fn update(&mut self, before: &[Face], current: Vec<Face>) -> bool {
        let changed: Vec<_> = current
            .into_iter()
            .filter(|v| {
                before
                    .iter()
                    .any(|old| same_owner(old, v) && old.minute != v.minute)
            })
            .collect();
        let notify = changed.iter().any(|v| !self.pending.contains(v));
        self.pending = changed;
        notify
    }
    pub(crate) fn poll(&mut self, s: &ShellState) -> bool {
        let now = now(s);
        let current = faces(s, now);
        let before = DISPLAYED.with(|v| {
            let mut v = v.borrow_mut();
            v.retain(|old| current.iter().any(|v| same_owner(old, v)));
            v.clone()
        });
        let changed = self.update(&before, current.clone());
        if fixture::enabled(s) {
            fixture::tick(now, &before, &current, changed);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn face(now: i64, offset: Option<i32>) -> Face {
        Face {
            slot: "pane",
            account: Some("A".into()),
            epoch: 1,
            generation: 0,
            team: "T".into(),
            user: "U".into(),
            minute: minute(offset, now),
        }
    }
    #[test]
    fn first_poll_after_render_boundary_notifies_once_until_rendered() {
        let mut c = Clock::default();
        let old = face(59, Some(0));
        let next = face(60, Some(0));
        assert!(c.update(&[old.clone()], vec![next.clone()]));
        assert!(!c.update(&[old], vec![next.clone()]));
        assert!(!c.update(&[next.clone()], vec![next]));
    }
    #[test]
    fn unchanged_display_day_jump_and_metadata_render_need_no_notification() {
        let mut c = Clock::default();
        assert!(!c.update(&[face(60, Some(0))], vec![face(86460, Some(0))]));
        assert!(!c.update(&[face(86460, Some(19800))], vec![face(86460, Some(19800))]));
        assert!(!c.update(&[face(86460, None)], vec![face(86520, None)]));
        assert!(!c.update(&[], vec![]));
    }
    #[test]
    fn local_second_boundary_midnight_and_missing_offset() {
        let mut c = Clock::default();
        assert!(c.update(&[face(28, Some(31))], vec![face(29, Some(31))]));
        assert!(c.update(&[face(86399, Some(0))], vec![face(86400, Some(0))]));
        assert!(c.update(&[face(86400, Some(0))], vec![face(86400, None)]));
    }
    #[test]
    fn replacement_and_unmounted_owners_do_not_invalidate_successor() {
        let mut c = Clock::default();
        let old = face(59, Some(0));
        let mut next = face(120, Some(0));
        next.epoch = 2;
        assert!(!c.update(&[old.clone()], vec![next]));
        let mut next = face(120, Some(0));
        next.generation = 1;
        assert!(!c.update(&[old.clone()], vec![next]));
        assert!(!c.update(&[old.clone()], vec![]));
        assert!(!c.update(&[], vec![old]));
    }
}
