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

#[test]
fn link_readback_camel_case_and_equal_aliases_preserve_labels_and_raw_fields() {
    let column = column("link", Value::Null);
    let mut link = json!({"attachment":{},"displayAsUrl":false,
        "displayName":"Native Link verified","originalUrl":"http://example.com"});
    for duplicate in [false, true] {
        if duplicate {
            link["original_url"] = link["originalUrl"].clone();
            link["display_name"] = link["displayName"].clone();
            link["display_as_url"] = link["displayAsUrl"].clone();
        }
        let record = record(json!({"key":"field","link":[link.clone()]}));
        let original = record.raw.clone();
        assert_eq!(
            column.presentation(&record),
            CellPresentation::Link {
                url: "http://example.com".into(),
                label: "Native Link verified".into(),
            }
        );
        assert_eq!(record.raw, original);
    }
    let record = record(json!({"key":"field","link":[{
        "originalUrl":"http://example.com","displayName":"Name","displayAsUrl":true}]}));
    assert_eq!(column.display_text(&record), "http://example.com");
}

#[test]
fn conflicting_link_aliases_never_choose_a_destination_or_name_by_precedence() {
    let column = column("link", Value::Null);
    for (key, conflict) in [
        ("original_url", json!("https://different.test")),
        ("display_name", json!("Different name")),
        ("display_as_url", json!(true)),
        ("original_url", Value::Null),
    ] {
        let mut link = json!({"originalUrl":"https://example.test", "displayName":"Name", "displayAsUrl":false});
        link[key] = conflict;
        let record = record(json!({"key":"field","link":[link]}));
        assert_eq!(
            column.presentation(&record),
            CellPresentation::Unsupported {
                fallback: "Unsupported link".into()
            }
        );
    }
}
