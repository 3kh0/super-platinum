use super::*;

pub(crate) fn rich_wire(state: Signal<ShellState>, page: u32) -> Value {
    let mut value = response(state, page, 7, 121, false);
    let message = &mut value["items"][0]["messages"][0];
    message["text"] = json!("Fallback must not duplicate");
    message["blocks"] = json!([
        {"type":"rich_text","elements":[
            {"type":"rich_text_section","elements":[
                {"type":"text","text":"\u{e000}Indexed release\u{e001}\nSecond paragraph","style":{"bold":true}},
                {"type":"link","url":"https://fixture.invalid/notes","text":"Read notes"}]},
            {"type":"rich_text_list","style":"ordered","elements":[
                {"type":"rich_text_section","elements":[{"type":"text","text":"Review changes"}]}]},
            {"type":"rich_text_preformatted","elements":[{"type":"text","text":"cargo test --locked"}]},
            {"type":"rich_text_quote","elements":[{"type":"text","text":"Keep indexed context"}]}
        ]},
        {"type":"image","image_url":"https://fixture.invalid/search.png","alt_text":"Search diagram"},
        {"type":"actions","elements":[{"type":"button","action_id":"inert-search","text":{"type":"plain_text","text":"Review action"},"url":"https://fixture.invalid/action"}]}
    ]);
    value
}
pub(crate) async fn rich_harness() -> Harness {
    let h = setup();
    let (request, read) = first(&h);
    load(
        &h,
        request,
        read,
        Ok(rich_wire(h.state, 1)),
        &Cell::new(0),
        &Cell::new(0),
    )
    .await;
    h
}

#[tokio::test]
async fn accepted_indexed_body_preserves_styles_cache_first_duplicate_and_shared_edit_separation() {
    let h = rich_harness().await;
    let mut state = h.state;
    let old = rows(&state.peek())[0].clone();
    let indexed_body = body(&state.peek(), 0).to_vec();
    let debug = format!("{indexed_body:?}");
    for expected in [
        "bold: true",
        "List {",
        "Code(",
        "Quote(",
        "Read notes",
        "Search diagram",
        "Review action",
    ] {
        assert!(debug.contains(expected), "{expected}");
    }
    for absent in [
        "Link {",
        "Button {",
        "Media {",
        "InlineImage",
        "ImageBlock",
        "FileMention",
        "Canvas(",
        "fixture.invalid",
        "Fallback must not duplicate",
        "\u{e000}",
        "\u{e001}",
    ] {
        assert!(!debug.contains(absent), "{absent}");
    }
    assert!(!old.text.contains(['\u{e000}', '\u{e001}']));
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry(old.channel_id.clone())
        .or_default()
        .upsert(
            serde_json::from_value(
                json!({"ts":old.ts,"user":"U_PAGE_AUTHOR","text":"Before shared edit"}),
            )
            .unwrap(),
        );
    let event = super_platinum_core::slack::realtime::parse_event(&json!({"type":"message","subtype":"message_changed","channel":old.channel_id,"message":{"ts":old.ts,"user":"U_PAGE_AUTHOR","text":"New shared body"}}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    assert_eq!(
        state.peek().core.workspaces["T1"].messages[&old.channel_id]
            .messages
            .iter()
            .find(|m| m.ts.as_deref() == Some(&old.ts))
            .and_then(|m| m.text.as_deref()),
        Some("New shared body")
    );
    assert_eq!(body(&state.peek(), 0), indexed_body.as_slice());
    assert_eq!(rows(&state.peek())[0], old);
    let calls = Cell::new(0);
    let lookups = Cell::new(0);
    let Plan::Fetch(work) = planned(state, 2, false) else {
        panic!()
    };
    let mut wire = response(state, 2, 7, 121, false);
    let mut duplicate = wire["items"][0]["messages"][0].clone();
    wire["items"][0]["messages"][0]["team"] = json!("FOREIGN");
    duplicate["text"] = json!("Later duplicate cannot lend body");
    wire["items"][0]["messages"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    finish(&h, work, Ok(wire), &calls, &lookups).await;
    assert_eq!(rows(&state.peek()).len(), 1);
    assert_eq!(rows(&state.peek())[0].text, "Page 2 release");
    assert!(body(&state.peek(), 0).is_empty());
    assert!(matches!(planned(state, 1, false), Plan::Cached));
    assert_eq!(body(&state.peek(), 0), indexed_body.as_slice());
    assert_eq!(rows(&state.peek())[0], old);
    assert_eq!(calls.get(), 1);
    state
        .write()
        .set_search_query("retired indexed page".into());
    assert!(body(&state.peek(), 0).is_empty());
    assert!(!active(&state.peek()));
}
