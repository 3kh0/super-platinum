use super::*;

#[tokio::test]
async fn actual_deleted_dm_summary_stays_deleted_across_old_refresh_then_recovers_on_new_arrival() {
    let (mut dom, mut state, captured) = mount("dms-cold-rows", render);
    let owner = Owner::capture(&state.peek()).unwrap();
    assert_eq!(
        load_with(state, owner.clone(), false, None, |_| async { Ok(page()) }).await,
        Outcome::Published
    );
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Older DM preview"));
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.core.workspaces["T1"]
                .messages
                .get("D_COLD_CACHED")
                .and_then(|b| b.last_read.clone()),
        )
    };
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(load_with(state, owner, false, None, |_| async move {
        held.await.unwrap();
        Ok(page())
    }));
    pending(work.as_mut()).await;
    let event = parse_event(&json!({"type":"message","subtype":"message_deleted","channel":"D_COLD_CACHED","deleted_ts":"1719801000.000100"}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    dom.render_immediate_to_vec();
    let deleted = text(captured.borrow().as_ref().unwrap());
    assert!(deleted.contains("This message was deleted."));
    assert!(!deleted.contains("Older DM preview"));
    assert!(deleted.contains("Other DM preview"));
    release.send(()).unwrap();
    assert_eq!(work.await, Outcome::Published);
    dom.render_immediate_to_vec();
    let late = text(captured.borrow().as_ref().unwrap());
    assert!(late.contains("This message was deleted.") && !late.contains("Older DM preview"));
    {
        let s = state.peek();
        assert_eq!(s.core.dms.entries.len(), 2);
        assert_eq!(s.core.dms.next_cursor.as_deref(), Some("keep-older"));
    }
    let event = parse_event(&json!({"type":"message","channel":"D_COLD_CACHED","user":"U2","ts":"1719803000.000100","text":"New message after deleted summary"}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    dom.render_immediate_to_vec();
    let fresh = text(captured.borrow().as_ref().unwrap());
    assert!(fresh.contains("New message after deleted summary"));
    assert!(!fresh.contains("This message was deleted.") && !fresh.contains("Older DM preview"));
    let s = state.peek();
    assert_eq!(
        s.core.dms.entries[0].latest.as_deref(),
        Some("1719803000.000100")
    );
    assert_eq!(
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.core.workspaces["T1"]
                .messages
                .get("D_COLD_CACHED")
                .and_then(|b| b.last_read.clone())
        ),
        before
    );
}
