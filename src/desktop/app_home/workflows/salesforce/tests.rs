use super::*;
use serde_json::json;
use super_platinum_core::{
    block_actions::home::{
        Home, View,
        workflow::{TripResult, Workflow, salesforce::Context},
    },
    config::WorkspaceSession,
};
fn connection(jwt: bool) -> Connection {
    let home = Home {
        channel: "D1".into(),
        app: "A1".into(),
        installed_team: "T1".into(),
        enabled: true,
        messages: true,
        view_id: Some("V1".into()),
    };
    let view=View::decode(&home,json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","hash":"h1","blocks":[{"type":"actions","block_id":"block","elements":[{"type":"workflow_button","action_id":"action","text":{"type":"plain_text","text":"Run"},"workflow":{"trigger":{"url":"https://slack.com/shortcuts/Ft12345678"}}}]}]})).unwrap();
    let workflow = Workflow::capture(&view, "block", "action", "U1").unwrap();
    let info = json!({"third_party_auths":{"is_shown":true,"is_satisfied":false,"auths":[{"app_id":"A04T99UKKQE","function_id":"Fn1","provider_key":"salesforce","provider_name":"Salesforce","salesforce_org_id":"00Dtarget","valid_tokens":[]}]},"missing_context_parameters":{"is_shown":false,"context_parameters":[]},"missing_customizable_input_parameters":{"is_shown":false,"customizable_input_parameters":[]},"scopes_acknowledgements":{"is_shown":false,"bot_scopes":[],"user_scopes":[]}});
    let TripResult::Review(review) = workflow
        .response(json!({"ok":false,"error":"pretrip_ack_required","pre_trip_info":info}))
        .unwrap()
    else {
        panic!("review")
    };
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-not-a-token".into(),
    };
    let context = Context::observe(
        "https://app.slack.com/client/T1",
        &json!("on"),
        &json!(if jwt { "on" } else { "off" }),
    )
    .unwrap();
    Connection::new(
        Target::capture(&review, &view, &workspace, "Fn1", "salesforce", &context).unwrap(),
    )
}
fn request(effect: Effect, expected: Action) -> Ticket {
    let Effect::Request { ticket, action } = effect else {
        panic!("request")
    };
    assert_eq!(action, expected);
    ticket
}
fn org(auth: &str) -> Value {
    json!({"ok":true,"salesforce_orgs":[{"salesforce_org_id":"00Dtarget","auth":auth,"name":"Target"}]})
}
#[test]
fn unsent_ticket_is_consumed_without_uncertain_authentication_or_replay() {
    let mut state = connection(false);
    let initial = request(state.begin().unwrap(), Action::List(ListReason::Initial));
    state.response(initial, org("error")).unwrap();
    let manual = request(state.sign_in().unwrap(), Action::Manual);
    assert!(state.expects(manual, Action::Manual));
    assert!(!state.expects(manual, Action::Disconnect));
    assert!(!state.not_sent(initial));
    assert!(state.expects(manual, Action::Manual));
    assert!(state.not_sent(manual));
    assert!(!state.uncertain());
    assert!(!state.busy());
    assert!(!state.not_sent(manual));
    assert!(state.response(manual, json!({"ok":true})).is_err());
    let replacement = request(state.sign_in().unwrap(), Action::Manual);
    assert_ne!(manual, replacement);
    state.failed(replacement);
    assert!(state.uncertain());
    let read = request(
        state.check().unwrap(),
        Action::List(ListReason::ManualRecheck),
    );
    assert!(state.not_sent(read));
    assert!(
        state.uncertain(),
        "a later unsent read cannot erase an earlier unknown write"
    );
    assert!(state.sign_in().is_none());
}
#[test]
fn cancelled_or_duplicate_response_cannot_advance() {
    let mut state = connection(false);
    let ticket = request(state.begin().unwrap(), Action::List(ListReason::Initial));
    assert!(state.begin().is_none());
    state.cancel();
    assert!(state.response(ticket, org("ok")).is_err());
    assert!(state.sign_in().is_none());
    assert!(state.check().is_none());
    let mut state = connection(false);
    let ticket = request(state.begin().unwrap(), Action::List(ListReason::Initial));
    assert!(state.response(ticket, org("ok")).is_ok());
    assert!(state.response(ticket, org("none")).is_err());
    assert!(state.org().unwrap().connected());
}
#[test]
fn browser_focus_requests_proof_and_never_authenticates() {
    let mut state = connection(true);
    let ticket = request(state.begin().unwrap(), Action::List(ListReason::Initial));
    state.response(ticket, org("error")).unwrap();
    let ticket = request(state.sign_in().unwrap(), Action::LoginUrl);
    let Effect::OpenExternal { ticket, .. } = state
        .response(
            ticket,
            json!({"ok":true,"auth_url":"https://login.salesforce.com/auth"}),
        )
        .unwrap()
    else {
        panic!("browser")
    };
    assert!(!state.org().unwrap().connected());
    let next = request(
        state.browser_returned(ticket).unwrap(),
        Action::List(ListReason::ManualRecheck),
    );
    assert!(!state.org().unwrap().connected());
    assert!(state.browser_returned(ticket).is_none());
    state.response(next, org("none")).unwrap();
    assert_eq!(state.status(), Status::ManualInProgress);
}
#[test]
fn silent_failure_only_reads_and_retains_uncertain_custody() {
    let mut state = connection(false);
    let ticket = request(state.begin().unwrap(), Action::List(ListReason::Initial));
    let ticket = request(state.response(ticket, org("none")).unwrap(), Action::Silent);
    let ticket = request(
        state.failed(ticket).unwrap(),
        Action::List(ListReason::ForcedRefetch),
    );
    state.failed(ticket).unwrap();
    assert!(state.uncertain());
    assert!(state.sign_in().is_none());
    assert!(!state.use_current_connection());
    let ticket = request(
        state.check().unwrap(),
        Action::List(ListReason::ManualRecheck),
    );
    state.response(ticket, org("none")).unwrap();
    assert!(state.uncertain());
    assert!(state.use_current_connection());
    request(state.sign_in().unwrap(), Action::Manual);
}
#[test]
fn disconnect_requires_actual_connected_target_and_rechecks() {
    let mut state = connection(false);
    assert!(state.disconnect().is_none());
    let ticket = request(state.begin().unwrap(), Action::List(ListReason::Initial));
    state.response(ticket, org("ok")).unwrap();
    let ticket = request(state.disconnect().unwrap(), Action::Disconnect);
    assert!(state.disconnect().is_none());
    let ticket = request(
        state.response(ticket, json!({"ok":true})).unwrap(),
        Action::List(ListReason::ManualRecheck),
    );
    state.response(ticket, org("none")).unwrap();
    assert!(!state.org().unwrap().connected());
    assert!(state.disconnect().is_none());
}
