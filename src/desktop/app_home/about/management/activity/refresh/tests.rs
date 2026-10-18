use super::*;

fn shell() -> (ShellState, u64) {
    let (mut s, visit) = super::super::tests::shell();
    let detail = ActivityDetailRequest::new("WF_REVIEW", "E1")
        .unwrap()
        .decode(json!({"ok":true,"workflow_event":{"trigger_message":{
            "template":"By {u}","variables":{"u":{"type":"USER","value":"U1"}}},
            "step_events":[]}}))
        .unwrap();
    let d = document_mut(&mut s, visit).unwrap();
    d.title = "Unsent title 🪷".into();
    d.description = "Unsent\nmultiline description".into();
    d.activity.detail = Some(detail);
    d.activity.detail_row = Some(json!({"workflow_execution_id":"E1","opaque":1}));
    d.activity.detail_revision = next();
    (s, visit)
}

#[test]
fn attempted_revision_deduplicates_failure_but_new_policy_can_retry() {
    let (mut s, visit) = shell();
    let ticket = Ticket::capture(&s).unwrap();
    assert!(ticket.current(&s));
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .name_refresh_attempted = Some(ticket.key.clone());
    assert!(Ticket::capture(&s).is_none());
    s.app_home.workflow_names_revision += 1;
    assert!(!ticket.current(&s));
    assert!(ticket.source_current(&s)); // Old read can clean up before latest starts.
    assert!(Ticket::capture(&s).is_some());
}

#[test]
fn exact_document_draft_row_filters_search_and_composition_retire_ticket() {
    let mutations: Vec<Box<dyn Fn(&mut ShellState, u64)>> = vec![
        Box::new(|s, v| document_mut(s, v).unwrap().title.push('!')),
        Box::new(|s, v| document_mut(s, v).unwrap().description.push('!')),
        Box::new(|s, v| document_mut(s, v).unwrap().revision = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().pending = Some(next())),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.detail_revision = next()),
        Box::new(|s, v| {
            document_mut(s, v)
                .unwrap()
                .activity
                .detail_row
                .as_mut()
                .unwrap()["opaque"] = json!(2)
        }),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.detail = None),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.filters.revision = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.user_search.push('x')),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.member_generation = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.member_composing = true),
        Box::new(|s, _| s.core.session.as_mut().unwrap().d_cookie.push('x')),
        Box::new(|s, _| {
            s.core.active_account = Some("00000000-0000-0000-0000-000000000099".into())
        }),
    ];
    for (i, mutate) in mutations.iter().enumerate() {
        let (mut s, visit) = shell();
        let ticket = Ticket::capture(&s).unwrap();
        mutate(&mut s, visit);
        assert!(!ticket.source_current(&s), "mutation {i}");
    }
}

#[test]
fn refresh_retains_detail_and_draft_and_only_releases_its_own_read() {
    let (mut s, visit) = shell();
    let before = document(&s, visit).unwrap().clone();
    let read = reads::Read::begin_refresh(&mut s, visit).unwrap();
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .name_refresh_read = Some(read.id());
    let after = document(&s, visit).unwrap();
    assert_eq!(after.title, before.title);
    assert_eq!(after.description, before.description);
    assert_eq!(after.pending, before.pending);
    assert_eq!(after.activity.detail, before.activity.detail);
    assert_eq!(after.activity.detail_row, before.activity.detail_row);
    assert_eq!(
        after.activity.detail_revision,
        before.activity.detail_revision
    );
    assert!(read.current(&s));
    assert!(Ticket::capture(&s).is_none());
    read.release(&mut s);
    assert!(document(&s, visit).unwrap().activity.detail_read.is_none());
    assert!(
        document(&s, visit)
            .unwrap()
            .activity
            .name_refresh_read
            .is_none()
    );
    let replacement = reads::Read::begin_refresh(&mut s, visit).unwrap();
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .name_refresh_read = Some(replacement.id());
    read.release(&mut s);
    assert!(replacement.current(&s));
    assert_eq!(
        document(&s, visit).unwrap().activity.name_refresh_read,
        Some(replacement.id())
    );
}

#[test]
fn retired_account_cleanup_releases_original_document_bookkeeping_only() {
    let (mut s, visit) = shell();
    let id = document(&s, visit).unwrap().id;
    let read = reads::Read::begin_refresh(&mut s, visit).unwrap();
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .name_refresh_read = Some(read.id());
    s.core.active_account = Some("00000000-0000-0000-0000-000000000099".into());
    assert!(!read.current(&s));
    read.release(&mut s);
    let original = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .find(|d| d.id == id)
        .unwrap();
    assert!(original.activity.name_refresh_read.is_none());
    assert!(original.activity.detail_read.is_none());
    assert!(original.activity.detail.is_some());
    assert_eq!(original.title, "Unsent title 🪷");
}
