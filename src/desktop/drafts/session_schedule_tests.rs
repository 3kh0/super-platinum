use super::*;
use super_platinum_core::drafts::{
    codec,
    sync::{ConsumeKind, DraftSync, OperationKind, RemoteDraft, Scope},
};

fn schedule(known: bool) -> (ShellState, super_platinum_core::drafts::sync::Operation) {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "drafts-empty");
    let scope = Scope {
        account_id: "00000000-0000-4000-8000-000000000091".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C2".into(),
            thread: None,
        },
    };
    let snapshot = codec::encode(&scope.key, "Frozen schedule").unwrap();
    let mut state = if known {
        DraftSync::adopt_remote(
            &scope,
            RemoteDraft {
                scope: scope.clone(),
                id: "DrSchedule".into(),
                client_msg_id: "00000000-0000-4000-8000-000000000092".into(),
                last_updated_ts: "100.000001".into(),
                snapshot: snapshot.clone(),
            },
        )
        .unwrap()
    } else {
        let mut state = DraftSync::new(scope.clone());
        state.edit(&scope, Some(snapshot));
        state
    };
    state
        .request_consume(
            &scope,
            state.local_revision(),
            ConsumeKind::Schedule { post_at: 500 },
        )
        .unwrap();
    let op = state.begin_operation(&scope).unwrap();
    assert_eq!(op.kind, OperationKind::Schedule { post_at: 500 });
    state.mark_uncertain(&scope, &op.id);
    shell.core.active_account = Some(scope.account_id.clone());
    shell.core.account_epoch = 17;
    shell.core.active_team = Some(scope.key.team.clone());
    shell.managed_drafts.begin(&scope.account_id, 17);
    shell.managed_drafts.apply_view(
        &scope.account_id,
        17,
        scope.key,
        View {
            local: LocalDraft {
                composer: super_platinum_core::ComposerState::with_text("Newer local"),
                attachments: vec![],
            },
            stored: Some(Stored { revision: 3, state }),
            blocked: None,
            uncommitted: None,
        },
    );
    (shell, op)
}

#[test]
fn known_schedule_owns_schedule_row_and_check_label() {
    let (shell, op) = schedule(true);
    let rows = recovery_rows(&shell);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "schedule");
    assert_eq!(rows[0].status, "Schedule unconfirmed");
    assert_eq!(rows[0].snapshot, op.snapshot);
    assert!(!rows[0].explanation.contains("delivery"));
    let kind = super::super::recovery::CheckKind::for_operation(&op).unwrap();
    assert_eq!(kind.label(), "Check schedule");
    assert_eq!(kind.attribute(), "schedule");
    assert_eq!(
        shell
            .managed_drafts
            .entry(&op.scope.account_id, &op.scope.key)
            .unwrap()
            .local
            .composer
            .text,
        "Newer local"
    );
}

#[test]
fn unknown_schedule_remains_visible_without_read_action_or_delivery_copy() {
    let (shell, op) = schedule(false);
    let rows = recovery_rows(&shell);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "schedule");
    assert_eq!(rows[0].snapshot, op.snapshot);
    assert!(super::super::recovery::CheckKind::for_operation(&op).is_none());
    assert!(
        super::super::recovery::operation(
            shell
                .managed_drafts
                .entry(&op.scope.account_id, &op.scope.key)
                .unwrap()
        )
        .is_none()
    );
}

#[test]
fn file_or_extra_route_schedule_has_no_read_action() {
    let (_, op) = schedule(true);
    for variant in 0..2 {
        let mut changed = op.clone();
        if variant == 0 {
            changed
                .snapshot
                .as_mut()
                .unwrap()
                .file_ids
                .push("F1".into());
        } else {
            changed.snapshot.as_mut().unwrap().destinations[0]["extra_route"] =
                serde_json::json!(true);
        }
        assert!(super::super::recovery::CheckKind::for_operation(&changed).is_none());
    }
}
