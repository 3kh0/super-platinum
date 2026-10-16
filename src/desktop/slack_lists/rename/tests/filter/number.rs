use super::*;
use crate::slack_lists::rename::filter as flow;
use super_platinum_core::slack_lists::NumberFilterOperator as Op;
fn number_shell() -> (ShellState, Cleanup) {
    let (mut s, cleanup) = filter_shell();
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    for c in loaded.document.raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
    {
        if c["id"] == "C_N" {
            c["type"] = json!("number");
            c["options"] = json!({"precision":2});
        }
    }
    loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
    rebuild_rows(&mut s);
    open(&mut s);
    (s, cleanup)
}
fn number(op: Op, value: Option<&str>) -> ViewFilterChoice {
    ViewFilterChoice::Number {
        column_id: "C_N".into(),
        operator: op,
        value: value.map(str::to_owned),
    }
}
fn input(s: &ShellState) -> ViewFilterChoice {
    let Input::Filter(value) = entry(s).unwrap().input else {
        panic!()
    };
    value
}
#[test]
fn consecutive_same_render_values_do_not_lose_latest_edit_or_rotate_epoch() {
    let (s, _c) = number_shell();
    assert!(change(&s, number(Op::Equal, None)));
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let rendered = input(&s);
    for v in ["1", "12", "12.", "12.50", "1e", "1e2"] {
        assert!(flow::value_change(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            &rendered,
            v.into()
        ));
        assert_eq!(entry(&s).unwrap().filter_choice, e.filter_choice);
        assert_eq!(input(&s), number(Op::Equal, Some(v)));
    }
    // An operator callback captured before those events preserves the latest text.
    assert!(flow::operator_change(
        &s,
        e.serial,
        visit,
        e.filter_choice,
        &rendered,
        Op::NotEqual
    ));
    assert_eq!(input(&s), number(Op::NotEqual, Some("1e2")));
}
#[test]
fn field_and_operator_aba_refuse_old_values_and_old_conditions() {
    let (s, _c) = number_shell();
    assert!(change(&s, number(Op::Equal, Some("-0"))));
    let old = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let rendered = input(&s);
    assert!(flow::operator_change(
        &s,
        old.serial,
        visit,
        old.filter_choice,
        &rendered,
        Op::LessThan
    ));
    let e = entry(&s).unwrap();
    assert!(flow::operator_change(
        &s,
        e.serial,
        visit,
        e.filter_choice,
        &input(&s),
        Op::Equal
    ));
    assert!(!flow::value_change(
        &s,
        old.serial,
        visit,
        old.filter_choice,
        &rendered,
        "99".into()
    ));
    assert!(!flow::operator_change(
        &s,
        old.serial,
        visit,
        old.filter_choice,
        &rendered,
        Op::GreaterThan
    ));
    assert!(change(
        &s,
        ViewFilterChoice::Checkbox {
            column_id: "C_OTHER".into(),
            checked: true
        }
    ));
    assert!(change(&s, number(Op::Equal, Some("-0"))));
    assert!(!flow::value_change(
        &s,
        old.serial,
        visit,
        old.filter_choice,
        &rendered,
        "99".into()
    ));
    assert_eq!(input(&s), rendered);
}
#[test]
fn empty_operators_drop_value_and_return_to_comparison_requires_new_input() {
    let (mut s, _c) = number_shell();
    assert!(change(&s, number(Op::Equal, Some("12.50"))));
    for op in [Op::IsEmpty, Op::IsNotEmpty, Op::GreaterThan] {
        let e = entry(&s).unwrap();
        let v = generation(&s).unwrap();
        assert!(flow::operator_change(
            &s,
            e.serial,
            v,
            e.filter_choice,
            &input(&s),
            op
        ));
        assert_eq!(input(&s), number(op, None));
    }
    let serial = entry(&s).unwrap().serial;
    assert!(prepare(&mut s, serial).is_none());
    assert_eq!(entry(&s).unwrap().stage, Stage::Editing);
}
#[test]
fn invalid_drafts_and_native_invalidity_never_claim_a_job() {
    let (mut s, _c) = number_shell();
    assert!(change(&s, number(Op::Equal, None)));
    let e = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let rendered = input(&s);
    for value in ["", "1e", "NaN", "Infinity", " 2", "1e999", "12."] {
        assert!(flow::value_change(
            &s,
            e.serial,
            visit,
            e.filter_choice,
            &rendered,
            value.into()
        ));
        assert!(prepare(&mut s, e.serial).is_none());
        assert_eq!(input(&s), number(Op::Equal, Some(value)));
    }
    assert!(
        flow::prepare_number(
            &mut s,
            e.serial,
            visit,
            e.filter_choice,
            &rendered,
            "12.50".into(),
            false
        )
        .is_none()
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Editing);
    assert!(entry(&s).unwrap().job.is_none());
    let (_, intent) = flow::prepare_number(
        &mut s,
        e.serial,
        visit,
        e.filter_choice,
        &rendered,
        "12.50".into(),
        true,
    )
    .unwrap();
    let Intent::Filter(intent) = intent else {
        panic!()
    };
    assert_eq!(
        intent.filters(),
        &json!([{"key":"estimate","operator":"equal","values":["12.50"]}])
    );
    assert!(!flow::value_change(
        &s,
        e.serial,
        visit,
        e.filter_choice,
        &rendered,
        "9".into()
    ));
    assert!(prepare(&mut s, e.serial).is_none());
}
#[test]
fn native_snapshot_stale_epoch_owner_selection_and_source_refuse() {
    for mode in 0..4 {
        let (mut s, _c) = number_shell();
        assert!(change(&s, number(Op::Equal, Some("1"))));
        let e = entry(&s).unwrap();
        let visit = generation(&s).unwrap();
        let rendered = input(&s);
        match mode {
            0 => {
                assert!(flow::operator_change(
                    &s,
                    e.serial,
                    visit,
                    e.filter_choice,
                    &rendered,
                    Op::NotEqual
                ));
            }
            1 => s.core.active_account = Some("different".into()),
            2 => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            _ => {
                let l = s
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                l.document.raw["list_metadata"]["other"] = json!(1);
                l.document = Document::parse(l.document.raw.clone()).unwrap();
            }
        }
        assert!(
            flow::prepare_number(
                &mut s,
                e.serial,
                visit,
                e.filter_choice,
                &rendered,
                "2".into(),
                true
            )
            .is_none()
        );
    }
}
#[test]
fn number_reply_never_projects_and_lexical_readback_conflict_never_replays() {
    let (mut s, _c) = number_shell();
    assert!(change(&s, number(Op::Equal, Some("12.50"))));
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let serial = entry(&s).unwrap().serial;
    let (g, i) = prepare(&mut s, serial).unwrap();
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(metadata(&s, None).document, before.document);
    let check = checking(&s);
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"estimate","operator":"equal","values":["12.5"]}]);
    m.document = Document::parse(m.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(prepare(&mut s, serial).is_none());
    let check = checking(&s);
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"estimate","operator":"equal","values":["12.50"]}]);
    m.document = Document::parse(m.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before.records
    );
    let mut restored = metadata(&s, None).document.raw;
    restored["list_metadata"]["views"][0]["filters"] = json!([]);
    assert_eq!(restored, before.document.raw);
}

