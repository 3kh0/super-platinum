//! Pure retained-selection checks with the actual Shell/Home/Preflight types.
//! No directory transport or native proof is fabricated by this test setup.
use super::*;
use serde_json::json;

fn fixture() -> (ShellState, Selection, Preflight) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let home = h.home.clone().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"actions","block_id":"workflow","elements":[{
        "type":"workflow_button","action_id":"run","text":{"type":"plain_text","text":"Run"},
        "workflow":{"trigger":{"url":"https://slack.com/shortcuts/Ft12345678"}}
    }]}]);
    h.view = Some(View::decode(&home, raw).unwrap());
    inputs::hydrate(&mut s);
    let h = s.app_home.surface.as_ref().unwrap();
    let workflow = Workflow::capture(
        h.view.as_ref().unwrap(),
        "workflow",
        "run",
        &h.owner.ctx.session.user_id,
    )
    .unwrap();
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
    let info = json!({
        "third_party_auths":{"is_shown":false,"is_satisfied":true,"auths":[]},
        "missing_context_parameters":{"is_shown":false,"context_parameters":[]},
        "missing_customizable_input_parameters":{"is_shown":true,"customizable_input_parameters":[{"name":"person","type":"slack#/types/user_id"}]},
        "scopes_acknowledgements":{"is_shown":false,"bot_scopes":[],"user_scopes":[]}
    });
    let TripResult::Review(mut review) = workflow
        .response(json!({"ok":false,"error":"pretrip_ack_required","pre_trip_info":info}))
        .unwrap()
    else {
        panic!("review")
    };
    review.set_custom("person", "UENTITY2".into()).unwrap();
    let user =
        DirectoryUser::decode(&json!({"id":"UENTITY2","profile":{"display_name":"Exact result"}}))
            .unwrap();
    let selection = Selection {
        origin,
        scope: None,
        local: None,
        channel: None,
        field: "person".into(),
        custom: true,
        query: "Exact".into(),
        label: "Exact result".into(),
        proof: Proof::User {
            user,
            agents: None,
            cache: None,
        },
    };
    assert!(selection.current(&s));
    (s, selection, review)
}

#[test]
fn retained_id_is_bound_to_exact_field_kind_and_selected_value() {
    let (s, selection, mut review) = fixture();
    assert!(selection.current(&s));
    assert!(selection.matches(&review, true, "person"));
    assert!(!selection.matches(&review, false, "person"));
    assert!(!selection.matches(&review, true, "other"));
    review.set_custom("person", "UOTHER".into()).unwrap();
    assert!(!selection.matches(&review, true, "person"));
    assert_eq!(selection.id(), "UENTITY2");
}

#[test]
fn later_member_ingress_invalidates_instead_of_becoming_selection_authority() {
    let (mut s, selection, _) = fixture();
    let team = selection.origin.owner.ctx.session.team_id.clone();
    let user = serde_json::from_value(json!({"id":"UENTITY2","deleted":true})).unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .insert("UENTITY2".into(), user);
    assert!(!selection.current(&s));
    assert_eq!(selection.id(), "UENTITY2");
    assert_eq!(selection.label(), "Exact result");
}

#[test]
fn account_or_home_revision_replacement_refuses_retained_selection() {
    let (mut s, selection, _) = fixture();
    s.app_home.surface.as_mut().unwrap().revision += 1;
    assert!(!selection.current(&s));
    let (mut s, selection, _) = fixture();
    s.core.account_epoch += 1;
    assert!(!selection.current(&s));
}

#[test]
fn labels_do_not_replace_entity_ids_and_preserve_unicode() {
    assert_eq!(
        user_label(&json!({"id":"U1","profile":{"display_name":"Ada 🦉"}})),
        "Ada 🦉"
    );
    assert_eq!(
        user_label(&json!({"id":"W1","profile":{"display_name":"","real_name":"Grace"}})),
        "Grace"
    );
    assert_eq!(user_label(&json!({"id":"U2"})), "U2");
}
