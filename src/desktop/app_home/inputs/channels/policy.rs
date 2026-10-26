//! Canonical public/nonarchived admission plus the distinct internal-team restriction.
use super::*;
use super_platinum_core::slack::models::Channel;
fn public_filter() -> conversations::Filter {
    conversations::Filter {
        spec: super_platinum_core::block_actions::form::ConversationFilter {
            include: Some(vec!["public".into()]),
            ..Default::default()
        },
        enterprise_id: None,
        default_channels: false,
    }
}
pub(super) fn candidates(
    s: &ShellState,
    t: &Ticket,
    remote: &[Channel],
    query: &str,
) -> Vec<directory::Choice> {
    let Some(w) = s.core.workspaces.get(&t.owner.ctx.session.team_id) else {
        return vec![];
    };
    let rows = remote
        .iter()
        .map(|c| (c, true))
        .chain(w.channels.values().map(|c| (c, false)))
        .filter(|(c, _)| {
            t.policy.allows(
                &serde_json::to_value(conversations::effective_channel(w, c))
                    .expect("channel JSON"),
            )
        });
    conversations::candidates(w, &public_filter(), std::iter::empty(), rows, query)
}
pub(super) fn choice_current(s: &ShellState, t: &Ticket, c: &directory::Choice) -> bool {
    let Some(w) = s.core.workspaces.get(&t.owner.ctx.session.team_id) else {
        return false;
    };
    let Some(raw) = c.canonical.as_ref() else {
        return false;
    };
    let Ok(channel) = serde_json::from_value::<Channel>(raw.clone()) else {
        return false;
    };
    c.id == channel.id
        && conversations::choice_current(w, &public_filter(), c)
        && t.policy.allows(
            &serde_json::to_value(conversations::effective_channel(w, &channel))
                .expect("channel JSON"),
        )
}
/// Accepted search metadata updates retained proof, without rewriting displayed or committed IDs.
pub(super) fn refresh(s: &mut ShellState, t: &Ticket, rows: &[Channel]) {
    for (_, widget) in s
        .app_home
        .channels
        .multiples
        .iter_mut()
        .filter(|((o, _), _)| o.epoch == t.form.epoch && o.team == t.form.team)
    {
        if widget.conversation {
            continue;
        }
        for choice in widget.retained_choices.values_mut() {
            refresh_choice(choice, rows);
        }
    }
    for ((owner, _), widget) in s
        .app_home
        .channels
        .widgets
        .iter_mut()
        .filter(|((o, _), _)| o.epoch == t.form.epoch && o.team == t.form.team)
    {
        let _ = owner;
        if widget.conversation {
            continue;
        }
        if let Some(choice) = widget.retained_choice.as_mut() {
            refresh_choice(choice, rows);
        }
    }
}
fn refresh_choice(choice: &mut directory::Choice, rows: &[Channel]) {
    if let Some(row) = rows.iter().find(|c| c.id == choice.id) {
        let old = choice
            .canonical
            .as_ref()
            .and_then(|v| v["updated"].as_u64());
        if matches!((old,row.updated),(Some(a),Some(b)) if a>b) {
            return;
        }
        choice.label = row
            .name
            .as_ref()
            .map(|v| format!("#{v}"))
            .unwrap_or_else(|| row.id.clone());
        choice.canonical = Some(serde_json::to_value(row).expect("channel JSON"));
    }
}