#[test]
fn observed_number_conditions_compose_sort_find_and_preserve_raw_population() {
    let mut s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-number-filter",
    );
    let _cleanup = Cleanup(Key::current(&s).unwrap());
    let v = s.slack_lists.view.as_mut().unwrap();
    let l = v.loaded.as_mut().unwrap();
    let mut raw = l.records.raw.clone();
    for i in 0..7 {
        raw["records"][i]["fields"][0]["text"] = json!(format!("Number cohort {i}"));
    }
    l.records = RecordsPage::parse(raw).unwrap();
    v.layout.find.input = "Number cohort".into();
    v.layout.find.applied = "Number cohort".into();
    v.layout.find.revision = 81;
    rebuild_rows(&mut s);
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let cases = [
        (Op::Equal, Some("12.50"), vec![0, 5]),
        (Op::NotEqual, Some("12.50"), vec![2, 3, 1, 4]),
        (Op::LessThan, Some("0"), vec![2]),
        (Op::GreaterThan, Some("0"), vec![1, 4, 0, 5]),
        (Op::IsEmpty, None, vec![6]),
        (Op::IsNotEmpty, None, vec![2, 3, 1, 4, 0, 5]),
    ];
    for (op, value, order) in cases {
        let desired = ViewFilterChoice::Number {
            column_id: "C_NUMBER".into(),
            operator: op,
            value: value.map(str::to_owned),
        };
        if input(&s) != desired {
            assert!(change(&s, desired));
        }
        let serial = entry(&s).unwrap().serial;
        let (g, i) = prepare(&mut s, serial).unwrap();
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let check = checking(&s);
        let mut m = metadata(&s, None);
        let Intent::Filter(i) = i else { panic!() };
        m.document.raw["list_metadata"]["views"][0]["filters"] = i.filters().clone();
        m.document.raw["list_metadata"]["views"][0]["info_column_filters"] =
            i.info_column_filters();
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
        assert_eq!(
            ids,
            order
                .into_iter()
                .map(|i| format!("R_LIST_{i:03}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
        let mut restored = v.loaded.as_ref().unwrap().document.raw.clone();
        restored["list_metadata"]["views"][0]["filters"] = json!([]);
        assert_eq!(restored, before.document.raw);
        assert_eq!(
            (
                &v.layout.find.input,
                &v.layout.find.applied,
                v.layout.find.revision
            ),
            (&"Number cohort".to_owned(), &"Number cohort".to_owned(), 81)
        );
        assert!(close_entry(&s, serial));
        open(&mut s);
    }
    assert!(change(&s, ViewFilterChoice::None));
    let serial = entry(&s).unwrap().serial;
    let (_, i) = prepare(&mut s, serial).unwrap();
    let Intent::Filter(i) = i else { panic!() };
    assert_eq!(i.filters(), &json!([]));
    assert_eq!(i.info_column_filters(), json!([]));
}
