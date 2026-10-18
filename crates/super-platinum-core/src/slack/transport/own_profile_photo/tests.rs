use super::*;
use crate::{config::WorkspaceSession, slack::SlackClient};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() < 128 * 1024);
        if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() == end + 4 + length {
                return bytes;
            }
        }
    }
}
fn workspace(address: std::net::SocketAddr) -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: format!("http://{address}"),
        token: "synthetic".into(),
    }
}
#[tokio::test]
async fn real_delete_builder_is_single_attempt_with_redirect_rate_and_lost_reply_retained() {
    for (status, body) in [
        ("200 OK", r#"{"ok":true}"#),
        ("429 Too Many Requests", ""),
        ("503 Service Unavailable", ""),
        ("302 Found", ""),
        ("lost", ""),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let bytes = request(&mut stream).await;
            let wire = String::from_utf8_lossy(&bytes);
            assert!(wire.starts_with("POST /api/users.deletePhoto?"));
            assert!(wire.contains("_x_reason=edit_profile_delete_photo"));
            if status != "lost" {
                stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 7\r\nLocation: http://{address}/again\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("synthetic")
            .unwrap()
            .mutate_own_photo(photo::remove(&SlackClient::default(), &workspace(address)))
            .await;
        match status {
            "200 OK" => assert!(result.is_ok()),
            "lost" => assert!(matches!(result, Err(Error::Transport(_)))),
            "429 Too Many Requests" => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(7)
                })
            )),
            _ => assert!(matches!(result, Err(Error::HttpStatus { .. }))),
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn actual_staged_upload_streams_exact_image_and_parses_prepared_identity_once() {
    use ::image::ImageEncoder;
    let mut image = Vec::new();
    ::image::codecs::png::PngEncoder::new(&mut image)
        .write_image(
            &vec![31; 36 * 36 * 4],
            36,
            36,
            ::image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let expected = image.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let bytes = request(&mut stream).await;
        let end = bytes.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
        let headers = std::str::from_utf8(&bytes[..end]).unwrap();
        assert!(headers.starts_with("POST /api/users.preparePhoto?"));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("multipart/form-data; boundary=")
        );
        let body = &bytes[end + 4..];
        let text = String::from_utf8_lossy(body);
        for key in ["name=\"token\"", "name=\"_x_reason\"", "name=\"image\""] {
            assert_eq!(text.matches(key).count(), 1);
        }
        assert!(text.contains("\r\n\r\nEditProfileModal\r\n"));
        assert!(!text.contains("name=\"user\""));
        let start = body
            .windows(expected.len())
            .position(|w| w == expected)
            .unwrap();
        assert!(body[start + expected.len()..].starts_with(b"\r\n--super-platinum-photo-"));
        let response = r#"{"ok":true,"id":"12345","url":"https://files.slack.com/prepared.png"}"#;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let prepared = Transport::new("synthetic")
        .unwrap()
        .prepare_own_photo(photo::prepare(
            &SlackClient::default(),
            &workspace(address),
            photo::Image::parse(image).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(prepared.url(), "https://files.slack.com/prepared.png");
    server.await.unwrap();
}
#[test]
fn multipart_contains_only_exact_staged_fields_and_original_file_bytes() {
    use ::image::ImageEncoder;
    let mut bytes = Vec::new();
    ::image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &vec![0; 36 * 36 * 4],
            36,
            36,
            ::image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let w = workspace("127.0.0.1:1234".parse().unwrap());
    let image = photo::Image::parse(bytes.clone()).unwrap();
    let upload = photo::prepare(&SlackClient::default(), &w, image);
    let body = multipart(&upload, "test-boundary");
    let text = String::from_utf8_lossy(&body);
    for key in ["name=\"token\"", "name=\"_x_reason\"", "name=\"image\""] {
        assert_eq!(text.matches(key).count(), 1);
    }
    assert!(text.contains("EditProfileModal"));
    assert!(!text.contains("name=\"user\""));
    assert!(body.windows(bytes.len()).any(|w| w == bytes));
    assert!(body.ends_with(b"\r\n--test-boundary--\r\n"));
}
#[tokio::test]
async fn disconnected_photo_transport_never_obtains_a_client() {
    let w = workspace("127.0.0.1:1".parse().unwrap());
    let transport = Transport::disconnected_fixture("synthetic");
    assert!(matches!(
        transport
            .mutate_own_photo(photo::remove(&SlackClient::default(), &w))
            .await,
        Err(Error::Offline(_))
    ));
}
