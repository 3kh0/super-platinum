use super::*;
use serde_json::{Value, json};
use std::cell::Cell;
fn request(marker: Option<&str>) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: "https://edgeapi.slack.com/cache/T1/mpims/search".into(),
        headers: vec![],
        body: crate::slack::client::RequestBody::Json(
            json!({"query":"New Team","count":100,"fuzz":1,"use_display_name":true,"marker":marker}),
        ),
    }
}
#[tokio::test]
async fn actual_first_is_seeded_once_results_then_mpims_consume_raw100_before_unused_marker() {
    let calls = Cell::new(0);
    let rows=fetch_calendar_sharing_mpim_rows_after_with(
        json!({"results":vec![json!({"id":"GDUP00001"});99],"next_marker":"next"}),
        |marker|{assert_eq!(marker,Some("next"));Ok(Some(request(marker)))},
        |r|{calls.set(calls.get()+1);let crate::slack::client::RequestBody::Json(body)=r.body else{panic!("JSON")};
            assert_eq!(body["count"],100);assert_eq!(body["use_display_name"],true);
            async {Ok(Some(json!({"results":[{"id":"GLATER001"}],"mpims":[null],"huddles":[],"next_marker":{"unused":true}})))}}
        ,||true).await.unwrap().unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(rows.len(), 100);
    assert!(rows[..99].iter().all(|v| v["id"] == "GDUP00001"));
    assert_eq!(rows[99]["id"], "GLATER001");
    let rows=fetch_calendar_sharing_mpim_rows_after_with(
        json!({"results":vec![Value::Null;50],"mpims":vec![json!({"id":"GTAIL0001"});50],"next_marker":{}}),
        |_|panic!("initial response must not build a second request"),
        |_|async{panic!("initial response must not redispatch")},||true).await.unwrap().unwrap();
    assert_eq!(rows.len(), 100);
    assert!(rows[..50].iter().all(Value::is_null));
    assert!(rows[50..].iter().all(|v| v["id"] == "GTAIL0001"));
}
#[tokio::test]
async fn seeded_closed_pages_refusal_retirement_and_total_thirty_attempts_never_return_partial() {
    for bad in [
        json!({"results":{}}),
        json!({"results":null}),
        json!({"mpims":false}),
        json!({}),
        json!({"results":[],"ok":false}),
        json!({"results":[],"huddles":[{}]}),
        json!({"results":vec![Value::Null;513]}),
        json!({"mpims":vec![Value::Null;513]}),
    ] {
        assert!(
            fetch_calendar_sharing_mpim_rows_after_with(
                bad,
                |m| Ok(Some(request(m))),
                |_| async { panic!("invalid initial must not dispatch successor") },
                || true
            )
            .await
            .is_err()
        );
    }
    let result = fetch_calendar_sharing_mpim_rows_after_with(
        json!({"mpims":[{}],"next_marker":"next"}),
        |_| Ok(None),
        |_| async { panic!("refused") },
        || true,
    )
    .await
    .unwrap();
    assert!(result.is_none());
    let result = fetch_calendar_sharing_mpim_rows_after_with(
        json!({"results":[{}],"next_marker":"loop"}),
        |m| Ok(Some(request(m))),
        |_| async { Ok(Some(json!({"results":[],"next_marker":"loop"}))) },
        || true,
    )
    .await;
    assert!(result.is_err());
    let alive = Cell::new(true);
    let observed = Cell::new(0);
    let result = fetch_calendar_sharing_mpim_rows_after_with(
        json!({"mpims":[{}],"next_marker":"next"}),
        |m| Ok(Some(request(m))),
        |_| {
            observed.set(observed.get() + 1);
            alive.set(false);
            async {
                Err(Error::RateLimited {
                    retry_after_secs: Some(37),
                })
            }
        },
        || alive.get(),
    )
    .await
    .unwrap();
    assert!(result.is_none());
    assert_eq!(observed.get(), 1);
    let attempts = Cell::new(0);
    assert!(
        fetch_calendar_sharing_mpim_rows_after_with(
            json!({"results":[],"next_marker":"m0"}),
            |m| Ok(Some(request(m))),
            |_| {
                let n = attempts.get() + 1;
                attempts.set(n);
                async move { Ok(Some(json!({"results":[],"next_marker":format!("m{n}")}))) }
            },
            || true
        )
        .await
        .is_err()
    );
    assert_eq!(
        attempts.get(),
        29,
        "one actual initial plus29successors is30 total"
    );
    assert!(
        fetch_calendar_sharing_mpim_rows_after_with(
            Value::Null,
            |_| panic!("retired"),
            |_| async { panic!("retired") },
            || false
        )
        .await
        .unwrap()
        .is_none()
    );
}
