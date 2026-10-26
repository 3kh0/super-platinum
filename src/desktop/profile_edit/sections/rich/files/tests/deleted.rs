use super::*;
fn deleted(shell: &mut ShellState, token: &Token) {
    let generation = socket(shell, token).unwrap();
    let revision = crate::file_events::snapshot_revision(shell);
    install(
        shell,
        token,
        "FFILE0001",
        generation,
        revision,
        &json!({"ok":false,"error":"file_deleted"}),
    )
    .unwrap();
}
#[test]
fn deleted_reference_partial_then_full_recovery_keeps_source_and_workspace_guards() {
    let (mut shell, token) = setup();
    let id = "FFILE0001";
    put(&mut shell, &token, &response(id, "Old secret", "quip"));
    let old = Observation::capture(&shell, &token, id).unwrap();
    deleted(&mut shell, &token);
    assert!(!old.current(&shell));
    let gone = Observation::capture(&shell, &token, id).unwrap();
    let raw = json!({"type":"canvas","file_id":id,"text":"https://fixture.invalid/docs/FFILE0001","opaque":{"keep":7},"style":{"bold":true}});
    assert_eq!(gone.label(&raw).unwrap(), "Deleted canvas");
    assert!(gone.deleted_canvas() && !gone.private_canvas());
    assert!(
        gone.quip.is_none()
            && gone.members.is_none()
            && gone.channels.is_none()
            && gone.groups.is_none()
    );
    assert_eq!(
        gone.row.effective(),
        &json!({"id":id,"mode":"tombstone","is_deleted":true})
    );
    assert!(gone.label(&json!({"type":"file","file_id":id})).is_err());
    let deps = dependencies(&shell, &token, &[id.into()]).unwrap();
    assert!(deps.ids.is_empty() && deps.channels.is_empty() && deps.groups.is_empty());
    put(
        &mut shell,
        &token,
        &json!({"ok":true,"file":{"id":id,"updated":102}}),
    );
    assert!(!gone.current(&shell));
    let partial = Observation::capture(&shell, &token, id).unwrap();
    assert!(partial.deleted_canvas());
    assert_eq!(partial.row.effective()["mode"], "tombstone");
    assert_eq!(partial.row.effective()["is_deleted"], false);
    assert_eq!(partial.label(&raw).unwrap(), "Deleted canvas");
    put(
        &mut shell,
        &token,
        &response(id, "Recovered canvas", "quip"),
    );
    assert!(!partial.current(&shell));
    let normal = Observation::capture(&shell, &token, id).unwrap();
    assert_eq!(normal.label(&raw).unwrap(), "Recovered canvas");
    assert!(!normal.deleted_canvas());
    // A known workspace tombstone is independent authority, never cleared by this reader.
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .deleted_files
        .insert(id.into());
    assert!(!normal.current(&shell));
    assert!(Observation::capture(&shell, &token, id).is_err());
    assert!(!metadata_current(&shell, &token, id));
    deleted(&mut shell, &token);
    assert!(Observation::capture(&shell, &token, id).is_err());
    put(
        &mut shell,
        &token,
        &response(id, "Attempted resurrection", "quip"),
    );
    assert!(Observation::capture(&shell, &token, id).is_err());
    assert!(
        shell.core.workspaces[&token.owner.team]
            .deleted_files
            .contains(id)
    );
}
#[test]
fn deleted_reference_generic_and_malformed_errors_do_not_reattest_source() {
    let (mut shell, token) = setup();
    deleted(&mut shell, &token);
    let id = "FFILE0001";
    let before = Observation::capture(&shell, &token, id).unwrap();
    for raw in [
        json!({"ok":false,"error":"file_not_found"}),
        json!({"ok":false,"error":"access_denied"}),
        json!({"ok":false,"error":7}),
        json!({"ok":true,"file":{"id":"FFOREIGN","mode":"tombstone"}}),
    ] {
        assert!(install(&mut shell, &token, id, before.socket, before.revision, &raw).is_err());
        assert!(before.current(&shell));
        assert_eq!(
            before.row.receipt(),
            &json!({"ok":false,"error":"file_deleted"})
        );
    }
    crate::file_events::invalidate(&mut shell, &token.owner.team, id);
    assert!(!before.current(&shell));
    assert!(Observation::capture(&shell, &token, id).is_err());
}
#[test]
fn deleted_canvas_presentation_is_mounted_custody_and_exact_capture_proof() {
    let (mut shell, token) = setup();
    deleted(&mut shell, &token);
    let raw = json!({"type":"canvas","file_id":"FFILE0001","opaque":9,"style":{"bold":true}});
    fixture::mentions::set_blocks(
        &mut shell,
        vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[raw]}]}),
        ],
    );
    let view = crate::profile_edit::sections::presentation(&shell).unwrap();
    let opened = opening::begin(&mut shell, view.visit, view.serial, "long", 77).unwrap();
    assert!(opening::finish(&mut shell, opened));
    let catalog = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .rich
        .as_ref()
        .unwrap()
        .catalog
        .clone();
    let mut witness = crate::profile_edit::sections::rich::catalog::Witness {
        label_units: None,
        value: raw.clone(),
        label: "Deleted canvas".into(),
        presentation: Some(
            crate::profile_edit::sections::rich::catalog::Presentation::deleted_canvas(),
        ),
    };
    assert!(
        catalog
            .context(std::slice::from_ref(&witness), &token.owner)
            .is_ok()
    );
    put(
        &mut shell,
        &token,
        &response("FFILE0001", "Recovered", "quip"),
    );
    assert!(
        catalog
            .context(std::slice::from_ref(&witness), &token.owner)
            .is_ok()
    );
    witness.presentation = None;
    assert!(
        catalog
            .context(std::slice::from_ref(&witness), &token.owner)
            .is_err()
    );
    witness.presentation =
        serde_json::from_value(json!({"kind":"deleted-canvas","icon":"delete","disabled":false}))
            .ok();
    assert!(
        catalog
            .context(std::slice::from_ref(&witness), &token.owner)
            .is_err()
    );
    witness.presentation =
        Some(crate::profile_edit::sections::rich::catalog::Presentation::deleted_canvas());
    witness.value["opaque"] = json!(10);
    assert!(catalog.context(&[witness], &token.owner).is_err());
}

