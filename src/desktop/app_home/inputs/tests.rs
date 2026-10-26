use super::*;
use serde_json::json;
fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_inputs(&mut s);
    native::seed(&mut s);
    s
}
fn visit(s: &ShellState) -> actions::Key {
    let h = s.app_home.surface.as_ref().unwrap();
    actions::Key {
        visit: h.id,
        revision: h.revision,
    }
}
fn input() -> InputKey {
    InputKey {
        block_id: "home-name".into(),
        action_id: "value".into(),
    }
}
#[test]
fn ordinary_parent_feedback_stays_absent_before_and_after_invalid_edit() {
    let mut s = setup();
    let choice = InputKey {
        block_id: "home-choice".into(),
        action_id: "value".into(),
    };
    assert!(form(&s).unwrap().fields[&choice].error.is_some());
    let pristine = form(&s).unwrap().clone();
    assert_eq!(presentation_error(&s, &choice), None);
    assert_eq!(form(&s).unwrap(), &pristine);

    let k = visit(&s);
    assert!(set_local(
        &mut s,
        k,
        &input(),
        InputValue::Text(String::new())
    ));
    assert!(form(&s).unwrap().fields[&input()].dirty);
    assert!(form(&s).unwrap().fields[&input()].error.is_some());
    let edited = form(&s).unwrap().clone();
    let wire = edited.state().unwrap();
    assert_eq!(presentation_error(&s, &input()), None);
    assert_eq!(form(&s).unwrap(), &edited);
    assert_eq!(form(&s).unwrap().state().unwrap(), wire);
    s.app_home.surface.as_mut().unwrap().id = next();
    assert_eq!(presentation_error(&s, &input()), None);
    assert_eq!(form(&s).unwrap(), &edited);
}

#[test]
fn native_empty_buffer_feedback_survives_overlay_but_not_foreign_owner() {
    let mut s = setup();
    let o = owner(&s).unwrap();
    let day = InputKey {
        block_id: "home-day".into(),
        action_id: "value".into(),
    };
    s.app_home
        .forms
        .get_mut(&o)
        .unwrap()
        .set_native_error(&day, String::new(), "Complete the native date.".into())
        .unwrap();
    assert_eq!(form(&s).unwrap().native_error_text(&day), Some(""));
    assert_eq!(
        presentation_error(&s, &day).as_deref(),
        Some("Complete the native date.")
    );
    assert!(form(&s).unwrap().state().is_err());
    s.overlay = Some(Overlay::AppModal);
    assert_eq!(
        presentation_error(&s, &day).as_deref(),
        Some("Complete the native date.")
    );
    s.core.account_epoch += 1;
    assert_eq!(presentation_error(&s, &day), None);
    assert!(s.app_home.forms[&o].native_error_text(&day).is_some());
}
#[test]
fn actual_quiet_home_edit_button_form_and_update_keep_exact_state_and_conversation_drafts() {
    let mut s = setup();
    s.core.composer.text = "unsent".into();
    let k = visit(&s);
    assert!(set_local(
        &mut s,
        k,
        &input(),
        InputValue::Text("  Reviewed 東京  ".into())
    ));
    let h = s.app_home.surface.as_ref().unwrap();
    let view = h.view.as_ref().unwrap();
    let b = view
        .button_with_form(
            "entry",
            "review",
            &h.owner.ctx.session.user_id,
            form(&s).unwrap(),
        )
        .unwrap();
    fixture::activate(&mut s, &b);
    assert_eq!(s.overlay, Some(Overlay::AppModal));
    let fields = s.app_home.fixture_forms[0]["fields"].as_array().unwrap();
    let state = fields.iter().find(|p| p[0] == "state").unwrap()[1]
        .as_str()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(state).unwrap()["values"]["home-name"]["value"]["value"],
        "Reviewed 東京"
    );
    let submission = s
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .submission()
        .unwrap();
    assert!(fixture::submit(&mut s, &submission, "submitted"));
    assert_eq!(
        form(&s).unwrap().fields[&input()].value,
        InputValue::Text("  Reviewed 東京  ".into())
    );
    assert_eq!(s.core.composer.text, "unsent");
}
#[test]
fn stale_input_callbacks_and_raw_view_removal_cannot_replace_retained_home_input() {
    let mut s = setup();
    let k = visit(&s);
    assert!(set_local(
        &mut s,
        k,
        &input(),
        InputValue::Text("retained".into())
    ));
    s.app_home.surface.as_mut().unwrap().revision = next();
    assert!(!set_local(
        &mut s,
        k,
        &input(),
        InputValue::Text("stale".into())
    ));
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["block_id"] != "home-name");
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    hydrate(&mut s);
    assert!(!ready(&s));
    assert_eq!(
        form(&s).unwrap().fields[&input()].value,
        InputValue::Text("retained".into())
    );
    let h = s.app_home.surface.as_ref().unwrap();
    assert!(
        h.view
            .as_ref()
            .unwrap()
            .button_with_form(
                "entry",
                "review",
                &h.owner.ctx.session.user_id,
                form(&s).unwrap()
            )
            .is_err()
    );
}
#[test]
fn captured_button_state_cannot_change_under_dispatch_or_escape_uncertain_action_identity() {
    let mut s = setup();
    let h = s.app_home.surface.as_ref().unwrap();
    let v = h.view.as_ref().unwrap();
    let a = v
        .button_with_form(
            "entry",
            "review",
            &h.owner.ctx.session.user_id,
            form(&s).unwrap(),
        )
        .unwrap();
    let k = visit(&s);
    assert!(set_local(
        &mut s,
        k,
        &input(),
        InputValue::Text("changed".into())
    ));
    assert!(!a.same_form(form(&s).unwrap()));
    let h = s.app_home.surface.as_ref().unwrap();
    let b = h
        .view
        .as_ref()
        .unwrap()
        .button_with_form(
            "entry",
            "review",
            &h.owner.ctx.session.user_id,
            form(&s).unwrap(),
        )
        .unwrap();
    assert!(a.same_action(&b));
    assert_eq!(form(&s).unwrap().raw_view()["hash"], json!("initial"));
}
