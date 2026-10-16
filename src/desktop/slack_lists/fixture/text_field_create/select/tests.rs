use super::*;
use crate::slack_lists::editor;
#[path = "../../../check/text_field_create/select/oracle.rs"]
mod oracle;

// The production arm requires the native creation ledger. This test-only arm
// starts at that continuation boundary; it never fabricates worker observations.
fn setup() -> ShellState {
    setup_kind(false)
}
fn setup_kind(multi: bool) -> ShellState {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-select-field-create",
    );
    assert!(crate::slack_lists::rename::text_create::select::close_fixture(&shell));
    let mut opts = options();
    if multi {
        opts["format"] = json!("multi_select");
    }
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().insert(1,json!({"id":COLUMN,"key":KEY,"name":"Release phase","type":"select","options":opts,"opaque":{"fixture":"retained"}}));
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
    let values = if multi {
        vec![
            vec![SELECTED.into(), "OptAAAAAAA0".into()],
            vec![SELECTED.into()],
            vec![],
        ]
    } else {
        vec![vec![SELECTED.into()], vec![]]
    };
    let pages = pages_for(&f.records, &values).unwrap();
    f.select_create = Some(State {
        scope: scope.clone(),
        visit,
        selection,
        document: document.clone(),
        pages: pages.clone(),
        values,
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
        SELECTED,
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
fn actual_select_worker_noops_two_acks_and_owned_matching_reads() {
    let mut shell = setup();
    let pages = state(&shell).unwrap().pages.clone();
    let g = open(&mut shell);
    let before = snapshot(&shell).unwrap();
    assert_eq!(
        *editor::presentation(&shell).unwrap().original,
        CellEditValue::Select(vec![])
    );
    let initial = super::super::super::loaded(shell.slack_lists.fixture.as_ref().unwrap());
    let reason = CellEdit::capture(
        &initial.document,
        &initial.records,
        ROW,
        COLUMN,
        initial.allow_lists.as_deref(),
        initial.personal_task_lists.as_ref(),
        CellEditValue::Select(vec![]),
    )
    .err()
    .unwrap();
    assert_eq!(
        reason.to_string(),
        "This List field is already set to that value."
    );
    assert_eq!(
        editor::begin_save(&mut shell, g, CellEditValue::Select(vec![]))
            .err()
            .unwrap(),
        "This value cannot be saved. Check the value and choices."
    );
    assert_eq!(snapshot(&shell).unwrap(), before);
    check(&shell, 0);
    let first =
        editor::begin_save(&mut shell, g, CellEditValue::Select(vec![SELECTED.into()])).unwrap();
    let req = form(&shell, &first);
    assert!(request(&mut shell, &first, &req).is_err());
    super::super::super::dispatch(&mut shell, first.clone());
    check(&shell, 1);
    assert!(request(&mut shell, &first, &req).is_err());
    assert!(editor::begin_save(&mut shell, g, CellEditValue::Select(vec![])).is_err());
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
    let second = editor::begin_save(&mut shell, g2, CellEditValue::Select(vec![])).unwrap();
    super::super::super::dispatch(&mut shell, second.clone());
    check(&shell, 4);
    super::super::super::complete_pending(&mut shell);
    check(&shell, 5);
    assert_eq!(raw(&shell), pages[1]); // A positive ACK is not canonical application.
    super::super::super::refresh(&mut shell);
    check(&shell, 6);
    assert_eq!(raw(&shell), pages[2]);
    assert!(editor::presentation(&shell).is_none());
    let g3 = open(&mut shell);
    let before = snapshot(&shell).unwrap();
    assert!(editor::begin_save(&mut shell, g3, CellEditValue::Select(vec![])).is_err());
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
        assert!(oracle::exact(&bad, 6, &s.document, &s.pages, &s.sync, SELECTED).is_err());
    }
}
#[test]
fn missing_select_is_known_empty_and_owner_retirement_refuses() {
    let mut shell = setup();
    let g = open(&mut shell);
    let a =
        editor::begin_save(&mut shell, g, CellEditValue::Select(vec![SELECTED.into()])).unwrap();
    let data = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    assert_eq!(
        a.edit.readback_value(&data.document, &data.records),
        Some(CellEditValue::Select(vec![]))
    );
    shell.core.account_epoch += 1;
    assert!(snapshot(&shell).is_err());
    assert!(preflight(&mut shell, &a, &data).is_err());
}

const SELECTED: &str = "OptAAAAAAA1";
fn options() -> Value {
    json!({"format":"single_select","choices":create::select_create::LABELS.iter().enumerate().map(|(i,label)|json!({"label":label,"value":format!("OptAAAAAAA{i}"),"color":"blue"})).collect::<Vec<_>>()})
}

mod multi;
