use super::*;
use serde_json::json;

fn shell(alias: &str) -> ShellState {
    ShellState::fixture_variant(MediaRegistry::default(), alias)
}
fn loaded(s: &ShellState) -> Loaded {
    s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap()
}
fn gesture(s: &ShellState) -> (String, u64, u64) {
    let v = s.slack_lists.view.as_ref().unwrap();
    (
        v.rows[0].instance_id.clone(),
        v.generation,
        v.layout.generation,
    )
}
fn start(s: &mut ShellState, intent: VoteIntent) -> EditAttempt {
    let (i, v, l) = gesture(s);
    begin_table(s, &i, "C_VOTE", v, l, intent).unwrap()
}
fn voters(s: &ShellState, row: &str) -> Vec<String> {
    let l = loaded(s);
    l.document
        .column("C_VOTE")
        .unwrap()
        .vote_value(
            l.records.record(row).unwrap(),
            &l.document.id,
            l.personal_task_lists.as_ref(),
        )
        .unwrap()
}

#[test]
fn table_actual_worker_add_remove_preserves_others_and_waits_for_canonical_read() {
    for alias in ["slack-list-vote-cells", "slack-list-vote-cells-narrow"] {
        let mut s = shell(alias);
        assert!(s.core.session.is_none() && s.core.transport.is_none());
        let initial = loaded(&s);
        assert_eq!(initial.records.records.len(), 73);
        let row = s.slack_lists.view.as_ref().unwrap().rows[0].id.clone();
        let user = s.self_account.user_id.clone();
        assert!(voters(&s, &row).is_empty());
        for (n, intent) in [VoteIntent::AddMyVote, VoteIntent::RemoveMyVote]
            .into_iter()
            .enumerate()
        {
            let before = loaded(&s);
            let attempt = start(&mut s, intent);
            assert!(begin(&mut s, &row, "C_VOTE", EditAuthority::Layout, intent).is_err());
            fixture::dispatch(&mut s, attempt.clone());
            assert_eq!(fixture::attempt_count(&s), n + 1);
            assert!(fixture::prepared_valid(&s));
            fixture::complete_pending(&mut s);
            assert_eq!(loaded(&s).records, before.records);
            assert_eq!(s.slack_lists.operations.len(), 1);
            fixture::refresh(&mut s);
            assert!(s.slack_lists.operations.is_empty());
            assert_eq!(
                voters(&s, &row),
                if n == 0 { vec![user.clone()] } else { vec![] }
            );
            let after = loaded(&s);
            assert_eq!(after.document, initial.document);
            assert_eq!(after.records.next_cursor, initial.records.next_cursor);
            for old in &initial.records.records {
                if old.id != row {
                    assert_eq!(after.records.record(&old.id).unwrap(), old);
                }
            }
            fixture::complete_pending(&mut s);
            fixture::dispatch(&mut s, attempt);
            assert_eq!(fixture::attempt_count(&s), n + 1);
            // The refused duplicate preflight may set an error; it cannot issue a request.
            fixture::refresh(&mut s);
        }
    }
}

