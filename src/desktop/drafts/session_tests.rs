use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::drafts::{
    codec,
    store::sync::BlockedLocal,
    sync::{ConsumeKind, DraftSync, RemoteDraft, Scope},
};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000091";

#[test]
fn uncertain_update_has_its_own_recovery_row_while_local_text_remains_present() {
    let mut shell = shell();
    let scope = Scope {
        account_id: ACCOUNT.into(),
        key: key(),
    };
    let remote = RemoteDraft {
        scope: scope.clone(),
        id: "Dr1".into(),
        client_msg_id: "00000000-0000-4000-8000-000000000092".into(),
        last_updated_ts: "100.000001".into(),
        snapshot: codec::encode(&key(), "Before").unwrap(),
    };
    let mut state = DraftSync::adopt_remote(&scope, remote).unwrap();
    state.edit(
        &scope,
        Some(codec::encode(&key(), "Frozen update").unwrap()),
    );
    let op = state.begin_operation(&scope).unwrap();
    state.mark_uncertain(&scope, &op.id);
    let mut saved = loaded(Stored { revision: 4, state });
    saved.local.push((
        key(),
        LocalDraft {
            composer: super_platinum_core::ComposerState::with_text("Frozen update"),
            attachments: vec![],
        },
    ));
    shell.managed_drafts.accept(ACCOUNT, 17, saved);
    let rows = recovery_rows(&shell);
    assert_eq!(rows.iter().filter(|row| row.kind == "update").count(), 1);
    assert_eq!(
        codec::decode(
            &key(),
            rows.iter()
                .find(|row| row.kind == "update")
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
        )
        .unwrap()
        .composer()
        .text,
        "Frozen update"
    );
    assert_eq!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .local
            .composer
            .text,
        "Frozen update"
    );
}

#[test]
fn projection_identity_requires_same_optional_writer_in_both_directions() {
    let original = Arc::new(());
    let replacement = Arc::new(());
    assert!(same_transport::<()>(&None, &None));
    assert!(same_transport(
        &Some(original.clone()),
        &Some(original.clone())
    ));
    assert!(!same_transport(&None, &Some(original.clone())));
    assert!(!same_transport(&Some(original.clone()), &None));
    assert!(!same_transport(&Some(original), &Some(replacement)));
}
fn key() -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: None,
    }
}
fn consumed() -> Stored {
    let scope = Scope {
        account_id: ACCOUNT.into(),
        key: key(),
    };
    let mut state = DraftSync::new(scope.clone());
    state.edit(
        &scope,
        Some(codec::encode(&key(), "Frozen message survives restart").unwrap()),
    );
    state
        .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
        .unwrap();
    let op = state.begin_operation(&scope).unwrap();
    state.mark_uncertain(&scope, &op.id);
    Stored { revision: 3, state }
}
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.core.active_account = Some(ACCOUNT.into());
    shell.core.account_epoch = 17;
    shell.managed_drafts.begin(ACCOUNT, 17);
    shell
}
fn loaded(stored: Stored) -> Loaded {
    Loaded {
        local: vec![],
        synced: BTreeMap::from([(key(), stored)]),
        blocked_local: BTreeMap::new(),
        uncommitted_local: BTreeMap::new(),
        local_alternates: BTreeMap::new(),
    }
}

#[test]
fn empty_local_uncertain_consumption_is_visible_after_metadata_load() {
    let mut shell = shell();
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    assert!(shell.managed_drafts.ready_for(ACCOUNT, 17));
    let rows = recovery_rows(&shell);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "Delivery unconfirmed");
    assert_eq!(
        codec::decode(&key(), rows[0].snapshot.as_ref().unwrap())
            .unwrap()
            .composer()
            .text,
        "Frozen message survives restart"
    );
    assert!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .local
            .empty()
    );
}

#[test]
fn wrong_account_epoch_scope_and_older_snapshot_do_not_replace_current_metadata() {
    let mut shell = shell();
    let stored = consumed();
    assert!(shell.managed_drafts.apply_stored(
        ACCOUNT,
        17,
        key(),
        stored.clone(),
        LocalDraft::default()
    ));
    assert!(!shell.managed_drafts.apply_stored(
        "other",
        17,
        key(),
        stored.clone(),
        LocalDraft::default()
    ));
    assert!(!shell.managed_drafts.apply_stored(
        ACCOUNT,
        18,
        key(),
        stored.clone(),
        LocalDraft::default()
    ));
    let mut older = stored.clone();
    older.revision = 2;
    assert!(
        !shell
            .managed_drafts
            .apply_stored(ACCOUNT, 17, key(), older, LocalDraft::default())
    );
    let mut other_key = key();
    other_key.channel = "C1".into();
    assert!(!shell.managed_drafts.apply_stored(
        ACCOUNT,
        17,
        other_key,
        stored,
        LocalDraft::default()
    ));
    shell.core.account_epoch += 1;
    assert!(recovery_rows(&shell).is_empty());
}

