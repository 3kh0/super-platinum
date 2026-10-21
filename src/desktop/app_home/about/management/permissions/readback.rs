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
            embedded: false,
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
        (
            a.original.id,
            original,
            a.run_group.is_some() || a.manager_group.is_some(),
        )
    };
    dioxus::core::spawn_forever(async move {
        let (id, original, has_run) = captured;
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
        let current = native::current_for(&original, has_run).await;
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
                        let has_run = run::attempt(&s, id)
                            .is_some_and(|a| a.run_group.is_some() || a.manager_group.is_some());
                        let d = document_mut(&mut s, visit).unwrap();
                        d.title = b.title.clone();
                        d.description = b.description.clone();
                        d.notice=Some(if has_run {"Current workflow could not be verified. Your draft and permissions responses are retained; no request will be replayed."}else{"Current workflow could not be verified. Your draft and copy-access responses are retained; no request will be replayed."}.into());
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
    if let Some(group) = &a.manager_group {
        return group.plan.observation(
            current,
            a.copy_changed.then_some(a.desired.as_str()),
            a.run_group.as_ref().map(|g| g.plan.slots()[0].plan()),
            a.run_group.as_ref().and_then(groups::Run::restriction),
        ) == Observation::Matches;
    }
    if a.run_group.is_none() {
        return a
            .original
            .editor
            .copy_permission_observation(current, &a.desired)
            == Observation::Matches;
    }
    a.original.editor.permissions_observation(
        current,
        a.copy_changed.then_some(a.desired.as_str()),
        a.run_group.as_ref().map(|g| g.plan.slots()[0].plan()),
        a.run_group.as_ref().and_then(groups::Run::restriction),
    ) == Observation::Matches
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
    let has_run = a.run_group.is_some() || a.manager_group.is_some();
    let d = document_mut(s, visit).unwrap();
    // Preserve actual original native fields, including an
    // ahead buffer; it never authorizes successful adoption.
    d.title = b.title.clone();
    d.description = b.description.clone();
    if resolve {
        d.editor = editor;
        d.pending = None;
        d.observed = None;
        d.notice = Some(
            if has_run {
                "Permissions are verified. Your local draft is retained."
            } else {
                "Copy access is verified. Your local draft is retained."
            }
            .into(),
        );
    } else {
        d.observed = Some(editor);
        d.notice=Some("Current permissions were read. The operation remains unconfirmed or differs from the original settings; no request will be replayed. Your draft is retained.".into());
    }
    d.revision = next();
}
