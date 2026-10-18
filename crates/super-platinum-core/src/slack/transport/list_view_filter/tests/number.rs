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
async fn actual_numeric_capture_builder_transport_preserve_six_operators_and_lexical_values() {
    use NumberFilterOperator::*;
    for (operator, value) in [
        (Equal, Some("-0")),
        (NotEqual, Some("12.50")),
        (LessThan, Some("-2.75")),
        (GreaterThan, Some("1e2")),
        (IsEmpty, None),
        (IsNotEmpty, None),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let document=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[{"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":0}}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[],"filters":[],"info_column_filters":[]}]}})).unwrap();
        let personal = PersonalTaskLists::from_response(
            &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .unwrap();
        let intent = ViewFilter::capture(
            &document,
            "V1",
            ViewFilterChoice::Number {
                column_id: "C_NUMBER".into(),
                operator,
                value: value.map(str::to_owned),
            },
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
            ("filters".into(),serde_json::json!([{"key":"estimate","operator":operator.as_str(),"values":value.into_iter().collect::<Vec<_>>()}]).to_string()),
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

#[tokio::test]
async fn malformed_numeric_wire_never_dispatches_and_current_guard_still_applies() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let valid = serde_json::json!({"key":"estimate","operator":"equal","values":["12.50"]});
    let mut rules = Vec::new();
    for values in [
        serde_json::json!([]),
        serde_json::json!([12.5]),
        serde_json::json!(["2", "3"]),
        serde_json::json!(["NaN"]),
        serde_json::json!(["1e309"]),
        serde_json::json!([" 2"]),
        serde_json::json!(["0x10"]),
        serde_json::json!(["0".repeat(129)]),
    ] {
        let mut rule = valid.clone();
        rule["values"] = values;
        rules.push(rule);
    }
    for operator in ["less_than_or_equal", "greater_than_or_equal", "is_empty"] {
        let mut rule = valid.clone();
        rule["operator"] = Value::String(operator.into());
        rules.push(rule);
    }
    for (key, value) in [
        ("column_id", serde_json::json!("C_NUMBER")),
        ("opaque", serde_json::json!(true)),
    ] {
        let mut rule = valid.clone();
        rule[key] = value;
        rules.push(rule);
    }
    // Membership is valid for Select IDs, including numeric-looking IDs. The
    // raw wire has no column type; Number rejection is checked against schema below.
    for rule in rules {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[3].1 = serde_json::json!([rule]).to_string();
        assert!(!allowed(&r), "malformed rule was admitted: {rule}");
        assert!(matches!(
            transport.execute_list_view_filter(r, || true).await,
            Err(ListViewFilterFailure::NotRequested(_))
        ));
    }
    let mut r = request(address);
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    fields[3].1 = serde_json::json!([valid]).to_string();
    assert!(matches!(
        transport.execute_list_view_filter(r, || false).await,
        Err(ListViewFilterFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn numeric_column_cannot_capture_or_replace_membership_rules() {
    use crate::slack_lists::SelectFilterOperator;
    let document = Document::parse(serde_json::json!({
        "id":"F1","mode":"list","access":"write","list_metadata":{
            "schema":[{"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":0}}],
            "views":[{"id":"V1","name":"Custom","type":"table","columns":[],"filters":[],"info_column_filters":[]}]
        }
    })).unwrap();
    let personal = PersonalTaskLists::from_response(
        &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    for operator in [
        SelectFilterOperator::Contains,
        SelectFilterOperator::DoesNotContain,
    ] {
        assert!(
            ViewFilter::capture(
                &document,
                "V1",
                ViewFilterChoice::Select {
                    column_id: "C_NUMBER".into(),
                    operator,
                    values: vec!["12.50".into()],
                },
                Some("full"),
                Some(&personal),
            )
            .is_err(),
            "Number schema must not mint a membership permit"
        );
        let mut raw = document.raw.clone();
        raw["list_metadata"]["views"][0]["filters"] = serde_json::json!([
            {"key":"estimate","operator":operator.as_str(),"values":["12.50"]}
        ]);
        let loaded = Document::parse(raw).unwrap();
        assert_eq!(ViewFilter::current(&loaded, "V1"), None);
        assert!(
            ViewFilter::capture(
                &loaded,
                "V1",
                ViewFilterChoice::None,
                Some("full"),
                Some(&personal),
            )
            .is_err(),
            "unknown Number membership must not be silently replaced"
        );
    }
}
