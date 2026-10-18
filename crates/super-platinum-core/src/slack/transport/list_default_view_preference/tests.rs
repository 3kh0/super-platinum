use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{Document, ListDefaultViewPreference, ListDefaultViewPreferenceAttempt},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr, read: bool) -> PreparedRequest {
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "fixture-token".into(),
    };
    if read {
        return api::list_default_view_preference_read(&SlackClient::default(), &session);
    }
    let doc=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[],"views":[{"id":"V1","name":"Delivery","type":"board"}]}})).unwrap();
    let before =
        serde_json::json!({"ok":true,"prefs":{"lists_default_view":"{\"F_OTHER\":\"V_OTHER\"}"}});
    let intent = ListDefaultViewPreference::capture(&before, &doc, "V1").unwrap();
    let permit = ListDefaultViewPreferenceAttempt::new(intent)
        .admit(&before, &doc)
        .unwrap();
    api::list_default_view_preference(&SlackClient::default(), &session, permit)
}
#[tokio::test]
async fn actual_builder_and_shared_executor_preserve_exact_read_and_write_forms() {
    for read in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let request = request(listener.local_addr().unwrap(), read);
        assert!(allowed(&request, read));
        assert!(!allowed(&request, !read));
        let RequestBody::Form(fields) = &request.body else {
            panic!()
        };
        if !read {
            assert_eq!(
                serde_json::from_str::<Value>(&fields[2].1).unwrap(),
                serde_json::json!({"F1":"V1","F_OTHER":"V_OTHER"})
            );
        }
        let expected = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .finish();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut all = Vec::new();
            let start = loop {
                let mut b = [0; 2048];
                let n = socket.read(&mut b).await.unwrap();
                assert!(n > 0);
                all.extend_from_slice(&b[..n]);
                if let Some(at) = all.windows(4).position(|s| s == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let header = String::from_utf8(all[..start].to_vec()).unwrap();
            assert!(header.starts_with(if read {
                "POST /api/users.prefs.get"
            } else {
                "POST /api/users.prefs.set"
            }));
            let length: usize = header
                .lines()
                .find_map(|l| {
                    l.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse().unwrap())
                })
                .unwrap();
            while all.len() < start + length {
                let mut b = [0; 2048];
                let n = socket.read(&mut b).await.unwrap();
                assert!(n > 0);
                all.extend_from_slice(&b[..n]);
            }
            assert_eq!(&all[start..start + length], expected.as_bytes());
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let transport = Transport::new("fixture-cookie").unwrap();
        let result = if read {
            transport
                .execute_list_default_view_preference_read(request, || true)
                .await
        } else {
            transport
                .execute_list_default_view_preference(request, || true)
                .await
        };
        assert_eq!(result.unwrap(), serde_json::json!({"ok":true}));
        server.await.unwrap();
    }
}
#[test]
fn alternate_preferences_extra_fields_and_ambiguous_json_never_pass_validator() {
    for read in [false, true] {
        let valid = request("127.0.0.1:9".parse().unwrap(), read);
        let mut wrong = valid.clone();
        let RequestBody::Form(fields) = &mut wrong.body else {
            panic!()
        };
        fields[1].1 = "other_pref".into();
        assert!(!allowed(&wrong, read));
        let mut wrong = valid.clone();
        let RequestBody::Form(fields) = &mut wrong.body else {
            panic!()
        };
        fields.push(("prefs".into(), "other_pref".into()));
        assert!(!allowed(&wrong, read));
        if !read {
            for value in ["{}", r#"{"F1":"V1","F1":"V2"}"#, r#"{"F1":true}"#] {
                let mut wrong = valid.clone();
                let RequestBody::Form(fields) = &mut wrong.body else {
                    panic!()
                };
                fields[2].1 = value.into();
                assert!(!allowed(&wrong, false));
            }
        }
    }
}
