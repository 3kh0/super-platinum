use super::*;
fn setup() -> (SlackClient, WorkspaceSession, Value, protocol::Message) {
    let client = SlackClient::default();
    let session = WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        enterprise_id: None,
        user_id: "UWORKFLOW".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-resend-token".into(),
    };
    let trigger = json!({"id":"TR_REVIEW","type":"shortcut","shortcut_url":"https://slack.com/shortcuts/original-review"});
    let message = protocol::Message::new(
        &trigger,
        MESSAGE,
        raw::DEST,
        "4d11c69e-0dc9-4f95-825f-3e0292717e85",
        "1781524998.000001",
    )
    .unwrap();
    (client, session, trigger, message)
}
#[test]
fn resend_requires_distinct_exact_factory_and_original_identity() {
    let (c, s, t, m) = setup();
    let original = m.request(&c, &s).unwrap();
    let retry = m.resend_request(&c, &s, None).unwrap();
    let got = inspect_resend(&retry, &t, &c, &s, &original.inspect(), None).unwrap();
    assert_eq!(got["fields"].as_array().unwrap().len(), 15);
    assert_eq!(got["fields"][8], json!(["user", "UWORKFLOW"]));
    assert_eq!(got["fields"][14], json!(["skip_dlp_user_warning", "false"]));
    assert!(inspect_resend(&original, &t, &c, &s, &original.inspect(), None).is_err());
    let mut altered = original.inspect();
    altered["fields"][5][1] = json!("other-id");
    assert!(inspect_resend(&retry, &t, &c, &s, &altered, None).is_err());
    altered = original.inspect();
    altered["fields"][3][1] = json!("[]");
    assert!(inspect_resend(&retry, &t, &c, &s, &altered, None).is_err());
}
#[test]
fn resend_cannot_change_credentials_trigger_or_original_plain_content() {
    let (c, s, t, m) = setup();
    let original = m.request(&c, &s).unwrap();
    let retry = m.resend_request(&c, &s, None).unwrap();
    let mut foreign = s.clone();
    foreign.token = "other-synthetic-token".into();
    assert!(inspect_resend(&retry, &t, &c, &foreign, &original.inspect(), None).is_err());
    let mut trigger = t.clone();
    trigger["name"] = json!("different original");
    assert!(inspect_resend(&retry, &trigger, &c, &s, &original.inspect(), None).is_err());
    let wrong = protocol::Message::new(&t, "different", raw::DEST, m.client_msg_id(), m.local_ts())
        .unwrap();
    assert!(
        inspect_resend(
            &wrong.resend_request(&c, &s, None).unwrap(),
            &t,
            &c,
            &s,
            &original.inspect(),
            None
        )
        .is_err()
    );
}
#[test]
fn explicit_held_retry_releases_actual_sender_once_without_extra_server_row() {
    let model = Model::for_case("lost-response".into());
    let (c, s, _, m) = setup();
    let wire = m.request(&c, &s).unwrap().inspect();
    let raw = reply(&m, false);
    for serial in 1..=3 {
        dispatch(&model, serial, &wire, &Value::Null, &raw);
    }
    let mut d = Data::new();
    let (tx, mut rx) = oneshot::channel();
    d.held = Some(tx);
    model.0.lock().unwrap().resend = Some(d);
    assert_eq!(
        release(&model).unwrap(),
        json!({"intent":1,"serial":1,"receiverRetired":false,"delivered":true})
    );
    assert_eq!(rx.try_recv(), Ok(()));
    assert!(release(&model).is_err());
    assert_eq!(model.snapshot()["applied"], json!([raw["message"]]));
    assert_eq!(model.snapshot()["resend"]["settlements"], json!([]));
}
#[test]
fn dropped_retry_receiver_has_honest_nondelivery_and_settlement() {
    let model = Model::for_case("lost-response".into());
    let mut d = Data::new();
    let (tx, rx) = oneshot::channel();
    drop(rx);
    d.held = Some(tx);
    model.0.lock().unwrap().resend = Some(d);
    assert_eq!(release(&model).unwrap()["receiverRetired"], true);
    assert_eq!(
        model.snapshot()["resend"]["releases"][0]["delivered"],
        false
    );
    drop(Settled {
        model: model.clone(),
        returned: false,
    });
    assert_eq!(
        model.snapshot()["resend"]["settlements"],
        json!([{"intent":1,"serial":1,"returned":false}])
    );
    assert_eq!(model.snapshot()["resend"]["attempts"], json!([]));
}
