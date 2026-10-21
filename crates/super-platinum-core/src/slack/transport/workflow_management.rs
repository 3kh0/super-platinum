//! Workflow metadata writes and export initiation retain responses and never replay.
use super::*;
mod collaborators;
mod confirmation_add;
mod copy_permission;
mod delete;
mod publish;
mod run_permission;
mod unpublish;

impl Transport {
    /// A plan's caller owns policy, exact draft, preflight and per-slot outcomes.
    /// Only a local refusal proves no dispatch. HTTP/error/partial responses must
    /// remain in that caller's attempt book; this function never reconciles them.
    pub async fn execute_workflow_management(
        &self,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if !allowed(&request) {
            return Err(Error::Api("workflow_management_not_sent_method".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Workflow metadata was not sent while offline".into(),
            ));
        }
        let http = self
            .single_attempt_client()
            .map_err(|_| Error::Api("workflow_management_not_sent_client".into()))?;
        // Same structured response reader as execute_once_with_response, using
        // the existing client that disables both redirects and internal retries.
        // The ordinary media-capable client follows redirects, including 307/308.
        self.execute_once_using(&request, true, http)
            .await
            .map_err(|error| match error {
                // The shared reader marks health offline for timeouts/resets as
                // well as failed connects. Once dispatch starts that verdict
                // cannot establish that this mutation did not reach Slack.
                Error::Offline(_) => {
                    Error::Transport("Workflow metadata response was lost after dispatch".into())
                }
                error => error,
            })
    }
}

