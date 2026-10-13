use super::*;
use serde_json::json;

#[test]
fn message_send_requires_an_explicit_positive_success_flag() {
    let receipt = json!({
        "ok":true, "channel":"C1", "ts":"1800000000.000001",
        "message":{"text":"Preserve 日本語 🚀", "ts":"1800000000.000001"}
    });
    let sent = decode_sent_message(receipt.clone()).unwrap();
    assert_eq!(sent.channel, "C1");
    assert_eq!(sent.ts, "1800000000.000001");
    assert_eq!(sent.message.text.as_deref(), Some("Preserve 日本語 🚀"));

    let mut missing = receipt.clone();
    missing.as_object_mut().unwrap().remove("ok");
    assert!(matches!(
        decode_sent_message(missing),
        Err(Error::Transport(_))
    ));
    for flag in [
        json!(false),
        json!(null),
        json!("true"),
        json!(1),
        json!({}),
        json!([]),
    ] {
        let mut malformed = receipt.clone();
        malformed["ok"] = flag;
        assert!(matches!(
            decode_sent_message(malformed),
            Err(Error::Transport(_))
        ));
    }
}

#[test]
fn positive_flag_does_not_hide_a_missing_or_malformed_message_receipt() {
    for receipt in [
        json!({"ok":true}),
        json!({"ok":true,"channel":"C1","ts":12,"message":{}}),
        json!({"ok":true,"channel":"C1","ts":"1800000000.000001","message":null}),
    ] {
        assert!(matches!(
            decode_sent_message(receipt),
            Err(Error::Transport(_))
        ));
    }
}
