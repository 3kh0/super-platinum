use super::*;
/// Source changes only browse data. No idle mutation slot is reserved.
pub(crate) fn set_mode(mut state: Signal<ShellState>, id: u64, frame: u64, workspace: bool) {
    let work = {
        let Ok(mut s) = state.try_write() else {
            return;
        };
        let Some((p, prepared)) = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_ref()
            .filter(|v| v.owner.id == id && v.frame == frame && !v.loading)
            .and_then(|v| Some((v.owner.clone(), v.ready.clone()?)))
        else {
            return;
        };
        if !parent_current(&p, &s, &prepared) {
            return;
        }
        let session = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_mut()
            .unwrap();
        if session.custom.mode == workspace || !picker::bump(session) {
            return;
        }
        session.custom.mode = workspace;
        if !workspace && session.custom.loading {
            // Retire the exact loading visit, including leave/reenter with the
            // same query. A future visit cannot consume its late response.
            session.custom.serial = session.custom.serial.saturating_add(1);
            session.custom.loading = false;
        }
        session.cursor = 0;
        session.limit = 120;
        session.matches.clear();
        picker::QueryWork::for_session(session)
    };
    if let Some(work) = work {
        let matches = work.compute();
        picker::publish_query(state, work, matches);
    }
    if workspace {
        start(state, id);
    }
}
pub(crate) fn retry(state: Signal<ShellState>, id: u64, frame: u64) {
    let allowed = state.try_peek().is_ok_and(|s| {
        s.thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_ref()
            .is_some_and(|v| {
                v.owner.id == id
                    && v.frame == frame
                    && v.custom.mode
                    && !v.custom.loading
                    && v.custom.ready.is_none()
            })
    });
    if allowed {
        start(state, id);
    }
}
fn start(mut state: Signal<ShellState>, id: u64) {
    let work = {
        let Ok(mut s) = state.try_write() else {
            return;
        };
        let Some((p, prepared, existing)) = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_ref()
            .filter(|v| v.owner.id == id && v.custom.mode && !v.custom.loading && !v.loading)
            .and_then(|v| Some((v.owner.clone(), v.ready.clone()?, v.custom.ready.clone())))
        else {
            return;
        };
        if !parent_ready(&p, &s, &prepared) {
            return;
        }
        if existing.as_ref().is_some_and(|r| r.current(&s)) {
            return;
        }
        let Some(attempt) = read::Attempt::capture(&s, p, prepared) else {
            s.thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_mut()
                .unwrap()
                .custom
                .notice = "Workspace emoji could not load yet. Please retry.".into();
            return;
        };
        let session = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_mut()
            .unwrap();
        let Some(serial) = session.custom.serial.checked_add(1) else {
            return;
        };
        session.custom.serial = serial;
        session.custom.loading = true;
        session.custom.ready = None;
        session.custom.notice = "Loading workspace emoji…".into();
        (attempt, serial)
    };
    dioxus::core::spawn_forever(read::load(state, work.0, work.1));
}
