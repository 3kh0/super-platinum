//! Durable draft receipts own retries. The HTTP layer must never replay a write.
use super::*;

impl Transport {
    pub async fn execute_draft_mutation(&self, req: PreparedRequest) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api("unsupported_draft_mutation_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Api("draft_mutation_not_sent_offline".into()));
        }
        let http = self.single_attempt_client()?;
        self.execute_once_using(&req, true, http)
            .await
            .map_err(|error| match error {
                Error::Offline(_) => {
                    Error::Transport("draft mutation response lost after dispatch".into())
                }
                error => error,
            })
    }
}

fn allowed(req: &PreparedRequest) -> bool {
    req.method == "POST"
        && url::Url::parse(&req.url).is_ok_and(|url| {
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
                    "/api/drafts.create"
                        | "/api/drafts.update"
                        | "/api/drafts.delete"
                        | "/api/chat.postMessage"
                        | "/api/files.share"
                        | "/api/files.completeUploadExternal"
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn request(address: std::net::SocketAddr, path: &str) -> PreparedRequest {
        PreparedRequest {
            method: "POST",
            url: format!("http://{address}{path}"),
            headers: vec![],
            body: RequestBody::Form(vec![]),
        }
    }

    #[tokio::test]
    async fn rejects_offline_and_unrelated_requests_before_dispatch() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let transport = Transport::new("synthetic-cookie").unwrap();
        transport.set_health(Health::Offline);
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            transport.execute_draft_mutation(request(address, "/api/chat.postMessage")),
        )
        .await
        .unwrap();
        assert!(matches!(result,Err(Error::Api(code)) if code=="draft_mutation_not_sent_offline"));
        transport.set_health(Health::Online);
        for path in [
            "/api/chat.postMessage/",
            "/api/files.delete",
            "/api/drafts.list",
        ] {
            assert!(matches!(
                transport
                    .execute_draft_mutation(request(address, path))
                    .await,
                Err(Error::Api(_))
            ));
        }
        let mut get = request(address, "/api/chat.postMessage");
        get.method = "GET";
        assert!(matches!(
            transport.execute_draft_mutation(get).await,
            Err(Error::Api(_))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn never_replays_redirect_errors_or_lost_responses() {
        for status in [
            "302 Found",
            "429 Too Many Requests",
            "503 Service Unavailable",
            "lost",
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = [0; 4096];
                let count = stream.read(&mut bytes).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&bytes[..count])
                        .starts_with("POST /api/chat.postMessage")
                );
                if status != "lost" {
                    let response = format!(
                        "HTTP/1.1 {status}\r\nRetry-After: 9\r\nLocation: http://{address}/api/chat.postMessage\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                drop(stream);
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err()
                );
            });
            let transport = Transport::new("synthetic-cookie").unwrap();
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                transport.execute_draft_mutation(request(address, "/api/chat.postMessage")),
            )
            .await
            .unwrap();
            match status {
                "lost" => assert!(matches!(result, Err(Error::Transport(_)))),
                "429 Too Many Requests" => assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(9)
                    })
                )),
                _ => assert!(matches!(result, Err(Error::HttpStatus { .. }))),
            }
            server.await.unwrap();
        }
    }
}