fn allowed(request: &PreparedRequest) -> bool {
    request.method == "POST"
        && matches!(request.body, RequestBody::Form(_))
        && url::Url::parse(&request.url).is_ok_and(|url| {
            let origin = url.scheme() == "https"
                && url
                    .host_str()
                    .is_some_and(|host| host.ends_with(".slack.com"))
                && url.port_or_known_default() == Some(443);
            #[cfg(test)]
            let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
            origin
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && matches!(
                    url.path(),
                    "/api/functions.workflows.update"
                        | "/api/workflows.triggers.update"
                        | "/api/workflows.activity.export"
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn request(method: &str) -> PreparedRequest {
        let workspace = crate::config::WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        };
        if method == "workflows.activity.export" {
            use crate::block_actions::workflow_management::{
                ActivityExport, ActivityFilters, ActivityQuery,
            };
            let query = ActivityQuery::new("WF1", ActivityFilters::default()).unwrap();
            return ActivityExport::new(&query, Some(&serde_json::json!("on")))
                .unwrap()
                .take_request()
                .unwrap()
                .request(&crate::slack::SlackClient::default(), &workspace);
        }
        crate::slack::SlackClient::default().rest_form(
            &workspace,
            method,
            vec![
                ("workflow_id", "WF1".into()),
                ("title", "Fixture".into()),
                ("_x_reason", "update-hermes-workflow-metadata".into()),
            ],
        )
    }

    #[test]
    fn export_executor_accepts_exact_plan_without_widening_origin_method_or_body() {
        let good = request("workflows.activity.export");
        assert!(allowed(&good));
        for url in [
            "https://example.test/api/workflows.activity.export",
            "https://fixture.slack.com.evil.test/api/workflows.activity.export",
            "http://fixture.slack.com/api/workflows.activity.export",
            "https://fixture.slack.com:444/api/workflows.activity.export",
            "https://user@fixture.slack.com/api/workflows.activity.export",
            "https://fixture.slack.com/api/workflows.activity.export#fragment",
            "https://fixture.slack.com/api/workflows.activity.export.extra",
            "https://fixture.slack.com/api/workflows.activity.export/download",
            "https://fixture.slack.com/api/workflows.activity.list",
        ] {
            let mut bad = good.clone();
            bad.url = url.into();
            assert!(!allowed(&bad), "{url}");
        }
        let mut bad = good.clone();
        bad.method = "GET";
        assert!(!allowed(&bad));
        let mut bad = good;
        bad.body = RequestBody::Json(serde_json::json!({"workflow_id":"WF1","filters":{}}));
        assert!(!allowed(&bad));
    }

    #[test]
    fn metadata_executor_refuses_other_methods_origins_and_paths() {
        for endpoint in ["functions.workflows.update", "workflows.triggers.update"] {
            assert!(allowed(&request(endpoint)));
        }
        for endpoint in [
            "functions.workflows.getLatest",
            "chat.postMessage",
            "functions.workflows.update.extra",
            "functions.workflows.delete",
            "functions.workflows.steps.run",
        ] {
            assert!(!allowed(&request(endpoint)));
        }
        let good = request("functions.workflows.update");
        for url in [
            "https://example.test/api/functions.workflows.update",
            "https://fixture.slack.com.evil.test/api/functions.workflows.update",
            "http://fixture.slack.com/api/functions.workflows.update",
            "https://fixture.slack.com:444/api/functions.workflows.update",
            "https://user@fixture.slack.com/api/functions.workflows.update",
            "https://fixture.slack.com/api/functions.workflows.update#fragment",
        ] {
            let mut wrong = good.clone();
            wrong.url = url.into();
            assert!(!allowed(&wrong));
        }
        let mut wrong = good.clone();
        wrong.method = "GET";
        assert!(!allowed(&wrong));
        let mut wrong = good;
        wrong.body = RequestBody::Json(serde_json::json!({}));
        assert!(!allowed(&wrong));
    }

    #[tokio::test]
    async fn metadata_executor_preserves_raw_success_rejection_and_ambiguous_objects() {
        for endpoint in ["functions.workflows.update", "workflows.activity.export"] {
            for body in [
                r#"{"ok":true,"retained_unknown":{"revision":73}}"#,
                r#"{"ok":false,"error":"not_allowed","partial":{"workflow":true}}"#,
                r#"{"ok":true,"error":"unexpected"}"#,
                r#"{"unrecognized":true}"#,
            ] {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let server = tokio::spawn(async move {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut bytes = [0; 4096];
                    assert!(stream.read(&mut bytes).await.unwrap() > 0);
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                    drop(stream);
                    assert!(
                        tokio::time::timeout(Duration::from_millis(100), listener.accept())
                            .await
                            .is_err()
                    );
                });
                let mut req = request(endpoint);
                req.url = format!("http://{address}/api/{endpoint}");
                let actual = tokio::time::timeout(
                    Duration::from_secs(2),
                    Transport::new("synthetic")
                        .unwrap()
                        .execute_workflow_management(req),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(actual, serde_json::from_str::<Value>(body).unwrap());
                server.await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn metadata_executor_never_retries_redirects_or_lost_responses() {
        for endpoint in ["workflows.triggers.update", "workflows.activity.export"] {
            for status in [
                "307 Temporary Redirect",
                "308 Permanent Redirect",
                "429 Too Many Requests",
                "503 Service Unavailable",
                "lost",
                "invalid-json",
            ] {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let server = tokio::spawn(async move {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut bytes = [0; 4096];
                    assert!(stream.read(&mut bytes).await.unwrap() > 0);
                    if status != "lost" {
                        let (status, body) = if status == "invalid-json" {
                            ("200 OK", "not json")
                        } else {
                            (status, "")
                        };
                        stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 7\r\nLocation: http://{address}/api/workflows.triggers.update\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                    }
                    drop(stream);
                    assert!(
                        tokio::time::timeout(Duration::from_millis(150), listener.accept())
                            .await
                            .is_err()
                    );
                });
                let mut req = request(endpoint);
                req.url = format!("http://{address}/api/{endpoint}");
                let result = tokio::time::timeout(
                    Duration::from_secs(2),
                    Transport::new("synthetic")
                        .unwrap()
                        .execute_workflow_management(req),
                )
                .await
                .unwrap();
                match status {
                    "429 Too Many Requests" => assert!(matches!(
                        result,
                        Err(Error::RateLimited {
                            retry_after_secs: Some(7)
                        })
                    )),
                    "lost" | "invalid-json" => assert!(matches!(result, Err(Error::Transport(_)))),
                    _ => assert!(matches!(result, Err(Error::HttpStatus { .. }))),
                }
                server.await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn known_offline_or_invalid_request_is_unsent_without_a_hold() {
        for endpoint in ["functions.workflows.update", "workflows.activity.export"] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let mut req = request(endpoint);
            req.url = format!("http://{address}/api/{endpoint}");
            let transport = Transport::new("synthetic").unwrap();
            transport.set_health(Health::Offline);
            assert!(matches!(
                tokio::time::timeout(
                    Duration::from_millis(100),
                    transport.execute_workflow_management(req.clone())
                )
                .await
                .unwrap(),
                Err(Error::Offline(_))
            ));
            transport.set_health(Health::Online);
            req.method = "GET";
            assert!(matches!(transport.execute_workflow_management(req).await,
            Err(Error::Api(code)) if code == "workflow_management_not_sent_method"));
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        }
    }
}
