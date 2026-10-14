use super::*;
use crate::drafts::{
    codec,
    sync::{ConsumeKind, DraftObservation, ObservationKind, RemoteDraft, Workflow},
};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000001";
const OTHER_ACCOUNT: &str = "00000000-0000-4000-8000-000000000009";
const UUID_A: &str = "00000000-0000-4000-8000-000000000002";
const UUID_B: &str = "00000000-0000-4000-8000-000000000003";

fn scope(channel: &str) -> Scope {
    Scope {
        account_id: ACCOUNT.into(),
        key: DraftKey {
            team: "T1".into(),
            channel: channel.into(),
            thread: None,
        },
    }
}
fn acknowledged(scope: Scope, id: &str, uuid: &str) -> Stored {
    let state = DraftSync::adopt_remote(
        &scope,
        RemoteDraft {
            scope: scope.clone(),
            id: id.into(),
            client_msg_id: uuid.into(),
            last_updated_ts: "100.000001".into(),
            snapshot: codec::encode(&scope.key, "Owned content").unwrap(),
        },
    )
    .unwrap();
    Stored { revision: 7, state }
}
fn local(scope: Scope, uuid: &str) -> Stored {
    let mut state = DraftSync::new(scope.clone());
    state.client_msg_id = uuid.into();
    state.edit(
        &scope,
        Some(codec::encode(&scope.key, "Local content").unwrap()),
    );
    Stored { revision: 8, state }
}
fn creating(scope: Scope, uuid: &str) -> Stored {
    let mut row = local(scope.clone(), uuid);
    row.state.begin_operation(&scope).unwrap();
    row
}
fn loaded(rows: Vec<Stored>) -> Loaded {
    Loaded {
        synced: rows
            .into_iter()
            .map(|row| (row.state.scope().key.clone(), row))
            .collect(),
        local: vec![],
        blocked_local: BTreeMap::new(),
        uncommitted_local: BTreeMap::new(),
        local_alternates: BTreeMap::new(),
    }
}
fn index(rows: Vec<Stored>) -> OwnedDraftIndex {
    OwnedDraftIndex::from_loaded(ACCOUNT, &loaded(rows)).unwrap()
}
fn active<'a>(id: &'a str, uuid: &'a str) -> Identity<'a> {
    Identity {
        draft_id: id,
        client_msg_id: Some(uuid),
        kind: IdentityKind::Active,
    }
}
fn deleted<'a>(id: &'a str, uuid: Option<&'a str>) -> Identity<'a> {
    Identity {
        draft_id: id,
        client_msg_id: uuid,
        kind: IdentityKind::Deleted,
    }
}
fn unsupported(row: &mut Stored, id: &str) {
    let event = DraftObservation {
        scope: row.state.scope().clone(),
        draft_id: id.into(),
        client_msg_id: row.state.client_msg_id().into(),
        event_ts: "200.000001".into(),
        kind: ObservationKind::Unsupported(
            serde_json::json!({"id":id,"client_msg_id":row.state.client_msg_id(),"future_field":true}),
        ),
    };
    assert_eq!(row.state.observe_event(&event), Ok(true));
}

#[test]
fn acknowledged_active_and_uuidless_delete_keep_exact_thread_destination() {
    let mut expected = scope("C1");
    expected.key.thread = Some("10.000001".into());
    let row = acknowledged(expected.clone(), "Dr1", UUID_A);
    let index = index(vec![row.clone()]);
    for identity in [active("Dr1", UUID_A), deleted("Dr1", None)] {
        let candidate = index.resolve(ACCOUNT, "T1", identity).unwrap().unwrap();
        assert_eq!(candidate.scope(), &expected);
        assert_eq!(candidate.revision(), 7);
        assert_eq!(candidate.draft_id(), "Dr1");
        assert_eq!(candidate.client_msg_id(), UUID_A);
        assert_eq!(candidate.authority(), Authority::Acknowledged);
        assert_eq!(candidate.revalidate(&index, Some(&row)), Ok(()));
    }
}

