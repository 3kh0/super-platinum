use super::*;
use crate::slack::client::RequestBody;
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn snapshot(value: Value) -> NotificationSchedule {
    NotificationSchedule::from_response(&json!({"ok":true,"prefs":value}))
        .unwrap()
        .with_user_timezone(&User {
            id: "U1".into(),
            tz: Some("America/New_York".into()),
            ..Default::default()
        })
        .unwrap()
}
fn action(p: &NotificationSchedule, a: ScheduleAction) -> NotificationScheduleIntent {
    NotificationScheduleIntent::capture(&workspace(), p, a).unwrap()
}
#[test]
fn raw_unpadded_null_and_disabled_preview_do_not_invent_pref_defaults() {
    let p = snapshot(
        json!({"dnd_enabled":true,"dnd_days":"every_day","dnd_end_hour":"8:00","dnd_start_hour":null,"unknown":"preserve"}),
    );
    assert_eq!(p.raw()["dnd_end_hour"], "8:00");
    assert_eq!(p.raw()["dnd_start_hour"], Value::Null);
    assert_eq!(p.view().unwrap().start.as_deref(), Some("08:00"));
    assert_eq!(p.view().unwrap().end, None);
    assert!(!p.raw().contains_key("unknown"));
    assert!(!p.raw().contains_key("tz"));
    let p = snapshot(json!({"dnd_enabled":false,"dnd_days":"custom"}));
    let v = p.view().unwrap();
    assert_eq!(v.mode, Some(ScheduleMode::EveryDay));
    assert_eq!(v.start.as_deref(), Some("00:00"));
    assert_eq!(v.end.as_deref(), Some("23:59"));
    for bad in [
        json!({"dnd_enabled":"true"}),
        json!({"dnd_days":"future"}),
        json!({"dnd_start_hour":"24:00"}),
    ] {
        assert!(NotificationSchedule::from_response(&json!({"ok":true,"prefs":bad})).is_err());
    }
}
#[test]
fn every_action_is_changed_keys_only_and_uses_exact_json_boolean_form() {
    let p = snapshot(
        json!({"dnd_enabled":true,"dnd_days":"every_day","dnd_end_hour":"8:00","dnd_start_hour":"20:00"}),
    );
    let i = action(&p, ScheduleAction::Mode(ScheduleMode::Weekdays));
    assert_eq!(
        serde_json::to_value(i.desired_patch()).unwrap(),
        json!({"dnd_days":"weekdays","dnd_enabled":true})
    );
    let req = notification_schedule_update_request(&SlackClient::default(), &workspace(), &i, &p)
        .unwrap();
    assert_eq!(
        url::Url::parse(&req.url).unwrap().path(),
        "/api/users.prefs.set"
    );
    let RequestBody::Form(fields) = req.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields["_x_reason"], "prefs-api/setMultipleUserPrefsByApi");
    assert_eq!(
        serde_json::from_str::<Value>(&fields["prefs"]).unwrap(),
        json!({"dnd_days":"weekdays","dnd_enabled":true})
    );
    let i = action(
        &p,
        ScheduleAction::Time {
            day: None,
            bound: ScheduleBound::End,
            value: "21:00".into(),
        },
    );
    assert_eq!(
        serde_json::to_value(i.desired_patch()).unwrap(),
        json!({"dnd_start_hour":"21:00","dnd_enabled":true})
    );
}
#[test]
fn allowed_start_moves_paired_end_using_original_raw_props_including_null() {
    for end in [
        None,
        Some(Value::Null),
        Some(json!("")),
        Some(json!("8:00")),
        Some(json!("09:00")),
    ] {
        let mut raw = json!({"dnd_enabled":true,"dnd_days":"every_day"});
        if let Some(end) = end {
            raw["dnd_start_hour"] = end;
        }
        let p = snapshot(raw);
        let i = action(
            &p,
            ScheduleAction::Time {
                day: None,
                bound: ScheduleBound::Start,
                value: "09:00".into(),
            },
        );
        assert_eq!(i.desired_patch()["dnd_end_hour"], "09:00");
        assert_eq!(i.desired_patch()["dnd_start_hour"], "09:30");
    }
    let p = snapshot(json!({"dnd_enabled":true,"dnd_days":"every_day","dnd_start_hour":"22:00"}));
    let i = action(
        &p,
        ScheduleAction::Time {
            day: None,
            bound: ScheduleBound::Start,
            value: "23:30".into(),
        },
    );
    assert_eq!(i.desired_patch()["dnd_start_hour"], "23:59");
}
#[test]
fn custom_none_six_day_limit_disabled_preview_and_partial_updates() {
    let mut raw = json!({"dnd_enabled":true,"dnd_days":"custom","dnd_enabled_sunday":"disabled","dnd_before_sunday":"8:00","dnd_after_sunday":"09:00"});
    for d in ScheduleDay::ALL.into_iter().take(6) {
        raw[format!("dnd_enabled_{}", d.as_str())] = json!("all_day");
    }
    let p = snapshot(raw);
    let sunday = p.view().unwrap().days.pop().unwrap();
    assert_eq!(sunday.start.as_deref(), Some("00:00"));
    assert_eq!(sunday.end.as_deref(), Some("23:59"));
    assert!(!sunday.can_none);
    assert!(
        NotificationScheduleIntent::capture(
            &workspace(),
            &p,
            ScheduleAction::None(ScheduleDay::Sunday)
        )
        .is_err()
    );
    let i = action(&p, ScheduleAction::None(ScheduleDay::Monday));
    assert_eq!(i.desired_patch().len(), 2);
    let i = action(
        &p,
        ScheduleAction::Time {
            day: Some(ScheduleDay::Sunday),
            bound: ScheduleBound::Start,
            value: "10:00".into(),
        },
    );
    assert_eq!(
        serde_json::to_value(i.desired_patch()).unwrap(),
        json!({"dnd_before_sunday":"10:00","dnd_after_sunday":"10:30","dnd_enabled_sunday":"partial","dnd_enabled":true})
    );
}
#[test]
fn irregular_option_insertion_matches_captured_head_and_negative_slice_rules() {
    let p = snapshot(
        json!({"dnd_enabled":true,"dnd_days":"every_day","dnd_end_hour":"09:00","dnd_start_hour":"08:45"}),
    );
    let options = p.options(None, ScheduleBound::End).unwrap();
    assert_eq!(options.first().map(String::as_str), Some("09:30"));
    assert_eq!(options.last().map(String::as_str), Some("08:45"));
    assert!(!options.contains(&"00:00".into()));
    let p = snapshot(json!({"dnd_enabled":true,"dnd_days":"every_day","dnd_end_hour":"23:59"}));
    let options = p.options(None, ScheduleBound::Start).unwrap();
    assert_eq!(
        &options[options.len() - 2..],
        &["23:59".to_string(), "23:30".to_string()]
    );
    let p = snapshot(
        json!({"dnd_enabled":true,"dnd_days":"custom","dnd_before_monday":"8:15","dnd_after_monday":"17:20","dnd_enabled_monday":"partial"}),
    );
    assert!(
        p.options(Some(ScheduleDay::Monday), ScheduleBound::Start)
            .unwrap()
            .contains(&"08:15".into())
    );
    assert!(
        p.options(Some(ScheduleDay::Monday), ScheduleBound::End)
            .unwrap()
            .contains(&"17:20".into())
    );
}
#[test]
fn preflight_pins_raw_times_timezone_and_owner_readback_requires_explicit_keys() {
    let p = snapshot(
        json!({"dnd_enabled":true,"dnd_days":"every_day","dnd_end_hour":"8:00","tz":null,"set_tz_automatically":true}),
    );
    let i = action(&p, ScheduleAction::Mode(ScheduleMode::Custom));
    let mut changed = p.clone();
    changed.raw.insert("dnd_end_hour".into(), json!("08:00"));
    assert!(i.revalidate(&changed).is_err());
    let mut changed = p.clone();
    changed.raw.insert("time24".into(), json!(true));
    assert!(i.revalidate(&changed).is_ok());
    let changed = p
        .clone()
        .with_user_timezone(&User {
            id: "U1".into(),
            tz: Some("Europe/London".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(i.revalidate(&changed).is_err());
    let mut changed = p.clone();
    changed.raw.remove("tz");
    assert!(i.revalidate(&changed).is_err());
    let mut other = workspace();
    other.user_id = "U2".into();
    assert!(notification_schedule_update_request(&SlackClient::default(), &other, &i, &p).is_err());
    let mut fresh = p.clone();
    fresh.raw.insert("dnd_days".into(), json!("custom"));
    assert!(i.confirms(&fresh));
    let other_zone = fresh
        .clone()
        .with_user_timezone(&User {
            id: "U1".into(),
            tz: Some("Europe/London".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(i.confirms(&other_zone));
    let other_user = fresh
        .clone()
        .with_user_timezone(&User {
            id: "U2".into(),
            tz: Some("America/New_York".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(!i.confirms(&other_user));
    fresh.raw.remove("dnd_enabled");
    assert!(!i.confirms(&fresh));
}
