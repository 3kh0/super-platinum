//! Pure model/snapshot checks only: no native DOM lease, event or HTTP proof.
use super::super::fixture as text_fixture;
use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::client::RequestBody;

fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    text_fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"].as_array_mut().unwrap().push(json!({"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"submit","text":{"type":"plain_text","text":"Review"},"value":"send"}]}));
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    inputs::hydrate(&mut s);
    for block in ["enter", "characters"] {
        mount(&mut s, block);
    }
    assert!(ready(&s));
    s
}
fn binding(s: &ShellState, block: &str) -> Binding {
    let h = s.app_home.surface.as_ref().unwrap();
    Binding {
        owner: h.owner.clone(),
        form: owner(s).unwrap(),
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: text_fixture::key(block),
        raw: h.view.as_ref().unwrap().raw().clone(),
    }
}
fn mount(s: &mut ShellState, block: &str) {
    let b = binding(s, block);
    let InputValue::Text(value) = &form(s).unwrap().fields[&b.key].value else {
        panic!("text")
    };
    let value = value.clone();
    s.app_home.text.mounts.insert(
        (b.form.clone(), b.key.clone()),
        Mount {
            id: next(),
            seq: 1,
            value,
            binding: b,
        },
    );
}
fn expected(s: &ShellState) -> Value {
    bridge::expected_visit(s, binding(s, "enter").visit).unwrap()
}
fn row<'a>(v: &'a Value, block: &str) -> &'a Value {
    let id = crate::view::app_modal::field_id(&text_fixture::key(block));
    v["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap()
}
fn edit(s: &mut ShellState, b: &Binding, kind: &str, value: &str) -> Option<u64> {
    let m = &s.app_home.text.mounts[&(b.form.clone(), b.key.clone())];
    let e = Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: kind.into(),
        value: value.into(),
        key: (kind == "enter").then(|| "Enter".into()),
        code: None,
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
        repeat: false,
        composing: false,
        trusted: false,
        prevented: kind == "enter",
        number: None,
        email: None,
    };
    receive(s, b, &e)
}

#[test]
fn mixed_snapshot_names_all_text_but_not_calendar_and_keeps_quiet_literal() {
    let mut s = setup();
    assert!(needed(&s));
    let o = owner(&s).unwrap();
    s.app_home
        .forms
        .get_mut(&o)
        .unwrap()
        .set(
            &text_fixture::key("quiet"),
            InputValue::Text("  quiet literal  ".into()),
        )
        .unwrap();
    let v = expected(&s);
    assert_eq!(v["fields"].as_array().unwrap().len(), 3);
    for block in ["enter", "characters"] {
        let b = binding(&s, block);
        let m = &s.app_home.text.mounts[&(b.form, b.key)];
        assert_eq!(row(&v, block)["mount"], m.id);
        assert_eq!(row(&v, block)["seq"], 1);
    }
    assert_eq!(row(&v, "quiet")["value"], "  quiet literal  ");
    assert!(row(&v, "quiet")["mount"].is_null());
    assert!(row(&v, "quiet")["seq"].is_null());
    assert_eq!(
        form(&s).unwrap().state().unwrap()["values"]["quiet"]["value"]["value"],
        "quiet literal"
    );
}

