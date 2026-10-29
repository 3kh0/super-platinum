use super::*;
use people_pages::AccountType;
fn typed(address: std::net::SocketAddr, choice: AccountType) -> Request {
    let session = WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "UONE".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    };
    let mut r = people_pages::request_for_account_type(
        &SlackClient::default(),
        &session,
        Input {
            query: "Alex",
            page: 1,
            query_session: uuid::Uuid::from_u128(1),
            search_session: uuid::Uuid::from_u128(2),
            sort: Sort::Relevant,
            hide_deactivated: true,
            primary_channel: "CONE",
        },
        choice,
    )
    .unwrap();
    r.inner.url = format!("http://{address}/api/search.modules.people");
    r.destination = "127.0.0.1".into();
    r
}
#[tokio::test]
async fn account_type_actual_multipart_omission_exact_common_values_and_closed_transport_rejects_unknown_duplicates()
 {
    tokio::time::timeout(Duration::from_secs(10),async {
        for (choice,expected) in [(AccountType::Any,None),(AccountType::RegularMembers,Some("0,1,2,3,7")),(AccountType::Deactivated,Some("6"))] {
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address=listener.local_addr().unwrap();let request=typed(address,choice);
            let server=async {
                let (mut socket,_)=listener.accept().await.unwrap();
                let bytes=read(&mut socket).await;let wire=String::from_utf8(bytes).unwrap();
                let (headers,body)=wire.split_once("\r\n\r\n").unwrap();
                assert!(headers.lines().next().unwrap().starts_with("POST /api/search.modules.people "));
                assert!(headers.to_ascii_lowercase().contains("cookie: d=synthetic-cookie"));
                let content=headers.lines().find_map(|line|{let(k,v)=line.split_once(':')?;k.eq_ignore_ascii_case("content-type").then_some(v.trim())}).unwrap();
                let boundary=content.split(';').find_map(|p|p.trim().strip_prefix("boundary=")).unwrap().trim_matches('"');
                let parts:Vec<_>=body.split(&format!("--{boundary}")).collect();
                let fields:Vec<_>=parts[1..parts.len()-1].iter().map(|p|{
                    let (h,v)=p.strip_prefix("\r\n").unwrap().split_once("\r\n\r\n").unwrap();
                    (h.strip_prefix("Content-Disposition: form-data; name=\"").unwrap().strip_suffix('"').unwrap(),v.strip_suffix("\r\n").unwrap())
                }).collect();
                assert_eq!(fields.first(),Some(&("token","synthetic-token")));
                assert_eq!(fields.iter().filter(|(k,_)|*k=="account_type").map(|(_,v)|*v).collect::<Vec<_>>(),expected.into_iter().collect::<Vec<_>>());
                assert_eq!(fields.iter().find(|(k,_)|*k=="hide_deactivated_users").unwrap().1,"1");
                assert_eq!(fields.iter().find(|(k,_)|*k=="search_only_team").unwrap().1,"");
                assert!(!body.contains("filename="));
                reply(&mut socket,"200 OK",r#"{"ok":true,"items":[],"pagination":{"page":1,"page_count":0,"total_count":0}}"# ,"").await;
            };
            let transport=Transport::new("synthetic-cookie").unwrap();
            let (result,())=tokio::join!(transport.execute_people_page_current(&request,||true,|_|{}),server);assert!(result.is_ok());
            for value in ["0,1,2,3,4,5,6,7,8,9,10","4,5,8","10","6,6",""] {
                let mut invalid=typed(address,AccountType::Any);
                let RequestBody::Form(fields)=&mut invalid.inner.body else {unreachable!()};fields.push(("account_type".into(),value.into()));
                assert!(!allowed(&invalid));
            }
            let mut duplicate=typed(address,AccountType::RegularMembers);
            let RequestBody::Form(fields)=&mut duplicate.inner.body else {unreachable!()};fields.push(("account_type".into(),"0,1,2,3,7".into()));assert!(!allowed(&duplicate));
        }
    }).await.expect("bounded synthetic loopback only");
}
