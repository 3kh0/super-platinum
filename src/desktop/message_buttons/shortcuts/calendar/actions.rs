use super::*;
#[derive(Clone, Copy)]
pub(super) enum Field {
    Title,
    Description,
    Date,
    Start,
    End,
    Duration,
    Calendar,
    ShareChannel,
}
pub(super) fn edit(mut state: Signal<ShellState>, id: u64, field: Field, value: String) {
    let slots_changed = {
        let mut s = state.write();
        if !editable(&s, id) {
            return;
        }
        let c = active_mut(&mut s, id).unwrap();
        c.confirm_close = false;
        match field {
            Field::Title => {
                c.title = value;
                false
            }
            Field::Description => {
                c.description = value;
                false
            }
            Field::Calendar => {
                if value.is_empty() || c.calendars.iter().any(|x| x.id == value) {
                    c.calendar = value;
                }
                false
            }
            Field::ShareChannel => {
                if c.share_choices.iter().any(|x| x.id == value) {
                    c.share_selected = c.share_remote.iter().find(|x| x.id() == value).cloned();
                    c.share_channel = value;
                }
                false
            }
            Field::Date => {
                if value.len() > 10 {
                    return;
                }
                c.date = value;
                c.start = None;
                c.end = None;
                c.start_text.clear();
                c.end_text.clear();
                true
            }
            Field::Start => {
                if value.len() > 8 {
                    return;
                }
                c.start = cal::time::parse(&c.date, &value);
                c.start_text = value;
                false
            }
            Field::End => {
                if value.len() > 8 {
                    return;
                }
                c.end = cal::time::parse(&c.date, &value);
                c.end_text = value;
                false
            }
            Field::Duration => {
                let Ok(v) = value.parse::<u16>() else {
                    return;
                };
                if !matches!(v, 15 | 30 | 45 | 60 | 90 | 120) {
                    return;
                }
                c.duration = v;
                true
            }
        }
    };
    if slots_changed {
        load::slots(state, id);
    }
}
pub(super) fn mode(mut state: Signal<ShellState>, id: u64, custom: bool) {
    {
        let mut s = state.write();
        if !editable(&s, id) {
            return;
        }
        let c = active_mut(&mut s, id).unwrap();
        if c.custom == custom {
            return;
        }
        c.custom = custom;
        c.start = None;
        c.end = None;
        c.start_text.clear();
        c.end_text.clear();
    }
    load::slots(state, id);
}
pub(super) fn slot(mut state: Signal<ShellState>, id: u64, value: String) {
    let mut s = state.write();
    if !editable(&s, id) {
        return;
    }
    let c = active_mut(&mut s, id).unwrap();
    let Ok(start) = value.parse::<i64>() else {
        return;
    };
    let Some(slot) = c.slots.iter().find(|slot| slot.start == start) else {
        return;
    };
    c.start = Some(slot.start);
    c.end = Some(slot.end);
    c.start_text = cal::time::label(slot.start);
    c.end_text = cal::time::label(slot.end);
}
pub(super) fn guest(mut state: Signal<ShellState>, id: u64, user: String, remove: bool) {
    {
        let mut s = state.write();
        if !editable(&s, id) {
            return;
        }
        let c = active_mut(&mut s, id).unwrap();
        if remove {
            c.guests.retain(|p| p.id != user);
        } else {
            if c.guests.len() >= 512 || c.guests.iter().any(|p| p.id == user) {
                return;
            }
            let Some(person) = c.guest_choices.iter().find(|p| p.id == user).cloned() else {
                return;
            };
            c.guests.push(person);
        }
    }
    load::slots(state, id);
}
pub(super) fn share_query(state: Signal<ShellState>, id: u64, value: String) {
    sharing::search(state, id, value);
}
pub(super) fn share(state: Signal<ShellState>, id: u64, value: bool) {
    writes::preference(state, id, value);
}
pub(super) fn resume(mut state: Signal<ShellState>, id: u64) {
    let mut s = state.write();
    let Some(c) = active_mut(&mut s, id) else {
        return;
    };
    if c.stage == Stage::Failed && c.account.is_some() {
        c.stage = Stage::Editing;
        c.notice = None;
        c.attempt = None;
    }
}
pub(super) fn fresh(mut state: Signal<ShellState>, id: u64) {
    let job = fresh_owned(&mut state.write(), id);
    if let Some((next_id, ctx)) = job {
        load::start(state, next_id, ctx);
    }
}
// A new draft is a new dialog identity, even in the same conversation. Query
// generations start again at zero; an old read must never match that new epoch.
fn fresh_owned(s: &mut ShellState, id: u64) -> Option<(u64, Arc<Context>)> {
    let c = active(s, id)?;
    if c.fixture || !matches!(c.stage, Stage::Created | Stage::Uncertain | Stage::Failed) {
        return None;
    }
    let ctx = context(s, id)?;
    let d = s.message_buttons.shortcuts.dialog.as_ref()?;
    let next = Calendar::new(
        c.provider,
        d.owner.channel.clone(),
        vec![],
        load::share_choices(s, &ctx, "", &d.owner.channel),
    );
    let next_id = super::super::super::next_id();
    let d = s.message_buttons.shortcuts.dialog.as_mut()?;
    d.id = next_id;
    d.calendar = Some(next);
    Some((next_id, ctx))
}
#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
pub(super) fn event(s: &ShellState, id: u64) -> Option<cal::Event> {
    if !editable(s, id) {
        return None;
    }
    let c = active(s, id)?;
    if c.room_pending.is_some() || c.room_unknown || c.slot_loading {
        return None;
    }
    let d = s.message_buttons.shortcuts.dialog.as_ref()?;
    let channel = if c.share && matches!(c.account, Some(cal::Account::Legacy { .. })) {
        sharing::resolve(s, id)?
    } else {
        String::new()
    };
    // The retained thread belongs to the original conversation, not a newly
    // selected sharing target or a timestamp reused by another conversation.
    let thread = d
        .owner
        .thread
        .as_ref()
        .filter(|_| channel == d.owner.channel)
        .and_then(|(ts, _)| {
            let w = s.core.workspaces.get(&d.owner.ctx.session.team_id)?;
            w.messages
                .get(&channel)?
                .messages
                .iter()
                .any(|m| m.ts.as_ref() == Some(ts))
                .then(|| ts.clone())
        })
        .unwrap_or_default();
    let workspace = s.core.workspaces.get(&d.owner.ctx.session.team_id)?;
    if c.guests.iter().any(|p| {
        workspace
            .users
            .get(&p.id)
            .is_some_and(|u| !load::eligible(u, &d.owner.ctx.session.user_id))
    }) {
        return None;
    }
    let e = cal::Event {
        start: c.start?,
        end: c.end?,
        title: c.title.clone(),
        description: c.description.clone(),
        attendees: c.guests.iter().map(|p| p.id.clone()).collect(),
        calendar: c.calendar.clone(),
        meeting_url: c.rooms.get(&c.selected_room).cloned().unwrap_or_default(),
        share_channel: channel,
        share_thread: thread,
    };
    e.valid().then_some(e)
}
