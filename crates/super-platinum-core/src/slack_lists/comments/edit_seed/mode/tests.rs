use super::*;
use serde_json::json;

#[test]
fn observed_plain_mode_wins_over_portable_treatment() {
    assert_eq!(EditMode::from_observed(false, false), EditMode::Rich);
    assert_eq!(EditMode::from_observed(false, true), EditMode::Portable);
    assert_eq!(EditMode::from_observed(true, false), EditMode::PlainText);
    assert_eq!(EditMode::from_observed(true, true), EditMode::PlainText);
}

#[test]
fn source_modes_partition_raw_blocks_without_duplicating_opaque_content() {
    let rich = json!({"type":"rich_text","elements":[],"extra":[null,true,1]});
    let context = json!({"type":"context","elements":[],"unknown":{"a":1}});
    let future = json!({"type":"future_block","raw":[1,{"x":"y"}]});
    let message = json!({"blocks":[future,rich,context],"text":"fallback","extra":17});
    let original = message.clone();
    let rich_plan = SeedPlan::capture(&message, EditMode::Rich).unwrap();
    assert_eq!(
        rich_plan.input(),
        &SeedInput::RichBlocks(vec![rich.clone()])
    );
    assert_eq!(
        rich_plan.preserved_blocks(),
        &[future.clone(), context.clone()]
    );
    let plain = SeedPlan::capture(&message, EditMode::PlainText).unwrap();
    assert_eq!(
        plain.input(),
        &SeedInput::PlainBlocks(message["blocks"].as_array().unwrap().clone())
    );
    assert_eq!(plain.preserved_blocks(), &[future.clone(), context.clone()]);
    let portable = SeedPlan::capture(&message, EditMode::Portable).unwrap();
    assert_eq!(
        portable.input(),
        &SeedInput::PortableBlocks(vec![future, rich])
    );
    assert_eq!(portable.preserved_blocks(), &[context]);
    assert_eq!(message, original);
}

#[test]
fn plain_all_editable_and_mixed_array_routes_match_source() {
    let section = json!({"type":"section","text":{"type":"mrkdwn","text":"*bold*"}});
    let divider = json!({"type":"divider","block_id":"D"});
    let context = json!({"type":"context","elements":[]});
    let all = json!({"blocks":[section,divider],"text":"fallback"});
    let plan = SeedPlan::capture(&all, EditMode::PlainText).unwrap();
    assert_eq!(
        plan.input(),
        &SeedInput::PlainBlocks(all["blocks"].as_array().unwrap().clone())
    );
    assert!(plan.preserved_blocks().is_empty());
    let mixed = json!({"blocks":[section,divider,context],"text":"fallback"});
    let plan = SeedPlan::capture(&mixed, EditMode::PlainText).unwrap();
    assert_eq!(
        plan.input(),
        &SeedInput::Text {
            text: "fallback".into(),
            mode: TextMode::Edit
        }
    );
    assert_eq!(plan.preserved_blocks(), &[context]);
}

#[test]
fn portable_context_only_falls_back_to_normal_text_and_preserves_context() {
    let context = json!({"type":"context","elements":[],"deep":{"x":[1]}});
    let plan = SeedPlan::capture(
        &json!({"blocks":[context],"text":"body"}),
        EditMode::Portable,
    )
    .unwrap();
    assert_eq!(
        plan.input(),
        &SeedInput::Text {
            text: "body".into(),
            mode: TextMode::Normal
        }
    );
    assert_eq!(plan.preserved_blocks(), &[context]);
}

#[test]
fn editable_section_predicate_preserves_javascript_empty_collection_truthiness() {
    let base = json!({"type":"section","text":{"type":"plain_text","text":""}});
    assert!(editable_block(&base));
    for key in ["fields", "accessory"] {
        for value in [json!([]), json!({}), json!("x"), json!(1)] {
            let mut b = base.clone();
            b[key] = value;
            assert!(!editable_block(&b));
        }
        for value in [Value::Null, json!(false), json!(0), json!("")] {
            let mut b = base.clone();
            b[key] = value;
            assert!(editable_block(&b));
        }
    }
    assert!(!editable_block(&json!({"type":"section"})));
}

#[test]
fn plan_rejects_malformed_container_without_rewriting_unknown_valid_blocks() {
    for message in [
        Value::Null,
        json!([]),
        json!({"blocks":{}}),
        json!({"blocks":[null]}),
        json!({"blocks":[{}]}),
        json!({"text":1}),
    ] {
        assert_eq!(
            SeedPlan::capture(&message, EditMode::Rich),
            Err(SeedUnavailable::MalformedMessage)
        );
    }
    let raw = json!({"type":"future","opaque":{"a":1},"block_id":"same"});
    let p = SeedPlan::capture(&json!({"blocks":[raw,raw]}), EditMode::Portable).unwrap();
    assert_eq!(
        p.input(),
        &SeedInput::PortableBlocks(vec![raw.clone(), raw])
    );
    assert!(p.preserved_blocks().is_empty());
}
