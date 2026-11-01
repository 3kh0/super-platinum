use super::*;
use crate::resource_reads::render_test::{button, click, mount};
use super_platinum_core::slack::events::RtEvent;

#[tokio::test(flavor = "current_thread")]
async fn thread_edited_owned_get_view_vnode_and_original_rt_preserve_inert_body_and_merge_contract()
{
    use serde_json::{Value, json};
    let (mut dom, mut state, captured) = mount("thread-feed-rich-root", render);
    state.write().overlay = None;
    let mut response = page(state);
    let channel = response.threads[0].root_msg.channel.clone().unwrap();
    let root = response.threads[0].root_msg.ts.clone().unwrap();
    let seconds: u64 = root.split('.').next().unwrap().parse().unwrap();
    let reply = format!("{}.000001", seconds + 1);
    let edited = format!("{}.000002", seconds + 2);
    let blocks = |text: &str| json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":{"bold":true}}]}]}]);
    let root_message = json!({"type":"message","channel":channel,"team":"T1","ts":root,"user":"U1","text":"root fallback must stay hidden","blocks":blocks("Root Ω\nsecond paragraph"),"edited":{"ts":edited,"user":"U1"}});
    let reply_message = json!({"type":"message","channel":channel,"team":"T1","ts":reply,"thread_ts":root,"user":"U2","text":"reply fallback must stay hidden","blocks":blocks("Reply original"),"edited":{"ts":edited,"user":"U2"}});
    response.threads = vec![serde_json::from_value(json!({"root_msg":root_message,"latest_replies":[reply_message.clone()],"unread_replies":[reply_message]})).unwrap()];
    publish(state, response).await;
    assert!(row(state).current(&state.peek()));
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let pending_media = state.peek().media.has_pending();

    fn painted(
        dom: &mut VirtualDom,
        captured: &crate::resource_reads::render_test::Captured,
    ) -> String {
        dom.render_immediate_to_vec();
        crate::resource_reads::render_test::text(captured.borrow().as_ref().unwrap())
    }
    fn inert_marker_nodes(v: &dioxus::core::VNode, require_bold: bool) -> usize {
        use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
        fn text_at(v: &VNode, node: &TemplateNode, out: &mut String) {
            match node {
                TemplateNode::Text { text } => out.push_str(text),
                TemplateNode::Element { children, .. } => {
                    for child in children.iter() {
                        text_at(v, child, out);
                    }
                }
                TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                    DynamicNode::Text(text) => out.push_str(&text.value),
                    DynamicNode::Fragment(nodes) => {
                        for child in nodes.iter() {
                            for root in child.template.roots.iter() {
                                text_at(child, root, out);
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
        fn at(v: &VNode, node: &TemplateNode, parent: Option<&str>, bold: &mut bool) -> usize {
            match node {
                TemplateNode::Element {
                    tag,
                    attrs,
                    children,
                    ..
                } => {
                    let (mut marker, mut listener) = (false, false);
                    let mut names = Vec::new();
                    for attr in attrs.iter() {
                        match attr {
                            TemplateAttribute::Static { name, value, .. } => {
                                names.push(*name);
                                if *name == "class" {
                                    marker |= value
                                        .split_ascii_whitespace()
                                        .any(|c| c == "message-edited");
                                    *bold |= value.split_ascii_whitespace().any(|c| c == "bold");
                                }
                            }
                            TemplateAttribute::Dynamic { id } => {
                                for attr in v.dynamic_attrs[*id].iter() {
                                    names.push(attr.name);
                                    listener |= matches!(&attr.value, AttributeValue::Listener(_));
                                    if attr.name == "class"
                                        && let AttributeValue::Text(value) = &attr.value
                                    {
                                        marker |= value
                                            .split_ascii_whitespace()
                                            .any(|c| c == "message-edited");
                                        *bold |=
                                            value.split_ascii_whitespace().any(|c| c == "bold");
                                    }
                                }
                            }
                        }
                    }
                    if marker {
                        assert_eq!(*tag, "span");
                        assert_eq!(parent, Some("p"), "cue belongs inside original paragraph");
                        assert!(!listener, "metadata cannot acquire callbacks");
                        assert!(
                            names
                                .into_iter()
                                .all(|name| matches!(name, "class" | "dir")),
                            "no resource/URL/title props"
                        );
                        let mut text = String::new();
                        for child in children.iter() {
                            text_at(v, child, &mut text);
                        }
                        assert_eq!(text, "\u{a0}(edited)\u{a0}");
                    }
                    let mut count = usize::from(marker);
                    for child in children.iter() {
                        count += at(v, child, Some(*tag), bold);
                    }
                    count
                }
                TemplateNode::Dynamic { id } => {
                    let mut count = 0;
                    if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                        for child in nodes.iter() {
                            for root in child.template.roots.iter() {
                                count += at(child, root, parent, bold);
                            }
                        }
                    }
                    count
                }
                _ => 0,
            }
        }
        let mut bold = false;
        let mut count = 0;
        for node in v.template.roots.iter() {
            count += at(v, node, None, &mut bold);
        }
        if require_bold {
            assert!(bold, "original styled body remains styled");
        }
        count
    }
    let cues = |text: &str, expected| {
        assert_eq!(text.matches("\u{a0}(edited)\u{a0}").count(), expected);
        assert_eq!(
            inert_marker_nodes(captured.borrow().as_ref().unwrap(), false),
            expected
        );
    };
    let initial = painted(&mut dom, &captured);
    cues(&initial, 2);
    assert_eq!(
        inert_marker_nodes(captured.borrow().as_ref().unwrap(), true),
        2
    );
    assert!(
        initial.contains("Root Ω")
            && initial.contains("second paragraph")
            && initial.contains("Reply original")
    );
    assert!(!initial.contains("fallback must stay hidden"));

    let update = |mut state: Signal<ShellState>,
                  ts: &str,
                  text: &str,
                  edit: Option<Value>,
                  clear: bool,
                  socket: u64| {
        let mut message = json!({"ts":ts,"channel":channel,"thread_ts":root,"text":format!("{text} fallback"),"blocks":if clear {json!([])} else {blocks(text)}});
        if let Some(edit) = edit {
            message
                .as_object_mut()
                .unwrap()
                .insert("edited".into(), edit);
        }
        let raw = json!({"type":"message","subtype":"message_changed","channel":channel,"message":message});
        let event = super_platinum_core::slack::realtime::parse_event(&raw.to_string()).unwrap();
        assert!(matches!(
            &event,
            RtEvent::MessageChanged {
                original_update: Some(_),
                ..
            }
        ));
        crate::realtime::apply_in_view(&mut state.write().core, "T1", socket, event, false);
        crate::thread_feed::after_native_mutation(&state.peek(), "T1", socket);
    };
    update(
        state,
        &root,
        "stale owner must not land",
        Some(json!({"ts":edited})),
        false,
        generation.wrapping_add(1),
    );
    assert!(!painted(&mut dom, &captured).contains("stale owner must not land"));
    for (ts, text) in [
        (&root, "Root accepted edit"),
        (&reply, "Reply accepted edit"),
    ] {
        update(
            state,
            ts,
            text,
            Some(json!({"ts":edited})),
            false,
            generation,
        );
    }
    let changed = painted(&mut dom, &captured);
    cues(&changed, 2);
    assert!(changed.contains("Root accepted edit") && changed.contains("Reply accepted edit"));
    assert!(!changed.contains("Reply original") && !changed.contains("fallback must stay hidden"));

    for (edit, expected, word) in [
        (None, 2, "omitted"),
        (Some(Value::Null), 2, "null-retained"),
        (Some(json!({"ts":""})), 0, "malformed"),
        (Some(json!(true)), 0, "boolean"),
        (Some(json!({"ts":[]})), 0, "non-string"),
        (Some(json!({"ts":edited})), 2, "restored"),
    ] {
        for (ts, name) in [(&root, "Root"), (&reply, "Reply")] {
            update(
                state,
                ts,
                &format!("{name} {word}"),
                edit.clone(),
                false,
                generation,
            );
        }
        cues(&painted(&mut dom, &captured), expected);
        if word == "null-retained" {
            assert!(
                state.peek().core.threads_view.items[0]
                    .root_msg
                    .edited
                    .is_some(),
                "existing explicit-null merge retention is preserved"
            );
        }
    }
    for (ts, name) in [(&root, "Root cleared"), (&reply, "Reply cleared")] {
        update(state, ts, name, None, true, generation);
    }
    let cleared = painted(&mut dom, &captured);
    cues(&cleared, 0);
    assert!(
        cleared.contains("Root cleared fallback") && cleared.contains("Reply cleared fallback")
    );
    assert!(
        state.peek().core.threads_view.items[0]
            .root_msg
            .blocks
            .is_empty()
    );
    assert_eq!(
        state.peek().media.has_pending(),
        pending_media,
        "inert metadata adds no shell media work"
    );
    assert!(captured.borrow().is_some());
    state.write().core.account_epoch += 1;
    let retired = painted(&mut dom, &captured);
    cues(&retired, 0);
    assert!(
        !retired.contains("Root cleared fallback") && !retired.contains("Reply cleared fallback")
    );
}

fn render(state: Signal<ShellState>) -> Element {
    crate::view::thread_list_for_test(state, &state.read())
}
fn page(state: Signal<ShellState>) -> ThreadsViewPage {
    ThreadsViewPage {
        threads: state.peek().core.threads_view.items.clone(),
        has_more: true,
        ..Default::default()
    }
}
async fn publish(state: Signal<ShellState>, response: ThreadsViewPage) {
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async { Ok(response) })
            .await
            .is_some()
    );
}
fn row(state: Signal<ShellState>) -> Row {
    let s = state.peek();
    Source::capture(&s)
        .unwrap()
        .row(&s, &s.core.threads_view.items[0])
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn actual_published_row_click_admits_one_original_threads_pane_before_spawn_and_queued_old_credentials_never_load()
 {
    let (mut dom, mut state, captured) = mount("thread-feed-rich-root", render);
    state.write().overlay = None;
    assert!(state.peek().thread_feed_reads.accepted.is_none());
    let response = page(state);
    let channel = response.threads[0].root_msg.channel.clone().unwrap();
    let root = response.threads[0].root_msg.ts.clone().unwrap();
    let id = format!("thread-feed-open-{channel}-{root}");
    assert!(
        button(captured.borrow().as_ref().unwrap(), &id)
            .unwrap()
            .disabled,
        "loaded fixture data alone cannot authorize the native action"
    );
    publish(state, response).await;
    dom.render_immediate_to_vec();
    assert!(
        !button(captured.borrow().as_ref().unwrap(), &id)
            .unwrap()
            .disabled
    );
    let before = {
        let mut s = state.write();
        s.core.composer.text = "Keep channel draft".into();
        let scope = s.thread_scope();
        (
            scope,
            s.thread_generation,
            s.core.threads_view.load_seq,
            format!("{:?}", s.core.threads),
        )
    };
    let original = row(state);
    let control = button(captured.borrow().as_ref().unwrap(), &id).unwrap();
    click(&dom, control);
    // No event-loop pump or await: the actual handler itself starts this pane.
    let admitted = state.peek().thread_scope();
    assert_ne!(admitted, before.0);
    assert_eq!(state.peek().thread_generation, before.1 + 1);
    assert_eq!(state.peek().thread_root.as_deref(), Some(root.as_str()));
    assert_eq!(state.peek().main_view, MainView::Threads);
    assert_eq!(state.peek().surfaces[&MainView::Threads].channel, channel);
    assert_eq!(state.peek().core.composer.text, "Keep channel draft");
    assert_eq!(state.peek().core.threads_view.load_seq, before.2);
    assert!(!original.current(&state.peek()));
    original.start(state);
    assert_eq!(
        state.peek().thread_scope(),
        admitted,
        "retained control cannot begin a second pane"
    );
    {
        let mut s = state.write();
        s.core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .token
            .push_str("-replacement");
        s.toast = None;
    }
    for _ in 0..8 {
        dom.render_immediate_to_vec();
        tokio::task::yield_now().await;
    }
    let s = state.peek();
    assert_eq!(s.thread_scope(), admitted);
    assert!(
        s.thread_error.is_none() && s.toast.is_none(),
        "checked queued task retires before the disconnected transport error path"
    );
    assert_eq!(format!("{:?}", s.core.threads), before.3);
    assert_eq!(s.core.threads_view.load_seq, before.2);
}

#[tokio::test(flavor = "current_thread")]
async fn original_row_tracks_native_publication_and_visit_without_promoting_public_pages_or_losing_cached_failure_content()
 {
    for case in [
        "native-failure",
        "pending",
        "republication",
        "removed",
        "reentry",
        "account",
        "client",
        "public-page",
        "retired-key",
        "presentation",
    ] {
        let (mut dom, mut state, _) = mount("thread-feed-rich-root", render);
        state.write().overlay = None;
        let response = page(state);
        publish(state, response.clone()).await;
        let original = row(state);
        assert!(original.current(&state.peek()));
        match case {
            "native-failure" => {
                let source = Source::capture(&state.peek()).unwrap();
                source
                    .run_with(state, Mode::Refresh, |_| async {
                        Err(Error::Offline("synthetic".into()))
                    })
                    .await;
                assert!(!original.current(&state.peek()));
                assert!(row(state).current(&state.peek()));
                assert_eq!(
                    serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
                    serde_json::to_value(&response.threads).unwrap()
                );
                continue;
            }
            "pending" => {
                let source = Source::capture(&state.peek()).unwrap();
                let (_send, receive) = tokio::sync::oneshot::channel::<()>();
                let mut work = Box::pin(source.run_with(state, Mode::Refresh, |_| async {
                    receive.await.unwrap();
                    Ok(response.clone())
                }));
                crate::bootstrap::test_support::pending(work.as_mut()).await;
                assert!(!original.current(&state.peek()));
                assert!(
                    row(state).current(&state.peek()),
                    "new current cached-row control may open during its known native refresh"
                );
                drop(work);
                assert!(row(state).current(&state.peek()));
                continue;
            }
            "republication" => publish(state, response).await,
            "removed" => state
                .write()
                .core
                .threads_view
                .observe(&RtEvent::MessageDeleted {
                    channel: original.key.0.clone(),
                    deleted_ts: original.key.1.clone(),
                }),
            "reentry" => {
                let mut s = state.write();
                s.main_view = MainView::Home;
                s.close_thread();
                s.main_view = MainView::Threads;
            }
            "account" => state.write().core.active_account = Some("replacement-account".into()),
            "client" => {
                let mut config = super_platinum_core::slack::SlackClientConfig::default();
                config.identity.user_agent.push_str(" changed");
                state.write().core.client = SlackClient::new(config);
            }
            "public-page" => {
                let request = state.write().core.threads_view.begin(false).unwrap();
                assert!(state.write().core.apply_thread_feed_page(
                    "T1",
                    &request,
                    response.clone(),
                    &Default::default()
                ));
                let s = state.peek();
                assert!(
                    Source::capture(&s)
                        .unwrap()
                        .row(&s, &s.core.threads_view.items[0])
                        .is_none(),
                    "a public Core page cannot borrow the old native coordinate"
                );
                drop(s);
                let native = Source::capture(&state.peek()).unwrap();
                let (_send, receive) = tokio::sync::oneshot::channel::<()>();
                let mut held = Box::pin(native.run_with(state, Mode::Refresh, |_| async {
                    receive.await.unwrap();
                    Err(Error::Offline("synthetic".into()))
                }));
                crate::bootstrap::test_support::pending(held.as_mut()).await;
                {
                    let s = state.peek();
                    assert!(
                        s.thread_feed_reads.accepted.is_none(),
                        "native begin must retire the untracked publication permanently"
                    );
                    assert!(
                        Source::capture(&s)
                            .unwrap()
                            .row(&s, &s.core.threads_view.items[0])
                            .is_none()
                    );
                }
                drop(held);
                let native = Source::capture(&state.peek()).unwrap();
                native
                    .run_with(state, Mode::Refresh, |_| async {
                        Err(Error::Offline("synthetic".into()))
                    })
                    .await;
                let s = state.peek();
                assert!(s.thread_feed_reads.accepted.is_none());
                assert!(
                    Source::capture(&s)
                        .unwrap()
                        .row(&s, &s.core.threads_view.items[0])
                        .is_none(),
                    "native failure cannot recreate provenance for a public Core page"
                );
            }
            "retired-key" => {
                let generation = state.peek().core.workspaces["T1"].rt_generation;
                crate::realtime::apply_in_view(
                    &mut state.write().core,
                    "T1",
                    generation,
                    RtEvent::MessageDeleted {
                        channel: original.key.0.clone(),
                        deleted_ts: original.key.1.clone(),
                    },
                    false,
                );
                crate::thread_feed::after_native_mutation(&state.peek(), "T1", generation);
                state
                    .write()
                    .core
                    .threads_view
                    .upsert(response.threads[0].clone());
                let s = state.peek();
                assert!(
                    Source::capture(&s)
                        .unwrap()
                        .row(&s, &s.core.threads_view.items[0])
                        .is_none(),
                    "retirement cannot transfer to an equal-key replacement"
                );
            }
            "presentation" => {
                crate::thread_feed::fixture(&mut state.write(), "thread-feed-rich-root")
            }
            _ => unreachable!(),
        }
        assert!(!original.current(&state.peek()), "{case}");
        let retained_scope = state.peek().thread_scope();
        original.start(state);
        assert_eq!(state.peek().thread_scope(), retained_scope, "{case}");
        dom.render_immediate_to_vec();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn admitted_reply_read_keeps_its_original_guard_after_await_and_cannot_publish_into_a_replacement_owner_or_pane()
 {
    use std::{cell::Cell, rc::Rc};
    for replacement in ["token", "transport", "pane"] {
        let (mut dom, mut state, _) = mount("thread-feed-rich-root", render);
        state.write().overlay = None;
        publish(state, page(state)).await;
        let original = row(state);
        let channel = original.key.0.clone();
        let root = original.key.1.clone();
        let (scope, check) = original.begin(state).unwrap();
        assert!(check(&state.peek()));
        let (send, receive) = tokio::sync::oneshot::channel();
        let attempts = Rc::new(Cell::new(0));
        let done = Rc::new(Cell::new(false));
        let called = attempts.clone();
        let finished = done.clone();
        let expected_root = root.clone();
        let work = crate::bootstrap::load_started_thread_checked_for_test(
            state,
            channel,
            root,
            None,
            scope,
            check,
            move |_, client, session, args| async move {
                called.set(called.get() + 1);
                assert_eq!(args.ts, expected_root);
                assert_eq!(session.token, "synthetic-resource-read");
                receive.await.unwrap();
                api::fetch_replies_with(&client,&session,args,|_|async {
                    Ok(serde_json::json!({"ok":true,"messages":[{"ts":expected_root,"text":"late retired body must not publish"}],"has_more":false}))
                }).await
            },
        );
        dom.in_runtime(|| {
            dom.in_scope(dioxus::core::ScopeId::ROOT, || {
                spawn(async move {
                    work.await;
                    finished.set(true);
                })
            })
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while attempts.get() == 0 {
                dom.render_immediate_to_vec();
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual checked replies driver must reach held fetch");
        {
            let mut s = state.write();
            match replacement {
                "token" => s
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token
                    .push_str("-replacement"),
                "transport" => {
                    s.core.transport = Some(crate::bootstrap::test_support::transport(
                        "synthetic-resource-read",
                    ))
                }
                "pane" => s.begin_thread("1719800500.000000".into(), None),
                _ => unreachable!(),
            }
            s.toast = None;
        }
        let retained_scope = state.peek().thread_scope();
        let retained_bags = format!("{:?}", state.peek().core.threads);
        send.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !done.get() {
                dom.render_immediate_to_vec();
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("retired held read must settle without another request");
        assert_eq!(attempts.get(), 1);
        assert_eq!(state.peek().thread_scope(), retained_scope);
        assert_eq!(format!("{:?}", state.peek().core.threads), retained_bags);
        assert!(state.peek().toast.is_none() && state.peek().thread_error.is_none());
    }
}

#[path = "tests/unread.rs"]
mod unread;
