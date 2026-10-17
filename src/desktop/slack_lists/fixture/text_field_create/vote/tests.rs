use super::*;
use crate::slack_lists::{editor, layout, record_detail};
#[path = "../../../check/text_field_create/vote/oracle.rs"]
mod oracle;

// Private test-only arming starts after creation. Every subsequent worker
// observation is made by the same hooks used by the named native fixture.
fn setup(mode: create::FieldCreateMode) -> ShellState {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-vote-field-create",
    );
    assert!(rename::text_create::rating::close_vote_fixture(&shell));
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().insert(
        1, json!({"id":COLUMN,"key":KEY,"name":"Support Équipe 東京","type":"vote","options":mode.vote_options()}),
    );
    f.document["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .insert(1, json!({"id":COLUMN,"key":KEY,"visible":true}));
    let mut board = f.document["list_metadata"]["views"][0].clone();
    board["id"] = json!("V_VOTE_BOARD");
    board["name"] = json!("Vote board");
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
    let pages = pages(&f.records).unwrap();
    assert_eq!(pages, oracle::pages(&f.records).unwrap());
    f.uncertain = true;
    f.vote = Some(State {
        mode,
        different: None,
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
    )
    .unwrap();
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

fn start(shell: &mut ShellState, detail: bool) -> EditAttempt {
    use super_platinum_core::slack_lists::VoteIntent;
    if detail {
        let g = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
            .unwrap()
            .generation;
        record_detail::begin_vote_shell(shell, g, COLUMN, VoteIntent::RemoveMyVote).unwrap()
    } else {
        let v = shell.slack_lists.view.as_ref().unwrap();
        let visit = v.generation;
        let layout = v.layout.generation;
        let instance = v
            .rows
            .iter()
            .find(|r| r.id == ROW)
            .unwrap()
            .instance_id
            .clone();
        crate::slack_lists::vote::begin_table(
            shell,
            &instance,
            COLUMN,
            visit,
            layout,
            VoteIntent::AddMyVote,
        )
        .unwrap()
    }
}
fn read(shell: &mut ShellState) {
    let ticket = begin_read(shell).unwrap();
    let mut data = Ok(loaded(shell.slack_lists.fixture.as_ref().unwrap()));
    read_input(shell, &ticket, &mut data).unwrap();
    assert!(ticket.current(shell) && ticket.owns_read(shell));
    assert!(!finish_read(shell, &ticket, data.clone()));
    read_completed(shell, &ticket).unwrap();
    assert!(!ticket.current(shell) && !ticket.owns_read(shell));
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().read, 0);
    let consumed = snapshot(shell).unwrap();
    let rev = shell.slack_lists.view.as_ref().unwrap().revision;
    assert!(!finish_read(shell, &ticket, data));
    assert!(read_completed(shell, &ticket).is_err());
    assert_eq!(snapshot(shell).unwrap(), consumed);
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().revision, rev);
}
#[test]
fn both_vote_cases_use_actual_workers_known_different_retry_and_detail_remove() {
    for mode in [
        create::FieldCreateMode::VoteDefault,
        create::FieldCreateMode::VoteCustom,
    ] {
        let mut shell = setup(mode);
        let expected = state(&shell).unwrap().pages.clone();
        check(&shell, 0);
        // The actual shared seed allocates a synthetic account before opening the
        // visit. Stage zero isolates this contract from later attempt equality.
        let seed = snapshot(&shell).unwrap();
        let st = state(&shell).unwrap();
        for account in [
            Value::Null,
            json!(false),
            json!("OTHER_ACCOUNT"),
            json!("number-precision-fixture-0"),
            json!("number-precision-fixture-01"),
            json!("number-precision-fixture-18446744073709551616"),
            json!(format!("number-precision-fixture-{}", st.visit)),
        ] {
            let mut bad_sync = st.sync.clone();
            bad_sync["owner"][0] = account;
            let mut bad = seed.clone();
            bad["sync"] = bad_sync.clone();
            assert!(oracle::exact(&bad, 0, &st.document, &st.pages, &bad_sync).is_err());
        }
        let a = start(&mut shell, false);
        super::super::super::dispatch(&mut shell, a);
        check(&shell, 1);
        assert!(
            crate::slack_lists::vote::begin(
                &mut shell,
                ROW,
                COLUMN,
                EditAuthority::Layout,
                super_platinum_core::slack_lists::VoteIntent::AddMyVote
            )
            .is_err()
        );
        super::super::super::complete_pending(&mut shell);
        check(&shell, 2);
        assert_eq!(raw(&shell), expected[0]);
        different(&mut shell).unwrap();
        assert!(different(&mut shell).is_err());
        read(&mut shell);
        check(&shell, 3);
        assert_eq!(raw(&shell), expected[2]);
        let a = start(&mut shell, false);
        super::super::super::dispatch(&mut shell, a);
        check(&shell, 4);
        super::super::super::complete_pending(&mut shell);
        check(&shell, 5);
        assert_eq!(raw(&shell), expected[2]);
        read(&mut shell);
        check(&shell, 6);
        navigate(&mut shell, "V_VOTE_BOARD");
        check(&shell, 7);
        {
            let v = shell.slack_lists.view.as_ref().unwrap();
            let visit = v.generation;
            let layout = v.layout.generation;
            let instance = v
                .rows
                .iter()
                .find(|r| r.id == ROW)
                .unwrap()
                .instance_id
                .clone();
            assert!(
                crate::slack_lists::vote::begin_table(
                    &mut shell,
                    &instance,
                    COLUMN,
                    visit,
                    layout,
                    super_platinum_core::slack_lists::VoteIntent::RemoveMyVote
                )
                .is_err()
            );
        }
        detail(&mut shell);
        check(&shell, 8);
        let old = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
        let a = start(&mut shell, true);
        super::super::super::dispatch(&mut shell, a);
        check(&shell, 9);
        super::super::super::complete_pending(&mut shell);
        check(&shell, 10);
        assert_eq!(raw(&shell), expected[3]);
        read(&mut shell);
        check(&shell, 11);
        let new = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
        assert!(new.generation > old.generation);
        assert_eq!(new.open_generation, old.open_generation);
        assert_eq!(raw(&shell), expected[4]);
        close_detail(&mut shell);
        check(&shell, 12);
        navigate(&mut shell, "V_FILTER");
        check(&shell, 13);
        let final_state = snapshot(&shell).unwrap();
        assert_eq!(final_state["requests"].as_array().unwrap().len(), 3);
        assert_eq!(final_state["recordOpens"].as_array().unwrap().len(), 1);
        assert_eq!(
            final_state["preflights"][1]["attempt"]["cells"][0]["user"],
            json!(["U_OTHER_B", "U_OTHER_A", "U0"])
        );
        assert_eq!(
            final_state["preflights"][2]["attempt"]["cells"][0]["user"],
            json!(["U_OTHER_B", "U_OTHER_A"])
        );
        for key in ["requests", "recordOpens", "navigations"] {
            let mut bad = final_state.clone();
            bad[key].as_array_mut().unwrap().push(json!({}));
            let s = state(&shell).unwrap();
            assert!(oracle::exact(&bad, 13, &s.document, &s.pages, &s.sync).is_err());
        }
        let s = state(&shell).unwrap();
        for value in [json!(s.reads[0]["read"]), json!(null), json!(false)] {
            let mut bad = final_state.clone();
            bad["reads"][0]["after"]["read"] = value;
            assert!(oracle::exact(&bad, 13, &s.document, &s.pages, &s.sync).is_err());
        }
    }
}
#[test]
fn armed_vote_observer_refuses_changed_owner_and_retained_absence_is_exact() {
    let mut shell = setup(create::FieldCreateMode::VoteDefault);
    let mut bad = state(&shell).unwrap().pages[0].clone();
    bad["records"][3]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":KEY,"user":[]}));
    assert!(pages(&bad).is_err());
    assert!(oracle::pages(&bad).is_err());
    shell.core.account_epoch += 1;
    assert!(snapshot(&shell).is_err());
}
