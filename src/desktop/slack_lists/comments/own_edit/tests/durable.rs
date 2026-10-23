use super::*;
use crate::slack_lists::comments::own_edit::durable::{Durable, Outcome, restored_attempt};
use super_platinum_core::slack_lists::comments::journal::Scope as DiskScope;

struct Local(std::path::PathBuf);
impl Local {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "list-own-edit-host-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
    }
    fn disk(&self, seed: &OwnEditSeed) -> storage::Disk {
        storage::Disk::test(
            DiskScope::capture(seed.workspace(), seed.user(), seed.target()).unwrap(),
            self.0.clone(),
        )
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("sqlite-journal"));
    }
}
fn record_parts() -> (OwnEditRecord, Vec<Value>) {
    let (shell, binding, policy) = setup();
    let record = intent(&shell, &binding, &policy).record().clone();
    let editable =
        record.blocks()[..record.blocks().len() - record.seed().preserved().len()].to_vec();
    (record, editable)
}

#[tokio::test]
async fn plain_capture_reopens_as_delta_and_rejects_rich_overwrite() {
    use super_platinum_core::slack_lists::comments::{
        edit_seed::plain::{PlainObservations, PlainSeed},
        ordered::ResponseWitness,
    };
    let (shell, binding, _) = setup();
    let original = session(&shell, &binding).unwrap().seed.clone();
    let raw = original.http_raw();
    let bytes = serde_json::to_vec(raw).unwrap();
    let ordered = ResponseWitness::decode(&bytes, raw).unwrap();
    let message = ordered.message(1).unwrap();
    let page = Page::parse_ordered(original.target(), ordered).unwrap();
    let mut observed = PlainObservations::default();
    observed.seed.emoji_as_text = Some(true);
    observed.highlight_words = Some(vec![]);
    let initialized = PlainSeed::capture(&message, &observed).unwrap();
    let policy = OwnEditPolicy::observed(
        original.workspace(),
        original.user(),
        json!({"id":original.target().channel_id()}),
        true,
        false,
        vec![-1],
        true,
        false,
    )
    .unwrap();
    let seed = OwnEditSeed::capture_plain(
        original.workspace(),
        original.user(),
        &page,
        &page.messages()[1],
        &initialized,
        &policy,
        1_700_000_100_000,
    )
    .unwrap();
    let local = Local::new();
    let disk = local.disk(&seed);
    let delta = PlainDelta::capture(vec![json!({"insert":"retained plain draft\n"})]).unwrap();
    let edit = Durable::load(disk.clone(), TS.into()).await.unwrap();
    edit.capture_content(&seed, &Content::PlainDelta(delta.clone()))
        .await
        .unwrap();
    assert!(edit.capture(&seed, &[]).await.is_err());
    drop(edit);
    let reopened = Durable::load(disk, TS.into()).await.unwrap();
    let inventory = reopened.inventory().await;
    assert_eq!(inventory.plain_delta(), Some(&delta));
    assert!(inventory.blocks().is_empty());
    assert!(inventory.attempt().is_none());
}

#[tokio::test]
async fn reopened_edit_preserves_original_seed_empty_rich_buffer_and_independent_reply() {
    let (record, blocks) = record_parts();
    let local = Local::new();
    let disk = local.disk(record.seed());
    let scope = disk.scope.clone();
    let reply = rich("Independent reply A 中🙂");
    let expected_reply = reply.clone();
    disk.work(move |j| {
        let inventory = j.inventory(&scope)?;
        j.save(&inventory, &uuid::Uuid::new_v4().to_string(), reply)
    })
    .await
    .unwrap();
    let edit = Durable::load(disk.clone(), TS.into()).await.unwrap();
    edit.capture(record.seed(), &blocks).await.unwrap();
    drop(edit);
    let reopened = Durable::load(disk.clone(), TS.into()).await.unwrap();
    let inventory = reopened.inventory().await;
    assert_eq!(inventory.seed(), Some(record.seed()));
    assert_eq!(inventory.blocks(), blocks);
    assert!(inventory.attempt().is_none());
    reopened.capture(record.seed(), &[]).await.unwrap();
    drop(reopened);
    let reopened = Durable::load(disk.clone(), TS.into()).await.unwrap();
    assert!(reopened.inventory().await.blocks().is_empty());
    let scope = disk.scope.clone();
    assert_eq!(
        disk.work(move |j| j.inventory(&scope))
            .await
            .unwrap()
            .blocks(),
        expected_reply
    );
}

