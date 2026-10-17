use super::*;
use std::path::PathBuf;

mod corrections;
mod decisions;
mod failures;
mod retirement;
mod retirement_failures;
mod retirement_scale;
mod snapshots;

struct Temp {
    dir: PathBuf,
    account: String,
}
impl Temp {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("sp-canvas-creation-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        Self {
            dir,
            account: uuid::Uuid::new_v4().to_string(),
        }
    }
    fn path(&self) -> PathBuf {
        self.dir.join("creation.sqlite")
    }
    fn store(&self) -> Store {
        Store::open(self.path(), &self.account).unwrap()
    }
    fn owner(&self) -> Owner {
        Owner::new(&self.account, "T1", "U1").unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn writer() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn content(label: &str) -> Content {
    Content::new(
        format!(" {label} "),
        format!("{label}\n\u{1f642}\u{4e2d}e\u{301}  \n"),
    )
    .unwrap()
}
fn seed(temp: &Temp, store: &mut Store) -> Draft {
    store
        .create(&temp.owner(), &writer(), content("Original"))
        .unwrap()
}
fn start(store: &mut Store, draft: &Draft) -> (Draft, Attempt) {
    let reservation = store.reserve(draft, 100).unwrap();
    let attempt = store.admit(&reservation, 100).unwrap().consume();
    (reservation.draft.clone(), attempt)
}

#[test]
fn exact_unicode_empty_local_roundtrip_and_no_secret_shape() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    let saved = store.save(&draft, &writer(), Content::empty()).unwrap();
    assert!(matches!(store.reserve(&saved, 100), Err(Fault::Invalid)));
    drop(store);
    let store = temp.store();
    assert!(store.active(&temp.owner()).unwrap() == Some(saved));
    let body = serde_json::to_value(&draft).unwrap();
    assert!(
        body.get("token").is_none()
            && body.get("cookie").is_none()
            && body.get("transport").is_none()
    );
    assert!(draft.content.markdown().contains('\u{1f642}'));
    assert!(draft.content.markdown().contains("e\u{301}"));
}
#[test]
fn exact_revision_content_and_writer_guard_stale_saves_and_reservation() {
    let temp = Temp::new();
    let mut a = temp.store();
    let mut b = temp.store();
    let first = seed(&temp, &mut a);
    let newer = a.save(&first, &writer(), content("New")).unwrap();
    assert!(matches!(
        b.save(&first, &writer(), content("Old")),
        Err(Fault::Stale)
    ));
    assert!(matches!(b.reserve(&first, 100), Err(Fault::Stale)));
    let mut forged = newer.clone();
    forged.writer = writer();
    assert!(matches!(b.reserve(&forged, 100), Err(Fault::Stale)));
    assert!(b.active(&temp.owner()).unwrap() == Some(newer));
}
#[test]
fn two_connections_only_one_can_admit_same_reservation() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    let reservation = store.reserve(&draft, 100).unwrap();
    drop(store);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let path = temp.path();
        let account = temp.account.clone();
        let r = reservation.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut store = Store::open(path, &account).unwrap();
            barrier.wait();
            store.admit(&r, 100).map(|permit| permit.consume()).is_ok()
        }));
    }
    let wins = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|won| *won)
        .count();
    assert_eq!(wins, 1);
    let mut store = temp.store();
    assert!(matches!(
        store.reservation(&temp.owner(), reservation.attempt.id()),
        Err(Fault::Blocked)
    ));
    assert!(store.cancel_reserved(&reservation).is_err());
    assert!(store.admit(&reservation, 100).is_err());
}
#[test]
fn restored_reserved_cancel_prevents_old_admission_and_keeps_content() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    let reservation = store.reserve(&draft, 100).unwrap();
    drop(store);
    let mut fresh = temp.store();
    let restored = fresh
        .reservation(&temp.owner(), reservation.attempt.id())
        .unwrap();
    let canceled = fresh.cancel_reserved(&restored).unwrap();
    assert!(matches!(
        canceled.delivery,
        Delivery::CanceledBeforeAdmission
    ));
    assert!(fresh.admit(&reservation, 100).is_err());
    assert!(fresh.active(&temp.owner()).unwrap().unwrap().content == draft.content);
    assert!(fresh.reserve(&restored.draft, 100).is_ok());
}
#[test]
fn lost_permit_is_unresolved_and_outcome_retry_only_settles_original() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    let reservation = store.reserve(&draft, 100).unwrap();
    drop(store.admit(&reservation, 100).unwrap());
    drop(store);
    let mut store = temp.store();
    let started = store
        .attempt(&temp.owner(), reservation.attempt.id())
        .unwrap()
        .unwrap();
    assert!(matches!(started.delivery, Delivery::Started));
    assert!(matches!(
        store.save(&reservation.draft, &writer(), content("Retry")),
        Err(Fault::Blocked)
    ));
    let uncertain = store.settle(&started, Outcome::Uncertain).unwrap();
    assert!(store.settle(&started, Outcome::Uncertain).unwrap() == uncertain);
    assert!(matches!(
        store.settle(
            &started,
            Outcome::Created {
                file_id: "F1".into()
            }
        ),
        Err(Fault::Stale)
    ));
    let created = store
        .settle(
            &uncertain,
            Outcome::Created {
                file_id: "F1".into(),
            },
        )
        .unwrap();
    assert!(matches!(created.delivery, Delivery::Created { .. }));
    assert!(store.admit(&reservation, 100).is_err());
}
#[test]
fn unknown_api_and_offline_are_uncertain_and_429_is_explicit() {
    for error in [
        crate::slack::Error::Api("not_allowed".into()),
        crate::slack::Error::Api("future_unknown_error".into()),
        crate::slack::Error::Offline("synthetic".into()),
    ] {
        assert!(matches!(
            Outcome::from_response(&Err(error), 100).unwrap(),
            Outcome::Uncertain
        ));
    }
    for response in [
        serde_json::json!({"ok":true}),
        serde_json::json!({"ok":false,"file_id":"F1"}),
        serde_json::json!({"file_id":"F1","error":"unknown"}),
    ] {
        assert!(matches!(
            Outcome::from_response(&Ok(response), 100).unwrap(),
            Outcome::Uncertain
        ));
    }
    assert!(matches!(
        Outcome::from_response(&Ok(serde_json::json!({"ok":true,"file_id":"F1"})), 100).unwrap(),
        Outcome::Created { .. }
    ));
    assert!(matches!(
        Outcome::from_response(
            &Err(crate::slack::Error::RateLimited {
                retry_after_secs: Some(20)
            }),
            100
        )
        .unwrap(),
        Outcome::RateLimited { retry_at: 120 }
    ));
    assert!(matches!(
        Outcome::from_response(
            &Err(crate::slack::Error::RateLimited {
                retry_after_secs: None
            }),
            100
        )
        .unwrap(),
        Outcome::RateLimited { retry_at: 100 }
    ));
}
#[test]
fn rate_barrier_survives_save_discard_new_draft_and_reopen() {
    let temp = Temp::new();
    let mut store = temp.store();
    let original = seed(&temp, &mut store);
    let (draft, started) = start(&mut store, &original);
    store
        .settle(&started, Outcome::RateLimited { retry_at: 200 })
        .unwrap();
    let edited = store.save(&draft, &writer(), content("Edited")).unwrap();
    store.discard(&edited).unwrap();
    let next = store
        .create(&temp.owner(), &writer(), content("Next"))
        .unwrap();
    drop(store);
    let mut store = temp.store();
    assert!(matches!(store.reserve(&next, 199), Err(Fault::Cooldown)));
    assert!(store.reserve(&next, 200).is_ok());
    assert_eq!(count(&store.conn, "attempts").unwrap(), 2);
}
#[test]
fn late_archived_429_blocks_already_reserved_new_draft_admission() {
    let temp = Temp::new();
    let mut store = temp.store();
    let original = seed(&temp, &mut store);
    let (draft, started) = start(&mut store, &original);
    store
        .archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &writer(),
            &writer(),
            100,
        )
        .unwrap();
    let empty = store.active(&temp.owner()).unwrap().unwrap();
    let next = store.save(&empty, &writer(), content("Next")).unwrap();
    let reservation = store.reserve(&next, 100).unwrap();
    store
        .settle(&started, Outcome::RateLimited { retry_at: 200 })
        .unwrap();
    assert!(matches!(
        store.admit(&reservation, 100),
        Err(Fault::Cooldown)
    ));
    assert!(store.cancel_reserved(&reservation).is_ok());
}
#[test]
fn conflicts_are_immutable_global_capacity_and_block_admission() {
    let temp = Temp::new();
    let mut store = temp.store();
    let a = seed(&temp, &mut store);
    let second = Owner::new(&temp.account, "T2", "U2").unwrap();
    let b = store.create(&second, &writer(), content("B")).unwrap();
    let reservation = store.reserve(&a, 100).unwrap();
    let same_writer = writer();
    let mut retained = Vec::new();
    for index in 0..MAX_CONFLICTS {
        let draft = if index % 2 == 0 { &a } else { &b };
        retained.push(
            store
                .preserve_conflict(&draft, &same_writer, &writer(), content(&index.to_string()))
                .unwrap(),
        );
    }
    assert!(matches!(
        store.admit(&reservation, 100),
        Err(Fault::Blocked)
    ));
    assert!(matches!(
        store.preserve_conflict(&a, &same_writer, &writer(), content("17")),
        Err(Fault::Capacity)
    ));
    assert_eq!(store.conflicts(&a.owner).unwrap().len(), 8);
    assert_eq!(store.conflicts(&b.owner).unwrap().len(), 8);
    let original = retained.remove(0);
    let mut forged = original.clone();
    forged.content = content("wrong");
    assert!(matches!(store.remove_conflict(&forged), Err(Fault::Stale)));
    store.remove_conflict(&original).unwrap();
    assert!(store.remove_conflict(&original).is_err());
    assert_eq!(count(&store.conn, "conflicts").unwrap(), 15);
}
