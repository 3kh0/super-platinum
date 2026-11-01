//! Actual owned refresh -> header -> Close/reopen -> denied new-version journey.
use super::*;

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_refresh_reopen_keeps_owned_details_and_header_version_current_without_canonical_publication()
 {
    let h = harness();
    let mut initial = h.state;
    setup(initial);
    let before = users(initial);
    let reader = admitted(initial).await;
    members_ready(reader, initial).await;
    let shell = std::mem::replace(
        &mut *initial.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, captured) = mount(shell);
    let id = "thread-profile-C1-10.0-10.0";
    let stale = open(state, "10.0").unwrap();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), id).unwrap(),
    );
    dom.render_immediate_to_vec();
    let token = crate::bootstrap::profile_search::presentation(&state.peek(), "U1")
        .unwrap()
        .retry
        .unwrap();
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    crate::bootstrap::profile_search::refresh_member_with(
        state,
        token,
        move |_, request| async move {
            count.set(count.get() + 1);
            let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
                panic!()
            };
            assert_eq!(body["updated_ids"], json!({"U1":0}));
            let mut fresh = member("U1", "Newest Maya");
            fresh["profile"]["title"] = json!("Fresh title");
            fresh["profile"]["phone"] = json!("+15550000001");
            Ok(json!({"results":[fresh]}))
        },
    )
    .await;
    dom.render_immediate_to_vec();
    assert_eq!(reads.get(), 1);
    let next = open(state, "10.0").unwrap();
    assert_eq!(
        next.user(&state.peek())
            .unwrap()
            .user()
            .profile
            .as_ref()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Newest Maya")
    );
    let generation = state.peek().profile_generation;
    stale.start(state);
    assert_eq!(
        state.peek().profile_generation,
        generation,
        "old header cannot borrow refreshed member version"
    );
    assert_eq!(
        users(state),
        before,
        "fresh reading projection never mutates canonical users"
    );
    state.write().close_profile();
    dom.render_immediate_to_vec();
    let header = text(captured.borrow().as_ref().unwrap());
    assert!(header.contains("Newest Maya") && !header.contains("Fresh Maya"));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), id).unwrap(),
    );
    dom.render_immediate_to_vec();
    let reopened = text(captured.borrow().as_ref().unwrap());
    assert!(
        reopened.contains("Newest Maya")
            && reopened.contains("Fresh title")
            && reopened.contains("+15550000001")
    );
    assert!(reopened.contains("Read-only profile details"));
    assert!(!reopened.contains("Message") && !reopened.contains("Huddle"));
    assert_eq!(
        reads.get(),
        1,
        "reopen reuses admitted refreshed details without another lookup"
    );
    assert!(!state.peek().core.profile_pane.as_ref().unwrap().loading);
    assert_eq!(users(state), before);
    // The active pane must carry the NEW origin, or denial would retire only
    // the old member and a fresh header could resurrect the superseded data.
    let token = crate::bootstrap::profile_search::presentation(&state.peek(), "U1")
        .unwrap()
        .retry
        .unwrap();
    crate::bootstrap::profile_search::refresh_member_with(state, token, |_, _| async {
        let mut denied = member("U1", "Denied new version");
        denied["deleted"] = json!(true);
        Ok(json!({"results":[denied]}))
    })
    .await;
    assert!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U1")
            .unwrap()
            .user
            .is_none()
    );
    assert!(open(state, "10.0").is_none());
    next.start(state);
    assert!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U1")
            .unwrap()
            .user
            .is_none()
    );
    assert_eq!(users(state), before);
}