#[tokio::test]
async fn committed_pending_attempt_reopens_as_unknown_and_cannot_begin_or_clear() {
    let (record, blocks) = record_parts();
    let local = Local::new();
    let disk = local.disk(record.seed());
    let edit = Durable::load(disk.clone(), TS.into()).await.unwrap();
    edit.capture(record.seed(), &blocks).await.unwrap();
    let id = edit.begin(record.clone()).await.unwrap();
    drop(edit);
    let restored = Durable::load(disk.clone(), TS.into()).await.unwrap();
    let inventory = restored.inventory().await;
    let a = restored_attempt(&inventory, &disk.account).unwrap();
    let a = a.lock().unwrap();
    assert_eq!(a.disk_id.as_deref(), Some(id.as_str()));
    assert!(a.phase == Phase::Unknown);
    assert!(a.receipt.is_none());
    drop(a);
    assert!(restored.begin(record).await.is_err());
    assert!(restored.clear().await.is_err());
    assert_eq!(restored.inventory().await.blocks(), blocks);
}

#[tokio::test]
async fn positive_readback_is_separate_from_original_unknown_transport_receipt() {
    let (record, blocks) = record_parts();
    let local = Local::new();
    let disk = local.disk(record.seed());
    let edit = Durable::load(disk.clone(), TS.into()).await.unwrap();
    edit.capture(record.seed(), &blocks).await.unwrap();
    let id = edit.begin(record.clone()).await.unwrap();
    edit.resolve(id.clone(), Outcome::Uncertain(None), 0)
        .await
        .unwrap();
    let mut raw = record.seed().http_raw().clone();
    let message = raw["messages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|m| m["ts"] == TS)
        .unwrap();
    message["blocks"] = json!(record.blocks());
    message["text"] = json!(record.text());
    let page = Page::parse(record.target(), raw).unwrap();
    edit.resolve(id, Outcome::Observed(page), 0).await.unwrap();
    drop(edit);
    let restored = Durable::load(disk.clone(), TS.into()).await.unwrap();
    let inventory = restored.inventory().await;
    assert!(inventory.attempt().unwrap().raw().is_none());
    assert!(inventory.attempt().unwrap().readback().is_some());
    assert!(
        restored_attempt(&inventory, &disk.account)
            .unwrap()
            .lock()
            .unwrap()
            .phase
            == Phase::Observed
    );
    restored.clear().await.unwrap();
    assert!(
        Durable::load(disk, TS.into())
            .await
            .unwrap()
            .inventory()
            .await
            .seed()
            .is_none()
    );
}

#[tokio::test]
async fn clear_does_not_allow_an_old_loaded_capture_to_resurrect_the_edit() {
    let (record, blocks) = record_parts();
    let local = Local::new();
    let disk = local.disk(record.seed());
    let edit = Durable::load(disk.clone(), TS.into()).await.unwrap();
    edit.capture(record.seed(), &blocks).await.unwrap();
    let old = Durable::load(disk.clone(), TS.into()).await.unwrap();
    edit.clear().await.unwrap();
    assert!(
        old.capture(record.seed(), &rich("Old callback"))
            .await
            .is_err()
    );
    assert!(
        Durable::load(disk, TS.into())
            .await
            .unwrap()
            .inventory()
            .await
            .seed()
            .is_none()
    );
}

#[tokio::test]
async fn a_failed_local_result_commit_is_retried_before_the_next_native_capture_ack() {
    let (record, blocks) = record_parts();
    let local = Local::new();
    let disk = local.disk(record.seed());
    let edit = Durable::load(disk.clone(), TS.into()).await.unwrap();
    edit.capture(record.seed(), &blocks).await.unwrap();
    let id = edit.begin(record.clone()).await.unwrap();
    // Isolated synthetic test database only; retain and restore its exact bytes.
    let original = std::fs::read(&local.0).unwrap();
    std::fs::write(&local.0, b"temporary invalid test database").unwrap();
    assert!(edit.resolve(id, Outcome::Uncertain(None), 0).await.is_err());
    std::fs::write(&local.0, original).unwrap();
    edit.capture(record.seed(), &blocks).await.unwrap();
    let restored = Durable::load(disk, TS.into()).await.unwrap();
    assert!(
        restored
            .inventory()
            .await
            .attempt()
            .unwrap()
            .phase()
            .locked()
    );
    assert!(restored.clear().await.is_err());
}

#[tokio::test]
async fn recovered_confirmed_receipt_publishes_once_without_replacing_newer_content() {
    let (mut shell, binding, policy) = setup();
    let intent = intent(&shell, &binding, &policy);
    let record = intent.record().clone();
    let raw = json!({"ok":true,"channel":"CCOMMENT01","ts":TS,
        "message":{"blocks":record.blocks(),"text":record.text(),
        "edited":{"ts":"1700000100.000001"}}});
    let accepted = intent.accept(raw.clone()).unwrap();
    let local = Local::new();
    let disk = local.disk(record.seed());
    let edit = Durable::load(disk.clone(), TS.into()).await.unwrap();
    let blocks =
        record.blocks()[..record.blocks().len() - record.seed().preserved().len()].to_vec();
    edit.capture(record.seed(), &blocks).await.unwrap();
    let id = edit.begin(record.clone()).await.unwrap();
    let blocks = rich("Newer unsent edit");
    let original = std::fs::read(&local.0).unwrap();
    std::fs::write(&local.0, b"temporary invalid test database").unwrap();
    assert!(
        edit.resolve(id, Outcome::Confirmed(accepted), 0)
            .await
            .is_err()
    );
    assert!(edit.capture(record.seed(), &blocks).await.is_err());
    std::fs::write(&local.0, original).unwrap();
    let publication = edit.capture(record.seed(), &blocks).await.unwrap().unwrap();
    assert_eq!(publication.raw(), &raw);
    assert_eq!(edit.inventory().await.blocks(), blocks);
    assert_eq!(edit.inventory().await.attempt().unwrap().raw(), Some(&raw));
    assert!(publish::accepted(&mut shell, &binding, &publication));
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert_eq!(source::message(entry, TS).unwrap().blocks, record.blocks());
    assert!(
        edit.capture(record.seed(), &blocks)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!publish::accepted(&mut shell, &binding, &publication));

    let (mut newer, newer_binding, _) = setup();
    let entry = newer.slack_lists.comments.entry.as_mut().unwrap();
    let mut page = entry.page.as_ref().unwrap().raw().clone();
    page["messages"][1]["blocks"] = json!(rich("Newer server body"));
    entry.page = Some(Page::parse(&entry.target, page).unwrap());
    assert!(!publish::accepted(&mut newer, &newer_binding, &publication));
    assert_eq!(
        source::message(newer.slack_lists.comments.entry.as_ref().unwrap(), TS)
            .unwrap()
            .blocks,
        rich("Newer server body")
    );
}

#[test]
fn recovered_original_seed_blocks_saving_against_a_newer_body_but_keeps_cancel() {
    let (mut shell, binding, _) = setup();
    ready(&mut shell, &binding, "restored-seed");
    let old_seed = session(&shell, &binding).unwrap().seed.clone();
    let e = shell.slack_lists.comments.entry.as_mut().unwrap();
    // Rebind to a new visit's observed source while keeping the saved old seed.
    let mut raw = e.page.as_ref().unwrap().raw().clone();
    raw["messages"][1]["blocks"] = json!(rich("New server body"));
    e.page = Some(Page::parse(&e.target, raw).unwrap());
    let source = Arc::new(Source::capture(&shell, binding.source.owner.serial, TS).unwrap());
    let s = shell.slack_lists.comments.own_edit.active.as_mut().unwrap();
    s.binding.source = source;
    s.seed = old_seed;
    let p = presentation(&shell).unwrap();
    assert!(!p.can_save);
    assert!(p.can_cancel);
}

#[test]
fn local_finish_failure_keeps_retry_return_available_without_a_native_editor() {
    let (mut shell, binding, _) = setup();
    let s = session_mut(&mut shell, &binding).unwrap();
    s.finishing = true;
    s.native = None;
    s.native_ready = false;
    let p = presentation(&shell).unwrap();
    assert!(p.finishing && p.can_cancel);
    assert!(!p.can_save && !p.can_check && !p.can_retry_capture);
}

#[test]
fn positive_older_readback_cannot_automatically_discard_a_newer_edit_buffer() {
    let (mut shell, binding, policy) = setup();
    let a = attempt(&shell, &binding, &policy, Phase::Observed);
    let s = session_mut(&mut shell, &binding).unwrap();
    s.blocks = rich("Edited 日");
    s.attempt = Some(a);
    assert!(is_attempt_buffer(s));
    s.blocks = rich("Newer edit B 中🙂");
    assert!(!is_attempt_buffer(s));
}
