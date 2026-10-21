use super::super::{Direction, Row};
use super::*;

const ID: &str = "F12345678";
const ROOT: &str = "1719800000.000100";
fn raw() -> Value {
    json!({"type":"message","subtype":"document_comment_root","ts":ROOT,"user":"U1",
        "text":"Original fallback","document_comment":{"thread_id":"original-control"},
        "blocks":[{"type":"rich_text","block_id":"","elements":[]}],
        "files":[{"id":ID,"mode":"hosted","name":"Embedded name","title":"Not canonical",
            "permalink":"https://fixture.slack.com/files/old"},
            {"id":ID,"mode":"hosted","name":"Canonical & \u{4e2d}\u{1f98a}",
                "title":"Not display title","permalink":"https://fixture.slack.com/files/current"}]})
}
fn setup(raw: &Value) -> (ShellState, Comments, Ticket, Read) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
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
    let read = Read::capture(&shell, &ticket.owner).unwrap();
    let message: Message = serde_json::from_value(raw.clone()).unwrap();
    let body =
        super::super::projection::metadata_vm(&shell.core.workspaces["T1"], &message, &shell.media);
    let model = Comments {
        owner: Some(ticket.owner.clone()),
        serial: 1,
        query: Some(ticket.query.clone()),
        root: Some(message.clone()),
        rows: vec![Row {
            message,
            body,
            preview_body: None,
        }],
        ..Default::default()
    };
    (shell, model, ticket, read)
}
fn current(shell: &ShellState, model: &Comments) -> Option<Token> {
    token(shell, model, &model.rows[0].message)
}

#[test]
fn original_first_occurrence_uses_last_canonical_upsert_without_rewriting_message() {
    let original = raw();
    let (shell, mut model, ticket, read) = setup(&original);
    let before = serde_json::to_value(&model.rows[0].message).unwrap();
    publish(&shell, &mut model, &ticket, &read, &[original.clone()]).unwrap();
    let selected = current(&shell, &model).unwrap();
    assert_eq!(selected.id(), ID);
    assert_eq!(selected.name(), "Canonical & \u{4e2d}\u{1f98a}");
    assert_eq!(selected.url(), "https://fixture.slack.com/files/current");
    assert_eq!(selected.block_id(), "");
    assert_eq!(selected.observed.source(), &original["files"][1]);
    assert_eq!(
        serde_json::to_value(&model.rows[0].message).unwrap(),
        before
    );
}

#[test]
fn same_id_replacement_retires_token_but_unrelated_intake_does_not() {
    let original = raw();
    let (shell, mut model, ticket, read) = setup(&original);
    publish(&shell, &mut model, &ticket, &read, &[original]).unwrap();
    let selected = current(&shell, &model).unwrap();
    model.file_source.store.observe_batch(&[
        json!({"id":"F87654321","mode":"hosted","name":"Other","permalink":"https://fixture.slack.com/other"})
    ]).unwrap();
    assert!(selected.current(&shell, &model));
    model
        .file_source
        .store
        .observe_batch(&[selected.observed.source().clone()])
        .unwrap();
    assert!(
        !selected.current(&shell, &model),
        "equal-value replacement is a new receipt"
    );
}

#[test]
fn local_file_invalidation_without_worker_retires_only_its_id_and_fresh_read_repairs() {
    let original = raw();
    let (mut shell, mut model, ticket, read) = setup(&original);
    publish(&shell, &mut model, &ticket, &read, &[original.clone()]).unwrap();
    let selected = current(&shell, &model).unwrap();
    crate::file_events::invalidate(&mut shell, "T1", "F87654321");
    assert!(selected.current(&shell, &model));
    crate::file_events::invalidate(&mut shell, "T1", ID);
    assert!(!selected.current(&shell, &model));
    assert!(publish(&shell, &mut model, &ticket, &read, &[original.clone()]).is_err());
    let fresh = Read::capture(&shell, &ticket.owner).unwrap();
    publish(&shell, &mut model, &ticket, &fresh, &[original]).unwrap();
    assert!(current(&shell, &model).is_some());
    assert!(!selected.current(&shell, &model));
}

