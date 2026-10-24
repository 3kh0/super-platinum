//! Synthetic replies preserve tombstones and feed the ordinary sealed custody path.
use super::*;
use super_platinum_core::canvas::edit::{structure::AcceptedStructure, text::AcceptedText};
fn ack_rows(
    data: &RawMessage,
    source: &CanvasCollabSnapshot,
) -> (EditAck, Arc<CanvasCollabSnapshot>, Value) {
    let update = Section::from_raw(data.message(1).unwrap().unwrap()).unwrap();
    let old = source.section(update.id());
    let sequence = old.and_then(|s| s.sequence().unwrap()).unwrap_or(0) + 1000;
    let canonical = old.map_or_else(|| update.clone(), |s| s.merge_partial(&update).unwrap());
    let mut raw = canonical.raw().clone();
    if old.is_none() {
        raw = raw
            .replace_fields(6, &[t(6, source.thread_id())])
            .unwrap()
            .replace_fields(7, &[t(7, source.document_id())])
            .unwrap();
        if let Some(position) = raw
            .string(8)
            .unwrap()
            .and_then(|p| p.strip_suffix(":temp"))
            .map(str::to_owned)
        {
            raw = raw
                .replace_fields(8, &[t(8, &position)])
                .unwrap()
                .replace_fields(21, &[t(21, &position)])
                .unwrap();
        }
    }
    let wire = update
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
        .unwrap();
    let raw = raw
        .replace_fields(2, &[n(2, sequence)])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
    let bytes = msg(&[
        n(1, 1),
        m(2, &msg(&[t(15, source.document_id()), m(1, &raw)])),
    ]);
    let ack = EditAck::decode(bytes.raw()).unwrap();
    let mut fresh = source.clone();
    let canonical = Section::from_raw(raw).unwrap();
    if let Some(row) = fresh.sections.iter_mut().find(|s| s.id() == update.id()) {
        *row = canonical;
    } else {
        fresh.sections.push(canonical);
    }
    let evidence = json!({"raw":STANDARD.encode(bytes.raw()),"canonical":seed::saved(&fresh)});
    (ack, Arc::new(fresh), evidence)
}
pub(super) fn delete(
    p: PreparedStructureEdit,
    source: Arc<CanvasCollabSnapshot>,
) -> (AcceptedStructure, Arc<CanvasCollabSnapshot>, Value) {
    let (ack, fresh, e) = ack_rows(p.document_data(), &source);
    let a = p.validate_ack(&ack).unwrap();
    (a, fresh, e)
}
pub(super) fn text(
    p: PreparedTextEdit,
    source: Arc<CanvasCollabSnapshot>,
) -> (AcceptedText, Arc<CanvasCollabSnapshot>, Value) {
    let (ack, fresh, e) = ack_rows(p.document_data(), &source);
    let a = p.validate_ack(&ack).unwrap();
    (a, fresh, e)
}
fn token_current(shell: &ShellState, source: &Arc<CanvasCollabSnapshot>, token: &Arc<()>) -> bool {
    same_source(shell, source)
        && shell
            .canvas_checklist
            .structure_fixture
            .as_ref()
            .and_then(|f| f.owner_delete.as_ref())
            .and_then(|f| f.completing.as_ref())
            .is_some_and(|t| Arc::ptr_eq(t, token))
}
pub(super) async fn complete(mut state: Signal<ShellState>, unknown: bool) -> Result<(), String> {
    let (source, attempt, p, token, session, sequence) = {
        let mut shell = state.write();
        if !enabled(&shell) {
            return Err("Fixture owner changed".into());
        }
        let source = snapshot(&shell);
        let f = fixture(&mut shell);
        if f.completing.is_some() {
            return Err("Completion already running".into());
        }
        let (a, p) = f.pending.take().ok_or("No pending deletion")?;
        let token = Arc::new(());
        f.completing = Some(token.clone());
        let session = p.session_id().to_owned();
        let sequence = p.sequence();
        (source, a, p, token, session, sequence)
    };
    let previous = source.clone();
    // An owned omitted-endpoint restart must load the exact failed read, not the
    // original live child or the complete ACK's retained tombstone.
    let persist_omitted =
        (child_delete::requested() || retained_pair::requested() || owner_first::requested())
            && phase() == "omitted";
    let target = attempt.section.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut result = delete(p, previous);
        if persist_omitted {
            let mut omitted = (*result.1).clone();
            omitted.sections.retain(|s| s.id() != target);
            result.1 = Arc::new(omitted);
        }
        if unknown || persist_omitted {
            std::fs::write(
                snapshot_path(),
                serde_json::to_vec(&seed::saved(&result.1)).unwrap(),
            )
            .unwrap();
        }
        result
    })
    .await
    .map_err(|_| "Deletion completion worker failed")?;
    let (a, mut fresh, e) = result;
    let omitted = phase() == "omitted";
    if omitted {
        let mut rows = (*fresh).clone();
        rows.sections.retain(|s| s.id() != attempt.section);
        fresh = Arc::new(rows);
    }

    {
        let mut shell = state.write();
        if !token_current(&shell, &source, &token)
            || !checklist::structure_ops::fixture_completion_current(
                &shell, &attempt, &session, sequence,
            )
        {
            return Err("Deletion completion owner changed".into());
        }
        let f = fixture(&mut shell);
        f.completing = None;
        f.acknowledgements.push(e);
        shell
            .canvas_checklist
            .structure_fixture
            .as_mut()
            .unwrap()
            .canonical = Some(fresh.clone());
        if unknown {
            checklist::finish(&mut shell, &attempt, checklist::Outcome::Uncertain);
        } else if !checklist::structure_ops::finish_structure(&mut shell, &attempt, a) {
            return Err("Sealed deletion ACK rejected".into());
        }
    }
    let read = checklist::begin_read(&mut state.write()).ok_or("Read unavailable")?;
    if !checklist::finish_read(&mut state.write(), &read, Ok(fresh)) {
        return Err("Read owner changed".into());
    }
    if !unknown {
        checklist::recovery::settle(state, &read).await;
        if !omitted && !checklist::text_ops::settled(&state.peek(), &attempt) {
            return Err("Deletion did not settle after worker proof and durable retirement".into());
        }
    }
    Ok(())
}
pub(super) async fn complete_text(mut state: Signal<ShellState>) -> Result<(), String> {
    let (source, attempt, p, mut lease, token, session, sequence) = {
        let mut shell = state.write();
        let source = snapshot(&shell);
        let f = fixture(&mut shell);
        if f.completing.is_some() {
            return Err("Completion running".into());
        }
        let (a, p, l) = f.pending_text.take().ok_or("No pending child edit")?;
        let token = Arc::new(());
        f.completing = Some(token.clone());
        let session = p.session_id().to_owned();
        let sequence = p.sequence();
        (source, a, p, l, token, session, sequence)
    };
    let claim = lease.claim();
    let previous = source.clone();
    let result = tokio::task::spawn_blocking(move || {
        let (a, fresh, e) = text(p, previous);
        let memo = lease.remember(a);
        (lease, memo, fresh, e)
    })
    .await;
    let (lease, memo, fresh, e) = match result {
        Ok(v) => v,
        Err(_) => {
            claim.poison(&mut state.write());
            return Err("Child completion worker failed".into());
        }
    };
    let read = {
        let mut shell = state.write();
        let owned = claim.owns(&shell);
        lease.restore(&mut shell);
        if !owned
            || !token_current(&shell, &source, &token)
            || !checklist::text_ops::fixture_completion_current(
                &shell, &attempt, &session, sequence,
            )
        {
            return Err("Child completion owner changed".into());
        }
        let f = fixture(&mut shell);
        f.completing = None;
        f.acknowledgements.push(e);
        shell
            .canvas_checklist
            .structure_fixture
            .as_mut()
            .unwrap()
            .canonical = Some(fresh.clone());
        if !checklist::text_ops::finish_text_with_memo(&mut shell, &attempt, memo) {
            return Err("Child ACK rejected".into());
        }
        let read = checklist::begin_read(&mut shell).ok_or("Child read unavailable")?;
        if !checklist::finish_read(&mut shell, &read, Ok(fresh)) {
            return Err("Child read changed".into());
        }
        checklist::confirm_readback(&mut shell, &attempt, &read);
        read
    };
    checklist::recovery::settle(state, &read).await;
    if !checklist::text_ops::settled(&state.peek(), &attempt) {
        return Err("Child readback unconfirmed".into());
    }
    Ok(())
}
