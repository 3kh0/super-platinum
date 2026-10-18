//! One explicit file-Share recipient opening; never shares or retries a file.
use super::*;

impl Transport {
    pub async fn execute_file_share_direct_open(
        &self,
        req: PreparedRequest,
    ) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api("unsupported_file_share_direct_open".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Api("file_share_direct_not_sent_offline".into()));
        }
        self.execute_once_using(&req, true, self.single_attempt_client()?)
            .await
            .map_err(|error| match error {
                Error::Offline(_) => {
                    Error::Transport("Direct conversation response lost after dispatch".into())
                }
                other => other,
            })
    }
}
fn allowed(req: &PreparedRequest) -> bool {
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    req.method == "POST"
        && fields.len() == 3
        && fields[0].0 == "token"
        && !fields[0].1.is_empty()
        && fields[1].0 == "users"
        && (9..=64).contains(&fields[1].1.len())
        && fields[1].1.starts_with('U')
        && fields[1]
            .1
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && fields[2] == ("return_im".into(), "true".into())
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
                && url.query_pairs().all(|(key, _)| {
                    matches!(
                        key.as_ref(),
                        "_x_id"
                            | "_x_csid"
                            | "_x_desktop_ia"
                            | "_x_foreground"
                            | "_x_frontend_build_type"
                            | "_x_gantry"
                            | "_x_num_retries"
                            | "_x_version_ts"
                            | "fp"
                            | "slack_route"
                    )
                })
                && url.fragment().is_none()
                && url.path() == "/api/conversations.open"
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn request(address: std::net::SocketAddr) -> PreparedRequest {
        PreparedRequest {
            method: "POST",
            url: format!("http://{address}/api/conversations.open"),
            headers: vec![],
            body: RequestBody::Form(vec![
                ("token".into(), "synthetic".into()),
                ("users".into(), "U00000002".into()),
                ("return_im".into(), "true".into()),
            ]),
        }
    }
    #[tokio::test]
    async fn offline_and_wrong_user_forms_never_dispatch() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let transport = Transport::new("synthetic").unwrap();
        for bad in ["U00000002,U00000003", "D00000001", "U0", "U0000\n002"] {
            let mut req = request(address);
            let RequestBody::Form(fields) = &mut req.body else {
                unreachable!()
            };
            fields[1].1 = bad.into();
            assert!(matches!(
                transport.execute_file_share_direct_open(req).await,
                Err(Error::Api(_))
            ));
        }
        transport.set_health(Health::Offline);
        assert!(
            matches!(transport.execute_file_share_direct_open(request(address)).await,Err(Error::Api(e)) if e=="file_share_direct_not_sent_offline")
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn one_open_attempt_never_replays_unknown_rate_limit_or_redirect() {
        for status in [
            "lost",
            "429 Too Many Requests",
            "503 Service Unavailable",
            "302 Found",
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 4096];
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&buffer[..n])
                        .starts_with("POST /api/conversations.open")
                );
                if status != "lost" {
                    stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 9\r\nLocation: http://{address}/api/conversations.open\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
                }
                drop(stream);
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err()
                );
            });
            let transport = Transport::new("synthetic").unwrap();
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                transport.execute_file_share_direct_open(request(address)),
            )
            .await
            .unwrap();
            if status.starts_with("429") {
                assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(9)
                    })
                ));
            } else {
                assert!(result.is_err());
            }
            server.await.unwrap();
        }
    }
}
