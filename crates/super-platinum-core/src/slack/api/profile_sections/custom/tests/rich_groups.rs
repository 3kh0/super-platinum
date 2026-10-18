use super::*;

fn captured(extra: usize) -> (Vec<Value>, RichTextContext) {
    let a = json!({"type":"usergroup","usergroup_id":"S123","future":{"occurrence":1}});
    let b = json!({"type":"usergroup","usergroup_id":"S123","future":{"occurrence":2},"tab_id":{"keep":true}});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    for token in [&a, &b] {
        context.mention(token, "@@ e\u{301} ".into()).unwrap();
    }
    (
        vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":format!("{}{}","\u{1f680}".repeat(2498),"x".repeat(extra))},a,b]}]}),
        ],
        context,
    )
}

#[test]
fn group_context_5000_boundary_preserves_duplicate_tokens_in_wire_and_readback() {
    let source = response();
    let mut draft = draft(&source);
    let (blocks, context) = captured(0);
    draft.change_rich("long", blocks.clone(), context).unwrap();
    let intent = draft.capture().unwrap();
    let encoded = intent
        .elements
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["element_id"] == "long")
        .unwrap()["long_text"]["blocks"]
        .as_str()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(encoded).unwrap(),
        json!(blocks)
    );
    let mut fresh = source;
    section(&mut fresh)["profileElements"][6]["blocks"] = json!(encoded);
    assert!(intent.confirms_readback(&snapshot(&fresh)));
    let mut reordered = blocks;
    reordered[0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .swap(1, 2);
    section(&mut fresh)["profileElements"][6]["blocks"] =
        json!(serde_json::to_string(&reordered).unwrap());
    assert!(!intent.confirms_readback(&snapshot(&fresh)));
}

#[test]
fn invalid_group_length_retains_draft_and_plain_change_retires_context() {
    let mut draft = draft(&response());
    let (blocks, context) = captured(1);
    draft.change_rich("long", blocks.clone(), context).unwrap();
    assert!(draft.validation("long").is_err());
    assert!(draft.capture().is_err());
    assert!(matches!(draft.value("long"),Some(SectionValue::LongText(v)) if v==&blocks));
    for (team, user) in [("TOTHER", "U1"), ("T1", "UOTHER")] {
        assert!(
            draft
                .change_rich(
                    "long",
                    vec![],
                    RichTextContext::new_ordered(team, user).unwrap()
                )
                .is_err()
        );
        assert!(matches!(draft.value("long"),Some(SectionValue::LongText(v)) if v==&blocks));
    }
    let (blocks, context) = captured(0);
    draft.change_rich("long", blocks.clone(), context).unwrap();
    assert!(draft.capture().is_ok());
    draft
        .change("long", SectionValue::LongText(blocks))
        .unwrap();
    assert!(draft.rich_fields.is_empty());
    assert!(draft.capture().is_err());
}

#[test]
fn empty_group_label_does_not_remove_raw_token_from_saved_blocks() {
    let mut draft = draft(&response());
    let group = json!({"type":"usergroup","usergroup_id":"S123","future":true});
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[group.clone()]}]}),
    ];
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context.mention(&group, String::new()).unwrap();
    draft.change_rich("long", blocks.clone(), context).unwrap();
    let intent = draft.capture().unwrap();
    let encoded = intent
        .elements
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["element_id"] == "long")
        .unwrap()["long_text"]["blocks"]
        .as_str()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(encoded).unwrap(),
        json!(blocks)
    );
}
