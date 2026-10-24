//! End fixture ACK/canonical work uses a worker; shell authority stays serial.
use super::pair::AcceptedPair;
use super::*;

enum Outcome {
    Partial {
        accepted: bool,
    },
    Complete {
        accepted: AcceptedPair,
        fresh: Arc<CanvasCollabSnapshot>,
        inventory: Value,
    },
    Rejected,
}

fn build(
    prepared: PreparedPair,
    previous: Arc<CanvasCollabSnapshot>,
    partial: bool,
) -> (Value, Outcome) {
    let rows = authored_rows(&prepared, &previous);
    let ack_rows = if partial { &rows[..1] } else { &rows[..] };
    let mut data = vec![t(15, previous.document_id())];
    data.extend(ack_rows.iter().map(|row| m(1, row)));
    let bytes = msg(&[n(1, 1), m(2, &msg(&data))]);
    let ack = EditAck::decode(bytes.raw()).unwrap();
    let accepted = prepared.validate_ack(&ack);
    let evidence =
        json!({"raw":STANDARD.encode(bytes.raw()),"partial":partial,"accepted":accepted.is_ok()});
    if partial {
        return (
            evidence,
            Outcome::Partial {
                accepted: accepted.is_ok(),
            },
        );
    }
    let Ok(accepted) = accepted else {
        return (evidence, Outcome::Rejected);
    };
    let mut canonical = Vec::new();
    for row in &previous.sections {
        if is_reuse() {
            canonical.push(
                rows.iter()
                    .find(|updated| updated.string(1).ok().flatten() == Some(row.id()))
                    .cloned()
                    .unwrap_or_else(|| row.raw().clone()),
            );
        } else if row.id() == parent_id() {
            canonical.extend(rows.clone());
        } else {
            canonical.push(row.raw().clone());
        }
    }
    if (is_repeat() || is_sequence()) && is_reuse() {
        canonical.extend(
            rows.iter()
                .filter(|row| previous.section(row.string(1).unwrap().unwrap()).is_none())
                .cloned(),
        );
    }
    let fresh = rebuild(&previous, canonical);
    let inventory = inventory(&fresh);
    // The normal readback gate validates this exact immutable snapshot before
    // retiring custody. Do not duplicate all CRDT proofs on the fixture task.
    (
        evidence,
        Outcome::Complete {
            accepted,
            fresh,
            inventory,
        },
    )
}

pub(super) async fn end(
    mut state: Signal<ShellState>,
    attempt: checklist::Attempt,
    prepared: PreparedPair,
    previous: Arc<CanvasCollabSnapshot>,
) -> bool {
    let source = previous.clone();
    let partial = scenario() == "partial";
    let Ok((acknowledgement, outcome)) =
        tokio::task::spawn_blocking(move || build(prepared, source, partial)).await
    else {
        return false;
    };
    {
        let shell = state.peek();
        if !enabled(&shell)
            || !checklist::end_ops::fixture_completion_current(&shell, &attempt)
            || !shell
                .canvas_checklist
                .split_fixture
                .as_ref()
                .and_then(|f| f.canonical.as_ref())
                .is_some_and(|current| Arc::ptr_eq(current, &previous))
        {
            return false;
        }
    }
    state
        .write()
        .canvas_checklist
        .split_fixture
        .as_mut()
        .unwrap()
        .acknowledgements
        .push(acknowledgement);
    let (accepted, fresh, inventory) = match outcome {
        Outcome::Partial { accepted: false } => {
            return checklist::finish(&mut state.write(), &attempt, checklist::Outcome::Uncertain);
        }
        Outcome::Partial { accepted: true } | Outcome::Rejected => return false,
        Outcome::Complete {
            accepted,
            fresh,
            inventory,
        } => (accepted, fresh, inventory),
    };
    {
        let mut shell = state.write();
        if !accepted.finish(&mut shell, &attempt) {
            return false;
        }
        let fixture = shell.canvas_checklist.split_fixture.as_mut().unwrap();
        fixture.readbacks.push(inventory);
        fixture.canonical = Some(fresh.clone());
    }
    let Some(read) = checklist::begin_read(&mut state.write()) else {
        return false;
    };
    checklist::finish_read(&mut state.write(), &read, Ok(fresh));
    // Includes the same worker-bound all-member proof as the live paired loader.
    checklist::recovery::settle(state, &read).await;
    checklist::text_ops::settled(&state.peek(), &attempt)
}
