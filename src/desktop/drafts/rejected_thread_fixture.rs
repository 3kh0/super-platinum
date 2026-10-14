//! Dedicated thread/broadcast recovery. Only the named check opens injected SQLite.
use super::*;
use crate::state::{ComposerTarget, ShellState};
use super_platinum_core::{
    ComposerState,
    drafts::{
        codec,
        sync::{ConsumeKind, DraftSync, Workflow},
    },
};

pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000099";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const UNCERTAIN_ROOT: &str = "1719800500.000000";
pub(crate) const TEXT: &str = "Rejected thread reply restored with its channel broadcast choice.";
pub(crate) const UNCERTAIN: &str = "Unconfirmed sibling thread must not become another send.";
pub(crate) const CHANNEL_TEXT: &str = "Independent channel draft stays unchanged.";
pub(crate) const FAILED_TS: &str = "1990000000.000001";

pub(crate) fn key(root: &str) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: Some(root.into()),
    }
}

pub(crate) fn apply(shell: &mut ShellState) {
    session::fixture(shell);
    shell.managed_drafts.entries.clear();
    shell.managed_drafts.local_alternates.clear();
    shell.core.drafts = Default::default();
    shell.core.composer = Default::default();
    shell.core.thread_composer = Default::default();
    shell.core.composer_attachments.clear();
    shell.core.thread_composer_attachments.clear();
    shell.thread_root = None;
    let mut bag = crate::thread_vm::fixture_bag(4);
    for (root, text, rejected) in [(ROOT, TEXT, true), (UNCERTAIN_ROOT, UNCERTAIN, false)] {
        let key = key(root);
        let scope = Scope {
            account_id: ACCOUNT.into(),
            key: key.clone(),
        };
        let mut composer = ComposerState::with_text(text);
        composer.reply_broadcast = true;
        let mut sync = DraftSync::new(scope.clone());
        sync.edit(
            &scope,
            Some(codec::encode_composer(&key, &composer).unwrap()),
        );
        sync.request_consume(&scope, sync.local_revision(), ConsumeKind::Send)
            .unwrap();
        let operation = sync.begin_operation(&scope).unwrap();
        if rejected
            && std::env::var("SUPER_PLATINUM_REJECTED_THREAD_PHASE").as_deref() != Ok("restart")
        {
            let self_user = shell.core.workspaces["T1"].self_user_id.clone();
            let message = serde_json::from_value(serde_json::json!({
                "ts":FAILED_TS,"user":self_user,"client_msg_id":operation.client_msg_id,
                "channel":"C2","thread_ts":ROOT,"subtype":"thread_broadcast","text":TEXT,
                "blocks":operation.snapshot.as_ref().unwrap().blocks,
            }))
            .unwrap();
            bag.upsert(message);
            bag.pending.push(FAILED_TS.into());
            shell.pending_delivery.insert(
                FAILED_TS.into(),
                crate::state::PendingDeliveryStatus::Failed,
            );
        }
        if rejected {
            assert!(sync.reject(&scope, &operation.id));
        } else {
            sync = sync.restore_after_restart();
        }
        shell.managed_drafts.entries.insert(
            (ACCOUNT.into(), key),
            View {
                local: LocalDraft::default(),
                stored: Some(Stored {
                    revision: 1,
                    state: sync,
                }),
                blocked: None,
                uncommitted: None,
            },
        );
    }
    shell
        .core
        .threads
        .insert(("T1".into(), "C2".into(), ROOT.into()), bag);
    shell.sync_composer_drafts();
    shell.core.composer.text = CHANNEL_TEXT.into();
    shell.checkpoint_composers();
}

/// Read through a fresh owner; never infer durability from the projected book.
pub(crate) async fn verify() -> Result<serde_json::Value, Fault> {
    let account = settlement_fixture::account(ACCOUNT)?;
    let restored = account.owner(key(ROOT)).review_local().await?;
    let uncertain = account.owner(key(UNCERTAIN_ROOT)).review_local().await?;
    let exact = restored.local.composer.text == TEXT
        && restored.local.composer.reply_broadcast
        && restored.local.attachments.is_empty()
        && restored.stored.is_none()
        && restored.blocked.is_none()
        && restored.alternates.is_empty();
    let protected = uncertain.local.empty()
        && uncertain.stored.as_ref().is_some_and(|stored| {
            stored.state.consumption().is_some()
                && matches!(stored.state.workflow(), Workflow::Uncertain(_))
        });
    Ok(
        serde_json::json!({"ok":exact&&protected,"durableThreadRestored":exact,"uncertainPreserved":protected}),
    )
}

pub(crate) fn exact_composer(shell: &ShellState) -> bool {
    shell.composer_key(ComposerTarget::Thread) == Some(key(ROOT))
        && shell.core.thread_composer.text == TEXT
        && shell.core.thread_composer.reply_broadcast
        && shell.core.composer.text == CHANNEL_TEXT
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thread_fixture_uses_exact_broadcast_scope_and_keeps_uncertainty() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        apply(&mut shell);
        let restored = &shell.managed_drafts.entries[&(ACCOUNT.into(), key(ROOT))];
        let intent = recovery::rejected(restored).expect("definite thread rejection");
        let decoded = codec::decode(&key(ROOT), &intent.snapshot).unwrap();
        assert_eq!(decoded.composer().text, TEXT);
        assert!(decoded.composer().reply_broadcast);
        let uncertain = &shell.managed_drafts.entries[&(ACCOUNT.into(), key(UNCERTAIN_ROOT))];
        assert!(recovery::rejected(uncertain).is_none());
        assert!(shell.core.session.is_none() && shell.core.transport.is_none());
        assert_eq!(shell.core.composer.text, CHANNEL_TEXT);
    }
}
