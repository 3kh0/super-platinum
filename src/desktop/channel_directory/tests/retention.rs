use super::*;

#[tokio::test(flavor = "current_thread")]
async fn actual_close_reopen_restores_only_intent_and_starts_fresh_page_without_reviving_old_work()
{
    let (mut dom, state, captured) = mount("your-channels", render);
    let entry = enter(state);
    entry.change(state, None, Some(Sort::FewestMembers));
    entry.change(state, Some("release".into()), Some(Sort::CreatedNewest));
    entry.change_kind(state, Kind::Shared);
    publish(&entry, state, read::Mode::Search, 1).await;
    publish(&entry, state, read::Mode::Next, 2).await;
    let old_page = state.peek().channel_directory.loaded.clone().unwrap();
    let (old_request, old_lease) = entry.begin(state, read::Mode::Refresh, None).unwrap();
    let before = (
        state.peek().core.active_channel.clone(),
        state.peek().channel_generation,
        state.peek().core.composer.text.clone(),
    );
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-close").unwrap(),
    );
    assert!(state.peek().overlay.is_none());
    assert!(!old_request.current() && !old_page.current(&state.peek()));
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "open-your-channels").unwrap(),
    );
    assert!(
        state.peek().channel_directory.busy(),
        "actual reopen reserves a fresh request before spawn"
    );
    let reopened = state.peek().channel_directory.entry.clone().unwrap();
    assert!(Arc::ptr_eq(&entry.family, &reopened.family));
    assert!(!Arc::ptr_eq(&entry.origin, &reopened.origin));
    assert!(
        state.peek().channel_directory.loaded.is_none()
            && state.peek().channel_directory.selected.is_none()
    );
    assert!(state.peek().channel_directory.cursors.is_empty());
    entry.change(state, Some("old visit input".into()), None);
    entry.close(state);
    drop(old_lease);
    assert!(reopened.current(&state.peek()) && state.peek().channel_directory.busy());
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    let failed = state.peek().channel_directory.failed.clone().unwrap();
    assert_eq!(failed.text, "release");
    assert!(
        failed.kind == Kind::Shared
            && failed.scope == Scope::Joined
            && failed.sort == Sort::CreatedNewest
    );
    assert_eq!((failed.page, failed.cursor), (1, None));
    publish(&reopened, state, read::Mode::Retry, 1).await;
    reopened.change(state, Some(String::new()), None);
    assert!(
        state.peek().channel_directory.sort() == Sort::FewestMembers,
        "browse bucket survives independently"
    );
    assert_eq!(
        (
            state.peek().core.active_channel.clone(),
            state.peek().channel_generation,
            state.peek().core.composer.text.clone()
        ),
        before
    );
}

#[tokio::test(flavor = "current_thread")]
async fn expiry_owner_and_workspace_isolation_preserve_the_original_account_rate_limit() {
    let (_dom, mut state, _) = mount("your-channels", render);
    let a = enter(state);
    a.change(
        state,
        Some("private T1 intent".into()),
        Some(Sort::MostMembers),
    );
    a.change_kind(state, Kind::Private);
    a.close(state);
    {
        let mut s = state.write();
        let mut w = s.core.workspaces["T1"].clone();
        w.team_id = "T2".into();
        let mut session = s.core.session.as_ref().unwrap().workspaces["T1"].clone();
        session.team_id = "T2".into();
        s.core.workspaces.insert("T2".into(), w);
        s.core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .insert("T2".into(), session);
        s.core.active_team = Some("T2".into());
    }
    let b = enter(state);
    assert!(
        state.peek().channel_directory.draft.is_empty()
            && state.peek().channel_directory.kind == Kind::Active
    );
    b.change(state, Some("T2 only".into()), Some(Sort::CreatedOldest));
    b.close(state);
    state.write().core.active_team = Some("T1".into());
    let again = enter(state);
    assert!(Arc::ptr_eq(&a.family, &again.family));
    assert_eq!(state.peek().channel_directory.draft, "private T1 intent");
    assert!(
        state.peek().channel_directory.kind == Kind::Private
            && state.peek().channel_directory.sort() == Sort::MostMembers
    );
    again.close(state);
    assert!(
        a.family
            .retained
            .lock()
            .unwrap()
            .take_at(Instant::now() + super::super::retention::WINDOW)
            .is_none(),
        "five-minute closed intent expires without a timer"
    );
    a.family.observe(&Err(Error::RateLimited {
        retry_after_secs: Some(60),
    }));
    let expired = enter(state);
    assert!(
        state.peek().channel_directory.draft.is_empty()
            && state.peek().channel_directory.sort() == Sort::Name
    );
    assert!(
        expired.begin(state, read::Mode::Search, None).is_none(),
        "expiring settings cannot clear original429"
    );
    expired.change(state, Some("old owner".into()), None);
    expired.close(state);
    state.write().core.account_epoch += 1;
    let successor = enter(state);
    assert!(!Arc::ptr_eq(&a.family, &successor.family) && successor.family.ready());
    assert!(
        state.peek().channel_directory.draft.is_empty()
            && state.peek().channel_directory.kind == Kind::Active
    );
    expired.change(state, Some("stale overwrite".into()), None);
    expired.close(state);
    assert!(successor.current(&state.peek()) && state.peek().channel_directory.draft.is_empty());
    successor.change(state, Some("x".repeat(4097)), None);
    successor.close(state);
    let bounded = enter(state);
    assert!(
        state.peek().channel_directory.draft.is_empty(),
        "invalid oversized input is not retained"
    );
    bounded.change(state, Some("invalid\0intent".into()), None);
    bounded.close(state);
    let last = enter(state);
    assert!(last.current(&state.peek()) && state.peek().channel_directory.draft.is_empty());
}
