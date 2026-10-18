use super::*;
const DM: &str = "D00000001";
const GROUP: &str = "G00000001";
fn peer() -> Value {
    json!({"id":"U00000002","name":"alice","is_bot":false,"deleted":false,"future":{"keep":1}})
}
fn direct() -> Value {
    json!({"id":DM,"is_im":true,"user":"U00000002","is_open":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false})
}
fn group() -> Value {
    json!({"id":GROUP,"is_mpim":true,"is_private":true,"is_open":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false,"members":["U00000001","U00000002"]})
}
fn seed(raw: &Value) -> ShellState {
    let mut shell = sample();
    let w = shell.core.workspaces.get_mut("T00000001").unwrap();
    w.channels.insert(
        raw["id"].as_str().unwrap().into(),
        serde_json::from_value(raw.clone()).unwrap(),
    );
    w.users
        .insert("U00000002".into(), serde_json::from_value(peer()).unwrap());
    shell
}
fn finish(
    owner: &mut Owner,
    shell: &ShellState,
    raw: Value,
    peer: Option<Value>,
) -> Result<Snapshot, Refusal> {
    let read = owner.begin(shell)?;
    owner.finish_response(
        shell,
        read,
        Response {
            channel: json!({"ok":true,"channel":raw}),
            peer: peer.map(|p| json!({"ok":true,"results":[p],"failed_ids":[]})),
        },
    )
}
#[test]
fn existing_human_im_and_mpim_use_exact_ids_and_raw_permission_inputs() {
    for raw in [direct(), group()] {
        let shell = seed(&raw);
        let id = raw["id"].as_str().unwrap();
        let mut owner = Owner::capture(&shell, id).unwrap();
        let snapshot = finish(&mut owner, &shell, raw.clone(), (id == DM).then(peer)).unwrap();
        assert_eq!(snapshot.eligibility(), Eligibility::Allowed);
        assert_eq!(snapshot.raw(), &raw);
        assert!(owner.current(&shell, &snapshot));
        assert_eq!(snapshot.peer_raw, (id == DM).then(peer));
    }
}
#[test]
fn existing_dm_cache_is_never_raw_peer_permission_evidence() {
    for change in 0..7 {
        let raw = direct();
        let shell = seed(&raw);
        let mut owner = Owner::capture(&shell, DM).unwrap();
        let mut p = peer();
        match change {
            0 => {
                p.as_object_mut().unwrap().remove("is_bot");
            }
            1 => {
                p.as_object_mut().unwrap().remove("deleted");
            }
            2 => p["deleted"] = json!(true),
            3 => p["is_bot"] = json!(true),
            4 => p["id"] = json!("U00000003"),
            5 => p["isUnknown"] = json!(true),
            _ => p["deleted"] = json!("false"),
        }
        assert!(
            !finish(&mut owner, &shell, raw, Some(p))
                .is_ok_and(|s| s.eligibility() == Eligibility::Allowed)
        );
    }
}
#[test]
fn selected_recipient_replacement_close_and_account_switch_refuse() {
    for change in 0..5 {
        let raw = direct();
        let mut shell = seed(&raw);
        let mut owner = Owner::capture(&shell, DM).unwrap();
        let read = owner.begin(&shell).unwrap();
        let original = crate::file_share::recipients::capture(&shell, DM).unwrap();
        shell.file_share.selected_recipient = Some(("dialog".into(), DM.into(), original));
        assert!(crate::file_share::recipients::chosen(&shell, "dialog", DM));
        match change {
            0 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .channels
                    .get_mut(DM)
                    .unwrap()
                    .user = Some("U00000003".into())
            }
            1 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .channels
                    .get_mut(DM)
                    .unwrap()
                    .extra
                    .insert("is_open".into(), json!(false));
            }
            2 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .users
                    .get_mut("U00000002")
                    .unwrap()
                    .deleted = true
            }
            3 => shell.core.active_account = Some("other".into()),
            _ => {
                shell
                    .core
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .users
                    .get_mut("U00000002")
                    .unwrap()
                    .real_name = Some("Changed".into())
            }
        }
        assert!(
            owner
                .finish_response(
                    &shell,
                    read,
                    Response {
                        channel: json!({"ok":true,"channel":raw}),
                        peer: Some(json!({"ok":true,"results":[peer()]}))
                    }
                )
                .is_err()
        );
    }
}
#[test]
fn server_response_cannot_retarget_peer_group_members_or_family() {
    for (original, mut changed) in [
        (direct(), direct()),
        (group(), group()),
        (direct(), direct()),
    ] {
        let shell = seed(&original);
        let id = original["id"].as_str().unwrap();
        let mut owner = Owner::capture(&shell, id).unwrap();
        if id == GROUP {
            changed["members"] = json!(["U00000001", "U00000003"]);
        } else {
            changed["user"] = json!("U00000003");
        }
        assert!(finish(&mut owner, &shell, changed, (id == DM).then(peer)).is_err());
    }
    let raw = direct();
    let shell = seed(&raw);
    let mut owner = Owner::capture(&shell, DM).unwrap();
    let mut changed = raw;
    changed["is_im"] = json!(false);
    changed["is_channel"] = json!(true);
    assert!(finish(&mut owner, &shell, changed, Some(peer())).is_err());
}
#[test]
fn selection_requires_existing_open_human_conversation_and_saved_label_is_not_a_capture() {
    for (key, value) in [
        ("is_open", json!(false)),
        ("is_archived", json!(true)),
        ("is_ext_shared", json!(true)),
        ("is_pending_ext_shared", json!(true)),
        ("isUnknown", json!(true)),
    ] {
        let mut raw = direct();
        raw[key] = value;
        let shell = seed(&raw);
        assert!(crate::file_share::admission::selected(&shell, DM).is_none());
    }
    let shell = seed(&direct());
    assert!(crate::file_share::admission::selected(&shell, DM).is_some());
    assert!(!crate::file_share::recipients::chosen(
        &shell, "restored", DM
    ));
    assert!(crate::file_share::admission::selected(&shell, "U00000002").is_none());
    assert!(crate::file_share::admission::selected(&shell, "D00000099").is_none());
}
#[test]
fn malformed_failed_or_duplicate_edge_peer_response_refuses() {
    for response in [
        json!({"ok":false,"results":[peer()]}),
        json!({"ok":true,"results":[peer(),peer()]}),
        json!({"ok":true,"results":[peer()],"failed_ids":["U00000002"]}),
        json!({"ok":true,"results":[peer()],"error":"hidden"}),
    ] {
        assert!(crate::file_share::recipients::raw_peer(&response, "U00000002").is_none());
    }
}
