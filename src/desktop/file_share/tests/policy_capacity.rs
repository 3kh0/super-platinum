use super::*;
fn registry() -> (ShellState, State) {
    let (mut shell, _) = sample();
    let team = shell.core.active_team.clone().unwrap();
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    let user = workspace.self_user_id.clone();
    workspace
        .users
        .entry(user.clone())
        .or_insert_with(|| serde_json::from_value(json!({"id":user,"is_bot":false})).unwrap());
    for n in 1..=33 {
        let id = format!("C{n:08}");
        workspace.channels.insert(
            id.clone(),
            serde_json::from_value(
                json!({"id":id,"is_channel":true,"is_member":true,"is_ext_shared":false}),
            )
            .unwrap(),
        );
    }
    let mut state = std::mem::take(&mut shell.file_share);
    state.sync(&shell);
    (shell, state)
}
fn response(id: &str) -> Value {
    json!({"ok":true,"channel":{"id":id,"is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false}})
}
fn ready(state: &mut State, shell: &ShellState, id: &str) -> policy_owner::Snapshot {
    let read = state.begin_policy(shell, id).unwrap();
    state
        .policies
        .get_mut(id)
        .unwrap()
        .finish(shell, read, response(id))
        .unwrap()
}
#[test]
fn all_live_reads_refuse_new_channel_without_removing_any_owner() {
    let (shell, mut state) = registry();
    let reads: Vec<_> = (1..=32)
        .map(|n| state.begin_policy(&shell, &format!("C{n:08}")).unwrap())
        .collect();
    let keys: Vec<_> = state.policies.keys().cloned().collect();
    assert!(matches!(
        state.begin_policy(&shell, "C00000033"),
        Err(Refusal::PolicyCapacity)
    ));
    assert_eq!(keys, state.policies.keys().cloned().collect::<Vec<_>>());
    assert_eq!(
        Refusal::PolicyCapacity.message(),
        "File sharing checks are busy. Try again shortly."
    );
    assert_ne!(
        Refusal::PolicyCapacity.message(),
        Refusal::Capacity.message()
    );
    for (index, read) in reads.into_iter().enumerate() {
        let id = format!("C{:08}", index + 1);
        let snapshot = state
            .policies
            .get_mut(&id)
            .unwrap()
            .finish(&shell, read, response(&id))
            .unwrap();
        assert!(state.policies[&id].current(&shell, &snapshot));
    }
}
#[test]
fn pressure_preserves_external_snapshot_and_reclaims_only_internal_latest() {
    let (shell, mut state) = registry();
    let held = ready(&mut state, &shell, "C00000001");
    let cloned = held.clone();
    for n in 2..=32 {
        drop(ready(&mut state, &shell, &format!("C{n:08}")));
    }
    drop(held);
    let next = state.begin_policy(&shell, "C00000033").unwrap();
    assert_eq!(state.policies.len(), 32);
    assert!(state.policies["C00000001"].current(&shell, &cloned));
    assert!(!state.policies.contains_key("C00000002"));
    state.policies.get_mut("C00000033").unwrap().failed(&next);
}
#[test]
fn canceled_ticket_is_pinned_until_exact_retirement_then_reclaimed() {
    let (shell, mut state) = registry();
    let read = state.begin_policy(&shell, "C00000001").unwrap();
    drop(delivery::Ticket {
        custody: state.custody.clone(),
        channel: "C00000001".into(),
        read: Some(read),
    });
    assert!(!state.policies["C00000001"].reclaimable());
    assert_eq!(state.custody.lock().unwrap().canceled_reads.len(), 1);
    state.retire_reads();
    assert!(state.policies["C00000001"].reclaimable());
    for n in 2..=32 {
        drop(ready(&mut state, &shell, &format!("C{n:08}")));
    }
    let _read = state.begin_policy(&shell, "C00000033").unwrap();
    assert!(!state.policies.contains_key("C00000001"));
}
#[test]
fn failed_new_capture_or_observation_cannot_evict_existing_facts() {
    let (mut shell, mut state) = registry();
    for n in 1..=32 {
        drop(ready(&mut state, &shell, &format!("C{n:08}")));
    }
    let keys: Vec<_> = state.policies.keys().cloned().collect();
    assert!(matches!(
        state.begin_policy(&shell, "C99999999"),
        Err(Refusal::Destination)
    ));
    assert_eq!(keys, state.policies.keys().cloned().collect::<Vec<_>>());
    let team = shell.core.active_team.clone().unwrap();
    shell.core.workspaces.get_mut(&team).unwrap().users.clear();
    assert!(matches!(
        state.begin_policy(&shell, "C00000033"),
        Err(Refusal::Changed)
    ));
    assert_eq!(keys, state.policies.keys().cloned().collect::<Vec<_>>());
}
#[test]
fn reclaimed_policy_restarts_unknown_and_omission_cannot_regrant_restrictions() {
    let (shell, mut state) = registry();
    let read = state.begin_policy(&shell, "C00000001").unwrap();
    let mut restricted = response("C00000001");
    restricted["channel"]["is_general"] = json!(true);
    restricted["channel"]["is_read_only"] = json!(true);
    let snapshot = state
        .policies
        .get_mut("C00000001")
        .unwrap()
        .finish(&shell, read, restricted)
        .unwrap();
    assert_ne!(snapshot.eligibility(), Eligibility::Allowed);
    drop(snapshot);
    for n in 2..=32 {
        drop(ready(&mut state, &shell, &format!("C{n:08}")));
    }
    drop(ready(&mut state, &shell, "C00000033"));
    assert!(!state.policies.contains_key("C00000001"));
    let read = state.begin_policy(&shell, "C00000001").unwrap();
    let mut partial = response("C00000001");
    partial["channel"]
        .as_object_mut()
        .unwrap()
        .remove("is_read_only");
    let snapshot = state
        .policies
        .get_mut("C00000001")
        .unwrap()
        .finish(&shell, read, partial)
        .unwrap();
    assert_eq!(
        snapshot.eligibility(),
        Eligibility::Unsupported(policy::Unsupported::PostingFacts)
    );
    drop(snapshot);
    assert_eq!(
        ready(&mut state, &shell, "C00000001").eligibility(),
        Eligibility::Allowed
    );
    assert_eq!(state.policies.len(), 32);
}

#[test]
fn all_external_snapshot_clones_refuse_until_last_holder_releases() {
    let (shell, mut state) = registry();
    let mut snapshots: Vec<_> = (1..=32)
        .map(|n| ready(&mut state, &shell, &format!("C{n:08}")))
        .collect();
    let clone = snapshots[0].clone();
    assert!(matches!(
        state.begin_policy(&shell, "C00000033"),
        Err(Refusal::PolicyCapacity)
    ));
    drop(snapshots.remove(0));
    assert!(matches!(
        state.begin_policy(&shell, "C00000033"),
        Err(Refusal::PolicyCapacity)
    ));
    drop(clone);
    let _read = state.begin_policy(&shell, "C00000033").unwrap();
    assert!(!state.policies.contains_key("C00000001"));
    assert_eq!(state.policies.len(), 32);
    for (index, snapshot) in snapshots.iter().enumerate() {
        assert!(state.policies[&format!("C{:08}", index + 2)].current(&shell, snapshot));
    }
}
