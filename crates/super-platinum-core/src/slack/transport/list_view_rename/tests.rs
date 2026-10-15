use super::super::{Duration, Error, Health, Transport};
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
        slack_lists::{Document, PersonalTaskLists, ViewRename, ViewRenameAttempt},
    };
    let document=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[],"views":[{"id":"V1","name":"Old","type":"table","grouping":{}}]}})).unwrap();
    let personal = PersonalTaskLists::from_response(
        &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    let intent =
        ViewRename::capture(&document, "V1", "New title", Some("full"), Some(&personal)).unwrap();
    let permit = ViewRenameAttempt::new(intent)
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
    api::list_view_rename(&SlackClient::default(), &session, permit)
}
#[tokio::test]
async fn retired_offline_and_wrong_request_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let mut bad = Vec::new();
    for path in [
        "/api/lists.cells.update",
        "/api/lists.views.update/",
        "/api/lists.views.update?cursor=guessed",
    ] {
        let mut req = request(address);
        req.url = format!("http://{address}{path}");
        bad.push(req);
    }
    let mut extra = request(address);
    if let RequestBody::Form(fields) = &mut extra.body {
        fields.push(("type".into(), "number".into()));
    }
    bad.push(extra);
    let mut get = request(address);
    get.method = "GET";
    bad.push(get);
    let mut foreign = request(address);
    foreign.url = "https://foreign.example/api/lists.views.update".into();
    bad.push(foreign);
    for req in bad {
        assert!(matches!(
            transport.execute_list_view_rename(req, || true).await,
            Err(ListViewRenameFailure::NotRequested(_))
        ));
    }
    assert!(matches!(
        transport
            .execute_list_view_rename(request(address), || false)
            .await,
        Err(ListViewRenameFailure::NotRequested(_))
    ));
    let mut calls = 0;
    assert!(matches!(
        transport
            .execute_list_view_rename(request(address), || {
                calls += 1;
                calls == 1
            })
            .await,
        Err(ListViewRenameFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_view_rename(request(address), || true)
            .await,
        Err(ListViewRenameFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn http_denial_or_retry_after_never_replays_and_retains_known_limit() {
    for status in [200, 429, 503] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            stream.read(&mut bytes).await.unwrap();
            stream.write_all(format!("HTTP/1.1 {status} Result\r\nContent-Length: 11\r\nRetry-After: 7\r\nConnection: close\r\n\r\n{{\"ok\":true}}").as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_view_rename(request(address), || true)
            .await;
        match result {
            Err(ListViewRenameFailure::Unconfirmed(Error::RateLimited {
                retry_after_secs: Some(7),
            })) if status == 429 => {}
            Err(ListViewRenameFailure::Unconfirmed(Error::HttpStatus {
                status: actual,
                retry_after_secs: Some(7),
            })) if actual == status => {}
            other => panic!("unexpected {other:?}"),
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn duplicate_json_and_lost_response_remain_unconfirmed() {
    for body in [Some("{\"ok\":false,\"ok\":true}"), None] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            stream.read(&mut bytes).await.unwrap();
            if let Some(body) = body {
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_view_rename(request(address), || true)
            .await;
        assert!(matches!(result, Err(ListViewRenameFailure::Unconfirmed(_))));
        server.await.unwrap();
    }
}
#[tokio::test]
async fn original_response_is_retained_after_current_owner_retires() {
    use std::sync::atomic::AtomicBool;
    let live = Arc::new(AtomicBool::new(true));
    let retired = live.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        stream.read(&mut bytes).await.unwrap();
        retired.store(false, Ordering::SeqCst);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .await
            .unwrap();
    });
    let result = Transport::new("fixture-cookie")
        .unwrap()
        .execute_list_view_rename(request(address), || live.load(Ordering::SeqCst))
        .await
        .unwrap();
    assert_eq!(result, serde_json::json!({"ok":true}));
    assert!(!live.load(Ordering::SeqCst));
    server.await.unwrap();
}

#[tokio::test]
async fn neither_wrapper_accepts_the_other_operations_form() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(matches!(
        transport
            .execute_list_column_rename(request(address), || true)
            .await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    let mut column = request(address);
    column.url = format!("http://{address}/api/lists.columns.update");
    column.body = RequestBody::Form(vec![
        ("token".into(), "fixture-token".into()),
        ("list_id".into(), "F1".into()),
        ("key".into(), "title".into()),
        ("name".into(), "New".into()),
        ("options".into(), "{\"format\":\"text\"}".into()),
        ("backfill_missing_cells_with_default".into(), "true".into()),
        ("_x_reason".into(), "update-column".into()),
    ]);
    assert!(matches!(
        transport.execute_list_view_rename(column, || true).await,
        Err(ListViewRenameFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn redirects_declared_and_streamed_bounds_and_nested_duplicates_never_retry() {
    for (headers,body) in [
        ("HTTP/1.1 302 Found\r\nLocation: /api/files.info\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),Vec::new()),
        ("HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n".to_owned(),Vec::new()),
        ("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_owned(),vec![b' ';1048577]),
        ("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_owned(),br#"{"ok":true,"file":{"id":"F1","id":"F2"}}"#.to_vec()),
    ]{
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();
        let server=tokio::spawn(async move{
            let(mut stream,_)=listener.accept().await.unwrap();let mut bytes=[0;4096];stream.read(&mut bytes).await.unwrap();
            let _=stream.write_all(headers.as_bytes()).await;let _=stream.write_all(&body).await;drop(stream);
            assert!(tokio::time::timeout(Duration::from_millis(80),listener.accept()).await.is_err());
        });
        let result=Transport::new("fixture-cookie").unwrap().execute_list_view_rename(request(addr),||true).await;
        assert!(matches!(result,Err(ListViewRenameFailure::Unconfirmed(_))));server.await.unwrap();
    }
}