#[test]
fn newer_local_and_alternate_edits_stay_separate_from_frozen_payload() {
    let mut shell = shell();
    let mut loaded = loaded(consumed());
    let local = LocalDraft {
        composer: super_platinum_core::ComposerState::with_text("Newer local typing"),
        attachments: vec![],
    };
    loaded.local.push((key(), local.clone()));
    loaded.blocked_local.insert(
        key(),
        BlockedLocal {
            token: "version-token".into(),
            draft: LocalDraft {
                composer: super_platinum_core::ComposerState::with_text("Alternate edit"),
                attachments: vec![],
            },
        },
    );
    loaded.uncommitted_local.insert(key(), local.clone());
    shell.managed_drafts.accept(ACCOUNT, 17, loaded);
    let rows = recovery_rows(&shell);
    assert_eq!(rows.len(), 3);
    assert_eq!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .local
            .composer
            .text,
        "Newer local typing"
    );
    assert!(rows.iter().any(|row| row.kind == "alternate"
        && row.local.as_ref().unwrap().composer.text == "Alternate edit"));
    assert!(
        rows.iter()
            .any(|row| row.kind == "delivery" && row.snapshot.is_some())
    );
}

#[test]
fn refresh_failure_keeps_records_visible_but_blocks_authorization() {
    let mut shell = shell();
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    shell.managed_drafts.fail();
    assert!(!shell.managed_drafts.ready_for(ACCOUNT, 17));
    assert!(shell.managed_drafts.error.is_some());
    assert_eq!(recovery_rows(&shell).len(), 1);
    shell.core.active_account = Some("other-account".into());
    assert!(recovery_rows(&shell).is_empty());
}

#[test]
fn boot_merge_removes_detached_legacy_text_without_losing_unmanaged_memory_overlay() {
    let mut shell = shell();
    let mut other = key();
    other.channel = "C1".into();
    let local = |text: &str| LocalDraft {
        composer: super_platinum_core::ComposerState::with_text(text),
        attachments: vec![],
    };
    shell.core.drafts = DraftBook::restored(vec![
        (key(), local("Old bound text already consumed")),
        (other.clone(), local("Unmanaged in-memory edit")),
    ]);
    shell.core.composer =
        super_platinum_core::ComposerState::with_text("Old bound text already consumed");
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    restore_locals(&mut shell, ACCOUNT);
    assert!(!shell.core.drafts.items.contains_key(&key()));
    assert!(shell.core.composer.text.is_empty());
    assert_eq!(
        shell.core.drafts.items[&other].composer.text,
        "Unmanaged in-memory edit"
    );
    assert_eq!(recovery_rows(&shell).len(), 1);
}

#[test]
fn metadata_refresh_does_not_replace_live_typing_or_regress_newer_revision() {
    let mut shell = shell();
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    shell.core.composer = super_platinum_core::ComposerState::with_text("Typing during refresh");
    let mut newer = consumed();
    newer.revision = 10;
    shell
        .managed_drafts
        .apply_stored(ACCOUNT, 17, key(), newer, LocalDraft::default());
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    assert_eq!(shell.core.composer.text, "Typing during refresh");
    assert_eq!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .stored
            .as_ref()
            .unwrap()
            .revision,
        10
    );
}

#[test]
fn native_fixture_contains_empty_frozen_send_and_separate_local_and_remote_versions() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    super::fixture(&mut shell);
    assert!(shell.core.composer.text.is_empty());
    assert!(shell.core.transport.is_none());
    assert!(shell.core.session.is_none());
    let rows = recovery_rows(&shell);
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .find(|row| row.kind == "delivery")
            .unwrap()
            .status,
        "Delivery unconfirmed"
    );
    assert!(rows.iter().any(|row| row.kind == "remote"));
    assert!(rows.iter().any(|row| row.kind == "alternate"));
    assert_eq!(shell.core.drafts.items.len(), 2);
    assert_eq!(
        shell
            .core
            .drafts
            .items
            .values()
            .find(|local| local.composer.text == "Newer local draft stays separate.")
            .unwrap()
            .composer
            .text,
        "Newer local draft stays separate."
    );
}

