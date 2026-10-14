use super::super::tests::workspace;
use super::*;
use crate::slack::client::RequestBody;
fn prefs(global: Value, mute: Option<Value>) -> PresentationNotificationPreferences {
    let mut raw = json!({"ok":true,"prefs":{"all_notifications_prefs":{"global":global,"channels":{"C1":{"unknown":9}},"unknown":[1]}}});
    if let Some(m) = mute {
        raw["prefs"]["mute_sounds"] = m;
    }
    PresentationNotificationPreferences::from_response(&raw).unwrap()
}
fn capture(
    p: &PresentationNotificationPreferences,
    a: PresentationNotificationAction,
) -> PresentationNotificationIntent {
    PresentationNotificationIntent::capture(&workspace(), p, a).unwrap()
}
#[test]
fn named_read_decodes_json_and_preserves_null_absence_unknowns() {
    let missing = prefs(json!({}), None);
    let null = prefs(
        json!({"desktop_sound":null,"priority_desktop_sound":null}),
        Some(Value::Null),
    );
    assert_eq!(missing.settings().unwrap(), null.settings().unwrap());
    assert!(!missing.raw().contains_key("mute_sounds"));
    assert_eq!(null.raw()["mute_sounds"], Value::Null);
    assert!(missing.settings().unwrap().include_preview);
    let response = json!({"ok":true,"prefs":{"all_notifications_prefs":missing.raw()["all_notifications_prefs"].to_string(),"mute_sounds":false,"other":7}});
    let parsed = PresentationNotificationPreferences::from_response(&response).unwrap();
    assert_eq!(parsed.raw()["other"], 7);
    assert_eq!(
        parsed.raw()["all_notifications_prefs"]["channels"]["C1"]["unknown"],
        9
    );
    for bad in [
        json!({"ok":true,"prefs":{}}),
        json!({"ok":false,"prefs":{}}),
        json!({"ok":true,"prefs":{"all_notifications_prefs":{"channels":{},"global":{"desktop_sound":5}}}}),
    ] {
        assert!(PresentationNotificationPreferences::from_response(&bad).is_err());
    }
    let req = presentation_notification_preferences_request(&SlackClient::default(), &workspace());
    let RequestBody::Form(fields) = req.body else {
        panic!()
    };
    assert!(fields.contains(&(
        "prefs".into(),
        "all_notifications_prefs,mute_sounds,mac_ssb_bounce".into()
    )));
}
#[test]
fn exact_scalar_families_and_inverse_privacy() {
    let p = prefs(json!({}), None);
    for (action, name, value, user, reason) in [
        (
            PresentationNotificationAction::DesktopSound("none".into()),
            "desktop_sound",
            "none",
            false,
            "",
        ),
        (
            PresentationNotificationAction::PrioritySound("boop.mp3".into()),
            "priority_desktop_sound",
            "boop.mp3",
            false,
            "select-new-vip-notification-sound",
        ),
        (
            PresentationNotificationAction::IncludePreview(false),
            "no_text_in_notifications",
            "true",
            false,
            "",
        ),
        (
            PresentationNotificationAction::MuteSounds(true),
            "mute_sounds",
            "true",
            true,
            "prefs-api/setUserPrefByApi",
        ),
    ] {
        let i = capture(&p, action);
        assert_eq!(i.is_user_pref(), user);
        let req = presentation_notification_request(&SlackClient::default(), &workspace(), &i, &p)
            .unwrap();
        assert_eq!(
            url::Url::parse(&req.url).unwrap().path(),
            if user {
                "/api/users.prefs.set"
            } else {
                "/api/users.prefs.setNotifications"
            }
        );
        let RequestBody::Form(fields) = req.body else {
            panic!()
        };
        let f: BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(f["name"], name);
        assert_eq!(f["value"], value);
        assert_eq!(f["_x_reason"], reason);
        assert!(!f.contains_key("prefs"));
        assert!(!f.contains_key("channel_id"));
        assert_eq!(
            f.get("global").map(String::as_str),
            (!user).then_some("true")
        );
        assert_eq!(
            f.get("sync").map(String::as_str),
            (!user).then_some("false")
        );
    }
}
#[test]
fn stale_owner_and_store_guard_preserve_unrelated_preferences() {
    let p = prefs(json!({"desktop_sound":"b2.mp3","other":9}), None);
    let i = capture(
        &p,
        PresentationNotificationAction::DesktopSound("boop.mp3".into()),
    );
    let mut changed = p.clone();
    changed.raw["all_notifications_prefs"]["channels"]["C1"]["muted"] = json!(true);
    assert!(i.revalidate(&changed).is_ok());
    changed.raw["all_notifications_prefs"]["global"]["other"] = json!(10);
    assert!(i.revalidate(&changed).is_err());
    let mut w = workspace();
    w.user_id = "other".into();
    assert!(presentation_notification_request(&SlackClient::default(), &w, &i, &p).is_err());
    assert!(!i.confirms(&p));
    changed.raw["all_notifications_prefs"]["global"]["desktop_sound"] = json!("boop.mp3");
    assert!(i.confirms(&changed));
    let mute = capture(&p, PresentationNotificationAction::MuteSounds(true));
    assert!(mute.revalidate(&changed).is_ok());
    changed.raw.insert("mute_sounds".into(), Value::Null);
    assert!(mute.revalidate(&changed).is_err()); // missing != null
    assert!(!mute.confirms(&changed));
    changed.raw.insert("mute_sounds".into(), json!(true));
    assert!(mute.confirms(&changed));
}
#[test]
fn catalog_is_exact_and_unknown_stored_sound_is_not_reauthored() {
    assert_eq!(NOTIFICATION_SOUNDS.len(), 13);
    let p = prefs(
        json!({"desktop_sound":"future.mp3","priority_desktop_sound":""}),
        None,
    );
    assert_eq!(
        p.settings().unwrap().desktop_sound.as_deref(),
        Some("future.mp3")
    );
    for value in ["future.mp3", "", "boop_remix.mp3", "../b2.mp3"] {
        assert!(
            PresentationNotificationIntent::capture(
                &workspace(),
                &p,
                PresentationNotificationAction::DesktopSound(value.into())
            )
            .is_err()
        );
    }
    for (value, _) in NOTIFICATION_SOUNDS {
        assert!(
            PresentationNotificationIntent::capture(
                &workspace(),
                &p,
                PresentationNotificationAction::PrioritySound((*value).into())
            )
            .is_ok()
        );
    }
}

