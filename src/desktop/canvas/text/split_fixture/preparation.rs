//! Pure preparation/evidence workers; only the caller may publish their result.
use super::*;
use super_platinum_core::canvas::edit::{
    Error,
    structure::{SplitWitness, recovery::Checkpoint},
};
type Prepared = Result<(PreparedSplitEdit, Checkpoint), Error>;

pub(super) fn current(
    shell: &ShellState,
    attempt: &checklist::Attempt,
    fresh: &Arc<CanvasCollabSnapshot>,
) -> bool {
    checklist::recovery::split_preparation_current(shell, attempt)
        && shell
            .canvas_checklist
            .split_fixture
            .as_ref()
            .and_then(|fixture| fixture.canonical.as_ref())
            .is_some_and(|canonical| Arc::ptr_eq(canonical, fresh))
}

pub(super) async fn prepare(
    intent: SplitIntent,
    fresh: Arc<CanvasCollabSnapshot>,
    source: Arc<CanvasCollabSnapshot>,
    witness: SplitWitness,
    now: u64,
    second_middle: bool,
) -> Result<(Prepared, Value, Option<Value>), &'static str> {
    let prepared = super::super::split_save::prepare(intent, fresh.clone(), witness, now).await;
    // Evidence serialization also stays off the UI task. Move the exact prepared
    // pair through this worker; no second CRDT construction or checkpoint check.
    tokio::task::spawn_blocking(move || {
        let preflight = json!({"source":inventory(&source),"fresh":inventory(&fresh),
            "outcome":match &prepared {Ok(_)=>"prepared".to_owned(),Err(error)=>format!("{error:?}")}});
        let write = prepared.as_ref().ok().map(|(prepared, _)| {
            json!({"kind":if second_middle{"second-middle"}else{"split"},
                "secondChild":prepared.second_child_section(),
                "members":if second_middle { prepared.member_ids().iter().zip(prepared.member_drafts()).skip(1)
                    .map(|(id,d)|json!({"id":id,"finalRtml":d.rtml().unwrap()})).collect::<Vec<_>>() } else { Vec::new() },
                "visualOrder":prepared.visual_order(),"documentData":STANDARD.encode(prepared.document_data().raw()),
                "formTitle":prepared.form_title(),"session":prepared.session_id(),"sequence":prepared.sequence(),
                "parent":prepared.target_section(),"child":prepared.child_section(),"httpFactoryExecuted":false})
        });
        (prepared, preflight, write)
    }).await.map_err(|_| "The disconnected split evidence could not be prepared.")
}

#[cfg(test)]
mod tests;
