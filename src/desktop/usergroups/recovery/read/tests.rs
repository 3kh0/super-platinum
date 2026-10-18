use super::*;
use serde_json::json;
const A: &str = "SGROUP001";
const B: &str = "SGROUP002";
const C: &str = "SGROUP003";

#[test]
fn directory_then_fallback_first_wins_and_opaque_observations_survive() {
    let first = json!({"id":A,"handle":"first","opaque":{"keep":1}});
    let duplicate = json!({"id":A,"handle":"second"});
    let fallback = json!({"id":B,"handle":"fallback","date_delete":123});
    let mut part = Part::default();
    part.append(Part {
        rows: vec![first.clone(), duplicate],
        failed: vec![B.into()],
        ..Default::default()
    });
    part.append(Part {
        rows: vec![json!({"id":A,"handle":"fallback-late"}), fallback.clone()],
        ..Default::default()
    });
    let received = part.finish();
    assert_eq!(
        received
            .rows
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec![A, B]
    );
    assert_eq!(received.rows[0].label_source, Some(first));
    assert_eq!(received.rows[1].label_source, Some(fallback));
    assert_eq!(received.rows[1].date_delete, 123);
}

#[test]
fn pending_and_invalid_are_not_failed_id_fallback_or_terminal_rows() {
    let read = Read::capture(vec![A.into(), B.into(), C.into()]).unwrap();
    let page = Page::decode(
        &read,
        json!({"ok":true,"results":[],"pending_ids":[A],"invalid_ids":[B],"failed_ids":[C]}),
        None,
    )
    .unwrap();
    let mut part = Part::default();
    part.page(&page);
    assert_eq!(part.failed, vec![C]);
    assert!(part.rows.is_empty());
    let received = part.finish();
    assert!(received.rows.is_empty());
}

#[test]
fn failed_fallback_retains_prior_success_and_raw_error_evidence() {
    let wire = json!({"ok":true,"results":[{"id":A,"handle":"good"}],"failed_ids":[B]});
    let failed = Arc::new(Error::Offline("synthetic".into()));
    let mut part = Part {
        rows: wire["results"].as_array().unwrap().clone(),
        raw: vec![wire.clone()],
        failed: vec![B.into()],
        ..Default::default()
    };
    part.append(Part {
        errors: vec![failed.clone()],
        ..Default::default()
    });
    let received = part.finish();
    assert_eq!(received.rows.len(), 1);
    assert_eq!(received.rows[0].id, A);
    assert_eq!(received.raw, vec![wire]);
    assert!(Arc::ptr_eq(&received.errors[0], &failed));
}

#[test]
fn malformed_model_row_does_not_erase_valid_sibling_or_become_unknown() {
    let received = Part {
        rows: vec![
            json!({"id":A,"handle":"good"}),
            json!({"id":B,"handle":["bad"]}),
        ],
        ..Default::default()
    }
    .finish();
    assert_eq!(received.rows.len(), 1);
    assert_eq!(received.rows[0].id, A);
    assert_eq!(received.errors.len(), 1);
}

#[tokio::test]
async fn fallback_all_polls_every_chunk_and_keeps_request_order() {
    let started = std::cell::Cell::new(0);
    let jobs = (0..4)
        .map(|index| {
            let started = &started;
            async move {
                let mut first = true;
                poll_fn(|cx| {
                    if first {
                        first = false;
                        started.set(started.get() + 1);
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        assert_eq!(started.get(), 4);
                        Poll::Ready(index)
                    }
                })
                .await
            }
        })
        .collect();
    assert_eq!(all(jobs).await, vec![0, 1, 2, 3]);
}
