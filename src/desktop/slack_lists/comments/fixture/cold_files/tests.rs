use super::*;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: completion::SELF.into(),
        url: "https://list-navigation-fixture.slack.com".into(),
        ..super::super::session()
    }
}
#[test]
fn singleton_exact_factory_rejects_generic_info_duplicates_and_foreign_target() {
    let client = SlackClient::default();
    let session = owner();
    for id in [CANVAS, TARGET_LIST] {
        let request = expected(&client, &session, id);
        let (accepted, args) = validate(&client, &session, &request).unwrap();
        assert_eq!(accepted, id);
        assert_eq!(
            args,
            json!({"file":id,"page":"1","count":"500","truncate":"true"})
        );
        for mutation in 0..6 {
            let mut bad = request.clone();
            let RequestBody::Form(body) = &mut bad.body else {
                panic!("form")
            };
            match mutation {
                0 => body.push(("file".into(), id.into())),
                1 => body.push(("include_shares".into(), "true".into())),
                2 => body.iter_mut().find(|(k, _)| k == "file").unwrap().1 = "FFOREIGN01".into(),
                3 => body.iter_mut().find(|(k, _)| k == "count").unwrap().1 = "100".into(),
                4 => body.iter_mut().find(|(k, _)| k == "truncate").unwrap().1 = "false".into(),
                _ => bad.url = "https://foreign.invalid/api/files.info".into(),
            }
            assert!(
                validate(&client, &session, &bad).is_err(),
                "mutation {mutation}"
            );
        }
        assert!(
            validate(
                &client,
                &session,
                &super_platinum_core::slack::api::file_info(&client, &session, id)
            )
            .is_err()
        );
    }
}
#[test]
fn literal_singleton_envelopes_retain_received_fields_and_project_real_candidates() {
    use super_platinum_core::slack::api::custom::texty::file_title_source::Store;
    let mut store = Store::new();
    for id in [CANVAS, TARGET_LIST] {
        let text = original_text(id).unwrap();
        let ordered = OrderedFileInfo::decode(text.as_bytes()).unwrap();
        let raw = ordered.raw().clone();
        assert_eq!(raw["file"]["id"], id);
        assert_eq!(raw["file"]["fixture_opaque"]["preserve"], id);
        store.ingest_info("T1", &ordered).unwrap();
        assert_eq!(store.row(id).unwrap().receipt(), &raw);
        assert_eq!(store.candidate(id).unwrap().source(), &raw["file"]);
    }
    assert_eq!(
        store.candidate(CANVAS).unwrap().channel_map_keys().unwrap(),
        ["z-last", "a-first", "m-foreign"]
    );
    assert_eq!(
        store
            .candidate(TARGET_LIST)
            .unwrap()
            .channel_map_keys()
            .unwrap(),
        ["y-last", "b-first", "x-foreign"]
    );
    assert_eq!(
        store.candidate(CANVAS).unwrap().effective()["channels"],
        json!(["CZMAP001", "CAMAP001", "CTAIL001"])
    );
    assert!(original_text("FFOREIGN01").is_err());
    assert_eq!(
        file_navigation::received_message()["blocks"][0]["elements"][0]["elements"][0]["text"],
        "Original Canvas fallback"
    );
}
#[test]
fn future_rename_does_not_rewrite_held_response_or_original_literal() {
    let original = original_text(CANVAS).unwrap().to_owned();
    let (sender, receiver) = oneshot::channel();
    let mut s = State::default();
    s.future.insert(CANVAS.into(), original.clone());
    s.held.push(Held {
        serial: 1,
        id: CANVAS.into(),
        sender,
        response: original.clone(),
    });
    let updated = renamed(&mut s, CANVAS, "Renamed cold Canvas").unwrap();
    assert_eq!(updated["file"]["title"], "Renamed cold Canvas");
    assert_eq!(updated["file"]["updated"], 102);
    let renamed = OrderedFileInfo::decode(s.future[CANVAS].as_bytes()).unwrap();
    let mut store = super_platinum_core::slack::api::custom::texty::file_title_source::Store::new();
    store.ingest_info("T1", &renamed).unwrap();
    assert_eq!(
        store.candidate(CANVAS).unwrap().channel_map_keys().unwrap(),
        ["z-last", "a-first", "m-foreign"]
    );

    assert_eq!(s.held[0].response, original);
    assert_eq!(original_text(CANVAS).unwrap(), original);
    drop(receiver);
    let held = s.held.remove(0);
    assert!(held.sender.is_closed());
    assert_eq!(held.sender.send(held.response).unwrap_err(), original);
}
#[test]
fn held_mode_is_per_identity_and_late_release_cannot_retarget_other_id() {
    let mut s = State::default();
    assert!(matches!(
        s.modes.get(CANVAS).copied().unwrap_or_default(),
        Mode::Held
    ));
    s.modes.insert(CANVAS.into(), Mode::Success);
    assert!(matches!(
        s.modes.get(TARGET_LIST).copied().unwrap_or_default(),
        Mode::Held
    ));
    let (sender, mut receiver) = oneshot::channel();
    s.held.push(Held {
        serial: 4,
        id: TARGET_LIST.into(),
        sender,
        response: original_text(TARGET_LIST).unwrap().to_owned(),
    });
    s.future.insert(
        TARGET_LIST.into(),
        original_text(TARGET_LIST).unwrap().to_owned(),
    );
    renamed(&mut s, TARGET_LIST, "Renamed cold List").unwrap();
    let held = s.held.remove(0);
    assert_eq!(held.id, TARGET_LIST);
    held.sender.send(held.response).unwrap();
    assert_eq!(
        parsed(&receiver.try_recv().unwrap()).unwrap()["file"]["title"],
        completion::files::LIST_LABEL
    );
    assert_eq!(
        parsed(&s.future[TARGET_LIST]).unwrap()["file"]["title"],
        "Renamed cold List"
    );
}
