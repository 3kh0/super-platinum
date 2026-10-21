use super::super::tests::{ID, attempt, raw, setup};
use super::*;

#[test]
fn success_maps_original_envelope_omissions_without_typed_defaults() {
    let raw = json!({"ok":true,"file":{"id":ID,"name":"","is_deleted":true,"todos":null,
        "comments":"stale","content":"stale","content_html":"stale","opaque":{"x":false}},
        "comments":null,"content":"fresh"});
    let value = mapped(&raw, ID).unwrap();
    assert_eq!(value["name"], "");
    assert_eq!(value["is_deleted"], false);
    assert_eq!(value["comments"], Value::Null);
    assert_eq!(value["content"], "fresh");
    assert!(value.get("content_html").is_none());
    assert!(value.get("todos").is_none());
    assert!(value.get("permalink").is_none());
    assert_eq!(value["opaque"], json!({"x":false}));
    assert_eq!(raw["file"]["is_deleted"], true);
    for bad in [
        json!({"ok":true,"file":{"id":"F87654321"}}),
        json!({"ok":false,"file":{"id":ID}}),
        json!({"ok":true,"error":"access_denied","file":{"id":ID}}),
        json!({"ok":true,"file":null}),
    ] {
        assert!(mapped(&bad, ID).is_err());
    }
}
#[test]
fn positive_access_denied_file_is_known_without_retry_or_link_override() {
    let (shell, mut model) = setup(&[raw()]);
    let original = attempt(&shell, &mut model);
    let envelope = json!({"ok":true,"file":{"id":ID,"name":"Blocked",
        "permalink":"https://example.test/blocked","file_access":"access_denied"}});
    finish(&shell, &mut model, &original, Ok(envelope.clone()));
    assert!(model.file_acquisition.attempts[0].phase == Phase::Ready);
    assert_eq!(model.file_acquisition.attempts[0].receipt, Some(envelope));
    assert!(!original.cold.absent(&shell, &model));
    assert!(pending(&shell, &model).is_empty());
    assert!(file_source::token(&shell, &model, &model.rows[0].message).is_none());
}
#[test]
fn endpoint_limit_stops_other_ids_and_survives_read_retirement_without_retry() {
    let mut other = raw();
    other["ts"] = json!("1719800002.000100");
    other["files"] = json!([{"id":"F87654321","file_access":"check_file_info"}]);
    let (shell, mut model) = setup(&[raw(), other]);
    let first = attempt(&shell, &mut model);
    let second = attempt(&shell, &mut model);
    model.file_acquisition.attempts[1].phase = Phase::Queued;
    let deadline = finish(
        &shell,
        &mut model,
        &first,
        Err(Error::RateLimited {
            retry_after_secs: Some(15),
        }),
    )
    .unwrap();
    assert!(deadline > Instant::now());
    assert!(cooling(&shell, &model));
    assert!(
        model
            .file_acquisition
            .attempts
            .iter()
            .all(|a| a.phase == Phase::Unavailable)
    );
    assert!(!requeue(
        &shell,
        &mut model,
        &Retry {
            token: second.token,
            cold: second.cold
        }
    ));
    retire(&mut model.file_acquisition);
    assert!(model.file_acquisition.attempts.is_empty());
    assert!(cooling(&shell, &model));
    model.file_acquisition.limit.as_mut().unwrap().until = Some(Instant::now());
    assert!(
        cooling(&shell, &model),
        "model mutation cannot erase process debt"
    );
    model.file_acquisition = State::default();
    sync_limit(&shell, &mut model);
    assert_eq!(
        model.file_acquisition.limit.as_ref().unwrap().until,
        Some(deadline)
    );
    assert!(
        model.file_acquisition.attempts.is_empty(),
        "reconstructing debt never queues a request"
    );
    let (limited, deadline, _) = failure(&Error::RateLimited {
        retry_after_secs: Some(u64::MAX),
    });
    assert!(limited);
    assert!(deadline.is_none(), "overflow retains blocked endpoint");
    let (limited, deadline, _) = failure(&Error::RateLimited {
        retry_after_secs: None,
    });
    assert!(limited);
    assert!(
        deadline.is_some(),
        "missing Retry-After uses native bounded fallback"
    );
}
#[test]
fn malformed_or_retired_read_never_installs_a_synthetic_status_row() {
    let (mut shell, mut model) = setup(&[raw()]);
    let original = attempt(&shell, &mut model);
    finish(
        &shell,
        &mut model,
        &original,
        Ok(json!({"ok":true,"file":{"id":"F87654321"}})),
    );
    assert!(original.cold.absent(&shell, &model));
    assert!(model.file_acquisition.attempts[0].phase == Phase::Unavailable);
    assert!(model.file_acquisition.attempts[0].receipt.is_some());
    crate::file_events::invalidate(&mut shell, "T1", ID);
    finish(
        &shell,
        &mut model,
        &original,
        Ok(json!({"ok":true,"file":{"id":ID,"name":"late"}})),
    );
    assert!(model.file_acquisition.attempts[0].phase == Phase::Retired);
    assert!(
        file_source::snapshot(&shell, &model)["rows"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn guarded_carrier_is_dropped_on_retirement_and_rechecks_terminal_poll() {
    use std::cell::Cell;
    struct DropProof<'a>(&'a Cell<bool>);
    impl Drop for DropProof<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async {
            let current = Cell::new(true);
            let dropped = Cell::new(false);
            let read = async {
                let _held = DropProof(&dropped);
                std::future::pending::<Result<(), Error>>().await
            };
            let retire = async {
                tokio::time::sleep(Duration::from_millis(1)).await;
                current.set(false);
            };
            let (result, ()) = tokio::join!(guarded(read, || current.get()), retire);
            assert!(result.is_err());
            assert!(dropped.get());
            current.set(true);
            assert!(
                guarded(
                    async {
                        current.set(false);
                        Ok(())
                    },
                    || current.get()
                )
                .await
                .is_err()
            );
        });
}
