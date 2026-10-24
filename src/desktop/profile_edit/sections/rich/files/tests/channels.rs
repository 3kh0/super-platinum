use super::*;

#[test]
fn nested_channel_receipt_stays_separate_from_member_and_canonical_admission() {
    let (mut shell, token) = setup();
    let mut raw = response("FFILE0001", "Not a nested label", "quip");
    raw["file"]["title_blocks"] = json!([{"type":"rich_text","elements":[
        {"type":"rich_text_section","elements":[{"type":"text","text":"In "},
        {"type":"channel","channel_id":"CUNKNOWN1","opaque":{"keep":1}}]}]}]);
    put(&mut shell, &token, &raw);
    let before = dependencies(&shell, &token, &["FFILE0001".into()]).unwrap();
    assert!(before.ids.is_empty());
    assert_eq!(
        before.channels,
        std::collections::BTreeSet::from(["CUNKNOWN1".into()])
    );
    assert!(metadata_current(&shell, &token, "FFILE0001"));
    assert!(reuse_metadata(&shell, &token, "FFILE0001", true));
    assert!(Observation::capture(&shell, &token, "FFILE0001").is_err());
    assert!(channel_hits(&mut shell, &token, &before).is_empty());
    assert_eq!(
        shell
            .profile_edit
            .sections
            .file_titles
            .as_ref()
            .unwrap()
            .store
            .row("FFILE0001")
            .unwrap()
            .source(),
        &raw["file"]
    );

    // The dependency scanner must not send channel IDs to member recovery, or
    // lose channels when an actual user also selects the Quip-bypass branch.
    raw["file"]["title_blocks"][0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"user","user_id":"UMISSING1"}));
    put(&mut shell, &token, &raw);
    assert!(!before.current(&shell, &token));
    let mixed = dependencies(&shell, &token, &["FFILE0001".into()]).unwrap();
    assert_eq!(mixed.ids, vec!["UMISSING1"]);
    assert_eq!(
        mixed.channels,
        std::collections::BTreeSet::from(["CUNKNOWN1".into()])
    );
    shell.core.account_epoch += 1;
    assert!(!mixed.current(&shell, &token));
    assert!(channel_hits(&mut shell, &token, &mixed).is_empty());
}

#[test]
fn nested_channel_feedback_does_not_admit_a_fabricated_outer_channel() {
    let owner = Owner {
        account: "account".into(),
        team: "T12345678".into(),
        user: "U12345678".into(),
    };
    let raw = json!({"type":"canvas","file_id":"FFILE0001","text":"https://fixture.invalid/canvas",
        "opaque":{"keep":1}});
    let mut catalog = catalog::Catalog::default();
    catalog.insert(&raw, "In #at-opening".into()).unwrap();
    catalog.channel_dependencies(["C12345678".into()]);
    assert!(catalog.contains_any(&["C12345678".into()]));
    assert!(!catalog.contains_any(&["COTHER001".into(), "FFILE0001".into()]));
    let mut witness = catalog::Witness {
        value: raw.clone(),
        label: "In #at-opening".into(),
    };
    assert!(
        catalog
            .context(std::slice::from_ref(&witness), &owner)
            .is_ok()
    );
    witness.label = "In #after-refresh".into();
    assert!(
        catalog
            .context(std::slice::from_ref(&witness), &owner)
            .is_err()
    );
    witness.value = json!({"type":"channel","channel_id":"C12345678"});
    witness.label = "#at-opening".into();
    assert!(catalog.context(&[witness], &owner).is_err());
    // Registering refresh feedback did not alter either raw token or its label.
    assert!(
        catalog
            .context(
                &[catalog::Witness {
                    value: raw,
                    label: "In #at-opening".into()
                }],
                &owner
            )
            .is_ok()
    );
}
