use super::*;
use serde_json::json;

fn column(kind: &str, options: Value) -> Column {
    Column {
        id: "COLUMN".into(),
        key: "field".into(),
        name: "Field".into(),
        kind: kind.into(),
        raw: json!({"id":"COLUMN","key":"field","name":"Field","type":kind,"options":options}),
    }
}
fn record(field: Value) -> Record {
    Record::parse(json!({"id":"RECORD","fields":[field]})).unwrap()
}

#[test]
fn select_labels_follow_schema_order_and_keep_unknown_or_ambiguous_ids() {
    let column = column(
        "multi_select",
        json!({"choices":[
            {"value":"b","label":"Beta","color":"blue"},
            {"value":"a","label":"<literal & label>"},
            {"value":"duplicate","label":"One"},
            {"value":"duplicate","label":"Two"}
        ]}),
    );
    let record = record(
        json!({"key":"field","select":["a","b","unknown","duplicate","a"],"unknown":{"keep":true}}),
    );
    let raw = record.raw.clone();
    let CellPresentation::Select(labels) = column.presentation(&record) else {
        panic!()
    };
    assert_eq!(
        labels
            .iter()
            .map(|label| label.label.as_str())
            .collect::<Vec<_>>(),
        ["Beta", "<literal & label>", "unknown", "duplicate"]
    );
    assert_eq!(labels[0].color.as_deref(), Some("blue"));
    assert_eq!(labels[3].color, None);
    assert_eq!(
        column.display_text(&record),
        "Beta, <literal & label>, unknown, duplicate"
    );
    assert_eq!(record.raw, raw);
}

#[test]
fn rich_text_preserves_blocks_and_has_useful_plain_fallback() {
    let column = column("text", Value::Null);
    let blocks = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Launch ","style":{"bold":true}},
        {"type":"link","text":"notes","url":"https://example.test"},
        {"type":"text","text":" with "},{"type":"user","user_id":"U1"}
    ]}]}]);
    let record = record(json!({"key":"field","rich_text":blocks}));
    let CellPresentation::RichText {
        blocks: projected,
        fallback,
    } = column.presentation(&record)
    else {
        panic!()
    };
    assert_eq!(projected, blocks.as_array().unwrap().clone());
    assert_eq!(fallback, "Launch notes with @U1");
    assert_eq!(column.display_text(&record), fallback);
    let record = record_with_fallback();
    assert_eq!(column.display_text(&record), "Server fallback");
}
fn record_with_fallback() -> Record {
    record(json!({"key":"field","richText":[],"text":"Server fallback"}))
}

#[test]
fn reference_kinds_keep_array_identity_without_splitting_delimiters() {
    for (kind, member, expected) in [
        ("user", "user", ReferenceKind::User),
        ("channel", "channel", ReferenceKind::Channel),
        ("canvas", "attachment", ReferenceKind::Attachment),
    ] {
        let column = column(kind, Value::Null);
        let record = record(
            json!({"key":"field",(member):["first,one","second|two"],"text":"Existing names"}),
        );
        let CellPresentation::References {
            kind,
            ids,
            fallback,
        } = column.presentation(&record)
        else {
            panic!()
        };
        assert_eq!(kind, expected);
        assert_eq!(ids, ["first,one", "second|two"]);
        assert_eq!(fallback, "Existing names");
    }
}

#[test]
fn links_honor_display_as_url_and_do_not_claim_destination_is_safe() {
    let column = column("link", Value::Null);
    for (display_as_url, expected) in [
        (false, "Readable name"),
        (true, "https://example.test/path"),
    ] {
        let record = record(
            json!({"key":"field","link":[{"original_url":"https://example.test/path","display_name":"Readable name","display_as_url":display_as_url}]}),
        );
        assert_eq!(column.display_text(&record), expected);
    }
    let record = record(
        json!({"key":"field","link":[{"original_url":"javascript:alert(1)","display_name":"Literal name"}]}),
    );
    assert!(
        matches!(column.presentation(&record), CellPresentation::Link { url, label } if url == "javascript:alert(1)" && label == "Literal name")
    );
}
