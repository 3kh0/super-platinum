use super::*;
struct Temp {
    path: std::path::PathBuf,
    owner: Owner,
}
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("canvas-local-review-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self {
            path,
            owner: Owner::new(&uuid::Uuid::new_v4().to_string(), "T1", "U1").unwrap(),
        }
    }
    fn store(&self) -> Store {
        Store::open(self.path.join("private.sqlite"), self.owner.account()).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn text(t: &str) -> Content {
    Content::new(t.into(), "\u{4e2d}\u{1f642}\n exact".into()).unwrap()
}
fn seed(store: &mut Store, owner: &Owner) -> (Draft, Conflict) {
    let draft = store.create(owner, &id(), text("canonical")).unwrap();
    let conflict = store
        .preserve_conflict(&draft, &id(), &id(), text("retained local"))
        .unwrap();
    (draft, conflict)
}
#[test]
fn explicit_choice_atomically_replaces_only_editable_canonical_and_fences_all_reviewed_conflicts() {
    let t = Temp::new();
    let mut s = t.store();
    let (base, c) = seed(&mut s, &t.owner);
    let other = s
        .preserve_conflict(&base, &id(), &id(), text("other version"))
        .unwrap();
    let review = s.retirement_review(&t.owner).unwrap();
    let writer = id();
    let after = s
        .resolve_local_review(
            &review,
            &LocalVersionChoice::Conflict(c.id().into()),
            &writer,
        )
        .unwrap();
    assert!(after.content() == c.content());
    assert_eq!(after.revision(), base.revision() + 1);
    assert!(s.conflicts(&t.owner).unwrap().is_empty());
    assert!(matches!(
        s.preserve_conflict(&base, c.writer(), c.id(), c.content().clone()),
        Err(Fault::Retired)
    ));
    assert!(matches!(
        s.preserve_conflict(&base, other.writer(), other.id(), other.content().clone()),
        Err(Fault::Retired)
    ));
    assert!(
        s.resolve_local_review(
            &review,
            &LocalVersionChoice::Conflict(c.id().into()),
            &writer
        )
        .unwrap()
            == after
    );
}
#[test]
fn whole_inventory_change_rejects_without_deleting_versions() {
    let t = Temp::new();
    let mut s = t.store();
    let (base, _) = seed(&mut s, &t.owner);
    let review = s.retirement_review(&t.owner).unwrap();
    s.preserve_conflict(&base, &id(), &id(), text("arrived after display"))
        .unwrap();
    assert!(matches!(
        s.resolve_local_review(&review, &LocalVersionChoice::Canonical, &id()),
        Err(Fault::Stale)
    ));
    assert_eq!(s.conflicts(&t.owner).unwrap().len(), 2);
    assert!(s.active(&t.owner).unwrap().unwrap() == base);
}
#[test]
fn started_or_uncertain_creation_is_never_unlocked_by_local_choice() {
    let t = Temp::new();
    let mut s = t.store();
    let base = s.create(&t.owner, &id(), text("sent content")).unwrap();
    let reservation = s.reserve(&base, 1).unwrap();
    let started = s.admit(&reservation, 2).unwrap().consume();
    let current = s.active(&t.owner).unwrap().unwrap();
    s.preserve_conflict(&current, &id(), &id(), text("local"))
        .unwrap();
    let review = s.retirement_review(&t.owner).unwrap();
    assert!(matches!(
        s.resolve_local_review(&review, &LocalVersionChoice::Canonical, &id()),
        Err(Fault::Blocked)
    ));
    assert!(s.attempt(&t.owner, started.id()).unwrap().unwrap() == started);
    assert_eq!(s.conflicts(&t.owner).unwrap().len(), 1);
}
#[test]
fn transaction_failure_after_conflict_deletion_rolls_back_and_lost_reply_retry_cannot_overwrite_new_edit()
 {
    let t = Temp::new();
    let mut s = t.store();
    let (base, c) = seed(&mut s, &t.owner);
    let review = s.retirement_review(&t.owner).unwrap();
    let writer = id();
    s.conn.execute_batch("CREATE TEMP TRIGGER reject_canvas_review BEFORE UPDATE ON drafts BEGIN SELECT RAISE(ABORT,'synthetic'); END;").unwrap();
    assert!(
        s.resolve_local_review(
            &review,
            &LocalVersionChoice::Conflict(c.id().into()),
            &writer
        )
        .is_err()
    );
    assert!(s.active(&t.owner).unwrap().unwrap() == base);
    assert_eq!(s.conflicts(&t.owner).unwrap().len(), 1);
    s.conn
        .execute_batch("DROP TRIGGER reject_canvas_review;")
        .unwrap();
    let applied = s
        .resolve_local_review(
            &review,
            &LocalVersionChoice::Conflict(c.id().into()),
            &writer,
        )
        .unwrap();
    let edited = s
        .save(&applied, &id(), text("later canonical edit"))
        .unwrap();
    assert!(matches!(
        s.resolve_local_review(
            &review,
            &LocalVersionChoice::Conflict(c.id().into()),
            &writer
        ),
        Err(Fault::Stale)
    ));
    assert!(s.active(&t.owner).unwrap().unwrap() == edited);
}
