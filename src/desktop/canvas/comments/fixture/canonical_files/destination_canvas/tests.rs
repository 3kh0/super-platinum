use super::*;

fn opening() -> (ShellState, checklist::Read) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(ACCOUNT.into());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: CANVAS.into(),
        title: TITLE.into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    checklist::open_visit(&mut shell);
    let read = checklist::begin_read(&mut shell).unwrap();
    (shell, read)
}

#[test]
fn original_pair_decodes_complete_readonly_target_without_edit_authority() {
    let (init, bytes) = responses();
    let file: File = serde_json::from_value(file()).unwrap();
    let canonical = CanvasCollabSnapshot::from_responses(
        file.clone(),
        CanvasControllerInit::parse(init, &owner()).unwrap(),
        LoadDataResponse::decode(&bytes).unwrap(),
    )
    .unwrap();
    assert_eq!(canonical.file_id(), CANVAS);
    assert_eq!(canonical.thread_id(), THREAD);
    assert_eq!(canonical.document_id(), DOCUMENT);
    assert!(canonical.sections.is_empty());
    assert_eq!(file.extra["editable"], false);
    assert_eq!(super_platinum_core::state::file_title(&file), TITLE);
    assert!(super_platinum_core::canvas::content_url(EXPORT));
    assert!(CanvasDocument::parse(HTML).is_ok());
    assert!(HTML.contains(CONTENT));
    assert!(HTML.contains("&amp; \u{4e2d}\u{1f98a}"));
}

#[test]
fn paired_receipt_parsers_refuse_foreign_owner_thread_and_malformed_load() {
    let (init, bytes) = responses();
    let mut foreign = owner();
    foreign.user_id = "UOTHER".into();
    assert!(CanvasControllerInit::parse(init.clone(), &foreign).is_err());
    foreign = owner();
    foreign.team_id = "TOTHER".into();
    assert!(CanvasControllerInit::parse(init.clone(), &foreign).is_err());
    let mut wrong_file = file();
    wrong_file["quip_thread_id"] = json!("foreign-thread");
    assert!(
        CanvasCollabSnapshot::from_responses(
            serde_json::from_value(wrong_file).unwrap(),
            CanvasControllerInit::parse(init, &owner()).unwrap(),
            LoadDataResponse::decode(&bytes).unwrap(),
        )
        .is_err()
    );
    assert!(LoadDataResponse::decode(&[0xff]).is_err());
}

#[test]
fn actual_factories_record_only_original_destination_and_strip_credentials() {
    let client = super_platinum_core::slack::SlackClient::default();
    let owner = owner();
    let info = record(
        api::canvas_info(&client, &owner, CANVAS),
        json!({"ok":true,"file":file()}),
    )
    .unwrap();
    assert_eq!(info["path"], "/api/files.info");
    assert_eq!(info["args"], json!({"file":CANVAS}));
    assert_eq!(info["response"]["file"]["id"], CANVAS);
    assert!(info["args"].get("token").is_none());
    let load = record(
        api::canvas_collab_load_request(&client, &owner, THREAD).unwrap(),
        json!({"ok":true}),
    )
    .unwrap();
    assert_eq!(load["path"], "/canvas/-/load-data");
    assert!(!load.to_string().contains("fixture-only"));
}

#[test]
fn credential_free_original_read_is_retired_by_successor_visit_and_epoch() {
    let (mut shell, read) = opening();
    assert!(identity(&shell));
    assert!(read.current(&shell));
    assert!(read.request(&shell).is_none());
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    checklist::open_visit(&mut shell);
    let successor = checklist::begin_read(&mut shell).unwrap();
    assert!(successor.current(&shell));
    assert!(!read.owns_view(&shell));
    shell.core.account_epoch = shell.core.account_epoch.wrapping_add(1);
    assert!(!successor.current(&shell));
}

#[test]
fn carrier_identity_and_actual_read_refuse_foreign_account_and_deleted_target() {
    let (mut shell, read) = opening();
    shell.core.active_account = Some("foreign-account".into());
    assert!(!identity(&shell));
    assert!(!read.current(&shell));
    let (mut shell, read) = opening();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .deleted_files
        .insert(CANVAS.into());
    assert!(!read.current(&shell));
    let (mut shell, _) = opening();
    shell.self_account.user_id = "UOTHER".into();
    assert!(!identity(&shell));
}
