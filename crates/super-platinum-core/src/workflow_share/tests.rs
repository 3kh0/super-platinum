use super::*;
const UUID: &str = "e3525c02-e6aa-4267-8dbc-8fd408c4ad23";
const TS: &str = "1781524999.000001";
fn trigger() -> Value {
    json!({"id":"TR_REVIEW","share_url":"https://slack.com/shortcuts/share","shortcut_url":"https://slack.com/shortcuts/fallback","opaque":[1]})
}
fn message(text: &str) -> Message {
    Message::new(&trigger(), text, "CADDDEST01", UUID, TS).unwrap()
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-secret".into(),
    }
}
#[test]
fn truthy_url_precedence_retains_original_source_and_never_synthesizes() {
    let original = trigger();
    let m = message("");
    assert_eq!(m.url(), original["share_url"]);
    assert_eq!(m.trigger(), &original);
    for false_value in [Value::Null, json!(false), json!(0), json!("")] {
        let mut t = trigger();
        t["share_url"] = false_value;
        assert_eq!(
            Message::new(&t, "", "CADDDEST01", UUID, TS).unwrap().url(),
            t["shortcut_url"]
        );
    }
    for malformed in [json!(true), json!(1), json!([]), json!({})] {
        let mut t = trigger();
        t["share_url"] = malformed;
        assert!(Message::new(&t, "", "CADDDEST01", UUID, TS).is_err());
    }
    assert_eq!(
        Message::new(&json!({"id":"T"}), "", "CADDDEST01", UUID, TS)
            .unwrap()
            .blocks(),
        &json!([])
    );
}
#[test]
fn plain_blocks_preserve_unicode_and_source_whitespace_rules_without_interpretation() {
    let m = message(" \u{feff}\nHello 中🦊 *literal*\nnext  ");
    assert_eq!(
        m.blocks(),
        &json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Hello 中🦊 *literal*\nnext\n"},{"type":"link","url":m.url()}]}]}])
    );
    assert_eq!(
        m.text_fallback(),
        format!("Hello 中🦊 *literal*\nnext\n{}", m.url())
    );
    assert!(message("\u{0085}x").text_fallback().starts_with('\u{0085}')); // not JS whitespace
    for text in ["", " \n\t\u{00a0}"] {
        let m = message(text);
        assert_eq!(
            m.blocks()[0]["elements"][0]["elements"],
            json!([{"type":"link","url":m.url()}])
        );
        assert_eq!(m.text_fallback(), m.url());
    }
}
#[test]
fn host_identity_and_size_refuse_malformed_without_granting_permission() {
    for id in ["", "Cbad space", "U123", "C/1"] {
        assert!(Message::new(&trigger(), "", id, UUID, TS).is_err());
    }
    for id in ["", "not-a-uuid", "00000000-0000-0000-0000-000000000000"] {
        assert!(Message::new(&trigger(), "", "CADDDEST01", id, TS).is_err());
    }
    for ts in ["", "0", "1781524999.bad"] {
        assert!(Message::new(&trigger(), "", "CADDDEST01", UUID, ts).is_err());
    }
    assert!(
        Message::new(
            &trigger(),
            &"x".repeat(MAX_TEXT + 1),
            "CADDDEST01",
            UUID,
            TS
        )
        .is_err()
    );
}
#[test]
fn exact_token_first_factory_omits_unsupplied_message_fields_and_debug_secrets() {
    let m = message("Hello");
    let req = m.request(&SlackClient::default(), &session()).unwrap();
    let RequestBody::Form(fields) = &req.prepared().body else {
        panic!()
    };
    assert_eq!(
        fields,
        &vec![
            ("token".into(), "synthetic-secret".into()),
            ("channel".into(), "CADDDEST01".into()),
            ("ts".into(), TS.into()),
            ("type".into(), "message".into()),
            ("blocks".into(), m.blocks().to_string()),
            ("unfurl".into(), json!([{"url":m.url()}]).to_string()),
            ("client_msg_id".into(), UUID.into()),
            ("include_channel_perm_error".into(), "true".into()),
            ("_x_reason".into(), REASON.into())
        ]
    );
    assert!(req.fields_current());
    assert!(
        !req.prepared()
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    );
    assert!(!req.inspect().to_string().contains("synthetic-secret"));
    assert!(!format!("{req:?}").contains("synthetic-secret"));
    assert_eq!(req.message().blocks(), m.blocks());
    let mut bad = session();
    bad.token.clear();
    assert!(m.request(&SlackClient::default(), &bad).is_err());
}
#[test]
fn raw_ack_retained_and_host_correlation_separate_from_generic_api_success() {
    let m = message("Hello");
    let raw = json!({"ok":true,"channel":m.channel(),"ts":TS,"message":{"ts":TS,"client_msg_id":UUID,"blocks":m.blocks(),"thread_ts":TS},"opaque":{"keep":true}});
    let receipt = m.classify(raw.clone());
    assert_eq!(receipt.correlation(), Correlation::Acknowledged);
    assert_eq!(receipt.raw(), &raw);
    assert_eq!(receipt.ts(), Some(TS));
    for path in [
        "/channel",
        "/ts",
        "/message/ts",
        "/message/client_msg_id",
        "/message/thread_ts",
    ] {
        let mut wrong = raw.clone();
        *wrong.pointer_mut(path).unwrap() = json!("wrong");
        assert!(acknowledged(&wrong));
        assert_eq!(m.classify(wrong).correlation(), Correlation::Unknown);
    }
    assert!(acknowledged(&json!({"ok":true})));
    assert_eq!(
        m.classify(json!({"ok":true})).correlation(),
        Correlation::Unknown
    );
}
#[test]
fn rejection_unknown_and_nonboolean_ok_never_manufacture_ack() {
    let m = message("");
    for raw in [
        json!({"ok":false}),
        json!({"ok":false,"error":"internal_error"}),
        json!({"ok":1}),
        Value::Null,
        json!({"ok":true,"error":"conflict","channel":m.channel(),"ts":TS}),
    ] {
        let r = m.classify(raw.clone());
        assert_eq!(r.correlation(), Correlation::Unknown);
        assert_eq!(r.raw(), &raw);
        assert_eq!(r.ts(), None);
    }
    let raw = json!({"ok":false,"error":"restricted_action_read_only_channel","opaque":[1]});
    let r = m.classify(raw.clone());
    assert_eq!(r.correlation(), Correlation::Rejected);
    assert_eq!(r.raw(), &raw);
}

