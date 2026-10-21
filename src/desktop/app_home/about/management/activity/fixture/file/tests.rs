use super::*;
use super_platinum_core::{config::WorkspaceSession, slack::SlackClient};
#[test]
fn initial_file_setup_has_current_original_owner_and_no_http_capability() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    super::super::install_case(&mut shell, "file");
    assert_eq!(shell.core.active_account.as_deref(), Some(ACCOUNT));
    assert_eq!(shell.self_account.user_id, "UWORKFLOW");
    assert!(shell.core.transport.as_ref().unwrap().http().is_err());
    assert!(
        shell
            .app_home
            .about
            .surface
            .as_ref()
            .unwrap()
            .owner
            .current(&shell)
    );
    assert!(shell.slack_lists.fixture.is_none());
    assert!(custody(&shell)["documents"].as_array().unwrap().is_empty());
}
#[test]
fn exact_request_refuses_foreign_body_credential_duplicates_and_endpoint() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    super::super::install_case(&mut shell, "file");
    let session = &shell.core.session.as_ref().unwrap().workspaces["TWORKFLOW"];
    let good = api::file_info(&shell.core.client, session, LIST);
    assert_eq!(request_id(&shell, &good).unwrap(), LIST);
    for variant in 0..5 {
        let mut bad = good.clone();
        match variant {
            0 => bad.url = bad.url.replace("files.info", "files.delete"),
            1 => bad.method = "GET",
            _ => {
                let RequestBody::Form(fields) = &mut bad.body else {
                    panic!("form");
                };
                match variant {
                    2 => fields.push(("file".into(), LIST.into())),
                    3 => fields.push(("extra".into(), "true".into())),
                    _ => {
                        fields.iter_mut().find(|(k, _)| k == "token").unwrap().1 = "foreign".into()
                    }
                }
            }
        }
        assert!(request_id(&shell, &bad).is_err());
    }
}
#[test]
fn original_bytes_are_ordered_carriers_and_titles_remain_escaped_at_wire_boundary() {
    for id in [LIST, CANVAS] {
        let raw = raw(id).unwrap();
        let decoded = OrderedFileInfo::decode(raw.as_bytes()).unwrap();
        assert_eq!(decoded.file()["id"], id);
        assert!(decoded.file()["title"].as_str().unwrap().contains("&amp;"));
        assert_ne!(decoded.file()["title"], decoded.file()["name"]);
        assert_eq!(decoded.raw(), &serde_json::from_str::<Value>(raw).unwrap());
    }
}
#[test]
fn response_modes_are_original_wrong_id_type_and_status_not_synthetic_errors() {
    assert!(OrderedFileInfo::decode(response(LIST, Mode::Malformed).unwrap().as_bytes()).is_err());
    for (mode, field, value) in [
        (Mode::WrongId, "id", json!("FFOREIGN01")),
        (Mode::Unsupported, "mode", json!("hosted")),
        (Mode::Deleted, "is_deleted", json!(true)),
    ] {
        let decoded = OrderedFileInfo::decode(response(LIST, mode).unwrap().as_bytes()).unwrap();
        assert_eq!(decoded.file()[field], value);
        assert_eq!(decoded.file()["opaque"], file()["opaque"]);
    }
}
#[test]
fn request_factory_trace_normalization_rejects_unknown_duplicate_and_invalid_queries() {
    let c = SlackClient::default();
    let w = WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        user_id: "UWORKFLOW".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let a = api::file_info(&c, &w, LIST);
    let b = api::file_info(&c, &w, LIST);
    assert!(same_url(&a.url, &b.url));
    for suffix in [
        "&foreign=true",
        "&_x_id=invalid",
        "&_x_id=00000000-0000-0000-0000-000000000000",
        "&fp=invalid",
        "#fragment",
    ] {
        assert!(!same_url(&format!("{}{suffix}", a.url), &b.url));
    }
    assert!(!same_url(
        &a.url.replace("example.test", "foreign.test"),
        &b.url
    ));
}
#[test]
fn original_duplicate_occurrences_and_static_canvas_are_not_reordered() {
    let mut row = json!({"step_events":[{"outcome_message":null}]});
    messages(&mut row);
    assert_eq!(
        row["trigger_message"]["template"],
        "Open {list} then {list}"
    );
    assert_eq!(
        row["trigger_message"]["variables"]["list"],
        json!({"type":"LIST","value":LIST})
    );
    assert_eq!(
        row["step_events"][0]["outcome_message"]["variables"]["canvas"],
        json!({"type":"CANVAS","value":CANVAS})
    );
}
