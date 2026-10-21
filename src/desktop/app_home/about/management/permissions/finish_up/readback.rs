use super::*;
pub(in crate::app_home::about::management) fn check(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
) {
    let captured = {
        let s = state.peek();
        attempt(&s, id)
            .filter(|a| {
                a.finished
                    && a.original.visit == visit
                    && a.dialog_original
                        .as_ref()
                        .unwrap_or(&a.original)
                        .current(&s, Some(id))
            })
            .map(|a| {
                (
                    a.dialog_original.as_ref().unwrap_or(&a.original).clone(),
                    a.local.expected(),
                )
            })
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !native::current(&o, &e, None).await {
            return;
        }
        let lease = {
            let mut s = state.write();
            if !o.current(&s, Some(id)) {
                return;
            }
            let Some(lease) = read::Lease::begin(&mut s, visit) else {
                return;
            };
            lease
        };
        let query = validation::Query {
            original: &o,
            lease: &lease,
            pending: id,
        };
        let result = async {
            let p = policy::acquire(state, &query).await?;
            let editor = query.editor(state).await?;
            if !p.current(&state.peek()) || !p.same(&o.policy) {
                return Err(unavailable());
            }
            Ok(editor)
        }
        .await;
        let native_current = native::current(&o, &e, None).await;
        let mut s = state.write();
        if lease.current(&s) && o.current(&s, Some(id)) && native_current {
            let a = attempt_mut(&mut s, id).unwrap();
            match result{
                Ok(editor)=>{let matched=a.followup.as_ref().is_some_and(|f|f.publication_observation_first_publish(&editor)==Observation::Matches);
                    a.resolved=a.complete&&matched;a.readback=Some((editor,matched));
                    a.local.notice=Some(if a.resolved{"The published workflow and trigger synchronization are verified."}else{"Current workflow does not confirm a complete first publication. Original responses remain unchanged; nothing will be replayed."}.into());},
                Err(_)=>a.local.notice=Some("Current publication could not be verified. Check again without replaying any operation.".into()),
            }
        }
        lease.finish(&mut s, None);
    });
}
pub(in crate::app_home::about::management) fn close(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
) {
    let captured = {
        let s = state.peek();
        attempt(&s, id)
            .filter(|a| {
                a.finished
                    && a.original.visit == visit
                    && a.dialog_original
                        .as_ref()
                        .unwrap_or(&a.original)
                        .current(&s, Some(id))
            })
            .map(|a| {
                (
                    a.dialog_original.as_ref().unwrap_or(&a.original).clone(),
                    a.local.expected(),
                )
            })
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !native::current(&o, &e, None).await {
            return;
        }
        let mut s = state.write();
        if !o.current(&s, Some(id)) {
            return;
        }
        close_current(&mut s, visit, id, None);
    });
}

// Caller has proved the exact original native buffers and current document.
pub(super) fn close_current(s: &mut ShellState, visit: u64, id: u64, notice: Option<&str>) {
    let a = attempt_mut(s, id).unwrap();
    a.closed = true;
    let release = a.release.take();
    let editor = a
        .readback
        .as_ref()
        .filter(|_| a.resolved)
        .map(|(e, _)| e.clone());
    let pending_or_not_sent =
        |s: &protocol::Slot| matches!(s.outcome(), Outcome::Pending | Outcome::NotSent);
    let no_send = a.publication.slots().iter().all(pending_or_not_sent)
        && a.metadata
            .iter()
            .flat_map(|p| p.slots())
            .all(pending_or_not_sent)
        && a.copy
            .iter()
            .flat_map(|p| p.slots())
            .all(pending_or_not_sent)
        && a.run
            .iter()
            .flat_map(|g| g.plan.slots())
            .all(pending_or_not_sent)
        && a.manager
            .iter()
            .flat_map(|g| g.plan.slots())
            .all(pending_or_not_sent);
    let d = document_mut(s, visit).unwrap();
    if let Some(editor) = editor {
        d.editor = editor;
        d.pending = None;
        d.observed = None;
        d.notice =
            Some("First publication is verified. Your original metadata draft is retained.".into());
    } else if no_send {
        d.pending = None;
        d.notice = Some("Publication was not sent. Your original draft is retained.".into());
    } else {
        d.notice =
            Some("Publication responses remain unconfirmed. No operation will be replayed.".into());
    }
    d.revision = next();
    if let Some(release) = release {
        release.release();
    }
    if let Some(notice) = notice {
        if let Some(d) = document_mut(s, visit) {
            d.notice = Some(notice.into());
        }
    }
}

pub(in crate::app_home::about::management) fn review_available(s: &ShellState, visit: u64) -> bool {
    document(s, visit)
        .and_then(|d| d.pending)
        .and_then(|id| attempt(s, id))
        .is_some_and(|a| a.closed && a.finished && a.original.owner.current(s))
}
pub(in crate::app_home::about::management) fn resume(mut state: Signal<ShellState>, visit: u64) {
    let captured = {
        let s = state.peek();
        let Some(d) = document(&s, visit) else { return };
        let Some(a) = d
            .pending
            .and_then(|id| attempt(&s, id))
            .filter(|a| a.closed && a.finished && a.original.owner.current(&s))
        else {
            return;
        };
        let mut o = a.original.clone();
        o.revision = d.revision;
        o.title = d.title.clone();
        o.description = d.description.clone();
        if !o.current(&s, Some(o.id)) {
            return;
        }
        o
    };
    dioxus::core::spawn_forever(async move {
        let Some(release) = super::super::native::lock(&captured).await else {
            return;
        };
        let mut s = state.write();
        if !captured.current(&s, Some(captured.id)) {
            release.release();
            return;
        }
        let Some(a) = attempt_mut(&mut s, captured.id) else {
            release.release();
            return;
        };
        a.dialog_original = Some(captured);
        a.closed = false;
        a.release = Some(release);
    });
}
