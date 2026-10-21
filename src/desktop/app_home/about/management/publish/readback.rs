use super::*;
pub(in crate::app_home::about::management) fn check(mut state: Signal<ShellState>, visit: u64) {
    let captured = {
        let s = state.peek();
        if !admitted(&s, visit)
            || delete::confirming(&s, visit)
            || unpublish::confirming(&s, visit)
            || permissions::confirming(&s, visit)
        {
            return;
        }
        let Some(d) = document(&s, visit) else { return };
        let Some(id) = d.pending else { return };
        let Some(a) =
            attempt(&s, id).filter(|a| a.finished && !a.resolved && a.original.document == d.id)
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
        (
            a.original.id,
            Original {
                id: next(),
                visit,
                document: d.id,
                revision: d.revision,
                title: d.title.clone(),
                description: d.description.clone(),
                editor: d.editor.clone(),
                policy,
                owner: h.owner.clone(),
            },
        )
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
            let query = validation::Query {
                original: &original,
                lease: &lease,
                pending: id,
            };
            let policy = policy::acquire(state, &query).await?;
            let editor = query.editor(state).await?;
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
        if lease.current(&s) && original.current(&s, Some(id)) {
            if let Some(b) = buffers {
                let same =
                    current && b.title == original.title && b.description == original.description;
                match result {
                    Ok(editor) => adopt(&mut s, visit, id, editor, &b, same),
                    Err(_) => {
                        let d = document_mut(&mut s, visit).unwrap();
                        d.title = b.title;
                        d.description = b.description;
                        d.notice=Some("Current publication could not be verified. Each response and your text are retained; no operation will be replayed.".into());
                        d.revision = next();
                    }
                }
            }
        }
        lease.finish(&mut s, None);
        release.release();
    });
}
fn adopt(
    s: &mut ShellState,
    visit: u64,
    id: u64,
    editor: Editor,
    b: &native::Finish,
    current: bool,
) {
    let a = attempt_mut(s, id).unwrap();
    let matched = a
        .followup
        .as_ref()
        .is_some_and(|p| p.publication_observation(&editor) == Observation::Matches);
    let resolve = a.complete && matched && current;
    a.readback = Some((editor.clone(), matched));
    a.resolved = resolve;
    let d = document_mut(s, visit).unwrap();
    d.title = b.title.clone();
    d.description = b.description.clone();
    if resolve {
        d.editor = editor;
        d.pending = None;
        d.observed = None;
        d.notice = Some("The published version and trigger synchronization are verified.".into());
    } else {
        d.observed = Some(editor);
        d.notice=Some("Current workflow does not confirm a complete publication. Each response and your text are retained; no operation will be replayed.".into());
    }
    d.revision = next();
}
