use super::*;
use serde_json::json;

fn mounted() -> (ShellState, u64, u64) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_interaction(&mut shell);
    let d = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (visit, serial) = (d.visit, d.serial);
    let opening = opening::begin(&mut shell, visit, serial, "long", 1).unwrap();
    assert!(opening::finish(&mut shell, opening));
    (shell, visit, serial)
}

#[test]
fn newer_query_retires_old_reads_and_old_close_cannot_erase_successor() {
    let (mut shell, visit, serial) = mounted();
    let old = begin(&mut shell, visit, serial, 1, 1).unwrap();
    assert!(old.current(&shell));
    assert!(begin(&mut shell, visit, serial, 1, 1).is_none());
    let next = begin(&mut shell, visit, serial, 1, 2).unwrap();
    assert!(!old.current(&shell));
    assert!(!close(&mut shell, &old));
    assert!(next.current(&shell));
    assert!(close(&mut shell, &next));
    assert!(!next.current(&shell));
    assert!(begin(&mut shell, visit, serial, 1, 2).is_none());
    assert!(begin(&mut shell, visit, serial, 1, 3).is_some());
}

#[test]
fn same_field_key_and_sequence_in_new_opening_do_not_revive_old_lease() {
    let (mut shell, visit, serial) = mounted();
    let old = begin(&mut shell, visit, serial, 1, 1).unwrap();
    let opening = opening::begin(&mut shell, visit, serial, "long", 1).unwrap();
    assert!(opening::finish(&mut shell, opening));
    let fresh = begin(&mut shell, visit, serial, 1, 1).unwrap();
    assert!(!old.current(&shell));
    assert!(!close(&mut shell, &old));
    assert!(fresh.current(&shell));
}

#[test]
fn invalid_sequence_or_mount_does_not_retire_live_query() {
    let (mut shell, visit, serial) = mounted();
    let live = begin(&mut shell, visit, serial, 1, 1).unwrap();
    for sequence in [0, MAX_SAFE, u64::MAX] {
        assert!(begin(&mut shell, visit, serial, 1, sequence).is_none());
    }
    assert!(begin(&mut shell, visit + 1, serial, 1, 2).is_none());
    assert!(begin(&mut shell, visit, serial + 1, 1, 2).is_none());
    assert!(begin(&mut shell, visit, serial, 2, 2).is_none());
    assert!(live.current(&shell));
}

#[tokio::test]
async fn pending_reader_is_woken_by_successor_close_selection_and_unmount() {
    for action in 0..4 {
        let (mut shell, visit, serial) = mounted();
        let lease = begin(&mut shell, visit, serial, 1, 1).unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), lease.cancelled())
                .await
                .is_err()
        );
        match action {
            0 => {
                begin(&mut shell, visit, serial, 1, 2).unwrap();
            }
            1 => {
                assert!(close(&mut shell, &lease));
            }
            2 => {
                // Exercise the same private once-only selection sink; public
                // choose additionally requires a current canonical witness.
                let session = shell
                    .profile_edit
                    .sections
                    .draft
                    .as_mut()
                    .unwrap()
                    .rich
                    .as_mut()
                    .unwrap();
                let q = session.query.as_mut().unwrap();
                q.revision = 1;
                q.values.push(Completion {
                    value: json!({"type":"channel","channel_id":"C00000001"}),
                    label: "#channel".into(),
                });
                assert!(q.consume(1, 1, 0, &mut session.catalog).is_some());
            }
            _ => shell.profile_edit.sections.draft.as_mut().unwrap().rich = None,
        }
        tokio::time::timeout(std::time::Duration::from_secs(1), lease.cancelled())
            .await
            .expect("query retirement must wake its reader");
    }
}

#[test]
fn cooldown_blocks_dispatch_without_erasing_pending_query_and_old_account_cannot_set_it() {
    let (mut shell, visit, serial) = mounted();
    let lease = begin(&mut shell, visit, serial, 1, 1).unwrap();
    assert!(lease.can_dispatch(&shell));
    lease.retain_failure(
        &mut shell,
        &Error::RateLimited {
            retry_after_secs: Some(60),
        },
    );
    assert!(lease.current(&shell));
    assert!(!lease.can_dispatch(&shell));
    shell
        .profile_edit
        .sections
        .entries
        .get_mut(&lease.token.owner)
        .unwrap()
        .retry_at = now() - 1;
    assert!(lease.can_dispatch(&shell));
    shell.core.account_epoch += 1;
    let previous = shell.profile_edit.sections.entries[&lease.token.owner].retry_at;
    lease.retain_failure(
        &mut shell,
        &Error::RateLimited {
            retry_after_secs: Some(600),
        },
    );
    assert_eq!(
        shell.profile_edit.sections.entries[&lease.token.owner].retry_at,
        previous
    );
}