#[test]
fn release_projection_requires_exact_account_visit_and_storage_revision() {
    let mut shell = shell();
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    assert!(
        !shell
            .managed_drafts
            .remove_entry("another-account", 17, &key(), 3)
    );
    assert!(!shell.managed_drafts.remove_entry(ACCOUNT, 18, &key(), 3));
    assert!(!shell.managed_drafts.remove_entry(ACCOUNT, 17, &key(), 2));
    assert_eq!(recovery_rows(&shell).len(), 1);
    assert!(shell.managed_drafts.remove_entry(ACCOUNT, 17, &key(), 3));
    assert!(recovery_rows(&shell).is_empty());
    assert!(!shell.managed_drafts.remove_entry(ACCOUNT, 17, &key(), 3));
}

#[test]
fn failed_released_input_is_visible_without_managed_state_and_cannot_replace_reenrollment() {
    let mut shell = shell();
    let local = LocalDraft {
        composer: super_platinum_core::ComposerState::with_text("Failed handoff 🚀"),
        attachments: vec![],
    };
    assert!(
        !shell
            .managed_drafts
            .apply_local_recovery("other", 17, key(), local.clone())
    );
    assert!(
        !shell
            .managed_drafts
            .apply_local_recovery(ACCOUNT, 18, key(), local.clone())
    );
    assert!(
        shell
            .managed_drafts
            .apply_local_recovery(ACCOUNT, 17, key(), local.clone())
    );
    let rows = recovery_rows(&shell);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "unsaved");
    assert_eq!(
        rows[0].local.as_ref().unwrap().composer.text,
        "Failed handoff 🚀"
    );
    assert!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .stored
            .is_none()
    );
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    assert!(
        !shell
            .managed_drafts
            .apply_local_recovery(ACCOUNT, 17, key(), local)
    );
    assert_eq!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .stored
            .as_ref()
            .unwrap()
            .revision,
        3
    );
}

#[test]
fn restored_orphan_alternate_is_inventoried_without_replacing_current_local_draft() {
    let mut shell = shell();
    let local = |text: &str| LocalDraft {
        composer: super_platinum_core::ComposerState::with_text(text),
        attachments: vec![],
    };
    shell
        .core
        .drafts
        .items
        .insert(key(), local("Current ordinary local"));
    shell.managed_drafts.accept(
        ACCOUNT,
        17,
        Loaded {
            local: vec![(key(), local("Current ordinary local"))],
            synced: BTreeMap::new(),
            blocked_local: BTreeMap::from([(
                key(),
                BlockedLocal {
                    token: "saved-alternate".into(),
                    draft: local("Preserved failed handoff"),
                },
            )]),
            uncommitted_local: BTreeMap::new(),
            local_alternates: BTreeMap::new(),
        },
    );
    restore_locals(&mut shell, ACCOUNT);
    assert_eq!(
        shell.core.drafts.items[&key()].composer.text,
        "Current ordinary local"
    );
    let rows = recovery_rows(&shell);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "alternate");
    assert_eq!(
        rows[0].local.as_ref().unwrap().composer.text,
        "Preserved failed handoff"
    );
}

#[test]
fn refresh_transport_guard_rejects_reopened_account_with_same_epoch() {
    let first = Some(Arc::new(()));
    let same = first.clone();
    let reopened = Some(Arc::new(()));
    assert!(same_transport(&first, &same));
    assert!(!same_transport(&first, &reopened));
    assert!(!same_transport(&first, &None));
    assert!(!same_transport(&None, &first));
    assert!(same_transport::<()>(&None, &None));
}

#[test]
fn clean_local_ownership_is_reviewable_after_interrupted_preparation() {
    let mut shell = shell();
    let local = LocalDraft {
        composer: super_platinum_core::ComposerState::with_text("Unfinished local edit"),
        attachments: vec![],
    };
    let scope = Scope {
        account_id: ACCOUNT.into(),
        key: key(),
    };
    let mut sync = DraftSync::new(scope.clone());
    sync.edit(
        &scope,
        Some(codec::encode(&key(), &local.composer.text).unwrap()),
    );
    shell.managed_drafts.apply_stored(
        ACCOUNT,
        17,
        key(),
        Stored {
            revision: 1,
            state: sync,
        },
        local,
    );
    let rows = recovery_rows(&shell);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "local");
    assert!(super::super::recovery::clean_local(
        shell.managed_drafts.entry(ACCOUNT, &key()).unwrap()
    ));
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    assert!(!super::super::recovery::clean_local(
        shell.managed_drafts.entry(ACCOUNT, &key()).unwrap()
    ));
}

