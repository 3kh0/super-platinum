use super::*;
use crate::config::WorkspaceSession;
use crate::slack::client::{RequestBody, SlackClient};
use serde_json::json;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_SYNTHETIC".into(),
        user_id: "U_SYNTHETIC".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn status(value: &str) -> CalendarStatus {
    CalendarStatus::decode(
        json!({"ok":true,"status":[{"cal_app_type":"GCAL","user_status":value,"future":7}]}),
    )
    .unwrap()
}
fn event(id: &str, link: &str, title: &str) -> Event {
    decode_event(link,&json!({"ok":true,"event":{"id":id,"event_link_id":link,"title":title,"start":100,"future":{"v":[1,2]}}})).unwrap()
}
fn owner() -> ReadOwner {
    ReadOwner {
        team: "T_SYNTHETIC".into(),
        user: "U_SYNTHETIC".into(),
        credential_generation: 7,
        query_generation: 11,
    }
}
#[test]
fn status_is_positive_observation_not_empty_default_or_provider_pending() {
    for raw in [
        json!({}),
        json!({"ok":false}),
        json!({"ok":true,"status":[]}),
        json!({"ok":true,"status":[{}]}),
        json!({"ok":true,"status":[{"cal_app_type":"GCAL","user_status":null}]}),
        json!({"ok":true,"status":[{"cal_app_type":"GCAL","user_status":"connected"}],"error":"bad"}),
    ] {
        assert!(CalendarStatus::decode(raw).is_err());
    }
    assert!(!status("provider_connected").connected());
    assert!(!status("no_auth").connected());
    assert!(status("connected").connected());
    assert_eq!(status("connected").raw()["status"][0]["future"], 7);
    let duplicate=CalendarStatus::decode(json!({"ok":true,"status":[
        {"cal_app_type":"GCAL","user_status":"connected"},{"cal_app_type":"GCAL","user_status":"no_auth"}]})).unwrap();
    assert!(!duplicate.connected());
}
#[test]
fn actual_prepared_read_requests_have_exact_domain_fields() {
    let client = SlackClient::default();
    let ws = workspace();
    let request = request::user_status(&client, &ws);
    assert_eq!(
        request.url.split('?').next(),
        Some("https://fixture.slack.com/api/calendar.user.status")
    );
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.body,
        RequestBody::Form(vec![("token".into(), "fixture-only".into())])
    );
    let request = request::event_by_link(&client, &ws, &status("connected"), "L:1").unwrap();
    assert_eq!(
        request.url.split('?').next(),
        Some("https://fixture.slack.com/api/calendar.event.getByEventLinkId")
    );
    assert_eq!(
        request.body,
        RequestBody::Form(vec![
            ("token".into(), "fixture-only".into()),
            ("event_link_id".into(), "L:1".into())
        ])
    );
    assert!(!request.retry_safe());
    assert!(request::event_by_link(&client, &ws, &status("no_auth"), "L1").is_err());
    for link in ["", "L 1", "L\n1"] {
        assert!(request::event_by_link(&client, &ws, &status("connected"), link).is_err());
    }
}
#[test]
fn window_builder_uses_actual_date_range_shape() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let days = request::initial_days(today).unwrap();
    assert_eq!(
        days,
        vec![
            "2025-12-31",
            "2026-01-01",
            "2026-01-02",
            "2026-01-03",
            "2026-01-04"
        ]
    );
    let request = request::window(
        &SlackClient::default(),
        &workspace(),
        &status("connected"),
        "GCAL",
        &days,
    )
    .unwrap();
    assert_eq!(
        request.url.split('?').next(),
        Some("https://fixture.slack.com/api/calendar.event.listByDateRange")
    );
    let RequestBody::Form(fields) = request.body else {
        panic!("actual form expected")
    };
    assert_eq!(
        fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        vec!["token", "cal_app_type", "utc_days", "days"]
    );
    assert_eq!(fields[1].1, "GCAL");
    assert_eq!(fields[2].1, "true");
    assert_eq!(
        serde_json::from_str::<Value>(&fields[3].1).unwrap(),
        json!(days.iter().map(|d| json!({"day":d})).collect::<Vec<_>>())
    );
    assert!(
        request::window(
            &SlackClient::default(),
            &workspace(),
            &status("connected"),
            "GCAL",
            &["2026-02-31".into()]
        )
        .is_err()
    );
}
#[test]
fn unsuccessful_and_malformed_event_reads_never_authorize_missing() {
    for raw in [
        json!({"ok":false,"error":"not_found"}),
        json!({"ok":true}),
        json!({"ok":true,"event":null}),
        json!({"ok":true,"event":{}}),
        json!({"ok":true,"event":{"id":"E1","event_link_id":"OTHER"}}),
        json!({"ok":true,"event":{"id":"E1"},"error":"unknown"}),
    ] {
        assert!(decode_event("L1", &raw).is_err());
    }
    let e = decode_event(
        "L1",
        &json!({"ok":true,"event":{"id":"E1","event_link_id":null,"opaque":[1,2]}}),
    )
    .unwrap();
    assert_eq!(e.link(), Some("L1"));
    assert_eq!(e.raw()["opaque"], json!([1, 2]));
}
#[test]
fn format_inputs_preserve_unknowns_and_do_not_guess_timezone_or_locale() {
    let e = event("E1", "L1", "A &amp; B");
    assert_eq!(
        e.format_input().unwrap(),
        FormatInput {
            raw_title: "A &amp; B",
            start: Some(100.0),
            all_day: false
        }
    );
    let e = decode_event(
        "L1",
        &json!({"ok":true,"event":{"id":"E1","title":"Only title","is_all_day":true}}),
    )
    .unwrap();
    assert_eq!(e.format_input().unwrap().start, None);
    assert_eq!(
        serde_json::to_value(e.format_input().unwrap()).unwrap(),
        json!({"title":"Only title","allDay":true})
    );
    for start in [Value::Null, json!("100"), json!({})] {
        let e = decode_event("L1", &json!({"ok":true,"event":{"id":"E1","start":start}})).unwrap();
        assert!(e.format_input().is_err());
    }
}
#[test]
fn account_credential_query_and_link_replacement_refuse_adoption() {
    let captured = EventRead::capture(owner(), "L1".into()).unwrap();
    let raw = json!({"ok":true,"event":{"id":"E1","title":"Known"}});
    assert!(captured.adopt(&owner(), "L1", &raw).is_ok());
    for mutation in 0..4 {
        let mut next = owner();
        match mutation {
            0 => next.team = "T_OTHER".into(),
            1 => next.user = "U_OTHER".into(),
            2 => next.credential_generation += 1,
            _ => next.query_generation += 1,
        }
        assert!(!captured.current(&next, "L1"));
        assert!(captured.adopt(&next, "L1", &raw).is_err());
    }
    assert!(captured.adopt(&owner(), "L2", &raw).is_err());
}
#[test]
fn first_index_order_and_full_replacement_match_source() {
    let mut window = EventWindow::default();
    window.upsert(event("E1", "L1", "First")).unwrap();
    window.upsert(event("E2", "L1", "Second")).unwrap();
    assert_eq!(window.by_link("L1").unwrap().id(), "E1");
    window.upsert(event("E1", "L2", "Moved")).unwrap();
    assert_eq!(window.by_link("L1").unwrap().id(), "E2");
    assert_eq!(
        window.by_link("L2").unwrap().raw()["future"],
        json!({"v":[1,2]})
    );
    let removed = window
        .realtime("deleted", json!({"id":"E1"}))
        .unwrap()
        .unwrap();
    assert_eq!(removed.id(), "E1");
    assert!(window.by_link("L2").is_none());
    assert!(window.by_link("L1").is_some());
}
#[test]
fn realtime_unknown_subtype_and_malformed_event_are_atomic() {
    let mut window = EventWindow::default();
    window.upsert(event("E1", "L1", "First")).unwrap();
    let before = window.clone();
    assert!(window.realtime("future", json!({"id":"E1"})).is_err());
    assert!(window.realtime("updated", json!({"id":null})).is_err());
    assert_eq!(window, before);
}
#[test]
fn realtime_id_format_change_requires_refresh_without_mixing_generations() {
    let mut window = EventWindow::default();
    window.upsert(event("E1", "L1", "Keep")).unwrap();
    let before = window.clone();
    assert_eq!(
        window.realtime("updated", json!({"id":"123","event_link_id":"L1"})),
        Err(Fault::RefreshRequired)
    );
    assert_eq!(
        window.realtime("deleted", json!({"id":"legacy:1"})),
        Err(Fault::RefreshRequired)
    );
    assert_eq!(window, before);
}
#[test]
fn complete_window_validation_preserves_old_snapshot_on_syncing_or_partial() {
    let requested = vec!["2026-10-17".into(), "2026-10-18".into()];
    for raw in [
        json!({"ok":false}),
        json!({"ok":true,"days":[]}),
        json!({"ok":true,"days":[{"day":"2026-10-17","events":[]}]}),
        json!({"ok":true,"days":[{"day":"2026-10-17","events":[]},{"day":"2026-10-18","sync_status":"syncing","events":[]}]}),
    ] {
        assert!(decode_window(&raw, &requested).is_err());
    }
}
#[test]
fn replacing_one_day_retains_event_referenced_by_another_day() {
    let mut window = EventWindow::default();
    let days = vec!["2026-10-17".into(), "2026-10-18".into()];
    let e = json!({"id":"E1","event_link_id":"L1","title":"Both"});
    let raw = json!({"ok":true,"days":[{"day":days[0],"events":[e.clone()]},{"day":days[1],"events":[e]}]});
    window
        .replace_days(decode_window(&raw, &days).unwrap())
        .unwrap();
    let empty = json!({"ok":true,"days":[{"day":"2026-10-17","events":[]}]});
    window
        .replace_days(decode_window(&empty, &days[..1]).unwrap())
        .unwrap();
    assert!(window.by_link("L1").is_some());
    assert!(window.day_loaded(&days[0]));
    let empty = json!({"ok":true,"days":[{"day":"2026-10-18","events":[]}]});
    window
        .replace_days(decode_window(&empty, &days[1..]).unwrap())
        .unwrap();
    assert!(window.by_link("L1").is_none());
    // A passive None here is window membership only, not an authoritative
    // server not-found result for all time or an unqueried event link.
}
#[test]
fn window_filters_working_location_and_deleted_without_inventing_events() {
    let days = vec!["2026-10-17".into()];
    let raw = json!({"ok":true,"days":[{"day":days[0],"events":[
        {"id":"E1","event_link_id":"L1","calendar_event_type":"working_location"},
        {"id":"E2","event_link_id":"L2","is_deleted":true},
        {"id":"E3","event_link_id":"L3","opaque":"retained"}]}]});
    let mut window = EventWindow::default();
    window
        .replace_days(decode_window(&raw, &days).unwrap())
        .unwrap();
    assert!(window.by_link("L1").is_none());
    assert!(window.by_link("L2").is_none());
    assert_eq!(window.by_link("L3").unwrap().raw()["opaque"], "retained");
}
#[test]
fn capacity_and_invalid_date_fail_without_partial_adoption() {
    let mut window = EventWindow::default();
    window.upsert(event("E0", "L0", "Keep")).unwrap();
    let before = window.clone();
    let e = decode_event(
        "L1",
        &json!({"ok":true,"event":{"id":"E1","start":1,"end":1000000000000_i64}}),
    )
    .unwrap();
    assert!(window.upsert(e).is_err());
    assert_eq!(window, before);
    let raw = json!({"ok":true,"days":[{"day":"2026-10-17","events":[{"id":"E1"},{"id":"E1"}]}]});
    assert!(decode_window(&raw, &["2026-10-17".into()]).is_err());
}