#[test]
fn identical_ids_in_other_teams_do_not_cross_source_boundaries() {
    let first = acknowledged(scope("C1"), "Dr1", UUID_A);
    let mut second_scope = scope("C2");
    second_scope.key.team = "T2".into();
    let second = acknowledged(second_scope.clone(), "Dr1", UUID_A);
    let index = index(vec![first, second]);
    assert_eq!(
        index
            .resolve(OTHER_ACCOUNT, "T1", active("Dr1", UUID_A))
            .err(),
        Some(IndexError::Source)
    );
    assert!(
        index
            .resolve(ACCOUNT, "T3", active("Dr1", UUID_A))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        index
            .resolve(ACCOUNT, "T2", active("Dr1", UUID_A))
            .unwrap()
            .unwrap()
            .scope(),
        &second_scope
    );
}

#[test]
fn missing_or_malformed_identity_never_uses_a_visible_destination() {
    let index = index(vec![acknowledged(scope("C1"), "Dr1", UUID_A)]);
    for identity in [
        active("bad id", UUID_A),
        active("Dr1", ""),
        active("Dr1", "00000000-0000-0000-0000-000000000000"),
        Identity {
            draft_id: "Dr1",
            client_msg_id: None,
            kind: IdentityKind::Active,
        },
    ] {
        assert_eq!(
            index.resolve(ACCOUNT, "T1", identity).err(),
            Some(IndexError::Identity)
        );
    }
    assert!(
        index
            .resolve(ACCOUNT, "T1", active("Unowned", UUID_B))
            .unwrap()
            .is_none()
    );
}

#[test]
fn local_only_uuid_is_not_create_authority_but_a_durable_pending_create_is() {
    let local_row = local(scope("C1"), UUID_A);
    let local_index = index(vec![local_row]);
    assert!(
        local_index
            .resolve(ACCOUNT, "T1", active("DrNew", UUID_A))
            .unwrap()
            .is_none()
    );
    let index = index(vec![creating(scope("C1"), UUID_A)]);
    for identity in [active("DrNew", UUID_A), deleted("DrNew", Some(UUID_A))] {
        assert_eq!(
            index
                .resolve(ACCOUNT, "T1", identity)
                .unwrap()
                .unwrap()
                .authority(),
            Authority::PendingCreate
        );
    }
    assert!(
        index
            .resolve(ACCOUNT, "T1", deleted("DrNew", None))
            .unwrap()
            .is_none()
    );
}

#[test]
fn duplicate_client_or_server_ids_are_ambiguous_even_with_one_exact_pair() {
    for rows in [
        vec![
            acknowledged(scope("C1"), "Dr1", UUID_A),
            acknowledged(scope("C2"), "Dr2", UUID_A),
        ],
        vec![
            acknowledged(scope("C1"), "Dr1", UUID_A),
            acknowledged(scope("C2"), "Dr1", UUID_B),
        ],
        vec![creating(scope("C1"), UUID_A), creating(scope("C2"), UUID_A)],
    ] {
        assert_eq!(
            index(rows)
                .resolve(ACCOUNT, "T1", active("Dr1", UUID_A))
                .err(),
            Some(IndexError::Ambiguous)
        );
    }
}

#[test]
fn uuidless_delete_cannot_borrow_a_known_ambiguous_client_identity() {
    let index = index(vec![
        acknowledged(scope("C1"), "Dr1", UUID_A),
        acknowledged(scope("C2"), "Dr2", UUID_A),
    ]);
    assert_eq!(
        index.resolve(ACCOUNT, "T1", deleted("Dr1", None)).err(),
        Some(IndexError::Ambiguous)
    );
}

#[test]
fn conflicting_pair_and_acknowledged_identity_rotation_are_refused() {
    let index = index(vec![
        acknowledged(scope("C1"), "Dr1", UUID_A),
        acknowledged(scope("C2"), "Dr2", UUID_B),
    ]);
    for identity in [
        active("Dr1", UUID_B),
        active("NewServerId", UUID_A),
        active("Dr1", "00000000-0000-4000-8000-000000000004"),
    ] {
        assert_eq!(
            index.resolve(ACCOUNT, "T1", identity).err(),
            Some(IndexError::Mismatch)
        );
    }
}

