use super::*;
use serde_json::json;

fn original() -> Value {
    json!({"id":"F1","title":"Release notes","mode":"snippet","filetype":"text",
        "mimetype":"text/plain","created":1700000000,"contents_extracts":["release candidate"]})
}
fn parts(raw: &Value) -> Option<Vec<(String, bool)>> {
    Some(
        from_original_item(raw)?
            .parts()
            .iter()
            .map(|p| (p.text().to_owned(), p.highlighted()))
            .collect(),
    )
}
fn first(text: &str) -> Value {
    let mut raw = original();
    raw["contents_extracts"] = json!([text]);
    raw
}

#[test]
fn original_guard_knownness_and_literal_excerpt_corpus_preserve_normal_page_data() {
    // Missing policy operands are used only by the exact incoming operations.
    // No visible/zero/isUnknown field is synthesized or required on this item.
    assert_eq!(
        parts(&original()),
        Some(vec![("release candidate".into(), false)])
    );
    for (access, restriction) in [(Value::Null, Value::Null), (json!("visible"), json!(0))] {
        let mut raw = original();
        raw["file_access"] = access;
        raw["restriction_type"] = restriction;
        for flag in [
            "isNonExistent",
            "is_hidden_by_limit",
            "is_deleted",
            "is_tombstoned",
        ] {
            raw[flag] = json!(false);
        }
        assert!(from_original_item(&raw).is_some());
    }
    let mut raw = original();
    raw["isUnknown"] = json!(true);
    assert!(
        from_original_item(&raw).is_some(),
        "positive constructor overlay, not raw flag absence"
    );
    raw["isNonExistent"] = json!(true);
    assert!(
        from_original_item(&raw).is_none(),
        "raw entry rejects before the timestamp fold"
    );

    for mode in [
        "tombstone",
        "not_found",
        "hidden_by_limit",
        "access_denied",
        "canvas_slack_connect_blocked",
        "lists_slack_connect_blocked",
        "not_visible",
        "template_not_visible",
    ] {
        let mut raw = original();
        raw["mode"] = json!(mode);
        assert!(from_original_item(&raw).is_none(), "{mode}");
    }
    for access in [
        "file_not_found",
        "access_denied",
        "not_visible",
        "template_not_visible",
        "canvas_globally_disabled",
        "slack_connect_team_blocked",
        "slack_connect_canvas_sharing_blocked",
        "canvas_disabled_user_team",
        "canvas_disabled_file_team",
        "slack_connect_lists_team_blocked",
        "slack_connect_lists_sharing_blocked",
    ] {
        let mut raw = original();
        raw["file_access"] = json!(access);
        assert!(from_original_item(&raw).is_none(), "{access}");
    }
    for key in [
        "isNonExistent",
        "is_hidden_by_limit",
        "is_deleted",
        "is_tombstoned",
    ] {
        for bad in [json!(true), json!("false"), json!([]), json!({}), json!(0)] {
            let mut raw = original();
            raw[key] = bad;
            assert!(
                from_original_item(&raw).is_none(),
                "unsupported/denied {key}"
            );
        }
    }
    for bad in [
        json!(1),
        json!(2),
        json!(-1),
        json!("0"),
        json!("1"),
        json!(false),
        json!([]),
        json!({}),
    ] {
        let mut raw = original();
        raw["restriction_type"] = bad;
        assert!(from_original_item(&raw).is_none());
    }
    for bad in [
        json!(""),
        json!("unknown-policy"),
        json!(false),
        json!(0),
        json!([]),
        json!({}),
    ] {
        let mut raw = original();
        raw["file_access"] = bad;
        assert!(from_original_item(&raw).is_none());
    }

    // VP selects the first truthy operand, not the first valid numeric operand.
    for bad in [
        json!("0"),
        json!("bad"),
        json!(true),
        json!([]),
        json!({}),
        json!(-1),
        json!(1e300),
    ] {
        let mut raw = original();
        raw["updated"] = bad;
        assert!(
            from_original_item(&raw).is_none(),
            "never skip a truthy malformed updated"
        );
    }
    for skipped in [json!(0), json!(-0.0), Value::Null, json!(false), json!("")] {
        let mut raw = original();
        raw["updated"] = skipped;
        assert!(from_original_item(&raw).is_some());
    }
    let mut raw = original();
    raw.as_object_mut().unwrap().remove("created");
    raw["isUnknown"] = json!(false);
    assert!(
        from_original_item(&raw).is_none(),
        "raw marker/id alone is not knownness"
    );
    raw["created"] = json!(0);
    raw["timestamp"] = json!(1700000001);
    assert!(from_original_item(&raw).is_some());
    raw["created"] = json!(-1);
    assert!(
        from_original_item(&raw).is_none(),
        "selected negative created blocks timestamp fallback"
    );
    raw["created"] = json!(MAX_TIMESTAMP + 1.0);
    assert!(from_original_item(&raw).is_none());
    raw["created"] = json!(0.5);
    assert!(
        from_original_item(&raw).is_some(),
        "positive bounded numeric Source truthiness"
    );

    for (key, value) in [
        ("id", json!("bad_id")),
        ("mode", json!("hosted")),
        ("filetype", json!("quip")),
        ("mimetype", json!("text/html")),
    ] {
        let mut raw = original();
        raw[key] = value;
        assert!(from_original_item(&raw).is_none());
    }
    for value in [
        Value::Null,
        json!([]),
        json!([null]),
        json!([false]),
        json!([0]),
        json!({"0":"not an array"}),
        json!(["", "no later fallback"]),
        json!(["has:colon", "plain"]),
    ] {
        let mut raw = original();
        raw["contents_extracts"] = value;
        assert!(from_original_item(&raw).is_none());
    }
    let mut raw = original();
    raw.as_object_mut().unwrap().remove("contents_extracts");
    raw["preview_highlight"] = json!("<pre>not an excerpt fallback</pre>");
    assert!(from_original_item(&raw).is_none());

    assert_eq!(
        parts(&first(
            "\t \u{feff}\u{a0}literal &amp; <img src=x> <@U1> *text* ✅"
        )),
        Some(vec![(
            "literal &amp; <img src=x> <@U1> *text* ✅".into(),
            false
        )])
    );
    assert_eq!(
        parts(&first("\u{feff}\u{85}  keep")),
        Some(vec![("\u{85}  keep".into(), false)])
    );
    assert_eq!(
        parts(&first("\u{180e}keep")),
        Some(vec![("\u{180e}keep".into(), false)])
    );
    assert!(from_original_item(&first("\u{feff}\u{2028}\u{2029}\u{3000}")).is_none());
    assert_eq!(
        parts(&first(
            "A \u{e000}one\u{e000}two\u{e001} Z\u{e001} end\u{e000}"
        )),
        Some(vec![
            ("A ".into(), false),
            ("one\u{e000}two".into(), true),
            (" Z\u{e001} end\u{e000}".into(), false)
        ])
    );
    assert_eq!(
        parts(&first("\u{e000}\u{e001}tail")),
        Some(vec![("".into(), true), ("tail".into(), false)])
    );
    assert_eq!(
        parts(&first("🙂 \u{e000}release✅\u{e001}\nkeep  spaces")),
        Some(vec![
            ("🙂 ".into(), false),
            ("release✅".into(), true),
            ("\nkeep  spaces".into(), false)
        ])
    );

    assert!(from_original_item(&first(&"a".repeat(MAX_UTF16_UNITS))).is_some());
    assert!(from_original_item(&first(&"a".repeat(MAX_UTF16_UNITS + 1))).is_none());
    assert!(from_original_item(&first(&"🙂".repeat(MAX_UTF16_UNITS / 2))).is_some());
    assert!(from_original_item(&first(&"🙂".repeat(MAX_UTF16_UNITS / 2 + 1))).is_none());
    assert!(from_original_item(&first(&"a".repeat(MAX_BYTES + 1))).is_none());
    let dense = first(&"\u{e000}\u{e001}".repeat(MAX_PARTS));
    let dense = from_original_item(&dense).unwrap();
    assert_eq!(dense.parts().len(), MAX_PARTS);
    assert!(
        dense
            .parts()
            .iter()
            .all(|p| p.highlighted() && p.text().is_empty())
    );
    assert!(from_original_item(&first(&"\u{e000}\u{e001}".repeat(MAX_PARTS + 1))).is_none());

    // The normal decoder keeps returned occurrences and old metadata even when
    // optional excerpt projection refuses a row. No File/model/IO is produced.
    let mut second = first("second occurrence");
    second["title"] = json!("Same id, second row");
    let mut denied = original();
    denied["id"] = json!("F2");
    denied["is_deleted"] = json!(true);
    let mut malformed = original();
    malformed["id"] = json!("F3");
    malformed["contents_extracts"] = json!({});
    let page = super::super::Page::decode(
        json!({"ok":true,
        "pagination":{"page":1,"page_count":1,"total_count":4},
        "items":[first("first occurrence"),second,denied,malformed]}),
        1,
    )
    .unwrap();
    assert_eq!(page.hits().len(), 4);
    assert_eq!(page.hits()[0].id(), page.hits()[1].id());
    assert_eq!(
        page.hits()[0].excerpt().unwrap().parts()[0].text(),
        "first occurrence"
    );
    assert_eq!(
        page.hits()[1].excerpt().unwrap().parts()[0].text(),
        "second occurrence"
    );
    assert_eq!(page.hits()[1].title(), "Same id, second row");
    assert!(page.hits()[2].excerpt().is_none() && page.hits()[3].excerpt().is_none());
    assert_eq!(page.number(), 1);
    assert_eq!(page.page_count(), 1);
    assert_eq!(page.total(), 4);
}
