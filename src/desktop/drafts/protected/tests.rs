//! Opaque proofs come only from the isolated production worker command facade.
//! Requires the root-reviewed core protected-test-support dev feature.
use super::account::wait;
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use super_platinum_core::ComposerState;
const ID: &str = "00000000-0000-4000-8000-000000000151";
struct Fixture {
    _isolated: durable::test_support::Isolated,
    account: Arc<Account>,
    owner: Owner,
}

#[path = "tests/actions.rs"]
mod actions_tests;
#[path = "tests/delivery.rs"]
mod delivery_tests;
#[path = "tests/retention.rs"]
mod retention_tests;
#[path = "tests/storage.rs"]
mod storage_tests;
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
impl Fixture {
    async fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "protected-desktop-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let isolated = durable::test_support::Isolated::create(ID.to_owned(), directory).unwrap();
        let account = Account::isolated(ID.to_owned(), isolated.client());
        account.initialize().await.unwrap();
        let owner = account
            .owner(DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            })
            .unwrap();
        let inventory = owner.local_inventory().await.unwrap();
        let lease = wait(account.client().unwrap().renew_local(inventory.clone()))
            .await
            .unwrap();
        let serial = lease.initial_serial();
        let inventory = match wait(account.client().unwrap().write_local(
            lease,
            serial,
            serial + 1,
            inventory.local().clone(),
            local("Original"),
        ))
        .await
        .unwrap()
        {
            durable::LocalWriteResult::Applied(value) => value,
            _ => panic!("first isolated input was not canonical"),
        };
        wait(
            account
                .client()
                .unwrap()
                .enroll_local(owner.scope().clone(), inventory),
        )
        .await
        .unwrap();
        Self {
            _isolated: isolated,
            account,
            owner,
        }
    }
    async fn reenroll(&self) -> (durable::Review, durable::Review) {
        let old = self.owner.review().await.unwrap();
        let writer = wait(self.owner.register(old.clone()).unwrap())
            .await
            .unwrap();
        let release = wait(self.owner.release(old.clone()).unwrap())
            .await
            .unwrap();
        wait(self.account.client().unwrap().start_local(release, writer))
            .await
            .unwrap();
        let local = self.owner.local_inventory().await.unwrap();
        let next = wait(
            self.account
                .client()
                .unwrap()
                .enroll_local(self.owner.scope().clone(), local),
        )
        .await
        .unwrap();
        (old, next)
    }
}
#[tokio::test]
async fn same_bytes_reenrollment_changes_authority_and_refuses_old_publication() {
    let fixture = Fixture::new().await;
    let mut session = Session::default();
    session.select(ID.into(), 9);
    let load = session.begin_load().unwrap();
    session
        .accept_load(load, &fixture.account.inventory().await.unwrap())
        .unwrap();
    let before = session
        .entry(ID, 9, &fixture.owner.scope().key)
        .unwrap()
        .clone();
    let (old, new) = fixture.reenroll().await;
    assert!(same_local(old.local(), new.local()));
    assert!(!old.row().same_enrollment(new.row()));
    assert!(!old.same(&new));
    let load = session.begin_load().unwrap();
    session
        .accept_load(load, &fixture.account.inventory().await.unwrap())
        .unwrap();
    assert!(!session.apply(ID, 9, &before, View::new(Inventory::Managed(old), None)));
    assert!(
        session
            .entry(ID, 9, &fixture.owner.scope().key)
            .unwrap()
            .inventory()
            .managed()
            .unwrap()
            .same(&new)
    );
}
#[tokio::test]
async fn detached_old_editor_preserves_origin_without_overwriting_new_enrollment() {
    let fixture = Fixture::new().await;
    let editor = editor::Editor::open(fixture.owner.clone()).await.unwrap();
    let old = fixture.owner.review().await.unwrap();
    fixture.reenroll().await;
    editor
        .submit(&local("Detached input \u{65e5}\u{672c} \u{1f680}"))
        .unwrap();
    assert!(matches!(
        editor.flush().await,
        Err(durable::Error::ReviewRequired)
    ));
    let current = fixture.owner.local_inventory().await.unwrap();
    assert_eq!(current.local().composer.text, "Original");
    let version = current
        .alternates()
        .iter()
        .find(|version| version.draft.composer.text.starts_with("Detached input"))
        .unwrap();
    let origin = current
        .origins()
        .iter()
        .find(|origin| origin.writer_id() == version.writer_id)
        .unwrap();
    assert!(
        matches!(origin.origin(), durable::WriterOrigin::Enrollment(id) if id == old.row().lineage().enrollment())
    );
    assert!(!current.row().unwrap().same_enrollment(old.row()));
}
#[tokio::test]
async fn no_op_managed_input_advances_serial_and_rejects_different_same_serial_payload() {
    let fixture = Fixture::new().await;
    let editor = editor::Editor::open(fixture.owner.clone()).await.unwrap();
    let serial = editor.submit(&local("Original")).unwrap();
    editor.flush().await.unwrap();
    let current = fixture.owner.review().await.unwrap();
    let writer = editor.state.lock().unwrap().writer.clone();
    let result = wait(
        fixture
            .owner
            .checkpoint(writer, serial, current, local("Different"), None)
            .unwrap(),
    )
    .await;
    assert!(matches!(result, Err(durable::Error::Stale)));
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "Original"
    );
}
#[tokio::test]
async fn local_noop_advances_clock_and_invalidates_same_bytes_renewal() {
    let fixture = Fixture::new().await;
    let old = fixture.owner.review().await.unwrap();
    let writer = wait(fixture.owner.register(old.clone()).unwrap())
        .await
        .unwrap();
    let release = wait(fixture.owner.release(old.clone()).unwrap())
        .await
        .unwrap();
    let lease = wait(
        fixture
            .account
            .client()
            .unwrap()
            .start_local(release, writer),
    )
    .await
    .unwrap();
    let before = fixture.owner.local_inventory().await.unwrap();
    let lane = local_writer::LocalWriter::from_handoff(
        fixture.account.clone(),
        lease,
        before.local().clone(),
    )
    .unwrap();
    let serial = lane.submit(before.local()).unwrap();
    let cloned = lane.clone();
    assert_eq!(cloned.snapshot().serial, serial);
    lane.flush().await.unwrap();
    let after = fixture.owner.local_inventory().await.unwrap();
    assert!(same_local(before.local(), after.local()));
    assert!(after.clock() > before.clock());
    assert!(!before.same(&after));
    assert!(matches!(
        wait(fixture.account.client().unwrap().renew_local(before)).await,
        Err(durable::Error::Stale)
    ));
}
#[tokio::test]
async fn stale_load_and_wrong_scope_do_not_replace_current_inventory() {
    let fixture = Fixture::new().await;
    let mut session = Session::default();
    session.select(ID.into(), 3);
    let old = session.begin_load().unwrap();
    let new = session.begin_load().unwrap();
    let inventory = fixture.account.inventory().await.unwrap();
    assert!(matches!(
        session.accept_load(old, &inventory),
        Err(durable::Error::Stale)
    ));
    session.accept_load(new, &inventory).unwrap();
    let other = fixture
        .account
        .owner(DraftKey {
            team: "T1".into(),
            channel: "C2".into(),
            thread: None,
        })
        .unwrap();
    assert!(matches!(
        other.control(
            fixture.owner.review().await.unwrap(),
            durable::Control::Clear
        ),
        Err(durable::Error::Stale)
    ));
}
