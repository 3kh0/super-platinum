//! Shared queue/actual request preparation for retained xjrl/z+fW62.
use super::*;
use super_platinum_core::slack::client::RequestBody;
fn key() -> InputKey {
    InputKey {
        block_id: "members".into(),
        action_id: "pick".into(),
    }
}
fn setup() -> (ShellState, Binding, u64) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"members","dispatch_action":true,"label":{"type":"plain_text","text":"Members"},"element":{"type":"multi_channels_select","action_id":"pick","initial_channels":[],"max_selected_items":2}},
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
    let instance =
        super::super::channels::dispatch::proof(&s)["multiChannelWidgets"][0]["instance"]
            .as_u64()
            .unwrap();
    assert!(b.current(&s));
    assert!(super::super::channels::dispatch::instance_current(
        &s, &b, instance
    ));
    (s, b, instance)
}
fn callback(
    s: &mut ShellState,
    b: &Binding,
    ids: &[&str],
) -> Option<super_platinum_core::block_actions::form::home::StaticEvent> {
    s.app_home
        .forms
        .get_mut(&b.form)
        .unwrap()
        .multi_channel_input(&b.key, ids.iter().map(|id| (*id).to_owned()).collect())
        .unwrap()
}
fn enqueue(s: &mut ShellState, b: &Binding, instance: u64, ids: &[&str]) -> u64 {
    let event = callback(s, b, ids).unwrap();
    let (_, id, leading) = enqueue_typed(s, b.clone(), instance, event).unwrap();
    assert!(leading.is_none());
    id
}
fn prepared(s: &ShellState, q: &Queued) -> (Value, Value) {
    let b = &q.binding;
    let request = q
        .action
        .prepare_with_context(
            &b.owner.ctx.client,
            &b.owner.ctx.session,
            s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap(),
            form(s).unwrap(),
            "synthetic",
            q.route.team_id(),
        )
        .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    (
        serde_json::from_str(&fields["actions"]).unwrap(),
        serde_json::from_str(&fields["state"]).unwrap(),
    )
}

#[test]
fn first_event_is_trailing_and_equal_array_restarts_timer_identity() {
    let (mut s, b, instance) = setup();
    let first = enqueue(&mut s, &b, instance, &["CSELECT02", "CSELECT01"]);
    let index = (b.form.clone(), b.key.clone());
    assert_eq!(
        s.app_home.static_select.windows[&index]
            .trailing
            .as_ref()
            .unwrap()
            .edge,
        "trailing"
    );
    s.app_home
        .static_select
        .windows
        .get_mut(&index)
        .unwrap()
        .last = Instant::now() - Duration::from_millis(400);
    let restarted_after = Instant::now();
    let second = enqueue(&mut s, &b, instance, &["CSELECT02", "CSELECT01"]);
    assert!(s.app_home.static_select.windows[&index].last >= restarted_after);
    assert_ne!(first, second);
    // The actual timer consumer refuses an earlier wakeup and retains latest.
    assert!(take_trailing(&mut s, &index, first).is_none());
    assert_eq!(s.app_home.static_select.windows[&index].id, second);
    let q = take_trailing(&mut s, &index, second).unwrap();
    assert_eq!(q.id, second);
    assert!(q.current(&s));
    let (action, state) = prepared(&s, &q);
    assert_eq!(
        action[0]["selected_channels"],
        json!(["CSELECT02", "CSELECT01"])
    );
    assert_eq!(action[0]["initial_channels"], json!([]));
    assert!(action[0].get("response_url_enabled").is_none());
    assert!(action[0].get("placeholder").is_none());
    assert_eq!(
        state["values"]["members"]["pick"]["selected_channels"],
        action[0]["selected_channels"]
    );
    assert!(take_trailing(&mut s, &index, second).is_none());
}

