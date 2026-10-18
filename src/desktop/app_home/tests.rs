use super::*;
use serde_json::json;
fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
#[test]
fn actual_home_entry_modal_submission_and_home_update_preserve_drafts() {
    let mut s = setup();
    s.core.composer.text = "unsent channel".into();
    s.core.thread_composer.text = "unsent thread".into();
    let h = s.app_home.surface.as_ref().unwrap();
    let b = h
        .view
        .as_ref()
        .unwrap()
        .button("entry", "review", &h.owner.ctx.session.user_id)
        .unwrap();
    let revision = h.revision;
    fixture::activate(&mut s, &b);
    assert_eq!(s.overlay, Some(Overlay::AppModal));
    assert_eq!(s.app_home.fixture_forms.len(), 1);
    let form = s.app_modal.as_mut().unwrap().current_form_mut().unwrap();
    form.set_value(
        &super_platinum_core::block_actions::form::InputKey {
            block_id: "name".into(),
            action_id: "value".into(),
        },
        super_platinum_core::block_actions::form::InputValue::Text("Edited Équipe 東京".into()),
    )
    .unwrap();
    let payload = form.submission().unwrap();
    assert!(fixture::submit(&mut s, &payload, "submitted"));
    assert_eq!(s.overlay, None);
    assert_eq!(s.app_home.fixture_forms.len(), 2);
    assert!(s.app_home.surface.as_ref().unwrap().revision > revision);
    assert_eq!(
        s.app_home
            .surface
            .as_ref()
            .unwrap()
            .view
            .as_ref()
            .unwrap()
            .raw()["hash"],
        "saved"
    );
    assert_eq!(s.core.composer.text, "unsent channel");
    assert_eq!(s.core.thread_composer.text, "unsent thread");
}
fn update(s: &ShellState) -> serde_json::Value {
    let h = s.app_home.surface.as_ref().unwrap();
    let mut v = h.view.as_ref().unwrap().raw().clone();
    v["hash"] = json!("next");
    json!({"type":"view_updated","view_id":"V_HOME","app_id":"A_HOME","view":v})
}
#[test]
fn exact_realtime_account_app_installed_team_channel_visit_guards() {
    for kind in 0..6 {
        let mut s = setup();
        let owner = s.app_home.surface.as_ref().unwrap().owner.clone();
        let old = s.app_home.surface.as_ref().unwrap().revision;
        let mut event = update(&s);
        match kind {
            0 => event["app_id"] = json!("A_OTHER"),
            1 => event["view"]["app_installed_team_id"] = json!("T_OTHER"),
            2 => event["view_id"] = json!("V_OTHER"),
            3 => s.channel_generation += 1,
            4 => s.core.account_epoch += 1,
            _ => {
                s.core
                    .workspaces
                    .get_mut(&owner.ctx.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
        }
        assert!(!realtime(
            &mut s,
            &owner.ctx.session.team_id,
            owner.rt,
            &RtEvent::Unknown(serde_json::from_value(event).unwrap())
        ));
        assert_eq!(s.app_home.surface.as_ref().unwrap().revision, old);
    }
    let mut s = setup();
    let owner = s.app_home.surface.as_ref().unwrap().owner.clone();
    let mut e = update(&s);
    e["type"] = json!("view_opened");
    e["view_type"] = json!("home");
    e["channel_id"] = json!("D_FOREIGN");
    let old = s.app_home.surface.as_ref().unwrap().revision;
    realtime(
        &mut s,
        &owner.ctx.session.team_id,
        owner.rt,
        &RtEvent::Unknown(serde_json::from_value(e).unwrap()),
    );
    assert_eq!(s.app_home.surface.as_ref().unwrap().revision, old);
}
#[test]
fn contentless_update_invalidates_old_buttons_and_late_read_revision() {
    let mut s = setup();
    let o = s.app_home.surface.as_ref().unwrap().owner.clone();
    let r = s.app_home.surface.as_ref().unwrap().revision;
    let mut e = update(&s);
    e.as_object_mut().unwrap().remove("view");
    assert!(realtime(
        &mut s,
        &o.ctx.session.team_id,
        o.rt,
        &RtEvent::Unknown(serde_json::from_value(e).unwrap())
    ));
    let h = s.app_home.surface.as_ref().unwrap();
    assert!(h.view.is_none());
    assert!(h.revision > r)
}
#[test]
fn returning_to_home_cannot_revive_old_visit_or_modal_token() {
    let mut s = setup();
    let owner = s.app_home.surface.as_ref().unwrap().owner.clone();
    s.app_home.correlations.push(("old".into(), owner.clone()));
    leave(&mut s);
    assert!(!owner.current(&s));
    fixture::install(&mut s);
    assert!(!owner.current(&s));
    s.app_home.correlations.push(("old".into(), owner));
    let e = serde_json::from_value(json!({"type":"view_opened","client_token":"old"})).unwrap();
    assert!(!accepts_modal(&s, &e))
}
#[test]
fn actual_rate_limit_is_account_scoped_and_unrepresentable_deadline_stays_closed() {
    let mut s = setup();
    let ctx = s.app_home.surface.as_ref().unwrap().owner.ctx.clone();
    failure(
        &mut s,
        &ctx,
        &Error::RateLimited {
            retry_after_secs: Some(u64::MAX),
        },
    );
    assert!(cooling(&s, &ctx));
    assert!(s.app_home.cooldowns[&(ctx.epoch, ctx.session.team_id.clone())].is_none());
    failure(
        &mut s,
        &ctx,
        &Error::RateLimited {
            retry_after_secs: Some(1),
        },
    );
    assert!(s.app_home.cooldowns[&(ctx.epoch, ctx.session.team_id.clone())].is_none());
    s.core.account_epoch += 1;
    let other = Context::capture(&s).unwrap();
    assert!(!cooling(&s, &other))
}
