use super::*;
fn key() -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    }
}
fn emoji() -> Arc<Value> {
    let tables: Value = serde_json::from_str(include_str!("arguments/emoji-fixture.json")).unwrap();
    Arc::new(tables["16"].clone())
}
fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!("arguments/rich-cases.json")).unwrap()
}
fn snapshot(case: &Value) -> Snapshot {
    Snapshot {
        blocks: case["full"]["blocks"].as_array().unwrap().clone(),
        file_ids: vec![],
        destinations: vec![json!({"channel_id":"C1"})],
    }
}

#[test]
fn exact_known_rich_snapshot_derives_same_actual_source_arguments() {
    let mut count = 0;
    for case in cases().into_iter().filter(|c| c["accepted"] == true) {
        let desired = snapshot(&case);
        let emoji = emoji();
        let pair = derive(&key(), &desired, emoji.clone())
            .unwrap_or_else(|e| panic!("{}: {e:?}", case["name"]));
        assert_eq!(pair.desired(), &desired);
        assert!(pair.matches(&key(), &desired, &emoji));
        assert_eq!(
            pair.arguments().blocks(),
            case["source"]["request"]["args"].get("blocks"),
            "{}",
            case["name"]
        );
        assert_eq!(
            super::super::arguments::full_blocks(pair.delta(), &emoji).unwrap(),
            Some(Value::Array(desired.blocks.clone()))
        );
        count += 1;
    }
    assert_eq!(count, 24);
}
#[test]
fn desired_destination_catalog_generation_or_unknown_fields_cannot_be_substituted() {
    let desired = snapshot(&cases()[0]);
    let emoji = emoji();
    let pair = derive(&key(), &desired, emoji.clone()).unwrap();
    let mut key2 = key();
    key2.channel = "C2".into();
    assert!(!pair.matches(&key2, &desired, &emoji));
    assert!(derive(&key2, &desired, emoji.clone()).is_err());
    assert!(!pair.matches(&key(), &desired, &Arc::new((*emoji).clone())));
    let mut changed = desired.clone();
    changed.blocks[0]["elements"][1]["future"] = json!(true);
    assert!(!pair.matches(&key(), &changed, &emoji));
    assert!(derive(&key(), &changed, emoji.clone()).is_err());
    changed = desired.clone();
    changed.file_ids.push("F12345678".into());
    assert!(derive(&key(), &changed, emoji).is_err());
}

#[test]
fn noncanonical_adjacent_sections_and_normalized_borders_remain_opaque() {
    let mut desired = Snapshot {
        blocks: vec![json!({"type":"rich_text","elements":[
            {"type":"rich_text_section","elements":[{"type":"text","text":"/demo\n"}]},
            {"type":"rich_text_section","elements":[{"type":"text","text":"argument"}]}
        ]})],
        file_ids: vec![],
        destinations: vec![json!({"channel_id":"C1"})],
    };
    assert!(derive(&key(), &desired, emoji()).is_err());
    desired.blocks[0]["elements"][1] =
        json!({"type":"rich_text_quote","border":0,"elements":[{"type":"text","text":"quote"}]});
    assert!(derive(&key(), &desired, emoji()).is_err());
    desired.blocks[0]["elements"][1] = json!({"type":"rich_text_preformatted","border":2,"elements":[{"type":"text","text":"code"}]});
    assert!(derive(&key(), &desired, emoji()).is_err());
}
#[test]
fn retained_header_or_rich_snapshot_does_not_satisfy_legacy_codec_pair() {
    for case in cases().into_iter().filter(|c| {
        matches!(
            c["name"].as_str(),
            Some("header_1" | "quote_style" | "list_bullet" | "pre_multiline")
        )
    }) {
        let desired = snapshot(&case);
        assert!(derive(&key(), &desired, emoji()).is_ok());
        assert!(codec::decode(&key(), &desired).is_err());
    }
}
#[test]
fn arbitrary_offset_unknown_node_and_forged_emoji_unicode_cannot_roundtrip() {
    let list = cases()
        .into_iter()
        .find(|c| c["name"] == "list_ordered")
        .unwrap();
    let mut desired = snapshot(&list);
    desired.blocks[0]["elements"][1]["offset"] = json!(50);
    assert!(derive(&key(), &desired, emoji()).is_err());
    let mut desired = snapshot(&list);
    desired.blocks[0]["elements"][1]["type"] = json!("unknown_block");
    assert!(derive(&key(), &desired, emoji()).is_err());
    let style = cases()
        .into_iter()
        .find(|c| c["name"] == "list_styles")
        .unwrap();
    let mut desired = snapshot(&style);
    desired.blocks[0]["elements"][1]["elements"][0]["elements"][4]["unicode"] = json!("forged");
    assert!(derive(&key(), &desired, emoji()).is_err());
}
