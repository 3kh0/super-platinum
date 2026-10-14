//! Multi-channel selections are checked together at the final mutation boundary.
use super::*;
use crate::message_buttons::inputs::channels;
use super_platinum_core::block_actions::form::{FormError, InputKind};

/// Accepted discovery updates every retained selection in this owner/session,
/// including another field or a stacked parent. A query is not a field's private
/// truth about a channel. Call only after the query and session ownership guard.
pub(super) fn refresh(
    session: &mut Session,
    records: &[super_platinum_core::slack::models::Channel],
) {
    // This runs only after the accepted owner/query guard. Origin eligibility
    // must see newer metadata even after its initial chip has been removed.
    let origins = session
        .modal
        .stack()
        .iter()
        .filter_map(|view| {
            view.viewed_conversation
                .as_ref()
                .map(|origin| origin.id().to_owned())
        })
        .collect::<Vec<_>>();
    channels::refresh(&mut session.viewed_proofs, &origins, records);
    for (view, form) in &session.forms {
        for (key, field) in &form.fields {
            if let InputValue::Channels(selected) | InputValue::Conversations(selected) =
                &field.value
            {
                let proofs = session
                    .channel_proofs
                    .entry((view.clone(), key.clone()))
                    .or_default();
                channels::refresh(proofs, selected, records);
            }
        }
    }
}

pub(super) fn validate_submission(shell: &ShellState) -> Result<(), FormError> {
    let session = shell
        .app_modal
        .as_ref()
        .ok_or(FormError::ConflictingUpdate)?;
    let view = &session
        .modal
        .stack()
        .last()
        .ok_or(FormError::ConflictingUpdate)?
        .id;
    let form = session.current_form().ok_or(FormError::ConflictingUpdate)?;
    let team = &session.modal.scope().team;
    let w = shell
        .core
        .workspaces
        .get(team)
        .ok_or(FormError::ConflictingUpdate)?;
    let channel_policy = channels::filter(shell, team);
    let empty = channels::Proofs::new();
    let mut errors = Vec::new();
    for (key, field) in &form.fields {
        let (selected, policy, is_conversation) = match (&field.kind, &field.value) {
            (InputKind::MultiChannel { .. }, InputValue::Channels(ids)) => {
                (ids, channel_policy.clone(), false)
            }
            (InputKind::MultiConversation { .. }, InputValue::Conversations(ids)) => {
                let Some(policy) =
                    crate::message_buttons::inputs::user::conversations::workspace_filter(
                        shell, team, &field.raw,
                    )
                else {
                    return Err(FormError::ConflictingUpdate);
                };
                (ids, policy, true)
            }
            _ => continue,
        };
        {
            let proofs = session
                .channel_proofs
                .get(&(view.clone(), key.clone()))
                .unwrap_or(&empty);
            let validate = if is_conversation {
                crate::message_buttons::inputs::conversations::validate
            } else {
                channels::validate
            };
            if let Err(reason) = validate(w, &policy, selected, proofs) {
                errors.push((key.clone(), reason.into()));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(FormError::Validation(errors))
    }
}

pub(super) fn refresh_users(
    session: &mut Session,
    users: &[super_platinum_core::slack::models::User],
) {
    for (view, form) in &session.forms {
        for (key, field) in &form.fields {
            if let InputValue::Conversations(selected) = &field.value {
                let proofs = session
                    .channel_proofs
                    .entry((view.clone(), key.clone()))
                    .or_default();
                channels::refresh_users(proofs, selected, users);
            }
        }
    }
}
