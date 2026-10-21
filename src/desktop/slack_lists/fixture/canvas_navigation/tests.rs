use super::*;

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.active_account = Some(ACCOUNT.into());
    shell.core.active_team = Some("T1".into());
    shell.self_account.user_id = "U0".into();
    shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U0".into();
    shell
}

#[test]
fn closed_target_uses_actual_parsers_and_separate_readonly_document_and_row() {
    let raw = responses();
    let loaded = parse(&raw).unwrap();
    assert_eq!(loaded.document.id, TARGET);
    assert_eq!(loaded.document.raw, raw[0]["file"]);
    assert_eq!(loaded.records.records.len(), 1);
    assert_eq!(loaded.records.records[0].raw, raw[1]["records"][0]);
    let mut shell = shell();
    assert!(open_shell(
        &mut shell,
        serde_json::from_value(file()).unwrap()
    ));
    let read = begin_read(&mut shell).unwrap();
    assert!(!finish_read(&mut shell, &read, Ok(loaded)));
    let shown = presentation(&shell).unwrap();
    assert_eq!(shown.list_id, TARGET);
    assert_eq!(shown.rows.len(), 1);
    assert_eq!(shown.columns.len(), 1);
    assert!(shell.slack_lists.operations.is_empty());
    assert!(shell.slack_lists.fixture.is_none());
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
}

#[test]
fn foreign_document_or_record_cannot_be_adopted_as_target() {
    for case in 0..4 {
        let mut raw = responses();
        match case {
            0 => raw[0]["file"]["id"] = json!("FFOREIGN1"),
            1 => raw[1]["list_id"] = json!("FFOREIGN1"),
            2 => raw[1]["records"][0]["list_id"] = json!("FFOREIGN1"),
            _ => raw[0]["ok"] = json!(false),
        }
        assert!(parse(&raw).is_err());
    }
}

#[test]
fn original_read_cannot_publish_after_close_reopen_or_account_retirement() {
    for account_retired in [false, true] {
        let mut shell = shell();
        assert!(owner_matches(&shell));
        assert!(open_shell(
            &mut shell,
            serde_json::from_value(file()).unwrap()
        ));
        let original = begin_read(&mut shell).unwrap();
        if account_retired {
            shell.core.active_account = Some(uuid::Uuid::new_v4().to_string());
            assert!(!owner_matches(&shell));
        } else {
            close_shell(&mut shell);
            assert!(open_shell(
                &mut shell,
                serde_json::from_value(file()).unwrap()
            ));
        }
        assert!(!original.current(&shell));
        assert!(!finish_read(&mut shell, &original, parse(&responses())));
        assert!(shell.slack_lists.view.as_ref().unwrap().loaded.is_none());
    }
}

#[test]
fn original_factories_record_target_fields_without_credentials_or_live_context() {
    let shell = shell();
    let wire = requests(&shell).unwrap();
    assert_eq!(wire.len(), 5);
    for request in &wire {
        assert_eq!(request["method"], "POST");
        assert!(
            request["fields"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p[0] != "token")
        );
    }
    let fields = wire[1]["fields"].as_array().unwrap();
    for (name, value) in [
        ("list_id", TARGET),
        ("include_subtasks", "true"),
        ("archived", "false"),
        ("include_suggested", "false"),
        ("_x_reason", "get-list-records"),
    ] {
        assert!(fields.contains(&json!([name, value])));
    }
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
}
