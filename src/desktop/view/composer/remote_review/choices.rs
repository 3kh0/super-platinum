//! Display copies never become write authority. Choices retain exact store IDs.
use super_platinum_core::drafts::{
    LocalDraft, codec,
    store::sync::{LocalChoice, LocalReview, RemoteChoice, RemoteReview},
    sync::{Snapshot, Workflow},
};

#[derive(Clone)]
pub(super) enum Content {
    Local(LocalDraft),
    Snapshot(Snapshot),
}
#[derive(Clone)]
pub(super) struct Choice {
    pub label: String,
    pub choice: Option<RemoteChoice>,
    pub content: Content,
    /// Exact retained wire snapshot if this local choice preserves it. Showing
    /// the local text alone would hide rich blocks and destination metadata.
    pub retained: Option<Snapshot>,
    pub unavailable: Option<&'static str>,
}

pub(super) fn build(review: &RemoteReview) -> Vec<Choice> {
    let inventory = review.inventory();
    let mut result = local_choices(inventory);
    if let Some(stored) = &inventory.stored
        && let Workflow::Conflict {
            remote: Some(previous),
            ..
        } = stored.state.workflow()
        && previous.snapshot != review.remote().snapshot
    {
        result.push(Choice {
            label: "Previously preserved Slack version".into(),
            choice: None,
            content: Content::Snapshot(previous.snapshot.clone()),
            retained: None,
            unavailable: Some("This older Slack version is shown for comparison. Only the latest checked Slack version can be selected."),
        });
    }
    result.push(Choice {
        label: "Latest checked Slack version".into(),
        choice: Some(RemoteChoice::Remote),
        content: Content::Snapshot(review.remote().snapshot.clone()),
        retained: None,
        unavailable: None,
    });
    result
}

fn local_choices(inventory: &LocalReview) -> Vec<Choice> {
    let mut result = Vec::new();
    let mut push = |choice, label: String, local: &LocalDraft| {
        result.push(Choice {
            label,
            choice: Some(RemoteChoice::Local(choice)),
            content: Content::Local(local.clone()),
            retained: retained(inventory, local).cloned(),
            unavailable: (!supported(inventory, local)).then_some(
                "This version contains files or content that cannot be chosen here. Cancel to keep it and review it in Drafts; choosing another version replaces it.",
            ),
        });
    };
    push(
        LocalChoice::Current,
        "Current saved local draft".into(),
        &inventory.local,
    );
    if let Some(blocked) = &inventory.blocked {
        push(
            LocalChoice::Blocked {
                token: blocked.token.clone(),
            },
            "Preserved local version".into(),
            &blocked.draft,
        );
    }
    for (index, alternate) in inventory.alternates.iter().enumerate() {
        push(
            LocalChoice::Alternate {
                writer_id: alternate.writer_id.clone(),
                revision: alternate.revision,
            },
            format!("Saved local version {}", index + 1),
            &alternate.draft,
        );
    }
    result
}

fn supported(inventory: &LocalReview, local: &LocalDraft) -> bool {
    if !local.attachments.is_empty() {
        return false;
    }
    // Preserve adopted raw metadata, including explicit broadcast:false. This
    // mirrors the store's retained-projection admission, never its mutation.
    retained(inventory, local).is_some()
        || local.composer.text.is_empty()
        || codec::encode_composer(&inventory.scope.key, &local.composer).is_ok()
}

fn retained<'a>(inventory: &'a LocalReview, local: &LocalDraft) -> Option<&'a Snapshot> {
    inventory
        .stored
        .as_ref()
        .and_then(|row| row.state.desired())
        .filter(|raw| {
            codec::decode(&inventory.scope.key, raw).is_ok_and(|projection| {
                projection.composer().text == local.composer.text
                    && projection.composer().reply_broadcast == local.composer.reply_broadcast
            })
        })
}

pub(super) fn allowed(review: &RemoteReview, choice: &RemoteChoice) -> bool {
    match choice {
        RemoteChoice::Remote => true,
        RemoteChoice::Local(choice) => review
            .inventory()
            .chosen_local(choice)
            .is_some_and(|local| supported(review.inventory(), local)),
    }
}

#[cfg(test)]
mod tests;
