//! A continuation of the real mounted first-Add workflow, not a NativeAck.
use super::*;
use dioxus::core::{TemplateAttribute, TemplateNode};
const NAME: &str = "party_blob";
const URL: &str = "https://emoji.slack-edge.com/T1/party_blob/new.png";
fn complete() -> Value {
    json!({"ok":true,"emoji":{
        "party_blob":URL,"party_alias":"alias:party_blob","heart":URL,
        "constructor":URL,"bad_http":"http://fixture.invalid/not-admitted.png"
    }})
}
fn mode(dom: &mut VirtualDom, state: Signal<ShellState>, workspace: bool) {
    let p = presentation(&state.peek()).unwrap();
    let id = format!(
        "thread-root-first-add-mode-{}-{}",
        if workspace { "workspace" } else { "emoji" },
        p.id
    );
    let button = control(dom, &id).expect("actual source-mode control");
    assert!(!button.disabled);
    click(dom, button);
    dom.render_immediate_to_vec();
}
fn strings(dom: &VirtualDom) -> Vec<String> {
    fn walk(dom: &VirtualDom, v: &VNode, out: &mut Vec<String>) {
        fn attrs(n: &TemplateNode, out: &mut Vec<String>) {
            if let TemplateNode::Element {
                attrs: a, children, ..
            } = n
            {
                for a in a.iter() {
                    if let TemplateAttribute::Static { value, .. } = a {
                        out.push((*value).into());
                    }
                }
                for n in children.iter() {
                    attrs(n, out);
                }
            }
        }
        for n in v.template.roots.iter() {
            attrs(n, out);
        }
        for group in v.dynamic_attrs.iter() {
            for a in group.iter() {
                if let AttributeValue::Text(s) = &a.value {
                    out.push(s.clone());
                }
            }
        }
        for (i, node) in v.dynamic_nodes.iter().enumerate() {
            match node {
                DynamicNode::Fragment(nodes) => {
                    for v in nodes {
                        walk(dom, v, out)
                    }
                }
                DynamicNode::Component(c) => {
                    if let Some(scope) = c.mounted_scope(i, v, dom) {
                        walk(dom, scope.root_node(), out)
                    }
                }
                DynamicNode::Text(t) => out.push(t.value.clone()),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(dom, dom.base_scope().root_node(), &mut out);
    out
}
fn query(dom: &mut VirtualDom, state: Signal<ShellState>, literal: &str) {
    let id = presentation(&state.peek()).unwrap().id;
    input(dom, &format!("thread-root-first-add-query-{id}"), literal);
    dom.render_immediate_to_vec();
}
pub(super) async fn journey() {
    let (mut dom, mut state, _captured, io, connection) = mount();
    let key = physical(state);
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    let draft = state.peek().core.composer.text.clone();
    let root = raw(false, &user);
    assert!(root.get("reactions").is_none());
    let alias_rows = json!([{"name":"party_alias","count":1,"users":[user]}]);
    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, alias_rows.clone(), true).await;
    query(&mut dom, state, "eyes");
    let old_builtin = choice(state, "eyes");
    let before = io.requests.borrow().len();
    let held = io.expect("/api/emoji.list");
    mode(&mut dom, state, true);
    pump(&mut dom, || io.requests.borrow().len() == before + 1).await;
    let p = presentation(&state.peek()).unwrap();
    assert!(p.custom_mode && p.busy && !p.ready && p.choices.is_empty());
    assert!(!crate::reaction_writes::registry(&state.peek()).pending(&key));
    mode(&mut dom, state, false);
    assert!(
        presentation(&state.peek()).unwrap().ready,
        "held custom data does not disable builtins"
    );
    let completed = io.completed.get();
    held.send(Ok(json!({"ok":true,"emoji":{"stale_blob":URL}})))
        .unwrap();
    pump(&mut dom, || io.completed.get() > completed).await;
    assert!(!presentation(&state.peek()).unwrap().custom_mode);
    assert!(
        !state
            .peek()
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_ref()
            .unwrap()
            .custom
            .loading
    );
    assert!(
        state
            .peek()
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_ref()
            .unwrap()
            .custom
            .ready
            .is_none()
    );

    let failed = io.expect("/api/emoji.list");
    let before = io.requests.borrow().len();
    mode(&mut dom, state, true);
    pump(&mut dom, || io.requests.borrow().len() == before + 1).await;
    failed
        .send(Err(super_platinum_core::slack::Error::Api(
            "synthetic_catalog_failure".into(),
        )))
        .unwrap();
    pump(&mut dom, || {
        presentation(&state.peek()).is_some_and(|p| p.can_retry_custom)
    })
    .await;
    assert_eq!(writes(&io), 0);
    mode(&mut dom, state, false);
    assert!(presentation(&state.peek()).unwrap().ready);
    let reply = io.expect("/api/emoji.list");
    let before = io.requests.borrow().len();
    // Actual owned revisit starts one fresh request, never a /local map grant.
    mode(&mut dom, state, true);
    query(&mut dom, state, "PARTY BLOB");
    pump(&mut dom, || io.requests.borrow().len() == before + 1).await;
    let requested = io.requests.borrow().len();
    query(&mut dom, state, ":Party Blob:");
    assert_eq!(
        io.requests.borrow().len(),
        requested,
        "typing does not refetch definitions"
    );
    reply.send(Ok(complete())).unwrap();
    pump(&mut dom, || {
        presentation(&state.peek()).is_some_and(|p| p.ready && p.custom_mode)
    })
    .await;
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.query, ":Party Blob:");
    assert_eq!(p.total, 1);
    assert_eq!(p.choices[0].name, NAME);
    assert_eq!(p.choices[0].glyph, format!(":{NAME}:"));
    assert!(p.choices[0].image.is_some());
    let old_image = p.choices[0].image.clone().unwrap();
    let old_choice = p.choices[0].action.clone().unwrap();
    let n = io.requests.borrow().len();
    dom.in_scope(ScopeId::ROOT, || choose(state, old_builtin));
    assert_eq!(
        io.requests.borrow().len(),
        n,
        "old builtin frame cannot retarget custom index0"
    );
    query(&mut dom, state, "party");
    assert!(old_image.asset(&state.peek()).is_none());
    dom.in_scope(ScopeId::ROOT, || choose(state, old_choice));
    assert_eq!(io.requests.borrow().len(), n);
    let p = presentation(&state.peek()).unwrap();
    let image = p.choices[0].image.clone().unwrap();
    let media = state.peek().media.clone();
    let old_slot = media.register_emoji(NAME, "https://emoji.slack-edge.com/T1/party_blob/old.png");
    let png = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icons/icon-512.png"
    ))
    .as_slice();
    media.insert(old_slot.clone(), "image/png", png);
    let same_url_slot = media.register_emoji(NAME, URL);
    media.insert_provisional_for_test(same_url_slot.clone(), png);
    assert!(media.is_ready(&same_url_slot));
    assert!(media.ready_bytes(&same_url_slot).is_none());
    let asset = image.asset(&state.peek()).unwrap();
    assert_eq!(asset, same_url_slot, "same URL deduplication is preserved");
    assert_ne!(asset, old_slot);
    assert!(
        media.ready_bytes(&asset).is_none(),
        "provisional prior-URL pixels remain withheld"
    );
    state.write().media_epoch += 1;
    dom.render_immediate_to_vec();
    assert!(!strings(&dom).iter().any(|s| s.contains(URL)));
    use crate::media::MediaUri;
    assert!(
        !strings(&dom).contains(&asset.webview_uri_at(state.peek().media_epoch)),
        "same-URL provisional bytes cannot paint"
    );
    media.insert(asset.clone(), "image/png", png);
    state.write().media_epoch += 1;
    dom.render_immediate_to_vec();
    let uri = asset.webview_uri_at(state.peek().media_epoch);
    assert!(
        strings(&dom).contains(&uri),
        "actual mounted image uses the opaque stamped URI"
    );
    let revision = media.image_state(&asset).0.unwrap();
    assert!(media.reject_image(&asset, revision));
    state.write().media_epoch += 1;
    dom.render_immediate_to_vec();
    assert!(!strings(&dom).contains(&asset.webview_uri_at(state.peek().media_epoch)));
    assert!(strings(&dom).iter().any(|s| s == &format!(":{NAME}:")));
    assert_eq!(
        writes(&io),
        0,
        "image loading/decoder failure cannot issue a mutation"
    );

    let query_work = dom
        .in_scope(ScopeId::ROOT, || {
            picker::capture_query(state, id, "PARTY".into())
        })
        .unwrap();
    let query_matches = query_work.compute();
    let queued = realtime::protected_updates(
        &connection,
        31,
        r#"{"type":"reaction_added","user":"U2","reaction":"wave","item":{"type":"message","channel":"C2","ts":"1712000001.000009"}}"#,
    );
    dom.in_scope(ScopeId::ROOT, || {
        picker::publish_query(state, query_work, query_matches)
    });
    assert!(!presentation(&state.peek()).unwrap().ready);
    assert_eq!(writes(&io), 0);
    assert!(image.asset(&state.peek()).is_none());
    updates(state, queued);
    dom.render_immediate_to_vec();
    assert!(
        presentation(&state.peek()).unwrap().ready,
        "queued irrelevant peer is not permanent retirement"
    );
    assert_eq!(
        presentation(&state.peek()).unwrap().total,
        1,
        "pure matches survive queued-peer publication without another input"
    );
    let selected = choice(state, NAME);
    let queued = realtime::protected_updates(
        &connection,
        31,
        r#"{"type":"reaction_added","user":"U2","reaction":"wave","item":{"type":"message","channel":"C2","ts":"1712000001.000010"}}"#,
    );
    dom.in_scope(ScopeId::ROOT, || choose(state, selected.clone()));
    assert_eq!(writes(&io), 0, "current choice waits for queued peer");
    updates(state, queued);
    dom.render_immediate_to_vec();
    assert!(presentation(&state.peek()).unwrap().ready);
    metadata(&io, &root, alias_rows);
    let ack = io.expect("/api/reactions.add");
    let before = io.requests.borrow().len();
    click(&dom, control(&dom, &selected.id()).unwrap());
    pump(&mut dom, || writes(&io) == 1).await;
    assert_eq!(
        &io.paths()[before..],
        [
            "/api/conversations.history",
            "/api/conversations.info",
            "/api/users.prefs.get",
            "/api/reactions.get",
            "/api/reactions.add"
        ]
    );
    assert_eq!(field(io.requests.borrow().last().unwrap(), "name"), NAME);
    updates(state,realtime::protected_updates(&connection,31,&json!({"type":"reaction_added","user":user,"reaction":NAME,"item":{"type":"message","channel":"C1","ts":ROOT}}).to_string()));
    ack.send(Ok(json!({"ok":true}))).unwrap();
    pump(&mut dom, || {
        !crate::reaction_writes::registry(&state.peek()).pending(&key)
    })
    .await;
    assert!(matches!(
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .outcome,
        Outcome::Acknowledged
    ));
    assert!(opener(state).is_none_or(|o| o.disabled(&state.peek())));
    // A real new getView owner and complete membership are required for Remove.
    let own = json!([{"name":NAME,"count":1,"users":[user]}]);
    let mut refreshed = root.clone();
    refreshed["reactions"] = own.clone();
    publish(&mut dom, state, &io, refreshed.clone()).await;
    let id = open_ready(&mut dom, state, &io, &refreshed, own.clone(), false).await;
    let reply = io.expect("/api/emoji.list");
    let before = io.requests.borrow().len();
    mode(&mut dom, state, true);
    pump(&mut dom, || io.requests.borrow().len() == before + 1).await;
    reply.send(Ok(complete())).unwrap();
    pump(&mut dom, || {
        presentation(&state.peek()).is_some_and(|p| p.ready && p.custom_mode)
    })
    .await;
    query(&mut dom, state, NAME);
    let selected = choice(state, NAME);
    metadata(&io, &refreshed, own);
    let ack = io.expect("/api/reactions.remove");
    click(&dom, control(&dom, &selected.id()).unwrap());
    pump(&mut dom, || writes(&io) == 2).await;
    assert_eq!(field(io.requests.borrow().last().unwrap(), "name"), NAME);
    ack.send(Ok(json!({"ok":true}))).unwrap();
    pump(&mut dom, || {
        !crate::reaction_writes::registry(&state.peek()).pending(&key)
    })
    .await;
    assert_eq!(state.peek().core.composer.text, draft);
    assert!(state.peek().viewer.is_none());
    assert!(state.peek().core.active_thread.is_none());
    let _ = id;

    for retirement in 0..3 {
        let (mut dom, mut state, _captured, io, _connection) = mount();
        let user = state.peek().core.workspaces["T1"].self_user_id.clone();
        let root = raw(false, &user);
        publish(&mut dom, state, &io, root.clone()).await;
        open_ready(&mut dom, state, &io, &root, json!([]), true).await;
        let reply = io.expect("/api/emoji.list");
        let before = io.requests.borrow().len();
        mode(&mut dom, state, true);
        pump(&mut dom, || io.requests.borrow().len() == before + 1).await;
        match retirement {
            0 => {
                let mut config = super_platinum_core::slack::SlackClientConfig::default();
                config
                    .identity
                    .user_agent
                    .push_str(" direct-custom-other-client");
                state.write().core.client = super_platinum_core::slack::SlackClient::new(config);
            }
            1 => state.write().core.active_account = Some("other-account-label".into()),
            _ => {
                state
                    .write()
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .custom_emoji_revision += 1
            }
        }
        reply.send(Ok(complete())).unwrap();
        pump(&mut dom, || {
            state
                .peek()
                .thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_ref()
                .is_none_or(|v| !v.custom.loading)
        })
        .await;
        assert!(presentation(&state.peek()).is_none_or(|p| !p.ready));
        assert_eq!(writes(&io), 0);
        assert!(
            state
                .peek()
                .thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_ref()
                .is_none_or(|v| v.custom.ready.is_none())
        );
    }
}
