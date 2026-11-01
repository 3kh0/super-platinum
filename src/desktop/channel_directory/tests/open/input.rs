//! Actual form input/submit and canonical Open, not native/live acceptance.
use super::*;
use crate::resource_reads::render_test::{Captured, listener};
use dioxus::core::AttributeValue;

fn fire(dom: &VirtualDom, handler: AttributeValue, value: &str) {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let AttributeValue::Listener(callback) = handler else {
        panic!("actual form handler")
    };
    let data = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.in_scope(ScopeId::ROOT, || {
        callback.call(dioxus::core::Event::new(data.into_any(), true))
    });
}
fn handler(captured: &Captured, id: &str, event: &str) -> AttributeValue {
    listener(captured.borrow().as_ref().unwrap(), id, event).unwrap()
}
fn edit(dom: &mut VirtualDom, captured: &Captured, value: &str) {
    dom.render_immediate_to_vec();
    fire(
        dom,
        handler(captured, "your-channels-query", "oninput"),
        value,
    );
    dom.render_immediate_to_vec();
}
async fn page(
    entry: &Entry,
    state: Signal<ShellState>,
    mode: read::Mode,
    number: u32,
    query: &str,
) {
    let (request, lease) = entry.begin(state, mode, None).unwrap();
    assert_eq!(request.query().text, query);
    assert_eq!(request.query().page, number);
    assert!(request.query().scope == Scope::Joined);
    Entry::run_with(request,lease,move|request|async move{
        assert!(request.current());
        Ok(Some(api::decode(json!({"ok":true,"items":[{"id":SECOND,"name":format!("Loaded page {number}"),"is_member":true,"member_count":42}],"pagination":{"page":number,"page_count":2,"next_cursor":if number==1{Some("unsubmitted-next")}else{None}}}),request.query()).unwrap()))
    }).await;
}

