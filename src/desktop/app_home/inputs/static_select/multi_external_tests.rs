//! Multi-external uses the shared window, retaining old parent at local overmax.
use super::*;
use super_platinum_core::{block_actions::external::ExternalOption, slack::client::RequestBody};
fn key() -> InputKey {
    InputKey {
        block_id: "external".into(),
        action_id: "pick".into(),
    }
}
fn option(label: &str) -> ExternalOption {
    ExternalOption::parse(&json!({"text":{"type":"plain_text","text":label},"value":label,"description":{"type":"plain_text","text":"Full description"},"opaque":{"label":label}})).unwrap()
}
fn setup() -> (ShellState, Binding, u64) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"external","dispatch_action":true,"label":{"type":"plain_text","text":"Options"},"element":{"type":"multi_external_select","action_id":"pick","initial_options":[],"max_selected_items":2,"placeholder":{"type":"plain_text","text":"Choose"}}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Before"}}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::hydrate(&mut s);
    let h = s.app_home.surface.as_ref().unwrap();
    let b = Binding {
        owner: h.owner.clone(),
        form: owner(&s).unwrap(),
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: key(),
        raw: h.view.as_ref().unwrap().raw().clone(),
    };
    let instance = super::super::external::dispatch::proof(&s)["multiExternalWidgets"][0]["widget"]
        ["instance"]
        .as_u64()
        .unwrap();
    assert!(b.current(&s));
    assert!(super::super::external::dispatch::instance_current(
        &s, &b, instance
    ));
    (s, b, instance)
}
fn callback(
    s: &mut ShellState,
    b: &Binding,
    names: &[&str],
) -> Option<super_platinum_core::block_actions::form::home::StaticEvent> {
    s.app_home
        .forms
        .get_mut(&b.form)
        .unwrap()
        .multi_external_input(&b.key, names.iter().map(|x| option(x)).collect())
        .unwrap()
}
#[test]
fn no_leading_and_latest_valid_callback_restarts_while_invalids_preserve_deadline() {
    let (mut s, b, instance) = setup();
    let a = callback(&mut s, &b, &["A"]).unwrap();
    let (_, first, leading) = enqueue_typed(&mut s, b.clone(), instance, a).unwrap();
    assert!(leading.is_none());
    let beta = callback(&mut s, &b, &["B", "A"]).unwrap();
    let (_, last, leading) = enqueue_typed(&mut s, b.clone(), instance, beta).unwrap();
    assert!(leading.is_none());
    assert_ne!(first, last);
    let index = (b.form.clone(), b.key.clone());
    let deadline = s.app_home.static_select.windows[&index].last;
    let parent_before = form(&s).unwrap().state().unwrap();
    assert!(callback(&mut s, &b, &["A", "B", "C"]).is_none());
    assert_eq!(form(&s).unwrap().state().unwrap(), parent_before);
    assert!(callback(&mut s, &b, &[]).is_none());
    s.app_home
        .forms
        .get_mut(&b.form)
        .unwrap()
        .set(
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
    let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
    let state: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(
        actions[0]["selected_options"],
        json!([option("B").raw(), option("A").raw()])
    );
    assert_eq!(actions[0]["initial_options"], json!([]));
    assert_eq!(actions[0]["placeholder"]["text"], "Choose");
    assert_eq!(
        state["values"]["external"]["pick"]["selected_options"],
        json!([])
    );
    assert_eq!(state["values"]["notes"]["edit"]["value"], "Latest");
    assert_eq!(
        bridge::expected_visit(&s, b.visit).unwrap()["fields"],
        json!([])
    );
}
#[test]
fn pending_disables_multi_ui_without_rejecting_admitted_callback_and_reset_retires_instance() {
    let (mut s, b, instance) = setup();
    let e = callback(&mut s, &b, &["A"]).unwrap();
    enqueue_typed(&mut s, b.clone(), instance, e).unwrap();
    let route = context::Witness::capture(&s).unwrap();
    s.app_home.static_select.activity.insert(
        (b.form.clone(), b.key.block_id.clone()),
        Activity {
            binding: b.clone(),
            route,
            loading: true,
            error: None,
        },
    );
    assert!(super::super::external::dispatch::disabled(&s, &key()));
    let e = callback(&mut s, &b, &["B"]).unwrap();
    assert!(enqueue_typed(&mut s, b.clone(), instance, e).is_some());
    let q = s.app_home.static_select.windows[&(b.form.clone(), b.key.clone())]
        .trailing
        .as_ref()
        .unwrap()
        .clone();
    super::super::external::dispatch::reset(&mut s, &b.form);
    assert!(!q.current(&s));
    assert!(!super::super::external::dispatch::instance_current(
        &s, &b, instance
    ));
}
