use super::*;
fn private(shell: &mut ShellState, token: &Token, error: &str) {
    let raw = if error == "not_visible" {
        json!({"ok":false,"error":error,"title":"Secret title","filetype":"quip"})
    } else {
        json!({"ok":false,"error":error})
    };
    install(
        shell,
        token,
        "FFILE0001",
        socket(shell, token).unwrap(),
        crate::file_events::snapshot_revision(shell),
        &raw,
    )
    .unwrap();
}
#[test]
fn unavailable_canvas_reference_is_receipt_owned_and_recovers_without_reset() {
    for error in ["not_visible", "template_not_visible"] {
        let (mut shell, token) = setup();
        let id = "FFILE0001";
        put(&mut shell, &token, &response(id, "Original", "quip"));
        let original = Observation::capture(&shell, &token, id).unwrap();
        private(&mut shell, &token, error);
        assert!(!original.current(&shell));
        let hidden = Observation::capture(&shell, &token, id).unwrap();
        let raw = json!({"type":"canvas","file_id":id,"text":"https://fixture.invalid/private","opaque":{"keep":7},"style":{"bold":true}});
        assert_eq!(hidden.label(&raw).unwrap(), "Private canvas");
        assert!(hidden.private_canvas());
        assert!(
            hidden.quip.is_none()
                && hidden.members.is_none()
                && hidden.channels.is_none()
                && hidden.groups.is_none()
        );
        assert!(hidden.label(&json!({"type":"file","file_id":id})).is_err());
        let deps = dependencies(&shell, &token, &[id.into()]).unwrap();
        assert!(deps.ids.is_empty() && deps.channels.is_empty() && deps.groups.is_empty());
        put(&mut shell, &token, &response(id, "Recovered", "quip"));
        assert!(!hidden.current(&shell));
        let fresh = Observation::capture(&shell, &token, id).unwrap();
        assert_eq!(
            fresh.label(&raw).unwrap(),
            if error == "not_visible" {
                "Private canvas"
            } else {
                "Recovered"
            }
        );
        assert_eq!(fresh.private_canvas(), error == "not_visible");
        shell
            .core
            .workspaces
            .get_mut(&token.owner.team)
            .unwrap()
            .rt_generation += 1;
        assert!(!fresh.current(&shell));
    }
}
#[test]
fn malformed_receipt_never_reattests_old_private_category_or_foreign_id() {
    let (mut shell, token) = setup();
    private(&mut shell, &token, "template_not_visible");
    let id = "FFILE0001";
    let observed = Observation::capture(&shell, &token, id).unwrap();
    let old_reads = shell
        .profile_edit
        .sections
        .file_titles
        .as_ref()
        .unwrap()
        .reads
        .clone();
    for raw in [
        json!({"ok":false,"error":7}),
        json!({"ok":false,"error":"access_denied"}),
        json!({"ok":true,"file":{"id":"FFOREIGN","mode":"quip"}}),
    ] {
        assert!(
            install(
                &mut shell,
                &token,
                id,
                observed.socket,
                observed.revision + 1,
                &raw
            )
            .is_err()
        );
        assert!(observed.current(&shell));
        assert_eq!(
            shell
                .profile_edit
                .sections
                .file_titles
                .as_ref()
                .unwrap()
                .reads,
            old_reads
        );
        assert_eq!(observed.row.receipt()["error"], "template_not_visible");
    }
    crate::file_events::invalidate(&mut shell, &token.owner.team, id);
    assert!(!observed.current(&shell));
    assert!(Observation::capture(&shell, &token, id).is_err());
}
#[test]
fn private_canvas_presentation_is_mounted_custody_and_exact_capture_proof() {
    let (mut shell, token) = setup();
    private(&mut shell, &token, "template_not_visible");
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
        label: "Private canvas".into(),
        presentation: Some(
            crate::profile_edit::sections::rich::catalog::Presentation::private_canvas(),
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
        serde_json::from_value(json!({"kind":"private-canvas","icon":"lock","disabled":true})).ok();
    assert!(
        catalog
            .context(std::slice::from_ref(&witness), &token.owner)
            .is_err()
    );
    witness.presentation =
        Some(crate::profile_edit::sections::rich::catalog::Presentation::private_canvas());
    witness.value["opaque"] = json!(10);
    assert!(catalog.context(&[witness], &token.owner).is_err());
}

#[test]
fn private_pre_mount_source_change_refuses_but_normal_same_spelling_has_no_icon_authority() {
    let (mut shell, token) = setup();
    private(&mut shell, &token, "template_not_visible");
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
        &response("FFILE0001", "Private canvas", "quip"),
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
        label: "Private canvas".into(),
        presentation: Some(
            crate::profile_edit::sections::rich::catalog::Presentation::private_canvas(),
        ),
    };
    assert!(session.catalog.context(&[witness], &token.owner).is_err());
    assert_eq!(session.before, blocks);
}
