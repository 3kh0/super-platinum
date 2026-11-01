use super::*;

pub(crate) fn root_wire(state: Signal<ShellState>) -> Value {
    let mut value = rich::rich_wire(state, 1);
    value["items"][0]["messages"][0]["reply_count"] = json!(3);
    value
}
pub(crate) async fn root_harness() -> Harness {
    let h = setup();
    let (request, read) = first(&h);
    load(
        &h,
        request,
        read,
        Ok(root_wire(h.state)),
        &Cell::new(0),
        &Cell::new(0),
    )
    .await;
    h
}
pub(crate) fn reply_wire(channel: &str, root: &str) -> Value {
    json!({"ok":true,"messages":[
        {"type":"message","channel":channel,"ts":root,"user":"U_PAGE_AUTHOR","text":"Accepted thread root","reply_count":3},
        {"type":"message","channel":channel,"ts":"120.000002","thread_ts":root,"user":"U_PAGE_AUTHOR","text":"Accepted first reply"},
        {"type":"message","channel":channel,"ts":"120.000003","thread_ts":root,"user":"U_PAGE_AUTHOR","text":"Accepted second reply"},
        {"type":"message","channel":channel,"ts":"120.000004","thread_ts":root,"user":"U_PAGE_AUTHOR","text":"Accepted third reply"}
    ],"has_more":false,"response_metadata":{"next_cursor":""}})
}

pub(crate) fn history_io() -> std::rc::Rc<crate::bootstrap::search_navigation::test_io::Io> {
    std::rc::Rc::new(Default::default())
}
pub(crate) fn replies_io() -> std::rc::Rc<crate::bootstrap::thread::test_io::Io> {
    std::rc::Rc::new(Default::default())
}
pub(crate) fn provide_io(
    dom: &dioxus::core::VirtualDom,
    history: std::rc::Rc<crate::bootstrap::search_navigation::test_io::Io>,
    replies: std::rc::Rc<crate::bootstrap::thread::test_io::Io>,
) {
    dom.in_scope(dioxus::core::ScopeId::ROOT, || {
        dioxus::prelude::provide_context(history);
        dioxus::prelude::provide_context(replies);
    });
}

#[tokio::test]
async fn search_root_thread_requires_coherent_ordinary_root_positive_count_and_original_row_owner()
{
    let h = root_harness().await;
    let mut state = h.state;
    let row = rows(&state.peek())[0].clone();
    let source = ThreadSource::capture(&state.peek(), 0).unwrap();
    let resolved = source.resolve(&state.peek()).unwrap();
    assert_eq!(resolved.thread_ts.as_deref(), Some(row.ts.as_str()));
    assert_eq!(
        (&resolved.channel_id, &resolved.ts),
        (&row.channel_id, &row.ts)
    );
    assert_eq!(
        RowSource::capture(&state.peek(), 0)
            .unwrap()
            .resolve(&state.peek())
            .unwrap(),
        row,
        "normal row still opens the original indexed target"
    );
    assert_eq!(reply_count(&state.peek(), 0), Some(3));
    let canonical = serde_json::from_value(
        json!({"ts":row.ts,"channel":row.channel_id,"reply_count":0,"text":"Known root"}),
    )
    .unwrap();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry(row.channel_id.clone())
        .or_default()
        .upsert(canonical);
    assert!(reply_count(&state.peek(), 0).is_none());
    assert!(
        source.resolve(&state.peek()).is_none(),
        "queued root control cannot borrow stale positive count"
    );
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .remove(&row.channel_id);
    let (request, read) = first(&h);
    let mut broadcast = root_wire(state);
    broadcast["items"][0]["messages"][0]["subtype"] = json!("thread_broadcast");
    load(
        &h,
        request,
        read,
        Ok(broadcast),
        &Cell::new(0),
        &Cell::new(0),
    )
    .await;
    assert!(ThreadSource::capture(&state.peek(), 0).is_none());
    assert!(
        source.resolve(&state.peek()).is_none(),
        "fresh Book is never the old control's owner"
    );
    for change in ["reply", "foreign", "zero"] {
        let (request, read) = first(&h);
        let mut value = root_wire(state);
        match change {
            "reply" => value["items"][0]["messages"][0]["thread_ts"] = json!("119.000000"),
            "foreign" => value["items"][0]["messages"][0]["team"] = json!("OTHER"),
            _ => value["items"][0]["messages"][0]["reply_count"] = json!(0),
        }
        load(&h, request, read, Ok(value), &Cell::new(0), &Cell::new(0)).await;
        assert!(
            ThreadSource::capture(&state.peek(), 0).is_none(),
            "{change}"
        );
    }
    state.write().set_search_query("replacement query".into());
    assert!(source.resolve(&state.peek()).is_none());
}
