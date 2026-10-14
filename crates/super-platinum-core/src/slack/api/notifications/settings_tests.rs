use super::tests::workspace;
use super::*;
use crate::slack::client::RequestBody;

fn snapshot(global: Value, channel: Value) -> NotificationPreferences {
    parse_notification_preferences(&json!({"ok":true,"prefs":{"all_notifications_prefs":{
        "global":global,"channels":{"C1":channel,"C2":{"unknown":[1,2]}}
    }}}))
    .unwrap()
}
fn form(
    intent: &ChannelNotificationIntent,
    prefs: &NotificationPreferences,
) -> BTreeMap<String, String> {
    let request =
        channel_notification_request(&SlackClient::default(), &workspace(), intent, prefs, 100)
            .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    fields.into_iter().collect()
}
fn capture(
    prefs: &NotificationPreferences,
    action: ChannelNotificationAction,
) -> ChannelNotificationIntent {
    ChannelNotificationIntent::capture(&workspace(), "C1", false, prefs, action, 100).unwrap()
}
#[test]
fn calculated_settings_preserve_missing_empty_legacy_and_direct_defaults() {
    let p = snapshot(json!({}), json!({}));
    let s = p.channel_settings("C1", false).unwrap();
    assert_eq!(s.desktop, ChannelNotificationMode::Mentions);
    assert!(s.desktop_push && s.mobile_enabled && !s.has_different_mobile);
    assert_eq!(
        p.channel_settings("C1", true).unwrap().desktop,
        ChannelNotificationMode::Everything
    );
    let p = snapshot(
        json!({"global_desktop":"everything","global_mobile":"","global_desktop_push_enabled":false}),
        json!({}),
    );
    let s = p.channel_settings("C1", false).unwrap();
    assert_eq!(s.mobile, ChannelNotificationMode::Everything);
    assert!(s.has_different_mobile);
    assert!(!s.desktop_push);
    let p = snapshot(
        json!({"global_desktop":"everything"}),
        json!({"desktop":"nothing","mobile":"nothing","desktop_push_enabled":true}),
    );
    let s = p.channel_settings("C1", false).unwrap();
    assert_eq!(s.desktop, ChannelNotificationMode::Mentions);
    assert_eq!(s.mobile, ChannelNotificationMode::Mentions);
    assert!(!s.mobile_enabled && !s.has_different_mobile);
    assert!(s.desktop_push);
    for bad in [json!("future"), json!(true), json!(5)] {
        assert!(
            snapshot(json!({}), json!({"mobile":bad}))
                .channel_settings("C1", false)
                .is_err()
        );
    }
}
#[test]
fn primary_linked_mobile_exact_pref_sync_and_muted_side_effect() {
    let p = snapshot(
        json!({"global_desktop":"mentions_dms","global_mobile":"mentions_dms"}),
        json!({"muted":false,"temp_mute_expiration_ts":200,"unknown":"keep","suppress_at_channel":true}),
    );
    let intent = capture(
        &p,
        ChannelNotificationAction::Mode(ChannelNotificationMode::Everything),
    );
    let fields = form(&intent, &p);
    assert_eq!(fields["channel_ids"], "C1");
    assert_eq!(fields["sync"], "true");
    assert_eq!(
        fields["_x_reason"],
        "prefs-store/setMultiChannelNotificationOverride"
    );
    assert!(!fields.contains_key("name"));
    assert_eq!(
        serde_json::from_str::<Value>(&fields["prefs"]).unwrap(),
        json!([
            {"name":"desktop","value":"everything","sync":true},
            {"name":"follow_all_threads","value":"false","sync":false},
            {"name":"badge_all_unreads","value":"false","sync":false},
            {"name":"muted","value":"false","sync":false}
        ])
    );
    assert!(!intent.desired_patch().contains_key("suppress_at_channel"));
    assert!(
        !intent
            .desired_patch()
            .contains_key("temp_mute_expiration_ts")
    );
    assert!(
        intent
            .changed_keys()
            .iter()
            .any(|k| k == "temp_mute_expiration_ts")
    );
    let mut fresh = p.clone();
    for (key, value) in intent.desired_patch() {
        fresh.raw["channels"]["C1"][key] = value.clone();
    }
    assert!(!intent.confirms(&fresh, 100)); // expiry is still active
    fresh.raw["channels"]["C1"]["temp_mute_expiration_ts"] = json!(0);
    assert!(intent.confirms(&fresh, 100));
    assert_eq!(fresh.raw["channels"]["C1"]["unknown"], "keep");
    assert_eq!(fresh.raw["channels"]["C2"], p.raw["channels"]["C2"]);
}
#[test]
fn different_mobile_is_not_overwritten_and_mentions_preserves_badge() {
    let p = snapshot(
        json!({}),
        json!({"desktop":"everything","mobile":"mentions_dms","badge_all_unreads":true}),
    );
    let intent = capture(
        &p,
        ChannelNotificationAction::Mode(ChannelNotificationMode::Mentions),
    );
    let fields = form(&intent, &p);
    assert_eq!(fields["sync"], "false");
    assert_eq!(
        serde_json::from_str::<Value>(&fields["prefs"]).unwrap(),
        json!([
            {"name":"desktop","value":"mentions_dms","sync":false},
            {"name":"follow_all_threads","value":"false"},
            {"name":"suppress_at_channel","value":"false"}
        ])
    );
    assert!(
        !intent
            .changed_keys()
            .iter()
            .any(|k| k == "mobile" || k == "badge_all_unreads")
    );
}
#[test]
fn advanced_single_pref_forms_and_off_on_restore_desktop_filter() {
    use ChannelNotificationAction::*;
    let p = snapshot(json!({"global_desktop":"everything"}), json!({"unknown":7}));
    for (action, key, value) in [
        (DesktopPush(false), "desktop_push_enabled", "false"),
        (MobilePush(false), "mobile", "nothing"),
        (MobilePush(true), "mobile", "everything"),
        (
            MobileOverride(ChannelNotificationMode::Mentions),
            "mobile",
            "mentions_dms",
        ),
        (FollowAllThreads(true), "follow_all_threads", "true"),
        (SuppressAtChannel(true), "suppress_at_channel", "true"),
        (BadgeAllUnreads(true), "badge_all_unreads", "true"),
    ] {
        let intent = capture(&p, action);
        let fields = form(&intent, &p);
        assert_eq!(fields["channel_id"], "C1");
        assert_eq!(fields["name"], key);
        assert_eq!(fields["value"], value);
        assert_eq!(fields["sync"], "false");
        assert_eq!(
            fields["_x_reason"],
            "prefs-store/setChannelNotificationOverride"
        );
        assert!(!fields.contains_key("prefs") && !fields.contains_key("channel_ids"));
        assert_eq!(intent.changed_keys(), &[key.to_string()]);
        let mut fresh = p.clone();
        for (key, value) in intent.desired_patch() {
            fresh.raw["channels"]["C1"][key] = value.clone();
        }
        assert!(intent.confirms(&fresh, 100));
        assert_eq!(fresh.raw["global"], p.raw["global"]);
        assert_eq!(fresh.raw["channels"]["C1"]["unknown"], 7);
    }
    let off = snapshot(json!({}), json!({"mobile":"nothing"}));
    assert!(
        ChannelNotificationIntent::capture(
            &workspace(),
            "C1",
            false,
            &off,
            MobileOverride(ChannelNotificationMode::Everything),
            100
        )
        .is_err()
    );
}
#[test]
fn stale_dependencies_owner_and_mute_expiry_cannot_dispatch_but_other_channels_can_change() {
    let p = snapshot(
        json!({"global_desktop":"everything"}),
        json!({"temp_mute_expiration_ts":150}),
    );
    let intent = capture(
        &p,
        ChannelNotificationAction::Mode(ChannelNotificationMode::Mentions),
    );
    assert!(intent.revalidate(&p, 151).is_err());
    let mut fresh = p.clone();
    fresh.raw["channels"]["C2"]["unknown"] = json!("changed");
    assert!(intent.revalidate(&fresh, 100).is_ok());
    fresh.raw["global"]["global_mobile"] = json!("everything");
    assert!(intent.revalidate(&fresh, 100).is_err());
    let mut other = workspace();
    other.user_id = "U2".into();
    assert!(
        channel_notification_request(&SlackClient::default(), &other, &intent, &p, 100).is_err()
    );
    let mut fresh = p.clone();
    fresh.raw["channels"]["C1"]["desktop"] = json!("mentions_dms");
    assert!(!intent.confirms(&fresh, 100)); // no other required keys
}

