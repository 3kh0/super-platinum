use super::super::{CellEdit, CellEditValue, CellValue};
use super::*;
use EntityCreateDefaults::{Null, Omitted, Selected};
use EntityCreateDomain::{Channel, People};
use EntityCreateFormat::{Multiple, Single};

fn baseline() -> (Document, RecordsPage, PersonalTaskLists) {
    let d = Document::parse(json!({"id":"F_ENTITY","mode":"list","user":"U1","access":"owner","created":1,"list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[{"id":"TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"TITLE","key":"NAME","visible":true}],"filters":[],"info_column_filters":[],"opaque":{"keep":1}}],"opaque":{"keep":2}}})).unwrap();
    let p = RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","fields":[{"key":"NAME","text":"Keep"}]},{"id":"R2","parent_record_id":"R1","fields":[]}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap();
    let policy = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    (d, p, policy)
}
fn ids(domain: EntityCreateDomain) -> Vec<String> {
    match domain {
        People => ["W00000002", "U00000001"],
        Channel => ["G00000002", "C00000001"],
    }
    .map(str::to_owned)
    .to_vec()
}
fn capture(
    d: &Document,
    p: &RecordsPage,
    policy: &PersonalTaskLists,
    options: EntityCreateOptions,
) -> ColumnCreate {
    ColumnCreate::capture_entity(
        d,
        p,
        "V1",
        " References \u{1f680} ",
        options,
        "U1",
        Some("full"),
        Some(policy),
    )
    .unwrap()
}
fn added(d: &Document, intent: &ColumnCreate) -> Document {
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"SERVER_ENTITY","key":"references","name":intent.desired_name(),"type":intent.kind().as_str(),"options":intent.options().unwrap(),"opaque":{"keep":3}}));
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"SERVER_ENTITY","key":"references","visible":true}));
    Document::parse(raw).unwrap()
}

#[test]
fn configured_matrix_freezes_domain_shape_order_and_observes_without_backfill() {
    let (d, p, policy) = baseline();
    for domain in [People, Channel] {
        for format in [Single, Multiple] {
            for defaults in [
                Null,
                Omitted,
                Selected(vec![ids(domain)[0].clone()]),
                Selected(ids(domain)),
            ] {
                for names in [true, false]
                    .into_iter()
                    .filter(|names| *names || domain == People)
                {
                    let options =
                        EntityCreateOptions::capture(domain, format, names, defaults.clone())
                            .unwrap();
                    let expected = options.wire();
                    let intent = capture(&d, &p, &policy, options.clone());
                    assert_eq!(intent.kind().as_str(), domain.as_str());
                    assert_eq!(intent.options(), Some(expected));
                    let _later = options
                        .with_format(if format == Single { Multiple } else { Single })
                        .unwrap();
                    assert_eq!(intent.options(), Some(options.wire()));
                    let fresh = added(&d, &intent);
                    let c = intent.matching_addition(&fresh, &p).unwrap();
                    assert_eq!(c.id, "SERVER_ENTITY");
                    assert!(p.records.iter().all(|r| c.cell(r) == CellValue::Empty));
                    assert!(
                        p.records
                            .iter()
                            .all(|r| !r.fields.contains_key("references"))
                    );
                    assert_eq!(intent.original(), &d);
                    assert_eq!(intent.original_records(), &p);
                }
            }
        }
    }
}

#[test]
fn legacy_default_capture_kind_wire_is_unchanged_and_owned_entity_matches_it() {
    let (d, p, policy) = baseline();
    for (domain, kind) in [
        (People, ColumnCreateKind::PeopleSingleDefault),
        (Channel, ColumnCreateKind::ChannelSingleDefault),
    ] {
        let legacy = ColumnCreate::capture_kind(
            &d,
            &p,
            "V1",
            " References \u{1f680} ",
            kind,
            "U1",
            Some("full"),
            Some(&policy),
        )
        .unwrap();
        let configured = capture(&d, &p, &policy, EntityCreateOptions::new(domain));
        assert_eq!(legacy.kind(), configured.kind());
        assert_eq!(
            legacy.options(),
            Some(json!({"format":"single_entity","show_member_name":true,"default_value":null}))
        );
        assert_eq!(legacy.options(), configured.options());
        assert!(
            legacy
                .matching_addition(&added(&d, &configured), &p)
                .is_some()
        );
    }
}

