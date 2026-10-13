//! Disconnected rejected-send recovery backed by the injected SQLite account.
use super::*;
use crate::state::ShellState;
use super_platinum_core::{
    ComposerState,
    drafts::{
        codec,
        sync::{ConsumeKind, DraftSync, Workflow},
    },
};

pub(crate) const RESTORED: &str = "This rejected send can return to an editable draft.";
pub(crate) const UNCERTAIN: &str = "Unconfirmed send must never return automatically.";
pub(crate) const FROZEN: &str = "Rejected original must not overwrite newer writing.";
pub(crate) const NEWER: &str = "Newer writing remains independently editable.";
pub(crate) const PARKED: &str = "Restored after navigation stays recoverable without a restart.";
const ACCOUNT: &str = "00000000-0000-4000-8000-000000000099";
fn key(channel: &str) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: channel.into(),
        thread: None,
    }
}

pub(crate) fn apply(shell: &mut ShellState) {
    super::session::fixture(shell);
    let mut synced = BTreeMap::new();
    let mut local = Vec::new();
    for (channel, text, rejected, newer) in [
        ("C1", RESTORED, true, false),
        ("C2", UNCERTAIN, false, false),
        ("C3", FROZEN, true, true),
        ("C4", PARKED, true, false),
    ] {
        let key = key(channel);
        let scope = Scope {
            account_id: ACCOUNT.into(),
            key: key.clone(),
        };
        let mut state = DraftSync::new(scope.clone());
        state.edit(&scope, Some(codec::encode(&key, text).unwrap()));
        state
            .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
            .unwrap();
        let operation = state.begin_operation(&scope).unwrap();
        if rejected {
            assert!(state.reject(&scope, &operation.id));
        } else {
            state = state.restore_after_restart();
        }
        if newer {
            state.edit(&scope, Some(codec::encode(&key, NEWER).unwrap()));
            local.push((
                key.clone(),
                LocalDraft {
                    composer: ComposerState::with_text(NEWER),
                    attachments: vec![],
                },
            ));
        }
        synced.insert(key, Stored { revision: 1, state });
    }
    let locals: BTreeMap<_, _> = local.into_iter().collect();
    shell.managed_drafts = session::Session {
        account: Some(ACCOUNT.into()),
        epoch: shell.core.account_epoch,
        ready: true,
        entries: synced
            .into_iter()
            .map(|(key, stored)| {
                let local = locals.get(&key).cloned().unwrap_or_default();
                (
                    (ACCOUNT.into(), key),
                    View {
                        local,
                        stored: Some(stored),
                        blocked: None,
                        uncommitted: None,
                    },
                )
            })
            .collect(),
        ..Default::default()
    };
    shell.core.drafts.items = locals;
    shell.core.composer = Default::default();
    shell.core.active_channel = Some("C2".into());
}

/// Complete the durable restoration, then deliberately omit foreground handoff.
/// This is the real editor state left when the originating surface disappears.
pub(crate) async fn park_return(
    mut shell: dioxus::prelude::Signal<ShellState>,
) -> Result<(), Fault> {
    use dioxus::prelude::*;
    let account = super::settlement_fixture::account(ACCOUNT)?;
    let key = key("C4");
    let (epoch, editor, intent, revision, serial) = {
        let mut state = shell.write();
        let epoch = state.core.account_epoch;
        let view = state
            .managed_drafts
            .entry(ACCOUNT, &key)
            .cloned()
            .ok_or(Fault::Unmanaged)?;
        let intent = super::recovery::rejected(&view).ok_or(Fault::Pending)?;
        let revision = view.stored.as_ref().ok_or(Fault::Unmanaged)?.revision;
        let editor = state
            .draft_writes
            .editor(&account, epoch, key.clone(), view)?;
        let serial = editor.snapshot().serial;
        (epoch, editor, intent, revision, serial)
    };
    let view = editor.return_rejected(revision, intent, serial).await?;
    if !shell
        .write()
        .managed_drafts
        .apply_view(ACCOUNT, epoch, key, view)
    {
        return Err(Fault::Stale);
    }
    Ok(())
}

pub(crate) async fn verify() -> Result<serde_json::Value, Fault> {
    let account = super::settlement_fixture::account(ACCOUNT)?;
    let restored = account.owner(key("C1")).review_local().await?;
    let uncertain = account.owner(key("C2")).review_local().await?;
    let newer = account.owner(key("C3")).review_local().await?;
    let parked = account.owner(key("C4")).review_local().await?;
    let parked_ok = parked.local.composer.text == PARKED && parked.stored.is_none();
    let restored_ok = restored.local.composer.text == RESTORED && restored.stored.is_none();
    let uncertain_ok = uncertain.local.empty()
        && uncertain.stored.as_ref().is_some_and(|row| {
            row.state.consumption().is_some()
                && matches!(row.state.workflow(), Workflow::Uncertain(_))
        });
    let newer_ok = newer.local.composer.text == NEWER
        && newer.stored.as_ref().is_some_and(|row| {
            row.state.consumption().is_some()
                && matches!(row.state.workflow(), Workflow::Rejected(_))
        });
    Ok(
        serde_json::json!({"ok": restored_ok && uncertain_ok && newer_ok && parked_ok, "restored": restored_ok, "parkedReturnReleased": parked_ok, "uncertainPreserved": uncertain_ok, "newerPreserved": newer_ok}),
    )
}
