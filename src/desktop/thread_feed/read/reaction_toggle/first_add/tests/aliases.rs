//! Complete exact-alias query lifecycle; scoped IO/VNode callbacks, no OS input.
use super::*;
const ALIAS: &str = "thumbsup::skin-tone-3";
const CANONICAL: &str = "+1::skin-tone-3";
fn query(dom: &mut VirtualDom, id: u64, literal: &str) {
    input(dom, &format!("thread-root-first-add-query-{id}"), literal);
    dom.render_immediate_to_vec();
}
async fn settled(
    dom: &mut VirtualDom,
    state: Signal<ShellState>,
    key: &crate::reaction_writes::Key,
) {
    pump(dom, || {
        !crate::reaction_writes::registry(&state.peek()).pending(key)
    })
    .await;
}
#[tokio::test(flavor = "current_thread")]
async fn root_first_add_alias_query_canonical_remove_literal_add_remove_and_both_wire_retarget_refusals()
 {
    let (mut dom, state, captured, io, connection) = mount();
    let key = physical(state);
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    let draft = state.peek().core.composer.text.clone();
    let canonical = json!([{"name":CANONICAL,"count":1,"users":[user]}]);
    let peer = json!([{"name":ALIAS,"count":1,"users":["U2"]},{"name":CANONICAL,"count":1,"users":[user]}]);
    let own = json!([{"name":ALIAS,"count":1,"users":[user]},{"name":CANONICAL,"count":1,"users":[user]}]);
    let mut root = raw(false, &user);
    root["reactions"] = canonical.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, canonical.clone(), true).await;
    assert_eq!(
        presentation(&state.peek()).unwrap().total,
        1892,
        "aliases do not inflate ordinary rows"
    );
    query(&mut dom, id, "thumbsup");
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.total, 1);
    assert_eq!(p.choices[0].name, ALIAS);
    assert_eq!(p.choices[0].glyph, "👍🏼");
    let stale = choice(state, ALIAS);
    let frame = p.frame;
    let before = io.requests.borrow().len();
    query(&mut dom, id, ":thumbsup:");
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.query, ":thumbsup:");
    assert!(p.frame > frame);
    dom.in_scope(ScopeId::ROOT, || choose(state, stale));
    assert_eq!(
        io.requests.borrow().len(),
        before,
        "same normalized alias does not recycle a raw query occurrence"
    );
    let selected = choice(state, ALIAS);
    metadata(&io, &root, canonical.clone());
    let ack = io.expect("/api/reactions.remove");
    click(&dom, control(&dom, &selected.id()).unwrap());
    pump(&mut dom, || writes(&io) == 1).await;
    assert_eq!(
        field(io.requests.borrow().last().unwrap(), "name"),
        CANONICAL,
        "absent alias preserves canonical exact-self Remove"
    );
    ack.send(Ok(json!({"ok":true}))).unwrap();
    settled(&mut dom, state, &key).await;
    assert!(
        !crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .adding
    );

    root["reactions"] = peer.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, peer.clone(), false).await;
    query(&mut dom, id, "thumbsup");
    let selected = choice(state, ALIAS);
    metadata(&io, &root, peer.clone());
    let ack = io.expect("/api/reactions.add");
    click(&dom, control(&dom, &selected.id()).unwrap());
    pump(&mut dom, || writes(&io) == 2).await;
    assert_eq!(
        field(io.requests.borrow().last().unwrap(), "name"),
        ALIAS,
        "own canonical is not family Remove of a peer alias"
    );
    updates(state,realtime::protected_updates(&connection,31,&json!({"type":"reaction_added","user":user,"reaction":ALIAS,"item":{"type":"message","channel":"C1","ts":ROOT}}).to_string()));
    assert!(crate::reaction_writes::registry(&state.peek()).pending(&key));
    ack.send(Ok(json!({"ok":true}))).unwrap();
    settled(&mut dom, state, &key).await;
    assert!(matches!(
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .outcome,
        crate::reaction_writes::Outcome::Acknowledged
    ));
    root["reactions"] = own.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, own.clone(), false).await;
    query(&mut dom, id, "thumbsup");
    let selected = choice(state, ALIAS);
    metadata(&io, &root, own);
    let ack = io.expect("/api/reactions.remove");
    click(&dom, control(&dom, &selected.id()).unwrap());
    pump(&mut dom, || writes(&io) == 3).await;
    assert_eq!(field(io.requests.borrow().last().unwrap(), "name"), ALIAS);
    ack.send(Ok(json!({"ok":true}))).unwrap();
    settled(&mut dom, state, &key).await;
    assert!(
        !crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .adding
    );

    // The matched alias stays fixed; only a fresh explicit reopen may choose
    // a different normalization branch after full membership changes.
    root["reactions"] = canonical.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, canonical.clone(), false).await;
    query(&mut dom, id, "thumbsup");
    let selected = choice(state, ALIAS);
    metadata(&io, &root, peer.clone());
    let reads_before = io.requests.borrow().len();
    click(&dom, control(&dom, &selected.id()).unwrap());
    assert!(crate::reaction_writes::registry(&state.peek()).pending(&key));
    settled(&mut dom, state, &key).await;
    assert_eq!(
        io.requests.borrow().len(),
        reads_before + 4,
        "appearance refusal follows four fresh reads"
    );
    assert_eq!(
        writes(&io),
        3,
        "alias appearance cannot retarget to literal wire"
    );
    assert!(matches!(
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .outcome,
        crate::reaction_writes::Outcome::NotSent
    ));
    let id = open_ready(&mut dom, state, &io, &root, peer, false).await;
    query(&mut dom, id, "thumbsup");
    let selected = choice(state, ALIAS);
    metadata(&io, &root, canonical);
    let reads_before = io.requests.borrow().len();
    click(&dom, control(&dom, &selected.id()).unwrap());
    assert!(crate::reaction_writes::registry(&state.peek()).pending(&key));
    settled(&mut dom, state, &key).await;
    assert_eq!(
        io.requests.borrow().len(),
        reads_before + 4,
        "disappearance refusal follows four fresh reads"
    );
    assert_eq!(
        writes(&io),
        3,
        "alias disappearance cannot retarget to canonical wire"
    );
    assert!(matches!(
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .outcome,
        crate::reaction_writes::Outcome::NotSent
    ));
    assert!(opener(state).is_some());
    assert_eq!(state.peek().core.composer.text, draft);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("First Add root content"));
    assert!(state.peek().viewer.is_none());
    assert!(state.peek().core.active_thread.is_none());
}
