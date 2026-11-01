use super::*;
const NEW: &str = "1719803000.000100";

fn arrival(ts: &str, body: &str) -> super_platinum_core::slack::events::RtEvent {
    parse_event(&json!({"type":"message","subtype":"bot_message","channel":"D_COLD_CACHED","user":"U2","bot_id":"B1","bot_profile":{"id":"B1","user_id":"U2","name":"Build helper"},"ts":ts,"text":body}).to_string()).unwrap()
}

#[tokio::test]
async fn actual_bot_arrival_updates_rendered_dm_and_survives_edit_refresh_without_reviving_deleted_arrival()
 {
    let (mut dom, mut state, captured) = mount("dms-cold-rows", render);
    let owner = Owner::capture(&state.peek()).unwrap();
    assert_eq!(
        load_with(state, owner.clone(), false, None, |_| async { Ok(page()) }).await,
        Outcome::Published
    );
    dom.render_immediate_to_vec();
    let initial = text(captured.borrow().as_ref().unwrap());
    assert!(initial.find("Other DM preview").unwrap() < initial.find("Older DM preview").unwrap());
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
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        arrival(NEW, "Current build bot result"),
        false,
    );
    dom.render_immediate_to_vec();
    let live = text(captured.borrow().as_ref().unwrap());
    assert!(
        live.find("Current build bot result").unwrap() < live.find("Other DM preview").unwrap()
    );
    assert!(!live.contains("Older DM preview"));
    let change = parse_event(&json!({"type":"message","subtype":"message_changed","channel":"D_COLD_CACHED","message":{"ts":NEW,"text":"Reviewed bot result","blocks":[],"attachments":[]}}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, change, false);
    release.send(()).unwrap();
    assert_eq!(work.await, Outcome::Published);
    dom.render_immediate_to_vec();
    let final_text = text(captured.borrow().as_ref().unwrap());
    assert!(
        final_text.find("Reviewed bot result").unwrap()
            < final_text.find("Other DM preview").unwrap()
    );
    assert!(!final_text.contains("Older DM preview"));
    {
        let s = state.peek();
        assert_eq!(s.core.dms.entries.len(), 2);
        assert_eq!(s.core.dms.entries[0].latest.as_deref(), Some(NEW));
        assert_eq!(
            s.core.dms.entries[0]
                .message
                .as_ref()
                .unwrap()
                .bot_id
                .as_deref(),
            Some("B1")
        );
        assert_eq!(s.core.dms.next_cursor.as_deref(), Some("keep-older"));
    }
    let deleted = "1719804000.000100";
    let event = parse_event(&json!({"type":"message","subtype":"message_deleted","channel":"D_COLD_CACHED","deleted_ts":deleted}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        arrival(deleted, "Deleted bot arrival"),
        false,
    );
    dom.render_immediate_to_vec();
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("Deleted bot arrival"));
    let s = state.peek();
    assert_eq!(s.core.dms.entries[0].latest.as_deref(), Some(NEW));
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
