//! Disconnected pair completion. Real journal workers and core seals, no transport.
use super::*;
use super_platinum_core::{
    canvas::{
        edit::structure::{AcceptedReplacement, PreparedReplacement, ReplacementIntent},
        wire::{EditAck, EncodedField as F, RawMessage, Section},
    },
    slack::api::CanvasCollabSnapshot,
};
pub(super) const ACCOUNT: &str = "00000000-0000-4000-8000-000000009997";
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-replacement" | "canvas-replacement-narrow")
        )
}
pub(super) fn enabled(shell: &ShellState) -> bool {
    requested() && structure_fixture::enabled(shell)
}
pub(in crate::canvas) fn snapshot() -> Arc<CanvasCollabSnapshot> {
    let mut snapshot = (*structure_fixture::snapshot()).clone();
    snapshot.sections.retain(|s| s.id() != "neighbor");
    // Retain opaque noncanonical bytes on an untouched title across both edits.
    let title = snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "title")
        .unwrap();
    let mut raw = title.raw().raw().to_vec();
    raw.extend_from_slice(&[0xc8, 0x3e, 0x81, 0x00]);
    *title = Section::decode(&raw).unwrap();
    Arc::new(snapshot)
}
pub(super) fn install_owner(shell: &mut ShellState, snapshot: &CanvasCollabSnapshot) -> bool {
    if !requested()
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_team.as_deref() != Some(snapshot.init.workspace_id())
    {
        return false;
    }
    let Some(user) = snapshot.init.options().string(33).ok().flatten() else {
        return false;
    };
    let Some(workspace) = shell.core.workspaces.get_mut(snapshot.init.workspace_id()) else {
        return false;
    };
    shell.core.active_account = Some(ACCOUNT.into());
    workspace.self_user_id = user.into();
    shell.self_account.user_id = user.into();
    true
}
pub(super) fn dispatch(
    mut state: Signal<ShellState>,
    draft: Signal<Option<Draft>>,
    generation: u64,
    attempt: checklist::Attempt,
    intent: ReplacementIntent,
) {
    if !enabled(&state.peek()) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let fresh = state
            .peek()
            .canvas_checklist
            .structure_fixture
            .as_ref()
            .unwrap()
            .canonical
            .clone()
            .unwrap();
        let prepared = match tokio::task::spawn_blocking(move || {
            intent.prepare(&fresh, checklist::save::now_usec().unwrap())
        })
        .await
        {
            Ok(Ok(p)) => p,
            _ => {
                replacement_save::reject(
                    state,
                    draft,
                    generation,
                    &attempt,
                    "Fixture preflight changed.",
                );
                return;
            }
        };
        let (prepared, ticket) =
            match checklist::recovery::reserve_replacement(state, &attempt, prepared).await {
                Ok(t) => t,
                Err(reason) => {
                    replacement_save::reject(state, draft, generation, &attempt, reason);
                    return;
                }
            };
        let mut shell = state.write();
        let fixture = shell.canvas_checklist.structure_fixture.as_mut().unwrap();
        fixture.pair_reservations += 1;
        fixture.reserved_replacement = Some((attempt, prepared, ticket));
    });
}
pub(super) async fn release(mut state: Signal<ShellState>) -> bool {
    if !enabled(&state.peek()) {
        return false;
    }
    let reserved = state
        .write()
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .reserved_replacement
        .take();
    let Some((attempt, prepared, ticket)) = reserved else {
        return false;
    };
    if !checklist::replacement_ops::mark_dispatched(&mut state.write(), &attempt, &prepared) {
        let _ = checklist::recovery::complete(state, &ticket).await;
        return false;
    }
    let mut shell = state.write();
    let fixture = shell.canvas_checklist.structure_fixture.as_mut().unwrap();
    fixture.attempts += 1;
    fixture.pending_replacement = Some((attempt, prepared));
    true
}
pub(super) async fn read(mut state: Signal<ShellState>) -> Result<checklist::Read, String> {
    if !enabled(&state.peek()) {
        return Err("Connected replacement read refused".into());
    }
    let snapshot = state
        .peek()
        .canvas_checklist
        .structure_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    let read = checklist::begin_read(&mut state.write()).ok_or("Missing fixture visit")?;
    checklist::recovery::load(state, &read).await;
    if !checklist::finish_read(&mut state.write(), &read, Ok(snapshot)) {
        return Err("Fixture read superseded".into());
    }
    let pending = checklist::pending_attempt(&state.peek());
    if let Some(attempt) = pending {
        checklist::confirm_readback(&mut state.write(), &attempt, &read);
    }
    Ok(read)
}
pub(in crate::canvas) fn accepted(
    prepared: PreparedReplacement,
    original: &CanvasCollabSnapshot,
    now: u64,
) -> (AcceptedReplacement, Arc<CanvasCollabSnapshot>) {
    let n = |tag, v| F::uint(tag, v).unwrap();
    let t = |tag, v: &str| F::bytes(tag, v.as_bytes()).unwrap();
    let m = |tag, v: &RawMessage| F::message(tag, v).unwrap();
    let mut sections = Vec::new();
    for update in prepared.document_data().messages(1).unwrap() {
        let update = Section::from_raw(update).unwrap();
        let old = original.section(update.id());
        let sequence = old.and_then(|s| s.sequence().unwrap()).unwrap_or(0) + 1000;
        let mut raw = old
            .map_or_else(|| update.clone(), |s| s.merge_partial(&update).unwrap())
            .raw()
            .clone();
        for field in [
            n(2, sequence),
            t(6, original.thread_id()),
            t(7, original.document_id()),
        ] {
            raw = raw
                .merge_partial(&RawMessage::decode(field.raw()).unwrap())
                .unwrap();
        }
        if let Some(position) = raw
            .string(8)
            .unwrap()
            .and_then(|s| s.strip_suffix(":temp"))
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
            .replace_fields(4, &[n(4, now + 60_000_000)])
            .unwrap();
        sections.push(Section::from_raw(raw.replace_fields(38, &[m(38, &wire)]).unwrap()).unwrap());
    }
    let data = RawMessage::from_fields(&sections.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>())
        .unwrap();
    let ack = EditAck::decode(
        RawMessage::from_fields(&[n(1, 1), m(2, &data)])
            .unwrap()
            .raw(),
    )
    .unwrap();
    let accepted = prepared.validate_ack(&ack).unwrap();
    let mut snapshot = original.clone();
    snapshot
        .sections
        .retain(|s| !sections.iter().any(|n| n.id() == s.id()));
    snapshot.sections.extend(
        sections
            .into_iter()
            .filter(|s| s.deleted().unwrap() != Some(true)),
    );
    assert!(accepted.confirms_readback(&snapshot).unwrap());
    (accepted, Arc::new(snapshot))
}
pub(super) fn accept(shell: &mut ShellState) -> Option<checklist::Attempt> {
    if !enabled(shell) {
        return None;
    }
    let fixture = shell.canvas_checklist.structure_fixture.as_mut()?;
    let (attempt, prepared) = fixture.pending_replacement.take()?;
    let (accepted, snapshot) = accepted(
        prepared,
        fixture.canonical.as_ref()?,
        checklist::save::now_usec()?,
    );
    fixture.canonical = Some(snapshot);
    checklist::replacement_ops::finish_replacement(shell, &attempt, accepted).then_some(attempt)
}
pub(super) fn uncertain(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let Some((attempt, _)) = shell
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .pending_replacement
        .take()
    else {
        return false;
    };
    checklist::finish(shell, &attempt, checklist::Outcome::Uncertain)
}
