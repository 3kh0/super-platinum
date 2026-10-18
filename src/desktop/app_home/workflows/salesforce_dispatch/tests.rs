//! Actual parent/controller settlement under a disconnected Shell fixture.
//! Dispatch/native leases are not fabricated or exercised by these unit tests;
//! the exact pending marker represents a controlled transport result boundary.
use super::*;
use serde_json::json;
use super_platinum_core::block_actions::home::workflow::salesforce::ListReason;

fn setup() -> (ShellState, u64, Ticket, Action) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    // Only app_home::fixture is compiled in the ordinary desktop test graph.
    // Install its synthetic account, then capture a matching disconnected owner
    // after changing the native service origin required by Salesforce.
    assert!(s.core.session.is_none());
    crate::app_home::fixture::install(&mut s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .url = "https://fixture.slack.com".into();
    s.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::disconnected_fixture("synthetic"),
    ));
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    h.revision = next();
    let home = h.home.clone().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"actions","block_id":"workflow","elements":[{"type":"workflow_button","action_id":"run","text":{"type":"plain_text","text":"Run"},"workflow":{"trigger":{"url":"https://slack.com/shortcuts/Ft12345678"}}}]}]);
    h.view = Some(View::decode(&home, raw).unwrap());
    inputs::hydrate(&mut s);
    let h = s.app_home.surface.as_ref().unwrap();
    let current = h.view.clone().unwrap();
    let workflow =
        Workflow::capture(&current, "workflow", "run", &h.owner.ctx.session.user_id).unwrap();
    let origin = Origin {
        owner: h.owner.clone(),
        key: Key {
            visit: h.id,
            revision: h.revision,
        },
        workflow: workflow.clone(),
        input_revision: inputs::revision(&s),
    };
    assert!(origin.current(&s));
    let raw = json!({"third_party_auths":{"is_shown":true,"is_satisfied":false,"auths":[{"app_id":"A04T99UKKQE","function_id":"Fn1","provider_key":"salesforce","provider_name":"Salesforce","salesforce_org_id":"00D1","valid_tokens":[]}]},"missing_context_parameters":{"is_shown":false,"context_parameters":[]},"missing_customizable_input_parameters":{"is_shown":false,"customizable_input_parameters":[]},"scopes_acknowledgements":{"is_shown":false,"bot_scopes":[],"user_scopes":[]}});
    let TripResult::Review(review) = workflow
        .response(json!({"ok":false,"error":"pretrip_ack_required","pre_trip_info":raw}))
        .unwrap()
    else {
        panic!("review")
    };
    let observation = Observation::fixture(origin.clone(),json!({"ok":true,"assignments":{"sfdc_sales_home_user_config_workflow_support":{"group":"on"},"slackforce_connected_app_deprecation_use_org_jwt":{"group":"off"}}})).unwrap();
    let target = Target::capture(
        &review,
        &current,
        &origin.owner.ctx.session,
        "Fn1",
        "salesforce",
        observation.native().context(),
    )
    .unwrap();
    let dialog = next();
    s.app_home.workflow = Some(Dialog {
        id: dialog,
        dismissed: false,
        origin,
        review,
        metadata: None,
        busy: false,
        notice: None,
        page: view::Page::Connections,
        entity_query: 0,
        entity_validation: None,
        entity: None,
        entity_selected: Default::default(),
        salesforce_query: 0,
        salesforce_observation: Some(observation.clone()),
        salesforce_view: Some(("Fn1".into(), "salesforce".into())),
        canvas: None,
        canvas_selected: Default::default(),
    });
    let (instance, Some(Effect::Request { ticket, action })) =
        attach(&mut s, dialog, observation, target).unwrap()
    else {
        panic!("initial")
    };
    mark_sent(&mut s, instance, ticket, action);
    assert!(settle(&mut s, instance, ticket, action, Ok(org("error"))).is_some());
    let Effect::Request { ticket, action } = s.app_home.workflow_salesforce[0]
        .connection
        .sign_in()
        .unwrap()
    else {
        panic!("manual")
    };
    mark_sent(&mut s, instance, ticket, action);
    (s, instance, ticket, action)
}
fn mark_sent(s: &mut ShellState, instance: u64, ticket: Ticket, action: Action) {
    let v = s
        .app_home
        .workflow_salesforce
        .iter_mut()
        .find(|v| v.id == instance)
        .unwrap();
    assert!(v.connection.expects(ticket, action));
    v.pending = Some((ticket, action));
    v.sent = true;
}
fn org(auth: &str) -> Value {
    json!({"ok":true,"salesforce_orgs":[{"salesforce_org_id":"00D1","auth":auth,"name":"Fixture"}]})
}

