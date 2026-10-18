use super::*;

fn shell() -> (ShellState, u64) {
    super::super::tests::shell()
}
fn page(s: &mut ShellState, visit: u64, rows: Value) {
    let d = document(s, visit).unwrap();
    let q = query(
        d.editor.workflow_id(),
        &d.activity.filters,
        Dates {
            from: None,
            to: None,
        },
    )
    .unwrap();
    let p = ActivityPage::decode(
        &q,
        json!({"ok":true,"workflow_events":rows,
        "response_metadata":{"next_cursor":"NEXT"}}),
    )
    .unwrap();
    adopt_page(
        &mut document_mut(s, visit).unwrap().activity,
        p,
        Page::First,
    );
}
fn armed() -> (ShellState, u64) {
    let (mut s, visit) = shell();
    assert!(arm(&mut s, visit, Some("EXECUTION".into())));
    document_mut(&mut s, visit).unwrap().activity.active = true;
    (s, visit)
}
fn selected() -> (ShellState, u64) {
    let (mut s, visit) = armed();
    page(
        &mut s,
        visit,
        json!([{"workflow_execution_id":"EXECUTION","opaque":{"version":7}}]),
    );
    (s, visit)
}

#[test]
fn fresh_entry_clears_old_activity_but_preserves_dirty_and_uncertain_metadata_and_filters() {
    let (mut s, visit) = shell();
    let d = document_mut(&mut s, visit).unwrap();
    d.title = "Unsent title".into();
    d.description = "Unsent\ntext".into();
    d.pending = Some(17);
    d.activity.filters.status = "ERROR".into();
    d.activity.filters.user = "U1".into();
    d.activity.breadcrumbs = vec![json!({"workflow_execution_id":"OLD"})];
    d.activity.detail_row = Some(json!({"workflow_execution_id":"OLD"}));
    let before = d.clone();
    assert!(arm(&mut s, visit, Some("EXECUTION".into())));
    let d = document(&s, visit).unwrap();
    assert_eq!(d.title, before.title);
    assert_eq!(d.description, before.description);
    assert_eq!(d.pending, before.pending);
    assert_eq!(d.revision, before.revision);
    assert!(d.editor.same_revision(&before.editor));
    assert!(d.activity.filters == before.activity.filters);
    assert!(!d.activity.active);
    assert!(d.activity.page.is_none());
    assert!(d.activity.breadcrumbs.is_empty());
    assert!(d.activity.detail_row.is_none());
    assert_eq!(
        d.activity.execution.as_ref().unwrap().execution,
        "EXECUTION"
    );
}

#[test]
fn absent_empty_or_missing_execution_never_fabricates_rows_or_claims_a_read() {
    for target in [None, Some(String::new())] {
        let (mut s, visit) = shell();
        assert!(arm(&mut s, visit, target));
        document_mut(&mut s, visit).unwrap().activity.active = true;
        page(
            &mut s,
            visit,
            json!([{"workflow_execution_id":"EXECUTION"}]),
        );
        assert!(Ticket::capture(&s, visit).is_none());
    }
    let (mut s, visit) = armed();
    assert!(Ticket::capture(&s, visit).is_none());
    page(&mut s, visit, json!([]));
    assert!(Ticket::capture(&s, visit).is_none());
    page(&mut s, visit, json!([{"workflow_execution_id":"OTHER"}]));
    assert!(Ticket::capture(&s, visit).is_none());
    let a = &document(&s, visit).unwrap().activity;
    assert!(!a.execution.as_ref().unwrap().consumed);
    assert!(a.detail_row.is_none());
    assert_eq!(a.page.as_ref().unwrap().rows().len(), 1);
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
    page(
        &mut s,
        visit,
        json!([{"workflow_execution_id":"EXECUTION","opaque":17}]),
    );
    assert_eq!(Ticket::capture(&s, visit).unwrap().row["opaque"], 17);
}

#[test]
fn only_nonloading_nonempty_exact_page_can_supply_the_initial_row() {
    let (mut s, visit) = selected();
    let read = reads::Read::begin(&mut s, visit).unwrap();
    assert!(Ticket::capture(&s, visit).is_none());
    read.release(&mut s);
    assert!(Ticket::capture(&s, visit).is_some());
    document_mut(&mut s, visit).unwrap().activity.breadcrumbs =
        vec![json!({"workflow_execution_id":"EXECUTION"})];
    page(&mut s, visit, json!([]));
    assert!(Ticket::capture(&s, visit).is_none());
}

