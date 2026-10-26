//! Real shared queue/lease projection with typed single-conversation instances.
use super::*;
use super_platinum_core::slack::client::RequestBody;
fn key() -> InputKey {
    InputKey {
        block_id: "destination".into(),
        action_id: "pick".into(),
    }
}
fn setup() -> (ShellState, Binding, u64) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some("EHOME0001".into());
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"destination","dispatch_action":true,"label":{"type":"plain_text","text":"Member"},"element":{"type":"conversations_select","action_id":"pick","initial_conversation":"CINITIAL1","response_url_enabled":true}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Before"}}
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
    let instance =
        super::super::channels::dispatch::proof(&s)["conversationWidgets"][0]["instance"]
            .as_u64()
            .unwrap();
    assert!(b.current(&s));
    assert!(super::super::channels::dispatch::instance_current(
        &s, &b, instance
    ));
    (s, b, instance)
}
fn event(
    s: &mut ShellState,
    b: &Binding,
    id: &str,
) -> super_platinum_core::block_actions::form::home::StaticEvent {
    s.app_home
        .forms
        .get_mut(&b.form)
        .unwrap()
        .conversation_input(&b.key, Some(id.into()))
        .unwrap()
        .unwrap()
}
#[test]
fn actual_shared_window_keeps_valid_action_through_required_null_and_latest_sibling() {
    let (mut s, b, instance) = setup();
    let a = event(&mut s, &b, "USELECT01");
    let (_, first, leading) = enqueue_typed(&mut s, b.clone(), instance, a).unwrap();
    let leading = leading.unwrap();
    assert_eq!(leading.edge, "leading");
    let beta = event(&mut s, &b, "GSELECT02");
    let (_, last, none) = enqueue_typed(&mut s, b.clone(), instance, beta).unwrap();
    assert!(none.is_none());
    assert_ne!(first, last);
    let equal = event(&mut s, &b, "GSELECT02");
    let (_, equal_id, none) = enqueue_typed(&mut s, b.clone(), instance, equal).unwrap();
    assert!(none.is_none());
    assert_ne!(last, equal_id);
    let last = equal_id;
    let index = (b.form.clone(), key());
    let deadline_origin = s.app_home.static_select.windows[&index].last;
    assert!(
        s.app_home
            .forms
            .get_mut(&b.form)
            .unwrap()
            .conversation_input(&b.key, None)
            .unwrap()
            .is_none()
    );
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
    let q = s.app_home.static_select.windows[&(b.form.clone(), key())]
        .trailing
        .as_ref()
        .unwrap();
    assert_eq!(q.id, last);
    assert_eq!(
        s.app_home.static_select.windows[&index].last,
        deadline_origin
    );
    assert_eq!(q.edge, "trailing");
    assert!(q.current(&s));
    assert!(leading.current(&s));
    let request = q
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
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
    let state: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(actions[0]["selected_conversation"], "GSELECT02");
    assert_eq!(actions[0]["initial_conversation"], "CINITIAL1");
    assert_eq!(actions[0]["response_url_enabled"], true);
    assert_eq!(
        state["values"]["destination"]["pick"]["response_url_enabled"],
        true
    );
    assert!(state["values"]["destination"]["pick"]["selected_conversation"].is_null());
    assert_eq!(state["values"]["notes"]["edit"]["value"], "Latest");
    // No fabricated browser-native Conversation entry is needed for sibling custody.
    let expected = bridge::expected_visit(&s, q.binding.visit).unwrap();
    assert_eq!(expected["fields"], json!([]));
}
#[test]
fn pending_disables_picker_without_rejecting_supplied_callback_or_queued_event() {
    let (mut s, b, instance) = setup();
    let action = event(&mut s, &b, "USELECT01");
    let (_, _, leading) = enqueue_typed(&mut s, b.clone(), instance, action).unwrap();
    let leading = leading.unwrap();
    let route = context::Witness::capture(&s).unwrap();
    s.app_home.static_select.activity.insert(
        (b.form.clone(), key().block_id),
        Activity {
            binding: b.clone(),
            route,
            loading: true,
            error: None,
        },
    );
    assert!(super::super::channels::dispatch::disabled(&s, &key()));
    assert!(leading.current(&s));
    let action = event(&mut s, &b, "GSELECT02");
    assert!(
        enqueue_typed(&mut s, b.clone(), instance, action)
            .unwrap()
            .2
            .is_none()
    );
    let activity = s
        .app_home
        .static_select
        .activity
        .get_mut(&(b.form.clone(), key().block_id))
        .unwrap();
    activity.loading = false;
    assert!(!super::super::channels::dispatch::disabled(&s, &key()));
    assert!(leading.current(&s));
}
#[test]
fn reset_and_raw_source_change_spend_queue_identity_without_new_manager() {
    let (mut s, b, instance) = setup();
    let action = event(&mut s, &b, "USELECT01");
    let (_, _, leading) = enqueue_typed(&mut s, b.clone(), instance, action).unwrap();
    let leading = leading.unwrap();
    super::super::channels::dispatch::reset(&mut s, &b.form);
    assert!(!leading.current(&s));
    super::super::channels::hydrate(&mut s);
    assert!(!leading.current(&s));
    let h = s.app_home.surface.as_mut().unwrap();
    h.revision = next();
    assert!(!b.current(&s));
}

#[test]
fn route_retirement_spends_queue_but_not_the_local_widget_or_parent() {
    let (mut s, b, instance) = setup();
    let action = event(&mut s, &b, "USELECT01");
    let (_, _, leading) = enqueue_typed(&mut s, b.clone(), instance, action).unwrap();
    let q = leading.unwrap();
    let before = form(&s).unwrap().state().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .clear();
    assert!(!q.current(&s));
    assert!(super::super::channels::dispatch::instance_current(
        &s, &b, instance
    ));
    assert_eq!(form(&s).unwrap().state().unwrap(), before);
}
