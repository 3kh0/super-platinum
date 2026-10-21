use super::super::tests::{ID, raw, setup};
use super::*;

fn known_row() -> Value {
    let mut row = raw();
    row["files"] = json!([{"id":ID,"mode":"hosted","name":"Original canonical",
        "permalink":"https://fixture.slack.com/original","opaque":{"original":true}}]);
    row
}
fn response(name: &str) -> Value {
    json!({"ok":true,"file":{"id":ID,"mode":"hosted","name":name,
        "permalink":"https://fixture.slack.com/fresh","opaque":{"fresh":true}}})
}
fn admit(shell: &ShellState, model: &mut super::super::super::State) -> Attempt {
    let work = pending_known(shell, model);
    assert_eq!(work.len(), 1);
    enqueue(shell, model, work);
    model.file_acquisition.attempts[0].clone()
}
fn original(model: &super::super::super::State) -> Value {
    json!({"rows":model.rows.iter().map(|r|serde_json::to_value(&r.message).unwrap()).collect::<Vec<_>>(),
        "root":model.root,"serial":model.serial,"query":format!("{:?}",model.query),
        "pendingFile":model.pending_file,"mutationError":model.mutation_error})
}

#[test]
fn event_demand_deduplicates_and_unrelated_events_do_not_retry_a_failure() {
    let row = known_row();
    let mut duplicate = row.clone();
    duplicate["ts"] = json!("1719800002.000100");
    let (mut shell, mut model) = setup(&[row.clone(), duplicate]);
    assert!(pending_known(&shell, &model).is_empty());
    let old_link = file_source::token(&shell, &model, &model.rows[0].message).unwrap();
    let unchanged = original(&model);
    crate::file_events::invalidate(&mut shell, "T1", ID);
    assert!(!old_link.current(&shell, &model));
    let first = admit(&shell, &mut model);
    assert!(first.cold.known());
    assert!(pending_known(&shell, &model).is_empty());
    run::finish(
        &shell,
        &mut model,
        &first,
        Err(Error::Offline("closed fixture".into())),
    );
    let failed = snapshot(&shell, &model);
    assert_eq!(failed["rows"][0]["phase"], "unavailable");
    assert_eq!(failed["rows"][0]["retryable"], true);
    assert_eq!(
        file_source::snapshot(&shell, &model)["rows"][0]["source"],
        row["files"][0]
    );
    assert!(file_source::token(&shell, &model, &model.rows[0].message).is_none());
    crate::file_events::invalidate(&mut shell, "T1", "F87654321");
    assert!(
        pending_known(&shell, &model).is_empty(),
        "unrelated global revision is not a new demand"
    );
    assert_eq!(snapshot(&shell, &model), failed);
    assert_eq!(original(&model), unchanged);
    let retry = Retry {
        token: first.token.clone(),
        cold: first.cold.clone(),
    };
    assert!(requeue(&shell, &mut model, &retry));
    assert!(!requeue(&shell, &mut model, &retry));
    let next = model.file_acquisition.attempts[0].clone();
    run::finish(
        &shell,
        &mut model,
        &next,
        Ok(response("Renamed \u{4e2d}\u{1f98a}")),
    );
    assert_eq!(
        file_source::token(&shell, &model, &model.rows[0].message)
            .unwrap()
            .name(),
        "Renamed \u{4e2d}\u{1f98a}"
    );
    assert_eq!(original(&model), unchanged);
    assert!(!old_link.current(&shell, &model));
    assert!(pending_known(&shell, &model).is_empty());
}

#[test]
fn newer_event_replaces_only_original_attempt_and_refuses_late_publication() {
    let (mut shell, mut model) = setup(&[known_row()]);
    crate::file_events::invalidate(&mut shell, "T1", ID);
    let first = admit(&shell, &mut model);
    crate::file_events::invalidate(&mut shell, "T1", ID);
    let next = admit(&shell, &mut model);
    assert!(!slot_current(&shell, &model, &first));
    assert!(slot_current(&shell, &model, &next));
    assert!(next.cold.event_revision() > first.cold.event_revision());
    assert!(
        first
            .cold
            .publish(&shell, &mut model, &response("late")["file"])
            .is_err()
    );
    assert_eq!(model.file_acquisition.attempts.len(), 1);
    run::finish(&shell, &mut model, &next, Ok(response("New event")));
    assert_eq!(
        file_source::token(&shell, &model, &model.rows[0].message)
            .unwrap()
            .name(),
        "New event"
    );
}

