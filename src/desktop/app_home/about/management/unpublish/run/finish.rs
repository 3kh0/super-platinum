use super::*;
pub(super) async fn finish(
    mut state: Signal<ShellState>,
    id: u64,
    original: &Original,
    notice: &str,
) {
    let buffers = native::finish_snapshot(original).await;
    let mut s = state.write();
    let source_current = exact(&s, id);
    let same_document = document(&s, original.visit).is_some_and(|d| d.id == original.document);
    let buffers = source_current.then_some(buffers.as_ref()).flatten();
    finish_owned(
        &mut s,
        id,
        notice,
        !same_document || buffers.is_some(),
        buffers,
    );
}
pub(in crate::app_home::about::management::unpublish) fn finish_owned(
    s: &mut ShellState,
    id: u64,
    notice: &str,
    update: bool,
    buffers: Option<&native::Finish>,
) {
    let Some(a) = attempt_mut(s, id) else { return };
    if a.finished {
        return;
    }
    a.finished = true;
    a.complete = a.plan.primary_succeeded()
        && a.followup
            .as_ref()
            .is_some_and(protocol::FollowUpPlan::fully_succeeded);
    a.no_send = a
        .plan
        .slots()
        .iter()
        .all(|s| matches!(s.outcome(), Outcome::Pending | Outcome::NotSent));
    let (doc, complete, no_send, lease, release) = (
        a.original.document,
        a.complete,
        a.no_send,
        a.lease.clone(),
        a.release.take(),
    );
    if let Some(d) = s
        .app_home
        .about
        .management
        .documents
        .iter_mut()
        .find(|d| d.id == doc && d.pending == Some(id) && update)
    {
        if let Some(b) = buffers {
            d.title = b.title.clone();
            d.description = b.description.clone();
        }
        d.notice=Some(if complete {"Unpublication and trigger synchronization acknowledged. Check the current workflow to verify its state; your local draft is retained."}else{notice}.into());
        d.revision = next();
        if no_send {
            d.pending = None;
        }
    }
    lease.finish(s, None);
    if let Some(release) = release {
        release.release();
    }
}