#[test]
fn account_socket_profile_and_draft_replacement_retire_pending_selection() {
    for mutation in 0..5 {
        let (mut shell, visit, serial) = mounted();
        let lease = begin(&mut shell, visit, serial, 1, 1).unwrap();
        match mutation {
            0 => shell.core.account_epoch += 1,
            1 => shell.profile_generation += 1,
            2 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&lease.token.owner.team)
                    .unwrap()
                    .rt_generation += 1
            }
            3 => shell.profile_edit.sections.draft.as_mut().unwrap().rich = None,
            _ => {
                assert!(change_shell(
                    &mut shell,
                    visit,
                    serial,
                    "long",
                    SectionValue::LongText(vec![
                        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"changed underneath"}]}]})
                    ])
                ));
            }
        }
        assert!(!lease.current(&shell), "mutation {mutation}");
        assert!(!close(&mut shell, &lease));
        assert!(choose(&mut shell, &lease, 1, 0).is_none());
    }
}

#[test]
fn competing_save_blocks_query_and_no_source_cannot_authorize_choice() {
    let (mut shell, visit, serial) = mounted();
    let lease = begin(&mut shell, visit, serial, 1, 1).unwrap();
    assert!(choose(&mut shell, &lease, 1, 0).is_none());
    assert!(change_shell(
        &mut shell,
        visit,
        serial,
        "text",
        SectionValue::Text("changed".into())
    ));
    assert!(crate::profile_edit::sections::begin(&mut shell, visit, serial).is_some());
    assert!(!lease.current(&shell));
    assert!(begin(&mut shell, visit, serial, 1, 2).is_none());
}

fn choices() -> Choices {
    Choices {
        sequence: 3,
        revision: 2,
        consumed: false,
        source: None,
        member_source: None,
        group_source: None,
        cancel: tokio::sync::watch::channel(false).0,
        ranking_current: None,
        files: None,
        values: vec![
            Completion {
                value: json!({"type":"channel","channel_id":"C00000001"}),
                label: "#same".into(),
            },
            Completion {
                value: json!({"type":"channel","channel_id":"C00000002"}),
                label: "#same".into(),
            },
        ],
    }
}

#[test]
fn indices_and_revisions_cannot_substitute_tokens_and_only_one_choice_is_spent() {
    let mut choices = choices();
    let mut catalog = catalog::Catalog::default();
    for (sequence, revision, index) in [
        (2, 2, 0),
        (3, 0, 0),
        (3, 1, 0),
        (3, 3, 0),
        (3, 2, 2),
        (3, 2, usize::MAX),
    ] {
        assert!(
            choices
                .consume(sequence, revision, index, &mut catalog)
                .is_none()
        );
        assert!(!choices.consumed);
    }
    let selected = choices.consume(3, 2, 1, &mut catalog).unwrap();
    assert_eq!(selected.value["channel_id"], "C00000002");
    assert!(choices.consume(3, 2, 0, &mut catalog).is_none());
    let (shell, _, _) = mounted();
    let owner = Owner::capture(&shell).unwrap();
    let witness = |id: &str, label: &str| catalog::Witness {
        value: json!({"type":"channel","channel_id":id}),
        label: label.into(),
    };
    assert!(
        catalog
            .context(&[witness("C00000002", "#same")], &owner)
            .is_ok()
    );
    assert!(
        catalog
            .context(&[witness("C00000001", "#same")], &owner)
            .is_err()
    );
    assert!(
        catalog
            .context(&[witness("C00000002", "#renamed")], &owner)
            .is_err()
    );
}

#[test]
fn selected_occurrence_keeps_prior_catalog_and_preserves_style_identity() {
    let mut choices = choices();
    let mut catalog = catalog::Catalog::default();
    let old = json!({"type":"channel","channel_id":"C00000001","opaque": {"keep":true}});
    catalog.insert(&old, "#old-name".into()).unwrap();
    let selected = choices.consume(3, 2, 0, &mut catalog).unwrap();
    let mut styled = selected.value.clone();
    styled["style"] = json!({"bold":true});
    let (shell, _, _) = mounted();
    let owner = Owner::capture(&shell).unwrap();
    assert!(
        catalog
            .context(
                &[
                    catalog::Witness {
                        value: old,
                        label: "#old-name".into()
                    },
                    catalog::Witness {
                        value: styled,
                        label: selected.label
                    },
                ],
                &owner
            )
            .is_ok()
    );
}

