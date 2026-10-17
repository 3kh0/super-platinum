use super::*;
use crate::slack_lists::editor;
#[path = "../../../check/text_field_create/link/oracle.rs"]
mod oracle;

// The production arm requires the native creation ledger. This test-only arm
// starts at that continuation boundary; it never fabricates worker observations.
fn setup() -> ShellState {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-link-field-create",
    );
    assert!(crate::slack_lists::rename::fixture::close_link_fixture(
        &shell
    ));
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().insert(1,json!({"id":COLUMN,"key":KEY,"name":"Reference \u{00c9}quipe \u{6771}\u{4eac}","type":"link","opaque":{"fixture":"retained"}}));
    f.document["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .insert(1, json!({"id":COLUMN,"key":KEY,"visible":true}));
    super::super::super::refresh(&mut shell);
    let scope = shell.slack_lists.scope.clone().unwrap();
    let visit = generation(&shell).unwrap();
    let selection = rename::selection(&shell).unwrap();
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    let document = f.document.clone();
    let pages = pages(&f.records).unwrap();
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.link_create = Some(State {
        scope: scope.clone(),
        visit,
        selection,
        document: document.clone(),
        pages: pages.clone(),
        sync: json!({"visit":visit,"selection":selection,"owner":scope.stable_owner(),"after":document,"population":pages[0]}),
        preflights: vec![],
        requests: vec![],
        completed: vec![],
        reads: vec![],
        test_only: true,
        error: None,
    });
    shell
}
fn open(shell: &mut ShellState) -> u64 {
    let visit = generation(shell).unwrap();
    editor::begin_shell(shell, ROW, COLUMN, visit).unwrap();
    editor::presentation(shell).unwrap().generation
}
fn raw(shell: &ShellState) -> Value {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone()
}
fn check(shell: &ShellState, stage: usize) {
    let s = state(shell).unwrap();
    oracle::exact(
        &snapshot(shell).unwrap(),
        stage,
        &s.document,
        &s.pages,
        &s.sync,
    )
    .unwrap();
}
fn form(shell: &ShellState, a: &EditAttempt) -> PreparedRequest {
    let session = WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_LIST_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    api::list_cell_update(&shell.core.client, &session, &a.edit)
}

