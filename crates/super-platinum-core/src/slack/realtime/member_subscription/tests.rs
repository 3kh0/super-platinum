use super::*;
use tokio::sync::mpsc;

fn socket(capacity: usize) -> (Connection, mpsc::Receiver<String>) {
    let (tx, rx) = mpsc::channel(capacity);
    (Connection::from_sender(tx), rx)
}
fn ids() -> Vec<String> {
    vec!["UMEMBER01".into()]
}
fn count(connection: &Connection) -> usize {
    connection.subscriptions.0.lock().unwrap().pending.len()
}

#[tokio::test]
async fn local_enqueue_is_not_completion_and_reply_retains_exact_request() {
    let (connection, mut wire) = socket(2);
    let mut pending = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    assert!(matches!(
        pending.receiver.as_mut().unwrap().try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    let request: Value = serde_json::from_str(&wire.recv().await.unwrap()).unwrap();
    assert_eq!(request, *pending.request());
    assert_eq!(request["type"], "subscription_update");
    assert_eq!(request["subtype"], "user_change_subscribe_request");
    assert_eq!(request["ids"], json!(ids()));
    assert!(request["id"].as_u64().unwrap() > 16_384);
    let reply = json!({"reply_to":request["id"],"ok":true,"opaque":{"kept":[1,2]}});
    assert!(connection.clone().receive_member_subscription_reply(&reply));
    let receipt = pending.wait().await.unwrap();
    assert_eq!(receipt.request(), &request);
    assert_eq!(receipt.reply(), &reply);
    assert_eq!(count(&connection), 0);
    assert!(!connection.receive_member_subscription_reply(&reply));
}

#[tokio::test]
async fn interleaved_subscribe_and_unsubscribe_match_only_their_own_reply() {
    let (connection, _wire) = socket(3);
    let first = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    let mut second = connection
        .request_member_subscription(&ids(), false)
        .unwrap();
    assert_ne!(first.request()["id"], second.request()["id"]);
    assert_eq!(
        second.request()["subtype"],
        "user_change_unsubscribe_request"
    );
    assert!(
        connection.receive_member_subscription_reply(
            &json!({"reply_to":first.request()["id"],"ok":true})
        )
    );
    assert!(matches!(
        second.receiver.as_mut().unwrap().try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    first.wait().await.unwrap();
    assert_eq!(count(&connection), 1);
    assert!(
        connection.receive_member_subscription_reply(
            &json!({"reply_to":second.request()["id"],"ok":true})
        )
    );
    second.wait().await.unwrap();
    assert_eq!(count(&connection), 0);
}

#[tokio::test]
async fn foreign_socket_cannot_settle_original_request() {
    let (original, _wire) = socket(1);
    let (replacement, _other_wire) = socket(1);
    let pending = original.request_member_subscription(&ids(), true).unwrap();
    let reply = json!({"reply_to":pending.request()["id"],"ok":true});
    assert!(!replacement.receive_member_subscription_reply(&reply));
    assert_eq!(count(&original), 1);
    assert!(original.receive_member_subscription_reply(&reply));
    pending.wait().await.unwrap();
}

#[tokio::test]
async fn explicit_false_rejects_and_preserves_gateway_refusal_evidence() {
    for (error, reason, code) in [
        (json!("not_allowed"), "not_allowed", None),
        (
            json!({"msg":"too_many_ids","code":12,"detail":"retained"}),
            "too_many_ids",
            Some(12),
        ),
        (json!({"code":15}), "unknown_error", Some(15)),
        (Value::Null, "unknown_error", None),
    ] {
        let (connection, _wire) = socket(1);
        let pending = connection
            .request_member_subscription(&ids(), true)
            .unwrap();
        let request = pending.request().clone();
        let reply = json!({"reply_to":request["id"],"ok":false,"error":error});
        assert!(connection.receive_member_subscription_reply(&reply));
        let Err(MemberSubscriptionError::Rejected(receipt)) = pending.wait().await else {
            panic!("expected server refusal")
        };
        assert_eq!(receipt.request(), &request);
        assert_eq!(receipt.reply(), &reply);
        assert_eq!(receipt.reason(), reason);
        assert_eq!(receipt.gateway_error_code(), code);
    }
}

#[tokio::test]
async fn only_boolean_false_is_source_rejection() {
    for ok in [
        None,
        Some(Value::Null),
        Some(json!(true)),
        Some(json!(0)),
        Some(json!("false")),
    ] {
        let (connection, _wire) = socket(1);
        let pending = connection
            .request_member_subscription(&ids(), true)
            .unwrap();
        let mut reply = json!({"reply_to":pending.request()["id"]});
        if let Some(ok) = ok {
            reply["ok"] = ok;
        }
        assert!(connection.receive_member_subscription_reply(&reply));
        assert_eq!(pending.wait().await.unwrap().reply(), &reply);
    }
}

#[tokio::test]
async fn socket_close_retires_every_original_waiter_and_prevents_new_admission() {
    let (connection, _wire) = socket(2);
    let close = connection.subscriptions.close_on_drop();
    let first = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    let second = connection
        .request_member_subscription(&ids(), false)
        .unwrap();
    let late = json!({"reply_to":first.request()["id"],"ok":true});
    drop(close);
    assert_eq!(
        first.wait().await,
        Err(MemberSubscriptionError::SocketClosed)
    );
    assert_eq!(
        second.wait().await,
        Err(MemberSubscriptionError::SocketClosed)
    );
    assert!(!connection.receive_member_subscription_reply(&late));
    assert!(matches!(
        connection.request_member_subscription(&ids(), true),
        Err(MemberSubscriptionError::SocketClosed)
    ));
    assert_eq!(count(&connection), 0);
}

#[tokio::test]
async fn completed_ack_survives_subsequent_socket_close() {
    let (connection, _wire) = socket(1);
    let close = connection.subscriptions.close_on_drop();
    let pending = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    assert!(
        connection.receive_member_subscription_reply(
            &json!({"reply_to":pending.request()["id"],"ok":true})
        )
    );
    drop(close);
    pending.wait().await.unwrap();
}

#[test]
fn dropping_one_waiter_does_not_unsubscribe_or_retire_its_sibling() {
    let (connection, mut wire) = socket(3);
    let first = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    let second = connection
        .request_member_subscription(&ids(), false)
        .unwrap();
    let late = json!({"reply_to":first.request()["id"],"ok":true});
    drop(first);
    assert_eq!(count(&connection), 1);
    assert!(!connection.receive_member_subscription_reply(&late));
    assert!(wire.try_recv().is_ok());
    assert!(wire.try_recv().is_ok());
    assert!(wire.try_recv().is_err());
    drop(second);
    assert_eq!(count(&connection), 0);
}

#[tokio::test]
async fn cancelled_wait_future_releases_only_local_pending_entry() {
    let (connection, _wire) = socket(1);
    let pending = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(1), pending.wait())
            .await
            .is_err()
    );
    assert_eq!(count(&connection), 0);
}

#[test]
fn full_and_closed_queues_leave_no_pending_completion_claim() {
    let (connection, wire) = socket(1);
    connection.try_send("already queued".into()).unwrap();
    assert!(matches!(
        connection.request_member_subscription(&ids(), true),
        Err(MemberSubscriptionError::QueueFull)
    ));
    assert_eq!(count(&connection), 0);
    drop(wire);
    assert!(matches!(
        connection.request_member_subscription(&ids(), true),
        Err(MemberSubscriptionError::SocketClosed)
    ));
    assert_eq!(count(&connection), 0);
}

#[test]
fn invalid_request_and_malformed_reply_never_consume_valid_pending_entry() {
    let (connection, _wire) = socket(1);
    for bad in [
        vec![],
        vec!["U".into()],
        vec!["UMEMBER01".into(); 2],
        (0..101).map(|n| format!("U{n:08}")).collect(),
    ] {
        assert!(matches!(
            connection.request_member_subscription(&bad, true),
            Err(MemberSubscriptionError::InvalidRequest)
        ));
        assert_eq!(count(&connection), 0);
    }
    let pending = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    for bad in [
        json!({}),
        json!({"reply_to":0}),
        json!({"reply_to":-1}),
        json!({"reply_to":1.5}),
        json!({"reply_to":true}),
        json!({"reply_to":[]}),
        json!({"reply_to":MAX_SAFE_ID+1}),
    ] {
        assert!(!connection.receive_member_subscription_reply(&bad));
        assert_eq!(count(&connection), 1);
    }
    drop(pending);
}

#[test]
fn ping_and_subscription_ids_share_a_monotonic_namespace() {
    let (connection, _wire) = socket(2);
    let before = next_message_id().unwrap();
    let first = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    let between = next_message_id().unwrap();
    let second = connection
        .request_member_subscription(&ids(), false)
        .unwrap();
    assert!(before < first.request()["id"].as_u64().unwrap());
    assert!(first.request()["id"].as_u64().unwrap() < between);
    assert!(between < second.request()["id"].as_u64().unwrap());
}

#[tokio::test]
async fn canonical_decimal_reply_key_matches_but_noncanonical_key_does_not() {
    let (connection, _wire) = socket(1);
    let pending = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    let id = pending.request()["id"].as_u64().unwrap();
    assert!(
        !connection
            .receive_member_subscription_reply(&json!({"reply_to":format!("0{id}"),"ok":true}))
    );
    assert!(
        connection.receive_member_subscription_reply(&json!({"reply_to":id.to_string(),"ok":true}))
    );
    pending.wait().await.unwrap();
    assert_eq!(reply_id(&json!(16385.0)), Some(16385));
}

#[test]
fn unowned_reply_cannot_enter_product_event_reducer() {
    assert!(
        super::super::parse_event(
            r#"{"reply_to":16385,"type":"user_change","user":{"id":"UMEMBER01"}}"#
        )
        .is_none()
    );
    assert!(
        super::super::parse_event(r#"{"type":"user_change","user":{"id":"UMEMBER01"}}"#).is_some()
    );
    assert!(
        super::super::parse_event(
            r#"{"reply_to":"unknown","type":"user_change","user":{"id":"UMEMBER01"}}"#
        )
        .is_none()
    );
}

#[tokio::test]
async fn receiver_reply_custody_does_not_keep_outbound_channel_alive() {
    let (connection, mut wire) = socket(1);
    let subscriptions = connection.subscriptions.clone();
    let close = subscriptions.close_on_drop();
    let pending = connection
        .request_member_subscription(&ids(), true)
        .unwrap();
    wire.recv().await.unwrap();
    drop(connection);
    // This is the receiver-loop ownership layout: reply state and its close
    // guard remain alive while all external outbound owners are gone.
    assert_eq!(
        wire.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    );
    drop(close);
    assert_eq!(
        pending.wait().await,
        Err(MemberSubscriptionError::SocketClosed)
    );
}
