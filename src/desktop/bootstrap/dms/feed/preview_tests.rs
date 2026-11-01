//! Actual DM row renderer + owned held page; no native/live acceptance claim.
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
fn rich() -> DmEntry {
    serde_json::from_value(json!({"id":"D_COLD_CACHED","latest":"1719801000.000400","channel":{"id":"D_COLD_CACHED","is_im":true,"is_open":false,"user":"U2"},"message":{"ts":"1719801000.000400","text":"Old fallback","blocks":[{"type":"section","text":{"type":"plain_text","text":"Original rich DM preview"}}],"attachments":[{"text":"Original attachment"}],"reactions":[{"name":"eyes","count":1,"users":["U1"]}]}})).unwrap()
}
fn page(entry: DmEntry) -> serde_json::Value {
    json!({"ok":true,"dms":[entry],"response_metadata":{"next_cursor":"same-older"}})
}
fn edit(body: serde_json::Value) -> super_platinum_core::slack::events::RtEvent {
    parse_event(&json!({"type":"message","subtype":"message_changed","channel":"D_COLD_CACHED","message":body}).to_string()).unwrap()
}

#[tokio::test]
async fn actual_dm_preview_updates_during_owned_refresh_preserves_late_edit_and_allows_fresh_rich_response()
 {
    let (mut dom, mut state, captured) = mount("dms-cold-rows", render);
    let owner = Owner::capture(&state.peek()).unwrap();
    assert_eq!(
        load_with(state, owner.clone(), false, None, |_| async {
            Ok(page(rich()))
        })
        .await,
        Outcome::Published
    );
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Original rich DM preview"));
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.detail_open(),
            s.core.workspaces["T1"]
                .messages
                .get("D_COLD_CACHED")
                .and_then(|b| b.last_read.clone()),
        )
    };
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(load_with(
        state,
        owner.clone(),
        false,
        None,
        |request| async move {
            assert!(request.url.contains("client.dms"));
            held.await.unwrap();
            Ok(page(rich()))
        },
    ));
    pending(work.as_mut()).await;
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        edit(
            json!({"ts":"1719801000.000400","text":"Edited plain DM preview","blocks":[],"attachments":[]}),
        ),
        false,
    );
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Edited plain DM preview"));
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("Original rich DM preview"));
    release.send(()).unwrap();
    assert_eq!(work.await, Outcome::Published);
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Edited plain DM preview"));
    {
        let s = state.peek();
        let current = s.core.dms.entries[0].message.as_ref().unwrap();
        assert!(current.blocks.is_empty() && current.attachments.is_empty());
        assert_eq!(current.reactions[0].name, "eyes");
        assert_eq!(s.core.dms.entries.len(), 1);
        assert_eq!(
            s.core.dms.entries[0].latest.as_deref(),
            Some("1719801000.000400")
        );
        assert_eq!(s.core.dms.next_cursor.as_deref(), Some("same-older"));
        assert_eq!(
            (
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.composer.text.clone(),
                s.detail_open(),
                s.core.workspaces["T1"]
                    .messages
                    .get("D_COLD_CACHED")
                    .and_then(|b| b.last_read.clone())
            ),
            before
        );
    }
    // A fresh read starts after the clear; its rich response wins even if a
    // count-only edit arrives while it is pending.
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(load_with(state, owner, false, None, |_| async move {
        held.await.unwrap();
        let mut fresh = rich();
        fresh.message.as_mut().unwrap().blocks = vec![
            json!({"type":"section","text":{"type":"plain_text","text":"Fresh rich DM preview"}}),
        ];
        Ok(page(fresh))
    }));
    pending(work.as_mut()).await;
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        edit(json!({"ts":"1719801000.000400","reply_count":6})),
        false,
    );
    release.send(()).unwrap();
    assert_eq!(work.await, Outcome::Published);
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Fresh rich DM preview"));
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("Edited plain DM preview"));
    assert_eq!(
        state.peek().core.dms.entries[0]
            .message
            .as_ref()
            .unwrap()
            .reply_count,
        Some(6)
    );
}

#[path = "preview_tests/styled.rs"]
mod styled;
