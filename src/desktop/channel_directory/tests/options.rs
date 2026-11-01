//! Real shared-type/sort form handlers through the unchanged native read owner.
use super::*;
use crate::resource_reads::render_test::{Captured, listener};
use dioxus::core::AttributeValue;
fn change(dom: &VirtualDom, captured: &Captured, id: &str, value: &str) -> AttributeValue {
    let control = listener(captured.borrow().as_ref().unwrap(), id, "onchange").unwrap();
    dispatch(dom, control.clone(), value);
    control
}
fn dispatch(dom: &VirtualDom, control: AttributeValue, value: &str) {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let AttributeValue::Listener(callback) = control else {
        panic!("actual select callback")
    };
    let data = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.in_scope(ScopeId::ROOT, || {
        callback.call(dioxus::core::Event::new(data.into_any(), true))
    });
}

#[tokio::test(flavor = "current_thread")]
async fn actual_shared_and_standard_sort_choices_keep_query_buckets_paging_and_owner_current() {
    let (mut dom, mut state, captured) = mount("your-channels", render);
    let entry = enter(state);
    entry.change(state, Some("release".into()), None);
    publish(&entry, state, read::Mode::Search, 1).await;
    dom.render_immediate_to_vec();
    let shown = text(captured.borrow().as_ref().unwrap());
    for label in [
        "Slack Connect channels",
        "Newest channel",
        "Oldest channel",
        "Fewest members",
    ] {
        assert!(shown.contains(label), "{label}");
    }
    let conversation = (
        state.peek().core.active_channel.clone(),
        state.peek().channel_generation,
        state.peek().core.composer.text.clone(),
    );
    change(&dom, &captured, "your-channels-type", "shared");
    assert!(state.peek().channel_directory.busy());
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    assert!(state.peek().channel_directory.failed.as_ref().unwrap().kind == Kind::Shared);
    publish(&entry, state, read::Mode::Retry, 1).await;
    let old_page = state.peek().channel_directory.loaded.clone().unwrap();
    let mut original_sort = None;
    for (value, sort) in [
        ("fewest-members", Sort::FewestMembers),
        ("created-newest", Sort::CreatedNewest),
        ("created-oldest", Sort::CreatedOldest),
    ] {
        dom.render_immediate_to_vec();
        original_sort = Some(change(&dom, &captured, "your-channels-sort", value));
        assert!(state.peek().channel_directory.busy());
        assert!(!old_page.current(&state.peek()));
        settle(&mut dom, || !state.peek().channel_directory.busy()).await;
        let failed = state.peek().channel_directory.failed.clone().unwrap();
        assert!(
            failed.kind == Kind::Shared && failed.scope == Scope::Joined && failed.sort == sort
        );
        assert_eq!(failed.text, "release");
        assert_eq!((failed.page, failed.cursor), (1, None));
        assert!(
            state.peek().channel_directory.loaded.is_some(),
            "failed sort keeps earlier metadata inert"
        );
        publish(&entry, state, read::Mode::Retry, 1).await;
    }
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-next").unwrap(),
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    let failed = state.peek().channel_directory.failed.clone().unwrap();
    assert!(failed.kind == Kind::Shared && failed.sort == Sort::CreatedOldest);
    assert_eq!(
        (failed.page, failed.cursor.as_deref()),
        (2, Some("next-page"))
    );
    publish(&entry, state, read::Mode::Retry, 2).await;
    entry.change(state, Some(String::new()), None);
    assert!(
        state.peek().channel_directory.sort() == Sort::Name,
        "browse bucket retains its own default"
    );
    dom.render_immediate_to_vec();
    change(&dom, &captured, "your-channels-sort", "fewest-members");
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    publish(&entry, state, read::Mode::Retry, 1).await;
    entry.change(state, Some("release".into()), None);
    assert!(
        state.peek().channel_directory.sort() == Sort::CreatedOldest,
        "search sort survives the separate browse choice"
    );
    let revision = state.peek().channel_directory.revision;
    state.write().core.account_epoch += 1;
    dispatch(&dom, original_sort.unwrap(), "created-newest");
    assert_eq!(state.peek().channel_directory.revision, revision);
    assert!(!state.peek().channel_directory.busy());
    assert_eq!(
        (
            state.peek().core.active_channel.clone(),
            state.peek().channel_generation,
            state.peek().core.composer.text.clone()
        ),
        conversation
    );
}
