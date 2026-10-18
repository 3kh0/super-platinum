use super::*;
use serde_json::json;
fn data() -> (File, Message) {
    (serde_json::from_value(json!({"id":"F123","filetype":"quip","access":"comment"})).unwrap(),serde_json::from_value(json!({"type":"message","channel":"C123","ts":"1700000000.000100","subtype":"document_comment_root","reply_count":1,"document_comment":{"thread_id":"quip-thread","is_archived":false}})).unwrap())
}
#[test]
fn existing_comment_permission_differs_from_resolve_and_keeps_raw_root() {
    let (f, r) = data();
    let id = uuid::Uuid::new_v4().to_string();
    let intent =
        ReplyIntent::capture("T1", "U1", &f, &r, "Reply \u{65e5}\u{1f98a} e\u{301}", &id).unwrap();
    assert_eq!(intent.record().root_ts(), "1700000000.000100");
    assert!(intent.revalidate(&f, &r).is_ok());
    let mut changed = r;
    changed.reply_count = Some(2);
    assert!(intent.revalidate(&f, &changed).is_err());
}
#[test]
fn archived_locked_or_read_only_roots_cannot_mint_send_authority() {
    let (f, r) = data();
    for key in ["is_locked", "is_deleted"] {
        let mut f = f.clone();
        f.extra.insert(key.into(), json!(true));
        assert!(!ReplyIntent::available(&f, &r));
    }
    let mut locked = r.clone();
    locked.extra.insert("is_locked".into(), json!(true));
    assert!(!ReplyIntent::available(&f, &locked));
    let mut f = f;
    f.extra.insert("access".into(), json!("read"));
    assert!(!ReplyIntent::available(&f, &r));
    let (f, mut r) = data();
    r.extra.get_mut("document_comment").unwrap()["is_archived"] = json!(true);
    assert!(!ReplyIntent::available(&f, &r));
}
#[test]
fn ack_requires_exact_client_channel_thread_and_text_not_visual_similarity() {
    let (f, r) = data();
    let id = uuid::Uuid::new_v4().to_string();
    let intent = ReplyIntent::capture("T1", "U1", &f, &r, "reply", &id).unwrap();
    let raw = json!({"ok":true,"channel":"C123","ts":"1700000001.000100","message":{"type":"message","channel":"C123","thread_ts":"1700000000.000100","ts":"1700000001.000100","text":"reply","client_msg_id":id}});
    assert!(intent.clone().accept(&raw).is_ok());
    for key in ["channel", "thread_ts", "client_msg_id", "text"] {
        let mut bad = raw.clone();
        bad["message"][key] = json!("wrong");
        assert!(intent.clone().accept(&bad).is_err());
    }
    let mut bad = raw;
    bad["error"] = json!(false);
    assert!(intent.accept(&bad).is_err());
}
#[test]
fn text_budget_is_utf16_and_empty_send_is_not_authority() {
    let (f, r) = data();
    let id = uuid::Uuid::new_v4().to_string();
    assert!(valid_text(&"\u{1f98a}".repeat(6000)));
    assert!(!valid_text(&"\u{1f98a}".repeat(6001)));
    assert!(!valid_text("bad\0"));
    assert!(ReplyIntent::capture("T1", "U1", &f, &r, " ", &id).is_err());
}

#[test]
fn rich_ack_proves_complete_blocks_and_preserves_v1_records() {
    let (file, root) = data();
    let id = uuid::Uuid::new_v4().to_string();
    let mut blocks = super::super::content::plain("same words");
    blocks[0]["elements"][0]["elements"][0]["style"] = json!({"bold":true});
    let intent = ReplyIntent::capture_rich("T1", "U1", &file, &root, blocks.clone(), &id).unwrap();
    let record = serde_json::to_value(intent.record()).unwrap();
    assert_eq!(record["version"], 2);
    let restored: ReplyRecord = serde_json::from_value(record).unwrap();
    assert!(restored.validate().is_ok());
    assert_eq!(restored.blocks().unwrap(), blocks);
    let mut raw = json!({"ok":true,"channel":"C123","ts":"1700000001.000100",
        "message":{"type":"message","channel":"C123","thread_ts":"1700000000.000100",
        "ts":"1700000001.000100","client_msg_id":id,"text":"server fallback","blocks":blocks}});
    raw["message"]["blocks"][0]["block_id"] = json!("server-generated");
    assert!(intent.clone().accept(&raw).is_ok());
    let mut wrong = raw.clone();
    wrong["message"]["blocks"][0]["elements"][0]["elements"][0]["style"]["bold"] = json!(false);
    assert!(intent.clone().accept(&wrong).is_err());
    wrong["message"]["text"] = json!("same words");
    assert!(intent.clone().accept(&wrong).is_err());
    raw["message"].as_object_mut().unwrap().remove("blocks");
    assert!(intent.accept(&raw).is_err());
    let plain = ReplyIntent::capture("T1", "U1", &file, &root, "legacy", &id).unwrap();
    let v1 = serde_json::to_value(plain.record()).unwrap();
    assert!(v1.get("blocks").is_none());
    let restored: ReplyRecord = serde_json::from_value(v1).unwrap();
    assert!(restored.validate().is_ok());
    assert!(restored.blocks().is_none());
}
