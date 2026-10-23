use super::*;
pub(super) const ID: &str = "F12345678";
const ROOT: &str = "1719800000.000100";
pub(super) fn raw() -> Value {
    json!({"type":"message","subtype":"document_comment_root","ts":ROOT,"user":"U1",
        "text":"Original fallback","document_comment":{"thread_id":"original-control"},
        "blocks":[{"type":"rich_text","block_id":"","elements":[]}],
        "files":[{"id":ID,"file_access":"check_file_info"}]})
}
pub(super) fn setup(raws: &[Value]) -> (ShellState, super::super::State) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    // The endpoint fence is intentionally process-wide; each test owns a
    // distinct account so parallel tests never share artificial server debt.
    shell.core.active_account = Some(uuid::Uuid::new_v4().to_string());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Fixture".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    let ticket = Ticket {
        owner: Owner::capture(&shell).unwrap(),
        serial: 1,
        query: Query::Replies(ROOT.into()),
        cursor: None,
        direction: Direction::First,
    };
    let read = file_source::Read::capture(&shell, &ticket.owner).unwrap();
    let mut model = super::super::State {
        owner: Some(ticket.owner.clone()),
        serial: 1,
        query: Some(ticket.query.clone()),
        ..Default::default()
    };
    for raw in raws {
        let message: Message = serde_json::from_value(raw.clone()).unwrap();
        let body = super::super::projection::metadata_vm(
            &shell.core.workspaces["T1"],
            &message,
            &shell.media,
        );
        if message.ts.as_deref() == Some(ROOT) {
            model.root = Some(message.clone());
        }
        model.rows.push(Row {
            message,
            body,
            preview: None,
            preview_body: None,
        });
    }
    file_source::publish(&shell, &mut model, &ticket, &read, raws).unwrap();
    (shell, model)
}
pub(super) fn attempt(shell: &ShellState, model: &mut super::super::State) -> Attempt {
    let attempt = Attempt {
        token: Arc::new(()),
        cold: pending(shell, model).remove(0),
        phase: Phase::Loading,
        notice: None,
        receipt: None,
    };
    model.file_acquisition.attempts.push(attempt.clone());
    attempt
}
#[test]
fn absent_attempt_snapshot_preserves_original_closed_row_schema() {
    let (shell, mut model) = setup(&[raw()]);
    attempt(&shell, &mut model);
    assert_eq!(
        snapshot(&shell, &model),
        json!({
            "running":false,"cooling":false,"rows":[{
                "id":ID,"rootTs":ROOT,"phase":"loading","current":true,
                "retryable":false,"notice":null,"receipt":null
            }]
        })
    );
}
#[test]
fn only_absent_original_first_file_demands_no_name_or_unknown_heuristic() {
    for file in [
        json!({"id":ID,"isUnknown":true}),
        json!({"id":ID}),
        json!({"id":ID,"name":"No permalink"}),
        json!({"id":ID,"permalink":"https://example.test/no-name"}),
        json!({"id":ID,"mode":"tombstone"}),
    ] {
        let mut row = raw();
        row["files"] = json!([file,{"id":"F87654321","file_access":"check_file_info"}]);
        let (shell, model) = setup(&[row]);
        assert!(pending(&shell, &model).is_empty());
    }
    let (shell, model) = setup(&[raw()]);
    assert_eq!(pending(&shell, &model).len(), 1);
}
#[test]
fn hidden_and_invalid_first_never_demand_second_and_duplicates_deduplicate() {
    let mut hidden = raw();
    hidden["document_comment"]["is_visible"] = json!(false);
    let (shell, model) = setup(&[hidden]);
    assert!(pending(&shell, &model).is_empty());
    let mut invalid = raw();
    invalid["files"] = json!([{"id":"Fbad","file_access":"check_file_info"},{"id":ID,"file_access":"check_file_info"}]);
    let (shell, model) = setup(&[invalid]);
    assert!(pending(&shell, &model).is_empty());
    let mut second = raw();
    second["ts"] = json!("1719800002.000100");
    let (shell, model) = setup(&[raw(), second]);
    assert_eq!(pending(&shell, &model).len(), 1);
}
#[test]
fn metadata_publication_keeps_original_row_root_serial_query_and_visit() {
    let (shell, mut model) = setup(&[raw()]);
    let cold = pending(&shell, &model).remove(0);
    let before = json!({"root":model.root,"row":model.rows[0].message,"serial":model.serial});
    let query = model.query.clone();
    let visit = shell.shortcut_generation;
    let owner = Arc::new(());
    model.file_acquisition.worker = Some(owner.clone());
    cold.publish(&shell,&mut model,&json!({"id":ID,"name":"Fetched exact","permalink":"https://example.test/exact","is_deleted":false})).unwrap();
    assert_eq!(
        before,
        json!({"root":model.root,"row":model.rows[0].message,"serial":model.serial})
    );
    assert_eq!(model.query, query);
    assert_eq!(shell.shortcut_generation, visit);
    assert!(Arc::ptr_eq(
        model.file_acquisition.worker.as_ref().unwrap(),
        &owner
    ));
    assert_eq!(
        file_source::token(&shell, &model, &model.rows[0].message)
            .unwrap()
            .name(),
        "Fetched exact"
    );
    assert!(!cold.absent(&shell, &model));
}
#[test]
fn original_origin_and_absent_event_fences_reject_late_adoption() {
    for change in [
        "serial",
        "root",
        "row",
        "socket",
        "epoch",
        "visit",
        "user",
        "query",
        "file-event",
        "deleted",
    ] {
        let (mut shell, mut model) = setup(&[raw()]);
        let cold = pending(&shell, &model).remove(0);
        match change {
            "serial" => model.serial += 1,
            "root" => model.root.as_mut().unwrap().text = Some("Replaced".into()),
            "row" => model.rows[0].message.text = Some("Replaced".into()),
            "socket" => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            "epoch" => shell.core.account_epoch += 1,
            "visit" => shell.shortcut_generation += 1,
            "user" => shell.self_account.user_id = "OTHER".into(),
            "query" => model.query = Some(Query::Stream(Filter::Recent)),
            "file-event" => crate::file_events::invalidate(&mut shell, "T1", ID),
            "deleted" => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .deleted_files
                    .insert(ID.into());
            }
            _ => unreachable!(),
        }
        assert!(!cold.current(&shell, &model), "{change}");
        assert!(
            cold.publish(
                &shell,
                &mut model,
                &json!({"id":ID,"name":"late","permalink":"https://example.test/late"})
            )
            .is_err(),
            "{change}"
        );
        assert!(
            file_source::snapshot(&shell, &model)["rows"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn explicit_retry_consumes_original_callback_and_never_rebinds_after_event() {
    let (mut shell, mut model) = setup(&[raw()]);
    let attempt = attempt(&shell, &mut model);
    model.file_acquisition.attempts[0].phase = Phase::Unavailable;
    let retry = Retry {
        token: attempt.token,
        cold: attempt.cold,
    };
    assert!(requeue(&shell, &mut model, &retry));
    assert!(!requeue(&shell, &mut model, &retry));
    model.file_acquisition.attempts[0].phase = Phase::Unavailable;
    let next = &model.file_acquisition.attempts[0];
    let next = Retry {
        token: next.token.clone(),
        cold: next.cold.clone(),
    };
    crate::file_events::invalidate(&mut shell, "T1", ID);
    assert!(!requeue(&shell, &mut model, &next));
}