#[test]
fn deleted_pre_mount_source_change_refuses_but_normal_same_spelling_has_no_icon_authority() {
    let (mut shell, token) = setup();
    deleted(&mut shell, &token);
    let raw = json!({"type":"canvas","file_id":"FFILE0001","opaque":4});
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[raw.clone()]}]}),
    ];
    fixture::mentions::set_blocks(&mut shell, blocks.clone());
    let view = crate::profile_edit::sections::presentation(&shell).unwrap();
    let stale = opening::begin(&mut shell, view.visit, view.serial, "long", 80).unwrap();
    put(
        &mut shell,
        &token,
        &response("FFILE0001", "Deleted canvas", "quip"),
    );
    assert!(!opening::finish(&mut shell, stale));
    let fresh = opening::begin(&mut shell, view.visit, view.serial, "long", 81).unwrap();
    assert!(opening::finish(&mut shell, fresh));
    let session = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .rich
        .as_ref()
        .unwrap();
    assert_eq!(session.catalog.presentations(), json!({}));
    let witness = crate::profile_edit::sections::rich::catalog::Witness {
        label_units: None,
        value: raw,
        label: "Deleted canvas".into(),
        presentation: Some(
            crate::profile_edit::sections::rich::catalog::Presentation::deleted_canvas(),
        ),
    };
    assert!(session.catalog.context(&[witness], &token.owner).is_err());
    assert_eq!(session.before, blocks);
}