#[test]
fn plural_writer_alternates_keep_latest_versions_separate_and_account_scoped() {
    let mut shell = shell();
    let version = |writer: &str, revision, text: &str| LocalAlternate {
        writer_id: writer.into(),
        revision,
        draft: LocalDraft {
            composer: super_platinum_core::ComposerState::with_text(text),
            attachments: vec![],
        },
    };
    let mut initial = loaded(consumed());
    initial.local_alternates.insert(
        key(),
        vec![
            version("writer-one", 3, "Latest first writer"),
            version("writer-two", 1, "Other account visit writer"),
        ],
    );
    shell.managed_drafts.accept(ACCOUNT, 17, initial);
    let mut stale = loaded(consumed());
    stale.local_alternates.insert(
        key(),
        vec![
            version("writer-one", 2, "Older first writer"),
            version("writer-two", 1, "Other account visit writer"),
        ],
    );
    shell.managed_drafts.accept(ACCOUNT, 17, stale);
    let rows = recovery_rows(&shell);
    let versions: Vec<_> = rows
        .iter()
        .filter(|row| row.kind == "writer-alternate")
        .collect();
    assert_eq!(versions.len(), 2);
    assert!(
        versions
            .iter()
            .any(|row| row.local.as_ref().unwrap().composer.text == "Latest first writer")
    );
    assert_ne!(versions[0].writer_id, versions[1].writer_id);
    assert!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .local
            .empty()
    );
    shell.core.active_account = Some("other".into());
    assert!(recovery_rows(&shell).is_empty());
}

#[test]
fn unmanaged_recovery_versions_block_consumption_without_claiming_managed_ownership() {
    let mut shell = shell();
    let local = LocalDraft {
        composer: super_platinum_core::ComposerState::with_text("Uncommitted alternate"),
        attachments: vec![],
    };
    assert!(!shell.managed_drafts.local_review_pending(ACCOUNT, &key()));
    assert!(
        shell
            .managed_drafts
            .apply_local_recovery(ACCOUNT, 17, key(), local)
    );
    assert!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .stored
            .is_none()
    );
    assert!(shell.managed_drafts.local_review_pending(ACCOUNT, &key()));
    assert!(
        !shell
            .managed_drafts
            .local_review_pending("other-account", &key())
    );
}

#[test]
fn explicit_settlement_invalidates_late_refresh_and_keeps_newer_writer_versions() {
    let mut shell = shell();
    let local = |text: &str| LocalDraft {
        composer: super_platinum_core::ComposerState::with_text(text),
        attachments: vec![],
    };
    let first = LocalAlternate {
        writer_id: "writer-one".into(),
        revision: 1,
        draft: local("Reviewed alternate"),
    };
    let mut initial = loaded(consumed());
    initial.local_alternates.insert(key(), vec![first.clone()]);
    shell.managed_drafts.accept(ACCOUNT, 17, initial);
    let old_projection = shell.managed_drafts.projection_epoch;
    let old = shell.managed_drafts.entry(ACCOUNT, &key()).unwrap().clone();
    let review = LocalReview {
        scope: Scope {
            account_id: ACCOUNT.into(),
            key: key(),
        },
        local: old.local.clone(),
        stored: old.stored.clone(),
        blocked: None,
        alternates: vec![first.clone()],
    };
    shell
        .managed_drafts
        .local_alternates
        .get_mut(&(ACCOUNT.into(), key()))
        .unwrap()[0] = LocalAlternate {
        revision: 2,
        draft: local("Newer writer input"),
        ..first
    };
    let mut chosen = old;
    chosen.stored.as_mut().unwrap().revision += 1;
    shell.managed_drafts.settled(&review, 17, chosen);
    assert_eq!(
        shell.managed_drafts.local_alternates[&(ACCOUNT.into(), key())][0].revision,
        2
    );
    let mut stale = loaded(consumed());
    stale
        .local_alternates
        .insert(key(), review.alternates.clone());
    assert!(
        !shell
            .managed_drafts
            .accept_refresh(ACCOUNT, 17, old_projection, stale)
    );
    assert_eq!(
        shell
            .managed_drafts
            .entry(ACCOUNT, &key())
            .unwrap()
            .stored
            .as_ref()
            .unwrap()
            .revision,
        4
    );
}

#[test]
fn fresh_refresh_removes_retired_sidecars_in_reopened_session() {
    let mut shell = shell();
    let mut initial = loaded(consumed());
    initial.local_alternates.insert(
        key(),
        vec![LocalAlternate {
            writer_id: "writer-one".into(),
            revision: 1,
            draft: LocalDraft::default(),
        }],
    );
    shell.managed_drafts.accept(ACCOUNT, 17, initial);
    assert!(!shell.managed_drafts.local_alternates.is_empty());
    shell.managed_drafts.accept(ACCOUNT, 17, loaded(consumed()));
    assert!(shell.managed_drafts.local_alternates.is_empty());
}
