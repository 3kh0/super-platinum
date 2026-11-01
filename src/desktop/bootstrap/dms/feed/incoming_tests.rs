use super::*;
use crate::{
    bootstrap::test_support::pending,
    resource_reads::render_test::{mount, text},
};
use serde_json::json;
use super_platinum_core::slack::{models::DmEntry, realtime::parse_event};

fn render(state: Signal<ShellState>) -> Element {
    crate::view::dm_list_for_test(state, &state.read())
}
fn entries() -> Vec<DmEntry> {
    serde_json::from_value(json!([
        {"id":"D_COLD_CACHED","latest":"1719801000.000100","channel":{"id":"D_COLD_CACHED","is_im":true,"is_open":false,"user":"U2"},"message":{"ts":"1719801000.000100","text":"Older DM preview"}},
        {"id":"D_OTHER","latest":"1719802000.000100","channel":{"id":"D_OTHER","is_im":true,"is_open":false,"user":"U3"},"message":{"ts":"1719802000.000100","text":"Other DM preview"}}
    ])).unwrap()
}
fn page() -> serde_json::Value {
    json!({"ok":true,"dms":entries(),"response_metadata":{"next_cursor":"keep-older"}})
}

#[tokio::test]
async fn ordinary_native_arrival_moves_actual_existing_dm_row_and_survives_held_refresh_with_following_edit()
 {
    let (mut dom, mut state, captured) = mount("dms-cold-rows", render);
    let owner = Owner::capture(&state.peek()).unwrap();
    assert_eq!(
        load_with(state, owner.clone(), false, None, |_| async { Ok(page()) }).await,
        Outcome::Published
    );
    dom.render_immediate_to_vec();
    let first = text(captured.borrow().as_ref().unwrap());
    assert!(first.find("Other DM preview").unwrap() < first.find("Older DM preview").unwrap());
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
    let mut work = Box::pin(load_with(state, owner, false, None, |request| async move {
        assert!(request.url.contains("client.dms"));
        held.await.unwrap();
        Ok(page())
    }));
    pending(work.as_mut()).await;
    let event=parse_event(&json!({"type":"message","channel":"D_COLD_CACHED","user":"U2","ts":"1719803000.000100","text":"Newest ordinary DM"}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    dom.render_immediate_to_vec();
    let live = text(captured.borrow().as_ref().unwrap());
    assert!(live.find("Newest ordinary DM").unwrap() < live.find("Other DM preview").unwrap());
    assert!(!live.contains("Older DM preview"));
    let event=parse_event(&json!({"type":"message","subtype":"message_changed","channel":"D_COLD_CACHED","message":{"ts":"1719803000.000100","text":"Edited newest ordinary DM","blocks":[],"attachments":[]}}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    release.send(()).unwrap();
    assert_eq!(work.await, Outcome::Published);
    dom.render_immediate_to_vec();
    let final_text = text(captured.borrow().as_ref().unwrap());
    assert!(
        final_text.find("Edited newest ordinary DM").unwrap()
            < final_text.find("Other DM preview").unwrap()
    );
    assert!(!final_text.contains("Older DM preview"));
    let s = state.peek();
    assert_eq!(s.core.dms.entries.len(), 2);
    assert_eq!(s.core.dms.entries[0].id, "D_COLD_CACHED");
    assert_eq!(
        s.core.dms.entries[0].latest.as_deref(),
        Some("1719803000.000100")
    );
    assert_eq!(s.core.dms.next_cursor.as_deref(), Some("keep-older"));
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

mod bots;

mod deleted;
