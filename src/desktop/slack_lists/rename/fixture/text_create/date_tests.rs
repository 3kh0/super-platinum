use super::*;
#[test]
fn date_gate_is_exact_and_excludes_each_other_mode_even_empty() {
    for alias in [
        "slack-list-date-field-create",
        "slack-list-date-field-create-narrow",
    ] {
        assert_eq!(
            mode_gate(None, None, None, Some("1"), Some(alias)),
            Some(FieldCreateMode::Date)
        );
        for flag in [None, Some(""), Some("true"), Some("0")] {
            assert_eq!(mode_gate(None, None, None, flag, Some(alias)), None);
        }
        for flag in [Some(""), Some("1"), Some("0")] {
            assert_eq!(mode_gate(flag, None, None, Some("1"), Some(alias)), None);
            assert_eq!(mode_gate(None, flag, None, Some("1"), Some(alias)), None);
            assert_eq!(mode_gate(None, None, flag, Some("1"), Some(alias)), None);
        }
    }
    for alias in [
        "slack-list-text-field-create",
        "slack-list-number-field-create",
        "slack-list-checkbox-field-create",
        "slack-list-date-field-create-extra",
    ] {
        assert_eq!(mode_gate(None, None, None, Some("1"), Some(alias)), None);
    }
    for (t, n, c, alias, mode) in [
        (
            Some("1"),
            None,
            None,
            "slack-list-text-field-create",
            FieldCreateMode::Text,
        ),
        (
            None,
            Some("1"),
            None,
            "slack-list-number-field-create",
            FieldCreateMode::Number,
        ),
        (
            None,
            None,
            Some("1"),
            "slack-list-checkbox-field-create",
            FieldCreateMode::Checkbox,
        ),
    ] {
        assert_eq!(mode_gate(t, n, c, None, Some(alias)), Some(mode));
        assert_eq!(mode_gate(t, n, c, Some(""), Some(alias)), None);
    }
}
#[test]
fn date_fixture_addition_preserves_exact_options_without_record_defaults() {
    let shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-date-field-create",
    );
    let opened = crate::slack_lists::rename::presentation(&shell).unwrap();
    assert_eq!(
        opened.field_choice.unwrap().kind,
        ColumnCreateKind::DateSingleDefault
    );
    assert!(crate::slack_lists::rename::close_entry(
        &shell,
        opened.serial
    ));
    let original = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let mut raw = original.document.raw.clone();
    let records = original.records.raw.clone();
    add_field(&mut raw, FieldCreateMode::Date).unwrap();
    let doc = Document::parse(raw).unwrap();
    let column = doc.column("C_DATE_OBSERVED").unwrap();
    assert_eq!(column.key, "date_observed");
    assert_eq!(column.name, "Target date");
    assert_eq!(
        column.raw["options"],
        json!({"format":"single","date_format":"default"})
    );
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
            .raw,
        records
    );
    assert!(add_field(&mut doc.raw.clone(), FieldCreateMode::Date).is_err());
}
