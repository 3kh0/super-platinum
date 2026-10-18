use super::*;

fn shell() -> (ShellState, u64) {
    super::super::tests::shell()
}

#[test]
fn read_is_exclusive_and_old_completion_cannot_clear_its_replacement() {
    let (mut s, visit) = shell();
    let first = Read::begin(&mut s, visit).unwrap();
    assert!(first.current(&s));
    assert!(Read::begin(&mut s, visit).is_none());
    edit(&mut s, visit, Field::Status, "ERROR".into());
    assert!(!first.current(&s));
    let replacement = Read::begin(&mut s, visit).unwrap();
    assert!(!first.finish(&mut s));
    assert!(replacement.current(&s));
    assert_eq!(
        s.app_home.about.management.surface.as_ref().unwrap().read,
        Some(replacement.lease.id)
    );
    assert!(replacement.finish(&mut s));
    assert!(
        s.app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .read
            .is_none()
    );
}

#[test]
fn original_document_and_raw_breadcrumb_witness_reject_replacement_at_same_route() {
    let (mut s, visit) = shell();
    let first = Read::begin(&mut s, visit).unwrap();
    let original = document(&s, visit).unwrap().clone();
    let mut replacement = original.clone();
    replacement.id = next();
    s.app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .document = Some(replacement.id);
    s.app_home.about.management.documents.push(replacement);
    assert!(!first.current(&s));
    assert!(!first.finish(&mut s));
    s.app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .document = Some(original.id);
    assert!(first.current(&s));
    document_mut(&mut s, visit).unwrap().activity.breadcrumbs = vec![json!({
        "workflow_execution_id":"PARENT","opaque":{"version":2}})];
    assert!(!first.current(&s));
    assert!(!first.finish(&mut s));
}

#[test]
fn actual_account_and_serialized_session_changes_retire_original_read() {
    let (mut s, visit) = shell();
    let first = Read::begin(&mut s, visit).unwrap();
    s.core.active_account = Some("00000000-0000-0000-0000-000000000099".into());
    assert!(!first.current(&s));
    assert!(!first.finish(&mut s));
    let (mut s, visit) = shell();
    let first = Read::begin(&mut s, visit).unwrap();
    s.core.session.as_mut().unwrap().d_cookie = "replacement-synthetic-cookie".into();
    assert!(!first.current(&s));
}

#[test]
fn back_and_native_date_revisions_retire_reads_even_when_date_text_is_unchanged() {
    let (mut s, visit) = shell();
    let first = Read::begin(&mut s, visit).unwrap();
    edit(&mut s, visit, Field::From, String::new());
    assert!(!first.current(&s));
    assert!(!first.finish(&mut s));
    let second = Read::begin(&mut s, visit).unwrap();
    back(&mut s, visit);
    assert!(!second.current(&s));
    assert!(!second.finish(&mut s));
    assert!(Read::begin(&mut s, visit).is_none());
}

#[test]
fn closing_displayed_detail_preserves_unrelated_list_read() {
    let (mut s, visit) = shell();
    let row = json!({"workflow_execution_id":"EXECUTION"});
    let detail = ActivityDetailRequest::new("WF_REVIEW", "EXECUTION")
        .unwrap()
        .decode(json!({"ok":true,"workflow_event":{"step_events":[]}}))
        .unwrap();
    let a = &mut document_mut(&mut s, visit).unwrap().activity;
    a.detail = Some(detail);
    a.detail_row = Some(row);
    a.detail_revision = next();
    a.detail_read = None;
    let revision = a.detail_revision;
    let page_read = Read::begin(&mut s, visit).unwrap();
    close_detail(&mut s, visit, revision);
    assert!(document(&s, visit).unwrap().activity.detail.is_none());
    assert!(page_read.current(&s));
    assert!(page_read.finish(&mut s));
}

