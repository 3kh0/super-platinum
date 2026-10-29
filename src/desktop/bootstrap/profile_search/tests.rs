use super::*;
use crate::bootstrap::test_support::{harness, pending, transport, users};
use serde_json::json;

fn user(id: &str, name: &str) -> User {
    serde_json::from_value(json!({"id":id,"is_admin":true,"profile":{"display_name":name,"email":"person@example.test"}})).unwrap()
}
fn token(state: Signal<ShellState>, id: &str) -> Retry {
    presentation(&state.peek(), id).unwrap().retry.unwrap()
}
fn name(state: Signal<ShellState>, id: &str) -> Option<String> {
    presentation(&state.peek(), id).and_then(|p| {
        p.user
            .and_then(|u| u.profile.as_ref())
            .and_then(|p| p.display_name.clone())
    })
}

#[tokio::test]
async fn unknown_profile_failure_retry_success_and_new_credentials_keep_details_scoped() {
    let h = harness();
    let mut state = h.state;
    let before = users(state);
    let ticket = show(&mut state.write(), user("U_REMOTE", "Search hint")).unwrap();
    assert!(presentation(&state.peek(), "U_REMOTE").unwrap().loading);
    assert_eq!(name(state, "U_REMOTE").as_deref(), Some("Search hint"));
    run_with(state, ticket, || async {
        Err(Error::Transport(
            "https://example.test/?token=HIDDEN".into(),
        ))
    })
    .await;
    assert_eq!(
        presentation(&state.peek(), "U_REMOTE")
            .unwrap()
            .error
            .as_deref(),
        Some(FAILED)
    );
    let retry = token(state, "U_REMOTE");
    let ticket = begin_retry(&mut state.write(), retry, true).unwrap();
    run_with(state, ticket, || async {
        Ok(vec![user("U_WRONG", "Wrong identity")])
    })
    .await;
    assert_eq!(name(state, "U_REMOTE").as_deref(), Some("Search hint"));
    assert_eq!(
        presentation(&state.peek(), "U_REMOTE")
            .unwrap()
            .error
            .as_deref(),
        Some(FAILED)
    );
    let retry = token(state, "U_REMOTE");
    let ticket = begin_retry(&mut state.write(), retry, true).unwrap();
    run_with(state, ticket, || async {
        Ok(vec![user("U_REMOTE", "Fresh details")])
    })
    .await;
    assert_eq!(name(state, "U_REMOTE").as_deref(), Some("Fresh details"));
    let previous = token(state, "U_REMOTE");
    {
        let mut shell = state.write();
        shell.core.session.as_mut().unwrap().d_cookie = "refreshed".into();
        shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .token = "fresh-token".into();
        shell.core.transport = Some(transport("refreshed"));
    }
    assert!(name(state, "U_REMOTE").is_none());
    assert_eq!(
        presentation(&state.peek(), "U_REMOTE")
            .unwrap()
            .error
            .as_deref(),
        Some(RETIRED)
    );
    let ticket = begin_retry(&mut state.write(), previous, true).unwrap();
    assert!(name(state, "U_REMOTE").is_none());
    run_with(state, ticket, || async {
        Ok(vec![user("U_REMOTE", "After refresh")])
    })
    .await;
    assert_eq!(name(state, "U_REMOTE").as_deref(), Some("After refresh"));
    assert_eq!(users(state), before);
    let previous = token(state, "U_REMOTE");
    state.write().core.active_account = Some("other-account".into());
    state.write().core.account_epoch += 1;
    assert!(presentation(&state.peek(), "U_REMOTE").is_none());
    assert!(begin_retry(&mut state.write(), previous, true).is_none());
}

#[tokio::test]
async fn held_profile_response_cannot_replace_same_id_reopen_or_another_persons_loading_slot() {
    for next_id in ["U_REMOTE", "U_ANOTHER"] {
        let h = harness();
        let mut state = h.state;
        let before = users(state);
        let ticket = show(&mut state.write(), user("U_REMOTE", "Original hint")).unwrap();
        let (release, held) = tokio::sync::oneshot::channel();
        let mut work = Box::pin(run_with(state, ticket, || async move {
            held.await.unwrap();
            Ok(vec![user("U_REMOTE", "Retired details")])
        }));
        pending(work.as_mut()).await;
        state.write().close_profile();
        let next = show(&mut state.write(), user(next_id, "Replacement hint")).unwrap();
        release.send(()).unwrap();
        work.await;
        assert_eq!(name(state, next_id).as_deref(), Some("Replacement hint"));
        assert!(presentation(&state.peek(), next_id).unwrap().loading);
        run_with(state, next, || async {
            Ok(vec![user(next_id, "Current details")])
        })
        .await;
        assert_eq!(name(state, next_id).as_deref(), Some("Current details"));
        assert_eq!(users(state), before);
    }
}

#[tokio::test]
async fn known_user_path_stays_canonical_and_uncached_self_id_receives_only_scoped_details() {
    let h = harness();
    let mut state = h.state;
    assert!(show(&mut state.write(), user("U2", "Untrusted override")).is_none());
    assert!(state.peek().profile_search.is_none());
    let self_id = state.peek().core.workspaces["T1"].self_user_id.clone();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .remove(&self_id);
    let before = users(state);
    let ticket = show(&mut state.write(), user(&self_id, "Self hint")).unwrap();
    assert!(presentation(&state.peek(), &self_id).is_some());
    run_with(state, ticket, || async {
        Ok(vec![user(&self_id, "Scoped self details")])
    })
    .await;
    assert_eq!(
        name(state, &self_id).as_deref(),
        Some("Scoped self details")
    );
    assert!(
        !state.peek().core.workspaces["T1"]
            .users
            .contains_key(&self_id)
    );
    assert_eq!(users(state), before);
}

#[cfg(feature = "protected-native-fixture")]
#[tokio::test]
async fn real_reconnect_retries_failed_read_only_profile_before_resource_surface_return() {
    let mut h = harness();
    let mut state = h.state;
    let before = users(state);
    let ticket = show(&mut state.write(), user("U_REMOTE", "Retained hint")).unwrap();
    run_with(state, ticket, || async {
        Err(Error::Offline("synthetic".into()))
    })
    .await;
    state.write().channel_tab = crate::state::ChannelTab::Canvas;
    let serial = state.peek().profile_search.as_ref().unwrap().serial;
    h._dom.in_runtime(|| {
        h._dom.in_scope(dioxus::core::ScopeId::ROOT, || {
            spawn(async move {
                crate::bootstrap::reload_after_outage(state).await;
            })
        })
    });
    h._dom.render_immediate_to_vec();
    assert!(
        state.peek().profile_search.as_ref().unwrap().serial > serial,
        "Profile retry was skipped by the resource-tab return"
    );
    assert_eq!(state.peek().profile_user.as_deref(), Some("U_REMOTE"));
    assert_eq!(name(state, "U_REMOTE").as_deref(), Some("Retained hint"));
    assert_eq!(users(state), before);
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}
