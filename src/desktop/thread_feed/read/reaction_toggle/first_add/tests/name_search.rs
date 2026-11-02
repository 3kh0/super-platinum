//! One genuine originalRoot/cold catalog/query/selection/publication trajectory.
use super::*;
const ALIAS: &str = "thumbsup::skin-tone-3";
const CANONICAL: &str = "+1::skin-tone-3";
fn query(dom: &mut VirtualDom, id: u64, literal: &str) {
    input(dom, &format!("thread-root-first-add-query-{id}"), literal);
    dom.render_immediate_to_vec();
}
#[tokio::test(flavor = "current_thread")]
async fn root_first_add_name_search_prefix_alias_wire_and_stale_query_owner_publication() {
    let (mut dom, state, captured, io, _connection) = mount();
    let key = physical(state);
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    let draft = state.peek().core.composer.text.clone();
    let canonical = json!([{"name":CANONICAL,"count":1,"users":[user]}]);
    let peer = json!([{"name":ALIAS,"count":1,"users":["U2"]},{"name":CANONICAL,"count":1,"users":[user]}]);
    let mut root = raw(false, &user);
    root["reactions"] = peer.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, peer.clone(), true).await;
    assert_eq!(presentation(&state.peek()).unwrap().total, 1892);
    query(&mut dom, id, "thumb");
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.total, 4);
    assert_eq!(
        p.choices
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        [
            ALIAS,
            "thumbsdown::skin-tone-3",
            "beach_with_umbrella",
            "hand_with_index_finger_and_thumb_crossed::skin-tone-3"
        ]
    );
    assert_eq!(p.choices[0].glyph, "👍🏼");
    let stale = choice(state, ALIAS);
    let held = picker::capture_query(state, id, ":thumb:".into()).unwrap();
    let old = held.compute();
    query(&mut dom, id, "eyes");
    let fresh = presentation(&state.peek()).unwrap();
    assert_eq!(fresh.total, 16);
    let frame = fresh.frame;
    let rows = fresh
        .choices
        .iter()
        .map(|c| c.name.clone())
        .collect::<Vec<_>>();
    picker::publish_query(state, held, old);
    dom.render_immediate_to_vec();
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.query, "eyes");
    assert_eq!(p.frame, frame);
    assert_eq!(
        p.choices.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        rows
    );
    let n = io.requests.borrow().len();
    dom.in_scope(ScopeId::ROOT, || choose(state, stale));
    assert_eq!(io.requests.borrow().len(), n);
    assert!(!crate::reaction_writes::registry(&state.peek()).pending(&key));
    assert!(presentation(&state.peek()).unwrap().ready);
    query(&mut dom, id, "thumb");
    let selected = choice(state, ALIAS);
    metadata(&io, &root, peer.clone());
    let ack = io.expect("/api/reactions.add");
    click(&dom, control(&dom, &selected.id()).unwrap());
    pump(&mut dom, || writes(&io) == 1).await;
    assert_eq!(
        field(io.requests.borrow().last().unwrap(), "name"),
        ALIAS,
        "winning raw alias is not replaced by a canonical synonym"
    );
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
        crate::reaction_writes::Outcome::Acknowledged
    ));

    root["reactions"] = canonical.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, canonical.clone(), false).await;
    query(&mut dom, id, "thumb");
    let selected = choice(state, ALIAS);
    metadata(&io, &root, peer);
    let before = io.requests.borrow().len();
    click(&dom, control(&dom, &selected.id()).unwrap());
    assert!(crate::reaction_writes::registry(&state.peek()).pending(&key));
    pump(&mut dom, || {
        !crate::reaction_writes::registry(&state.peek()).pending(&key)
    })
    .await;
    assert_eq!(io.requests.borrow().len(), before + 4);
    assert_eq!(writes(&io), 1);
    assert!(matches!(
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .outcome,
        crate::reaction_writes::Outcome::NotSent
    ));

    let id = open_ready(&mut dom, state, &io, &root, canonical.clone(), false).await;
    let held = picker::capture_query(state, id, "thumb".into()).unwrap();
    let old = held.compute();
    dom.render_immediate_to_vec();
    click(
        &dom,
        control(&dom, &format!("thread-root-first-add-close-{id}")).unwrap(),
    );
    dom.render_immediate_to_vec();
    let replacement = open_ready(&mut dom, state, &io, &root, canonical, false).await;
    assert_ne!(replacement, id);
    let fresh = presentation(&state.peek()).unwrap();
    let frame = fresh.frame;
    picker::publish_query(state, held, old);
    dom.render_immediate_to_vec();
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.id, replacement);
    assert_eq!(p.frame, frame);
    assert_eq!(p.query, "");
    assert_eq!(p.total, 1892);
    assert!(p.ready);
    assert!(!crate::reaction_writes::registry(&state.peek()).pending(&key));
    assert_eq!(writes(&io), 1);
    assert_eq!(state.peek().core.composer.text, draft);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("First Add root content"));
    assert!(state.peek().viewer.is_none());
    assert!(state.peek().core.active_thread.is_none());

    // Continue the same mounted journey with ordinary spaced/mixed-case input.
    // All original prefix, stale-publication, retarget and draft assertions above
    // remain; the new raw input still selects the existing opaque alias token.
    click(
        &dom,
        control(&dom, &format!("thread-root-first-add-close-{replacement}")).unwrap(),
    );
    dom.render_immediate_to_vec();
    let peer = json!([{"name":ALIAS,"count":1,"users":["U2"]},{"name":CANONICAL,"count":1,"users":[user]}]);
    root["reactions"] = peer.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, peer.clone(), false).await;
    let literal = " \u{feff}:ThUmB\t  Up:\u{a0}";
    query(&mut dom, id, literal);
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(
        p.query, literal,
        "normalization does not replace the original input occurrence"
    );
    assert_eq!(p.total, 1);
    assert_eq!(p.choices[0].name, ALIAS);
    assert_eq!(p.choices[0].glyph, "👍🏼");
    let selected = choice(state, ALIAS);
    metadata(&io, &root, peer);
    let ack = io.expect("/api/reactions.add");
    let before = io.requests.borrow().len();
    click(&dom, control(&dom, &selected.id()).unwrap());
    pump(&mut dom, || writes(&io) == 2).await;
    assert_eq!(io.requests.borrow().len(), before + 5);
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
    assert_eq!(field(io.requests.borrow().last().unwrap(), "name"), ALIAS);
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
        crate::reaction_writes::Outcome::Acknowledged
    ));
    assert_eq!(writes(&io), 2);
    assert_eq!(state.peek().core.composer.text, draft);
    assert!(state.peek().viewer.is_none());
    assert!(state.peek().core.active_thread.is_none());
    super::custom::journey().await;
}
