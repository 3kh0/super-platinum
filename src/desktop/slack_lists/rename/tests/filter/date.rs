use super::*;
use crate::slack_lists::rename::filter as flow;
use super_platinum_core::slack_lists::{DateFilterOperator as Op, NumberFilterOperator as Num};
fn mixed() -> (ShellState, Cleanup) {
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
        if column["id"] == "C_N" {
            column["type"] = json!("number");
            column["options"] = json!({"precision":2});
        }
        if column["id"] == "C_OTHER" {
            column["type"] = json!("date");
            column["options"] = json!({"date_format":"DD/MM/YYYY"});
        }
    }
    l.document = Document::parse(l.document.raw.clone()).unwrap();
    rebuild_rows(&mut s);
    open(&mut s);
    assert!(change(
        &s,
        ViewFilterChoice::All(vec![
            ViewFilterChoice::Number {
                column_id: "C_N".into(),
                operator: Num::LessThan,
                value: Some("1".into())
            },
            ViewFilterChoice::Date {
                column_id: "C_OTHER".into(),
                operator: Op::Equal,
                value: Some("2024-02-29".into())
            }
        ])
    ));
    (s, c)
}
fn snapshot(s: &ShellState, date: Option<&str>) -> Vec<flow::Snapshot> {
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
            operator: match v {
                ViewFilterChoice::Date { operator, .. } => operator.as_str(),
                _ => "less_than",
            }
            .into(),
            value: if i == 0 {
                Some("12.50".into())
            } else {
                date.map(str::to_owned)
            },
            valid: true,
        })
        .collect()
}
#[test]
fn atomic_date_snapshot_uses_late_native_calendar_and_refuses_partial_sibling_updates() {
    for bad in [
        None,
        Some(""),
        Some("2023-02-29"),
        Some("today"),
        Some("2024-02-29T00:00:00Z"),
    ] {
        let (mut s, _c) = mixed();
        let before = entry(&s).unwrap();
        let cap = flow::reserve(
            &s,
            before.serial,
            generation(&s).unwrap(),
            before.filter_choice,
        )
        .unwrap();
        assert_eq!(
            cap.proof()["rows"][1]["date"],
            json!({"active":true,"kind":"custom"})
        );
        let data = snapshot(&s, bad);
        assert!(flow::finish(&mut s, &cap, data).is_none());
        assert_eq!(entry(&s).unwrap().input, before.input);
        assert!(entry(&s).unwrap().intent.is_none());
        assert!(flow::release(&s, &cap));
    }
    let (mut s, _c) = mixed();
    let before = entry(&s).unwrap();
    let cap = flow::reserve(
        &s,
        before.serial,
        generation(&s).unwrap(),
        before.filter_choice,
    )
    .unwrap();
    let data = snapshot(&s, Some("2024-03-01"));
    assert!(flow::finish(&mut s, &cap, data).is_some());
    let Input::Filter(values) = entry(&s).unwrap().input else {
        panic!()
    };
    assert!(
        matches!(&values.conditions()[0],ViewFilterChoice::Number{value:Some(v),..} if v=="12.50")
    );
    assert!(
        matches!(&values.conditions()[1],ViewFilterChoice::Date{value:Some(v),..} if v=="2024-03-01")
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Preparing);
}
#[test]
fn preset_custom_operator_and_field_aba_rotate_authority_without_losing_invalid_input() {
    let (mut s, _c) = mixed();
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let old = e.filter_rows[1].clone();
    assert!(flow::update_value(
        &s,
        e.serial,
        visit,
        old.id,
        old.epoch,
        "invalid-calendar".into()
    ));
    assert!(!flow::update_value(
        &s,
        e.serial,
        visit,
        old.id,
        old.epoch,
        "today".into()
    ));
    let e = entry(&s).unwrap();
    let Input::Filter(v) = &e.input else { panic!() };
    assert!(!flow::valid(v));
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::DateOperator(old.id, old.epoch, Op::LessThan)
        )
        .is_some()
    );
    let e = entry(&s).unwrap();
    let Input::Filter(v) = &e.input else { panic!() };
    assert!(
        matches!(&v.conditions()[1],ViewFilterChoice::Date{value:Some(v),..} if v=="invalid-calendar")
    );
    assert!(!flow::update_value(
        &s,
        e.serial,
        visit,
        old.id,
        old.epoch,
        "2024-03-01".into()
    ));
    let row = e.filter_rows[1].clone();
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::DateValue(row.id, row.epoch, "next_week".into())
        )
        .is_some()
    );
    let e = entry(&s).unwrap();
    let row = e.filter_rows[1].clone();
    assert!(!flow::update_value(
        &s,
        e.serial,
        visit,
        row.id,
        row.epoch,
        "2024-03-01".into()
    ));
    let cap = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    assert_eq!(
        cap.proof()["rows"][1]["date"],
        json!({"active":true,"kind":"next_week"})
    );
    let bad = snapshot(&s, Some("today"));
    assert!(flow::finish(&mut s, &cap, bad).is_none());
    assert!(flow::release(&s, &cap));
    drop(cap);
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::DateValue(row.id, row.epoch, "custom".into())
        )
        .is_some()
    );
    let e = entry(&s).unwrap();
    let Input::Filter(v) = &e.input else { panic!() };
    assert!(matches!(
        &v.conditions()[1],
        ViewFilterChoice::Date {
            operator: Op::Equal,
            value: None,
            ..
        }
    ));
    assert!(
        !flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::DateValue(row.id, row.epoch, "next_week".into())
        )
        .is_some()
    );
    let row = e.filter_rows[1].clone();
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            flow::Action::DateOperator(row.id, row.epoch, Op::IsEmpty)
        )
        .is_some()
    );
    let e = entry(&s).unwrap();
    let cap = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    let data = snapshot(&s, None);
    assert!(flow::finish(&mut s, &cap, data).is_some());
}
#[test]
fn date_capture_cancel_and_owner_or_visit_changes_never_publish() {
    for case in 0..3 {
        let (mut s, _c) = mixed();
        let e = entry(&s).unwrap();
        let cap = flow::reserve(&s, e.serial, generation(&s).unwrap(), e.filter_choice).unwrap();
        let data = snapshot(&s, Some("2024-03-01"));
        if case == 0 {
            assert!(close_entry(&s, e.serial));
        } else if case == 1 {
            s.core.account_epoch += 1;
        } else {
            s.slack_lists.view.as_mut().unwrap().generation += 1;
        }
        assert!(flow::finish(&mut s, &cap, data).is_none());
    }
}

