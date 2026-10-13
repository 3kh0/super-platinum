use super::*;
use super_platinum_core::{config::Session, slack::models::Message};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.session = Some(Session {
        d_cookie: "fixture".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U0".into(),
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: String::new(),
            },
        )]
        .into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("fixture").unwrap()));
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 4;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C1".into())
        .or_default()
        .upsert(Message {
            ts: Some("900.0".into()),
            files: vec![file("original")],
            ..Default::default()
        });
    shell
}
fn file(title: &str) -> File {
    File {
        id: Some("F1".into()),
        title: Some(title.into()),
        name: Some(format!("{title}.txt")),
        ..Default::default()
    }
}
fn event() -> RtEvent {
    parse(r#"{"type":"file_change","file_id":"F1","file":{"id":"F1"}}"#)
}
fn parse(json: &str) -> RtEvent {
    super_platinum_core::slack::realtime::parse_event(json).unwrap()
}
fn title(shell: &ShellState) -> String {
    shell.core.workspaces["T1"].messages["C1"]
        .messages
        .iter()
        .find(|message| message.ts.as_deref() == Some("900.0"))
        .unwrap()
        .files[0]
        .title
        .clone()
        .unwrap()
}

#[test]
fn accepts_id_only_event_shapes_and_rejects_conflicting_ids() {
    assert_eq!(changed_id(&event()), Some("F1"));
    assert_eq!(
        changed_id(&parse(r#"{"type":"file_change","file":{"id":"F1"}}"#)),
        Some("F1")
    );
    assert_eq!(
        changed_id(&parse(r#"{"type":"file_change","file_id":"F1"}"#)),
        Some("F1")
    );
    assert_eq!(
        changed_id(&parse(
            r#"{"type":"file_change","file_id":"F1","file":{"id":"F2"}}"#
        )),
        None
    );
    assert_eq!(
        changed_id(&parse(r#"{"type":"file_change","file_id":""}"#)),
        None
    );
}

#[test]
fn burst_reads_are_serialized_and_only_latest_result_applies() {
    let mut shell = shell();
    let first = prepare(&mut shell, "T1", 4, &event()).unwrap();
    for _ in 0..5 {
        assert!(prepare(&mut shell, "T1", 4, &event()).is_none());
    }
    let (next, changed) = first.finish(&mut shell, Ok(file("outdated")));
    assert!(!changed);
    assert_eq!(title(&shell), "original");
    let (next, changed) = next.unwrap().finish(&mut shell, Ok(file("fresh")));
    assert!(next.is_none());
    assert!(changed);
    assert_eq!(title(&shell), "fresh");
    assert!(shell.file_events.pending.is_empty());
}

#[test]
fn local_snapshot_preserves_newer_event_and_discards_older_fetch() {
    let mut shell = shell();
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    // Local verification was already on the wire when another client changed
    // the title. Its older snapshot must not cancel that client's queued read.
    assert!(prepare(&mut shell, "T1", 4, &event()).is_none());
    invalidate(&mut shell, "T1", "F1");
    reconcile_metadata(&mut shell, "T1", &file("local snapshot"));
    let (fresh, changed) = read.finish(&mut shell, Ok(file("stale metadata")));
    assert!(!changed);
    assert_eq!(title(&shell), "local snapshot");
    let (next, changed) = fresh
        .unwrap()
        .finish(&mut shell, Ok(file("newer remote title")));
    assert!(next.is_none());
    assert!(changed);
    assert_eq!(title(&shell), "newer remote title");
    let pending = prepare(&mut shell, "T1", 4, &event()).unwrap();
    shell.core.remove_file("T1", "F1");
    assert!(!pending.finish(&mut shell, Ok(file("resurrected"))).1);
    assert!(prepare(&mut shell, "T1", 4, &event()).is_none());
}

#[test]
fn account_transport_and_socket_changes_reject_reads_but_navigation_does_not() {
    let mut shell = shell();
    assert!(prepare(&mut shell, "T1", 3, &event()).is_none());
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    shell.core.active_team = Some("OTHER".into());
    assert!(read.owns(&shell));
    assert!(read.finish(&mut shell, Ok(file("background"))).1);
    assert_eq!(title(&shell), "background");
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    shell.core.account_epoch += 1;
    assert!(!read.finish(&mut shell, Ok(file("wrong account"))).1);
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    shell.core.transport = Some(Arc::new(Transport::new("replacement").unwrap()));
    assert!(!read.finish(&mut shell, Ok(file("wrong transport"))).1);
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 5;
    assert!(!read.finish(&mut shell, Ok(file("wrong socket"))).1);
    assert_eq!(title(&shell), "background");
}

#[test]
fn inaccessible_or_mismatched_response_preserves_cache_and_confirmed_delete_tombstones() {
    let mut shell = shell();
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    assert!(
        !read
            .finish(&mut shell, Err(Error::Api("file_not_found".into())))
            .1
    );
    assert_eq!(title(&shell), "original");
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    assert!(
        !read
            .finish(
                &mut shell,
                Ok(File {
                    id: Some("OTHER".into()),
                    ..file("wrong")
                })
            )
            .1
    );
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    assert!(
        read.finish(&mut shell, Err(Error::Api("file_deleted".into())))
            .1
    );
    assert!(shell.core.workspaces["T1"].deleted_files.contains("F1"));
}

#[test]
fn realtime_delete_cancels_pending_fetch_without_accepting_stale_generation() {
    let mut shell = shell();
    let read = prepare(&mut shell, "T1", 4, &event()).unwrap();
    let deletion = parse(r#"{"type":"file_deleted","file_id":"F1"}"#);
    prepare(&mut shell, "T1", 3, &deletion);
    assert!(read.owns(&shell));
    prepare(&mut shell, "T1", 4, &deletion);
    assert!(!read.owns(&shell));
    assert!(!read.finish(&mut shell, Ok(file("stale"))).1);
}

#[test]
fn completed_remote_refresh_invalidates_an_older_local_snapshot() {
    let mut shell = shell();
    let before_local_read = snapshot_revision(&shell);
    let remote = prepare(&mut shell, "T1", 4, &event()).unwrap();
    assert!(remote.finish(&mut shell, Ok(file("remote"))).1);
    assert!(shell.file_events.pending.is_empty());
    assert!(changed_since(&shell, "T1", "F1", before_local_read));
    assert!(!changed_since(&shell, "T1", "OTHER", before_local_read));
    let after_remote_read = snapshot_revision(&shell);
    assert!(!changed_since(&shell, "T1", "F1", after_remote_read));
    shell.core.transport = Some(Arc::new(Transport::new("replacement").unwrap()));
    assert!(!changed_since(&shell, "T1", "F1", before_local_read));
    assert_eq!(title(&shell), "remote");
}
