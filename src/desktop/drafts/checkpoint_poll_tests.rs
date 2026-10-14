use super::*;
use super_platinum_core::ComposerState;

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000099";
const EPOCH: u64 = 7;

// In-memory editor construction only. No enrollment, submission, disk operation,
// production account, transport, or asynchronous writer is started.
fn fixture(count: usize, text_bytes: usize) -> Queue {
    let account = Account::new(Backend::Isolated {
        account: ACCOUNT.into(),
        path: "unused-draft-poll-test.sqlite".into(),
    });
    let mut queue = Queue::default();
    for index in 0..count {
        let key = DraftKey {
            team: "T1".into(),
            channel: format!("C{index:04}"),
            thread: None,
        };
        let owner = account.owner(key.clone());
        let text = "x".repeat(text_bytes);
        let mut sync = DraftSync::new(owner.scope.clone());
        sync.edit(
            &owner.scope,
            Some(super_platinum_core::drafts::sync::Snapshot {
                blocks: vec![serde_json::json!({
                    "type":"section", "text":{"type":"plain_text", "text":text}
                })],
                ..Default::default()
            }),
        );
        queue
            .editor(
                &account,
                EPOCH,
                key,
                View {
                    local: LocalDraft {
                        composer: ComposerState::with_text(text),
                        attachments: vec![],
                    },
                    stored: Some(Stored {
                        revision: 1,
                        state: sync,
                    }),
                    blocked: None,
                    uncommitted: None,
                },
            )
            .unwrap();
    }
    queue
}

fn signatures(queue: &Queue) -> BTreeMap<DraftKey, Signature> {
    queue
        .snapshots(ACCOUNT, EPOCH)
        .into_iter()
        .map(|(key, snapshot)| (key, Signature::from(&snapshot)))
        .collect()
}

// Frozen reference of integration's former clone-all, then retire-or-compare
// decision. Retain reuses the allocated snapshot vector rather than adding a
// second allocation to the benchmark's reference path.
fn reference(queue: &Queue, previous: &BTreeMap<DraftKey, Signature>) -> Vec<(DraftKey, Snapshot)> {
    let mut snapshots = queue.snapshots(ACCOUNT, EPOCH);
    integration_decision(&mut snapshots, previous);
    snapshots
}

fn optimized(queue: &Queue, previous: &BTreeMap<DraftKey, Signature>) -> Vec<(DraftKey, Snapshot)> {
    let mut snapshots = queue.changed_snapshots(ACCOUNT, EPOCH, previous);
    // Production integration still constructs its signature for emitted values.
    // Include that work in changed/first-observation benchmark cases too.
    integration_decision(&mut snapshots, previous);
    snapshots
}

fn integration_decision(
    snapshots: &mut Vec<(DraftKey, Snapshot)>,
    previous: &BTreeMap<DraftKey, Signature>,
) {
    snapshots.retain(|(key, snapshot)| {
        (snapshot.view.stored.is_none()
            && !snapshot.running
            && !snapshot.unsaved
            && snapshot.error.is_none())
            || previous.get(key) != Some(&Signature::from(snapshot))
    });
}

fn outcome(snapshot: &Snapshot) -> serde_json::Value {
    serde_json::json!({
        "serial":snapshot.serial, "local":snapshot.view.local,
        "stored":snapshot.view.stored.as_ref().map(|s| (&s.revision, &s.state)),
        "blocked":snapshot.view.blocked.as_ref().map(|b| (&b.token, &b.draft)),
        "uncommitted":snapshot.view.uncommitted, "pending":snapshot.pending,
        "error":format!("{:?}", snapshot.error),
        "unsaved":snapshot.unsaved, "running":snapshot.running,
        "observing":snapshot.observing,
        "projection":snapshot.projection.as_ref().map(|barrier| &barrier.id),
    })
}