#[test]
fn date_ack_waits_for_exact_whole_metadata_readback_and_never_replays() {
    let (mut s, _c) = mixed();
    let before = metadata(&s, None).document;
    let serial = entry(&s).unwrap().serial;
    let cap = flow::reserve(
        &s,
        serial,
        generation(&s).unwrap(),
        entry(&s).unwrap().filter_choice,
    )
    .unwrap();
    let data = snapshot(&s, Some("2024-03-01"));
    let (g, i) = flow::finish(&mut s, &cap, data).unwrap();
    let Intent::Filter(filter) = &i else { panic!() };
    let expected = filter.filters().clone();
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(metadata(&s, None).document, before);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    let check = checking(&s);
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["views"][0]["filters"] = expected.clone();
    m.document.raw["list_metadata"]["views"][0]["filters"][1]["values"] = json!(["today"]);
    m.document = Document::parse(m.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(prepare(&mut s, serial).is_none());
    let check = checking(&s);
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["views"][0]["filters"] = expected;
    m.document = Document::parse(m.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert!(prepare(&mut s, serial).is_none());
    assert!(close_entry(&s, serial));
    open(&mut s);
    assert!(change(&s, ViewFilterChoice::None));
    let serial = entry(&s).unwrap().serial;
    let (_, i) = prepare(&mut s, serial).unwrap();
    let Intent::Filter(i) = i else { panic!() };
    assert_eq!(i.filters(), &json!([]));
}
