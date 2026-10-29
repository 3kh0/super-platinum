use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api::people_pages::Sort},
};
use std::cell::Cell;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn prepared(address: std::net::SocketAddr, sort: Sort) -> Request {
    let session = WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "UONE".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    };
    let mut request =
        people_sort_preference::request(&SlackClient::default(), &session, sort).unwrap();
    // Test-only redirection of a validated closed request, never a public setter.
    request.inner.url = format!("http://{address}/api/users.prefs.set");
    request.destination = "127.0.0.1".into();
    request
}
async fn read(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() < 64 * 1024);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = header
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() == end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
fn fields(wire: &str) -> Vec<(String, String)> {
    let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
    assert!(
        headers
            .lines()
            .next()
            .unwrap()
            .starts_with("POST /api/users.prefs.set ")
    );
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("cookie: d=synthetic-cookie")
    );
    let content_type = headers
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-type").then_some(v.trim())
        })
        .unwrap();
    assert!(content_type.starts_with("multipart/form-data;"));
    let boundary = content_type
        .split(';')
        .find_map(|p| p.trim().strip_prefix("boundary="))
        .unwrap()
        .trim_matches('"');
    let delimiter = format!("--{boundary}");
    let parts: Vec<_> = body.split(&delimiter).collect();
    assert_eq!(parts.first(), Some(&""));
    assert_eq!(parts.last(), Some(&"--\r\n"));
    parts[1..parts.len() - 1]
        .iter()
        .map(|p| {
            let (header, value) = p
                .strip_prefix("\r\n")
                .unwrap()
                .split_once("\r\n\r\n")
                .unwrap();
            assert!(!header.contains("filename="));
            let name = header
                .strip_prefix("Content-Disposition: form-data; name=\"")
                .unwrap()
                .strip_suffix('"')
                .unwrap();
            (name.into(), value.strip_suffix("\r\n").unwrap().into())
        })
        .collect()
}
async fn reply(socket: &mut tokio::net::TcpStream, status: &str, body: &str, extra: &str) {
    socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",body.len()).as_bytes()).await.unwrap();
}