#[test]
fn closing_pending_detail_retires_its_response_without_clearing_new_read() {
    let (mut s, visit) = shell();
    let detail_read = Read::begin(&mut s, visit).unwrap();
    let a = &mut document_mut(&mut s, visit).unwrap().activity;
    a.detail_row = Some(json!({"workflow_execution_id":"EXECUTION"}));
    a.detail_revision = next();
    a.detail_read = Some(detail_read.lease.id);
    let revision = a.detail_revision;
    close_detail(&mut s, visit, revision);
    assert!(!detail_read.current(&s));
    let next = Read::begin(&mut s, visit).unwrap();
    close_detail(&mut s, visit, revision); // Old close callback must not clear it.
    assert!(!detail_read.finish(&mut s));
    assert!(next.current(&s));
    assert_ne!(
        document(&s, visit).unwrap().activity.detail_revision,
        revision
    );
}

#[test]
fn detail_name_reads_are_selected_by_rendered_user_parts_only() {
    for (trigger, steps, expected) in [
        (
            json!({"template":"No variable","variables":{"u":{"type":"USER","value":"U1"}}}),
            json!([]),
            false,
        ),
        (
            json!({"template":"{u}","variables":{"u":{"type":"USER","value":"U1"}}}),
            json!([]),
            true,
        ),
        (
            json!({"template":"{c}","variables":{"c":{"type":"CHANNEL","value":"C1"}}}),
            json!([]),
            false,
        ),
        (
            Value::Null,
            json!([{"outcome_message":{"template":"By {u}","variables":{"u":{"type":"USER","value":"U1"}}}}]),
            true,
        ),
    ] {
        let detail = ActivityDetailRequest::new("WF_REVIEW", "E1")
            .unwrap()
            .decode(
                json!({"ok":true,"workflow_event":{"trigger_message":trigger,"step_events":steps}}),
            )
            .unwrap();
        assert_eq!(detail_uses_names(&detail), expected);
    }
}

#[test]
fn pending_detail_policy_cannot_outlive_its_exact_selected_row_or_revision() {
    let (mut s, visit) = shell();
    let mut read = Read::begin(&mut s, visit).unwrap();
    let row = json!({"workflow_execution_id":"E1","opaque":1});
    let a = &mut document_mut(&mut s, visit).unwrap().activity;
    a.detail_revision = next();
    a.detail_row = Some(row.clone());
    a.detail_read = Some(read.lease.id);
    read.detail = Some((a.detail_revision, row));
    assert!(read.current(&s));
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .detail_row
        .as_mut()
        .unwrap()["opaque"] = json!(2);
    assert!(!read.current(&s));
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .detail_row
        .as_mut()
        .unwrap()["opaque"] = json!(1);
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .detail_revision = next();
    assert!(!read.current(&s));
}

#[test]
fn about_owned_name_preferences_invalidate_only_current_socket_account() {
    let (mut s, _) = shell();
    assert!(
        s.app_home
            .surface
            .as_ref()
            .is_none_or(|h| !h.owner.current(&s))
    );
    let owner = s.app_home.about.surface.as_ref().unwrap().owner.clone();
    let team = owner.ctx.session.team_id.clone();
    let event = super_platinum_core::slack::events::RtEvent::PreferencesChanged {
        name: "display_real_names_override".into(),
        value: json!(-1),
    };
    let revision = s.app_home.workflow_names_revision;
    crate::app_home::realtime(&mut s, &team, owner.rt.wrapping_add(1), &event);
    crate::app_home::realtime(&mut s, "OTHER", owner.rt, &event);
    assert_eq!(s.app_home.workflow_names_revision, revision);
    crate::app_home::realtime(&mut s, &team, owner.rt, &event);
    assert_eq!(s.app_home.workflow_names_revision, revision.wrapping_add(1));
    s.core.active_account = Some("foreign-account".into());
    crate::app_home::realtime(&mut s, &team, owner.rt, &event);
    assert_eq!(s.app_home.workflow_names_revision, revision.wrapping_add(1));
}
