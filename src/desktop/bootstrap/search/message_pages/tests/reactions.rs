use super::*;

pub(crate) fn reaction_wire(state: Signal<ShellState>, page: u32) -> Value {
    let mut value = rich::rich_wire(state, page);
    value["items"][0]["messages"][0]["reactions"] = json!([
        {"name":"thumbsup","count":2,"users":["U1","U2"]},
        {"name":"fixture_unknown","count":1,"users":["U2"]}
    ]);
    value
}
pub(crate) async fn reaction_harness() -> Harness {
    let h = setup();
    let (request, read) = first(&h);
    load(
        &h,
        request,
        read,
        Ok(reaction_wire(h.state, 1)),
        &Cell::new(0),
        &Cell::new(0),
    )
    .await;
    h
}
pub(crate) fn counts(state: Signal<ShellState>) -> Vec<(String, u32)> {
    super::super::reactions(&state.peek(), 0)
        .into_iter()
        .map(|r| (r.name, r.count))
        .collect()
}
pub(crate) fn shared(mut state: Signal<ShellState>, thread_only: bool, empty: bool) {
    let row = rows(&state.peek())[0].clone();
    let message=serde_json::from_value(json!({"ts":row.ts,"user":"U_PAGE_AUTHOR","text":"Shared body must not replace indexed body",
        "reactions":if empty{json!([])}else{json!([{"name":"thumbsup","count":4,"users":["U1","U2","U4","U5"]}])}})).unwrap();
    if thread_only {
        state
            .write()
            .core
            .threads
            .entry(("T1".into(), row.channel_id, row.thread_ts.unwrap_or(row.ts)))
            .or_default()
            .upsert(message);
    } else {
        state
            .write()
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .entry(row.channel_id)
            .or_default()
            .upsert(message);
    }
}
pub(crate) fn socket(mut state: Signal<ShellState>, kind: &str) {
    let row = rows(&state.peek())[0].clone();
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let value = match kind {
        "partial" => {
            json!({"type":"message","subtype":"message_replied","channel":row.channel_id,"message":{"ts":row.ts,"user":"U_PAGE_AUTHOR","text":"Later shared body","reply_count":3}})
        }
        "deleted" => {
            json!({"type":"message","subtype":"message_deleted","channel":row.channel_id,"deleted_ts":row.ts})
        }
        kind => {
            json!({"type":kind,"user":"U3","reaction":"thumbsup","item":{"type":"message","channel":row.channel_id,"ts":row.ts}})
        }
    };
    let event = super_platinum_core::slack::realtime::parse_event(&value.to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
}

#[tokio::test]
async fn search_reaction_reading_uses_supplied_fallback_live_shared_empty_cache_retry_and_exact_owner()
 {
    let h = reaction_harness().await;
    let mut state = h.state;
    let original = rows(&state.peek())[0].clone();
    let indexed = body(&state.peek(), 0).to_vec();
    assert_eq!(
        counts(state),
        [("thumbsup".into(), 2), ("fixture_unknown".into(), 1)]
    );
    assert!(
        !state.peek().core.workspaces["T1"]
            .messages
            .get(&original.channel_id)
            .is_some_and(|bag| bag
                .messages
                .iter()
                .any(|m| m.ts.as_deref() == Some(&original.ts))),
        "search does not seed a canonical message"
    );
    shared(state, true, false);
    assert_eq!(
        counts(state),
        [("thumbsup".into(), 4)],
        "exact thread bag is a shared copy too"
    );
    socket(state, "reaction_added");
    assert_eq!(counts(state), [("thumbsup".into(), 5)]);
    socket(state, "partial");
    assert_eq!(
        counts(state),
        [("thumbsup".into(), 5)],
        "partial parent cannot wipe reactions"
    );
    socket(state, "reaction_removed");
    assert_eq!(counts(state), [("thumbsup".into(), 4)]);
    shared(state, false, true);
    assert!(
        counts(state).is_empty(),
        "present empty shared list suppresses indexed and other-copy fallback"
    );
    assert_eq!(body(&state.peek(), 0), indexed);
    assert_eq!(rows(&state.peek())[0], original);
    let calls = Cell::new(0);
    let lookups = Cell::new(0);
    let Plan::Fetch(work) = planned(state, 2, false) else {
        panic!()
    };
    assert!(counts(state).is_empty());
    finish(
        &h,
        work,
        Err(Error::Offline("synthetic outage".into())),
        &calls,
        &lookups,
    )
    .await;
    assert!(status(&state.peek()).unwrap().5);
    assert_eq!(rows(&state.peek())[0], original);
    let Plan::Fetch(work) = planned(state, 2, true) else {
        panic!()
    };
    let mut foreign = reaction_wire(state, 2);
    foreign["items"][0]["messages"][0]["team"] = json!("FOREIGN");
    let mut duplicate = foreign["items"][0]["messages"][0].clone();
    duplicate["team"] = json!("T1");
    foreign["items"][0]["messages"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    finish(&h, work, Ok(foreign), &calls, &lookups).await;
    shared(state, false, false);
    assert!(
        counts(state).is_empty(),
        "foreign first row and later duplicate cannot lend reaction state"
    );
    assert!(matches!(planned(state, 1, false), Plan::Cached));
    assert!(
        counts(state).is_empty(),
        "page1's shared-empty overlay survives cache return"
    );
    let row = rows(&state.peek())[0].clone();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .remove(&row.channel_id);
    state.write().core.threads.clear();
    assert_eq!(
        counts(state).len(),
        2,
        "coherent supplied fallback remains in page cache"
    );
    socket(state, "deleted");
    assert!(counts(state).is_empty());
    assert!(row_was_deleted(&state.peek(), 0));
    assert_eq!(calls.get(), 2);
    state.write().set_search_query("retired result".into());
    assert!(counts(state).is_empty());
    assert!(!active(&state.peek()));
}

pub(crate) async fn grouping_harness() -> Harness {
    let h = setup();
    let (request, read) = first(&h);
    let mut value = reaction_wire(h.state, 1);
    value["items"][0]["messages"][0]["reactions"] = json!([
        {"name":"thumbsup::skin-tone-2","count":2,"users":["U1","U1","U2"]},
        {"name":"shipit","count":0,"users":[]},
        {"name":"thumbsup::skin-tone-5","count":3,"users":["U2","U3","U4"]},
        {"name":"thumbsup","count":1,"users":["U5"]},
        {"name":"+1","count":1,"users":["U1"]}
    ]);
    load(&h, request, read, Ok(value), &Cell::new(0), &Cell::new(0)).await;
    h
}
