use super::super::tests::workspace;
use super::*;
use crate::slack::client::RequestBody;
fn prefs(global: Value) -> NotificationPreferences {
    parse_notification_preferences(&json!({"ok":true,"prefs":{"all_notifications_prefs":{"global":global,"channels":{"C1":{"desktop":"everything","unknown":9}},"unknown":[1,2]}}})).unwrap()
}
fn capture(p: &NotificationPreferences, a: GlobalNotificationAction) -> GlobalNotificationIntent {
    GlobalNotificationIntent::capture(&workspace(), p, a).unwrap()
}
fn form(p: &NotificationPreferences, i: &GlobalNotificationIntent) -> BTreeMap<String, String> {
    let request = global_notification_request(&SlackClient::default(), &workspace(), i, p).unwrap();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/users.prefs.setNotifications"
    );
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    fields.into_iter().collect()
}
#[test]
fn global_projection_preserves_raw_missing_null_legacy_and_device_semantics() {
    let p = prefs(json!({}));
    let s = p.global_settings().unwrap();
    assert_eq!(s.desktop, None);
    assert_eq!(s.mobile, None);
    assert!(s.linked_mobile && s.mobile_enabled && s.desktop_push);
    assert!(!s.thread_replies);
    let p = prefs(json!({"global_desktop":null}));
    assert!(!p.global_settings().unwrap().linked_mobile);
    let p = prefs(
        json!({"global_desktop":null,"global_mobile":null,"global_desktop_push_enabled":null,"threads_everything":null}),
    );
    let s = p.global_settings().unwrap();
    assert!(s.linked_mobile && s.desktop_push);
    assert!(!s.thread_replies);
    let p = prefs(json!({"global_desktop":"nothing","global_mobile":"mentions_dms"}));
    let s = p.global_settings().unwrap();
    assert_eq!(s.desktop, Some(ChannelNotificationMode::Mentions));
    assert!(!s.linked_mobile);
    for global in [
        json!({"global_mobile":"future"}),
        json!({"global_desktop_push_enabled":"true"}),
        json!({"global_keywords":5}),
    ] {
        assert!(prefs(global).global_settings().is_err());
    }
}
#[test]
fn desktop_modes_use_explicit_paired_array_only_when_enabled_and_mounted_linked() {
    for (mobile, linked, expected) in [
        ("mentions_dms", true, 2),
        ("mentions_dms", false, 1),
        ("nothing", true, 1),
    ] {
        let p = prefs(json!({"global_desktop":"mentions_dms","global_mobile":mobile}));
        let i = capture(
            &p,
            GlobalNotificationAction::DesktopMode {
                mode: ChannelNotificationMode::Everything,
                linked_mobile: linked,
            },
        );
        let f = form(&p, &i);
        assert_eq!(f["global"], "true");
        assert_eq!(f["_x_reason"], "NotificationsChannelDefaults");
        assert!(!f.contains_key("sync"));
        assert!(!f.contains_key("channel_ids"));
        let wire: Value = serde_json::from_str(&f["prefs"]).unwrap();
        assert_eq!(wire.as_array().unwrap().len(), expected);
        assert_eq!(
            wire[0],
            json!({"name":"global_desktop","value":"everything"})
        );
        assert_eq!(i.changed_keys().len(), expected);
    }
}
#[test]
fn advanced_mobile_mode_and_relink_have_distinct_wire_and_no_invented_fallback() {
    let p = prefs(json!({"global_desktop":"everything","global_mobile":"mentions_dms"}));
    let i = capture(
        &p,
        GlobalNotificationAction::MobileMode(ChannelNotificationMode::Everything),
    );
    let f = form(&p, &i);
    assert_eq!(
        serde_json::from_str::<Value>(&f["prefs"]).unwrap(),
        json!([{"name":"global_mobile","value":"everything"}])
    );
    let i = capture(&p, GlobalNotificationAction::RelinkMobile);
    let f = form(&p, &i);
    assert_eq!(f["name"], "global_mobile");
    assert_eq!(f["value"], "everything");
    assert_eq!(f["sync"], "false");
    assert!(!f.contains_key("prefs"));
    for desktop in [None, Some(Value::Null)] {
        let mut raw = json!({"global_mobile":"everything"});
        if let Some(v) = desktop {
            raw["global_desktop"] = v;
        }
        assert!(
            GlobalNotificationIntent::capture(
                &workspace(),
                &prefs(raw),
                GlobalNotificationAction::RelinkMobile
            )
            .is_err()
        );
    }
    let p = prefs(json!({"global_desktop":"","global_mobile":"everything"}));
    assert_eq!(
        form(&p, &capture(&p, GlobalNotificationAction::RelinkMobile))["value"],
        ""
    );
}
#[test]
fn mobile_enable_uses_raw_desktop_and_device_toggles_are_boolean_scalars() {
    for (desktop, expected) in [
        (None, "mentions_dms"),
        (Some(""), "mentions_dms"),
        (Some("nothing"), "nothing"),
        (Some("everything"), "everything"),
    ] {
        let mut raw = json!({"global_mobile":"nothing"});
        if let Some(v) = desktop {
            raw["global_desktop"] = json!(v);
        }
        let p = prefs(raw);
        let i = capture(&p, GlobalNotificationAction::MobilePush(true));
        assert_eq!(form(&p, &i)["value"], expected);
    }
    let p = prefs(json!({}));
    for (action, name) in [
        (
            GlobalNotificationAction::DesktopPush(false),
            "global_desktop_push_enabled",
        ),
        (
            GlobalNotificationAction::ThreadReplies(false),
            "threads_everything",
        ),
    ] {
        let i = capture(&p, action);
        let f = form(&p, &i);
        assert_eq!(f["name"], name);
        assert_eq!(f["value"], "false");
        assert_eq!(f["_x_reason"], "NotificationsGlobalPrefCheckbox");
        assert_eq!(i.desired_patch()[name], false);
    }
}
#[test]
fn keyword_transforms_preserve_case_empty_tokens_unicode_and_only_one_ascii_space() {
    assert_eq!(
        notification_keywords_display("Alpha,  β,,"),
        "Alpha,   β, , "
    );
    assert_eq!(
        normalize_notification_keywords("Alpha,  β,\tγ,\nδ,\u{feff}E,,").unwrap(),
        "Alpha, β,\tγ,\nδ,\u{feff}E,,"
    );
    assert_eq!(normalize_notification_keywords("").unwrap(), "");
    assert!(normalize_notification_keywords(&vec![""; 100].join(",")).is_ok());
    assert!(normalize_notification_keywords(&vec!["x"; 101].join(",")).is_err());
    let p = prefs(json!({"global_keywords":"old"}));
    let i = capture(&p, GlobalNotificationAction::Keywords("".into()));
    let f = form(&p, &i);
    assert_eq!(f["name"], "global_keywords");
    assert_eq!(f["value"], "");
    assert_eq!(f["_x_reason"], "NotificationsKeywordsInput");
}
#[test]
fn stale_owner_global_dependency_and_explicit_canonical_confirmation() {
    let p = prefs(
        json!({"global_desktop":"mentions_dms","global_mobile":"mentions_dms","unknown":{"x":1}}),
    );
    let i = capture(
        &p,
        GlobalNotificationAction::DesktopMode {
            mode: ChannelNotificationMode::Everything,
            linked_mobile: true,
        },
    );
    let mut changed = p.clone();
    changed.raw["global"]["global_mobile"] = json!("nothing");
    assert!(i.revalidate(&changed).is_err());
    let mut other = workspace();
    other.user_id = "U2".into();
    assert!(global_notification_request(&SlackClient::default(), &other, &i, &p).is_err());
    let mut changed = p.clone();
    changed.raw["channels"]["C1"]["unknown"] = json!(10);
    assert!(i.revalidate(&changed).is_ok());
    assert!(!i.confirms(&changed));
    for (k, v) in i.desired_patch() {
        changed.raw["global"][k] = v.clone();
    }
    assert!(i.confirms(&changed));
    assert_eq!(changed.raw["global"]["unknown"], p.raw["global"]["unknown"]);
    assert_eq!(changed.raw["channels"]["C1"]["desktop"], "everything");
    changed.raw["global"]
        .as_object_mut()
        .unwrap()
        .remove("global_mobile");
    assert!(!i.confirms(&changed));
}
