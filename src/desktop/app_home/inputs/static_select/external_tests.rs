//! Shared dispatch timing and sibling inventory for typed external descriptors.
use super::*;
use super_platinum_core::{block_actions::external::ExternalOption, slack::client::RequestBody};

fn key() -> InputKey {
    InputKey {
        block_id: "external".into(),
        action_id: "pick".into(),
    }
}
fn option(label: &str) -> ExternalOption {
    ExternalOption::parse(&json!({"text":{"type":"plain_text","text":label},"value":"same",
        "description":{"type":"plain_text","text":format!("{label} description")},"opaque":{"kept":label}})).unwrap()
}
fn setup() -> (ShellState, Binding, u64) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team, false);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"external","dispatch_action":true,"label":{"type":"plain_text","text":"Option"},
         "element":{"type":"external_select","action_id":"pick","initial_option":option("Initial").raw(),"placeholder":{"type":"plain_text","text":"Choose"}}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},
         "element":{"type":"plain_text_input","action_id":"edit","initial_value":"Before"}}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::hydrate(&mut s);
    let h = s.app_home.surface.as_ref().unwrap();
    let b = Binding {
        owner: h.owner.clone(),
        form: crate::app_home::inputs::owner(&s).unwrap(),
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: key(),
        raw: h.view.as_ref().unwrap().raw().clone(),
    };
    let instance = super::super::external::dispatch::proof(&s)["externalWidgets"][0]["instance"]
        .as_u64()
        .unwrap();
    assert!(b.current(&s));
    assert!(super::super::external::dispatch::instance_current(
        &s, &b, instance
    ));
    (s, b, instance)
}
fn event(
    s: &mut ShellState,
    b: &Binding,
    label: &str,
) -> super_platinum_core::block_actions::form::home::StaticEvent {
    s.app_home
        .forms
        .get_mut(&b.form)
        .unwrap()
        .external_input(&b.key, Some(option(label)))
        .unwrap()
        .unwrap()
}
#[test]
fn external_queue_preserves_descriptor_while_required_null_and_sibling_change_latest_state() {
    let (mut s, b, instance) = setup();
    let a = event(&mut s, &b, "Alpha");
    let (_, _, leading) = enqueue_typed(&mut s, b.clone(), instance, a).unwrap();
    let leading = leading.unwrap();
    assert_eq!(leading.edge, "leading");
    assert!(
        s.app_home.static_select.windows[&(b.form.clone(), key())]
            .trailing
            .is_none()
    );
    let beta = event(&mut s, &b, "Beta");
    let (_, second, none) = enqueue_typed(&mut s, b.clone(), instance, beta).unwrap();
    assert!(none.is_none());
    let equal = event(&mut s, &b, "Beta");
    let (_, last, none) = enqueue_typed(&mut s, b.clone(), instance, equal).unwrap();
    assert!(none.is_none());
    assert_ne!(last, second);
    let index = (b.form.clone(), key());
    let deadline = s.app_home.static_select.windows[&index].last;
    let f = s.app_home.forms.get_mut(&b.form).unwrap();
    assert!(f.external_input(&key(), None).unwrap().is_none());
    f.set(
        &InputKey {
            block_id: "notes".into(),
            action_id: "edit".into(),
        },
        InputValue::Text("Latest".into()),
    )
    .unwrap();
    let w = &s.app_home.static_select.windows[&index];
    assert_eq!(w.last, deadline);
    let q = w.trailing.as_ref().unwrap();
    assert_eq!(q.id, last);
    assert_eq!(q.edge, "trailing");
    assert!(q.current(&s));
    assert!(leading.current(&s));
    let req = q
        .action
        .prepare_with_context(
            &b.owner.ctx.client,
            &b.owner.ctx.session,
            s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap(),
            form(&s).unwrap(),
            "synthetic",
            q.route.team_id(),
        )
        .unwrap();
    let RequestBody::Form(fields) = req.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    let action: Value = serde_json::from_str(&fields["actions"]).unwrap();
    let state: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(action[0]["selected_option"], option("Beta").raw().clone());
    assert_eq!(action[0]["initial_option"], option("Initial").raw().clone());
    assert_eq!(
        action[0]["placeholder"],
        json!({"type":"plain_text","text":"Choose"})
    );
    assert!(state["values"]["external"]["pick"]["selected_option"].is_null());
    assert_eq!(state["values"]["notes"]["edit"]["value"], "Latest");
    // This logical picker never invents a browser-native option entry.
    assert_eq!(
        bridge::expected_visit(&s, b.visit).unwrap()["fields"],
        json!([])
    );
}
#[test]
fn action_pending_disables_external_picker_without_blanket_callback_rejection() {
    let (mut s, b, instance) = setup();
    let a = event(&mut s, &b, "Alpha");
    let (_, _, leading) = enqueue_typed(&mut s, b.clone(), instance, a).unwrap();
    s.app_home.static_select.activity.insert(
        (b.form.clone(), b.key.block_id.clone()),
        Activity {
            binding: b.clone(),
            route: context::Witness::capture(&s).unwrap(),
            loading: true,
            error: None,
        },
    );
    assert!(super::super::external::dispatch::disabled(&s, &key()));
    assert!(leading.unwrap().current(&s));
    let a = event(&mut s, &b, "While pending");
    assert!(enqueue_typed(&mut s, b, instance, a).is_some());
}
#[test]
fn reset_and_raw_source_change_retire_queued_external_instance() {
    let (mut s, b, instance) = setup();
    let a = event(&mut s, &b, "Alpha");
    let (_, _, leading) = enqueue_typed(&mut s, b.clone(), instance, a).unwrap();
    let q = leading.unwrap();
    super::super::external::dispatch::reset(&mut s, &b.form);
    assert!(!q.current(&s));
    let (mut s, b, instance) = setup();
    let a = event(&mut s, &b, "Alpha");
    let (_, _, leading) = enqueue_typed(&mut s, b.clone(), instance, a).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!("changed-source");
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    assert!(!leading.unwrap().current(&s));
}
