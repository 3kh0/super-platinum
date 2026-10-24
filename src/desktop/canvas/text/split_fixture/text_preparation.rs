//! Multi-peer fixture preparation holds the real session through ACK adoption.
use super::*;
use checklist::session_worker::Claim;
const FAILED: &str =
    "This canvas edit session is unavailable. Sign out and sign in again before editing.";
fn available(f: &State) -> bool {
    (f.writes.len() == 1
        || ((is_middle_typing() || is_start() || is_empty()) && f.writes.len() == 2)
        || (second_middle::enabled() && (1..=3).contains(&f.writes.len()))
        || (is_repeat() && (1..=3).contains(&f.writes.len()))
        || (is_sequence() && (1..=sequence::count() + 1).contains(&f.writes.len())))
        && f.pending.is_none()
        && f.pending_start_typing.is_none()
        && f.pending_start.is_none()
        && f.pending_text.is_none()
        && f.text_completing.is_none()
}
fn current(
    shell: &ShellState,
    attempt: &checklist::Attempt,
    claim: &Claim,
    fresh: &Arc<CanvasCollabSnapshot>,
    writes: usize,
) -> bool {
    enabled(shell)
        && scenario() == "success"
        && claim.preparing(shell, attempt)
        && shell
            .canvas_checklist
            .split_fixture
            .as_ref()
            .is_some_and(|f| {
                available(f)
                    && f.writes.len() == writes
                    && f.canonical.as_ref().is_some_and(|c| Arc::ptr_eq(c, fresh))
            })
}
pub(in crate::canvas::text) async fn dispatch(
    mut state: Signal<ShellState>,
    attempt: &checklist::Attempt,
    intent: Arc<TextIntent>,
) -> Result<(), &'static str> {
    let (fresh, writes) = {
        let shell = state.peek();
        if !enabled(&shell) || scenario() != "success" || intent.associated_peer_count() == 0 {
            return Err("The disconnected canvas fixture is unavailable.");
        }
        let f = shell.canvas_checklist.split_fixture.as_ref().unwrap();
        if !available(f) {
            return Err("Wait for the previous canvas change to finish.");
        }
        (f.canonical.clone().unwrap(), f.writes.len())
    };
    let lease = checklist::session_worker::acquire(&mut state.write(), attempt)?;
    let claim = lease.claim();
    let Some(now) = checklist::save::now_usec() else {
        lease.restore(&mut state.write());
        return Err("Check your system clock before editing.");
    };
    let prepared = super::super::save::multi_peer::prepare(
        lease,
        intent,
        fresh.clone(),
        checklist::text_ops::draft(attempt).unwrap().clone(),
        now,
    )
    .await;
    let (lease, prepared) = match prepared {
        Ok(result) => result,
        Err(()) => {
            claim.poison(&mut state.write());
            return Err(FAILED);
        }
    };
    let source = attempt.snapshot.clone();
    let canonical = fresh.clone();
    let result=tokio::task::spawn_blocking(move || {
        let outcome=match &prepared {Ok(_)=>"prepared".to_owned(),Err(e)=>format!("{e:?}")};
        let preflight=json!({"source":inventory(&source),"fresh":inventory(&canonical),"outcome":outcome,"kind":"text"});
        let write=prepared.as_ref().ok().map(|p|json!({"kind":"text","parent":p.section_id(),"session":p.session_id(),
            "sequence":p.sequence(),"formTitle":p.form_title(),"documentData":STANDARD.encode(p.document_data().raw()),"httpFactoryExecuted":false}));
        (lease,prepared,preflight,write)
    }).await;
    let (lease, prepared, preflight, write) = match result {
        Ok(result) => result,
        Err(_) => {
            claim.poison(&mut state.write());
            return Err(FAILED);
        }
    };
    let mut shell = state.write();
    if !current(&shell, attempt, &claim, &fresh, writes) {
        lease.restore(&mut shell);
        return Err("This canvas changed before saving.");
    }
    shell
        .canvas_checklist
        .split_fixture
        .as_mut()
        .unwrap()
        .preflights
        .push(preflight);
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(_) => {
            lease.restore(&mut shell);
            return Err("This text or your access changed. Cancel and refresh.");
        }
    };
    if !checklist::text_ops::mark_dispatched(&mut shell, attempt, &prepared) {
        lease.restore(&mut shell);
        return Err("This canvas changed before saving.");
    }
    let fixture = shell.canvas_checklist.split_fixture.as_mut().unwrap();
    fixture
        .writes
        .push(write.expect("successful preparation evidence"));
    fixture.pending_text = Some((attempt.clone(), prepared, Some(lease)));
    Ok(())
}
