use super::*;
#[test]
fn channel_exact_alias_and_exclusive_flags_keep_eight_prior_modes() {
    for alias in [
        "slack-list-channel-field-create",
        "slack-list-channel-field-create-narrow",
    ] {
        assert_eq!(
            mode_gate_channel(
                None,
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
            Some(FieldCreateMode::Channel)
        );
        for flag in [None, Some(""), Some("0"), Some("true")] {
            assert!(
                mode_gate_channel(
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    flag,
                    Some(alias)
                )
                .is_none()
            );
        }
        for index in 0..8 {
            for extra in [Some(""), Some("0"), Some("1")] {
                let mut f = [None; 8];
                f[index] = extra;
                assert!(
                    mode_gate_channel(
                        f[0],
                        f[1],
                        f[2],
                        f[3],
                        f[4],
                        f[5],
                        f[6],
                        f[7],
                        Some("1"),
                        Some(alias)
                    )
                    .is_none()
                );
            }
        }
    }
    for (index, mode) in [
        FieldCreateMode::Text,
        FieldCreateMode::Number,
        FieldCreateMode::Checkbox,
        FieldCreateMode::Date,
        FieldCreateMode::Select,
        FieldCreateMode::MultiSelect,
        FieldCreateMode::People,
        FieldCreateMode::Link,
    ]
    .into_iter()
    .enumerate()
    {
        let mut f = [None; 8];
        f[index] = Some("1");
        assert_eq!(
            mode_gate_channel(
                f[0],
                f[1],
                f[2],
                f[3],
                f[4],
                f[5],
                f[6],
                f[7],
                None,
                Some(mode.prefix())
            ),
            Some(mode)
        );
        assert!(
            mode_gate_channel(
                f[0],
                f[1],
                f[2],
                f[3],
                f[4],
                f[5],
                f[6],
                f[7],
                Some("0"),
                Some(mode.prefix())
            )
            .is_none()
        );
    }
    assert!(
        mode_gate_channel(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("1"),
            Some("slack-list-link-field-create")
        )
        .is_none()
    );
}
#[test]
fn channel_exact_synthetic_addition_preserves_every_other_metadata_member() {
    let before = json!({"list_metadata":{"schema":[{"id":"old","key":"old","opaque":7}],"views":[{"columns":[{"id":"old","key":"old","visible":true}],"opaque":true}]},"opaque":[1,2]});
    let mut after = before.clone();
    add_field(&mut after, FieldCreateMode::Channel).unwrap();
    assert_eq!(
        after["list_metadata"]["schema"][1],
        json!({"id":"C_CHANNEL_OBSERVED","key":"channel_observed","name":"Channel Équipe 東京","type":"channel","options":{"format":"single_entity","show_member_name":true,"default_value":null},"opaque":{"fixture":"retained"}})
    );
    assert!(add_field(&mut after, FieldCreateMode::Channel).is_err());
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
