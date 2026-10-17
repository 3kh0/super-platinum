use super::*;
use crate::slack_lists::{editor, layout, record_detail};
#[path = "../../../check/text_field_create/rating/oracle.rs"]
mod oracle;

// Private test-only arming starts after creation. Every subsequent worker
// observation is made by the same hooks used by the named native fixture.
fn setup(mode: create::FieldCreateMode) -> ShellState {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-rating-field-create",
    );
    assert!(rename::text_create::rating::close_fixture(&shell));
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().insert(
        1, json!({"id":COLUMN,"key":KEY,"name":"Quality","type":"rating","options":mode.rating_options()}),
    );
    f.document["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .insert(1, json!({"id":COLUMN,"key":KEY,"visible":true}));
    let mut board = f.document["list_metadata"]["views"][0].clone();
    board["id"] = json!("V_RATING_BOARD");
    board["name"] = json!("Rating board");
    board["type"] = json!("board");
    board["grouping"] = json!({"group_by":"status"});
    f.document["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .push(board);
    super::super::super::refresh(&mut shell);
    let scope = shell.slack_lists.scope.clone().unwrap();
    let visit = generation(&shell).unwrap();
    let selection = rename::selection(&shell).unwrap();
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    let document = f.document.clone();
    let pages = pages(&f.records, mode.rating_value()).unwrap();
    assert_eq!(
        pages,
        oracle::pages(&f.records, mode.rating_value()).unwrap()
    );
    f.rating = Some(State {
        mode,
        navigations: vec![],
        record_opens: vec![],
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
fn check(shell: &ShellState, index: usize) {
    let s = state(shell).unwrap();
    oracle::exact(
        &snapshot(shell).unwrap(),
        index,
        &s.document,
        &s.pages,
        &s.sync,
        s.mode.rating_value(),
    )
    .unwrap();
}
fn open(shell: &mut ShellState, detail: bool) -> u64 {
    let visit = generation(shell).unwrap();
    if detail {
        let d = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
        editor::begin_with_authority(
            shell,
            ROW,
            COLUMN,
            visit,
            record_detail::EditAuthority::RecordDetail(d.generation),
        )
        .unwrap();
    } else {
        editor::begin_shell(shell, ROW, COLUMN, visit).unwrap();
    }
    editor::presentation(shell).unwrap().generation
}
fn navigate(shell: &mut ShellState, id: &str) {
    let visit = generation(shell).unwrap();
    let layout = shell.slack_lists.view.as_ref().unwrap().layout.generation;
    assert!(layout::select_shell(shell, Some(id.into()), visit, layout));
    let f = &shell.slack_lists.view.as_ref().unwrap().layout.find;
    assert!(f.input.is_empty() && f.applied.is_empty() && f.revision == 0);
    // Find text is test setup; native proof drives the real debounced DOM input.
    let team = shell.slack_lists.scope.as_ref().unwrap().team.clone();
    let v = shell.slack_lists.view.as_mut().unwrap();
    v.layout.find.input = "Launch item 0".into();
    v.layout.find.applied = v.layout.find.input.clone();
    v.layout.find.revision = next_id();
    layout::rebuild(v, shell.core.workspaces.get(&team));
    adopt_view(shell, id).unwrap();
    assert!(adopt_view(shell, id).is_err());
}
fn detail(shell: &mut ShellState) {
    let v = shell.slack_lists.view.as_ref().unwrap();
    let instance = v
        .rows
        .iter()
        .find(|r| r.id == ROW)
        .unwrap()
        .instance_id
        .clone();
    let visit = v.generation;
    let layout = v.layout.generation;
    assert!(record_detail::open_shell(shell, &instance, visit, layout));
    let d = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    // onmounted invokes this exact production fixture function after claiming the
    // mount. This unit test calls the prepared-request hook once explicitly.
    super::super::super::record_open(shell, "F_LIST_FIXTURE", ROW, d.open_generation);
    assert!(super::super::super::record_open_valid(shell));
}
fn close_detail(shell: &mut ShellState) {
    let d = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    assert!(record_detail::close_shell(shell, d.generation));
}

#[test]
fn three_rating_cases_use_actual_four_writes_owned_reads_and_two_detail_forms() {
    for mode in [
        create::FieldCreateMode::RatingDefault,
        create::FieldCreateMode::RatingStandard,
        create::FieldCreateMode::RatingCustom,
    ] {
        let mut shell = setup(mode);
        let pages = state(&shell).unwrap().pages.clone();
        let first = open(&mut shell, false);
        let before = snapshot(&shell).unwrap();
        assert_eq!(
            *editor::presentation(&shell).unwrap().original,
            CellEditValue::Rating(None)
        );
        assert!(editor::begin_save(&mut shell, first, CellEditValue::Rating(None)).is_err());
        assert_eq!(snapshot(&shell).unwrap(), before);
        assert!(editor::cancel_shell(&mut shell, first));
        for i in 0..4 {
            if i == 1 {
                navigate(&mut shell, "V_RATING_BOARD");
                check(&shell, 4);
                detail(&mut shell);
                check(&shell, 5);
                close_detail(&mut shell);
                navigate(&mut shell, "V_FILTER");
            }
            if i == 2 {
                navigate(&mut shell, "V_RATING_BOARD");
                check(&shell, 10);
                detail(&mut shell);
                check(&shell, 11);
            }
            if i == 3 {
                close_detail(&mut shell);
                navigate(&mut shell, "V_FILTER");
            }
            let g = open(&mut shell, i == 2);
            assert_eq!(
                *editor::presentation(&shell).unwrap().original,
                CellEditValue::Rating(if i % 2 == 0 {
                    None
                } else {
                    Some(mode.rating_value())
                })
            );
            let base = [0, 6, 12, 16][i];
            check(&shell, base);
            let a = editor::begin_save(
                &mut shell,
                g,
                CellEditValue::Rating((i % 2 == 0).then_some(mode.rating_value())),
            )
            .unwrap();
            let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
            assert!(preflight(&mut shell, &a, &canonical).is_err()); // still Preparing
            super::super::super::dispatch(&mut shell, a.clone());
            check(&shell, base + 1);
            let before = snapshot(&shell).unwrap();
            assert!(editor::begin_save(&mut shell, g, CellEditValue::Rating(None)).is_err());
            assert_eq!(snapshot(&shell).unwrap(), before);
            super::super::super::complete_pending(&mut shell);
            check(&shell, base + 2);
            assert_eq!(raw(&shell), pages[i]); // Positive ACK is not canonical readback.
            let ticket = begin_read(&mut shell).unwrap();
            let mut data = Ok(loaded(shell.slack_lists.fixture.as_ref().unwrap()));
            read_input(&mut shell, &ticket, &mut data).unwrap();
            assert!(ticket.owns_read(&shell) && ticket.current(&shell));
            assert!(!finish_read(&mut shell, &ticket, data.clone()));
            read_completed(&mut shell, &ticket).unwrap();
            assert!(!ticket.owns_read(&shell) && !ticket.current(&shell));
            assert_eq!(raw(&shell), pages[i + 1]);
            check(&shell, base + 3);
            let consumed = snapshot(&shell).unwrap();
            let revision = shell.slack_lists.view.as_ref().unwrap().revision;
            assert!(!finish_read(&mut shell, &ticket, data));
            assert!(read_completed(&mut shell, &ticket).is_err());
            assert_eq!(snapshot(&shell).unwrap(), consumed);
            assert_eq!(shell.slack_lists.view.as_ref().unwrap().revision, revision);
            if i == 2 {
                assert_eq!(super::super::super::record_open_count(&shell), 2);
            }
        }
        let final_ledger = snapshot(&shell).unwrap();
        assert_eq!(final_ledger["requests"].as_array().unwrap().len(), 4);
        assert_eq!(final_ledger["recordOpens"].as_array().unwrap().len(), 2);
        let g = open(&mut shell, false);
        assert!(editor::begin_save(&mut shell, g, CellEditValue::Rating(None)).is_err());
        assert!(editor::cancel_shell(&mut shell, g));
        assert_eq!(snapshot(&shell).unwrap(), final_ledger);
        // Four closed requests; a fifth actual different value cannot acquire
        // diagnostic admission, and no additional prepared form is recorded.
        let g = open(&mut shell, false);
        let a = editor::begin_save(&mut shell, g, CellEditValue::Rating(Some(1))).unwrap();
        let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
        assert!(preflight(&mut shell, &a, &canonical).is_err());
        let s = state(&shell).unwrap();
        assert_eq!(s.requests.len(), 4);
    }
}

#[test]
fn oracle_refuses_missing_wrong_or_extra_actual_forms_and_false_readbacks() {
    // Pure malformed population checks do not claim worker execution.
    let shell = setup(create::FieldCreateMode::RatingStandard);
    let s = state(&shell).unwrap();
    let good = snapshot(&shell).unwrap();
    oracle::exact(&good, 0, &s.document, &s.pages, &s.sync, 4).unwrap();
    for key in ["requests", "recordOpens", "navigations"] {
        let mut bad = good.clone();
        bad[key].as_array_mut().unwrap().push(json!({}));
        assert!(oracle::exact(&bad, 0, &s.document, &s.pages, &s.sync, 4).is_err());
    }
    let mut bad = s.pages[0].clone();
    bad["records"][3]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":KEY,"rating":[]}));
    assert!(pages(&bad, 4).is_err());
    assert!(oracle::pages(&bad, 4).is_err());
}
