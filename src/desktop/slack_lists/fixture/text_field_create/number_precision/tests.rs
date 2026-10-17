use super::*;
use crate::slack_lists::editor;
#[path = "../../../check/text_field_create/number_precision/oracle.rs"]
mod oracle;

// The production arm requires the native creation ledger. This test-only arm
// starts at that continuation boundary; it never fabricates worker observations.
fn setup() -> ShellState {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-number-precision-field-create",
    );
    assert!(rename::text_create::number::close_fixture(&shell));
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().insert(1,json!({"id":COLUMN,"key":KEY,"name":"Release count","type":"number","options":{"precision":3},"opaque":{"fixture":"retained"}}));
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
    f.number_precision = Some(State {
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
fn actual_number_worker_noops_positive_acks_clear_and_owned_reads() {
    let mut shell = setup();
    let pages = state(&shell).unwrap().pages.clone();
    let g = open(&mut shell);
    let before = snapshot(&shell).unwrap();
    assert_eq!(
        *editor::presentation(&shell).unwrap().original,
        CellEditValue::Number(None)
    );
    let initial = super::super::super::loaded(shell.slack_lists.fixture.as_ref().unwrap());
    let reason = CellEdit::capture(
        &initial.document,
        &initial.records,
        ROW,
        COLUMN,
        initial.allow_lists.as_deref(),
        initial.personal_task_lists.as_ref(),
        CellEditValue::Number(None),
    )
    .err()
    .unwrap();
    assert_eq!(
        reason.to_string(),
        "This List field is already set to that value."
    );
    assert_eq!(
        editor::begin_save(&mut shell, g, CellEditValue::Number(None))
            .err()
            .unwrap(),
        "This value cannot be saved. Check the value and choices."
    );
    assert_eq!(snapshot(&shell).unwrap(), before);
    check(&shell, 0);
    let first = editor::begin_save(
        &mut shell,
        g,
        CellEditValue::Number(Some("-12.3456".parse().unwrap())),
    )
    .unwrap();
    let req = form(&shell, &first);
    assert!(request(&mut shell, &first, &req).is_err());
    super::super::super::dispatch(&mut shell, first.clone());
    check(&shell, 1);
    assert!(request(&mut shell, &first, &req).is_err());
    assert!(editor::begin_save(&mut shell, g, CellEditValue::Number(None)).is_err());
    super::super::super::complete_pending(&mut shell);
    check(&shell, 2);
    assert_eq!(raw(&shell), pages[0]);
    let ticket = begin_read(&mut shell).unwrap();
    let mut data = Ok(loaded(shell.slack_lists.fixture.as_ref().unwrap()));
    read_input(&mut shell, &ticket, &mut data).unwrap();
    assert!(ticket.owns_read(&shell) && ticket.current(&shell));
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert!(view.loading);
    assert_eq!(view.read, ticket.read);
    assert_eq!(view.revision, ticket.revision);
    // True requests a replacement after invalidation; it is not an applied flag.
    assert!(!finish_read(&mut shell, &ticket, data.clone()));
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert!(!view.loading);
    assert_eq!(view.read, 0);
    assert!(view.revision > ticket.revision);
    assert!(!ticket.owns_read(&shell) && !ticket.current(&shell));
    assert_eq!(
        view.loaded.as_ref().unwrap().document.raw,
        state(&shell).unwrap().document
    );
    assert_eq!(raw(&shell), pages[1]);
    assert!(shell.slack_lists.operations.is_empty());
    assert!(editor::presentation(&shell).is_none());
    read_completed(&mut shell, &ticket).unwrap();
    check(&shell, 3);
    let before = snapshot(&shell).unwrap();
    let canonical = raw(&shell);
    let consumed_revision = shell.slack_lists.view.as_ref().unwrap().revision;
    assert!(!finish_read(&mut shell, &ticket, data));
    assert!(read_completed(&mut shell, &ticket).is_err());
    assert_eq!(snapshot(&shell).unwrap(), before);
    assert_eq!(raw(&shell), canonical);
    assert_eq!(
        shell.slack_lists.view.as_ref().unwrap().revision,
        consumed_revision
    );
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().read, 0);
    assert!(editor::presentation(&shell).is_none());
    let g2 = open(&mut shell);
    assert_ne!(g, g2);
    assert_eq!(
        *editor::presentation(&shell).unwrap().original,
        CellEditValue::Number(Some("-12.3456".parse().unwrap()))
    );
    check(&shell, 4);
    check(&shell, 5);
    let second = editor::begin_save(&mut shell, g2, CellEditValue::Number(None)).unwrap();
    super::super::super::dispatch(&mut shell, second.clone());
    check(&shell, 6);
    super::super::super::complete_pending(&mut shell);
    check(&shell, 7);
    assert_eq!(raw(&shell), pages[1]); // A positive ACK is not canonical application.
    super::super::super::refresh(&mut shell);
    check(&shell, 8);
    assert_eq!(raw(&shell), pages[2]);
    assert!(editor::presentation(&shell).is_none());
    let g3 = open(&mut shell);
    let before = snapshot(&shell).unwrap();
    assert!(editor::begin_save(&mut shell, g3, CellEditValue::Number(None)).is_err());
    assert!(editor::cancel_shell(&mut shell, g3));
    assert_eq!(snapshot(&shell).unwrap(), before);
    let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    let req = form(&shell, &second);
    assert!(preflight(&mut shell, &second, &canonical).is_err());
    assert!(request(&mut shell, &second, &req).is_err());
    assert!(completion(&mut shell, &second, false).is_err());
    assert_eq!(snapshot(&shell).unwrap(), before);
    // The literal oracle rejects wrong retry policy, extra forms and false unknown readback.
    let s = state(&shell).unwrap();
    for mutation in 0..7 {
        let mut bad = before.clone();
        match mutation {
            0 => bad["requests"][0]["retrySafe"] = json!(true),
            1 => {
                let extra = bad["requests"][0].clone();
                bad["requests"].as_array_mut().unwrap().push(extra);
            }
            2 => bad["reads"][1]["readback"] = Value::Null,
            3 => bad["requests"][0]["attempt"]["column"] = json!("wrong"),
            4 => {
                bad["preflights"][0]["population"]["response_metadata"]["next_cursor"] =
                    json!("wrong")
            }
            5 => {
                bad["reads"].as_array_mut().unwrap().pop();
            }
            _ => bad["server"] = s.pages[1].clone(),
        }
        assert!(oracle::exact(&bad, 8, &s.document, &s.pages, &s.sync).is_err());
    }
}
#[test]
fn missing_number_is_known_empty_not_checkbox_unknown_and_owner_retirement_refuses() {
    let mut shell = setup();
    let g = open(&mut shell);
    let a = editor::begin_save(
        &mut shell,
        g,
        CellEditValue::Number(Some("-12.3456".parse().unwrap())),
    )
    .unwrap();
    let data = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    assert_eq!(
        a.edit.readback_value(&data.document, &data.records),
        Some(CellEditValue::Number(None))
    );
    shell.core.account_epoch += 1;
    assert!(snapshot(&shell).is_err());
    assert!(preflight(&mut shell, &a, &data).is_err());
}

#[test]
fn known_empty_after_nonempty_ack_retires_receipt_without_inventing_match() {
    let mut shell = setup();
    let g = open(&mut shell);
    let attempt = editor::begin_save(
        &mut shell,
        g,
        CellEditValue::Number(Some("-12.3456".parse().unwrap())),
    )
    .unwrap();
    super::super::super::dispatch(&mut shell, attempt.clone());
    super::super::super::complete_pending(&mut shell);
    let ticket = begin_read(&mut shell).unwrap();
    let mut data = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    data.records = super_platinum_core::slack_lists::RecordsPage::parse(
        state(&shell).unwrap().pages[0].clone(),
    )
    .unwrap();
    assert_eq!(
        attempt.edit.readback_value(&data.document, &data.records),
        Some(CellEditValue::Number(None))
    );
    assert!(!attempt.edit.readback_matches(&data.document, &data.records));
    assert!(!finish_read(&mut shell, &ticket, Ok(data)));
    assert!(shell.slack_lists.operations.is_empty());
    let retained = editor::presentation(&shell).unwrap();
    assert_eq!(retained.generation, g);
    assert_eq!(retained.row_id, ROW);
    assert_eq!(retained.column_id, COLUMN);
    assert_eq!(*retained.original, CellEditValue::Number(None));
    assert!(retained.stale && retained.disabled && !retained.saving);
    assert_eq!(
        retained.error.as_deref(),
        Some("Slack currently shows a different value. Review it before changing it again.")
    );
    assert_eq!(raw(&shell), state(&shell).unwrap().pages[0]);
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().read, 0);
    assert!(editor::cancel_shell(&mut shell, g));
    assert!(editor::presentation(&shell).is_none());
}
