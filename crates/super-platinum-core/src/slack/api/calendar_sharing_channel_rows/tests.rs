use super::*;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

fn request(marker: Option<&str>) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: "https://edgeapi.slack.com/cache/T1/channels/search".into(),
        headers: vec![],
        body: crate::slack::client::RequestBody::Json(
            json!({"query":" literal ","count":100,"check_membership":true,"marker":marker}),
        ),
    }
}
#[tokio::test]
async fn sharing_raw100_precedes_decode_and_unused_marker_but_not_the_512_array_guard() {
    for case in 0..6 {
        let calls = Cell::new(0);
        let result=fetch_calendar_sharing_channel_rows_with(|marker|Ok(Some(request(marker))),|req|{
            let crate::slack::client::RequestBody::Json(body)=req.body else {panic!("JSON")};
            let call=calls.get();calls.set(call+1);
            assert_eq!(body["count"],100);assert_eq!(body["query"]," literal ");
            if call==0 {assert!(body["marker"].is_null());} else {assert_eq!(body["marker"],"next");}
            async move {Ok(Some(match(case,call) {
                (0,0)=>json!({"results":vec![Value::Null;99],"next_marker":"next"}),
                (0,_)=>json!({"results":(0..20).map(|n|json!({"id":format!("C{n:08}")})).collect::<Vec<_>>(),"next_marker":{"unused":true}}),
                (1,0)=>json!({"results":[],"next_marker":"next"}),
                (1,_)=>json!({"results":[{"id":"C00000001"}]}),
                (2,_)=>json!({"results":vec![Value::Null;512],"next_marker":{"unused":true}}),
                (3,_)=>json!({"results":vec![Value::Null;513]}),
                (4,_)=>json!({"results":{"C1":{"id":"C1"}}}),
                _=>json!({"results":[],"next_marker":"next"}),
            }))}
        },||true).await;
        match case {
            0 => {
                let rows = result.unwrap().unwrap();
                assert_eq!(rows.len(), 100);
                assert!(rows[..99].iter().all(Value::is_null));
                assert_eq!(rows[99]["id"], "C00000000");
                assert_eq!(calls.get(), 2);
            }
            1 => {
                assert_eq!(result.unwrap().unwrap().len(), 1);
                assert_eq!(calls.get(), 2);
            }
            2 => {
                assert_eq!(result.unwrap().unwrap().len(), 100);
                assert_eq!(calls.get(), 1);
            }
            3 | 4 => {
                assert!(matches!(result, Err(Error::Api(_))));
                assert_eq!(calls.get(), 1);
            }
            _ => {
                assert!(matches!(result,Err(Error::Transport(s)) if s.contains("did not advance")));
                assert_eq!(calls.get(), 2);
            }
        }
    }
}
#[tokio::test]
async fn sharing_partial_rows_never_escape_error_refusal_or_retirement() {
    for case in 0..5 {
        let current = Rc::new(Cell::new(case != 4));
        let now = current.clone();
        let inside = current.clone();
        let builds = Cell::new(0);
        let calls = Cell::new(0);
        let result = fetch_calendar_sharing_channel_rows_with(
            |marker| {
                builds.set(builds.get() + 1);
                if case == 1 && marker.is_some() {
                    return Ok(None);
                }
                Ok(Some(request(marker)))
            },
            |_| {
                let call = calls.get();
                calls.set(call + 1);
                let inside = inside.clone();
                async move {
                    if case == 0 {
                        inside.set(false);
                        return Err(Error::RateLimited {
                            retry_after_secs: Some(37),
                        });
                    }
                    if call == 0 {
                        return Ok(Some(
                            json!({"results":vec![Value::Null;99],"next_marker":"next"}),
                        ));
                    }
                    if case == 2 {
                        Err(Error::RateLimited {
                            retry_after_secs: Some(37),
                        })
                    } else {
                        Ok(None)
                    }
                }
            },
            || now.get(),
        )
        .await;
        if case == 2 {
            assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(37)
                })
            ));
        } else {
            assert!(result.unwrap().is_none());
        }
        assert_eq!(
            calls.get(),
            match case {
                4 => 0,
                0 | 1 => 1,
                _ => 2,
            }
        );
        assert_eq!(
            builds.get(),
            match case {
                4 => 0,
                0 => 1,
                _ => 2,
            }
        );
    }
}
