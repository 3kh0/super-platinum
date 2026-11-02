//! Complete original-root preferred glyph workflow; synthetic scoped IO only.
use super::*;
fn metadata_pref(io: &Io, root: &Value, rows: Value, pref: Value) {
    let mut preferences = prefs();
    preferences["prefs"]["preferred_skin_tone"] = pref;
    for (path, body) in [
        ("/api/conversations.history", history(root)),
        ("/api/conversations.info", info()),
        ("/api/users.prefs.get", preferences),
        ("/api/reactions.get", full(root, rows)),
    ] {
        io.expect(path).send(Ok(body)).unwrap();
    }
}
async fn ready(
    dom: &mut VirtualDom,
    state: Signal<ShellState>,
    io: &Io,
    root: &Value,
    rows: Value,
    pref: Value,
    cold: bool,
) -> u64 {
    let emoji = cold.then(|| io.expect("/api/emoji.list"));
    metadata_pref(io, root, rows, pref);
    let open = opener(state).unwrap();
    dom.render_immediate_to_vec();
    click(dom, control(dom, &open.id()).unwrap());
    if let Some(reply) = emoji {
        reply.send(Ok(json!({"ok":true,"emoji":{}}))).unwrap();
    }
    pump(dom, || presentation(&state.peek()).is_some_and(|p| p.ready)).await;
    presentation(&state.peek()).unwrap().id
}
fn filter(dom: &mut VirtualDom, id: u64, query: &str) {
    input(dom, &format!("thread-root-first-add-query-{id}"), query);
    dom.render_immediate_to_vec();
}

#[tokio::test(flavor = "current_thread")]
async fn root_first_add_preferred_wave_exact_add_remove_changed_pref_refusal_base_and_unknown_fallback()
 {
    let (mut dom, state, captured, io, _connection) = mount();
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    let draft = state.peek().core.composer.text.clone();
    let key = physical(state);
    let mut root = raw(false, &user);
    let other = json!([{"name":"wave::skin-tone-4","count":1,"users":[user]}]);
    root["reactions"] = other.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = ready(&mut dom, state, &io, &root, other.clone(), json!(3), true).await;
    assert_eq!(presentation(&state.peek()).unwrap().total, 1892);
    filter(&mut dom, id, "wave");
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.choices.first().unwrap().name, "wave::skin-tone-3");
    assert_eq!(p.choices.first().unwrap().glyph, "👋🏼");
    assert!(!crate::reaction_writes::registry(&state.peek()).pending(&key));
    let add = choice(state, "wave::skin-tone-3");
    metadata_pref(&io, &root, other.clone(), json!(3));
    let ack = io.expect("/api/reactions.add");
    click(&dom, control(&dom, &add.id()).unwrap());
    pump(&mut dom, || writes(&io) == 1).await;
    assert_eq!(
        io.paths().last().unwrap(),
        "/api/reactions.add",
        "another own tone is not Remove"
    );
    assert_eq!(
        field(io.requests.borrow().last().unwrap(), "name"),
        "wave::skin-tone-3"
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

    let both = json!([{"name":"wave::skin-tone-3","count":1,"users":[user]},{"name":"wave::skin-tone-4","count":1,"users":[user]}]);
    root["reactions"] = both.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = ready(&mut dom, state, &io, &root, both.clone(), json!(3), false).await;
    filter(&mut dom, id, "wave");
    let remove = choice(state, "wave::skin-tone-3");
    metadata_pref(&io, &root, both, json!(3));
    let ack = io.expect("/api/reactions.remove");
    click(&dom, control(&dom, &remove.id()).unwrap());
    pump(&mut dom, || writes(&io) == 2).await;
    assert_eq!(
        field(io.requests.borrow().last().unwrap(), "name"),
        "wave::skin-tone-3"
    );
    ack.send(Ok(json!({"ok":true}))).unwrap();
    pump(&mut dom, || {
        !crate::reaction_writes::registry(&state.peek()).pending(&key)
    })
    .await;
    assert!(
        !crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .adding
    );

    root["reactions"] = other.clone();
    publish(&mut dom, state, &io, root.clone()).await;
    let id = ready(&mut dom, state, &io, &root, other.clone(), json!(3), false).await;
    filter(&mut dom, id, "wave");
    let captured_choice = choice(state, "wave::skin-tone-3");
    metadata_pref(&io, &root, other.clone(), json!(4));
    click(&dom, control(&dom, &captured_choice.id()).unwrap());
    assert!(crate::reaction_writes::registry(&state.peek()).pending(&key));
    pump(&mut dom, || {
        !crate::reaction_writes::registry(&state.peek()).pending(&key)
    })
    .await;
    assert_eq!(
        writes(&io),
        2,
        "fresh preference change refuses, never recolors or replays"
    );
    assert!(matches!(
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .outcome,
        crate::reaction_writes::Outcome::NotSent
    ));
    assert!(
        opener(state).is_some(),
        "no attempted wire spent the original roster"
    );

    let id = ready(&mut dom, state, &io, &root, other.clone(), json!(1), false).await;
    filter(&mut dom, id, "wave");
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.choices.first().unwrap().name, "wave");
    assert_eq!(p.choices.first().unwrap().glyph, "👋");
    let base = choice(state, "wave");
    metadata_pref(&io, &root, other.clone(), json!(1));
    let ack = io.expect("/api/reactions.add");
    click(&dom, control(&dom, &base.id()).unwrap());
    pump(&mut dom, || writes(&io) == 3).await;
    assert_eq!(field(io.requests.borrow().last().unwrap(), "name"), "wave");
    ack.send(Ok(json!({"ok":true}))).unwrap();
    pump(&mut dom, || {
        !crate::reaction_writes::registry(&state.peek()).pending(&key)
    })
    .await;

    publish(&mut dom, state, &io, root.clone()).await;
    let id = ready(&mut dom, state, &io, &root, other, json!("3"), false).await;
    assert_eq!(
        presentation(&state.peek()).unwrap().total,
        1582,
        "unknown preference retains only original tone-free choices"
    );
    filter(&mut dom, id, "wave");
    assert!(
        !presentation(&state.peek())
            .unwrap()
            .choices
            .iter()
            .any(|c| c.name == "wave" || c.name.starts_with("wave::skin-tone-"))
    );
    click(
        &dom,
        control(&dom, &format!("thread-root-first-add-close-{id}")).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert!(presentation(&state.peek()).is_none());
    assert_eq!(writes(&io), 3);
    assert_eq!(state.peek().core.composer.text, draft);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("First Add root content"));
    assert!(state.peek().viewer.is_none());
    assert!(state.peek().core.active_thread.is_none());
}
