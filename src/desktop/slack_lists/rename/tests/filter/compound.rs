use super::*;
use crate::slack_lists::rename::filter as flow;
use super_platinum_core::slack_lists::NumberFilterOperator as Op;
#[test]
fn over_cap_current_text_survives_comparison_change_and_never_revives_old_valid_value() {
    let (mut s, _c) = two();
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let row = e.filter_rows[0].clone();
    let long = "9".repeat(129);
    assert!(flow::update_value(
        &s,
        e.serial,
        visit,
        row.id,
        row.epoch,
        "12.50".into()
    ));
    assert!(flow::update_value(
        &s,
        e.serial,
        visit,
        row.id,
        row.epoch,
        long.clone()
    ));
    let current = entry(&s).unwrap();
    assert!(!flow::valid(match &current.input {
        Input::Filter(v) => v,
        _ => panic!(),
    }));
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            current.filter_choice,
            flow::Action::Operator(row.id, row.epoch, Op::LessThan)
        )
        .is_some()
    );
    let current = entry(&s).unwrap();
    let Input::Filter(v) = &current.input else {
        panic!()
    };
    assert!(
        matches!(&v.conditions()[0],ViewFilterChoice::Number{operator:Op::LessThan,value:Some(v),..} if v==&long)
    );
    let capture = flow::reserve(&s, e.serial, visit, current.filter_choice).unwrap();
    let mut data = snapshots(&s);
    data[0].operator = "less_than".into();
    data[0].value = Some(long);
    assert!(flow::finish(&mut s, &capture, data).is_none());
    assert_eq!(entry(&s).unwrap().input, current.input);
    assert!(entry(&s).unwrap().intent.is_none());
    assert!(flow::release(&s, &capture));
}
fn two() -> (ShellState, Cleanup) {
    let (mut s, c) = filter_shell();
    let l = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    for column in l.document.raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
    {
        if ["C_N", "C_OTHER"].iter().any(|id| column["id"] == *id) {
            column["type"] = json!("number");
            column["options"] = json!({"precision":2});
        }
    }
    l.document = Document::parse(l.document.raw.clone()).unwrap();
    rebuild_rows(&mut s);
    open(&mut s);
    assert!(change(
        &s,
        ViewFilterChoice::All(
            ["C_N", "C_OTHER"]
                .map(|id| ViewFilterChoice::Number {
                    column_id: id.into(),
                    operator: Op::Equal,
                    value: Some("1".into())
                })
                .to_vec()
        )
    ));
    (s, c)
}
fn snapshots(s: &ShellState) -> Vec<flow::Snapshot> {
    let e = entry(s).unwrap();
    let Input::Filter(input) = e.input else {
        panic!()
    };
    e.filter_rows
        .iter()
        .zip(input.conditions())
        .enumerate()
        .map(|(i, (r, v))| flow::Snapshot {
            id: r.id.to_string(),
            epoch: r.epoch.to_string(),
            column: v.column_id().unwrap().into(),
            operator: "equal".into(),
            value: Some(if i == 0 { "12.50" } else { "-2.75" }.into()),
            valid: true,
        })
        .collect()
}
#[test]
fn two_native_values_are_atomic_and_invalid_second_never_changes_first() {
    for failure in 0..6 {
        let (mut s, _c) = two();
        let before = entry(&s).unwrap();
        let visit = generation(&s).unwrap();
        let capture = flow::reserve(&s, before.serial, visit, before.filter_choice).unwrap();
        let mut data = snapshots(&s);
        match failure {
            0 => data[1].valid = false,
            1 => {
                data.pop();
            }
            2 => data[1] = data[0].clone(),
            3 => data.swap(0, 1),
            4 => data[1].value = Some("NaN".into()),
            _ => data.push(data[0].clone()),
        }
        assert!(flow::finish(&mut s, &capture, data).is_none());
        let after = entry(&s).unwrap();
        assert_eq!(after.input, before.input);
        assert_eq!(after.job, None);
        assert!(after.intent.is_none());
        assert_eq!(after.stage, Stage::Editing);
        assert!(flow::release(&s, &capture));
        assert!(entry(&s).unwrap().filter_capture.is_none());
    }
    let (mut s, _c) = two();
    let before = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let capture = flow::reserve(&s, before.serial, visit, before.filter_choice).unwrap();
    let data = snapshots(&s);
    let (_, intent) = flow::finish(&mut s, &capture, data).unwrap();
    let Intent::Filter(intent) = intent else {
        panic!()
    };
    assert_eq!(
        intent.filters(),
        &json!([{"key":"estimate","operator":"equal","values":["12.50"]},{"key":"other","operator":"equal","values":["-2.75"]}])
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Preparing);
    assert!(entry(&s).unwrap().filter_capture.is_none());
    assert!(!flow::release(&s, &capture));
}
#[test]
fn reservation_blocks_callbacks_cancel_retires_and_old_drop_cannot_clear_new_capture() {
    let (mut s, _c) = two();
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let row = e.filter_rows[0].clone();
    for text in ["1", "12", "12.50"] {
        assert!(flow::update_value(
            &s,
            e.serial,
            visit,
            row.id,
            row.epoch,
            text.into()
        ));
    }
    let first = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    assert!(flow::reserve(&s, e.serial, visit, e.filter_choice).is_none());
    assert!(!flow::update_value(
        &s,
        e.serial,
        visit,
        row.id,
        row.epoch,
        "9".into()
    ));
    assert!(flow::structural(&s, e.serial, visit, e.filter_choice, flow::Action::Clear).is_none());
    assert!(prepare(&mut s, e.serial).is_none());
    assert!(flow::release(&s, &first));
    let second = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    drop(first);
    assert!(entry(&s).unwrap().filter_capture.is_some());
    let data = snapshots(&s);
    assert!(close_entry(&s, e.serial));
    assert!(flow::finish(&mut s, &second, data).is_none());
    assert!(entry(&s).is_none());
}
#[test]
fn stable_rows_survive_removal_and_field_operator_aba_refuses_old_callbacks() {
    let (s, _c) = two();
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let first = e.filter_rows[0].clone();
    let second = e.filter_rows[1].clone();
    let (_, target) = flow::structural(
        &s,
        e.serial,
        visit,
        e.filter_choice,
        flow::Action::Remove(first.id),
    )
    .unwrap();
    assert_eq!(target, format!("filter-field-{}", second.id));
    assert_eq!(entry(&s).unwrap().filter_rows, vec![second.clone()]);
    assert!(flow::update_value(
        &s,
        e.serial,
        visit,
        second.id,
        second.epoch,
        "12.50".into()
    ));
    let e = entry(&s).unwrap();
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::Add("C_N".into())
        )
        .is_some()
    );
    let e = entry(&s).unwrap();
    assert_ne!(e.filter_rows[1].id, first.id);
    assert!(!flow::update_value(
        &s,
        e.serial,
        visit,
        first.id,
        first.epoch,
        "8".into()
    ));
    let row = e.filter_rows[0].clone();
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::Operator(row.id, row.epoch, Op::IsEmpty)
        )
        .is_some()
    );
    let e = entry(&s).unwrap();
    let row2 = e.filter_rows[0].clone();
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::Operator(row2.id, row2.epoch, Op::Equal)
        )
        .is_some()
    );
    assert!(!flow::update_value(
        &s,
        e.serial,
        visit,
        row.id,
        row.epoch,
        "7".into()
    ));
    let Input::Filter(value) = entry(&s).unwrap().input else {
        panic!()
    };
    assert!(matches!(
        &value.conditions()[0],
        ViewFilterChoice::Number { value: None, .. }
    ));
}
#[test]
fn source_or_owner_retirement_refuses_whole_capture_without_partial_values() {
    for owner in [false, true] {
        let (mut s, _c) = two();
        let e = entry(&s).unwrap();
        let capture =
            flow::reserve(&s, e.serial, generation(&s).unwrap(), e.filter_choice).unwrap();
        let data = snapshots(&s);
        if owner {
            s.core.account_epoch += 1;
        } else {
            s.slack_lists.view.as_mut().unwrap().revision += 1;
        }
        assert!(flow::finish(&mut s, &capture, data).is_none());
        drop(capture);
        with_registry(|r| {
            let retained = r.entries.values().find(|v| v.serial == e.serial).unwrap();
            assert_eq!(retained.input, e.input);
            assert!(retained.filter_capture.is_none());
            assert!(retained.intent.is_none());
        });
    }
}

