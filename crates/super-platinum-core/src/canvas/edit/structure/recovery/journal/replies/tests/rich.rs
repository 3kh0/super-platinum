use super::*;
use crate::canvas::comments::{content, reply::ReplyIntent};
fn rich_intent(s: &Scope, blocks: &[serde_json::Value]) -> ReplyIntent {
    let file =
        serde_json::from_value(json!({"id":s.file,"filetype":"quip","access":"comment"})).unwrap();
    let root = serde_json::from_value(json!({"type":"message","channel":"C123","ts":ROOT,
        "subtype":"document_comment_root","document_comment":{"thread_id":"quip-thread"}}))
    .unwrap();
    ReplyIntent::capture_rich(
        &s.workspace,
        &s.user,
        &file,
        &root,
        blocks.to_vec(),
        &uuid::Uuid::new_v4().to_string(),
    )
    .unwrap()
}
#[test]
fn rich_capture_identity_preserves_format_and_unknown_leaf_fields_across_restart() {
    let (d, mut j, s) = setup();
    let capture = uuid::Uuid::new_v4().to_string();
    let mut blocks = content::plain("same visible text");
    blocks[0]["elements"][0]["elements"][0]["style"] = json!({"bold":true});
    blocks[0]["elements"][0]["elements"][0]["future_metadata"] =
        json!({"retained":[1,true,"opaque"]});
    let ReplySave::Stored(old) = j
        .save_rich_reply_draft(&s, ROOT, None, &capture, blocks.clone())
        .unwrap()
    else {
        panic!("stored")
    };
    assert_eq!(old.blocks(), Some(blocks.as_slice()));
    assert!(
        j.save_reply_draft(&s, ROOT, Some(&old), &capture, old.text())
            .is_err()
    );
    let mut changed = blocks.clone();
    changed[0]["elements"][0]["elements"][0]["style"] = json!({"italic":true});
    assert!(
        j.save_rich_reply_draft(&s, ROOT, Some(&old), &capture, changed)
            .is_err()
    );
    drop(j);
    let mut j = Journal::open(
        d.path().join("reply.sqlite"),
        "147b5480-9f1a-4443-89da-836472cbb209",
    )
    .unwrap();
    assert_eq!(
        j.reply_inventory(&s, ROOT).unwrap().drafts()[0].blocks(),
        Some(blocks.as_slice())
    );
    assert!(matches!(
        j.save_rich_reply_draft(&s, ROOT, Some(&old), &capture, blocks)
            .unwrap(),
        ReplySave::Stored(_)
    ));
}
#[test]
fn rich_acknowledgement_never_clears_newer_equal_text_different_format() {
    let (_d, mut j, s) = setup();
    let blocks = content::plain("same");
    let ReplySave::Stored(old) = j
        .save_rich_reply_draft(
            &s,
            ROOT,
            None,
            &uuid::Uuid::new_v4().to_string(),
            blocks.clone(),
        )
        .unwrap()
    else {
        panic!("stored")
    };
    let intent = rich_intent(&s, &blocks);
    let receipt = j
        .begin_reply(
            &j.reply_inventory(&s, ROOT).unwrap(),
            intent.record().clone(),
        )
        .unwrap()
        .unwrap();
    let mut newer = blocks.clone();
    newer[0]["elements"][0]["elements"][0]["style"] = json!({"italic":true});
    let ReplySave::Stored(new) = j
        .save_rich_reply_draft(
            &s,
            ROOT,
            Some(&old),
            &uuid::Uuid::new_v4().to_string(),
            newer.clone(),
        )
        .unwrap()
    else {
        panic!("stored")
    };
    let accepted = intent
        .clone()
        .accept(&json!({"ok":true,"channel":"C123","ts":"1700000001.000100",
        "message":{"type":"message","channel":"C123","thread_ts":ROOT,"ts":"1700000001.000100",
            "text":"same","blocks":blocks,"client_msg_id":intent.record().client_msg_id()}}))
        .unwrap();
    assert!(j.reply_acknowledged(&receipt, &accepted).unwrap());
    let inventory = j.reply_inventory(&s, ROOT).unwrap();
    assert_eq!(inventory.drafts(), &[new]);
    assert_eq!(inventory.drafts()[0].blocks(), Some(newer.as_slice()));
}
