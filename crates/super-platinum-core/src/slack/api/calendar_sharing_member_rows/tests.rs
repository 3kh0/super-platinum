use super::*;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};
fn request(marker: Option<&str>) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: "https://edgeapi.slack.com/cache/T1/users/search".into(),
        headers: vec![],
        body: crate::slack::client::RequestBody::Json(
            json!({"query":" literal ","count":100,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated","search_email":true,"include_profile_only_users":true,"marker":marker}),
        ),
    }
}
#[tokio::test]
async fn single_member_raw100_keeps_markers_and_raw_order_before_item_admission_or_unused_marker() {
    for case in 0..4 {
        let calls = Cell::new(0);
        let rows=fetch_calendar_sharing_member_rows_with(|marker|Ok(Some(request(marker))),|request|{
            let crate::slack::client::RequestBody::Json(body)=request.body else {panic!("JSON")};
            let call=calls.get();calls.set(call+1);
            assert_eq!(body["query"]," literal ");assert_eq!(body["count"],100);assert_eq!(body["include_profile_only_users"],true);
            if call==0 {assert!(body["marker"].is_null());} else {assert_eq!(body["marker"],"next");}
            async move {Ok(Some(match(case,call) {
                (0,0)=>json!({"results":vec![json!({"id":"UDUPLICATE","is_profile_only_user":true});99],"next_marker":"next"}),
                (0,_)=>json!({"results":[{"id":"ULATER"},null,{"invalid":"beyond100"}],"next_marker":{"unused":true}}),
                (1,0)=>json!({"results":[],"next_marker":"next","mpims":[],"huddles":[]}),
                (1,_)=>json!({"results":[{"id":"ULATER"}],"metadata":{"not_a_row":"UMETA"}}),
                (2,_)=>json!({"results":vec![Value::Null;512],"next_marker":{"unused":true}}),
                _=>json!({"results":[{"id":"U1"},null,{"id":"U1"}]}),
            }))}
        },||true).await.unwrap().unwrap();
        match case {
            0 => {
                assert_eq!(rows.len(), 100);
                assert!(rows[..99].iter().all(|r| r["id"] == "UDUPLICATE"));
                assert_eq!(rows[99]["id"], "ULATER");
                assert_eq!(calls.get(), 2);
            }
            1 => {
                assert_eq!(rows, json!([{"id":"ULATER"}]).as_array().unwrap().clone());
                assert_eq!(calls.get(), 2);
            }
            2 => {
                assert_eq!(rows.len(), 100);
                assert!(rows.iter().all(Value::is_null));
                assert_eq!(calls.get(), 1);
            }
            _ => {
                assert_eq!(rows.len(), 3);
                assert!(rows[1].is_null());
                assert_eq!(rows[0], rows[2]);
            }
        }
    }
}
#[tokio::test]
async fn single_member_closed_envelope_late_failures_and_retirement_never_publish_partial_rows() {
    let mut bad = vec![
        json!({"results":{}}),
        json!({"results":vec![Value::Null;513]}),
        json!({"results":[],"ok":false}),
        json!({"results":[],"ok":"true"}),
    ];
    for key in ["mpims", "huddles"] {
        for value in [Value::Null, json!(false), json!({}), json!([{"id":"AUX"}])] {
            let mut wire = json!({"results":vec![Value::Null;100]});
            wire[key] = value;
            bad.push(wire);
        }
    }
    for wire in bad {
        let calls = Cell::new(0);
        let result = fetch_calendar_sharing_member_rows_with(
            |marker| Ok(Some(request(marker))),
            |_| {
                calls.set(calls.get() + 1);
                let wire = wire.clone();
                async move { Ok(Some(wire)) }
            },
            || true,
        )
        .await;
        assert!(matches!(result, Err(Error::Api(_))));
        assert_eq!(calls.get(), 1);
    }
    for case in 0..4 {
        let calls = Cell::new(0);
        let current = Rc::new(Cell::new(true));
        let observed = Cell::new(0);
        let flag = current.clone();
        let result = fetch_calendar_sharing_member_rows_with(
            |marker| Ok(Some(request(marker))),
            |_| {
                let call = calls.get();
                calls.set(call + 1);
                let value = if call == 0 {
                    Ok(Some(
                        json!({"results":[{"id":"UPARTIAL"}],"next_marker":"next"}),
                    ))
                } else {
                    match case {
                        0 => {
                            observed.set(observed.get() + 1);
                            Err(Error::Transport("later page failed".into()))
                        }
                        1 => Ok(Some(json!({"results":[],"next_marker":"next"}))),
                        2 => Ok(None),
                        _ => {
                            observed.set(observed.get() + 1);
                            flag.set(false);
                            Err(Error::Transport("retired result".into()))
                        }
                    }
                };
                async move { value }
            },
            || current.get(),
        )
        .await;
        if case < 2 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_none());
        }
        assert_eq!(calls.get(), 2);
        assert_eq!(observed.get(), usize::from(case == 0 || case == 3));
    }
}
