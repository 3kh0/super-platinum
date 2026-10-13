//! Offline projection of the same managed records returned after restart.
use super::*;
use super_platinum_core::{
    ComposerState,
    drafts::{
        codec,
        store::sync::BlockedLocal,
        sync::{ConsumeKind, DraftSync, RemoteDraft, Scope},
    },
};

pub(crate) fn apply(shell: &mut ShellState) {
    let account = "00000000-0000-4000-8000-000000000099";
    shell.core.active_account = Some(account.into());
    shell.core.active_team = Some("T1".into());
    shell.core.active_channel = Some("C2".into());
    shell.core.session = None;
    shell.core.transport = None;
    let key = DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: None,
    };
    let scope = Scope {
        account_id: account.into(),
        key: key.clone(),
    };
    let mut frozen = DraftSync::new(scope.clone());
    frozen.edit(
        &scope,
        Some(codec::encode(&key, "Frozen send remains available after restart.").unwrap()),
    );
    frozen
        .request_consume(&scope, frozen.local_revision(), ConsumeKind::Send)
        .unwrap();
    frozen.begin_operation(&scope).unwrap();
    let frozen = frozen.restore_after_restart();
    let other = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    };
    let other_scope = Scope {
        account_id: account.into(),
        key: other.clone(),
    };
    let local = LocalDraft {
        composer: ComposerState::with_text("Newer local draft stays separate."),
        attachments: vec![],
    };
    let mut conflict = DraftSync::new(other_scope.clone());
    conflict.edit(
        &other_scope,
        Some(codec::encode(&other, &local.composer.text).unwrap()),
    );
    assert!(conflict.observe_conflict(RemoteDraft {
        scope: other_scope,
        id: "DrRecoveryFixture".into(),
        client_msg_id: conflict.client_msg_id().into(),
        last_updated_ts: "1800000000000".into(),
        snapshot: codec::encode(&other, "Remote draft text was preserved.").unwrap()
    }));
    let clean_key = DraftKey {
        team: "T1".into(),
        channel: "C3".into(),
        thread: None,
    };
    let clean_scope = Scope {
        account_id: account.into(),
        key: clean_key.clone(),
    };
    let clean_local = LocalDraft {
        composer: ComposerState::with_text("Unfinished preparation stays editable."),
        attachments: vec![],
    };
    let mut clean = DraftSync::new(clean_scope.clone());
    clean.edit(
        &clean_scope,
        Some(codec::encode(&clean_key, &clean_local.composer.text).unwrap()),
    );
    let loaded = Loaded {
        local: vec![(other.clone(), local), (clean_key.clone(), clean_local)],
        synced: BTreeMap::from([
            (
                clean_key,
                Stored {
                    revision: 1,
                    state: clean,
                },
            ),
            (
                key,
                Stored {
                    revision: 4,
                    state: frozen,
                },
            ),
            (
                other.clone(),
                Stored {
                    revision: 7,
                    state: conflict,
                },
            ),
        ]),
        blocked_local: BTreeMap::from([(
            other,
            BlockedLocal {
                token: "offline-alternate-version".into(),
                draft: LocalDraft {
                    composer: ComposerState::with_text("Alternate local text was preserved."),
                    attachments: vec![],
                },
            },
        )]),
        uncommitted_local: BTreeMap::new(),
        local_alternates: BTreeMap::new(),
    };
    shell.core.drafts = DraftBook::default();
    let epoch = shell.core.account_epoch;
    shell.managed_drafts.begin(account, epoch);
    shell.managed_drafts.accept(account, epoch, loaded);
    restore_locals(shell, account);
    shell.main_view = crate::state::MainView::Scheduled;
    shell.core.scheduled.ensure_scope(epoch, "T1");
    shell.core.scheduled.loaded = true;
    shell.core.scheduled.filter = super_platinum_core::scheduled::DraftFilter::Drafts;
    shell.toast = None;
}
