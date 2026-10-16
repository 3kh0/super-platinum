use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        Document, NumberFilterOperator, PersonalTaskLists, ViewFilter, ViewFilterAttempt,
        ViewFilterChoice,
    },
};

#[tokio::test]
async fn compound_capture_builder_transport_send_complete_array_once() {
    use NumberFilterOperator::*;
    for (operator, value) in [(LessThan, Some("12.50"))] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let document=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[{"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":0}},{"id":"C_CHECK","key":"done","name":"Done","type":"checkbox"}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[],"filters":[],"info_column_filters":[]}]}})).unwrap();
        let personal = PersonalTaskLists::from_response(
            &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .unwrap();
        let intent = ViewFilter::capture(
            &document,
            "V1",
            ViewFilterChoice::All(vec![
                ViewFilterChoice::Checkbox {
                    column_id: "C_CHECK".into(),
                    checked: true,
                },
                ViewFilterChoice::Number {
                    column_id: "C_NUMBER".into(),
                    operator,
                    value: value.map(str::to_owned),
                },
            ]),
            Some("full"),
            Some(&personal),
        )
        .unwrap();
        let mut attempt = ViewFilterAttempt::new(intent);
        let permit = attempt
            .admit(&document, Some("full"), Some(&personal))
            .unwrap();
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: format!("http://{address}"),
            token: "fixture-token".into(),
        };
        let prepared = api::list_view_filter(&SlackClient::default(), &session, permit);
        let RequestBody::Form(fields) = &prepared.body else {
            panic!()
        };
        assert_eq!(fields,&vec![
            ("token".into(),"fixture-token".into()),("list_id".into(),"F1".into()),("id".into(),"V1".into()),
            ("filters".into(),serde_json::json!([{"key":"done","operator":"is_not_empty","values":[]},{"key":"estimate","operator":operator.as_str(),"values":value.into_iter().collect::<Vec<_>>()}]).to_string()),
            ("info_column_filters".into(),"[]".into()),("_x_reason".into(),"update-view".into())]);
        let expected = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .finish();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut all = Vec::new();
            let body_start = loop {
                let mut b = [0; 2048];
                let n = socket.read(&mut b).await.unwrap();
                assert!(n > 0);
                all.extend_from_slice(&b[..n]);
                if let Some(at) = all.windows(4).position(|s| s == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let headers = String::from_utf8(all[..body_start].to_vec()).unwrap();
            assert!(headers.starts_with("POST /api/lists.views.update"));
            let length = headers
                .lines()
                .find_map(|l| {
                    l.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            while all.len() < body_start + length {
                let mut b = [0; 2048];
                let n = socket.read(&mut b).await.unwrap();
                assert!(n > 0);
                all.extend_from_slice(&b[..n]);
            }
            assert_eq!(&all[body_start..body_start + length], expected.as_bytes());
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        assert_eq!(
            Transport::new("fixture-cookie")
                .unwrap()
                .execute_list_view_filter(prepared, || true)
                .await
                .unwrap(),
            serde_json::json!({"ok":true})
        );
        server.await.unwrap();
        assert!(attempt.finish(false));
        assert_eq!(
            attempt.phase(),
            crate::slack_lists::ViewFilterPhase::Unconfirmed
        );
        assert!(
            attempt
                .admit(&document, Some("full"), Some(&personal))
                .is_err()
        );
    }
}

#[test]
fn later_rule_and_whole_vector_validation() {
    let address = "127.0.0.1:9".parse().unwrap();
    let first = serde_json::json!({"key":"one","operator":"less_than","values":["12.50"]});
    let test = |rules: Value| {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[3].1 = rules.to_string();
        allowed(&r)
    };
    assert!(test(
        serde_json::json!([first.clone(),{"key":"two","operator":"is_empty","values":[]}])
    ));
    for bad in [
        serde_json::json!({"key":"two","operator":"equal","values":[2]}),
        serde_json::json!({"key":"two","operator":"equal","values":["NaN"]}),
        serde_json::json!({"key":"two","operator":"equal","values":["1"],"column_id":"C2"}),
        first.clone(),
    ] {
        assert!(!test(serde_json::json!([first.clone(), bad])));
    }
    for (count, expected) in [(8, true), (9, false)] {
        let rows = (0..count)
            .map(|i| serde_json::json!({"key":format!("k{i}"),"operator":"is_empty","values":[]}))
            .collect::<Vec<_>>();
        assert_eq!(test(serde_json::json!(rows)), expected);
    }
}
