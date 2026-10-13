//! Stock sendDraftFiles finalizes privately, then files.share owns the one message.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn complete_and_share(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    thread: &str,
    text: &str,
    client_msg_id: &str,
    files: &[serde_json::Value],
    canceled: &AtomicBool,
) -> Result<CompletedUpload, UploadMessageError> {
    if canceled.load(Ordering::Relaxed) {
        return Err(UploadMessageError::BeforeDelivery(Error::UploadCanceled));
    }
    // No destination or caption on this call: completion cannot publish a reply.
    let response = transport
        .execute_draft_mutation(client.rest_form(
            workspace,
            "files.completeUploadExternal",
            vec![("files", serde_json::to_string(files).unwrap())],
        ))
        .await
        .map_err(UploadMessageError::BeforeDelivery)?;
    api_rejection(&response).map_err(UploadMessageError::BeforeDelivery)?;
    let complete = completed_upload(&response, files)
        .map_err(|error| UploadMessageError::BeforeDelivery(error.error().clone()))?;
    if canceled.load(Ordering::Relaxed) {
        return Err(UploadMessageError::BeforeDelivery(Error::UploadCanceled));
    }
    let request = share_request(
        client,
        workspace,
        channel,
        thread,
        text,
        client_msg_id,
        &complete.file_ids,
    );
    let response = transport
        .execute_draft_mutation(request)
        .await
        .map_err(|error| {
            if matches!(&error, Error::Api(code) if code=="draft_mutation_not_sent_offline") {
                UploadMessageError::BeforeDelivery(error)
            } else {
                UploadMessageError::Completion(error).retain_finalized(&complete)
            }
        })?;
    trace_share_response(&response, files);
    api_rejection(&response)
        .map_err(|error| UploadMessageError::Completion(error).retain_finalized(&complete))?;
    let receipt =
        completed_upload(&response, files).map_err(|error| error.retain_finalized(&complete))?;
    if !valid_receipt(&response, channel, thread, client_msg_id) {
        return Err(UploadMessageError::Completion(Error::Transport(
            "Slack did not confirm the broadcast file message identity".into(),
        ))
        .retain_finalized(&complete));
    }
    Ok(receipt)
}

fn trace_share_response(response: &serde_json::Value, staged: &[serde_json::Value]) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED
        .get_or_init(|| std::env::var("SUPER_PLATINUM_UPLOAD_RECEIPT_TRACE").as_deref() == Ok("1"))
    {
        return;
    }
    // This opt-in diagnostic must work in packaged launches without a tracing
    // subscriber. The allowlisted structure deliberately contains no payload.
    eprintln!(
        "file share receipt structure: {}",
        share_structure(response, staged)
    );
}
fn share_structure(
    response: &serde_json::Value,
    staged: &[serde_json::Value],
) -> serde_json::Value {
    use serde_json::{Value, json};
    let fields: serde_json::Map<String, Value> = [
        "ok",
        "error",
        "files",
        "file",
        "file_msg_ts",
        "channel",
        "thread_ts",
        "client_msg_id",
    ]
    .into_iter()
    .filter_map(|key| {
        response.get(key).map(|value| {
            let kind = match value {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            };
            (key.into(), json!(kind))
        })
    })
    .collect();
    json!({"fields":fields,"ok":response.get("ok").and_then(Value::as_bool),
        "valid_message_timestamp":response.get("file_msg_ts").is_some_and(|value|valid_receipt(&json!({"file_msg_ts":value}),"","","")),
        "files_count":response.get("files").and_then(Value::as_array).map(Vec::len),
        "exact_file_ids":completed_upload(response,staged).is_ok()})
}

fn api_rejection(value: &serde_json::Value) -> Result<(), Error> {
    if value.get("ok").and_then(serde_json::Value::as_bool) == Some(false)
        && let Some(code) = value.get("error").and_then(serde_json::Value::as_str)
    {
        return Err(Error::Api(code.to_owned()));
    }
    Ok(())
}

