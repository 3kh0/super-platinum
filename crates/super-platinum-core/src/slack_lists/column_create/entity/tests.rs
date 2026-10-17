use super::*;
use EntityCreateDefaults::{Null, Omitted, Selected};
use EntityCreateDomain::{Channel, People};
use EntityCreateFormat::{Multiple, Single};

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn selected(domain: EntityCreateDomain, format: EntityCreateFormat) -> EntityCreateOptions {
    let values = match domain {
        People => ids(&["W00000002", "U00000001"]),
        Channel => ids(&["G00000002", "C00000001"]),
    };
    EntityCreateOptions::capture(domain, format, true, Selected(values)).unwrap()
}

#[test]
fn fresh_defaults_are_exact_null_with_names_for_both_domains() {
    for domain in [People, Channel] {
        let options = EntityCreateOptions::new(domain);
        assert_eq!(
            options.wire(),
            json!({"format":"single_entity","show_member_name":true,"default_value":null})
        );
        assert_eq!(options.defaults(), &Null);
        assert_eq!(options.single_display_default(), None);
        assert_eq!(
            EntityCreateOptions::from_wire(domain, &options.wire()).unwrap(),
            options
        );
    }
}

#[test]
fn both_empty_shapes_survive_all_format_transitions() {
    for domain in [People, Channel] {
        for shape in [Null, Omitted] {
            for format in [Single, Multiple] {
                let options =
                    EntityCreateOptions::capture(domain, format, true, shape.clone()).unwrap();
                let wire = options.wire();
                assert_eq!(
                    wire.get("default_value"),
                    if shape == Null {
                        Some(&Value::Null)
                    } else {
                        None
                    }
                );
                assert!(wire.get("default_value_typed").is_none());
                for next in [Single, Multiple] {
                    assert_eq!(options.with_format(next).unwrap().defaults(), &shape);
                }
                assert_eq!(
                    EntityCreateOptions::decode(domain, wire.to_string().as_bytes()).unwrap(),
                    options
                );
            }
        }
    }
}

#[test]
fn ordered_scalar_and_typed_defaults_remain_coherent() {
    for domain in [People, Channel] {
        for format in [Single, Multiple] {
            let options = selected(domain, format);
            let wire = options.wire();
            assert_eq!(wire["default_value"], options.selected_ids().join("|"));
            assert_eq!(
                wire["default_value_typed"][domain.as_str()],
                json!(options.selected_ids())
            );
            assert_eq!(
                EntityCreateOptions::from_wire(domain, &wire).unwrap(),
                options
            );
        }
    }
}

#[test]
fn multi_to_single_preserves_vector_then_real_single_callback_replaces_or_clears() {
    for domain in [People, Channel] {
        let multi = selected(domain, Multiple);
        let single = multi.with_format(Single).unwrap();
        assert_eq!(single.selected_ids(), multi.selected_ids());
        assert_eq!(
            single.single_display_default(),
            Some(multi.selected_ids()[0].as_str())
        );
        assert_eq!(single.with_format(Multiple).unwrap(), multi);
        let first = single.single_display_default().unwrap();
        let sole = single.select_single(first).unwrap();
        assert_eq!(sole.selected_ids(), &[first.to_owned()]);
        assert_eq!(sole.select_single(first).unwrap().defaults(), &Null);
        assert_eq!(single.selected_ids().len(), 2);
    }
}

#[test]
fn multi_clear_omits_both_defaults_and_callbacks_reject_wrong_format() {
    let multi = selected(People, Multiple);
    let cleared = multi.select_multiple(vec![]).unwrap();
    assert_eq!(cleared.defaults(), &Omitted);
    assert!(cleared.wire().get("default_value").is_none());
    assert!(cleared.wire().get("default_value_typed").is_none());
    assert!(multi.select_single("U00000001").is_err());
    assert!(
        multi
            .with_format(Single)
            .unwrap()
            .select_multiple(vec![])
            .is_err()
    );
    assert!(EntityCreateOptions::capture(People, Multiple, true, Selected(vec![])).is_err());
}

#[test]
fn name_toggle_preserves_defaults_and_channel_false_is_not_authorable() {
    for defaults in [Null, Omitted, Selected(ids(&["U00000001", "W00000002"]))] {
        let options = EntityCreateOptions::capture(People, Single, true, defaults.clone()).unwrap();
        let hidden = options.with_show_names(false).unwrap();
        assert_eq!(hidden.defaults(), &defaults);
        assert_eq!(hidden.wire()["show_member_name"], false);
        assert_eq!(hidden.with_show_names(true).unwrap(), options);
    }
    let channel = selected(Channel, Multiple);
    assert!(channel.with_show_names(false).is_err());
    assert!(EntityCreateOptions::capture(Channel, Single, false, Null).is_err());
    let mut wire = channel.wire();
    wire["show_member_name"] = json!(false);
    assert!(EntityCreateOptions::from_wire(Channel, &wire).is_err());
}

#[test]
fn same_domain_preserves_and_changed_domain_resets_to_fresh_defaults() {
    let people = selected(People, Multiple).with_show_names(false).unwrap();
    assert_eq!(people.select_domain(People), people);
    assert_eq!(
        people.select_domain(Channel),
        EntityCreateOptions::new(Channel)
    );
    assert_eq!(
        selected(Channel, Multiple).select_domain(People),
        EntityCreateOptions::new(People)
    );
}

