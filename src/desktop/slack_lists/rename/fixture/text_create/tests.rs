fn mode_gate(
    text: Option<&str>,
    number: Option<&str>,
    alias: Option<&str>,
) -> Option<FieldCreateMode> {
    super::mode_gate(text, number, None, None, alias)
}
#[test]
fn checkbox_gate_is_exact_and_options_are_absent() {
    for alias in [
        "slack-list-checkbox-field-create",
        "slack-list-checkbox-field-create-narrow",
    ] {
        assert_eq!(
            super::mode_gate(None, None, Some("1"), None, Some(alias)),
            Some(FieldCreateMode::Checkbox)
        );
        for extra in [Some(""), Some("0"), Some("1")] {
            assert!(super::mode_gate(extra, None, Some("1"), None, Some(alias)).is_none());
            assert!(super::mode_gate(None, extra, Some("1"), None, Some(alias)).is_none());
        }
    }
    assert!(
        super::mode_gate(
            None,
            None,
            Some("1"),
            None,
            Some("slack-list-text-field-create")
        )
        .is_none()
    );
    let mut doc =
        json!({"list_metadata":{"schema":[{"id":"old"}],"views":[{"columns":[{"id":"old"}]}]}});
    add_field(&mut doc, FieldCreateMode::Checkbox).unwrap();
    assert!(doc["list_metadata"]["schema"][1].get("options").is_none());
    assert_eq!(doc["list_metadata"]["schema"][1]["type"], "checkbox");
}

