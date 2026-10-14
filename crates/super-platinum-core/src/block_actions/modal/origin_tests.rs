use super::*;
use serde_json::json;

const C1: &str = "C12345678";
const C2: &str = "C87654321";
fn scope() -> ModalScope {
    ModalScope {
        account_epoch: 1,
        team: "T1".into(),
        realtime_generation: 7,
    }
}
fn view(id: &str, previous: Option<&str>) -> Value {
    json!({"id":id,"type":"modal","app_id":"A1","previous_view_id":previous,"root_view_id":"V1","blocks":[]})
}
fn open(token: &str, id: &str, previous: Option<&str>) -> RawEvent {
    serde_json::from_value(json!({"type":"view_opened","view_id":id,"app_id":"A1",
        "view_type":"modal","client_token":token,"previous_view_id":previous,
        "timeout_range":500,"view":view(id,previous)}))
    .unwrap()
}
fn enqueue(state: &mut ModalState, token: &str, origin: Option<&str>) -> bool {
    state.enqueue_with_origin(
        &scope(),
        token.into(),
        "A1".into(),
        origin.map(str::to_owned),
        100,
        1000,
    )
}
fn origin(state: &ModalState) -> Option<&str> {
    state.stack().last().unwrap().origin_conversation.as_deref()
}

#[test]
fn opening_origin_belongs_to_the_exact_correlated_token_not_the_latest_action() {
    let mut state = ModalState::new(scope());
    assert!(enqueue(&mut state, "first", Some(C1)));
    assert!(enqueue(&mut state, "second", Some(C2)));
    assert_eq!(
        state.apply(&scope(), &open("first", "V1", None), 101),
        ModalChange::Opened
    );
    assert_eq!(origin(&state), Some(C1));
    assert_eq!(
        state.apply(&scope(), &open("second", "V2", Some("V1")), 102),
        ModalChange::Ignored
    );
    assert_eq!(state.stack().len(), 1);
    assert!(!state.tokens[0].opened.iter().any(|id| id == "V2"));
    assert_eq!(
        state.apply(&scope(), &open("first", "V2", Some("V1")), 102),
        ModalChange::Opened
    );
    assert_eq!(origin(&state), Some(C1));
}

#[test]
fn origin_survives_hydration_updates_submission_push_and_back() {
    let mut state = ModalState::new(scope());
    assert!(enqueue(&mut state, "token", Some(C1)));
    let mut frame = open("token", "V1", None);
    frame.rest.remove("view");
    assert_eq!(state.apply(&scope(), &frame, 101), ModalChange::Opened);
    assert_eq!(origin(&state), Some(C1));
    let revision = state.revision();
    let mut payload = view("V1", None);
    // Untrusted server payload cannot replace the locally captured lineage.
    payload["origin_conversation"] = json!(C2);
    assert!(state.hydrate_current(&scope(), "V1", revision, payload.clone()));
    assert_eq!(origin(&state), Some(C1));
    let update: RawEvent = serde_json::from_value(
        json!({"type":"view_updated","view_id":"V1","app_id":"A1","view":payload}),
    )
    .unwrap();
    assert_eq!(state.apply(&scope(), &update, 102), ModalChange::Updated);
    let revision = state.revision();
    assert!(state.apply_submission(
        &scope(),
        "V1",
        revision,
        &BlockViewResponseAction::Update(view("V1", None))
    ));
    assert_eq!(origin(&state), Some(C1));
    let revision = state.revision();
    assert!(state.apply_submission(
        &scope(),
        "V1",
        revision,
        &BlockViewResponseAction::Push(view("V2", Some("V1")))
    ));
    assert_eq!(origin(&state), Some(C1));
    assert_eq!(state.apply(&scope(), &update, 103), ModalChange::Updated);
    assert!(state.pop_local(&scope()));
    assert_eq!(origin(&state), Some(C1));
    assert!(state.enqueue(&scope(), "compatibility".into(), "A1".into(), 104, 1000));
    assert_eq!(
        state.apply(&scope(), &open("compatibility", "V3", Some("V1")), 105),
        ModalChange::Opened
    );
    assert_eq!(origin(&state), Some(C1));
}

#[test]
fn unknown_origin_cannot_be_replaced_by_a_later_action_and_scope_reset_drops_it() {
    let mut state = ModalState::new(scope());
    assert!(state.enqueue(&scope(), "root".into(), "A1".into(), 100, 1000));
    assert_eq!(
        state.apply(&scope(), &open("root", "V1", None), 101),
        ModalChange::Opened
    );
    assert_eq!(origin(&state), None);
    assert!(enqueue(&mut state, "other", Some(C1)));
    assert_eq!(
        state.apply(&scope(), &open("other", "V2", Some("V1")), 102),
        ModalChange::Ignored
    );
    let revision = state.revision();
    assert!(state.apply_submission(
        &scope(),
        "V1",
        revision,
        &BlockViewResponseAction::Push(view("V2", Some("V1")))
    ));
    assert_eq!(origin(&state), None);
    let next = ModalScope {
        account_epoch: 2,
        ..scope()
    };
    state.reset_scope(next.clone());
    assert_eq!(state.token_count(), 0);
    assert!(state.stack().is_empty());
    assert_eq!(
        state.apply(&next, &open("other", "V1", None), 103),
        ModalChange::Ignored
    );
    assert!(state.enqueue_with_origin(
        &next,
        "new".into(),
        "A1".into(),
        Some(C2.into()),
        104,
        1000
    ));
    assert_eq!(
        state.apply(&next, &open("new", "V1", None), 105),
        ModalChange::Opened
    );
    assert_eq!(origin(&state), Some(C2));
}

#[test]
fn origin_registration_keeps_identity_expiry_and_replay_guards() {
    let mut state = ModalState::new(scope());
    for invalid in ["", "C1", " C12345678"] {
        assert!(!enqueue(&mut state, "invalid", Some(invalid)));
    }
    assert!(enqueue(&mut state, "token", Some(C1)));
    assert!(!enqueue(&mut state, "token", Some(C2)));
    let wrong = ModalScope {
        realtime_generation: 8,
        ..scope()
    };
    assert!(!state.enqueue_with_origin(
        &wrong,
        "wrong".into(),
        "A1".into(),
        Some(C2.into()),
        100,
        1000
    ));
    assert_eq!(
        state.apply(&wrong, &open("token", "V1", None), 101),
        ModalChange::Ignored
    );
    assert_eq!(
        state.apply(&scope(), &open("token", "V1", None), 600),
        ModalChange::Ignored
    );
    assert_eq!(
        state.apply(&scope(), &open("token", "V1", None), 102),
        ModalChange::Opened
    );
    assert_eq!(origin(&state), Some(C1));
    assert!(state.pop_local(&scope()));
    assert_eq!(
        state.apply(&scope(), &open("token", "V1", None), 103),
        ModalChange::Ignored
    );
    assert!(state.stack().is_empty());
}
