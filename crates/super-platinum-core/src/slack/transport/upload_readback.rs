//! Explicit delivery checks expose Retry-After to their retained local owner.
use super::*;
impl Transport {
    pub async fn execute_upload_readback(&self, req: PreparedRequest) -> Result<Value, Error> {
        let allowed = req.method == "POST"
            && matches!(req.body, RequestBody::Form(_))
            && url::Url::parse(&req.url).is_ok_and(|url| {
                let origin = url.scheme() == "https"
                    && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
                    && url.port_or_known_default() == Some(443);
                #[cfg(test)]
                let origin =
                    origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
                origin
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none()
                    && matches!(
                        url.path(),
                        "/api/conversations.history" | "/api/conversations.replies"
                    )
            });
        if !allowed {
            return Err(Error::Api("invalid_upload_readback_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Delivery check not sent while offline".into(),
            ));
        }
        self.execute_once_using(&req, false, self.single_attempt_client()?)
            .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn delivery_check_reports_429_once_and_rejects_writes_and_offline() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let req = |path: &str| PreparedRequest {
            method: "POST",
            url: format!("http://{address}/api/{path}"),
            headers: vec![],
            body: RequestBody::Form(vec![]),
        };
        let t = Transport::new("synthetic-cookie").unwrap();
        for path in [
            "files.share",
            "chat.postMessage",
            "conversations.replies/extra",
        ] {
            assert!(matches!(
                t.execute_upload_readback(req(path)).await,
                Err(Error::Api(_))
            ));
        }
        t.set_health(Health::Offline);
        assert!(matches!(
            tokio::time::timeout(
                Duration::from_millis(100),
                t.execute_upload_readback(req("conversations.history"))
            )
            .await
            .unwrap(),
            Err(Error::Offline(_))
        ));
        t.set_health(Health::Online);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).await.unwrap();
            assert!(
                String::from_utf8_lossy(&bytes[..n]).starts_with("POST /api/conversations.replies")
            );
            socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 30\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        assert!(matches!(
            t.execute_upload_readback(req("conversations.replies"))
                .await,
            Err(Error::RateLimited {
                retry_after_secs: Some(30)
            })
        ));
        server.await.unwrap();
    }
}
