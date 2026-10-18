use super::*;

fn scene() -> (ShellState, u64) {
    let (mut s, visit) = crate::app_home::about::management::activity::tests::shell();
    let d = document_mut(&mut s, visit).unwrap();
    d.title = "Retained title 中".into();
    d.description = "Retained\ndescription".into();
    d.activity.detail_row =
        Some(json!({"workflow_execution_id":"EXECUTION","workflow_instance_status":"SUCCESS"}));
    d.activity.detail = Some(ActivityDetailRequest::new("WF_REVIEW","EXECUTION").unwrap()
        .decode(json!({"ok":true,"workflow_event":{
            "workflow_execution_id":"EXECUTION",
            "trigger_message":{"template":"Open {c} then {c}","variables":{"c":{"type":"CHANNEL","value":"C2"}}},
            "step_events":[{"outcome_message":{"template":"Check {c}","variables":{"c":{"type":"CHANNEL","value":"C2"}}}}]
        }})).unwrap());
    (s, visit)
}
fn source(s: &ShellState, visit: u64) -> Source {
    Source::capture(s, visit, Location::Trigger, 1, "C2").unwrap()
}

#[test]
fn duplicate_channels_remain_distinct_occurrences_and_plain_text_has_no_action() {
    let (s, visit) = scene();
    let first = source(&s, visit);
    let second = Source::capture(&s, visit, Location::Trigger, 3, "C2").unwrap();
    let step = Source::capture(&s, visit, Location::Step(0), 1, "C2").unwrap();
    assert!(first.current(&s) && second.current(&s) && step.current(&s));
    assert_ne!(first.button(), second.button());
    assert_ne!(first.button(), step.button());
    for (location, part, id) in [
        (Location::Trigger, 0, "C2"),
        (Location::Trigger, 1, "C3"),
        (Location::Step(1), 1, "C2"),
    ] {
        assert!(Source::capture(&s, visit, location, part, id).is_none());
    }
    assert_eq!(first.expected(1)["chain"], 1);
    assert_eq!(first.expected(2)["chain"], 2);
    assert!(first.expected(1).get("credentials").is_none());
}

#[test]
fn same_execution_and_channel_cannot_reuse_changed_raw_detail_or_row() {
    for mutation in 0..4 {
        let (mut s, visit) = scene();
        let old = source(&s, visit);
        let d = document_mut(&mut s, visit).unwrap();
        match mutation {
            0 => {
                let mut raw = d.activity.detail.as_ref().unwrap().raw().clone();
                raw["workflow_event"]["trigger_message"]["template"] =
                    json!("Changed {c} then {c}");
                d.activity.detail = Some(
                    ActivityDetailRequest::new("WF_REVIEW", "EXECUTION")
                        .unwrap()
                        .decode(raw)
                        .unwrap(),
                );
            }
            1 => d.activity.detail_row.as_mut().unwrap()["future"] = json!("new original row"),
            2 => d.activity.detail_revision = next(),
            _ => {
                d.activity.active = false;
            }
        }
        assert!(!old.current(&s), "mutation {mutation}");
    }
}

#[test]
fn exact_editor_filter_search_and_route_witnesses_retire_old_callback() {
    for mutation in 0..11 {
        let (mut s, visit) = scene();
        let old = source(&s, visit);
        let d = document_mut(&mut s, visit).unwrap();
        match mutation {
            0 => d.id = next(),
            1 => d.revision = next(),
            2 => d.title.push('x'),
            3 => d.description.push('x'),
            4 => d.pending = Some(next()),
            5 => d.activity.filters.user = "UOTHER".into(),
            6 => d.activity.member_generation = next(),
            7 => d.activity.user_search = "native next query".into(),
            8 => d.activity.member_composing = true,
            9 => d.activity.detail_read = Some(next()),
            _ => s.profile_generation = s.profile_generation.wrapping_add(1),
        }
        assert!(!old.current(&s), "mutation {mutation}");
    }
}

#[test]
fn account_cookie_credential_transport_and_socket_changes_retire_source() {
    for mutation in 0..5 {
        let (mut s, visit) = scene();
        let old = source(&s, visit);
        match mutation {
            0 => s.core.active_account = Some("other-account".into()),
            1 => s.core.session.as_mut().unwrap().d_cookie.push('x'),
            2 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("TWORKFLOW")
                .unwrap()
                .token
                .push('x'),
            3 => s.core.transport = None,
            _ => {
                s.core
                    .workspaces
                    .get_mut("TWORKFLOW")
                    .unwrap()
                    .rt_generation += 1
            }
        }
        assert!(!old.current(&s), "mutation {mutation}");
    }
}

#[test]
fn capture_and_refusal_do_not_mutate_retained_draft_or_pending_receipt() {
    let (mut s, visit) = scene();
    document_mut(&mut s, visit).unwrap().pending = Some(42);
    let old = source(&s, visit);
    assert!(old.current(&s));
    let d = document(&s, visit).unwrap();
    assert_eq!(d.title, "Retained title 中");
    assert_eq!(d.description, "Retained\ndescription");
    assert_eq!(d.pending, Some(42));
    assert!(d.dirty());
    s.channel_generation += 1;
    assert!(!old.current(&s));
    let d = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .find(|d| d.id == old.document)
        .unwrap();
    assert_eq!(d.pending, Some(42));
    assert_eq!(d.title, "Retained title 中");
    assert!(d.dirty());
}