#[test]
fn realtime_uses_source_first_key_in_mixed_acquisition_and_replacement() {
    let mut window = EventWindow::default();
    window.upsert(event("zEncoded", "L1", "First")).unwrap();
    window.upsert(event("a:legacy", "L2", "Second")).unwrap();
    // Updating an existing key preserves its original enumeration slot.
    window.upsert(event("zEncoded", "L3", "Updated")).unwrap();
    assert!(
        window
            .realtime("created", json!({"id":"newEncoded"}))
            .is_ok()
    );
    let before = window.clone();
    assert_eq!(
        window.realtime("updated", json!({"id":"new:legacy"})),
        Err(Fault::RefreshRequired)
    );
    assert_eq!(window, before);
    // Deleting then reinserting places the string key after the surviving key.
    window
        .realtime("deleted", json!({"id":"zEncoded"}))
        .unwrap();
    window
        .upsert(event("zEncoded", "L1", "Reinserted"))
        .unwrap();
    assert_eq!(
        window.realtime("updated", json!({"id":"zEncoded"})),
        Err(Fault::RefreshRequired)
    );
    assert!(window.realtime("updated", json!({"id":"a:legacy"})).is_ok());
    // Numeric array-index IDs outrank every string insertion slot.
    window.upsert(event("2", "L4", "Numeric")).unwrap();
    assert!(window.realtime("created", json!({"id":"3"})).is_ok());
    assert_eq!(
        window.realtime("updated", json!({"id":"a:legacy"})),
        Err(Fault::RefreshRequired)
    );
    window.clear();
    assert!(
        window
            .realtime("created", json!({"id":"resetEncoded"}))
            .is_ok()
    );
}

#[test]
fn window_acquisition_and_day_replacement_preserve_object_key_order() {
    let day = vec!["2026-10-17".to_owned()];
    let page = |ids: &[&str]| {
        decode_window(&json!({"ok":true,"days":[{"day":day[0],"events":ids.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}]}), &day).unwrap()
    };
    let mut window = EventWindow::default();
    window
        .replace_days(page(&["zEncoded", "a:legacy"]))
        .unwrap();
    assert!(
        window
            .realtime("created", json!({"id":"newEncoded"}))
            .is_ok()
    );
    window
        .replace_days(page(&["a:legacy", "zEncoded"]))
        .unwrap();
    // newEncoded was retained by the direct realtime acquisition and is now first.
    assert!(
        window
            .realtime("deleted", json!({"id":"newEncoded"}))
            .is_ok()
    );
    assert!(window.realtime("updated", json!({"id":"a:legacy"})).is_ok());
    assert_eq!(
        window.realtime("updated", json!({"id":"zEncoded"})),
        Err(Fault::RefreshRequired)
    );
}
