use super::*;
use automerge::{ScalarValue, marks::Mark};
use serde_json::Value;

const NEW: &str = "temp:C:DEF0123456789abcdef012345678";
fn vectors() -> Value {
    serde_json::from_str(include_str!("../fixtures/underline.json")).unwrap()
}
fn marks(value: &Value) -> Vec<Mark> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let scalar = match &m["value"] {
                Value::Bool(value) => ScalarValue::Boolean(*value),
                Value::String(value) => ScalarValue::Str(value.clone().into()),
                _ => panic!("closed source mark fixture"),
            };
            Mark::new(
                m["name"].as_str().unwrap().into(),
                scalar,
                m["start"].as_u64().unwrap() as usize,
                m["end"].as_u64().unwrap() as usize,
            )
        })
        .collect()
}
#[test]
fn original_escaped_annotation_vectors_keep_underline_and_independent_false_tombstones() {
    let all = vectors();
    let rows = all["vectors"].as_array().unwrap();
    let v = rows
        .iter()
        .find(|v| v["kind"] == "annotationMarks")
        .unwrap();
    let text = v["escapedText"].as_str().unwrap();
    assert_eq!(text.encode_utf16().count(), 13);
    assert_eq!(
        compact::render(text, &marks(&v["marks"])).unwrap(),
        v["canonicalAnnotated"].as_str().unwrap()
    );
    assert_eq!(
        compact::render(text, &marks(&v["falseMarks"])).unwrap(),
        v["afterUnderlineRemoval"].as_str().unwrap()
    );
    let removal = rows
        .iter()
        .find(|v| v["kind"] == "annotationRemoval")
        .unwrap();
    assert_eq!(
        compact::render(text, &marks(&removal["marks"])).unwrap(),
        removal["rtml"].as_str().unwrap()
    );
    let original = existing::from_rtml(v["seed"].as_str().unwrap()).unwrap();
    assert_eq!(original.spans.len(), 1);
    let old = &original.spans[0];
    assert_eq!(
        (old.start, old.end, old.escaped_start, old.escaped_end),
        (1, 4, 1, 8)
    );
    let projected = range::capture_existing(&original, 5, 6, NEW).unwrap();
    assert_eq!((projected.escaped_start, projected.escaped_end), (12, 13));
    assert_eq!(projected.rtml, v["canonicalAnnotated"].as_str().unwrap());
    assert!(
        projected
            .selected
            .runs
            .iter()
            .all(|run| run.style.underline)
    );
    assert!(range::capture_existing(&original, 3, 4, NEW).is_err());
}
#[test]
fn original_equal_range_style_order_includes_underline() {
    let all = vectors();
    let v = all["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["kind"] == "canonicalNesting")
        .unwrap();
    let draft = TextDraft::from_rtml(v["input"].as_str().unwrap()).unwrap();
    let model = draft.model().unwrap();
    // Production serialization consumes actual Automerge mark order, not the
    // draft's style-enumeration order.
    use automerge::{ReadDoc, marks::ExpandMark, transaction::Transactable};
    let mut doc = automerge::Automerge::new();
    let mut tx = doc.transaction();
    let object = tx
        .put_object(automerge::ROOT, "text", automerge::ObjType::Text)
        .unwrap();
    tx.splice_text(&object, 0, 0, &model.text).unwrap();
    for mark in model.marks {
        tx.mark(&object, mark, ExpandMark::After).unwrap();
    }
    tx.commit();
    assert_eq!(
        compact::render(&model.text, &doc.marks(&object).unwrap()).unwrap(),
        v["rtml"].as_str().unwrap()
    );
}
#[test]
fn underline_attributes_unknown_controls_and_malformed_marks_refuse() {
    for source in [
        "<u class='x'>x</u>",
        "<u style='x'>x</u>",
        "<u><control>x</control></u>",
        "<annotation id='a'><u><annotation id='b'>x</annotation></u></annotation>",
        "<u><annotation id='a' extra='x'>x</annotation></u>",
    ] {
        assert!(existing::from_rtml(source).is_err(), "{source}");
    }
    for mark in [
        Mark::new("u".into(), "true", 0, 2),
        Mark::new("u".into(), true, 1, 2),
        Mark::new("u".into(), true, 0, 3),
        Mark::new("unknown".into(), false, 0, 2),
    ] {
        assert!(compact::render("\u{1f642}", &[mark]).is_err());
    }
    assert!(
        compact::render(
            "x",
            &[
                Mark::new("u".into(), true, 0, 1),
                Mark::new("u".into(), true, 0, 1)
            ]
        )
        .is_err()
    );
}