#[tokio::test]
async fn actual_saved_sort_multipart_is_closed_single_attempt_and_observes_retired_rate_without_false_ack()
 {
    tokio::time::timeout(Duration::from_secs(40),async {
        for (sort,value) in [(Sort::Relevant,"relevant"),(Sort::Name,"name"),(Sort::NameDescending,"name_desc")] {
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let request=prepared(listener.local_addr().unwrap(),sort);let transport=Transport::new("synthetic-cookie").unwrap();let observations=Cell::new(0);
            assert!(!super::super::is_block_interaction_request(&request.inner),"Calendar's generic block whitelist remains closed to this preference");
            assert!(matches!(transport.execute_block_interaction(request.inner.clone()).await,Err(Error::Api(code)) if code=="unsupported_block_interaction_request"));
            assert!(matches!(transport.execute_list_default_view_preference(request.inner.clone(),||true).await,Err(super::super::ListColumnRenameFailure::NotRequested(_))),"the unrelated List preference wrapper is not widened");

            let server=async {
                let(mut socket,_)=listener.accept().await.unwrap();let wire=read(&mut socket).await;
                assert_eq!(fields(&wire),vec![("token".into(),"synthetic-token".into()),("name".into(),"search_people_sort".into()),("value".into(),value.into()),("_x_reason".into(),"prefs-api/setUserPrefByApi".into())]);
                reply(&mut socket,"200 OK",r#"{"ok":true,"value":"not a canonical echo"}"#,"").await;
            };
            let client=transport.execute_people_sort_preference_current(&request,||true,|r|{assert!(r.is_ok());observations.set(observations.get()+1);});
            let(_,result)=tokio::join!(server,client);assert!(result.is_ok());assert_eq!(observations.get(),1);
            transport.set_health(Health::Offline);
            assert!(matches!(transport.execute_people_sort_preference_current(&request,||true,|_|panic!("no attempted result while offline")).await,Err(PeopleSortPreferenceFailure::NotRequested(Error::Offline(_)))));
            transport.set_health(Health::Online);
            assert!(matches!(transport.execute_people_sort_preference_current(&request,||false,|_|panic!("no attempted result after retirement")).await,Err(PeopleSortPreferenceFailure::NotRequested(_))));
        }
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let request=prepared(listener.local_addr().unwrap(),Sort::Name);
        let current=Cell::new(true);let observed=Cell::new(false);let transport=Transport::new("synthetic-cookie").unwrap();
        let server=async {let(mut socket,_)=listener.accept().await.unwrap();let _=read(&mut socket).await;current.set(false);reply(&mut socket,"429 Too Many Requests",r#"{"ok":false}"#,"Retry-After: 7\r\n").await;};
        let client=transport.execute_people_sort_preference_current(&request,||current.get(),|r|{assert!(!current.get());assert!(matches!(r,Err(Error::RateLimited{retry_after_secs:Some(7)})));observed.set(true);});
        let(_,result)=tokio::join!(server,client);assert!(observed.get());assert!(matches!(result,Err(PeopleSortPreferenceFailure::Unconfirmed(Error::RateLimited{retry_after_secs:Some(7)}))));
        let target=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let request=prepared(listener.local_addr().unwrap(),Sort::Relevant);
        let location=format!("Location: http://{}/forbidden\r\n",target.local_addr().unwrap());
        let server=async {let(mut socket,_)=listener.accept().await.unwrap();let _=read(&mut socket).await;reply(&mut socket,"302 Found","{}",&location).await;};
        let client=transport.execute_people_sort_preference_current(&request,||true,|_|{});
        let(_,result)=tokio::join!(server,client);assert!(matches!(result,Err(PeopleSortPreferenceFailure::Unconfirmed(Error::HttpStatus{status:302,..}))));
        assert!(tokio::time::timeout(Duration::from_millis(50),target.accept()).await.is_err(),"redirect was not followed");
        for lost_reply in [true,false] {
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let request=prepared(listener.local_addr().unwrap(),Sort::Name);
            let transport=Transport::new("synthetic-cookie").unwrap();let observations=Cell::new(0);
            let server=async {
                let(mut socket,_)=listener.accept().await.unwrap();let _=read(&mut socket).await;
                if !lost_reply {reply(&mut socket,"503 Service Unavailable","{}","").await;}
                // A complete submitted body followed by a dropped reply cannot
                // be classified as known not transmitted.
            };
            let client=transport.execute_people_sort_preference_current(&request,||true,|r|{assert!(r.is_err());observations.set(observations.get()+1);});
            let(_,result)=tokio::join!(server,client);
            if lost_reply {assert!(matches!(result,Err(PeopleSortPreferenceFailure::Unconfirmed(Error::Transport(_)))));}
            else {assert!(matches!(result,Err(PeopleSortPreferenceFailure::Unconfirmed(Error::HttpStatus{status:503,..}))));}
            assert_eq!(observations.get(),1);
            assert!(tokio::time::timeout(Duration::from_millis(50),listener.accept()).await.is_err(),"neither dropped reply nor503 is replayed");
        }
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let request=prepared(listener.local_addr().unwrap(),Sort::NameDescending);
        let transport=Transport::new("synthetic-cookie").unwrap();let current=Cell::new(true);let observed=Cell::new(false);
        let(stop,wait)=tokio::sync::oneshot::channel();
        let server=async {
            let(mut socket,_)=listener.accept().await.unwrap();let _=read(&mut socket).await;
            current.set(false);wait.await.unwrap();drop(socket);
        };
        let client=async {
            let result=transport.execute_people_sort_preference_current(&request,||current.get(),|r|{assert!(!current.get());assert!(matches!(r,Err(Error::Transport(_))));observed.set(true);}).await;
            stop.send(()).unwrap();result
        };
        let(_,result)=tokio::join!(server,client);
        assert!(observed.get(),"the actual production deadline is observed after retirement");
        assert!(matches!(result,Err(PeopleSortPreferenceFailure::Unconfirmed(Error::Transport(_)))),"a timed-out dispatched save is unconfirmed, never NotRequested");
    }).await.expect("all local wire joins are bounded");
}