#[test]
fn dropped_reply_keeps_loaded_count_then_known_different_read_retires_without_replay() {
    let mut s = shell("slack-list-vote-cells");
    let before = loaded(&s);
    let attempt = start(&mut s, VoteIntent::AddMyVote);
    fixture::set_outcome_uncertain(&mut s);
    fixture::dispatch(&mut s, attempt.clone());
    fixture::complete_pending(&mut s);
    assert_eq!(loaded(&s).records, before.records);
    assert!(s.slack_lists.view.as_ref().unwrap().stale);
    // A later canonical third-party array is a known different result, never a retry.
    let mut fresh = before.clone();
    let mut raw = fresh.records.raw.clone();
    let row = raw["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == attempt.edit.record_id())
        .unwrap();
    row["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"support","user":["U_OTHER"]}));
    fresh.records = RecordsPage::parse(raw).unwrap();
    let read = begin_read(&mut s).unwrap();
    assert!(!finish_read(&mut s, &read, Ok(fresh)));
    assert!(s.slack_lists.operations.is_empty());
    assert_eq!(voters(&s, attempt.edit.record_id()), vec!["U_OTHER"]);
    assert_eq!(fixture::attempt_count(&s), 1);
    let next = start(&mut s, VoteIntent::AddMyVote);
    assert_eq!(
        next.edit.value(),
        CellEditValue::Votes(vec!["U_OTHER".into(), s.self_account.user_id.clone()])
    );
}

#[test]
fn exact_table_and_detail_authority_rejects_board_stale_owner_and_closed_detail() {
    let mut board = shell("slack-list-vote-board");
    let (i, v, l) = gesture(&board);
    assert!(begin_table(&mut board, &i, "C_VOTE", v, l, VoteIntent::AddMyVote).is_err());
    assert!(record_detail::open_shell(&mut board, &i, v, l));
    let generation = presentation(&board).unwrap().detail.unwrap().generation;
    let a =
        record_detail::begin_vote_shell(&mut board, generation, "C_VOTE", VoteIntent::AddMyVote)
            .unwrap();
    record_detail::dismiss_shell(&mut board);
    let fresh = loaded(&board);
    assert!(validate_edit(&mut board, &a, fresh).is_err());
    assert_eq!(fixture::attempt_count(&board), 0);
    for reason in 0..5 {
        let mut s = shell("slack-list-vote-cells");
        let (i, v, l) = gesture(&s);
        match reason {
            0 => s.core.account_epoch += 1,
            1 => s.slack_lists.view.as_mut().unwrap().generation = next_id(),
            2 => s.slack_lists.view.as_mut().unwrap().layout.generation = next_id(),
            3 => s.slack_lists.view.as_mut().unwrap().stale = true,
            _ => fixture::set_read_only(&mut s),
        }
        assert!(begin_table(&mut s, &i, "C_VOTE", v, l, VoteIntent::AddMyVote).is_err());
        assert_eq!(fixture::attempt_count(&s), 0);
    }
}

#[test]
fn detail_dispatch_uses_same_worker_and_no_generic_assignment_editor() {
    for alias in ["slack-list-vote-detail", "slack-list-vote-detail-narrow"] {
        let mut s = shell(alias);
        let p = presentation(&s).unwrap();
        let detail = p.detail.unwrap();
        let cell = detail
            .row
            .cells
            .iter()
            .find(|c| c.column_id == "C_VOTE")
            .unwrap();
        assert!(cell.editable && !cell.can_edit);
        assert!(
            editor::begin_with_authority(
                &mut s,
                &detail.record_id,
                "C_VOTE",
                p.generation,
                EditAuthority::RecordDetail(detail.generation)
            )
            .is_err()
        );
        let a = record_detail::begin_vote_shell(
            &mut s,
            detail.generation,
            "C_VOTE",
            VoteIntent::AddMyVote,
        )
        .unwrap();
        fixture::dispatch(&mut s, a);
        fixture::complete_pending(&mut s);
        fixture::refresh(&mut s);
        assert_eq!(fixture::attempt_count(&s), 1);
        assert!(s.slack_lists.operations.is_empty());
        let after = presentation(&s).unwrap().detail.unwrap();
        assert_ne!(after.generation, detail.generation);
        assert!(
            record_detail::begin_vote_shell(
                &mut s,
                detail.generation,
                "C_VOTE",
                VoteIntent::RemoveMyVote
            )
            .is_err()
        );
        assert_eq!(
            voters(&s, &after.record_id),
            vec![s.self_account.user_id.clone()]
        );
    }
}

#[test]
fn distinct_display_is_bounded_readonly_and_unknown_configuration_stays_inert() {
    let s = shell("slack-list-vote-cells");
    let l = loaded(&s);
    let c = l.document.column("C_VOTE").unwrap();
    let mut raw = l.records.records[0].raw.clone();
    raw["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"support","user":[s.self_account.user_id,"U_OTHER","U_OTHER"]}));
    let page = RecordsPage::parse(json!({"ok":true,"records":[raw]})).unwrap();
    let r = &page.records[0];
    let w = s
        .core
        .active_team
        .as_ref()
        .and_then(|id| s.core.workspaces.get(id));
    let (text, content) = project(w, &s.media, c, r).unwrap();
    assert_eq!(text, "2");
    assert!(
        matches!(content,CellContent::Vote{count:2,voted:true,ref tooltip,..}if tooltip.contains("You")&&tooltip.contains("U_OTHER"))
    );
    assert!(
        c.vote_value(r, &l.document.id, l.personal_task_lists.as_ref())
            .is_err()
    );
    assert!(!c.editable_value(r, &l.document.id, l.personal_task_lists.as_ref()));
    let mut doc = l.document.raw.clone();
    doc["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|x| x["id"] == "C_VOTE")
        .unwrap()["options"]["emoji_url"] = json!("https://fixture.invalid/e.png");
    let doc = Document::parse(doc).unwrap();
    assert_eq!(
        project(w, &s.media, doc.column("C_VOTE").unwrap(), r)
            .unwrap()
            .1,
        CellContent::Plain
    );
}
