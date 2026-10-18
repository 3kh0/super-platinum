//! Real worker, real private SQLite, closed synthetic capability. No credentials.
use super::*;

fn new_direct_seed(
    temp: &Temp,
    case: Case,
) -> (ShellState, crate::file_actions::Target, Capability) {
    let (mut shell, target, _) = seed(temp, case);
    let cap = Capability::for_test(temp.0.clone(), case, 7, 11)
        .unwrap()
        .with_conversation(fixture::Conversation::NewDirect);
    shell.file_share_fixture = Some(cap.clone());
    let workspace = shell.core.workspaces.get_mut(fixture::TEAM).unwrap();
    workspace.channels.remove("D00000001");
    workspace.users.insert(
        "U00000002".into(),
        serde_json::from_value(fixture::peer()).unwrap(),
    );
    (shell, target, cap)
}

#[tokio::test]
async fn person_selection_is_local_then_one_share_resolves_and_keeps_attempt_custody() {
    for case in [Case::Accepted, Case::Unconfirmed] {
        let temp = Temp::new(case);
        let (mut shell, target, cap) = new_direct_seed(&temp, case);
        let active = shell.core.active_channel.clone();
        let id = open_selected(&mut shell, target).await;
        assert!(
            destinations(&shell, "Alice")
                .iter()
                .any(|d| d.id == "person:U00000002")
        );
        choose(&mut shell, &id, "person:U00000002").unwrap();
        assert_eq!(cap.direct_evidence().unwrap()["count"], 0);
        assert!(
            !shell.core.workspaces[fixture::TEAM]
                .channels
                .contains_key("D00000001")
        );
        assert!(destination_current(&shell, visible(&shell).unwrap()));
        let (mut dom, run) = mount(shell, id);
        until(&mut dom, || cap.evidence().unwrap().awaiting_release).await;
        assert_eq!(cap.direct_evidence().unwrap()["count"], 1);
        let attempt = cap.evidence().unwrap().attempt.unwrap();
        cap.release(&attempt).unwrap();
        until(&mut dom, || run.done.borrow().is_some()).await;
        assert_eq!(*run.done.borrow(), Some(Ok(())));
        let signal = run.signal.get().unwrap();
        let shell = signal.peek();
        assert_eq!(shell.core.active_channel, active);
        assert!(shell.core.session.is_none() && shell.core.transport.is_none());
        let receipt = shell
            .file_share
            .dialog
            .as_ref()
            .unwrap()
            .receipt
            .as_ref()
            .unwrap();
        assert_eq!(receipt.scope.destination, "D00000001");
        assert_eq!(receipt.intent.note(), Some(note().as_slice()));
        let j = Journal::open(cap.storage_path(), fixture::ACCOUNT).unwrap();
        assert!(j.get(&receipt.scope).unwrap().as_ref() == Some(receipt));
        assert_eq!(receipt.unresolved(), case == Case::Unconfirmed);
        assert_eq!(cap.direct_evidence().unwrap()["count"], 1);
    }
}

#[tokio::test]
async fn changed_selected_person_never_opens_or_shares() {
    let temp = Temp::new(Case::Accepted);
    let (mut shell, target, cap) = new_direct_seed(&temp, Case::Accepted);
    let id = open_selected(&mut shell, target).await;
    choose(&mut shell, &id, "person:U00000002").unwrap();
    shell
        .core
        .workspaces
        .get_mut(fixture::TEAM)
        .unwrap()
        .users
        .get_mut("U00000002")
        .unwrap()
        .deleted = true;
    assert!(!destination_current(&shell, visible(&shell).unwrap()));
    let (mut dom, run) = mount(shell, id);
    until(&mut dom, || run.done.borrow().is_some()).await;
    assert_eq!(*run.done.borrow(), Some(Err(Refusal::Changed)));
    assert_eq!(cap.direct_evidence().unwrap()["count"], 0);
    assert!(cap.evidence().unwrap().begin_attempt.is_none());
}
