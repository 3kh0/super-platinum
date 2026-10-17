use super::*;
#[test]
fn link_exact_alias_and_exclusive_flags_preserve_original_modes() {
    for alias in [
        "slack-list-link-field-create",
        "slack-list-link-field-create-narrow",
    ] {
        assert_eq!(
            mode_gate_link(
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some("1"),
                Some(alias)
            ),
            Some(FieldCreateMode::Link)
        );
        for flag in [None, Some(""), Some("0"), Some("true")] {
            assert!(
                mode_gate_link(None, None, None, None, None, None, None, flag, Some(alias))
                    .is_none()
            );
        }
        for index in 0..7 {
            for extra in [Some(""), Some("0"), Some("1")] {
                let mut flags = [None; 7];
                flags[index] = extra;
                assert!(
                    mode_gate_link(
                        flags[0],
                        flags[1],
                        flags[2],
                        flags[3],
                        flags[4],
                        flags[5],
                        flags[6],
                        Some("1"),
                        Some(alias)
                    )
                    .is_none()
                );
            }
        }
    }
    for mode in [
        FieldCreateMode::Text,
        FieldCreateMode::Number,
        FieldCreateMode::Checkbox,
        FieldCreateMode::Date,
        FieldCreateMode::Select,
        FieldCreateMode::MultiSelect,
        FieldCreateMode::People,
    ] {
        let mut flags = [None; 7];
        let index = match mode {
            FieldCreateMode::Text => 0,
            FieldCreateMode::Number => 1,
            FieldCreateMode::Checkbox => 2,
            FieldCreateMode::Date => 3,
            FieldCreateMode::Select => 4,
            FieldCreateMode::MultiSelect => 5,
            FieldCreateMode::People => 6,
            _ => unreachable!(),
        };
        flags[index] = Some("1");
        assert_eq!(
            mode_gate_link(
                flags[0],
                flags[1],
                flags[2],
                flags[3],
                flags[4],
                flags[5],
                flags[6],
                None,
                Some(mode.prefix())
            ),
            Some(mode)
        );
        assert!(
            mode_gate_link(
                flags[0],
                flags[1],
                flags[2],
                flags[3],
                flags[4],
                flags[5],
                flags[6],
                Some("0"),
                Some(mode.prefix())
            )
            .is_none()
        );
    }
    assert!(
        mode_gate_link(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("1"),
            Some("slack-list-select-field-create")
        )
        .is_none()
    );
}
#[test]
fn link_exact_synthetic_addition_has_no_default_cell_or_normalization() {
    let before = json!({"list_metadata":{"schema":[{"id":"old","key":"old","opaque":7}],"views":[{"columns":[{"id":"old","key":"old","visible":true}],"opaque":true}]},"opaque":[1,2]});
    let mut after = before.clone();
    add_field(&mut after, FieldCreateMode::Link).unwrap();
    assert_eq!(
        after["list_metadata"]["schema"][1],
        json!({"id":"C_LINK_OBSERVED","key":"link_observed","name":"Reference \u{00c9}quipe \u{6771}\u{4eac}","type":"link","opaque":{"fixture":"retained"}})
    );
    assert!(add_field(&mut after, FieldCreateMode::Link).is_err());
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