#[tokio::test(flavor = "current_thread")]
async fn channel_query_buffer_actual_input_committed_pages_retry_empty_reset_and_canonical_open_reentry()
 {
    let (mut dom, mut state, captured) = mount("your-channels", render);
    initialize(&mut dom, state).await;
    state.write().core.composer.text = "Keep the original channel draft".into();
    let entry = enter(state);
    page(&entry, state, read::Mode::Search, 1, "").await;
    select(&mut dom, &captured, SECOND);
    let original_open = button(captured.borrow().as_ref().unwrap(), "your-channels-open").unwrap();
    assert!(!original_open.disabled);
    let loaded = state.peek().channel_directory.loaded.clone().unwrap();
    let revision = state.peek().channel_directory.revision;
    let canonical_before =
        serde_json::to_value(&state.peek().core.workspaces[TEAM].channels).unwrap();
    edit(&mut dom, &captured, "an unsubmitted search");
    {
        let s = state.peek();
        let d = &s.channel_directory;
        assert_eq!(d.edited.as_deref(), Some("an unsubmitted search"));
        assert_eq!(d.draft, "");
        assert_eq!(d.revision, revision);
        assert!(loaded.current(&s) && Arc::ptr_eq(&loaded, d.loaded.as_ref().unwrap()));
        assert_eq!(d.selected.as_deref(), Some(SECOND));
        assert!(d.pending.is_none() && d.failed.is_none());
    }
    assert!(
        !button(captured.borrow().as_ref().unwrap(), "your-channels-open")
            .unwrap()
            .disabled
    );

    // Typing and an implicit submit cannot hide the held read's busy revision.
    let (request, lease) = entry.begin(state, read::Mode::Refresh, None).unwrap();
    let pending = state.peek().channel_directory.pending.clone().unwrap();
    let revision = state.peek().channel_directory.revision;
    edit(&mut dom, &captured, "typed while a read is held");
    let submit = handler(&captured, "your-channels-form", "onsubmit");
    fire(&dom, submit.clone(), "");
    assert!(request.current());
    assert_eq!(state.peek().channel_directory.revision, revision);
    assert!(Arc::ptr_eq(
        &pending,
        state.peek().channel_directory.pending.as_ref().unwrap()
    ));
    assert_eq!(state.peek().channel_directory.draft, "");
    Entry::run_with(request,lease,|request|async move{
        assert!(request.current());
        Ok(Some(api::decode(json!({"ok":true,"items":[{"id":SECOND,"name":"Held read completed","is_member":true}],"pagination":{"page":1,"page_count":2,"next_cursor":"unsubmitted-next"}}),request.query()).unwrap()))
    }).await;
    assert_eq!(
        state.peek().channel_directory.edited.as_deref(),
        Some("typed while a read is held")
    );
    assert!(
        state
            .peek()
            .channel_directory
            .loaded
            .as_ref()
            .unwrap()
            .current(&state.peek())
    );

    // Paging and Retry spend the committed coordinate, never the visible edit.
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-next").unwrap(),
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    let failed = state.peek().channel_directory.failed.clone().unwrap();
    assert_eq!(
        (failed.text.as_str(), failed.page, failed.cursor.as_deref()),
        ("", 2, Some("unsubmitted-next"))
    );
    page(&entry, state, read::Mode::Retry, 2, "").await;
    assert_eq!(
        state.peek().channel_directory.edited.as_deref(),
        Some("typed while a read is held")
    );

    // Filter/sort controls retain edits and use the committed query.
    dom.render_immediate_to_vec();
    fire(
        &dom,
        handler(&captured, "your-channels-type", "onchange"),
        "shared",
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    assert!(state.peek().channel_directory.failed.as_ref().unwrap().kind == Kind::Shared);
    assert_eq!(
        state.peek().channel_directory.failed.as_ref().unwrap().text,
        ""
    );
    page(&entry, state, read::Mode::Retry, 1, "").await;
    dom.render_immediate_to_vec();
    fire(
        &dom,
        handler(&captured, "your-channels-sort", "onchange"),
        "members",
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    assert!(state.peek().channel_directory.failed.as_ref().unwrap().sort == Sort::MostMembers);
    assert_eq!(
        state.peek().channel_directory.failed.as_ref().unwrap().text,
        ""
    );
    page(&entry, state, read::Mode::Retry, 1, "").await;
    assert_eq!(
        state.peek().channel_directory.edited.as_deref(),
        Some("typed while a read is held")
    );
    let last_loaded = state.peek().channel_directory.loaded.clone().unwrap();
    edit(&mut dom, &captured, "submitted release");
    let submit = handler(&captured, "your-channels-form", "onsubmit");
    fire(&dom, submit.clone(), "");
    let pending = state.peek().channel_directory.pending.clone().unwrap();
    let revision = state.peek().channel_directory.revision;
    assert_eq!(state.peek().channel_directory.draft, "submitted release");
    assert!(state.peek().channel_directory.edited.is_none());
    fire(&dom, submit.clone(), "");
    assert_eq!(state.peek().channel_directory.revision, revision);
    assert!(Arc::ptr_eq(
        &pending,
        state.peek().channel_directory.pending.as_ref().unwrap()
    ));
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    let failed = state.peek().channel_directory.failed.clone().unwrap();
    assert_eq!(
        (failed.text.as_str(), failed.page, failed.cursor.as_deref()),
        ("submitted release", 1, None)
    );
    assert!(failed.sort == Sort::Relevant);
    assert!(Arc::ptr_eq(
        &last_loaded,
        state.peek().channel_directory.loaded.as_ref().unwrap()
    ));
    assert!(
        !state
            .peek()
            .channel_directory
            .loaded
            .as_ref()
            .unwrap()
            .current(&state.peek())
    );
    click(&dom, original_open);
    assert_eq!(state.peek().core.active_channel.as_deref(), Some(FIRST));
    edit(&mut dom, &captured, "a newer unsubmitted edit");
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-retry").unwrap(),
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    assert_eq!(
        state.peek().channel_directory.failed.as_ref().unwrap().text,
        "submitted release"
    );
    page(&entry, state, read::Mode::Retry, 1, "submitted release").await;
    assert_eq!(
        state.peek().channel_directory.edited.as_deref(),
        Some("a newer unsubmitted edit")
    );

    // Auto-clear cannot mask a held nonempty query's busy revision.
    let (request, lease) = entry.begin(state, read::Mode::Refresh, None).unwrap();
    let revision = state.peek().channel_directory.revision;
    edit(&mut dom, &captured, " \t ");
    assert!(request.current());
    assert_eq!(state.peek().channel_directory.revision, revision);
    assert_eq!(state.peek().channel_directory.draft, "submitted release");
    Entry::run_with(request,lease,|request|async move{
        assert!(request.current());
        Ok(Some(api::decode(json!({"ok":true,"items":[{"id":SECOND,"name":"Current held nonempty query","is_member":true}],"pagination":{"page":1,"page_count":1}}),request.query()).unwrap()))
    }).await;
    assert!(crate::channel_directory::input::empty_edit(
        "\t\n\u{00a0}\u{1680}\u{2000}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}"
    ));
    for value in ["\u{0085}", "\u{180e}", "text"] {
        assert!(!crate::channel_directory::input::empty_edit(value));
    }
    edit(&mut dom, &captured, "\u{0085}");
    assert_eq!(state.peek().channel_directory.draft, "submitted release");
    assert!(state.peek().channel_directory.pending.is_none());
    // Known canClear; JavaScript trims BOM, but does not trim NEL.
    dom.render_immediate_to_vec();
    // Observe synchronous reservation before render_immediate polls the task.
    fire(
        &dom,
        handler(&captured, "your-channels-query", "oninput"),
        "\u{feff}\t",
    );
    assert_eq!(state.peek().channel_directory.draft, "");
    assert_eq!(
        state.peek().channel_directory.edited.as_deref(),
        Some("\u{feff}\t")
    );
    assert!(state.peek().channel_directory.kind == Kind::default());
    assert!(state.peek().channel_directory.busy());
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    assert_eq!(
        state.peek().channel_directory.failed.as_ref().unwrap().text,
        ""
    );
    page(&entry, state, read::Mode::Retry, 1, "").await;
    assert_eq!(
        state.peek().channel_directory.edited.as_deref(),
        Some("\u{feff}\t")
    );
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-reset").unwrap(),
    );
    assert!(state.peek().channel_directory.edited.is_none());
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    page(&entry, state, read::Mode::Retry, 1, "").await;
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces[TEAM].channels).unwrap(),
        canonical_before
    );
    assert_eq!(
        state.peek().core.composer.text,
        "Keep the original channel draft"
    );

    // A current canonical result still opens while unrelated input is edited.
    edit(&mut dom, &captured, "retained but not submitted");
    select(&mut dom, &captured, SECOND);
    let retired_input = handler(&captured, "your-channels-query", "oninput");
    let retired_submit = handler(&captured, "your-channels-form", "onsubmit");
    let open = button(captured.borrow().as_ref().unwrap(), "your-channels-open").unwrap();
    assert!(!open.disabled);
    // The original Open-control tests cover the unchanged actual handler.
    // Use its existing guarded action/reader seam here: the member-fields
    // renderer cannot truthfully evaluate history scrolling or read marks.
    let action = {
        let s = state.peek();
        let d = &s.channel_directory;
        crate::channel_directory::open::capture(&s, d.loaded.clone().unwrap(), d.revision, SECOND)
            .unwrap()
    };
    let work = action.apply(state).unwrap();
    let reads = Rc::new(Cell::new(0));
    let calls = reads.clone();
    work.run_with(state, move |_| async move {
        calls.set(calls.get() + 1);
    })
    .await;
    assert_eq!(
        reads.get(),
        1,
        "current successor reaches inert history dependency once"
    );
    let generation = state.peek().channel_generation;
    click(&dom, open);
    assert_eq!(
        state.peek().channel_generation,
        generation,
        "queued control cannot replay consumed open"
    );
    assert_eq!(state.peek().core.active_channel.as_deref(), Some(SECOND));
    assert!(state.peek().overlay.is_none());
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "open-your-channels").unwrap(),
    );
    let next = state.peek().channel_directory.entry.clone().unwrap();
    assert!(!Arc::ptr_eq(&entry.origin, &next.origin));
    assert!(state.peek().channel_directory.loaded.is_none());
    assert!(state.peek().channel_directory.selected.is_none());
    assert_eq!(state.peek().channel_directory.draft, "");
    assert_eq!(
        state.peek().channel_directory.edited.as_deref(),
        Some("retained but not submitted")
    );
    let revision = state.peek().channel_directory.revision;
    fire(&dom, retired_input, "old visit input");
    fire(&dom, retired_submit, "");
    assert_eq!(state.peek().channel_directory.revision, revision);
    assert_eq!(
        state.peek().channel_directory.edited.as_deref(),
        Some("retained but not submitted")
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    assert_eq!(
        state.peek().channel_directory.failed.as_ref().unwrap().text,
        ""
    );
    page(&next, state, read::Mode::Retry, 1, "").await;
    dom.render_immediate_to_vec();
    fire(
        &dom,
        handler(&captured, "your-channels-form", "onsubmit"),
        "",
    );
    assert_eq!(
        state.peek().channel_directory.draft,
        "retained but not submitted"
    );
    assert!(state.peek().channel_directory.edited.is_none());
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;

    // The extra passive allocation obeys the existing4096-byte memory bound.
    edit(&mut dom, &captured, &"x".repeat(4097));
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-close").unwrap(),
    );
    let successor = enter(state);
    assert_eq!(
        state.peek().channel_directory.draft,
        "retained but not submitted"
    );
    assert!(state.peek().channel_directory.edited.is_none());
    edit(&mut dom, &captured, "current actor input");
    let old_input = handler(&captured, "your-channels-query", "oninput");
    let old_submit = handler(&captured, "your-channels-form", "onsubmit");
    let before = (
        state.peek().channel_directory.draft.clone(),
        state.peek().channel_directory.edited.clone(),
        state.peek().channel_directory.revision,
    );
    state.write().core.account_epoch += 1;
    fire(&dom, old_input, "lost owner");
    fire(&dom, old_submit, "");
    assert!(!successor.current(&state.peek()));
    assert_eq!(
        (
            state.peek().channel_directory.draft.clone(),
            state.peek().channel_directory.edited.clone(),
            state.peek().channel_directory.revision
        ),
        before
    );
}