#[test]
fn consume_uses_actual_row_and_once_state_survives_detail_close() {
    let (mut s, visit) = selected();
    let ticket = Ticket::capture(&s, visit).unwrap();
    let prepared = ticket.consume(&mut s).unwrap();
    let a = &document(&s, visit).unwrap().activity;
    assert!(a.execution.as_ref().unwrap().consumed);
    assert_eq!(a.detail_row.as_ref(), Some(&ticket.row));
    assert_eq!(a.detail_read, Some(prepared.id()));
    assert!(Ticket::capture(&s, visit).is_none());
    assert!(ticket.consume(&mut s).is_none());
    prepared.release(&mut s);
    close_detail(&mut s, visit, prepared.revision());
    assert!(
        document(&s, visit)
            .unwrap()
            .activity
            .execution
            .as_ref()
            .unwrap()
            .consumed
    );
    assert!(Ticket::capture(&s, visit).is_none());
    assert!(document(&s, visit).unwrap().activity.detail_row.is_none());
}

#[test]
fn replacement_entry_and_departure_retire_old_intent_without_metadata_changes() {
    let (mut s, visit) = selected();
    let ticket = Ticket::capture(&s, visit).unwrap();
    assert!(arm(&mut s, visit, Some("EXECUTION".into())));
    document_mut(&mut s, visit).unwrap().activity.active = true;
    page(
        &mut s,
        visit,
        json!([{"workflow_execution_id":"EXECUTION","opaque":{"version":7}}]),
    );
    assert!(!ticket.current(&s));
    let ticket = Ticket::capture(&s, visit).unwrap();
    back(&mut s, visit);
    document_mut(&mut s, visit).unwrap().activity.active = true;
    assert!(!ticket.current(&s));
    assert!(Ticket::capture(&s, visit).is_none());
}

#[test]
fn original_account_session_document_row_filters_and_native_generation_guard_consumption() {
    let mutations: Vec<Box<dyn Fn(&mut ShellState, u64)>> = vec![
        Box::new(|s, _| s.core.active_account = Some("another-account".into())),
        Box::new(|s, _| s.core.session.as_mut().unwrap().d_cookie.push('x')),
        Box::new(|s, v| document_mut(s, v).unwrap().revision = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().title.push('x')),
        Box::new(|s, v| document_mut(s, v).unwrap().pending = Some(next())),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.filters.revision = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.member_generation = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.member_composing = true),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.user_search.push('x')),
        Box::new(|s, v| {
            page(
                s,
                v,
                json!([{"workflow_execution_id":"EXECUTION","opaque":{"version":8}}]),
            )
        }),
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let (mut s, visit) = selected();
        let ticket = Ticket::capture(&s, visit).unwrap();
        mutate(&mut s, visit);
        assert!(!ticket.current(&s), "mutation {index}");
        assert!(ticket.consume(&mut s).is_none());
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
}

#[test]
fn loading_or_foreign_arm_cannot_replace_an_existing_intent() {
    let (mut s, visit) = selected();
    let before = document(&s, visit)
        .unwrap()
        .activity
        .execution
        .as_ref()
        .unwrap()
        .id;
    let read = reads::Read::begin(&mut s, visit).unwrap();
    assert!(!arm(&mut s, visit, Some("OTHER".into())));
    assert_eq!(
        document(&s, visit)
            .unwrap()
            .activity
            .execution
            .as_ref()
            .unwrap()
            .id,
        before
    );
    read.release(&mut s);
    assert!(!arm(&mut s, visit + 1, Some("OTHER".into())));
}

#[test]
fn back_navigation_snapshot_preserves_raw_filters_even_during_an_owned_read() {
    let (mut s, visit) = armed();
    let a = &mut document_mut(&mut s, visit).unwrap().activity;
    a.filters.from = "opaque native date".into();
    a.user_search = "Cafe\u{301}".into();
    let expected = navigation_fields(&s, visit).unwrap();
    assert_eq!(expected["filters"]["from"], "opaque native date");
    assert!(expected["filters"]["revision"].is_string());
    assert!(expected["memberGeneration"].is_string());
    let read = reads::Read::begin(&mut s, visit).unwrap();
    assert_eq!(navigation_fields(&s, visit), Some(expected));
    read.release(&mut s);
    back(&mut s, visit);
    assert!(navigation_fields(&s, visit).is_none());
}