#[test]
fn expired_or_dropped_capture_cannot_admit_or_wedge_the_next_save() {
    let (mut s, _c) = two();
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let capture = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    let data = snapshots(&s);
    assert!(
        flow::finish_at(
            &mut s,
            &capture,
            data,
            std::time::Instant::now() + std::time::Duration::from_secs(6)
        )
        .is_none()
    );
    assert_eq!(entry(&s).unwrap().input, e.input);
    drop(capture);
    let next = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    assert!(entry(&s).unwrap().filter_capture.is_some());
    drop(next);
    assert!(entry(&s).unwrap().filter_capture.is_none());
}

#[test]
fn mixed_and_readback_preserves_active_find_number_sort_and_full_raw_page() {
    let mut s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-compound-filter",
    );
    let _c = Cleanup(Key::current(&s).unwrap());
    let v = s.slack_lists.view.as_mut().unwrap();
    let l = v.loaded.as_mut().unwrap();
    let mut raw = l.records.raw.clone();
    for i in 0..7 {
        let fields = raw["records"][i]["fields"].as_array_mut().unwrap();
        fields.iter_mut().find(|f| f["key"] == "NAME").unwrap()["text"] =
            json!(format!("Compound cohort {i}"));
        if let Some(c) = fields.iter_mut().find(|f| f["key"] == "done") {
            c["checkbox"] = json!([false, true, false, true, true, false, false][i]);
        }
    }
    l.records = RecordsPage::parse(raw).unwrap();
    v.layout.find.input = "Compound cohort".into();
    v.layout.find.applied = "Compound cohort".into();
    v.layout.find.revision = 81;
    rebuild_rows(&mut s);
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let Input::Filter(input) = &e.input else {
        panic!()
    };
    let data = e
        .filter_rows
        .iter()
        .zip(input.conditions())
        .map(|(r, v)| {
            let (operator, value) = match v {
                ViewFilterChoice::Checkbox { checked, .. } => {
                    (if *checked { "is_not_empty" } else { "is_empty" }, None)
                }
                ViewFilterChoice::Number {
                    operator, value, ..
                } => (operator.as_str(), value.clone()),
                _ => panic!(),
            };
            flow::Snapshot {
                id: r.id.to_string(),
                epoch: r.epoch.to_string(),
                column: v.column_id().unwrap().into(),
                operator: operator.into(),
                value,
                valid: true,
            }
        })
        .collect();
    let capture = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    let (g, intent) = flow::finish(&mut s, &capture, data).unwrap();
    admitted(&s, &g, &intent, &metadata(&s, None)).unwrap();
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(metadata(&s, None).document, before.document);
    let Intent::Filter(intent) = intent else {
        panic!()
    };
    let check = checking(&s);
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
    m.document.raw["list_metadata"]["views"][0]["filters"]
        .as_array_mut()
        .unwrap()
        .reverse();
    m.document = Document::parse(m.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(prepare(&mut s, e.serial).is_none());
    let check = checking(&s);
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
    m.document = Document::parse(m.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let v = s.slack_lists.view.as_ref().unwrap();
    let ids = v
        .layout
        .entries
        .iter()
        .filter_map(|e| match e {
            crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, [3, 1, 4].map(|i| format!("R_LIST_{i:03}")));
    assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(v.layout.find.applied, "Compound cohort");
    assert_eq!(v.layout.find.revision, 81);
    let mut restored = v.loaded.as_ref().unwrap().document.raw.clone();
    restored["list_metadata"]["views"][0]["filters"] = json!([]);
    assert_eq!(restored, before.document.raw);
    assert!(close_entry(&s, e.serial));
    open(&mut s);
    assert_eq!(
        presentation(&s).unwrap().filter.unwrap().original,
        intent.desired_filter().clone()
    );
}

#[test]
fn add_defaults_cap_unique_fields_clear_and_stale_save_are_explicit() {
    let (mut s, _c) = two();
    let serial = entry(&s).unwrap().serial;
    assert!(close_entry(&s, serial));
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    for i in 2..8 {
        loaded.document.raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":format!("C_{i}"),"key":format!("k{i}"),"name":format!("N{i}"),"type":"number","options":{"precision":0}}));
    }
    loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
    rebuild_rows(&mut s);
    open(&mut s);
    let e = entry(&s).unwrap();
    let old = e.filter_choice;
    let visit = generation(&s).unwrap();
    let ids = ["C_N".into(), "C_OTHER".into()]
        .into_iter()
        .chain((2..8).map(|i| format!("C_{i}")))
        .collect::<Vec<String>>();
    for id in &ids {
        let e = entry(&s).unwrap();
        let (_, target) = flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::Add(id.clone()),
        )
        .unwrap();
        let e = entry(&s).unwrap();
        assert_eq!(
            target,
            format!("filter-operator-{}", e.filter_rows.last().unwrap().id)
        );
        let Input::Filter(input) = e.input else {
            panic!()
        };
        assert!(matches!(
            input.conditions().last(),
            Some(ViewFilterChoice::Number {
                operator: Op::Equal,
                value: None,
                ..
            })
        ));
    }
    let e = entry(&s).unwrap();
    assert_eq!(e.filter_rows.len(), 8);
    assert!(flow::reserve(&s, e.serial, visit, old).is_none());
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::Add(ids[0].clone())
        )
        .is_none()
    );
    let rows = e.filter_rows.clone();
    let (_, target) = flow::structural(
        &s,
        e.serial,
        visit,
        e.filter_choice,
        flow::Action::Remove(rows[2].id),
    )
    .unwrap();
    assert_eq!(target, format!("filter-field-{}", rows[3].id));
    let e = entry(&s).unwrap();
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::Add(ids[2].clone())
        )
        .is_some()
    );
    let e = entry(&s).unwrap();
    assert_ne!(e.filter_rows.last().unwrap().id, rows[2].id);
    let (_, target) =
        flow::structural(&s, e.serial, visit, e.filter_choice, flow::Action::Clear).unwrap();
    assert_eq!(target, "slack-list-filter-add-field");
    let e = entry(&s).unwrap();
    assert_eq!(e.input, Input::Filter(ViewFilterChoice::None));
    assert!(e.filter_rows.is_empty());
    assert!(flow::structural(&s, e.serial, visit, e.filter_choice, flow::Action::Clear).is_none());
    assert!(e.intent.is_none());
    assert!(e.job.is_none());
}