#[test]
fn cancelled_sent_write_keeps_exact_unknown_and_never_opens_browser() {
    let (mut s, instance, ticket, action) = setup();
    let dialog = s.app_home.workflow.as_ref().unwrap().id;
    cancel(&mut s, Some(dialog));
    assert_eq!(
        s.app_home.workflow_salesforce[0].pending,
        Some((ticket, action))
    );
    assert!(
        settle(
            &mut s,
            instance,
            ticket,
            action,
            Err(Error::Api("synthetic_unknown".into()))
        )
        .is_none()
    );
    let v = &s.app_home.workflow_salesforce[0];
    assert_eq!(v.unresolved, vec![(ticket, action)]);
    assert!(v.cancelled && v.pending.is_none() && v.browser.is_none());
    assert!(unresolved(
        &s,
        &s.app_home.workflow.as_ref().unwrap().origin
    ));
    assert!(!use_current(&mut s, instance));
    assert!(settle(&mut s, instance, ticket, action, Ok(json!({"ok":true}))).is_none());
    assert_eq!(s.app_home.workflow_salesforce[0].unresolved.len(), 1);
}
#[test]
fn cancelled_rate_limit_retires_unsent_instance_and_retains_account_deadline() {
    let (mut s, instance, ticket, action) = setup();
    cancel(&mut s, None);
    assert!(
        settle(
            &mut s,
            instance,
            ticket,
            action,
            Err(Error::RateLimited {
                retry_after_secs: Some(19)
            })
        )
        .is_none()
    );
    assert!(s.app_home.workflow_salesforce.is_empty());
    assert!(cooling(
        &s,
        &s.app_home.workflow.as_ref().unwrap().origin.owner.ctx
    ));
}
#[test]
fn current_actual_read_requires_explicit_disposition_and_cannot_authenticate_workflow() {
    let (mut s, instance, ticket, action) = setup();
    settle(
        &mut s,
        instance,
        ticket,
        action,
        Err(Error::Api("synthetic_unknown".into())),
    );
    assert!(!use_current(&mut s, instance));
    let Effect::Request { ticket, action } = s.app_home.workflow_salesforce[0]
        .connection
        .check()
        .unwrap()
    else {
        panic!("read")
    };
    assert_eq!(action, Action::List(ListReason::ManualRecheck));
    mark_sent(&mut s, instance, ticket, action);
    settle(&mut s, instance, ticket, action, Ok(org("ok")));
    assert_eq!(s.app_home.workflow_salesforce[0].unresolved.len(), 1);
    assert!(!s.app_home.workflow.as_ref().unwrap().review.auth_ready());
    assert!(use_current(&mut s, instance));
    assert!(s.app_home.workflow_salesforce[0].unresolved.is_empty());
    assert!(
        !s.app_home.workflow.as_ref().unwrap().review.auth_ready(),
        "only actual valid_tokens/event can grant workflow readiness"
    );
}

#[test]
fn whole_dialog_dismissal_reopens_exact_review_without_reissuing_trip_or_auth() {
    let (mut s, instance, ticket, action) = setup();
    let d = s.app_home.workflow.as_ref().unwrap();
    let dialog = d.id;
    let origin = d.origin.clone();
    let review = d.review.clone();
    super::super::dismiss(&mut s, dialog);
    assert!(s.app_home.workflow.as_ref().unwrap().dismissed);
    assert!(!dialog_current(&s, dialog));
    assert!(!super::super::locked(&s));
    settle(
        &mut s,
        instance,
        ticket,
        action,
        Err(Error::Api("synthetic_unknown".into())),
    );
    assert!(super::super::resume_connection_review(&mut s, &origin));
    assert!(dialog_current(&s, dialog));
    assert!(s.app_home.workflow.as_ref().unwrap().review == review);
    assert!(s.app_home.workflow_attempts.is_empty());
    assert_eq!(
        s.app_home.workflow_salesforce[0].unresolved,
        vec![(ticket, action)]
    );
    assert!(s.app_home.workflow_salesforce[0].pending.is_none());
    // Reopening settings joins the retained controller and creates only a new
    // read ticket. It cannot issue the old auth call or initial silent sign-in.
    s.app_home.workflow.as_mut().unwrap().salesforce_view =
        Some(("Fn1".into(), "salesforce".into()));
    let old = &s.app_home.workflow_salesforce[0];
    let observation = old.observation.clone();
    let target = old.connection.target().clone();
    let (
        same,
        Some(Effect::Request {
            ticket: read_ticket,
            action: read_action,
        }),
    ) = attach(&mut s, dialog, observation, target).unwrap()
    else {
        panic!("read review")
    };
    assert_eq!(same, instance);
    assert_ne!(read_ticket, ticket);
    assert_eq!(read_action, Action::List(ListReason::ManualRecheck));
    assert!(!use_current(&mut s, instance));
    super::super::dismiss(&mut s, dialog);
    s.core.account_epoch += 1;
    assert!(!super::super::resume_connection_review(&mut s, &origin));
}