use super::*;
#[test]
fn only_exact_opted_in_aliases_enable_text_executor() {
    for alias in [
        "slack-list-text-field-create",
        "slack-list-text-field-create-narrow",
    ] {
        assert_eq!(
            mode_gate(Some("1"), None, Some(alias)),
            Some(FieldCreateMode::Text)
        );
        for flag in [None, Some("true"), Some("0"), Some("")] {
            assert!(mode_gate(flag, None, Some(alias)).is_none());
        }
    }
    for alias in [
        None,
        Some("slack-list-field-visibility"),
        Some("slack-list-text-field-create-extra"),
    ] {
        assert!(mode_gate(Some("1"), None, alias).is_none());
    }
}
#[test]
fn number_gate_is_exact_and_cannot_borrow_text_authority() {
    for alias in [
        "slack-list-number-field-create",
        "slack-list-number-field-create-narrow",
    ] {
        assert_eq!(
            mode_gate(None, Some("1"), Some(alias)),
            Some(FieldCreateMode::Number)
        );
        assert!(mode_gate(Some("1"), Some("1"), Some(alias)).is_none());
        assert!(mode_gate(Some("1"), None, Some(alias)).is_none());
        for value in [None, Some("true"), Some("0"), Some("")] {
            assert!(mode_gate(None, value, Some(alias)).is_none());
        }
    }
    assert!(mode_gate(None, Some("1"), Some("slack-list-text-field-create")).is_none());
    assert!(
        mode_gate(
            None,
            Some("1"),
            Some("slack-list-number-field-create-extra")
        )
        .is_none()
    );
}
#[test]
fn number_addition_has_default_zero_precision_without_backfill() {
    let before = json!({"opaque":7,"list_metadata":{"schema":[{"id":"old","key":"old"}],"views":[{"columns":[{"id":"old","key":"old"}],"opaque":true}]}});
    let mut after = before.clone();
    add_field(&mut after, FieldCreateMode::Number).unwrap();
    assert_eq!(
        after["list_metadata"]["schema"][1],
        json!({"id":"C_NUMBER_OBSERVED","key":"number_observed","name":"Release count","type":"number","options":{"precision":0},"opaque":{"fixture":"retained"}})
    );
    after["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    after["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    assert_eq!(after, before);
}
#[test]
fn addition_preserves_full_prior_metadata_and_refuses_duplicate() {
    let before = json!({"opaque":7,"list_metadata":{"schema":[{"id":"old","key":"old","type":"text","opaque":3}],"views":[{"id":"V_FILTER","columns":[{"id":"old","key":"old"}],"sorts":[{"key":"estimate","ascending":true}],"filters":[{"key":"done","operator":"is_not_empty","values":[]}],"info_column_filters":[],"opaque":false}]}});
    let mut after = before.clone();
    add_field(&mut after, FieldCreateMode::Text).unwrap();
    let mut restored = after.clone();
    restored["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    restored["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    assert_eq!(restored, before);
    assert!(add_field(&mut after, FieldCreateMode::Text).is_err());
}
#[test]
fn replacement_seed_keeps_full_canonical_population_and_active_find_before_retired_read() {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-text-field-create",
    );
    let key = Key::current(&shell).unwrap();
    struct Cleanup(Key);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            with_registry(|r| {
                r.entries.remove(&self.0);
            });
        }
    }
    let _cleanup = Cleanup(key.clone());
    let e = entry(&shell).unwrap();
    let v = shell.slack_lists.view.as_mut().unwrap();
    v.layout.find.input = "Launch item 0".into();
    v.layout.find.applied = "Launch item 0".into();
    v.layout.find.revision = 37;
    crate::slack_lists::rebuild_rows(&mut shell);
    let saved = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    let find = shell.slack_lists.view.as_ref().unwrap().layout.find.clone();
    let (write, intent) = prepare(&mut shell, e.serial).unwrap();
    admitted(
        &shell,
        &write,
        &intent,
        &network::Metadata {
            experiments: None,
            document: saved.document.clone(),
            allow: saved.allow_lists.clone(),
            personal: saved.personal_task_lists.clone().unwrap(),
        },
    )
    .unwrap();
    crate::slack_lists::rename::completed(
        &mut shell,
        &write,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    let guard = Guard::capture_read(&shell, key.clone(), e.serial).unwrap();
    with_registry(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.job = Some(guard.job);
        e.stage = Stage::Checking;
    });
    let old_visit = generation(&shell).unwrap();
    let old_selection = selection(&shell).unwrap();
    let mut changed = saved.document.raw.clone();
    add_field(&mut changed, FieldCreateMode::Text).unwrap();
    crate::slack_lists::close_shell(&mut shell);
    shell.core.account_epoch += 1;
    assert!(crate::slack_lists::open_shell(
        &mut shell,
        serde_json::from_value(changed.clone()).unwrap()
    ));
    let visit = generation(&shell).unwrap();
    assert_ne!(visit, old_visit);
    assert_ne!(selection(&shell), Some(old_selection));
    crate::slack_lists::fixture::refresh(&mut shell);
    assert!(shell.slack_lists.view.as_ref().unwrap().loaded.is_none());
    assert!(blocks_read(&shell));
    let mut wrong = saved.clone();
    wrong.document = Document::parse(changed.clone()).unwrap();
    assert!(seed_reopened(&mut shell, visit, wrong, find.clone()).is_err());
    assert!(seed_reopened(&mut shell, old_visit, saved.clone(), find.clone()).is_err());
    assert!(seed_reopened(&mut shell, visit, saved.clone(), Default::default()).is_err());
    let mut wrong_records = saved.clone();
    wrong_records.records.raw["opaque_fixture_change"] = json!(true);
    assert!(seed_reopened(&mut shell, visit, wrong_records, find.clone()).is_err());
    seed_reopened(&mut shell, visit, saved.clone(), find.clone()).unwrap();
    let same = |s: &ShellState| {
        let v = s.slack_lists.view.as_ref().unwrap();
        assert_eq!(v.loaded.as_ref().unwrap().document, saved.document);
        assert_eq!(v.loaded.as_ref().unwrap().records, saved.records);
        assert_eq!(
            (
                &v.layout.find.input,
                &v.layout.find.applied,
                v.layout.find.revision
            ),
            (&find.input, &find.applied, find.revision)
        );
        assert_eq!(v.layout.selected.as_deref(), Some("V_FILTER"));
        assert_eq!(
            v.layout
                .entries
                .iter()
                .filter_map(|e| match e {
                    crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [3, 1, 7, 5]
                .into_iter()
                .map(|i| format!("R_LIST_{i:03}"))
                .collect::<Vec<_>>()
        );
    };
    same(&shell);
    let retained = entry(&shell).unwrap();
    assert_eq!(retained.stage, Stage::Checking);
    assert_eq!(retained.job, Some(guard.job));
    assert_eq!(retained.serial, e.serial);
    assert!(!guard.current(&shell, Stage::Checking));
    assert!(!crate::slack_lists::rename::checked(
        &mut shell,
        &guard,
        Ok(network::Metadata {
            experiments: None,
            document: Document::parse(changed).unwrap(),
            allow: saved.allow_lists.clone(),
            personal: saved.personal_task_lists.clone().unwrap()
        })
    ));
    same(&shell);
    assert_eq!(entry(&shell).unwrap().stage, Stage::Uncertain);
    assert!(seed_reopened(&mut shell, visit, saved.clone(), find.clone()).is_err());
}