#[test]
fn closed_wire_refuses_incoherent_or_unknown_shapes_without_repair() {
    let valid = selected(People, Single).wire();
    let mut bad = Vec::new();
    let mut value = valid.clone();
    value["default_value"] = json!("U00000001|W00000002");
    bad.push(value);
    let mut value = valid.clone();
    value["default_value_typed"] = json!({"channel":["W00000002","U00000001"]});
    bad.push(value);
    let mut value = valid.clone();
    value["default_value_typed"] = json!({"user":["W00000002","U00000001"],"channel":[]});
    bad.push(value);
    let mut value = valid.clone();
    value["default_value_typed"] = Value::Null;
    bad.push(value);
    let mut value = valid.clone();
    value["default_value_typed"] = json!({"user":["W00000002",false]});
    bad.push(value);
    let mut value = valid.clone();
    value.as_object_mut().unwrap().remove("default_value_typed");
    bad.push(value);
    let mut value = valid.clone();
    value.as_object_mut().unwrap().remove("default_value");
    bad.push(value);
    let mut value = valid.clone();
    value["default_value"] = Value::Null;
    bad.push(value);
    let mut value = valid.clone();
    value["default_value"] = json!("");
    value["default_value_typed"] = json!({"user":[]});
    bad.push(value);
    let mut value = valid.clone();
    value["format"] = json!("single");
    bad.push(value);
    let mut value = valid.clone();
    value["show_member_name"] = json!(1);
    bad.push(value);
    let mut value = valid.clone();
    value["unknown"] = json!(true);
    bad.push(value);
    let mut value = valid.clone();
    value["showMemberName"] = json!(true);
    bad.push(value);
    for value in bad {
        assert!(
            EntityCreateOptions::from_wire(People, &value).is_err(),
            "accepted {value}"
        );
    }
}

#[test]
fn entity_syntax_is_existing_domain_policy_without_coercion_or_deduplication() {
    for value in ["U00000001", "W00000002"] {
        assert!(People.valid_id(value));
    }
    for value in ["C00000001", "G00000002", "D00000003", "Z00000004"] {
        assert!(Channel.valid_id(value));
    }
    for value in [
        "",
        "U123",
        "u00000001",
        "U0000000a",
        " U00000001",
        "U00000001 ",
        "U0000000\n",
        "U00000001|W00000002",
        "U0000000\u{e9}",
        "C00000001",
    ] {
        assert!(
            EntityCreateOptions::capture(People, Single, true, Selected(ids(&[value]))).is_err(),
            "accepted {value:?}"
        );
    }
    assert!(
        EntityCreateOptions::capture(
            People,
            Multiple,
            true,
            Selected(ids(&["U00000001", "U00000001"]))
        )
        .is_err()
    );
    assert!(
        EntityCreateOptions::capture(
            Channel,
            Multiple,
            true,
            Selected(ids(&["C00000001", "U00000001"]))
        )
        .is_err()
    );
}

#[test]
fn no_invented_default_count_limit_or_single_cardinality_limit() {
    let values: Vec<_> = (0..32).map(|n| format!("U{n:08}")).collect();
    for format in [Single, Multiple] {
        let options =
            EntityCreateOptions::capture(People, format, true, Selected(values.clone())).unwrap();
        assert_eq!(options.selected_ids(), values);
        assert_eq!(
            EntityCreateOptions::from_wire(People, &options.wire()).unwrap(),
            options
        );
    }
}

#[test]
fn exact_serialized_budget_is_enforced_again_on_transitions() {
    // ID syntax has no separate length quota. Its repeated scalar/typed bytes
    // count toward the existing options budget; do not invent a server limit.
    let boundary = EntityCreateOptions::capture(
        People,
        Multiple,
        true,
        Selected(vec![format!("U{}", "0".repeat(4043))]),
    )
    .unwrap();
    assert_eq!(boundary.wire().to_string().len(), 8192);
    assert_eq!(
        EntityCreateOptions::from_wire(boundary.domain(), &boundary.wire()).unwrap(),
        boundary
    );
    let mut too_large = boundary.wire();
    let id = format!("{}0", boundary.selected_ids()[0]);
    too_large["default_value"] = json!(id);
    too_large["default_value_typed"][boundary.domain().as_str()] = json!([id]);
    assert_eq!(too_large.to_string().len(), 8194);
    assert!(EntityCreateOptions::from_wire(boundary.domain(), &too_large).is_err());
    // Each of these changes adds exactly one JSON byte: reject 8193 bytes.
    assert!(boundary.with_show_names(false).is_err());
    assert!(boundary.with_format(Single).is_err());
    let mut too_large_by_one = boundary.wire();
    too_large_by_one["show_member_name"] = json!(false);
    assert_eq!(too_large_by_one.to_string().len(), 8193);
    assert!(EntityCreateOptions::from_wire(People, &too_large_by_one).is_err());
    assert_eq!(boundary.wire().to_string().len(), 8192);
}

#[test]
fn byte_decoder_rejects_duplicate_keys_and_oversize_even_if_whitespace() {
    let duplicate = br#"{"format":"single_entity","format":"single_entity","show_member_name":true,"default_value":null}"#;
    assert!(EntityCreateOptions::decode(People, duplicate).is_err());
    let nested = br#"{"format":"single_entity","show_member_name":true,"default_value":"U00000001","default_value_typed":{"user":["U00000001"],"user":["U00000001"]}}"#;
    assert!(EntityCreateOptions::decode(People, nested).is_err());
    let wire = EntityCreateOptions::new(People).wire().to_string();
    let padded = format!("{wire}{}", " ".repeat(8193 - wire.len()));
    assert!(EntityCreateOptions::decode(People, padded.as_bytes()).is_err());
    assert!(EntityCreateOptions::decode(People, b"null").is_err());
}