#[test]
fn quiet_only_form_keeps_legacy_admission_without_text_mounts() {
    let mut s = setup();
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    for block in raw["blocks"].as_array_mut().unwrap() {
        block.as_object_mut().unwrap().remove("dispatch_action");
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(&mut s);
    s.app_home.text.mounts.clear();
    assert!(!needed(&s));
    assert!(ready(&s));
    assert_eq!(expected(&s)["fields"].as_array().unwrap().len(), 3);
}

#[test]
fn unacknowledged_missing_and_diverged_mounts_refuse_snapshot() {
    for change in ["missing", "unacknowledged", "diverged"] {
        let mut s = setup();
        let b = binding(&s, "enter");
        let index = (b.form.clone(), b.key.clone());
        match change {
            "missing" => {
                s.app_home.text.mounts.remove(&index);
            }
            "unacknowledged" => s.app_home.text.mounts.get_mut(&index).unwrap().seq = 0,
            "diverged" => {
                s.app_home.text.mounts.get_mut(&index).unwrap().value = "unreceived".into()
            }
            _ => unreachable!(),
        }
        assert!(bridge::expected_visit(&s, b.visit).is_none(), "{change}");
    }
}

#[test]
fn same_literal_new_sequence_or_mount_retires_prior_snapshot() {
    let mut s = setup();
    let b = binding(&s, "enter");
    let before = expected(&s);
    assert!(edit(&mut s, &b, "input", "Initial").is_none());
    let after = expected(&s);
    assert_eq!(
        row(&before, "enter")["value"],
        row(&after, "enter")["value"]
    );
    assert_ne!(before, after);
    mount(&mut s, "enter");
    let remounted = expected(&s);
    assert_ne!(
        row(&after, "enter")["mount"],
        row(&remounted, "enter")["mount"]
    );
    assert_ne!(after, remounted);
}

#[test]
fn source_account_socket_and_visit_retirement_refuse_old_snapshot() {
    for change in ["source", "account", "socket", "visit"] {
        let mut s = setup();
        let b = binding(&s, "enter");
        assert!(bridge::expected_visit(&s, b.visit).is_some());
        match change {
            "source" => {
                let h = s.app_home.surface.as_mut().unwrap();
                let mut raw = h.view.as_ref().unwrap().raw().clone();
                raw["hash"] = json!("replaced");
                h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
                h.revision = next();
            }
            "account" => s.core.account_epoch += 1,
            "socket" => {
                let team = s.core.active_team.clone().unwrap();
                s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
            }
            "visit" => s.app_home.surface.as_mut().unwrap().id = next(),
            _ => unreachable!(),
        }
        assert!(bridge::expected_visit(&s, b.visit).is_none(), "{change}");
    }
}

#[test]
fn button_snapshot_stays_immutable_while_later_text_event_owns_latest_form() {
    let mut s = setup();
    let b = binding(&s, "enter");
    let view = s
        .app_home
        .surface
        .as_ref()
        .unwrap()
        .view
        .as_ref()
        .unwrap()
        .clone();
    let button = view
        .button_with_form(
            "buttons",
            "submit",
            &b.owner.ctx.session.user_id,
            form(&s).unwrap(),
        )
        .unwrap();
    assert!(button.same_form(form(&s).unwrap()));
    let before = expected(&s);
    assert!(edit(&mut s, &b, "input", "  after  ").is_none());
    assert!(!button.same_form(form(&s).unwrap()));
    assert_ne!(before, expected(&s));
    edit(&mut s, &b, "enter", "  literal  ").unwrap();
    let q = &s.app_home.text.queued[&(b.form.clone(), b.key.clone())];
    assert_eq!(q.action.action()["value"], "  literal  ");
    assert!(q.current(&s));
    let request = button
        .prepare_with_context(
            &b.owner.ctx.client,
            &b.owner.ctx.session,
            &view,
            "synthetic-button",
            None,
        )
        .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    let sent: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(sent["values"]["enter"]["value"]["value"], "Initial");
    assert_eq!(
        form(&s).unwrap().state().unwrap()["values"]["enter"]["value"]["value"],
        "  literal  "
    );
}

#[test]
fn failed_lease_retry_requires_received_text_progress_not_unchanged_or_replaced_mount() {
    let mut s = setup();
    inputs::native::seed(&mut s);
    let b = binding(&s, "enter");
    let before = RetrySnapshot::capture(&s, b.visit).unwrap();
    assert!(!before.progressed(&s)); // Unseen DOM changes cannot change this stamp.
    edit(&mut s, &b, "input", "Initial");
    assert!(before.progressed(&s)); // Same literal, genuinely newer occurrence.
    let before = RetrySnapshot::capture(&s, b.visit).unwrap();
    mount(&mut s, "enter");
    assert!(!before.progressed(&s));
}

#[test]
fn quiet_received_edit_allows_fresh_lease_but_source_account_and_context_do_not() {
    for retirement in ["none", "source", "account", "socket", "visit"] {
        let mut s = setup();
        inputs::native::seed(&mut s);
        let b = binding(&s, "enter");
        let before = RetrySnapshot::capture(&s, b.visit).unwrap();
        assert!(inputs::set_local(
            &mut s,
            b.visit,
            &text_fixture::key("quiet"),
            InputValue::Text("Latest sibling".into())
        ));
        match retirement {
            "source" => s.app_home.surface.as_mut().unwrap().revision = next(),
            "account" => s.core.account_epoch += 1,
            "socket" => s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            "visit" => s.app_home.surface.as_mut().unwrap().id = next(),
            _ => {}
        }
        assert_eq!(before.progressed(&s), retirement == "none", "{retirement}");
    }
}