#[test]
fn actual_url_string_is_not_replaced_by_native_scheme_policy_and_empty_url_is_content_only() {
    for url in [
        "slack://shortcuts/X",
        "http://example.test/workflow",
        "custom:workflow",
        " ",
    ] {
        let t = json!({"share_url":url});
        let m = Message::new(&t, "Hi", "CADDDEST01", UUID, TS).unwrap();
        assert_eq!(m.url(), url);
    }
    let m = Message::new(&json!({}), " ", "CADDDEST01", UUID, TS).unwrap();
    assert_eq!(m.blocks(), &json!([]));
    let r = m.request(&SlackClient::default(), &session()).unwrap();
    let RequestBody::Form(f) = &r.prepared().body else {
        panic!()
    };
    assert!(!f.iter().any(|(k, _)| k == "blocks" || k == "text"));
    assert!(
        f.iter()
            .any(|(k, v)| k == "unfurl" && v == r#"[{"url":""}]"#)
    );
    let m = Message::new(&json!({}), "  text  \n", "CADDDEST01", UUID, TS).unwrap();
    assert_eq!(m.text_fallback(), "text");
}
#[test]
fn decorated_ack_blocks_do_not_change_identity_correlation() {
    let m = message("Hello");
    let mut blocks = m.blocks().clone();
    blocks[0]["block_id"] = json!("server-block-id");
    assert_eq!(m.classify(json!({"ok":true,"channel":m.channel(),"ts":TS,"message":{"client_msg_id":UUID,"blocks":blocks}})).correlation(),Correlation::Acknowledged);
    assert_eq!(strip_before_newlines("a  \nb\t  \n  \nx"), "a\nb\t  \n\nx");
}
