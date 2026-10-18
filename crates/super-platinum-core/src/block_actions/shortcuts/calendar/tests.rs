use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn form(r: PreparedRequest) -> std::collections::BTreeMap<String, String> {
    let RequestBody::Form(v) = r.body else {
        panic!("form required")
    };
    let n = v.len();
    let m: std::collections::BTreeMap<_, _> = v.into_iter().collect();
    assert_eq!(n, m.len());
    m
}
fn event() -> Event {
    Event {
        start: 1792332000,
        end: 1792333800,
        title: "\u{c9}quipe \u{6771}\u{4eac}".into(),
        description: "First\nSecond".into(),
        attendees: vec!["U2".into(), "W3".into()],
        calendar: "calendar@example.test".into(),
        meeting_url: "https://meeting.example.test/r1".into(),
        share_channel: "C1".into(),
        share_thread: "1700.000001".into(),
    }
}
#[test]
fn captured_modern_and_legacy_create_forms_are_distinct_and_complete() {
    let c = SlackClient::default();
    let s = session();
    let e = event();
    for p in [Provider::Google, Provider::Outlook, Provider::OutlookGcc] {
        let r = create(&c, &s, p, &Account::Modern, &e).unwrap();
        assert!(!r.retry_safe());
        let f = form(r);
        assert_eq!(f.len(), 10);
        assert_eq!(f["cal_app_type"], p.wire());
        assert!(!f.contains_key("channel"));
        assert!(!f.contains_key("thread_ts"));
        assert!(!f.contains_key("calapp_user_id"));
        assert_eq!(f["start"], "1792332000");
        assert_eq!(f["end"], "1792333800");
        assert_eq!(f["attendee_user_ids"], "U2,W3");
        assert_eq!(f["title"], e.title);
        assert_eq!(f["description"], e.description);
        assert_eq!(f["meeting_url"], e.meeting_url);
        assert_eq!(f["calendar_id"], e.calendar);
        assert_eq!(f["_x_reason"], "calendar-event-create-api");
        let f = form(
            create(
                &c,
                &s,
                p,
                &Account::Legacy {
                    user: "cal-1".into(),
                },
                &e,
            )
            .unwrap(),
        );
        assert_eq!(f.len(), 12);
        assert_eq!(f["calapp_user_id"], "cal-1");
        assert_eq!(f["channel"], "C1");
        assert_eq!(f["thread_ts"], "1700.000001");
        assert!(!f.contains_key("cal_app_type"));
    }
    let mut empty = e;
    empty.attendees.clear();
    empty.title.clear();
    empty.description.clear();
    empty.calendar.clear();
    empty.meeting_url.clear();
    empty.share_channel.clear();
    empty.share_thread.clear();
    assert!(empty.valid());
    let f = form(
        create(
            &c,
            &s,
            Provider::Google,
            &Account::Legacy {
                user: "cal-1".into(),
            },
            &empty,
        )
        .unwrap(),
    );
    assert!(!f.contains_key("attendee_user_ids"));
    for k in [
        "title",
        "description",
        "calendar_id",
        "meeting_url",
        "channel",
        "thread_ts",
    ] {
        assert_eq!(f[k], "");
    }
}
#[test]
fn active_availability_and_room_forms_match_captured_fetchers() {
    let c = SlackClient::default();
    let s = session();
    let a = Account::Legacy {
        user: "cal-1".into(),
    };
    let f = form(
        freebusy(
            &c,
            &s,
            Provider::OutlookGcc,
            &a,
            "2026-10-18",
            45,
            &["U2".into(), "U3".into()],
        )
        .unwrap(),
    );
    assert_eq!(f.len(), 7);
    assert_eq!(f["cal_user_id"], "cal-1");
    assert!(!f.contains_key("calapp_user_id"));
    assert_eq!(f["cal_app_type"], "OCAL_GCC");
    assert_eq!(f["day"], "2026-10-18");
    assert_eq!(f["minutes"], "45");
    assert_eq!(f["attendee_user_ids"], "U2,U3");
    assert_eq!(f["_x_reason"], "time-slot-selector");
    assert_eq!(
        form(connected_calendars(&c, &s, Provider::Google, &a))["calapp_user_id"],
        "cal-1"
    );
    let f = form(room(&c, &s, "A1", "C1").unwrap());
    assert_eq!(f.len(), 4);
    assert_eq!(f["app"], "A1");
    assert_eq!(f["channel"], "C1");
}
#[test]
fn calendar_special_flow_never_becomes_an_ordinary_execute() {
    let catalog=super::super::Catalog::decode(json!({"ok":true,"app_actions":[{"app_id":"ADZ494LHY","app_name":"Calendar","teams":["T1"],"actions":[{"action_id":"create","type":"global_action","name":"Create event"},{"action_id":"message","type":"message_action","name":"Calendar message"}]}]})).unwrap();
    let a = &catalog.actions[0];
    assert_eq!(Provider::for_action(a), Some(Provider::Google));
    assert!(a.available_in_chooser(&[]));
    assert!(!a.eligible(&["T1".into()]));
    assert!(
        super::super::prepare(
            &SlackClient::default(),
            &session(),
            &catalog,
            a,
            &super::super::Context {
                channel: Some("C1".into()),
                message_ts: None
            },
            &["T1".into()],
            Some("T1"),
            "token"
        )
        .is_err()
    );
    assert!(Provider::for_action(&catalog.actions[1]).is_none());
}
#[test]
fn malformed_and_unavailable_discovery_never_grants_a_connected_account() {
    assert!(modern(&json!({"ok":true})).is_err());
    assert!(!modern(&json!({"ok":true,"assignments":{}})).unwrap());
    assert!(
        modern(&json!({"ok":true,"assignments":{"calendar2webapp_create_event":{"group":"on"}}}))
            .unwrap()
    );
    assert_eq!(
        connection(&json!({"ok":true,"cal_user":{"id":"cal-1","is_provider_connected":true}}))
            .unwrap(),
        Connection::Connected(Account::Legacy {
            user: "cal-1".into()
        })
    );
    assert_eq!(
        connection(&json!({"ok":true,"auth_url":"https://calendar.example.test/connect"})).unwrap(),
        Connection::Connect {
            url: "https://calendar.example.test/connect".into()
        }
    );
    assert!(connection(&json!({"ok":true,"cal_user":{"is_provider_connected":true}})).is_err());
    assert_eq!(
        connection(&json!({"ok":true,"auth_url":"javascript:alert(1)"})).unwrap(),
        Connection::Unavailable
    );
    assert!(
        calendars(
            &json!({"ok":true,"connected_calendars":[{"id":"a","name":"A"},{"id":"a","name":"B"}]})
        )
        .is_err()
    );
}
#[test]
fn invalid_time_and_attendee_capture_is_refused_without_rewriting_the_draft() {
    let original = event();
    for variant in 0..5 {
        let mut e = original.clone();
        match variant {
            0 => e.end = e.start,
            1 => e.attendees.push("U2".into()),
            2 => e.attendees[0] = "U2,U3".into(),
            3 => e.meeting_url = "javascript:alert(1)".into(),
            _ => e.share_channel.clear(),
        };
        let before = e.clone();
        assert!(
            create(
                &SlackClient::default(),
                &session(),
                Provider::Google,
                &Account::Modern,
                &e
            )
            .is_err()
        );
        assert_eq!(before, e);
    }
    for duration in [0, 1, 29, 31, 1440] {
        assert!(
            freebusy(
                &SlackClient::default(),
                &session(),
                Provider::Google,
                &Account::Modern,
                "2026-10-18",
                duration,
                &[]
            )
            .is_err()
        );
    }
}
#[test]
fn success_rejection_and_unknown_are_not_interchangeable() {
    assert!(matches!(
        event_result(
            &json!({"ok":true,"provider_event_link":"https://calendar.example.test/event"})
        ),
        EventResult::Created {
            provider_link: Some(_),
            ..
        }
    ));
    assert_eq!(
        event_result(&json!({"ok":false,"error":"invalid_date"})),
        EventResult::Rejected
    );
    for raw in [
        json!({}),
        json!({"ok":false}),
        json!({"ok":false,"error":"dispatch_failed"}),
        json!({"ok":"true"}),
    ] {
        assert_eq!(event_result(&raw), EventResult::Unknown);
    }
    assert!(room_url(&json!({"ok":true})).is_err());
    assert!(room_url(&json!({"ok":true,"url":"file:///tmp/event"})).is_err());
}
#[test]
fn sharing_preference_absence_and_explicit_false_follow_actual_selector() {
    for (value, want) in [
        (None, true),
        (Some(json!(false)), false),
        (Some(json!(null)), false),
        (Some(json!(0)), false),
        (Some(json!("false")), true),
        (Some(json!(true)), true),
    ] {
        let mut raw = json!({"ok":true,"prefs":{}});
        if let Some(v) = value {
            raw["prefs"]["calendar_share_event_in_channel"] = v;
        }
        assert_eq!(share_enabled(&raw).unwrap(), want);
    }
    let f = form(set_share_preference(
        &SlackClient::default(),
        &session(),
        false,
    ));
    assert_eq!(f.len(), 4);
    assert_eq!(f["name"], "calendar_share_event_in_channel");
    assert_eq!(f["value"], "false");
    assert_eq!(f["_x_reason"], "prefs-api/setUserPrefByApi");
}
