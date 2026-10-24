//! Fixture ACK construction and canonical evidence run off the UI task.
use super::*;
use super_platinum_core::canvas::edit::text::{AcceptedText, PreparedTextEdit};

enum Acceptance {
    Ordinary(AcceptedText),
    Remembered(checklist::session_worker::Memo),
}
struct Completed {
    accepted: Acceptance,
    fresh: Arc<CanvasCollabSnapshot>,
    inventory: Value,
}
fn build(
    prepared: PreparedTextEdit,
    previous: Arc<CanvasCollabSnapshot>,
    replacement: String,
    parent: bool,
    mut lease: Option<checklist::session_worker::Lease>,
) -> (
    Option<checklist::session_worker::Lease>,
    Value,
    Option<Completed>,
) {
    let target = prepared.section_id().to_owned();
    let sent = prepared.document_data().message(1).unwrap().unwrap();
    let wire = sent
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, 3000)])
        .unwrap()
        .replace_fields(
            4,
            &[n(4, checklist::save::now_usec().unwrap() + 60_000_000)],
        )
        .unwrap();
    let row = previous
        .section(&target)
        .unwrap()
        .raw()
        .replace_fields(2, &[n(2, 3000)])
        .unwrap()
        .replace_fields(12, &[m(12, &body(&typing::escaped(&replacement), parent))])
        .unwrap()
        // Correlate this ACK with the new text attempt, not the preceding split.
        .replace_fields(20, &[n(20, sent.uint(20).unwrap().unwrap())])
        .unwrap()
        .replace_fields(33, &[t(33, sent.string(33).unwrap().unwrap())])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
    let ack_bytes = msg(&[
        n(1, 1),
        m(2, &msg(&[t(15, previous.document_id()), m(1, &row)])),
    ]);
    let ack = EditAck::decode(ack_bytes.raw()).unwrap();
    let accepted = prepared.validate_ack(&ack);
    let acknowledgement = json!({"kind":"text","raw":STANDARD.encode(ack_bytes.raw()),"accepted":accepted.is_ok(),"error":accepted.as_ref().err().map(|e|format!("{e:?}")),"partial":false});
    let Ok(accepted) = accepted else {
        return (lease, acknowledgement, None);
    };
    let fresh = rebuild(
        &previous,
        previous
            .sections
            .iter()
            .map(|s| {
                if s.id() == target {
                    row.clone()
                } else {
                    s.raw().clone()
                }
            })
            .collect(),
    );
    let inventory = inventory(&fresh);
    let accepted = match lease.as_mut() {
        Some(lease) => Acceptance::Remembered(lease.remember(accepted)),
        None => Acceptance::Ordinary(accepted),
    };
    // The normal readback worker proves this same immutable fresh snapshot.
    (
        lease,
        acknowledgement,
        Some(Completed {
            accepted,
            fresh,
            inventory,
        }),
    )
}

fn replacement(shell: &ShellState, target: &str) -> String {
    if (is_middle_typing() || is_start() || is_empty() || is_repeat() || is_sequence())
        && target == parent_id()
    {
        return corpus().edited.to_owned();
    }
    let fixture = shell.canvas_checklist.split_fixture.as_ref().unwrap();
    if is_sequence() || second_middle::enabled() {
        return fixture.writes[0]["members"]
            .as_array()
            .unwrap()
            .iter()
            .position(|m| m["id"] == target)
            .map(|index| {
                if second_middle::enabled() {
                    second_middle::edited(index + 1)
                } else {
                    sequence::edited(index)
                }
            })
            .unwrap_or_default();
    }
    if is_repeat() && fixture.writes[0]["secondChild"].as_str() == Some(target) {
        "Second paragraph edited".to_owned()
    } else {
        continued_text().to_owned()
    }
}

struct Fence {
    attempt: checklist::Attempt,
    previous: Arc<CanvasCollabSnapshot>,
    token: Arc<()>,
    session: String,
    sequence: u64,
}
impl Fence {
    fn current(&self, shell: &ShellState) -> bool {
        enabled(shell)
            && checklist::text_ops::fixture_completion_current(
                shell,
                &self.attempt,
                &self.session,
                self.sequence,
            )
            && shell
                .canvas_checklist
                .split_fixture
                .as_ref()
                .is_some_and(|fixture| current_fixture(fixture, &self.previous, &self.token))
    }
}

fn current_fixture(fixture: &State, previous: &Arc<CanvasCollabSnapshot>, token: &Arc<()>) -> bool {
    fixture.pending_text.is_none()
        && fixture
            .text_completing
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, token))
        && fixture
            .canonical
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, previous))
}

