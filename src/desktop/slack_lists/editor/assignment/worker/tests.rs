use super::*;
use serde_json::json;
#[test]
fn fresh_visibility_excludes_grant_without_changing_selected_assignment_ids() {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    assert!(fixture::seed(&mut shell));
    let guard = Guard::capture(&shell, &entry(&shell).unwrap()).unwrap();
    let selected = entry(&shell).unwrap().selected;
    assert_eq!(share_candidates(&entry(&shell).unwrap()).unwrap().len(), 2);
    guard.update(|e| {
        e.intent
            .observe_shared_access("U00000002", &json!({"ok":true,"can_see":true}))
            .unwrap();
    });
    let e = entry(&shell).unwrap();
    let candidates = share_candidates(&e).unwrap();
    assert_eq!(
        candidates.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        vec!["W00000003"]
    );
    assert_eq!(e.selected, selected);
    let p = presentation(&shell).unwrap();
    assert_eq!(p.people[0].status, "Access verified");
    assert!(p.can_share);
    guard.update(|e| {
        e.intent
            .observe_shared_access("W00000003", &json!({"ok":true,"can_see":true}))
            .unwrap();
    });
    assert!(
        share_candidates(&entry(&shell).unwrap())
            .unwrap()
            .is_empty()
    );
    assert!(!presentation(&shell).unwrap().can_share);
    let attempt = admit(&mut shell, &guard, TaskAssignmentDecision::ExistingAccess).unwrap();
    assert_eq!(attempt.edit.cells()[0]["user"], json!(selected));
    assert!(registry(|r| r.shares.is_empty()));
}