#[test]
fn invalid_edits_do_not_reset_valid_window_and_request_uses_latest_invalid_parent() {
    let (mut s, b, instance) = setup();
    let id = enqueue(&mut s, &b, instance, &["CSELECT02"]);
    let index = (b.form.clone(), b.key.clone());
    let deadline_origin = s.app_home.static_select.windows[&index].last;
    assert!(callback(&mut s, &b, &["CSELECT03", "CSELECT02", "CSELECT01"]).is_none());
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
    let q = s.app_home.static_select.windows[&index]
        .trailing
        .as_ref()
        .unwrap();
    assert_eq!(q.id, id);
    assert_eq!(
        s.app_home.static_select.windows[&index].last,
        deadline_origin
    );
    assert!(q.current(&s));
    let (action, state) = prepared(&s, q);
    assert_eq!(action[0]["selected_channels"], json!(["CSELECT02"]));
    assert_eq!(
        state["values"]["members"]["pick"]["selected_channels"],
        json!(["CSELECT03", "CSELECT02", "CSELECT01"])
    );
    assert_eq!(state["values"]["notes"]["edit"]["value"], "Latest");
    assert!(callback(&mut s, &b, &[]).is_none());
    assert_eq!(
        s.app_home.static_select.windows[&index].last,
        deadline_origin
    );
    let q = take_trailing(&mut s, &index, id).unwrap();
    let (action, state) = prepared(&s, &q);
    assert_eq!(action[0]["selected_channels"], json!(["CSELECT02"]));
    assert_eq!(
        state["values"]["members"]["pick"]["selected_channels"],
        json!([])
    );
    assert_eq!(
        bridge::expected_visit(&s, b.visit).unwrap()["fields"],
        json!([])
    );
}

#[test]
fn pending_projection_does_not_reject_supplied_callback_or_immutable_queue() {
    let (mut s, b, instance) = setup();
    let id = enqueue(&mut s, &b, instance, &["CSELECT01"]);
    let index = (b.form.clone(), b.key.clone());
    let q = take_trailing(&mut s, &index, id).unwrap();
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
    assert!(q.current(&s));
    let fresh = enqueue(&mut s, &b, instance, &["CSELECT02"]);
    assert_ne!(id, fresh);
    let (action, state) = prepared(&s, &q);
    assert_eq!(action[0]["selected_channels"], json!(["CSELECT01"]));
    assert_eq!(
        state["values"]["members"]["pick"]["selected_channels"],
        json!(["CSELECT02"])
    );
    s.app_home
        .static_select
        .activity
        .get_mut(&(b.form.clone(), key().block_id))
        .unwrap()
        .loading = false;
    assert!(!super::super::channels::dispatch::disabled(&s, &key()));
    assert_eq!(s.app_home.static_select.windows[&index].id, fresh);
}

#[test]
fn reset_source_and_route_retire_queue_without_invented_channel_native_mount() {
    let (mut s, b, instance) = setup();
    let id = enqueue(&mut s, &b, instance, &["CSELECT01"]);
    let q = take_trailing(&mut s, &(b.form.clone(), key()), id).unwrap();
    super::super::channels::dispatch::reset(&mut s, &b.form);
    assert!(!q.current(&s));
    super::super::channels::hydrate(&mut s);
    assert!(!q.current(&s));
    let (mut s, b, instance) = setup();
    let id = enqueue(&mut s, &b, instance, &["CSELECT01"]);
    let q = take_trailing(&mut s, &(b.form.clone(), key()), id).unwrap();
    s.app_home.surface.as_mut().unwrap().revision = next();
    assert!(!q.current(&s));
    let (mut s, b, instance) = setup();
    let id = enqueue(&mut s, &b, instance, &["CSELECT01"]);
    let q = take_trailing(&mut s, &(b.form.clone(), key()), id).unwrap();
    let parent = form(&s).unwrap().state().unwrap();
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
    assert_eq!(form(&s).unwrap().state().unwrap(), parent);
}
