use super::*;

#[tokio::test(flavor = "current_thread")]
async fn actual_reset_reads_first_default_filter_page_preserves_sort_and_retires_old_callbacks() {
    let (mut dom, mut state, captured) = mount("your-channels", render);
    let entry = enter(state);
    entry.change(state, None, Some(Sort::FewestMembers));
    entry.change(state, Some("release".into()), Some(Sort::CreatedNewest));
    entry.change_kind(state, Kind::Shared);
    publish(&entry, state, read::Mode::Search, 1).await;
    publish(&entry, state, read::Mode::Next, 2).await;
    let old_page = state.peek().channel_directory.loaded.clone().unwrap();
    let before = (
        state.peek().core.active_channel.clone(),
        state.peek().channel_generation,
        state.peek().core.composer.text.clone(),
    );
    dom.render_immediate_to_vec();
    let old_row = button(captured.borrow().as_ref().unwrap(), "your-channel-C2").unwrap();
    let reset = button(captured.borrow().as_ref().unwrap(), "your-channels-reset").unwrap();
    assert!(!reset.disabled);
    // A queued reset must not discard input made since its own render.
    entry.change(state, Some("newer draft".into()), None);
    click(&dom, reset);
    assert_eq!(state.peek().channel_directory.draft, "newer draft");
    assert!(!state.peek().channel_directory.busy());
    entry.change(state, Some("release".into()), None);
    let (old_request, old_lease) = entry.begin(state, read::Mode::Refresh, None).unwrap();
    // Capture at the current revision before a read begins. The real control
    // is disabled during a current read; this also checks the handler fence.
    dom.render_immediate_to_vec();
    assert!(
        button(captured.borrow().as_ref().unwrap(), "your-channels-reset")
            .unwrap()
            .disabled
    );
    drop(old_lease);
    assert!(!old_request.current());
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-reset").unwrap(),
    );
    assert!(
        state.peek().channel_directory.busy(),
        "reset reserves before spawn"
    );
    assert!(!old_page.current(&state.peek()));
    click(&dom, old_row);
    assert!(state.peek().channel_directory.selected.is_none());
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    let failed = state.peek().channel_directory.failed.clone().unwrap();
    assert!(failed.text.is_empty() && failed.kind == Kind::Active && failed.scope == Scope::Joined);
    assert!(failed.sort == Sort::FewestMembers);
    assert_eq!((failed.page, failed.cursor), (1, None));
    publish(&entry, state, read::Mode::Retry, 1).await;
    entry.change(state, Some("again".into()), None);
    assert!(state.peek().channel_directory.sort() == Sort::CreatedNewest);
    dom.render_immediate_to_vec();
    let reset = button(captured.borrow().as_ref().unwrap(), "your-channels-reset").unwrap();
    entry.family.observe(&Err(Error::RateLimited {
        retry_after_secs: Some(60),
    }));
    click(&dom, reset);
    assert!(!entry.family.ready() && !state.peek().channel_directory.busy());
    let blocked = state.peek().channel_directory.failed.clone().unwrap();
    assert!(blocked.text.is_empty() && blocked.kind == Kind::Active);
    assert_eq!((blocked.page, blocked.cursor), (1, None));
    assert!(blocked.sort == Sort::FewestMembers);
    // A callback from the departed account/visit cannot reset its successor.
    dom.render_immediate_to_vec();
    let retired = button(captured.borrow().as_ref().unwrap(), "your-channels-reset").unwrap();
    entry.close(state);
    state.write().core.account_epoch += 1;
    let successor = enter(state);
    successor.change(state, Some("new account draft".into()), None);
    click(&dom, retired);
    assert_eq!(state.peek().channel_directory.draft, "new account draft");
    assert!(successor.current(&state.peek()) && successor.family.ready());
    assert_eq!(
        (
            state.peek().core.active_channel.clone(),
            state.peek().channel_generation,
            state.peek().core.composer.text.clone()
        ),
        before
    );
}