#[test]
fn captured_primary_normalization_can_reenable_raw_mobile_off() {
    // Fpyc normalizes both platforms before BtIR compares them. This differs
    // from R7xz's raw mobile on/off switch; do not substitute intuition here.
    let p = snapshot(
        json!({}),
        json!({"desktop":"mentions_dms","mobile":"nothing"}),
    );
    let s = p.channel_settings("C1", false).unwrap();
    assert!(!s.mobile_enabled && !s.has_different_mobile);
    let intent = capture(
        &p,
        ChannelNotificationAction::Mode(ChannelNotificationMode::Everything),
    );
    assert_eq!(form(&intent, &p)["sync"], "true");
    assert_eq!(intent.desired_patch()["mobile"], "everything");
    let p = snapshot(
        json!({"global_desktop":"mentions_dms","global_mobile":"nothing"}),
        json!({}),
    );
    let s = p.channel_settings("C1", false).unwrap();
    assert!(s.mobile_enabled); // absent raw channel.mobile, not the global value
    assert!(!s.has_different_mobile);
}

#[test]
fn explicit_null_global_mobile_is_defined_for_strict_sync_comparison() {
    let p = snapshot(
        json!({"global_desktop":"everything","global_mobile":null}),
        json!({}),
    );
    assert!(
        p.channel_settings("C1", false)
            .unwrap()
            .has_different_mobile
    );
    let intent = capture(
        &p,
        ChannelNotificationAction::Mode(ChannelNotificationMode::Mentions),
    );
    assert_eq!(form(&intent, &p)["sync"], "false");
    assert!(!intent.changed_keys().iter().any(|key| key == "mobile"));
    let p = snapshot(
        json!({"global_desktop":null,"global_mobile":null}),
        json!({}),
    );
    assert!(
        !p.channel_settings("C1", false)
            .unwrap()
            .has_different_mobile
    );
    let p = snapshot(
        json!({"global_desktop":"everything","global_mobile":"everything"}),
        json!({"mobile":null}),
    );
    assert!(
        !p.channel_settings("C1", false)
            .unwrap()
            .has_different_mobile
    ); // null override falls back
}