#[test]
fn create_cannot_claim_another_owners_acknowledged_or_retired_id() {
    let creating = creating(scope("C1"), UUID_A);
    let acknowledged = acknowledged(scope("C2"), "DrCollision", UUID_B);
    assert_eq!(
        index(vec![creating.clone(), acknowledged])
            .resolve(ACCOUNT, "T1", active("DrCollision", UUID_A))
            .err(),
        Some(IndexError::Mismatch)
    );
    let mut retired = local(scope("C2"), UUID_B);
    retired.state.retired_ids.push("DrCollision".into());
    assert_eq!(
        index(vec![creating, retired])
            .resolve(ACCOUNT, "T1", active("DrCollision", UUID_A))
            .err(),
        Some(IndexError::Retired)
    );
}

#[test]
fn observed_server_id_is_a_collision_barrier_not_new_routing_authority() {
    let mut first = creating(scope("C1"), UUID_A);
    unsupported(&mut first, "DrObserved");
    let second = creating(scope("C2"), UUID_B);
    let index = index(vec![first, second]);
    assert_eq!(
        index
            .resolve(ACCOUNT, "T1", active("DrObserved", UUID_B))
            .err(),
        Some(IndexError::Mismatch)
    );
    assert!(
        index
            .resolve(ACCOUNT, "T1", deleted("DrObserved", None))
            .unwrap()
            .is_none()
    );
}

#[test]
fn recorded_create_uuid_cannot_rotate_to_a_different_server_identity() {
    let mut row = creating(scope("C1"), UUID_A);
    unsupported(&mut row, "DrFirst");
    let index = index(vec![row]);
    assert_eq!(
        index
            .resolve(ACCOUNT, "T1", active("DrSecond", UUID_A))
            .err(),
        Some(IndexError::Mismatch)
    );
    assert_eq!(
        index
            .resolve(ACCOUNT, "T1", active("DrFirst", UUID_A))
            .unwrap()
            .unwrap()
            .authority(),
        Authority::PendingCreate
    );
}

#[test]
fn delete_before_create_prevents_late_echo_routing_without_borrowing_a_uuid() {
    let mut row = creating(scope("C1"), UUID_A);
    let event = DraftObservation {
        scope: row.state.scope().clone(),
        draft_id: "DrDeleted".into(),
        client_msg_id: UUID_A.into(),
        event_ts: "200.000001".into(),
        kind: ObservationKind::Deleted,
    };
    assert_eq!(row.state.observe_event(&event), Ok(true));
    let index = index(vec![row]);
    for identity in [active("DrDeleted", UUID_A), deleted("DrDeleted", None)] {
        assert_eq!(
            index.resolve(ACCOUNT, "T1", identity).err(),
            Some(IndexError::Retired)
        );
    }
}

#[test]
fn stale_inventory_same_revision_changes_and_missing_rows_all_retire_candidates() {
    let row = acknowledged(scope("C1"), "Dr1", UUID_A);
    let index = index(vec![row.clone()]);
    let candidate = index
        .resolve(ACCOUNT, "T1", active("Dr1", UUID_A))
        .unwrap()
        .unwrap();
    assert_eq!(candidate.revalidate(&index.clone(), Some(&row)), Ok(()));
    let rebuilt = OwnedDraftIndex::from_loaded(ACCOUNT, &loaded(vec![row.clone()])).unwrap();
    assert_eq!(
        candidate.revalidate(&rebuilt, Some(&row)),
        Err(IndexError::StaleInventory)
    );
    let mut changed = row.clone();
    changed.revision += 1;
    assert_eq!(
        candidate.revalidate(&index, Some(&changed)),
        Err(IndexError::StaleOwner)
    );
    changed = row.clone();
    changed.state.edit(
        &scope("C1"),
        Some(codec::encode(&scope("C1").key, "New edit").unwrap()),
    );
    assert_eq!(
        candidate.revalidate(&index, Some(&changed)),
        Err(IndexError::StaleOwner)
    );
    assert_eq!(
        candidate.revalidate(&index, None),
        Err(IndexError::StaleOwner)
    );
}