#[test]
fn source_disabled_controls_and_sound_mute_drift_are_guarded() {
    let p = prefs(json!({}), None);
    let i = capture(
        &p,
        PresentationNotificationAction::DesktopSound("b2.mp3".into()),
    );
    let muted = prefs(json!({}), Some(json!(true)));
    assert!(i.revalidate(&muted).is_err());
    for p in [muted, prefs(json!({"global_desktop":"nothing"}), None)] {
        assert!(
            PresentationNotificationIntent::capture(
                &workspace(),
                &p,
                PresentationNotificationAction::DesktopSound("b2.mp3".into())
            )
            .is_err()
        );
    }
    let disabled = prefs(json!({"global_desktop":"nothing"}), None);
    assert!(
        PresentationNotificationIntent::capture(
            &workspace(),
            &disabled,
            PresentationNotificationAction::IncludePreview(true)
        )
        .is_err()
    );
    assert!(
        PresentationNotificationIntent::capture(
            &workspace(),
            &disabled,
            PresentationNotificationAction::MuteSounds(false)
        )
        .is_ok()
    );
    for raw in [json!("false"), json!(1), json!([]), json!({})] {
        let p = prefs(json!({"no_text_in_notifications":raw}), Some(raw.clone()));
        assert!(!p.settings().unwrap().include_preview);
        assert!(p.settings().unwrap().mute_sounds);
    }
}

#[test]
fn bounce_scalar_exact_wire_and_unknown_canonical_preservation() {
    let mut p = prefs(json!({}), None);
    p.raw.insert("mac_ssb_bounce".into(), json!("future"));
    assert_eq!(p.settings().unwrap().mac_bounce.as_deref(), Some("future"));
    for value in ["", "short", "long"] {
        let i = capture(&p, PresentationNotificationAction::MacBounce(value.into()));
        assert!(i.is_user_pref());
        let req = presentation_notification_request(&SlackClient::default(), &workspace(), &i, &p)
            .unwrap();
        assert_eq!(
            url::Url::parse(&req.url).unwrap().path(),
            "/api/users.prefs.set"
        );
        let RequestBody::Form(fields) = req.body else {
            panic!()
        };
        let f: BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(f["name"], "mac_ssb_bounce");
        assert_eq!(f["value"], value);
        assert_eq!(f["_x_reason"], "prefs-api/setUserPrefByApi");
        assert!(!f.contains_key("global") && !f.contains_key("sync"));
        let mut fresh = p.clone();
        fresh.raw.insert("mac_ssb_bounce".into(), json!(value));
        assert!(i.confirms(&fresh));
        fresh.raw.remove("mac_ssb_bounce");
        assert!(!i.confirms(&fresh));
    }
    assert!(
        PresentationNotificationIntent::capture(
            &workspace(),
            &p,
            PresentationNotificationAction::MacBounce("future".into())
        )
        .is_err()
    );
}
#[test]
fn bounce_preflight_binds_own_value_and_legacy_disabled_context_only() {
    let p = prefs(json!({"global_desktop":"mentions_dms"}), None);
    let i = capture(&p, PresentationNotificationAction::MacBounce("long".into()));
    let mut fresh = p.clone();
    fresh.raw.insert("mute_sounds".into(), json!(true));
    fresh.raw["all_notifications_prefs"]["channels"]["C1"]["muted"] = json!(true);
    assert!(i.revalidate(&fresh).is_ok());
    fresh.raw.insert("mac_ssb_bounce".into(), Value::Null);
    assert!(i.revalidate(&fresh).is_err()); // absent is not null
    fresh.raw.remove("mac_ssb_bounce");
    fresh.raw["all_notifications_prefs"]["global"]["global_desktop"] = json!("nothing");
    assert!(i.revalidate(&fresh).is_err());
    assert!(
        PresentationNotificationIntent::capture(
            &workspace(),
            &fresh,
            PresentationNotificationAction::MacBounce("".into())
        )
        .is_err()
    );
}