#[test]
fn known_read_requires_original_owner_event_and_canonical_preimage() {
    for change in [
        "account", "socket", "visit", "serial", "query", "root", "deleted", "preimage",
    ] {
        let raw = known_row();
        let (mut shell, mut model) = setup(&[raw.clone()]);
        crate::file_events::invalidate(&mut shell, "T1", ID);
        let attempt = admit(&shell, &mut model);
        match change {
            "account" => shell.core.account_epoch += 1,
            "socket" => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            "visit" => shell.shortcut_generation += 1,
            // Actual comment transitions retire origins/attempts before they
            // assign the successor query; editing only these scalar fields
            // would leave an artificial still-admissible origin behind.
            "serial" => model.next_serial(),
            "query" => {
                model.next_serial();
                model.query = Some(Query::Stream(Filter::Recent));
            }
            "root" => model.root.as_mut().unwrap().text = Some("replacement".into()),
            "deleted" => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .deleted_files
                    .insert(ID.into());
            }
            "preimage" => {
                let ticket = Ticket {
                    owner: model.owner.clone().unwrap(),
                    serial: model.serial,
                    query: model.query.clone().unwrap(),
                    cursor: None,
                    direction: Direction::First,
                };
                let read = file_source::Read::capture(&shell, &ticket.owner).unwrap();
                file_source::publish(&shell, &mut model, &ticket, &read, &[raw]).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(!attempt.cold.demanded(&shell, &model), "{change}");
        assert!(
            attempt
                .cold
                .publish(&shell, &mut model, &response("late")["file"])
                .is_err(),
            "{change}"
        );
        assert!(pending_known(&shell, &model).is_empty(), "{change}");
    }
}

#[test]
fn malformed_known_response_retains_preimage_and_requires_explicit_retry() {
    for raw in [
        json!({"ok":true,"file":{"id":"F87654321"}}),
        json!({"ok":true,"file":null}),
        json!({"ok":true,"error":"access_denied","file":{"id":ID}}),
    ] {
        let row = known_row();
        let (mut shell, mut model) = setup(&[row.clone()]);
        crate::file_events::invalidate(&mut shell, "T1", ID);
        let attempt = admit(&shell, &mut model);
        run::finish(&shell, &mut model, &attempt, Ok(raw.clone()));
        assert_eq!(snapshot(&shell, &model)["rows"][0]["receipt"], raw);
        assert_eq!(snapshot(&shell, &model)["rows"][0]["phase"], "unavailable");
        assert_eq!(
            file_source::snapshot(&shell, &model)["rows"][0]["source"],
            row["files"][0]
        );
        assert!(pending_known(&shell, &model).is_empty());
    }
}

#[test]
fn known_endpoint_debt_blocks_retry_without_importing_a_different_owner() {
    let (mut shell, mut model) = setup(&[known_row()]);
    crate::file_events::invalidate(&mut shell, "T1", ID);
    let attempt = admit(&shell, &mut model);
    run::finish(
        &shell,
        &mut model,
        &attempt,
        Err(Error::RateLimited {
            retry_after_secs: Some(15),
        }),
    );
    assert!(cooling(&shell, &model));
    assert!(!requeue(
        &shell,
        &mut model,
        &Retry {
            token: attempt.token,
            cold: attempt.cold
        }
    ));
    assert!(pending_known(&shell, &model).is_empty());
}

#[test]
fn current_incomplete_rows_are_quiet_and_event_refresh_respects_original_visibility() {
    for file in [
        json!({"id":ID}),
        json!({"id":ID,"isUnknown":true}),
        json!({"id":ID,"name":"Only name"}),
        json!({"id":ID,"permalink":"https://fixture.slack.com/only"}),
    ] {
        let mut row = known_row();
        row["files"] = json!([file]);
        let (mut shell, model) = setup(&[row]);
        assert!(pending_known(&shell, &model).is_empty());
        crate::file_events::invalidate(&mut shell, "T1", ID);
        assert_eq!(pending_known(&shell, &model).len(), 1);
    }
    for file in [
        json!({"id":ID,"mode":"tombstone"}),
        json!({"id":ID,"isNonExistent":true}),
        json!({"id":ID,"is_salesforce_file":true}),
        json!({"id":ID,"is_tombstoned":true}),
    ] {
        let mut row = known_row();
        row["files"] = json!([file]);
        let (mut shell, model) = setup(&[row]);
        crate::file_events::invalidate(&mut shell, "T1", ID);
        assert!(pending_known(&shell, &model).is_empty());
    }
    let mut row = known_row();
    row["document_comment"]["is_visible"] = json!(false);
    let (mut shell, model) = setup(&[row]);
    crate::file_events::invalidate(&mut shell, "T1", ID);
    assert!(pending_known(&shell, &model).is_empty());
}

#[test]
fn successful_permalink_omission_retires_old_link_without_automatic_retry() {
    let (mut shell, mut model) = setup(&[known_row()]);
    crate::file_events::invalidate(&mut shell, "T1", ID);
    let attempt = admit(&shell, &mut model);
    run::finish(
        &shell,
        &mut model,
        &attempt,
        Ok(json!({"ok":true,"file":{"id":ID,"name":"No permalink"}})),
    );
    assert_eq!(snapshot(&shell, &model)["rows"][0]["phase"], "ready");
    assert!(file_source::token(&shell, &model, &model.rows[0].message).is_none());
    assert!(pending_known(&shell, &model).is_empty());
    assert_eq!(
        file_source::snapshot(&shell, &model)["rows"][0]["current"],
        true
    );
}

#[test]
fn known_request_uses_exact_source_reason_without_unknown_manager_skip_shares() {
    use super_platinum_core::{
        config::WorkspaceSession,
        slack::{SlackClient, client::RequestBody},
    };
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U0".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let req = known_request(&SlackClient::default(), &session, ID);
    assert_eq!(req.method, "POST");
    assert!(
        req.url
            .starts_with("https://fixture.slack.com/api/files.info")
    );
    let RequestBody::Form(fields) = req.body else {
        panic!("original form")
    };
    let map: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
    assert_eq!(
        map,
        std::collections::BTreeMap::from([
            ("token".into(), "synthetic".into()),
            ("file".into(), ID.into()),
            ("page".into(), "1".into()),
            ("count".into(), "500".into()),
            ("truncate".into(), "true".into()),
            (
                "_x_reason".into(),
                "files-store.ConditionalFetchManager.fetch".into()
            )
        ])
    );
}
