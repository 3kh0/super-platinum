//! Readback is a new owned read, never a replacement mutation receipt.
use super::*;
pub(in crate::app_home::about::management) fn check(mut state: Signal<ShellState>, visit: u64) {
    let captured = {
        let s = state.peek();
        if !admitted(&s, visit) || confirming(&s, visit) || delete::confirming(&s, visit) {
            return;
        }
        let Some(d) = document(&s, visit) else { return };
        let Some(id) = d.pending else { return };
        let Some(a) = run::attempt(&s, id)
            .filter(|a| a.finished && !a.resolved && a.original.document == d.id)
        else {
            return;
        };
        let h = s.app_home.about.management.surface.as_ref().unwrap();
        if h.read.is_some() || d.activity.active {
            return;
        }
        let Some(policy) = h.policy.clone().filter(|p| p.current(&s) && p.can_create()) else {
            return;
        };
        let original = Original {
            id: next(),
            visit,
            document: d.id,
            revision: d.revision,
            title: d.title.clone(),
            description: d.description.clone(),
            editor: d.editor.clone(),
            policy,
            owner: h.owner.clone(),
        };
        (a.original.id, original)
    };
    dioxus::core::spawn_forever(async move {
        let (id, original) = captured;
        let Some(release) = native::lock(&original).await else {
            return;
        };
        let lease = {
            let mut s = state.write();
            if !original.current(&s, Some(id)) {
                release.release();
                return;
            }
            let Some(lease) = read::Lease::begin(&mut s, visit) else {
                release.release();
                return;
            };
            lease
        };
        let result = async {
            let policy = policy::acquire(state, &lease).await?;
            let editor = read::load_editor(state, &lease, original.editor.workflow_id()).await?;
            if !policy.current(&state.peek())
                || !policy.same(&original.policy)
                || !policy.can_create()
                || !allowed_editor(&editor, &lease.owner.ctx.session.user_id)
            {
                return Err(unavailable());
            }
            Ok(editor)
        }
        .await;
        let current = native::current(&original).await;
        let buffers = native::finish_snapshot(&original).await;
        let mut s = state.write();
        let owned = lease.current(&s) && original.current(&s, Some(id));
        if owned {
            if let Some(b) = buffers.as_ref() {
                let same = b.title == original.title && b.description == original.description;
                match result {
                    Ok(editor) => {
                        adopt(&mut s, visit, id, editor, b, current && same);
                    }
                    Err(_) => {
                        let d = document_mut(&mut s, visit).unwrap();
                        d.title = b.title.clone();
                        d.description = b.description.clone();
                        d.notice=Some("Current workflow could not be verified. Your draft and unpublication responses are retained; no request will be replayed.".into());
                        d.revision = next();
                    }
                }
            }
        }
        lease.finish(&mut s, None);
        release.release();
    });
}
pub(super) fn matches(a: &Attempt, current: &Editor) -> bool {
    a.followup
        .as_ref()
        .is_some_and(|p| p.publication_observation(current) == Observation::Matches)
}

pub(super) fn adopt(
    s: &mut ShellState,
    visit: u64,
    id: u64,
    editor: Editor,
    b: &native::Finish,
    native_current: bool,
) {
    let matched = run::attempt(s, id).is_some_and(|a| matches(a, &editor));
    let a = run::attempt_mut(s, id).unwrap();
    let resolve = a.complete && matched && native_current;
    a.readback = Some((editor.clone(), matched));
    a.resolved = resolve;
    let d = document_mut(s, visit).unwrap();
    // Preserve actual original native fields, including an
    // ahead buffer; it never authorizes successful adoption.
    d.title = b.title.clone();
    d.description = b.description.clone();
    if resolve {
        d.editor = editor;
        d.pending = None;
        d.observed = None;
        d.notice=Some("Workflow is unpublished and trigger synchronization is verified. Your local draft is retained.".into());
    } else {
        let unpublished = editor.raw_workflow()["is_published"] == false;
        d.observed = Some(editor);
        d.notice=Some(if unpublished {"Current workflow is unpublished. The operation remains unconfirmed or incomplete; no request will be replayed. Your draft is retained."}else{"Current workflow does not confirm unpublication. Each response and your draft are retained; no request will be replayed."}.into());
    }
    d.revision = next();
}