#[test]
fn invalid_row_scope_or_revision_does_not_create_partial_authority() {
    let first = acknowledged(scope("C1"), "Dr1", UUID_A);
    for mutation in 0..4 {
        let mut rows = loaded(vec![first.clone()]);
        let key = scope("C1").key;
        let row = rows.synced.get_mut(&key).unwrap();
        match mutation {
            0 => row.revision = 0,
            1 => row.state.scope.account_id = OTHER_ACCOUNT.into(),
            2 => row.state.scope.key.channel = "Wrong".into(),
            _ => row.state.acknowledged.as_mut().unwrap().client_msg_id = UUID_B.into(),
        }
        assert!(matches!(
            OwnedDraftIndex::from_loaded(ACCOUNT, &rows),
            Err(IndexError::Inventory)
        ));
    }
}

#[test]
fn rejected_create_and_unsupported_conflict_remain_owned_routing_candidates() {
    let mut row = local(scope("C1"), UUID_A);
    let create = row.state.begin_operation(&scope("C1")).unwrap();
    row.state
        .request_consume(&scope("C1"), row.state.local_revision(), ConsumeKind::Send)
        .unwrap();
    assert_eq!(create.kind, OperationKind::Create);
    assert!(row.state.reject(&scope("C1"), &create.id));
    assert!(matches!(row.state.workflow(), Workflow::Rejected(_)));
    assert_eq!(
        index(vec![row])
            .resolve(ACCOUNT, "T1", active("DrNew", UUID_A))
            .unwrap()
            .unwrap()
            .authority(),
        Authority::PendingCreate
    );
    let mut row = creating(scope("C1"), UUID_A);
    unsupported(&mut row, "DrNew");
    assert!(matches!(row.state.workflow(), Workflow::Conflict { .. }));
    assert!(
        index(vec![row])
            .resolve(ACCOUNT, "T1", active("DrNew", UUID_A))
            .unwrap()
            .is_some()
    );
}

#[test]
fn historical_uuid_in_another_owner_blocks_a_new_create_claim() {
    let mut historical = acknowledged(scope("C2"), "DrHistorical", UUID_A);
    let event = DraftObservation {
        scope: historical.state.scope().clone(),
        draft_id: "DrHistorical".into(),
        client_msg_id: UUID_A.into(),
        event_ts: "200.000001".into(),
        kind: ObservationKind::Deleted,
    };
    assert_eq!(historical.state.observe_event(&event), Ok(true));
    assert!(historical.state.acknowledged().is_none());
    assert_ne!(historical.state.client_msg_id(), UUID_A);
    let only_history = index(vec![historical.clone()]);
    assert!(
        only_history
            .resolve(ACCOUNT, "T1", active("DrNew", UUID_A))
            .unwrap()
            .is_none()
    );
    let collided = index(vec![creating(scope("C1"), UUID_A), historical]);
    assert_eq!(
        collided
            .resolve(ACCOUNT, "T1", active("DrNew", UUID_A))
            .err(),
        Some(IndexError::Ambiguous)
    );
}

#[test]
fn pending_base_and_conflict_remote_must_remain_inside_the_full_owner_scope() {
    let first = acknowledged(scope("C1"), "Dr1", UUID_A);
    for conflict in [false, true] {
        let mut row = first.clone();
        if conflict {
            let mut remote = row.state.acknowledged().unwrap().clone();
            remote.scope.account_id = OTHER_ACCOUNT.into();
            row.state.workflow = Workflow::Conflict {
                remote: Some(remote),
                pending: None,
            };
        } else {
            row.state.edit(
                &scope("C1"),
                Some(codec::encode(&scope("C1").key, "Edited").unwrap()),
            );
            row.state.begin_operation(&scope("C1")).unwrap();
            let Workflow::InFlight(operation) = &mut row.state.workflow else {
                panic!()
            };
            operation.base.as_mut().unwrap().scope.key.channel = "Other".into();
        }
        assert!(matches!(
            OwnedDraftIndex::from_loaded(ACCOUNT, &loaded(vec![row])),
            Err(IndexError::Inventory)
        ));
    }
}
