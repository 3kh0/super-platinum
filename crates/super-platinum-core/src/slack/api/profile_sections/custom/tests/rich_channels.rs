use super::*;

fn captured(extra: usize) -> (Vec<Value>, RichTextContext) {
    let token = json!({"type":"channel","channel_id":"C123","opaque":{"retained":[1,"two"]}});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context
        .mention(&token, "#canonical label longer than the ID".into())
        .unwrap();
    context
        .mention(&token, "#canonical label longer than the ID".into())
        .unwrap();
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"x".repeat(4996 + extra)},token,token]}]}),
    ];
    (blocks, context)
}
#[test]
fn ordered_channel_capture_saves_exact_wire_and_requires_exact_readback() {
    let source = response();
    let mut d = draft(&source);
    let (blocks, context) = captured(0);
    d.change_rich("long", blocks.clone(), context).unwrap();
    let intent = d.capture().unwrap();
    let request = section_request(
        &SlackClient::default(),
        &workspace(),
        &intent,
        &snapshot(&source),
    )
    .unwrap();
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let elements: Value =
        serde_json::from_str(&fields.iter().find(|(key, _)| key == "elements").unwrap().1).unwrap();
    let serialized = elements
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["element_id"] == "long")
        .unwrap()["long_text"]["blocks"]
        .as_str()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(serialized).unwrap(),
        json!(blocks)
    );
    let mut fresh = source.clone();
    let target = section(&mut fresh)["profileElements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["elementId"] == "long")
        .unwrap();
    target["blocks"] = json!(serialized);
    assert!(intent.confirms_readback(&snapshot(&fresh)));
    let mut missing = blocks.clone();
    missing[0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .pop();
    section(&mut fresh)["profileElements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["elementId"] == "long")
        .unwrap()["blocks"] = json!(serde_json::to_string(&missing).unwrap());
    assert!(!intent.confirms_readback(&snapshot(&fresh)));
}
#[test]
fn invalid_length_remains_editable_and_plain_changes_retire_captured_proof() {
    let source = response();
    let mut d = draft(&source);
    let (blocks, context) = captured(1);
    d.change_rich("long", blocks.clone(), context).unwrap();
    assert!(d.dirty());
    assert!(d.validation("long").is_err());
    assert!(d.capture().is_err());
    assert!(matches!(d.value("long"), Some(SectionValue::LongText(value)) if value == &blocks));
    let (blocks, context) = captured(0);
    d.change_rich("long", blocks.clone(), context).unwrap();
    assert!(d.capture().is_ok());
    d.change("long", SectionValue::LongText(blocks)).unwrap();
    assert!(d.rich_fields.is_empty());
    // Without captured attributes the legacy path counts both raw IDs.
    assert!(d.capture().is_err());
    let (blocks, _) = captured(0);
    let before = d.value("long").cloned();
    assert!(
        d.change_rich(
            "long",
            blocks,
            RichTextContext::new_ordered("TOTHER", "U1").unwrap()
        )
        .is_err()
    );
    assert!(d.value("long") == before.as_ref());
}