fn assert_outcomes(queue: &Queue, previous: &BTreeMap<DraftKey, Signature>, expected: usize) {
    let old = reference(queue, previous);
    let new = optimized(queue, previous);
    assert_eq!(old.len(), expected);
    assert_eq!(new.len(), old.len());
    assert_eq!(
        queue.changed_snapshots(ACCOUNT, EPOCH, previous).len(),
        expected
    );
    for ((old_key, old), (new_key, new)) in old.iter().zip(&new) {
        assert_eq!(old_key, new_key);
        assert_eq!(outcome(old), outcome(new));
    }
}

#[tokio::test]
async fn unchanged_poll_emits_nothing_but_first_observation_keeps_complete_snapshot() {
    let queue = fixture(3, 4096);
    assert_outcomes(&queue, &BTreeMap::new(), 3);
    assert_outcomes(&queue, &signatures(&queue), 0);
}

#[tokio::test]
async fn every_original_signature_transition_emits_the_same_owned_snapshot() {
    let changes: [fn(&mut State); 9] = [
        |s| s.serial += 1,
        |s| s.base.stored.as_mut().unwrap().revision += 1,
        |s| s.error = Some(Fault::Storage),
        |s| s.pending = Some((s.serial, s.base.local.clone())),
        |s| s.running = true,
        |s| s.settling = true,
        |s| s.observing = true,
        |s| {
            s.base.blocked = Some(BlockedLocal {
                token: "refused-input".into(),
                draft: s.base.local.clone(),
            })
        },
        |s| {
            s.base.stored = None;
            s.running = true; // Not an idle released writer: revision alone changes.
        },
    ];
    for change in changes {
        let queue = fixture(1, 1024);
        let previous = signatures(&queue);
        change(&mut queue.editors.values().next().unwrap().state.lock().unwrap());
        assert_outcomes(&queue, &previous, 1);
        assert_outcomes(&queue, &signatures(&queue), 0);
    }
    let queue = fixture(1, 1024);
    let editor = queue.editors.values().next().unwrap();
    editor.state.lock().unwrap().base.blocked = Some(BlockedLocal {
        token: "first-token".into(),
        draft: LocalDraft::default(),
    });
    let previous = signatures(&queue);
    editor
        .state
        .lock()
        .unwrap()
        .base
        .blocked
        .as_mut()
        .unwrap()
        .token = "new-token".into();
    assert_outcomes(&queue, &previous, 1);
}

#[tokio::test]
async fn comparison_keeps_original_content_and_combined_running_semantics() {
    let queue = fixture(1, 1024);
    let editor = queue.editors.values().next().unwrap();
    editor.state.lock().unwrap().running = true;
    let previous = signatures(&queue);
    {
        let mut state = editor.state.lock().unwrap();
        state.settling = true; // running || settling was already true.
        state.base.local.composer.text = "content without metadata change".into();
        state.base.uncommitted = Some(LocalDraft::default());
    }
    // Do not add a new content-based invalidation rule to the optimization.
    assert_outcomes(&queue, &previous, 0);
}

#[tokio::test]
async fn released_cleanup_bypasses_even_an_identical_cached_signature() {
    let queue = fixture(1, 1024);
    queue
        .editors
        .values()
        .next()
        .unwrap()
        .state
        .lock()
        .unwrap()
        .base
        .stored = None;
    assert_outcomes(&queue, &signatures(&queue), 1);
}

#[tokio::test]
async fn queue_scope_and_changed_subset_preserve_keys_and_order() {
    let queue = fixture(3, 256);
    let mut previous = signatures(&queue);
    previous.remove(queue.editors.keys().next().unwrap());
    queue
        .editors
        .values()
        .last()
        .unwrap()
        .state
        .lock()
        .unwrap()
        .serial += 1;
    assert_outcomes(&queue, &previous, 2);
    assert!(
        queue
            .changed_snapshots("other-account", EPOCH, &previous)
            .is_empty()
    );
    assert!(
        queue
            .changed_snapshots(ACCOUNT, EPOCH + 1, &previous)
            .is_empty()
    );
}

#[path = "checkpoint_poll_benchmark.rs"]
mod benchmark;
