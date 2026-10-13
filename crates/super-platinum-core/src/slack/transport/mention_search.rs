//! One read attempt for a replaceable mention query. Its owner retains 429 state.
use super::*;

impl Transport {
    pub async fn execute_mention_search(&self, req: PreparedRequest) -> Result<Value, Error> {
        let allowed = req.method == "POST"
            && url::Url::parse(&req.url).is_ok_and(|url| {
                let parts: Vec<_> = url.path().split('/').collect();
                parts.len() == 5
                    && parts[1] == "cache"
                    && !parts[2].is_empty()
                    && parts[3] == "users"
                    && parts[4] == "search"
            });
        if !allowed {
            return Err(Error::Api("invalid_mention_search_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Mention search not sent while offline".into(),
            ));
        }
        self.execute_once(&req).await
    }
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
            body: RequestBody::Json(serde_json::json!({"query":"synthetic"})),
        }
    }

    #[tokio::test]
    async fn mention_429_returns_deadline_without_retry_and_offline_never_waits() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let transport = Transport::new("synthetic-cookie").unwrap();
        transport.set_health(Health::Offline);
        assert!(matches!(
            tokio::time::timeout(
                Duration::from_millis(100),
                transport.execute_mention_search(request(address, "/cache/T1/users/search"))
            )
            .await
            .unwrap(),
            Err(Error::Offline(_))
        ));
        transport.set_health(Health::Online);
        for path in [
            "/api/chat.postMessage",
            "/cache/T1/users/search/",
            "/cache/T1/channels/search",
        ] {
            assert!(matches!(
                transport
                    .execute_mention_search(request(address, path))
                    .await,
                Err(Error::Api(_))
            ));
        }
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let count = stream.read(&mut bytes).await.unwrap();
            assert!(
                String::from_utf8_lossy(&bytes[..count]).starts_with("POST /cache/T1/users/search")
            );
            stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 30\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            transport.execute_mention_search(request(address, "/cache/T1/users/search")),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            Err(Error::RateLimited {
                retry_after_secs: Some(30)
            })
        ));
        server.await.unwrap();
    }
}