#[test]
fn actual_link_worker_three_acks_normalized_name_display_remove_and_owned_reads() {
    let mut shell = setup();
    let pages = state(&shell).unwrap().pages.clone();
    assert_eq!(pages, oracle::pages(&pages[0]).unwrap());
    let mut generations = vec![];
    for index in 0..3 {
        let g = open(&mut shell);
        assert!(!generations.contains(&g));
        generations.push(g);
        let expected = if index == 0 {
            CellEditValue::Link(None)
        } else {
            desired(index - 1)
        };
        assert_eq!(*editor::presentation(&shell).unwrap().original, expected);
        if index == 0 {
            let before = snapshot(&shell).unwrap();
            assert_eq!(
                editor::begin_save(&mut shell, g, CellEditValue::Link(None))
                    .err()
                    .unwrap(),
                "This value cannot be saved. Check the value and choices."
            );
            assert_eq!(snapshot(&shell).unwrap(), before);
            assert_eq!(
                CellEditValue::Link(
                    super_platinum_core::slack_lists::LinkValue::from_input(
                        "example.com/new",
                        Some("R\u{e9}f\u{e9}rence \u{6771}\u{4eac}")
                    )
                    .unwrap()
                ),
                desired(0)
            );
        }
        check(&shell, index * 4);
        let a = editor::begin_save(&mut shell, g, desired(index)).unwrap();
        let req = form(&shell, &a);
        assert!(request(&mut shell, &a, &req).is_err());
        super::super::super::dispatch(&mut shell, a.clone());
        check(&shell, index * 4 + 1);
        assert!(request(&mut shell, &a, &req).is_err());
        assert!(editor::begin_save(&mut shell, g, desired(index)).is_err());
        super::super::super::complete_pending(&mut shell);
        check(&shell, index * 4 + 2);
        assert_eq!(raw(&shell), pages[index]); // positive ACK never installs canonical cells
        let ticket = begin_read(&mut shell).unwrap();
        let mut data = Ok(loaded(shell.slack_lists.fixture.as_ref().unwrap()));
        read_input(&mut shell, &ticket, &mut data).unwrap();
        assert!(ticket.current(&shell) && ticket.owns_read(&shell));
        assert!(!finish_read(&mut shell, &ticket, data.clone()));
        read_completed(&mut shell, &ticket).unwrap();
        check(&shell, index * 4 + 3);
        assert_eq!(raw(&shell), pages[index + 1]);
        assert!(editor::presentation(&shell).is_none());
        let before = snapshot(&shell).unwrap();
        let revision = shell.slack_lists.view.as_ref().unwrap().revision;
        assert!(!ticket.current(&shell) && !ticket.owns_read(&shell));
        assert_eq!(shell.slack_lists.view.as_ref().unwrap().read, 0);
        assert!(!finish_read(&mut shell, &ticket, data));
        assert!(read_completed(&mut shell, &ticket).is_err());
        assert!(completion(&mut shell, &a, false).is_err());
        assert_eq!(snapshot(&shell).unwrap(), before);
        assert_eq!(shell.slack_lists.view.as_ref().unwrap().revision, revision);
    }
    let g = open(&mut shell);
    let before = snapshot(&shell).unwrap();
    let s = state(&shell).unwrap();
    for variant in 0..5 {
        let mut bad = before.clone();
        match variant {
            0 => bad["requests"][0]["retrySafe"] = json!(true),
            1 => bad["requests"][1]["fields"][2][1] = json!("[]"),
            2 => bad["reads"][2]["readback"] = Value::Null,
            3 => bad["reads"][0]["population"]["response_metadata"]["next_cursor"] = json!("wrong"),
            _ => {
                bad["requests"][2]["attempt"]["serial"] =
                    bad["requests"][0]["attempt"]["serial"].clone()
            }
        }
        assert!(oracle::exact(&bad, 11, &s.document, &s.pages, &s.sync).is_err());
    }
    assert!(editor::begin_save(&mut shell, g, CellEditValue::Link(None)).is_err());
    assert!(editor::cancel_shell(&mut shell, g));
    assert_eq!(snapshot(&shell).unwrap(), before);
    let g = open(&mut shell);
    let a = editor::begin_save(&mut shell, g, desired(0)).unwrap();
    let data = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    assert!(preflight(&mut shell, &a, &data).is_err());
    let req = form(&shell, &a);
    assert!(request(&mut shell, &a, &req).is_err());
}
#[test]
fn absent_link_is_known_empty_different_read_retires_and_duplicate_cannot_revive() {
    let mut shell = setup();
    let g = open(&mut shell);
    let a = editor::begin_save(&mut shell, g, desired(0)).unwrap();
    super::super::super::dispatch(&mut shell, a.clone());
    super::super::super::complete_pending(&mut shell);
    let ticket = begin_read(&mut shell).unwrap();
    let mut empty = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    empty.records = RecordsPage::parse(state(&shell).unwrap().pages[0].clone()).unwrap();
    assert_eq!(
        a.edit.readback_value(&empty.document, &empty.records),
        Some(CellEditValue::Link(None))
    );
    // Pure reducer boundary, deliberately outside the matching native adapter sequence.
    assert!(!finish_read(&mut shell, &ticket, Ok(empty)));
    assert!(shell.slack_lists.operations.is_empty());
    let p = editor::presentation(&shell).unwrap();
    assert!(p.stale && p.disabled);
    assert_eq!(p.generation, g);
    assert!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .is_some_and(|s| s.contains("different"))
    );
    let revision = shell.slack_lists.view.as_ref().unwrap().revision;
    let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    assert!(!finish_read(&mut shell, &ticket, Ok(canonical.clone())));
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().revision, revision);
    let next = begin_read(&mut shell).unwrap();
    assert!(!finish_read(&mut shell, &next, Ok(canonical)));
    assert!(shell.slack_lists.operations.is_empty());
    assert!(editor::presentation(&shell).unwrap().stale);
    assert!(editor::begin_save(&mut shell, g, desired(0)).is_err());
}
#[test]
fn malformed_link_stays_unknown_and_owner_retirement_refuses_observer() {
    let mut shell = setup();
    let g = open(&mut shell);
    let a = editor::begin_save(&mut shell, g, desired(0)).unwrap();
    super::super::super::dispatch(&mut shell, a.clone());
    super::super::super::complete_pending(&mut shell);
    let ticket = begin_read(&mut shell).unwrap();
    let mut data = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    let mut malformed = state(&shell).unwrap().pages[1].clone();
    let row = malformed["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == ROW)
        .unwrap();
    row["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["key"] == KEY)
        .unwrap()["link"] = json!([{"originalUrl":42}]);
    data.records = RecordsPage::parse(malformed).unwrap();
    assert_eq!(a.edit.readback_value(&data.document, &data.records), None);
    assert!(!finish_read(&mut shell, &ticket, Ok(data.clone())));
    assert!(matches!(
        shell.slack_lists.operations["F_LIST_FIXTURE"].phase,
        Phase::Uncertain
    ));
    shell.core.account_epoch += 1;
    assert!(snapshot(&shell).is_err());
    assert!(preflight(&mut shell, &a, &data).is_err());
}
