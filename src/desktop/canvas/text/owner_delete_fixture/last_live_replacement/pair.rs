use super::*;
pub(super) fn preflight_source(
    source: &CanvasCollabSnapshot,
    variant: &str,
) -> Arc<CanvasCollabSnapshot> {
    let mut fresh = source.clone();
    if variant == "missing" {
        fresh.sections.retain(|s| s.id() != "paragraph");
    } else if variant == "mutated" {
        let row = fresh
            .sections
            .iter_mut()
            .find(|s| s.id() == "paragraph")
            .unwrap();
        *row = Section::from_raw(row.raw().replace_fields(2, &[n(2, 9001)]).unwrap()).unwrap();
    }
    Arc::new(fresh)
}
/// Full canonical server adapter: preserve every old row, including the target
/// tombstone. Both update ACK rows retain their actual CRDT response identity.
pub(super) fn response(
    data: &RawMessage,
    source: &CanvasCollabSnapshot,
    partial: bool,
) -> (EditAck, Arc<CanvasCollabSnapshot>, Value) {
    let mut rows = Vec::new();
    for update in data.messages(1).unwrap() {
        let update = Section::from_raw(update).unwrap();
        let old = source.section(update.id());
        let sequence = old.and_then(|s| s.sequence().unwrap()).unwrap_or(0) + 1000;
        let mut raw = old
            .map_or_else(|| update.clone(), |s| s.merge_partial(&update).unwrap())
            .raw()
            .clone();
        raw = raw
            .replace_fields(2, &[n(2, sequence)])
            .unwrap()
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
        let wire = raw
            .message(38)
            .unwrap()
            .unwrap()
            .remove(12)
            .unwrap()
            .replace_fields(3, &[n(3, sequence)])
            .unwrap()
            .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
            .unwrap();
        rows.push(Section::from_raw(raw.replace_fields(38, &[m(38, &wire)]).unwrap()).unwrap());
    }
    assert_eq!(rows.len(), 2);
    let ack_rows = if partial { &rows[..1] } else { rows.as_slice() };
    let bytes = msg(&[
        n(1, 1),
        m(
            2,
            &msg(&ack_rows.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>()),
        ),
    ]);
    let ack = EditAck::decode(bytes.raw()).unwrap();
    let mut fresh = source.clone();
    for row in rows {
        if let Some(old) = fresh.sections.iter_mut().find(|s| s.id() == row.id()) {
            *old = row;
        } else {
            fresh.sections.push(row);
        }
    }
    let proof = json!({"raw":STANDARD.encode(bytes.raw()),"canonical":seed::saved(&fresh),"partial":partial});
    (ack, Arc::new(fresh), proof)
}
pub(super) async fn release(mut state: Signal<ShellState>) -> Result<String, String> {
    let (attempt, prepared, ticket) = state
        .write()
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .reserved_replacement
        .take()
        .ok_or("Missing reserved pair")?;
    let id = checklist::replacement_ops::replacement(&attempt)
        .ok_or("Missing replacement id")?
        .to_owned();
    if !checklist::replacement_ops::mark_dispatched(&mut state.write(), &attempt, &prepared) {
        let _ = checklist::recovery::complete(state, &ticket).await;
        return Err("Reserved pair lost dispatch authority".into());
    }
    let evidence = json!({"kind":"replacement","target":attempt.section,"replacement":id,
        "session":prepared.session_id(),"sequence":prepared.sequence(),
        "documentData":STANDARD.encode(prepared.document_data().raw()),"httpFactoryExecuted":false});
    let mut shell = state.write();
    fixture(&mut shell).writes.push(evidence);
    let f = shell.canvas_checklist.structure_fixture.as_mut().unwrap();
    f.attempts += 1;
    f.pending_replacement = Some((attempt, prepared));
    Ok(id)
}
pub(super) async fn complete(
    mut state: Signal<ShellState>,
    variant: &str,
) -> Result<checklist::Attempt, String> {
    let source = snapshot(&state.peek());
    let (attempt, prepared) = state
        .write()
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .pending_replacement
        .take()
        .ok_or("Missing dispatched pair")?;
    let previous = source.clone();
    let mode = variant.to_owned();
    let (accepted, fresh, mut evidence) = tokio::task::spawn_blocking(move || {
        let (ack, full, proof) = response(prepared.document_data(), &previous, mode == "partial");
        let accepted = prepared.validate_ack(&ack).ok();
        let fresh = if matches!(mode.as_str(), "missing" | "mutated") {
            preflight_source(&full, &mode)
        } else {
            full
        };
        (accepted, fresh, proof)
    })
    .await
    .map_err(|_| "Pair completion worker failed")?;
    evidence["accepted"] = json!(accepted.is_some());
    if variant == "partial" && accepted.is_some() {
        return Err("Partial ACK admitted".into());
    }
    if variant != "partial" && accepted.is_none() {
        return Err("Full ACK refused".into());
    }
    if variant == "retired" {
        if checklist::presentation(&state.peek()).is_some_and(|v| v.visit == attempt.visit()) {
            return Err("Retirement retained old visit".into());
        }
        // The old operation may consume its own response; it cannot publish into
        // the replacement view. This is the same owner gate as the worker path.
        let consumed = checklist::replacement_ops::finish_replacement(
            &mut state.write(),
            &attempt,
            accepted.unwrap(),
        );
        if !same_source(&state.peek(), &source) {
            return Err("Late ACK changed current canonical".into());
        }
        evidence["oldOperationConsumed"] = json!(consumed);
        evidence["publishedToCurrentView"] = json!(false);
        fixture(&mut state.write())
            .acknowledgements
            .push(evidence.clone());
        fixture(&mut state.write()).last_live.completion = Some(evidence);
        return Ok(attempt);
    }
    if !same_source(&state.peek(), &source)
        || checklist::presentation(&state.peek()).is_none_or(|v| v.visit != attempt.visit())
    {
        return Err("Pair completion owner changed".into());
    }
    // Persist the exact response candidate for a separate fresh-process restart.
    let saved = seed::saved(&fresh);
    let path = snapshot_path();
    tokio::task::spawn_blocking(move || std::fs::write(path, serde_json::to_vec(&saved).unwrap()))
        .await
        .map_err(|_| "Pair snapshot worker failed")?
        .map_err(|_| "Pair snapshot write failed")?;
    let mut shell = state.write();
    let f = fixture(&mut shell);
    f.acknowledgements.push(evidence.clone());
    f.last_live.completion = Some(evidence);
    shell
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .canonical = Some(fresh);
    let finished = if matches!(variant, "partial" | "unknown") {
        checklist::finish(&mut shell, &attempt, checklist::Outcome::Uncertain)
    } else {
        checklist::replacement_ops::finish_replacement(&mut shell, &attempt, accepted.unwrap())
    };
    if !finished {
        return Err("Pair completion lost its operation".into());
    }
    Ok(attempt)
}
pub(super) async fn read_and_settle(
    mut state: Signal<ShellState>,
    settle: bool,
) -> Result<checklist::Read, String> {
    let source = snapshot(&state.peek());
    let read = checklist::begin_read(&mut state.write()).ok_or("Pair read unavailable")?;
    checklist::recovery::load(state, &read).await;
    // Decode a genuinely untagged server snapshot. Only the production carry
    // worker may transfer prior local authority into this fresh read.
    let fresh = tokio::task::spawn_blocking(move || seed::restored(&seed::saved(&source)))
        .await
        .map_err(|_| "Fresh read decode worker failed")?;
    let fresh = checklist::replacement_ops::readback::carry_for_read(state, &read, fresh).await?;
    if !checklist::finish_read(&mut state.write(), &read, Ok(fresh)) {
        return Err("Pair read superseded".into());
    }
    let pending = checklist::pending_attempt(&state.peek());
    if let Some(attempt) = pending {
        checklist::confirm_readback(&mut state.write(), &attempt, &read);
    }
    if settle {
        checklist::recovery::settle(state, &read).await;
    }
    Ok(read)
}
pub(super) async fn retain_confirmed(mut state: Signal<ShellState>) -> Result<(), String> {
    let fresh = {
        let shell = state.peek();
        let view = checklist::presentation(&shell).ok_or("Confirmed pair view missing")?;
        if view.loading || view.stale || view.operation.is_some() {
            return Err("Pair not settled".into());
        }
        view.snapshot.ok_or("Confirmed pair snapshot missing")?
    };
    // This copies the exact production-certified snapshot, never constructs a
    // provenance token from the fixture's canonical bytes or ACK observation.
    state
        .write()
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .canonical = Some(fresh.clone());
    let saved = seed::saved(&fresh);
    let path = snapshot_path();
    tokio::task::spawn_blocking(move || std::fs::write(path, serde_json::to_vec(&saved).unwrap()))
        .await
        .map_err(|_| "Confirmed snapshot worker failed")?
        .map_err(|_| "Confirmed snapshot write failed")?;
    Ok(())
}
