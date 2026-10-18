use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    assert!(fixture::seed(&mut shell));
    shell
}
#[test]
fn changed_shares_can_be_abandoned_without_rebasing_or_forgetting_grants() {
    let mut shell = shell();
    let e = entry(&shell).unwrap();
    let guard = Guard::capture(&shell, &e).unwrap();
    guard.update(|e| {
        e.recipients[0].result = ShareResult::Uncertain;
        e.recipients[0].dm_attempted = true;
    });
    let loaded = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    loaded.document.raw["shares"] = json!({"private":{"D00000002":[]}});
    let before = loaded.clone();
    assert!(Guard::capture(&shell, &entry(&shell).unwrap()).is_err());
    let p = presentation(&shell).unwrap();
    assert!(!p.can_share && !p.can_check && !p.can_not_now);
    let recovery = recovery(&shell).unwrap();
    assert!(recovery.can_abandon);
    assert!(abandon_shell(&mut shell, &recovery));
    assert!(!blocks_mutation(&shell));
    assert!(can_open(&shell, "other", "other"));
    assert!(shell.slack_lists.view.as_ref().unwrap().editor.is_none());
    let after = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(after.document.raw, before.document.raw);
    assert_eq!(after.records, before.records);
    assert!(shell.slack_lists.operations.is_empty());
    let archived = previous_share(&guard.key, "U00000002").unwrap();
    assert_eq!(archived.result, ShareResult::Uncertain);
    assert!(archived.dm_attempted);
    assert!(!abandon_shell(&mut shell, &recovery));
    // Receipt-only late updates cannot recreate an abandoned Entry.
    guard.update(|e| e.stage = Stage::Review);
    assert!(entry(&shell).is_none());
}
#[test]
fn canceled_or_missing_cell_has_owned_detached_recovery() {
    let mut shell = shell();
    let guard = Guard::capture(&shell, &entry(&shell).unwrap()).unwrap();
    guard.update(|e| e.recipients[0].result = ShareResult::Acknowledged);
    assert!(super::super::super::cancel_shell(&mut shell, guard.draft));
    let view = shell.slack_lists.view.as_mut().unwrap();
    view.loaded.as_mut().unwrap().records =
        RecordsPage::parse(json!({"ok":true,"records":[]})).unwrap();
    let recovery = recovery(&shell).unwrap();
    assert!(!recovery.in_editor && recovery.can_abandon);
    assert!(abandon_shell(&mut shell, &recovery));
    assert!(!blocks_mutation(&shell));
    assert!(previous_share(&guard.key, "U00000002").is_some());
}
#[test]
fn busy_job_keeps_custody_until_response_then_archives_final_outcome() {
    let mut shell = shell();
    let guard = Guard::capture(&shell, &entry(&shell).unwrap()).unwrap();
    guard.update(|e| {
        e.job = Some(guard.job);
        e.recipients[0].result = ShareResult::Sending;
    });
    let recovery = recovery(&shell).unwrap();
    assert!(!recovery.can_abandon);
    assert!(!abandon_shell(&mut shell, &recovery));
    guard.update(|e| {
        e.recipients[0].result = ShareResult::Uncertain;
        e.job = None;
    });
    assert!(abandon_shell(&mut shell, &recovery));
    assert_eq!(
        previous_share(&guard.key, "U00000002").unwrap().result,
        ShareResult::Uncertain
    );
}
#[test]
fn recovery_callback_rejects_owner_visit_draft_scope_and_entry_aba() {
    for change in 0..5 {
        registry(|r| *r = Registry::default());
        let mut shell = shell();
        let recovery = recovery(&shell).unwrap();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.slack_lists.view.as_mut().unwrap().generation += 1,
            2 => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .editor
                    .as_mut()
                    .unwrap()
                    .generation += 1
            }
            3 => shell.slack_lists.scope = Scope::capture(&shell),
            _ => registry(|r| r.entries.get_mut(&recovery.key).unwrap().serial += 1),
        }
        assert!(!abandon_shell(&mut shell, &recovery));
        assert_eq!(registry(|r| r.entries.len()), 1);
        assert!(registry(|r| r.shares.is_empty()));
    }
}
#[test]
fn capacity_reserves_all_live_lists_and_transfer_never_evicts_unknowns() {
    let mut shell = shell();
    let key = Key::current(&shell).unwrap();
    let mut e = entry(&shell).unwrap();
    e.selected = (0..64).map(|i| format!("U{i:08}")).collect();
    registry(|r| {
        r.entries.clear();
        for i in 0..4 {
            let mut k = key.clone();
            k.list = format!("F_{i}");
            r.entries.insert(k, e.clone());
        }
    });
    assert!(can_reserve(0));
    assert!(!can_reserve(1));
    let mut fifth = key.clone();
    fifth.list = "F_FIFTH".into();
    assert!(insert(fifth, e.clone()).is_err());
    assert_eq!(registry(|r| r.entries.len()), 4);
    // Defensive transfer failure is atomic even for an overfull legacy state.
    registry(|r| {
        r.entries.clear();
        e.recipients[0].result = ShareResult::Uncertain;
        r.entries.insert(key.clone(), e.clone());
        r.shares = (0..LIMIT)
            .map(|_| ShareReceipt {
                key: key.clone(),
                serial: 1,
                person: e.recipients[0].clone(),
                observed: false,
            })
            .collect();
    });
    let recovery = recovery(&shell).unwrap();
    assert!(!recovery.can_abandon);
    assert!(!abandon_shell(&mut shell, &recovery));
    assert!(blocks_mutation(&shell));
    assert_eq!(registry(|r| r.shares.len()), LIMIT);
}

#[test]
fn actual_shell_replacement_keeps_attempted_registry_custody_for_exact_owner() {
    let old = shell();
    let guard = Guard::capture(&old, &entry(&old).unwrap()).unwrap();
    guard.update(|e| {
        e.job = Some(guard.job);
        e.recipients[0].result = ShareResult::Sending;
        e.recipients[0].dm_attempted = true;
    });
    drop(old);
    let mut replacement =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    assert!(Key::current(&replacement).as_ref() == Some(&guard.key));
    assert!(!guard.current(&replacement));
    assert!(blocks_mutation(&replacement));
    let token = recovery(&replacement).unwrap();
    assert!(!token.in_editor && !token.can_abandon);
    assert!(!abandon_shell(&mut replacement, &token));
    let account = replacement.core.active_account.clone();
    replacement.core.active_account = Some("foreign-assignment-owner".into());
    replacement.slack_lists.scope = Scope::capture(&replacement);
    assert!(entry(&replacement).is_none());
    assert!(recovery(&replacement).is_none());
    assert!(!abandon_shell(&mut replacement, &token));
    // An old worker may settle its own receipt while the replacement shows a
    // foreign account. It cannot apply its record or assignment into that shell.
    guard.update(|e| {
        e.recipients[0].result = ShareResult::Uncertain;
        e.job = None;
    });
    assert!(replacement.slack_lists.operations.is_empty());
    replacement.core.active_account = account;
    replacement.slack_lists.scope = Scope::capture(&replacement);
    assert!(!abandon_shell(&mut replacement, &token));
    let fresh = recovery(&replacement).unwrap();
    assert!(fresh.can_abandon);
    let data = replacement
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    assert!(abandon_shell(&mut replacement, &fresh));
    assert!(!blocks_mutation(&replacement));
    assert_eq!(
        previous_share(&guard.key, "U00000002").unwrap().result,
        ShareResult::Uncertain
    );
    let current = replacement
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(current.document.raw, data.document.raw);
    assert_eq!(current.records, data.records);
}
