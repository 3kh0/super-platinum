use super::*;

const CLIENT_ID: &str = "4d11c69e-0dc9-4f95-825f-3e0292717e85";
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        enterprise_id: None,
        user_id: "UWORKFLOW".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-share-token".into(),
    }
}
fn trigger() -> Value {
    json!({"id":"TR_REVIEW","type":"shortcut","name":"Original review",
        "shortcut_url":"https://slack.com/shortcuts/original-review"})
}
fn message(text: &str, channel: &str) -> protocol::Message {
    protocol::Message::new(&trigger(), text, channel, CLIENT_ID, "1781524998.000001").unwrap()
}
#[test]
fn exact_original_message_factory_has_no_plain_text_wire_or_exposed_token() {
    let s = session();
    let c = SlackClient::default();
    let r = message(MESSAGE, raw::DEST).request(&c, &s).unwrap();
    let got = inspect(&r, &trigger(), &c, &s).unwrap();
    assert_eq!(got["method"], "chat.postMessage");
    assert_eq!(got["tokenPresent"], true);
    assert!(!got.to_string().contains(&s.token));
    let fields = got["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 8);
    assert_eq!(fields[0], json!(["channel", raw::DEST]));
    assert_eq!(
        fields[3],
        json!([
            "blocks",
            json!([{"type":"rich_text","elements":[{
        "type":"rich_text_section","elements":[{"type":"text","text":format!("{MESSAGE}\n")},
        {"type":"link","url":"https://slack.com/shortcuts/original-review"}]}]}])
            .to_string()
        ])
    );
    assert_eq!(
        fields[4],
        json!([
            "unfurl",
            "[{\"url\":\"https://slack.com/shortcuts/original-review\"}]"
        ])
    );
    assert_eq!(fields[5], json!(["client_msg_id", CLIENT_ID]));
    assert_eq!(fields[6], json!(["include_channel_perm_error", "true"]));
    assert_eq!(fields[7], json!(["_x_reason", "webapp_message_send"]));
}
#[test]
fn alternate_text_destination_trigger_and_session_are_not_accepted() {
    let s = session();
    let c = SlackClient::default();
    for m in [
        message("Other text", raw::DEST),
        message(MESSAGE, raw::PRIVATE),
    ] {
        assert!(inspect(&m.request(&c, &s).unwrap(), &trigger(), &c, &s).is_err());
    }
    let r = message(MESSAGE, raw::DEST).request(&c, &s).unwrap();
    let mut changed = trigger();
    changed["name"] = json!("Changed despite same URL");
    assert!(inspect(&r, &changed, &c, &s).is_err());
    let mut foreign = s.clone();
    foreign.token = "different-synthetic-token".into();
    assert!(inspect(&r, &trigger(), &c, &foreign).is_err());
    foreign = s.clone();
    foreign.url = "https://other.slack.com".into();
    assert!(inspect(&r, &trigger(), &c, &foreign).is_err());
}
#[test]
fn query_trace_changes_are_allowed_but_wire_fields_and_origin_are_not() {
    let s = session();
    let c = SlackClient::default();
    let m = message(MESSAGE, raw::DEST);
    let a = m.request(&c, &s).unwrap();
    let b = m.request(&c, &s).unwrap();
    assert!(super::super::super::picker::same_request(a.prepared(), b.prepared()).is_ok());
    let mut changed = a.prepared().clone();
    if let RequestBody::Form(fields) = &mut changed.body {
        fields.push(("text".into(), "invented fallback".into()));
    }
    assert!(super::super::super::picker::same_request(&changed, a.prepared()).is_err());
    let mut changed = a.prepared().clone();
    changed.url = changed.url.replace("fixture.slack.com", "other.slack.com");
    assert!(super::super::super::picker::same_request(&changed, a.prepared()).is_err());
}
#[test]
fn raw_ack_and_rejection_are_independent_correlated_original_replies() {
    let m = message(MESSAGE, raw::DEST);
    let ack = reply(&m, false);
    assert_eq!(ack["opaque_receipt"], "original-share-ack");
    assert_eq!(ack["message"]["client_msg_id"], CLIENT_ID);
    assert_eq!(ack["message"]["blocks"], m.blocks().clone());
    assert_eq!(
        m.classify(ack).correlation(),
        protocol::Correlation::Acknowledged
    );
    let rejected = reply(&m, true);
    assert_eq!(
        rejected,
        json!({"ok":false,"error":"restricted_action_read_only_channel",
        "opaque_receipt":"original-share-rejected"})
    );
    assert_eq!(
        m.classify(rejected).correlation(),
        protocol::Correlation::Rejected
    );
}
#[test]
fn held_original_sender_releases_exactly_once_without_manufactured_return() {
    let m = Model::for_case("held".into());
    let (tx, mut rx) = oneshot::channel();
    m.0.lock().unwrap().held.push((1, tx));
    assert_eq!(
        m.release(1).unwrap(),
        json!({"serial":1,"receiverRetired":false,"delivered":true})
    );
    assert_eq!(rx.try_recv(), Ok(()));
    assert!(m.release(1).is_err());
    assert_eq!(m.snapshot()["attempts"], json!([]));
    assert_eq!(m.snapshot()["settlements"], json!([]));
}
#[test]
fn retired_receiver_and_drop_report_actual_nondelivery_separately() {
    let m = Model::for_case("held".into());
    let (tx, rx) = oneshot::channel();
    drop(rx);
    m.0.lock().unwrap().held.push((1, tx));
    assert_eq!(
        m.release(1).unwrap(),
        json!({"serial":1,"receiverRetired":true,"delivered":false})
    );
    drop(Settlement {
        model: m.clone(),
        serial: 1,
        returned: false,
    });
    assert_eq!(
        m.snapshot()["settlements"],
        json!([{"serial":1,"returned":false}])
    );
    assert_eq!(m.snapshot()["attempts"], json!([]));
}
#[test]
fn supported_cases_and_redacted_read_shapes_stay_closed() {
    for s in [
        "success",
        "rejected",
        "lost-response",
        "held",
        "destination-failure",
    ] {
        assert!(supported(s));
    }
    for s in ["", "success-extra", "first-add-success", "denied"] {
        assert!(!supported(s));
    }
    assert_eq!(
        safe_body(&RequestBody::Form(vec![
            ("token".into(), "secret".into()),
            ("enterprise_token".into(), "secret2".into()),
            ("channel".into(), raw::DEST.into())
        ])),
        json!([["channel", raw::DEST]])
    );
    assert_eq!(
        safe_body(&RequestBody::Json(
            json!({"token":"secret","enterprise_token":"secret2","channel":raw::DEST})
        )),
        json!({"channel":raw::DEST})
    );
}

#[test]
fn lost_reply_attempts_keep_one_server_message_and_no_fabricated_ack() {
    let m = Model::for_case("lost-response".into());
    let message = message(MESSAGE, raw::DEST);
    let request = message
        .request(&SlackClient::default(), &session())
        .unwrap();
    let wire = request.inspect();
    let raw = reply(&message, false);
    for serial in 1..=3 {
        assert!(dispatch(&m, serial, &wire, &json!({"team":"TWORKFLOW"}), &raw).is_none());
    }
    let s = m.snapshot();
    assert_eq!(s["applied"], json!([raw["message"]]));
    assert_eq!(s["requests"].as_array().unwrap().len(), 3);
    for (i, r) in s["responses"].as_array().unwrap().iter().enumerate() {
        assert_eq!(r, &json!({"serial":i+1,"error":"transport","applied":true}));
        assert_eq!(s["requests"][i]["request"], wire);
    }
    assert_eq!(s["settlements"], json!([]));
    assert_eq!(s["attempts"], json!([]));
}

#[test]
fn rejection_never_applies_while_held_ack_is_applied_before_delivery() {
    let message = message(MESSAGE, raw::DEST);
    let wire = message
        .request(&SlackClient::default(), &session())
        .unwrap()
        .inspect();
    let rejected = Model::for_case("rejected".into());
    assert!(dispatch(&rejected, 1, &wire, &Value::Null, &reply(&message, true)).is_none());
    assert_eq!(rejected.snapshot()["applied"], json!([]));
    assert_eq!(rejected.snapshot()["responses"][0]["applied"], false);
    let held = Model::for_case("held".into());
    let rx = dispatch(&held, 1, &wire, &Value::Null, &reply(&message, false)).unwrap();
    assert_eq!(held.snapshot()["applied"].as_array().unwrap().len(), 1);
    assert_eq!(held.snapshot()["held"], json!([1]));
    assert_eq!(held.snapshot()["settlements"], json!([]));
    drop(rx);
    assert_eq!(held.release(1).unwrap()["delivered"], false);
    assert_eq!(held.snapshot()["applied"].as_array().unwrap().len(), 1);
}
