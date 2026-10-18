use super::*;
use crate::slack_lists::editor;
#[path = "../../../check/text_field_create/date/oracle.rs"]
mod oracle;

// The production arm requires the native creation ledger. This test-only arm
// starts at that continuation boundary; it never fabricates worker observations.
fn setup() -> ShellState {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-date-field-create",
    );
    assert!(create::close_date_fixture(&shell));
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().insert(1,json!({"id":COLUMN,"key":KEY,"name":"Target date","type":"date","options":{"format":"single","date_format":"default"},"opaque":{"fixture":"retained"}}));
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
    f.date = Some(State {
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
fn actual_calendar_worker_noops_lost_response_clear_ack_and_owned_reads() {
    let mut shell = setup();
    let pages = state(&shell).unwrap().pages.clone();
    let g = open(&mut shell);
    let before = snapshot(&shell).unwrap();
    assert_eq!(
        *editor::presentation(&shell).unwrap().original,
        CellEditValue::Date(None)
    );
    let initial = super::super::super::loaded(shell.slack_lists.fixture.as_ref().unwrap());
    let reason = CellEdit::capture(
        &initial.document,
        &initial.records,
        ROW,
        COLUMN,
        initial.allow_lists.as_deref(),
        initial.personal_task_lists.as_ref(),
        CellEditValue::Date(None),
    )
    .err()
    .unwrap();
    assert_eq!(
        reason.to_string(),
        "This List field is already set to that value."
    );
    assert_eq!(
        editor::begin_save(&mut shell, g, CellEditValue::Date(None))
            .err()
            .unwrap(),
        "This date cannot be saved. Check the date and your current access."
    );
    assert_eq!(snapshot(&shell).unwrap(), before);
    check(&shell, 0);
    let first = editor::begin_save(
        &mut shell,
        g,
        CellEditValue::Date(Some("2028-02-29".into())),
    )
    .unwrap();
    let req = form(&shell, &first);
    assert!(request(&mut shell, &first, &req).is_err());
    super::super::super::dispatch(&mut shell, first.clone());
    check(&shell, 1);
    assert!(request(&mut shell, &first, &req).is_err());
    assert!(editor::begin_save(&mut shell, g, CellEditValue::Date(None)).is_err());
    super::super::super::set_outcome_uncertain(&mut shell);
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
    let second = editor::begin_save(&mut shell, g2, CellEditValue::Date(None)).unwrap();
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
    assert!(editor::begin_save(&mut shell, g3, CellEditValue::Date(None)).is_err());
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
        assert!(oracle::exact(&bad, 6, &s.document, &s.pages, &s.sync).is_err());
    }
}
#[test]
fn missing_calendar_is_known_empty_not_checkbox_unknown_and_owner_retirement_refuses() {
    let mut shell = setup();
    let g = open(&mut shell);
    let a = editor::begin_save(
        &mut shell,
        g,
        CellEditValue::Date(Some("2028-02-29".into())),
    )
    .unwrap();
    let data = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    assert_eq!(
        a.edit.readback_value(&data.document, &data.records),
        Some(CellEditValue::Date(None))
    );
    shell.core.account_epoch += 1;
    assert!(snapshot(&shell).is_err());
    assert!(preflight(&mut shell, &a, &data).is_err());
}

#[test]
fn date_observer_requires_task_custody_and_admitted_saving_before_recording_preflight() {
    let mut shell = setup();
    let generation = open(&mut shell);
    let attempt = editor::begin_save(
        &mut shell,
        generation,
        CellEditValue::Date(Some("2028-02-29".into())),
    )
    .unwrap();
    let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    let generic = CellEdit::capture(
        &canonical.document,
        &canonical.records,
        ROW,
        COLUMN,
        canonical.allow_lists.as_deref(),
        canonical.personal_task_lists.as_ref(),
        CellEditValue::Date(Some("2028-02-29".into())),
    )
    .unwrap();
    // Identical request cells do not establish the typed preference custody.
    assert_eq!(attempt.edit.cells(), generic.cells());
    assert_ne!(attempt.edit, generic);
    let preparing = snapshot(&shell).unwrap();
    assert_eq!(preparing["operation"]["phase"], "Preparing");
    assert_eq!(
        preflight(&mut shell, &attempt, &canonical).unwrap_err(),
        "Actual typed Date custody or admitted phase differs"
    );
    assert_eq!(snapshot(&shell).unwrap(), preparing);
    validate_edit(&mut shell, &attempt, canonical.clone()).unwrap();
    let saving = snapshot(&shell).unwrap();
    assert_eq!(saving["operation"]["phase"], "Saving");
    let mut missing_policy = canonical.clone();
    missing_policy.personal_task_lists = None;
    assert_eq!(
        preflight(&mut shell, &attempt, &missing_policy).unwrap_err(),
        "Date preflight lacks observed task preferences"
    );
    assert_eq!(snapshot(&shell).unwrap(), saving);
    let mut without_custody = attempt.clone();
    without_custody.edit = generic;
    assert_eq!(
        preflight(&mut shell, &without_custody, &canonical).unwrap_err(),
        "Actual typed Date custody or admitted phase differs"
    );
    assert_eq!(snapshot(&shell).unwrap(), saving);
    preflight(&mut shell, &attempt, &canonical).unwrap();
    let admitted = snapshot(&shell).unwrap();
    assert_eq!(admitted["preflights"].as_array().unwrap().len(), 1);
    assert_eq!(admitted["preflights"][0]["phase"], "Saving");
    assert_eq!(admitted["preflights"][0]["attempt"], attempt_json(&attempt));
    assert_eq!(admitted["requests"], json!([]));
    assert_eq!(admitted["operation"], saving["operation"]);
}
