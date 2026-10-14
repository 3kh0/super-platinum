//! Retained canonical entity evidence; z+fW channel policy remains public/nonarchived.
use super::user::{conversations, directory::Choice};
use crate::state::ShellState;
use std::collections::BTreeMap;
use super_platinum_core::state::Workspace;

pub(crate) type Proofs = BTreeMap<String, Choice>;
pub(crate) const INVALID: &str = "A selected channel is unavailable. Remove it or select it again.";

pub(crate) fn filter(shell: &ShellState, team: &str) -> conversations::Filter {
    super::channel::filter(shell, team)
}

/// Resolve by ID, never by a truncated search page. Cached current metadata wins;
/// remote-only records stay local to the original message/modal owner.
pub(crate) fn resolve(w: &Workspace, id: &str, retained: &Proofs) -> Option<Choice> {
    retained.get(id).cloned().or_else(|| {
        w.channels
            .get(id)
            .map(|c| Choice {
                id: id.into(),
                label: conversations::channel_label(w, c),
                canonical: Some(serde_json::to_value(c).expect("channel JSON")),
            })
            .or_else(|| {
                w.users.get(id).map(|u| Choice {
                    id: id.into(),
                    label: w.display_name(id),
                    canonical: Some(serde_json::to_value(u).expect("user JSON")),
                })
            })
    })
}

pub(crate) fn validate(
    w: &Workspace,
    policy: &conversations::Filter,
    selected: &[String],
    retained: &Proofs,
) -> Result<Vec<Choice>, &'static str> {
    selected
        .iter()
        .map(|id| {
            let choice = resolve(w, id, retained).ok_or(INVALID)?;
            if choice.id != *id
                || choice
                    .canonical
                    .as_ref()
                    .and_then(|v| v.get("id"))
                    .and_then(serde_json::Value::as_str)
                    != Some(id.as_str())
                || !conversations::choice_current(w, policy, &choice)
            {
                return Err(INVALID);
            }
            Ok(choice)
        })
        .collect()
}

/// A later accepted directory page may invalidate a selected remote-only record.
pub(crate) fn refresh(
    proofs: &mut Proofs,
    selected: &[String],
    records: &[super_platinum_core::slack::models::Channel],
) {
    for c in records.iter().filter(|c| selected.contains(&c.id)) {
        let old_updated = proofs
            .get(&c.id)
            .and_then(|p| p.canonical.as_ref())
            .and_then(|v| v.get("updated"))
            .and_then(serde_json::Value::as_u64);
        if matches!((old_updated, c.updated), (Some(old), Some(new)) if old > new) {
            continue;
        }
        proofs.insert(
            c.id.clone(),
            Choice {
                id: c.id.clone(),
                label: c.name.clone().unwrap_or_else(|| c.id.clone()),
                canonical: Some(serde_json::to_value(c).expect("channel JSON")),
            },
        );
    }
}

/// Search is read-only, but newly learned metadata must outlive Cancel. Keep it
/// with every already retained selection of this account/workspace/viewer;
/// neither staged choices nor committed wire arrays are changed here.
pub(super) fn refresh_retained(
    shell: &mut ShellState,
    owner: &super_platinum_core::block_actions::validation::MessageActionOwner,
    records: &[super_platinum_core::slack::models::Channel],
) {
    for entry in shell.message_buttons.inputs.values_mut().filter(|entry| {
        let candidate = entry.source.owner();
        candidate.account_epoch == owner.account_epoch
            && candidate.team_id == owner.team_id
            && candidate.self_user_id == owner.self_user_id
    }) {
        let mut selected = retained_ids(&entry.model);
        selected.extend(entry.channel_choices.keys().cloned());
        let selected = selected.into_iter().collect::<Vec<_>>();
        refresh(&mut entry.channel_choices, &selected, records);
    }
}

/// Prop reconciliation may reset field display while committed sibling wire
/// still owns the previous array. A remount can seed from either representation.
pub(super) fn retained_ids(model: &super::MessageInputs) -> std::collections::BTreeSet<String> {
    let mut ids = std::collections::BTreeSet::new();
    for (key, field) in &model.fields {
        if let super::InputValue::Channels(selected) | super::InputValue::Conversations(selected) =
            &field.value
        {
            ids.extend(selected.iter().cloned());
        }
        if let Some(saved) = model.saved_conversations(key) {
            ids.extend(saved);
        }
        if let Some(saved) = model.saved_channels(key) {
            ids.extend(saved);
        }
    }
    ids
}

/// Conversation IM values are member IDs, not opened DM IDs. Retain the actual
/// member record and refresh it even when a later page excludes that member.
pub(crate) fn refresh_users(
    proofs: &mut Proofs,
    selected: &[String],
    users: &[super_platinum_core::slack::models::User],
) {
    for user in users.iter().filter(|u| selected.contains(&u.id)) {
        let old = proofs
            .get(&user.id)
            .and_then(|p| p.canonical.as_ref())
            .and_then(|v| v.get("updated"))
            .and_then(serde_json::Value::as_u64);
        let new = user
            .extra
            .get("updated")
            .and_then(serde_json::Value::as_u64);
        if matches!((old,new),(Some(old),Some(new)) if old>new) {
            continue;
        }
        proofs.insert(
            user.id.clone(),
            Choice {
                id: user.id.clone(),
                label: user.name.clone().unwrap_or_else(|| user.id.clone()),
                canonical: Some(serde_json::to_value(user).expect("member JSON")),
            },
        );
    }
}
pub(super) fn refresh_retained_users(
    shell: &mut ShellState,
    owner: &super_platinum_core::block_actions::validation::MessageActionOwner,
    users: &[super_platinum_core::slack::models::User],
) {
    for entry in shell.message_buttons.inputs.values_mut().filter(|entry| {
        let candidate = entry.source.owner();
        candidate.account_epoch == owner.account_epoch
            && candidate.team_id == owner.team_id
            && candidate.self_user_id == owner.self_user_id
    }) {
        let mut selected = retained_ids(&entry.model);
        selected.extend(entry.channel_choices.keys().cloned());
        refresh_users(
            &mut entry.channel_choices,
            &selected.into_iter().collect::<Vec<_>>(),
            users,
        );
    }
}