#[test]
fn failed_save_remount_retires_old_click_without_refreshing_original_source() {
    let original = raw();
    let (mut shell, mut model, ticket, read) = setup(&original);
    publish(&shell, &mut model, &ticket, &read, &[original]).unwrap();
    let selected = current(&shell, &model).unwrap();
    remount(&mut model);
    assert!(!selected.current(&shell, &model));
    assert!(current(&shell, &model).is_some());
    crate::file_events::invalidate(&mut shell, "T1", ID);
    remount(&mut model);
    assert!(
        current(&shell, &model).is_none(),
        "remount never refreshes stamps"
    );
}

#[test]
fn original_read_root_socket_account_and_deleted_file_fences_remain_exact() {
    for change in [
        "serial", "root", "row", "socket", "account", "deleted", "loading",
    ] {
        let original = raw();
        let (mut shell, mut model, ticket, read) = setup(&original);
        publish(&shell, &mut model, &ticket, &read, &[original]).unwrap();
        let selected = current(&shell, &model).unwrap();
        match change {
            "serial" => model.serial += 1,
            "root" => model.root.as_mut().unwrap().text = Some("Replaced".into()),
            "row" => model.rows[0].message.text = Some("Replaced".into()),
            "socket" => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            "account" => shell.core.account_epoch += 1,
            "deleted" => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .deleted_files
                    .insert(ID.into());
            }
            "loading" => model.loading = true,
            _ => unreachable!(),
        }
        assert!(!selected.current(&shell, &model), "{change}");
    }
}

#[test]
fn stale_ticket_cannot_clear_successor_but_current_malformed_sidecar_removes_override() {
    let original = raw();
    let (shell, mut model, mut ticket, read) = setup(&original);
    publish(&shell, &mut model, &ticket, &read, &[original.clone()]).unwrap();
    model.serial += 1;
    ticket.serial = model.serial;
    publish(&shell, &mut model, &ticket, &read, &[original.clone()]).unwrap();
    let selected = current(&shell, &model).unwrap();
    ticket.serial -= 1;
    assert!(publish(&shell, &mut model, &ticket, &read, &[]).is_err());
    assert!(selected.current(&shell, &model));
    ticket.serial += 1;
    let mut malformed = original;
    malformed["files"] = json!([false]);
    assert!(publish(&shell, &mut model, &ticket, &read, &[malformed]).is_err());
    assert!(current(&shell, &model).is_none());
}

#[test]
fn blocked_first_never_uses_eligible_second_and_hidden_has_no_link() {
    for hidden in [false, true] {
        let mut original = raw();
        original["files"] = json!([
            {"id":ID,"mode":"access_denied","name":"Blocked","permalink":"https://fixture.slack.com/no"},
            {"id":"F87654321","mode":"hosted","name":"Second","permalink":"https://fixture.slack.com/yes"}]);
        if hidden {
            original["document_comment"]["is_visible"] = json!(false);
        }
        let (shell, mut model, ticket, read) = setup(&original);
        publish(&shell, &mut model, &ticket, &read, &[original]).unwrap();
        assert!(current(&shell, &model).is_none());
    }
}

#[test]
fn raw_omitted_permalink_removes_override_and_exact_empty_name_is_retained() {
    let mut original = raw();
    original["files"][1]["name"] = json!("");
    let (shell, mut model, ticket, read) = setup(&original);
    publish(&shell, &mut model, &ticket, &read, &[original.clone()]).unwrap();
    assert_eq!(current(&shell, &model).unwrap().name(), "");
    original["files"][1]
        .as_object_mut()
        .unwrap()
        .remove("permalink");
    let message: Message = serde_json::from_value(original.clone()).unwrap();
    model.rows[0].message = message.clone();
    model.root = Some(message);
    publish(&shell, &mut model, &ticket, &read, &[original]).unwrap();
    assert!(current(&shell, &model).is_none());
    assert!(
        model
            .file_source
            .store
            .get(ID)
            .unwrap()
            .effective()
            .get("permalink")
            .is_none()
    );
}
