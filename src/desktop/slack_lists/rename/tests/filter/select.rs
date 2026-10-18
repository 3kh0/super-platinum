use super::*;
use crate::slack_lists::rename::filter as flow;
use super_platinum_core::slack_lists::{NumberFilterOperator as Num, SelectFilterOperator as Op};
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
            column["type"] = json!("select");
            column["options"] = json!({"format":"multi_select","choices":[{"value":"a","label":"Same"},{"value":"b","label":"Same"}]});
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
            ViewFilterChoice::Select {
                column_id: "C_OTHER".into(),
                operator: Op::Contains,
                values: vec!["b".into(), "a".into()]
            }
        ])
    ));
    (s, c)
}
fn snapshot(s: &ShellState) -> Vec<flow::Snapshot> {
    let e = entry(s).unwrap();
    e.filter_rows
        .iter()
        .enumerate()
        .map(|(i, r)| flow::Snapshot {
            id: r.id.to_string(),
            epoch: r.epoch.to_string(),
            column: if i == 0 { "C_N" } else { "C_OTHER" }.into(),
            operator: if i == 0 { "less_than" } else { "contains" }.into(),
            value: Some(if i == 0 { "12.50" } else { r#"["b","a"]"# }.into()),
            valid: true,
        })
        .collect()
}
#[test]
fn select_snapshot_refuses_late_inconsistent_values_without_partial_number_publication() {
    for bad in [
        None,
        Some("null"),
        Some(r#"["a","b"]"#),
        Some(r#"["b","b"]"#),
        Some(r#"["unknown"]"#),
    ] {
        let (mut s, _c) = mixed();
        let e = entry(&s).unwrap();
        let visit = generation(&s).unwrap();
        let capture = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
        assert_eq!(
            capture.proof()["rows"][1]["select"],
            json!({"choices":["a","b"],"values":["b","a"],"active":true})
        );
        let mut data = snapshot(&s);
        data[1].value = bad.map(str::to_owned);
        assert!(flow::finish(&mut s, &capture, data).is_none());
        assert_eq!(entry(&s).unwrap().input, e.input);
        assert!(entry(&s).unwrap().intent.is_none());
        assert!(flow::release(&s, &capture));
    }
    let (mut s, _c) = mixed();
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let capture = flow::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    let data = snapshot(&s);
    assert!(flow::finish(&mut s, &capture, data).is_some());
    assert_eq!(entry(&s).unwrap().stage, Stage::Preparing);
}
#[test]
fn select_toggle_and_operator_epochs_refuse_aba_and_capture_cancel_staleness() {
    let (mut s, _c) = mixed();
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let row = e.filter_rows[1].clone();
    assert!(flow::update_select(
        &s, e.serial, visit, row.id, row.epoch, "b", false
    ));
    assert!(!flow::update_select(
        &s, e.serial, visit, row.id, row.epoch, "b", true
    ));
    let next = entry(&s).unwrap();
    let r = &next.filter_rows[1];
    assert!(flow::update_select(
        &s, e.serial, visit, r.id, r.epoch, "b", true
    ));
    let next = entry(&s).unwrap();
    let Input::Filter(v) = &next.input else {
        panic!()
    };
    assert!(
        matches!(&v.conditions()[1],ViewFilterChoice::Select{values,..} if values==&vec!["a".to_owned(),"b".to_owned()])
    );
    let r = &next.filter_rows[1];
    assert!(
        flow::structural(
            &s,
            e.serial,
            visit,
            next.filter_choice,
            flow::Action::SelectOperator(r.id, r.epoch, Op::IsEmpty)
        )
        .is_some()
    );
    let empty = entry(&s).unwrap();
    let Input::Filter(v) = &empty.input else {
        panic!()
    };
    assert!(
        matches!(&v.conditions()[1],ViewFilterChoice::Select{operator:Op::IsEmpty,values,..} if values.is_empty())
    );
    let cap = flow::reserve(&s, e.serial, visit, empty.filter_choice).unwrap();
    let r = &empty.filter_rows[1];
    assert!(!flow::update_select(
        &s, e.serial, visit, r.id, r.epoch, "a", true
    ));
    assert!(close_entry(&s, e.serial));
    assert!(flow::finish(&mut s, &cap, vec![]).is_none());
}