#[test]
fn same_label_members_retain_distinct_identity_and_original_opaque_occurrences() {
    let mut choices = choices();
    choices.values = ["UMEMBER01", "UMEMBER02"]
        .into_iter()
        .map(|id| Completion {
            value: json!({"type":"user","user_id":id}),
            label: "@Same name".into(),
        })
        .collect();
    let original = json!({"type":"user","user_id":"UMEMBER01","future":{"keep":true}});
    let mut catalog = catalog::Catalog::default();
    catalog.insert(&original, "@Original label".into()).unwrap();
    let selected = choices.consume(3, 2, 1, &mut catalog).unwrap();
    assert_eq!(selected.value["user_id"], "UMEMBER02");
    let mut styled = selected.value.clone();
    styled["style"] = json!({"bold":true,"italic":true});
    let (shell, _, _) = mounted();
    let owner = Owner::capture(&shell).unwrap();
    assert!(
        catalog
            .context(
                &[
                    catalog::Witness {
                        value: original,
                        label: "@Original label".into()
                    },
                    catalog::Witness {
                        value: styled,
                        label: selected.label.clone()
                    },
                ],
                &owner
            )
            .is_ok()
    );
    assert!(
        catalog
            .context(
                &[catalog::Witness {
                    value: json!({"type":"user","user_id":"UMEMBER01"}),
                    label: selected.label,
                }],
                &owner
            )
            .is_err()
    );
    assert!(choices.consume(3, 2, 0, &mut catalog).is_none());
}

#[test]
fn mixed_same_label_choices_keep_category_and_group_identity() {
    let mut choices = choices();
    choices.values = [
        json!({"type":"user","user_id":"UMEMBER01"}),
        json!({"type":"usergroup","usergroup_id":"SGROUP001"}),
        json!({"type":"usergroup","usergroup_id":"SGROUP002"}),
    ]
    .into_iter()
    .map(|value| Completion {
        value,
        label: "@Same name".into(),
    })
    .collect();
    let mut catalog = catalog::Catalog::default();
    let selected = choices.consume(3, 2, 2, &mut catalog).unwrap();
    assert_eq!(
        selected.value,
        json!({"type":"usergroup","usergroup_id":"SGROUP002"})
    );
    assert!(choices.consume(3, 2, 1, &mut catalog).is_none());
    assert!(choices.consume(3, 2, 0, &mut catalog).is_none());
}

#[test]
fn file_selection_keeps_canonical_wire_and_label_out_of_dom_and_catalog_custody() {
    let mut choices = choices();
    let raw = json!({"type":"canvas","file_id":"FCANVAS01","text":"https://fixture.invalid/f","url":"https://fixture.invalid/f"});
    choices.values = vec![Completion {
        value: raw.clone(),
        label: "Canonical title".into(),
    }];
    let presentation = Presentation {
        sequence: 3,
        revision: 2,
        labels: vec!["Canonical title".into()],
        kinds: vec![CompletionKind::Canvas],
    };
    let rendered = serde_json::to_value(&presentation).unwrap();
    assert_eq!(
        rendered,
        json!({"sequence":3,"revision":2,"labels":["Canonical title"],"kinds":["canvas"]})
    );
    assert!(!rendered.to_string().contains("FCANVAS01"));
    let mut catalog = catalog::Catalog::default();
    let selected = choices.consume(3, 2, 0, &mut catalog).unwrap();
    assert_eq!(selected.value, raw);
    let (shell, _, _) = mounted();
    let owner = Owner::capture(&shell).unwrap();
    let mut styled = raw.clone();
    styled["style"] = json!({"bold":true,"italic":true});
    assert!(
        catalog
            .context(
                &[catalog::Witness {
                    value: styled,
                    label: selected.label.clone()
                }],
                &owner
            )
            .is_ok()
    );
    for mutation in 0..3 {
        let mut wrong = raw.clone();
        match mutation {
            0 => wrong["file_id"] = json!("FOTHER001"),
            1 => wrong["url"] = json!("https://fixture.invalid/other"),
            _ => wrong["opaque"] = json!(true),
        }
        assert!(
            catalog
                .context(
                    &[catalog::Witness {
                        value: wrong,
                        label: selected.label.clone()
                    }],
                    &owner
                )
                .is_err()
        );
    }
    assert!(choices.consume(3, 2, 0, &mut catalog).is_none());
}

#[test]
fn unauthenticated_file_json_in_private_values_cannot_bypass_source_selection() {
    let (mut shell, visit, serial) = mounted();
    let lease = begin(&mut shell, visit, serial, 1, 1).unwrap();
    let q = shell
        .profile_edit
        .sections
        .draft
        .as_mut()
        .unwrap()
        .rich
        .as_mut()
        .unwrap()
        .query
        .as_mut()
        .unwrap();
    q.revision = 1;
    q.values.push(Completion {
        value: json!({"type":"file","file_id":"FFORGED01","text":"FFORGED01"}),
        label: "Forged".into(),
    });
    assert!(choose(&mut shell, &lease, 1, 0).is_none());
    assert!(lease.current(&shell));
}
