use super::*;
use NumberFilterOperator::*;

fn numeric(operator: NumberFilterOperator, value: Option<&str>) -> ViewFilterChoice {
    ViewFilterChoice::Number {
        column_id: "C_NUMBER".into(),
        operator,
        value: value.map(str::to_owned),
    }
}
fn observed(intent: &ViewFilter) -> Document {
    let mut raw = intent.original().raw.clone();
    raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
    raw["list_metadata"]["views"][0]["info_column_filters"] = json!([]);
    Document::parse(raw).unwrap()
}
#[test]
fn lexical_subset_is_bounded_finite_and_never_normalizes_input() {
    let exact_limit = format!("0.{}", "0".repeat(126));
    for value in [
        "0",
        "-0",
        "12.50",
        "-2.75",
        ".5",
        "-.5",
        "1e2",
        "1E-2",
        "1e+2",
        &exact_limit,
    ] {
        assert!(Equal.valid_value(Some(value)), "{value}");
        let intent = capture(&document(), numeric(Equal, Some(value)));
        assert_eq!(intent.filters()[0]["values"], json!([value]));
        assert_eq!(
            ViewFilter::current(&observed(&intent), "V1"),
            Some(numeric(Equal, Some(value)))
        );
    }
    for value in [
        "",
        " ",
        " 2",
        "2 ",
        "+2",
        "0x10",
        "NaN",
        "Infinity",
        "-",
        ".",
        "1.",
        "1e",
        "1e+",
        "1e309",
        "1_0",
        "１２",
        "2\n",
        &format!("{exact_limit}0"),
    ] {
        assert!(!Equal.valid_value(Some(value)), "{value:?}");
        assert!(
            ViewFilter::capture(
                &document(),
                "V1",
                numeric(Equal, Some(value)),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
    }
    assert!(!Equal.valid_value(None));
    assert!(IsEmpty.valid_value(None));
    assert!(!IsEmpty.valid_value(Some("")));
    assert!(!IsNotEmpty.valid_value(Some("0")));
    assert_eq!(NumberFilterOperator::parse("less_than_or_equal"), None);
    assert_eq!(NumberFilterOperator::parse("greater_than_or_equal"), None);
}
#[test]
fn six_operators_and_clear_replace_exact_arrays_preserving_other_metadata() {
    for operator in [Equal, NotEqual, LessThan, GreaterThan, IsEmpty, IsNotEmpty] {
        let value = operator.requires_value().then_some("12.50");
        let desired = numeric(operator, value);
        let intent = capture(&document(), desired.clone());
        assert_eq!(
            intent.filters(),
            &json!([{"key":"estimate","operator":operator.as_str(),"values":value.into_iter().collect::<Vec<_>>() }])
        );
        let current = observed(&intent);
        assert!(intent.observes_desired(&current));
        assert_eq!(ViewFilter::current(&current, "V1"), Some(desired.clone()));
        assert!(
            ViewFilter::capture(&current, "V1", desired, Some("full"), Some(&ordinary())).is_err()
        );
        let clear = capture(&current, ViewFilterChoice::None);
        assert_eq!(clear.filters(), &json!([]));
        assert!(clear.observes_desired(&document()));
        assert_eq!(intent.original(), &document());
        let checkbox = capture(&current, choice(true));
        assert!(checkbox.observes_desired(&filtered(true)));
    }
}
#[test]
fn numeric_raw_unknown_shapes_refuse_even_clear_without_mutating_them() {
    for rule in [
        json!({"key":"estimate","operator":"equal","values":[12.5]}),
        json!({"key":"estimate","operator":"equal","values":[]}),
        json!({"key":"estimate","operator":"equal","values":["2","3"]}),
        json!({"key":"estimate","operator":"equal","values":["2"],"column_id":"C_NUMBER"}),
        json!({"key":"estimate","operator":"equal","values":["2"],"opaque":true}),
        json!({"key":"estimate","operator":"less_than_or_equal","values":["2"]}),
        json!({"key":"estimate","operator":"is_empty","values":[""]}),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0]["filters"] = json!([rule]);
        let doc = Document::parse(raw.clone()).unwrap();
        assert_eq!(ViewFilter::current(&doc, "V1"), None);
        for desired in [
            ViewFilterChoice::None,
            numeric(Equal, Some("3")),
            choice(false),
        ] {
            assert!(
                ViewFilter::capture(&doc, "V1", desired, Some("full"), Some(&ordinary())).is_err()
            );
        }
        assert_eq!(doc.raw, raw);
    }
    for (key, value) in [
        ("filter_set", json!({"match":"all","groups":[]})),
        (
            "info_column_filters",
            json!([{"key":"created_by","operator":"is_empty","values":[]}]),
        ),
        (
            "filters",
            json!([{"key":"estimate","operator":"equal","values":["2"]},{"key":"estimate","operator":"is_empty","values":[]}]),
        ),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0][key] = value;
        let doc = Document::parse(raw.clone()).unwrap();
        assert!(
            ViewFilter::capture(
                &doc,
                "V1",
                numeric(Equal, Some("3")),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
        assert!(
            ViewFilter::capture(
                &doc,
                "V1",
                ViewFilterChoice::None,
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
        assert_eq!(doc.raw, raw);
    }
}
#[test]
fn loaded_mixed_rules_allow_complete_replacement_and_clear() {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["filters"] = json!([
        {"key":"estimate","operator":"equal","values":["2"]},
        {"key":"done","operator":"is_empty","values":[]}
    ]);
    let doc = Document::parse(raw.clone()).unwrap();
    assert_eq!(
        ViewFilter::current(&doc, "V1"),
        Some(ViewFilterChoice::All(vec![
            numeric(Equal, Some("2")),
            choice(false)
        ]))
    );
    for desired in [numeric(Equal, Some("3")), ViewFilterChoice::None] {
        let intent = capture(&doc, desired.clone());
        let expected = match desired {
            ViewFilterChoice::None => document(),
            _ => observed(&capture(&document(), desired.clone())),
        };
        assert_eq!(intent.filters(), &expected.views[0]["filters"]);
        assert_eq!(intent.info_column_filters(), json!([]));
        assert_eq!(observed(&intent), expected);
        assert!(intent.observes_desired(&expected));
        assert_eq!(ViewFilter::current(&expected, "V1"), Some(desired));
        assert_eq!(intent.original(), &doc);
    }
    assert_eq!(doc.raw, raw);
}
#[test]
fn number_precision_policy_and_choice_type_are_checked_independently() {
    for options in [
        Value::Null,
        json!({}),
        json!({"precision":6}),
        json!({"precision":-1}),
        json!({"precision":2,"opaque":true}),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][2]["options"] = options;
        let doc = Document::parse(raw).unwrap();
        assert!(!ViewFilter::supports_column(
            doc.column("C_NUMBER").unwrap()
        ));
        assert!(
            ViewFilter::capture(
                &doc,
                "V1",
                numeric(IsEmpty, None),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
    }
    for precision in 0..=5 {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][2]["options"] = json!({"precision":precision});
        assert!(ViewFilter::supports_column(
            Document::parse(raw).unwrap().column("C_NUMBER").unwrap()
        ));
    }
    for desired in [
        ViewFilterChoice::Checkbox {
            column_id: "C_NUMBER".into(),
            checked: true,
        },
        ViewFilterChoice::Number {
            column_id: "C_CHECK".into(),
            operator: IsEmpty,
            value: None,
        },
    ] {
        assert!(
            ViewFilter::capture(&document(), "V1", desired, Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    let mut raw = document().raw;
    raw["list_metadata"]["schema"][2]["type"] = json!("currency");
    assert!(
        ViewFilter::capture(
            &Document::parse(raw).unwrap(),
            "V1",
            numeric(Equal, Some("2")),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}
#[test]
fn numeric_admission_and_observation_retain_exact_lexical_and_schema_custody() {
    let doc = document();
    let intent = capture(&doc, numeric(Equal, Some("12.50")));
    for pointer in [
        "/list_metadata/schema/2/options/precision",
        "/list_metadata/schema/2/key",
        "/list_metadata/views/0/sorts/0/ascending",
        "/access",
    ] {
        let mut raw = doc.raw.clone();
        *raw.pointer_mut(pointer).unwrap() = json!("changed");
        assert!(
            intent
                .revalidate(
                    &Document::parse(raw).unwrap(),
                    Some("full"),
                    Some(&ordinary())
                )
                .is_err()
        );
    }
    assert!(intent.revalidate(&doc, None, Some(&ordinary())).is_err());
    assert!(intent.revalidate(&doc, Some("full"), None).is_err());
    let expected = observed(&intent);
    let mut attempt = ViewFilterAttempt::new(intent);
    attempt
        .admit(&doc, Some("full"), Some(&ordinary()))
        .unwrap();
    assert!(!attempt.observe(&expected));
    assert!(attempt.finish(false));
    assert!(
        attempt
            .admit(&doc, Some("full"), Some(&ordinary()))
            .is_err()
    );
    for value in [json!(12.5), json!("12.5"), json!("12.50 ")] {
        let mut raw = expected.raw.clone();
        raw["list_metadata"]["views"][0]["filters"][0]["values"][0] = value;
        assert!(!attempt.observe(&Document::parse(raw).unwrap()));
    }
    assert!(attempt.observe(&expected));
}
#[test]
fn number_filter_sort_and_find_preserve_raw_population_cursor_and_missing_semantics() {
    use crate::slack_lists::{
        RecordsPage,
        find::Query,
        saved_views::{self, ProjectionContext, SavedView},
    };
    let values = [
        Some(-2.0),
        Some(0.0),
        Some(2.0),
        Some(12.5),
        None,
        Some(2.0),
    ];
    let rows: Vec<_> = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| {
            let mut fields = vec![json!({"key":"name","text":if i==5{"Other"}else{"Keep"}})];
            if let Some(value) = value {
                fields.push(json!({"key":"estimate","number":[value]}));
            }
            json!({"id":format!("R{i}"),"position":i,"date_created":1,"fields":fields,"opaque":i})
        })
        .collect();
    let page = RecordsPage::parse(
        json!({"ok":true,"records":rows,"response_metadata":{"next_cursor":"owned-next"}}),
    )
    .unwrap();
    let untouched = page.clone();
    let context = ProjectionContext {
        find: Some(Query {
            text: "Keep".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    for (op, value, expected) in [
        (Equal, Some("0"), vec!["R1"]),
        (NotEqual, Some("0"), vec!["R0", "R2", "R3"]),
        (LessThan, Some("2"), vec!["R0", "R1"]),
        (GreaterThan, Some("0"), vec!["R2", "R3"]),
        (IsEmpty, None, vec!["R4"]),
        (IsNotEmpty, None, vec!["R0", "R1", "R2", "R3"]),
    ] {
        let doc = observed(&capture(&document(), numeric(op, value)));
        let view = SavedView::parse(doc.views[0].clone()).unwrap();
        let projected = saved_views::project(&doc, &page, &view, &context).unwrap();
        let ids: Vec<_> = projected
            .groups
            .iter()
            .flat_map(|g| g.rows.iter().map(|r| r.record_id.as_str()))
            .collect();
        assert_eq!(ids, expected);
        assert!(projected.incomplete);
        assert_eq!(page, untouched);
        assert_eq!(context.find.as_ref().unwrap().text, "Keep");
    }
}
