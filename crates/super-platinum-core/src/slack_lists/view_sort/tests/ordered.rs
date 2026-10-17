use super::*;
fn second(ascending: bool) -> ViewSortChoice {
    ViewSortChoice::Number {
        column_id: "C_OTHER".into(),
        ascending,
    }
}
fn ordered() -> ViewSortChoice {
    ViewSortChoice::from_rules(vec![choice(true), second(false)])
}
fn observed(doc: &Document, desired: &ViewSortChoice) -> Document {
    let intent = capture(doc, desired.clone());
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["views"][0]["sorts"] = intent.sorts().clone();
    Document::parse(raw).unwrap()
}
#[test]
fn ordered_replacement_reorder_remove_and_clear_preserve_all_other_metadata() {
    let original = document();
    let before = original.raw.clone();
    let desired = ordered();
    let intent = capture(&original, desired.clone());
    assert_eq!(
        intent.sorts(),
        &json!([{"key":"estimate","ascending":true},{"key":"second","ascending":false}])
    );
    let fresh = observed(&original, &desired);
    assert_eq!(ViewSort::current(&fresh, "V1"), Some(desired));
    assert!(intent.observes_desired(&fresh));
    let mut reversed = fresh.raw.clone();
    reversed["list_metadata"]["views"][0]["sorts"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reversed = Document::parse(reversed).unwrap();
    assert!(!intent.observes_desired(&reversed));
    assert!(
        capture(
            &fresh,
            ViewSortChoice::from_rules(vec![second(false), choice(true)])
        )
        .observes_desired(&reversed)
    );
    let mut unrelated = fresh.raw.clone();
    unrelated["list_metadata"]["views"][0]["filters"] = json!([{"opaque":1}]);
    assert!(!intent.observes_desired(&Document::parse(unrelated).unwrap()));
    let removed = observed(&fresh, &second(false));
    assert_eq!(ViewSort::current(&removed, "V1"), Some(second(false)));
    let cleared = observed(&removed, &ViewSortChoice::None);
    assert_eq!(cleared, original);
    assert_eq!(original.raw, before);
}
#[test]
fn nested_empty_duplicate_unknown_and_aliased_rules_refuse_before_capture_even_clear() {
    for desired in [
        ViewSortChoice::Ordered(vec![]),
        ViewSortChoice::Ordered(vec![choice(true)]),
        ViewSortChoice::Ordered(vec![choice(true), ViewSortChoice::None]),
        ViewSortChoice::Ordered(vec![choice(true), ordered()]),
        ViewSortChoice::Ordered(vec![choice(true), choice(false)]),
    ] {
        assert!(!ViewSort::valid_choice(&document(), &desired));
        assert!(
            ViewSort::capture(&document(), "V1", desired, Some("full"), Some(&ordinary())).is_err()
        );
    }
    for raw_rules in [
        json!([{"key":"estimate","ascending":true},{"key":"estimate","ascending":false}]),
        json!([{"key":"estimate","ascending":true},{"key":"deleted","ascending":true}]),
        json!([{"key":"estimate","ascending":true},{"key":"second","ascending":true,"column_id":"C_OTHER"}]),
        json!([{"key":"estimate","ascending":true},null]),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0]["sorts"] = raw_rules;
        let doc = Document::parse(raw.clone()).unwrap();
        assert!(ViewSort::current(&doc, "V1").is_none());
        assert!(
            ViewSort::capture(
                &doc,
                "V1",
                ViewSortChoice::None,
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
        assert_eq!(doc.raw, raw);
    }
}
#[test]
fn unique_rules_are_limited_by_existing_wire_bytes_not_an_invented_count_of_eight() {
    let mut raw = document().raw;
    let schema = raw["list_metadata"]["schema"].as_array_mut().unwrap();
    for i in 0..12 {
        schema.push(
            json!({"id":format!("C{i}"),"key":format!("key{i}"),"name":"Text","type":"text"}),
        );
    }
    let doc = Document::parse(raw).unwrap();
    let desired = ViewSortChoice::from_rules(
        (0..12)
            .map(|i| ViewSort::choice(doc.column(&format!("C{i}")).unwrap(), true).unwrap())
            .collect(),
    );
    assert_eq!(
        capture(&doc, desired.clone())
            .sorts()
            .as_array()
            .unwrap()
            .len(),
        12
    );
    let mut raw = doc.raw;
    for (i, c) in raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .skip(3)
        .enumerate()
    {
        c["key"] = json!(format!("{i}{}", "x".repeat(900)));
    }
    let doc = Document::parse(raw).unwrap();
    assert!(!ViewSort::valid_choice(&doc, &desired));
    let rules = doc
        .columns
        .iter()
        .skip(3)
        .map(|c| json!({"key":c.key,"ascending":true}))
        .collect::<Vec<_>>();
    let mut raw = doc.raw;
    raw["list_metadata"]["views"][0]["sorts"] = json!(rules);
    let doc = Document::parse(raw).unwrap();
    assert!(ViewSort::current(&doc, "V1").is_none());
    assert!(
        ViewSort::capture(
            &doc,
            "V1",
            ViewSortChoice::None,
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}
#[test]
fn mixed_direction_priority_is_lexicographic_with_stable_final_ties_and_missing_values() {
    use crate::slack_lists::{
        RecordsPage,
        saved_views::{ProjectionContext, SavedView, project},
    };
    let page=RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"A","position":1,"fields":[{"key":"estimate","number":[1]},{"key":"second","number":[2]}]},
        {"id":"B","position":2,"fields":[{"key":"estimate","number":[1]},{"key":"second","number":[3]}]},
        {"id":"C","position":3,"fields":[{"key":"estimate","number":[1]},{"key":"second","number":[3]}]},
        {"id":"D","position":4,"fields":[{"key":"estimate","number":[0]},{"key":"second","number":[1]}]},
        {"id":"E","position":5,"fields":[{"key":"second","number":[4]}]}
    ],"response_metadata":{"next_cursor":"unloaded+/="}})).unwrap();
    let before = page.clone();
    for (desired, expected) in [
        (ordered(), vec!["D", "B", "C", "A", "E"]),
        (
            ViewSortChoice::from_rules(vec![second(false), choice(true)]),
            vec!["E", "B", "C", "A", "D"],
        ),
    ] {
        let doc = observed(&document(), &desired);
        let view = SavedView::parse(doc.views[0].clone()).unwrap();
        let p = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
        assert_eq!(
            p.groups
                .iter()
                .flat_map(|g| g.rows.iter().map(|r| r.record_id.as_str()))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(p.incomplete);
        assert_eq!(page, before);
    }
}
#[test]
fn ordered_attempt_retains_schema_and_one_admission_after_lost_response() {
    let doc = document();
    let mut attempt = ViewSortAttempt::new(capture(&doc, ordered()));
    assert!(attempt.admit(&doc, Some("full"), Some(&ordinary())).is_ok());
    assert!(attempt.finish(false));
    assert!(
        attempt
            .admit(&doc, Some("full"), Some(&ordinary()))
            .is_err()
    );
    assert!(!attempt.observe(&doc));
    let desired = observed(&doc, &ordered());
    let mut changed = desired.raw.clone();
    changed["list_metadata"]["views"][1]["opaque"] = json!(true);
    assert!(!attempt.observe(&Document::parse(changed).unwrap()));
    assert!(attempt.observe(&desired));
}