pub(in crate::canvas::text) async fn complete_text(mut state: Signal<ShellState>) -> bool {
    let (prepared, fence, replacement, parent, lease) = {
        let mut shell = state.write();
        if !enabled(&shell) {
            return false;
        }
        let fixture = shell.canvas_checklist.split_fixture.as_ref().unwrap();
        if fixture.text_completing.is_some() {
            return false;
        }
        let Some((attempt, prepared, lease)) = fixture.pending_text.as_ref() else {
            return false;
        };
        if lease
            .as_ref()
            .is_some_and(|lease| !lease.claim().owns(&shell))
        {
            return false;
        }
        let session = prepared.session_id().to_owned();
        let sequence = prepared.sequence();
        if !checklist::text_ops::fixture_completion_current(&shell, attempt, &session, sequence) {
            return false;
        }
        let replacement = replacement(&shell, prepared.section_id());
        let parent = prepared.section_id() == parent_id();
        let previous = fixture.canonical.clone().unwrap();
        let fixture = shell.canvas_checklist.split_fixture.as_mut().unwrap();
        let (attempt, prepared, lease) = fixture.pending_text.take().unwrap();
        let token = Arc::new(());
        fixture.text_completing = Some(token.clone());
        (
            prepared,
            Fence {
                attempt,
                previous,
                token,
                session,
                sequence,
            },
            replacement,
            parent,
            lease,
        )
    };
    let source = fence.previous.clone();
    let claim = lease.as_ref().map(|lease| lease.claim());
    let result =
        tokio::task::spawn_blocking(move || build(prepared, source, replacement, parent, lease))
            .await;
    let read = {
        let mut shell = state.write();
        let (lease, acknowledgement, outcome) = match result {
            Ok(result) => result,
            Err(_) => {
                if let Some(claim) = &claim {
                    claim.poison(&mut shell);
                }
                return false;
            }
        };
        let owned = claim.as_ref().is_none_or(|claim| claim.owns(&shell));
        if let Some(lease) = lease {
            lease.restore(&mut shell);
        }
        // No ACK, evidence, canonical data or receipt may attach to a replacement
        // attempt, account, visit, read, source object or fixture instance.
        if !owned || !fence.current(&shell) {
            return false;
        }
        shell
            .canvas_checklist
            .split_fixture
            .as_mut()
            .unwrap()
            .text_completing = None;
        shell
            .canvas_checklist
            .split_fixture
            .as_mut()
            .unwrap()
            .acknowledgements
            .push(acknowledgement);
        let Some(completed) = outcome else {
            return false;
        };
        let finished = match completed.accepted {
            Acceptance::Ordinary(accepted) => {
                checklist::text_ops::finish_text(&mut shell, &fence.attempt, accepted)
            }
            Acceptance::Remembered(memo) => {
                checklist::text_ops::finish_text_with_memo(&mut shell, &fence.attempt, memo)
            }
        };
        if !finished {
            return false;
        }
        let fixture = shell.canvas_checklist.split_fixture.as_mut().unwrap();
        fixture.readbacks.push(completed.inventory);
        fixture.canonical = Some(completed.fresh.clone());
        let Some(read) = checklist::begin_read(&mut shell) else {
            return false;
        };
        if !checklist::finish_read(&mut shell, &read, Ok(completed.fresh)) {
            return false;
        }
        // Ordinary/pair text receipts retain their synchronous gate. Worker
        // variants return false here until recovery::settle installs the proof.
        checklist::confirm_readback(&mut shell, &fence.attempt, &read);
        read
    };
    // Includes TextSequence all-peer proof for the actual canonical snapshot.
    checklist::recovery::settle(state, &read).await;
    checklist::text_ops::settled(&state.peek(), &fence.attempt)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_boundary_is_send_and_fixture_fence_requires_exact_source_and_token() {
        fn send<T: Send + 'static>() {}
        send::<PreparedTextEdit>();
        send::<Completed>();
        let source = crate::canvas::text::fixture::snapshot("Canonical", 1000);
        let token = Arc::new(());
        let mut fixture = State {
            canonical: Some(source.clone()),
            text_completing: Some(token.clone()),
            ..Default::default()
        };
        assert!(current_fixture(&fixture, &source, &token));
        assert!(
            !current_fixture(&fixture, &Arc::new((*source).clone()), &token),
            "equal bytes do not prove canonical object ownership"
        );
        assert!(
            !current_fixture(&fixture, &source, &Arc::new(())),
            "a replacement completion token cannot adopt this ACK"
        );
        fixture.text_completing = None;
        assert!(
            !current_fixture(&fixture, &source, &token),
            "a consumed completion is never replayed"
        );
        fixture.text_completing = Some(token.clone());
        fixture.canonical = None;
        assert!(!current_fixture(&fixture, &source, &token));
        assert!(
            !current_fixture(&State::default(), &source, &token),
            "a replacement fixture cannot resurrect old work"
        );
    }
}
