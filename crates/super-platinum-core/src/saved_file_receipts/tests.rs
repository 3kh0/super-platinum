use super::*;
const ACCOUNT: &str = "00000000-0000-4000-8000-000000000098";
fn scope() -> FileScope {
    FileScope {
        team_id: "T1".into(),
        user_id: "U0".into(),
        file_id: "F1".into(),
    }
}
fn operation() -> Operation {
    Operation {
        title: "Résumé 🚀\n文件".into(),
        action: Action::Reminder(1234),
    }
}
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("saved-file-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, ACCOUNT).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[test]
fn restart_keeps_immutable_action_and_blocks_replay() {
    let f = Fixture::new();
    let Begin::Started(receipt) = f.open().begin(scope(), operation(), 100).unwrap() else {
        panic!()
    };
    let mut j = f.open();
    assert_eq!(j.get(&scope()).unwrap(), Some(receipt.clone()));
    assert!(matches!(
        j.begin(
            scope(),
            Operation {
                title: "Different".into(),
                action: Action::Remove
            },
            999
        )
        .unwrap(),
        Begin::Blocked(_)
    ));
    assert_eq!(j.unresolved("T1", "U0").unwrap(), vec![receipt]);
    assert!(j.unresolved("T2", "U0").unwrap().is_empty());
    assert!(Journal::open(&f.0, "00000000-0000-4000-8000-000000000099").is_err());
}
#[test]
fn exact_ids_cooldowns_and_retirement_cannot_resolve_replacements() {
    let f = Fixture::new();
    let mut j = f.open();
    let Begin::Started(first) = j.begin(scope(), operation(), 100).unwrap() else {
        panic!()
    };
    assert!(
        j.reserve_check(&scope(), &first.operation_id, 100)
            .unwrap()
            .is_some()
    );
    assert!(
        j.reserve_check(&scope(), &first.operation_id, 101)
            .unwrap()
            .is_none()
    );
    assert!(
        j.defer_check(&scope(), &first.operation_id, 101, 200)
            .unwrap()
    );
    assert!(j.retire(&scope(), &first.operation_id, 102).unwrap());
    assert!(j.unresolved("T1", "U0").unwrap().is_empty());
    assert_eq!(j.recoverable("T1", "U0", 199).unwrap().len(), 1);
    assert!(j.recoverable("T1", "U0", 200).unwrap().is_empty());
    assert!(matches!(
        j.begin(scope(), operation(), 199).unwrap(),
        Begin::Blocked(_)
    ));
    let Begin::Started(second) = j.begin(scope(), operation(), 200).unwrap() else {
        panic!()
    };
    assert_ne!(first.operation_id, second.operation_id);
    assert!(
        !j.resolve(&scope(), &first.operation_id, Outcome::Confirmed, 201, 0)
            .unwrap()
    );
    assert!(!j.retire(&scope(), &first.operation_id, 201).unwrap());
    assert_eq!(j.get(&scope()).unwrap().unwrap().status, Status::Uncertain);
}
#[test]
fn independent_connections_serialize_begin() {
    let f = Fixture::new();
    let mut a = f.open();
    let mut b = f.open();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let other = barrier.clone();
    let thread = std::thread::spawn(move || {
        other.wait();
        a.begin(scope(), operation(), 100).unwrap()
    });
    barrier.wait();
    let right = b.begin(scope(), operation(), 100).unwrap();
    let left = thread.join().unwrap();
    assert!(matches!(
        (left, right),
        (Begin::Started(_), Begin::Blocked(_)) | (Begin::Blocked(_), Begin::Started(_))
    ));
}
#[test]
fn terminal_retention_is_bounded_but_unresolved_survives() {
    let f = Fixture::new();
    let mut j = f.open();
    j.begin(scope(), operation(), 1).unwrap();
    for i in 0..140 {
        let mut s = scope();
        s.file_id = format!("F{i}x");
        let Begin::Started(r) = j.begin(s.clone(), operation(), 2 + i).unwrap() else {
            panic!()
        };
        j.resolve(&s, &r.operation_id, Outcome::Confirmed, 2 + i, 0)
            .unwrap();
    }
    assert_eq!(j.unresolved("T1", "U0").unwrap().len(), 1);
    let count: i64 = j
        .conn
        .query_row("SELECT COUNT(*) FROM receipts WHERE terminal=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 128);
}