fn share_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    thread: &str,
    text: &str,
    client_msg_id: &str,
    files: &[String],
) -> PreparedRequest {
    let mut fields = vec![
        ("files", files.join(",")),
        ("channel", channel.into()),
        ("thread_ts", thread.into()),
        ("broadcast", "true".into()),
        ("client_msg_id", client_msg_id.into()),
        ("resharing_aware", "true".into()),
        ("skip_dlp_user_warning", "false".into()),
        ("from_share_modal", "false".into()),
    ];
    if !text.is_empty() {
        let key = crate::drafts::DraftKey {
            team: workspace.team_id.clone(),
            channel: channel.into(),
            thread: Some(thread.into()),
        };
        match crate::drafts::codec::encode(&key, text) {
            Ok(snapshot) => {
                fields.push(("blocks", serde_json::to_string(&snapshot.blocks).unwrap()))
            }
            // The source files.share wrapper also accepts legacy message text as
            // comment when there are no blocks. Preserve unsupported syntax.
            Err(_) => fields.push(("comment", text.into())),
        }
    }
    client.rest_form(workspace, "files.share", fields)
}

fn valid_receipt(
    value: &serde_json::Value,
    channel: &str,
    thread: &str,
    client_msg_id: &str,
) -> bool {
    let ts = value.get("file_msg_ts").and_then(serde_json::Value::as_str);
    ts.is_some_and(|ts| {
        ts.split_once('.').is_some_and(|(s, f)| {
            !s.is_empty()
                && s.bytes().all(|b| b.is_ascii_digit())
                && s.parse::<u64>().is_ok_and(|s| s > 0)
                && f.len() == 6
                && f.bytes().all(|b| b.is_ascii_digit())
        })
    }) && [
        ("channel", channel),
        ("thread_ts", thread),
        ("client_msg_id", client_msg_id),
    ]
    .iter()
    .all(|(key, expected)| {
        value
            .get(key)
            .is_none_or(|actual| actual.as_str() == Some(*expected))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn broadcast_file_request_preserves_exact_destination_identity_and_formatting() {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic-token".into(),
        };
        let req = share_request(
            &SlackClient::default(),
            &workspace,
            "C1",
            "100.000001",
            "A *bold* reply 🦊",
            "fixture-message",
            &["F1".into(), "F2".into()],
        );
        assert_eq!(
            url::Url::parse(&req.url).unwrap().path(),
            "/api/files.share"
        );
        let super::super::super::super::client::RequestBody::Form(fields) = req.body else {
            panic!("form expected")
        };
        let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
        for (k, v) in [
            ("channel", "C1"),
            ("thread_ts", "100.000001"),
            ("broadcast", "true"),
            ("client_msg_id", "fixture-message"),
            ("files", "F1,F2"),
        ] {
            assert_eq!(fields[k], v);
        }
        assert!(!fields.contains_key("initial_comment"));
        let blocks: serde_json::Value = serde_json::from_str(&fields["blocks"]).unwrap();
        assert_eq!(
            blocks[0]["elements"][0]["elements"][1]["style"]["bold"],
            true
        );
    }

    #[test]
    fn broadcast_file_receipt_rejects_wrong_identity_or_missing_message() {
        assert!(valid_receipt(
            &json!({"file_msg_ts":"101.000001"}),
            "C1",
            "100.000001",
            "id1"
        ));
        for bad in [
            json!({}),
            json!({"file_msg_ts":""}),
            json!({"file_msg_ts":"101.000001","channel":"C2"}),
            json!({"file_msg_ts":"101.000001","thread_ts":"102.000001"}),
            json!({"file_msg_ts":"101.000001","client_msg_id":"id2"}),
        ] {
            assert!(!valid_receipt(&bad, "C1", "100.000001", "id1"));
        }
    }
    #[test]
    fn explicit_file_rejection_remains_distinct_from_ambiguous_receipt() {
        let error = api_rejection(&json!({"ok":false,"error":"not_in_channel"})).unwrap_err();
        assert!(matches!(&error,Error::Api(code) if code=="not_in_channel"));
        assert!(!UploadMessageError::Completion(error).delivery_uncertain());
        assert!(
            completed_upload(&json!({"ok":true}), &[json!({"id":"F1"})])
                .unwrap_err()
                .delivery_uncertain()
        );
    }

    #[tokio::test]
    async fn private_completion_precedes_single_broadcast_and_rejection_stops_publication() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for outcome in ["success", "reject-completion", "sparse-share", "lost-share"] {
            let reject_completion = outcome == "reject-completion";
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let count = if reject_completion { 1 } else { 2 };
                for index in 0..count {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut bytes = Vec::new();
                    loop {
                        let mut part = [0; 4096];
                        let n = stream.read(&mut part).await.unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&part[..n]);
                        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            let header = String::from_utf8_lossy(&bytes[..end]);
                            let len = header
                                .lines()
                                .find_map(|l| {
                                    l.split_once(':')
                                        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            if bytes.len() >= end + 4 + len {
                                break;
                            }
                        }
                    }
                    let request = String::from_utf8(bytes).unwrap();
                    let body = request.split("\r\n\r\n").nth(1).unwrap();
                    let fields: std::collections::HashMap<_, _> =
                        url::form_urlencoded::parse(body.as_bytes())
                            .into_owned()
                            .collect();
                    let response=if index==0 {
                        assert!(request.starts_with("POST /api/files.completeUploadExternal?"));
                        for key in ["channel","channel_id","thread_ts","initial_comment","comment"] {assert!(!fields.contains_key(key));}
                        if reject_completion {json!({"ok":false,"error":"invalid_arguments"})}else{json!({"ok":true,"files":[{"id":"F1"}]})}
                    } else {
                        assert!(request.starts_with("POST /api/files.share?"));
                        assert_eq!(fields["broadcast"],"true");assert_eq!(fields["thread_ts"],"100.000001");assert_eq!(fields["client_msg_id"],"id1");
                        if outcome=="lost-share" { drop(stream); continue; }
                        if outcome=="sparse-share" { json!({"ok":true,"file_msg_ts":"101.000001"}) }
                        else { json!({"ok":true,"files":[{"id":"F1"}],"file_msg_ts":"101.000001","channel":"C1","thread_ts":"100.000001","client_msg_id":"id1"}) }
                    }.to_string();
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();
                }
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                        .await
                        .is_err()
                );
            });
            let workspace = WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: format!("http://{address}"),
                token: "synthetic-token".into(),
            };
            let result = complete_and_share(
                &Transport::new("synthetic-cookie").unwrap(),
                &SlackClient::default(),
                &workspace,
                "C1",
                "100.000001",
                "A *bold* caption",
                "id1",
                &[json!({"id":"F1"})],
                &AtomicBool::new(false),
            )
            .await;
            if reject_completion {
                assert!(
                    matches!(result,Err(UploadMessageError::BeforeDelivery(Error::Api(code))) if code=="invalid_arguments")
                );
            } else if matches!(outcome, "sparse-share" | "lost-share") {
                let error = result.unwrap_err();
                assert!(error.delivery_uncertain());
                assert_eq!(error.finalized_files(), Some(["F1".to_owned()].as_slice()));
            } else {
                assert_eq!(result.unwrap().file_ids, vec!["F1"]);
            }
            server.await.unwrap();
        }
    }
    #[test]
    fn structural_trace_never_contains_response_values_or_dynamic_keys() {
        let structure = share_structure(
            &json!({"ok":true,"files":[{"id":"secret-id","name":"secret-title"}],"file_msg_ts":"101.000001","channel":"secret-channel","unexpected-sensitive-key":"secret-content","error":null}),
            &[json!({"id":"secret-id"})],
        );
        assert_eq!(structure["exact_file_ids"], true);
        assert_eq!(structure["valid_message_timestamp"], true);
        assert_eq!(structure["files_count"], 1);
        assert!(!structure.to_string().contains("secret"));
        assert!(!structure.to_string().contains("unexpected-sensitive-key"));
    }
}
