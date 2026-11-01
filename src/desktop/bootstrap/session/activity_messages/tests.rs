use super::*;
use crate::{
    bootstrap::test_support::pending,
    resource_reads::render_test::{mount, text},
    state::MainView,
};
use serde_json::json;
use super_platinum_core::slack::{models::MessagesListPage, realtime::parse_event};
const TS: &str = "1719801000.000100";
fn render(state: Signal<ShellState>) -> dioxus::prelude::Element {
    crate::view::activity_list_for_test(state, &state.read())
}
fn page() -> MessagesListPage {
    serde_json::from_value(json!({"messages_data":{"C1":{"messages":[{"ts":TS,"text":"Old fallback","blocks":[{"type":"section","text":{"type":"plain_text","text":"Old hydrated rich"}}],"attachments":[{"text":"Old attachment"}],"reactions":[{"name":"eyes","count":1,"users":["U1"]}]}]}}})).unwrap()
}
fn event(text: &str, blocks: serde_json::Value) -> super_platinum_core::slack::events::RtEvent {
    parse_event(&json!({"type":"message","subtype":"message_changed","channel":"C1","message":{"ts":TS,"text":text,"blocks":blocks,"attachments":[]}}).to_string()).unwrap()
}

#[tokio::test]
async fn actual_activity_card_keeps_original_edit_over_held_hydration_and_never_revives_stale_index_fallback()
 {
    let (mut dom, mut state, captured) = mount("activity", render);
    {
        let mut s = state.write();
        s.main_view = MainView::Activity;
        s.overlay = None;
        let epoch = s.core.account_epoch;
        s.core.activity.ensure_scope(epoch, "T1");
        s.core.activity.items=serde_json::from_value(json!([{"key":"mention-edit","feed_ts":TS,"is_unread":true,"item":{"type":"at_user","message":{"channel":"C1","ts":TS,"author_user_id":"U2","text":"Stale embedded body"}}}])).unwrap();
    }
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Stale embedded body"));
    let (transport, session, generation, before) = {
        let s = state.peek();
        (
            s.core.transport.clone().unwrap(),
            s.core.session.as_ref().unwrap().workspaces["T1"].clone(),
            s.core.workspaces["T1"].rt_generation,
            (
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.composer.text.clone(),
                s.core.activity.items[0].is_unread,
            ),
        )
    };
    let (release, held) = tokio::sync::oneshot::channel();
    let mut held = Some(held);
    let mut work = Box::pin(hydrate_with(state, &transport, &session, move |ids| {
        assert_eq!(ids, vec![("C1".into(), vec![TS.into()])]);
        let held = held.take().unwrap();
        async move {
            held.await.unwrap();
            Ok(page())
        }
    }));
    pending(work.as_mut()).await;
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        event("Edited Activity preview", json!([])),
        false,
    );
    release.send(()).unwrap();
    work.await;
    dom.render_immediate_to_vec();
    let edited = text(captured.borrow().as_ref().unwrap());
    assert!(edited.contains("Edited Activity preview"));
    assert!(!edited.contains("Old hydrated rich") && !edited.contains("Stale embedded body"));
    assert_eq!(
        state.peek().core.activity.hydrated[&("C1".into(), TS.into())].reactions[0].name,
        "eyes"
    );
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        event("", json!([{"type":"divider"}])),
        false,
    );
    dom.render_immediate_to_vec();
    let empty = text(captured.borrow().as_ref().unwrap());
    assert!(!empty.contains("Edited Activity preview") && !empty.contains("Stale embedded body"));
    let s = state.peek();
    assert_eq!(
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.core.activity.items[0].is_unread
        ),
        before
    );
}

mod deleted;

#[path = "tests/styled.rs"]
mod styled;