#[test]
fn readback_distinguishes_null_omitted_and_exact_ordered_scalar_typed_names_format() {
    let (d, p, policy) = baseline();
    for domain in [People, Channel] {
        let null = capture(&d, &p, &policy, EntityCreateOptions::new(domain));
        let omitted = capture(
            &d,
            &p,
            &policy,
            EntityCreateOptions::capture(domain, Single, true, Omitted).unwrap(),
        );
        assert!(null.matching_addition(&added(&d, &omitted), &p).is_none());
        assert!(omitted.matching_addition(&added(&d, &null), &p).is_none());
        let intent = capture(
            &d,
            &p,
            &policy,
            EntityCreateOptions::capture(domain, Single, true, Selected(ids(domain))).unwrap(),
        );
        let fresh = added(&d, &intent);
        for change in 0..8 {
            let mut raw = fresh.raw.clone();
            let options = &mut raw["list_metadata"]["schema"][1]["options"];
            match change {
                0 => {
                    options["default_value"] =
                        json!(ids(domain).into_iter().rev().collect::<Vec<_>>().join("|"))
                }
                1 => options["default_value_typed"][domain.as_str()]
                    .as_array_mut()
                    .unwrap()
                    .reverse(),
                2 => options["format"] = json!("multi_entity"),
                3 => options["show_member_name"] = json!(false),
                4 => {
                    options
                        .as_object_mut()
                        .unwrap()
                        .remove("default_value_typed");
                }
                5 => options["opaque_server_normalization"] = json!(true),
                6 => options["default_value"] = json!(ids(domain)[0]),
                _ => {
                    raw["list_metadata"]["schema"][1]["type"] =
                        json!(if domain == People { "channel" } else { "user" })
                }
            }
            assert!(
                intent
                    .matching_addition(&Document::parse(raw).unwrap(), &p)
                    .is_none()
            );
        }
    }
}

#[test]
fn configured_creation_preserves_complete_owner_population_and_representation_guards() {
    let (d, p, policy) = baseline();
    let options = EntityCreateOptions::new(People)
        .with_format(Multiple)
        .unwrap();
    let intent = capture(&d, &p, &policy, options.clone());
    assert!(
        intent
            .revalidate(&d, &p, "OTHER", Some("full"), Some(&policy))
            .is_err()
    );
    assert!(
        intent
            .revalidate(&d, &p, "U1", Some("read"), Some(&policy))
            .is_err()
    );
    assert!(
        ColumnCreate::capture_entity(
            &d,
            &p,
            "V1",
            "Names",
            options.clone(),
            "U1",
            Some("full"),
            None
        )
        .is_err()
    );
    let mut forged = d.clone();
    forged.id = "OTHER".into();
    assert!(
        ColumnCreate::capture_entity(
            &forged,
            &p,
            "V1",
            "Names",
            options,
            "U1",
            Some("full"),
            Some(&policy)
        )
        .is_err()
    );
    let mut changed = p.raw.clone();
    changed["response_metadata"]["next_cursor"] = json!("OTHER");
    assert!(
        intent
            .revalidate(
                &d,
                &RecordsPage::parse(changed).unwrap(),
                "U1",
                Some("full"),
                Some(&policy)
            )
            .is_err()
    );
    let mut changed = d.raw.clone();
    changed["list_metadata"]["opaque"]["keep"] = json!(9);
    assert!(
        intent
            .revalidate(
                &Document::parse(changed).unwrap(),
                &p,
                "U1",
                Some("full"),
                Some(&policy)
            )
            .is_err()
    );
}

#[test]
fn entity_attempt_never_rearms_after_lost_reply_or_proven_not_requested() {
    let (d, p, policy) = baseline();
    for domain in [People, Channel] {
        for not_requested in [false, true] {
            let intent = capture(
                &d,
                &p,
                &policy,
                EntityCreateOptions::capture(domain, Single, true, Selected(ids(domain))).unwrap(),
            );
            let expected = intent.options();
            let mut attempt = ColumnCreateAttempt::new(intent);
            assert!(
                attempt
                    .admit(&d, &p, "OTHER", Some("full"), Some(&policy))
                    .is_err()
            );
            assert_eq!(attempt.phase(), ColumnCreatePhase::Reviewing);
            let permit = attempt
                .admit(&d, &p, "U1", Some("full"), Some(&policy))
                .unwrap();
            assert_eq!(permit.into_intent().options(), expected);
            assert!(attempt.finish(not_requested));
            assert_eq!(
                attempt.phase(),
                if not_requested {
                    ColumnCreatePhase::NotRequested
                } else {
                    ColumnCreatePhase::Unconfirmed
                }
            );
            assert!(!attempt.finish(!not_requested));
            assert!(
                attempt
                    .admit(&d, &p, "U1", Some("full"), Some(&policy))
                    .is_err()
            );
        }
    }
}

#[test]
fn retained_two_default_single_creation_does_not_relax_single_cell_validation() {
    let (d, p, policy) = baseline();
    for domain in [People, Channel] {
        let options = EntityCreateOptions::capture(domain, Multiple, true, Selected(ids(domain)))
            .unwrap()
            .with_format(Single)
            .unwrap();
        let intent = capture(&d, &p, &policy, options);
        let fresh = added(&d, &intent);
        let column = intent.matching_addition(&fresh, &p).unwrap();
        assert_eq!(column.cell(&p.records[0]), CellValue::Empty);
        let edit_value = |values| match domain {
            People => CellEditValue::Users(values),
            Channel => CellEditValue::Channels(values),
        };
        assert!(
            CellEdit::capture(
                &fresh,
                &p,
                "R1",
                &column.id,
                Some("full"),
                Some(&policy),
                edit_value(ids(domain))
            )
            .is_err()
        );
        let edit = CellEdit::capture(
            &fresh,
            &p,
            "R1",
            &column.id,
            Some("full"),
            Some(&policy),
            edit_value(vec![ids(domain)[0].clone()]),
        )
        .unwrap();
        assert_eq!(edit.cells()[0][domain.as_str()], json!([ids(domain)[0]]));
        assert!(
            p.records
                .iter()
                .all(|r| !r.fields.contains_key("references"))
        );
    }
}
