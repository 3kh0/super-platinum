use super::*;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
    time::Instant,
};
use super_platinum_core::{ComposerState, domain::ComposerAttachment};

fn draft() -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text("Local with files"),
        attachments: vec![file(1), file(2)],
    }
}
fn file(id: u64) -> ComposerAttachment {
    ComposerAttachment {
        id,
        path: PathBuf::from(format!("synthetic-{id}.txt")),
        name: format!("file-{id}"),
        bytes: id * 5,
        uploading: false,
        upload_started: None,
        upload_cancel: None,
        upload_progress: None,
        preview_path: Some(PathBuf::from(format!("preview-{id}.png"))),
    }
}

#[test]
fn baseline_retains_full_idle_descriptors_but_plain_send_and_slash_still_refuse() {
    let local = draft();
    assert!(Policy::Baseline.same(&local, &local.clone()));
    assert!(!Policy::Plain.admits(&local));
    assert!(!Policy::Plain.same(&local, &local));
    assert!(
        !Policy::Baseline.permits_action(&crate::drafts::protected::Action::Consume(
            super_platinum_core::drafts::sync::ConsumeKind::Send
        ))
    );
    let mut plain = local.clone();
    plain.attachments.clear();
    assert!(Policy::Plain.same(&plain, &plain));
}

#[test]
fn every_ordered_descriptor_and_live_preview_change_invalidates_custody() {
    let before = draft();
    let changes: Vec<Box<dyn Fn(&mut LocalDraft)>> = vec![
        Box::new(|v| v.attachments.swap(0, 1)),
        Box::new(|v| {
            v.attachments.pop();
        }),
        Box::new(|v| v.attachments[0].id += 1),
        Box::new(|v| v.attachments[0].path = "different".into()),
        Box::new(|v| v.attachments[0].name.push('x')),
        Box::new(|v| v.attachments[0].bytes += 1),
        Box::new(|v| v.attachments[0].preview_path = None),
        Box::new(|v| v.composer.set_selection(1, 2)),
        Box::new(|v| v.composer.reply_broadcast = true),
        Box::new(|v| v.composer.text.push('x')),
    ];
    for change in changes {
        let mut newer = before.clone();
        change(&mut newer);
        assert!(!Policy::Baseline.same(&before, &newer));
    }
}

#[test]
fn idle_policy_refuses_even_shared_upload_capabilities() {
    for kind in 0..4 {
        let mut local = draft();
        match kind {
            0 => local.attachments[0].uploading = true,
            1 => local.attachments[0].upload_started = Some(Instant::now()),
            2 => local.attachments[0].upload_cancel = Some(Arc::new(AtomicBool::new(false))),
            _ => local.attachments[0].upload_progress = Some(Arc::new(AtomicU64::new(0))),
        }
        assert!(!Policy::Baseline.admits(&local));
        assert!(!Policy::Baseline.same(&local, &local.clone()));
        assert!(!Policy::Baseline.same_saved(&local, &local.clone()));
    }
}

#[test]
fn durable_membership_ignores_only_nonpersisted_preview_not_descriptors() {
    let local = draft();
    let mut saved: LocalDraft =
        serde_json::from_value(serde_json::to_value(&local).unwrap()).unwrap();
    assert!(Policy::Baseline.same_saved(&local, &saved));
    assert!(!Policy::Baseline.same(&local, &saved));
    saved.attachments[0].bytes += 1;
    assert!(!Policy::Baseline.same_saved(&local, &saved));
}
