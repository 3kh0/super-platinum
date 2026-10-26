//! o5++ Home conversations: no current-conversation provider or channel mode mask.
use super::*;
use super_platinum_core::slack::models::{Channel, User};
pub(crate) mod dispatch;
pub(crate) mod fixture;
#[cfg(test)]
mod multiple_tests;
#[cfg(test)]
mod tests;
pub(super) fn initial_key(raw: &Value) -> &'static str {
    if raw["type"] == "conversations_select" {
        "initial_conversation"
    } else {
        "initial_channel"
    }
}
pub(super) fn labels<'a>(
    s: &ShellState,
    team: &str,
    ids: impl Iterator<Item = &'a String>,
    retained: Option<&directory::Choice>,
) -> Vec<(String, String)> {
    let w = s.core.workspaces.get(team);
    ids.map(|id| {
        let label = w
            .and_then(|w| {
                w.channels
                    .get(id)
                    .map(|c| conversations::channel_label(w, c))
                    .or_else(|| w.users.get(id).map(|_| w.display_name(id)))
            })
            .or_else(|| retained.filter(|c| &c.id == id).map(|c| c.label.clone()))
            .unwrap_or_else(|| id.clone());
        (id.clone(), label)
    })
    .collect()
}
pub(super) fn candidates(
    s: &ShellState,
    t: &Ticket,
    users: &[User],
    channels: &[Channel],
    query: &str,
) -> Vec<directory::Choice> {
    let Some(w) = s.core.workspaces.get(&t.owner.ctx.session.team_id) else {
        return vec![];
    };
    let Some(filter) = &t.conversation else {
        return vec![];
    };
    conversations::candidates(
        w,
        filter,
        users
            .iter()
            .map(|u| (u, true))
            .chain(w.users.values().map(|u| (u, false))),
        channels
            .iter()
            .map(|c| (c, true))
            .chain(w.channels.values().map(|c| (c, false))),
        query,
    )
}
pub(super) fn choice_current(s: &ShellState, t: &Ticket, c: &directory::Choice) -> bool {
    t.conversation
        .as_ref()
        .zip(s.core.workspaces.get(&t.owner.ctx.session.team_id))
        .is_some_and(|(f, w)| conversations::choice_current(w, f, c))
}
pub(super) fn refresh(s: &mut ShellState, t: &Ticket, users: &[User], channels: &[Channel]) {
    let Some(w) = s.core.workspaces.get(&t.owner.ctx.session.team_id) else {
        return;
    };
    let labels: BTreeMap<_, _> = channels
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                (
                    conversations::channel_label(w, c),
                    serde_json::to_value(c).expect("channel"),
                ),
            )
        })
        .chain(users.iter().map(|u| {
            (
                u.id.clone(),
                (
                    super_platinum_core::state::display_name(Some(u), &u.id),
                    serde_json::to_value(u).expect("user"),
                ),
            )
        }))
        .collect();
    for ((owner, _), widget) in &mut s.app_home.channels.widgets {
        if !widget.conversation || owner.epoch != t.form.epoch || owner.team != t.form.team {
            continue;
        }
        if let Some(c) = widget.retained_choice.as_mut() {
            if let Some((label, raw)) = labels.get(&c.id) {
                let old = c.canonical.as_ref().and_then(|v| v["updated"].as_u64());
                let new = raw["updated"].as_u64();
                if !matches!((old,new),(Some(a),Some(b)) if a>b) {
                    c.label = label.clone();
                    c.canonical = Some(raw.clone());
                }
            }
        }
    }
    for ((owner, _), widget) in &mut s.app_home.channels.multiples {
        if !widget.conversation || owner.epoch != t.form.epoch || owner.team != t.form.team {
            continue;
        }
        for c in widget.retained_choices.values_mut() {
            if let Some((label, raw)) = labels.get(&c.id) {
                let old = c.canonical.as_ref().and_then(|v| v["updated"].as_u64());
                let new = raw["updated"].as_u64();
                if !matches!((old,new),(Some(a),Some(b)) if a>b) {
                    c.label = label.clone();
                    c.canonical = Some(raw.clone());
                }
            }
        }
    }
}
pub(super) async fn read(
    mut state: Signal<ShellState>,
    t: &Ticket,
    query: &str,
) -> Result<(Vec<User>, Vec<Channel>), Error> {
    let requests = directory::aggregate::prepare(&t.owner.ctx, query, t.conversation.as_ref());
    let fixture = fixture::request(&mut state.write(), t, &requests);
    match fixture {
        Some(rx) => {
            let value = rx.await.unwrap_or_else(|_| {
                Err(Error::Offline("Home conversation read unavailable".into()))
            })?;
            let response = |key: &str, enabled: bool| {
                if enabled {
                    value
                        .get(key)
                        .cloned()
                        .map(Some)
                        .ok_or_else(|| Error::Api("conversation_search_invalid".into()))
                } else {
                    Ok(None)
                }
            };
            directory::aggregate::decode(directory::aggregate::Responses {
                users: response("users", requests.users.as_ref().is_ok_and(|v| v.is_some())),
                channels: response(
                    "channels",
                    requests.channels.as_ref().is_ok_and(|v| v.is_some()),
                ),
                mpims: response("mpims", requests.mpims.as_ref().is_ok_and(|v| v.is_some())),
            })
        }
        None => directory::aggregate::execute(&t.owner.ctx, requests).await,
    }
}
