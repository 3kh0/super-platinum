use super::*;
use crate::bootstrap::test_support::{harness, pending};
use serde_json::json;
use std::{cell::Cell, rc::Rc};
use super_platinum_core::{
    slack::{
        Error,
        models::{Channel, Emoji, Message, User},
    },
    state::ChannelMessages,
};
fn setup(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    shell.overlay = None;
    shell.main_view = crate::state::MainView::Home;
    shell.sidebar_sections.clear();
    shell.thread_root = None;
    shell.profile_user = None;
    shell.core.threads.clear();
    shell.core.threads_view.items.clear();
    shell.core.dms.entries.clear();
    shell.core.activity.items.clear();
    shell.core.activity.hydrated.clear();
    shell.core.active_channel = Some("C_META".into());
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace.channels.clear();
    workspace.messages.clear();
    workspace.custom_emoji.clear();
    workspace.users.clear();
    let me = workspace.self_user_id.clone();
    workspace.users.insert(me.clone(), user(&me, "Me"));
    workspace.channels.insert(
        "C_META".into(),
        serde_json::from_value::<Channel>(json!({
            "id": "C_META", "name": "metadata", "is_channel": true, "is_member": true
        }))
        .unwrap(),
    );
    workspace.messages.insert(
        "C_META".into(),
        ChannelMessages {
            loaded: true,
            messages: vec![Message {
                user: Some("U_META".into()),
                ts: Some("1.1".into()),
                text: Some("<@U_MISSING> :meta_emoji: :meta_unknown:".into()),
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    // Old flat flags must not suppress a new account/team's unresolved names.
    shell.core.avatar_profile_hydrated.insert("U_META".into());
    shell
        .core
        .emoji_hydrated
        .insert(("T1".into(), "meta_emoji".into()));
}
fn owner(state: Signal<ShellState>) -> Arc<Owner> {
    let shell = state.peek();
    Arc::new(
        Owner::capture(
            &shell.core,
            shell.core.transport.as_ref().unwrap(),
            &shell.core.session.as_ref().unwrap().workspaces["T1"],
        )
        .unwrap(),
    )
}
fn user(id: &str, name: &str) -> User {
    serde_json::from_value(json!({"id":id,"profile":{"display_name":name,"image_72":"https://example.test/avatar.png"}})).unwrap()
}
fn emoji() -> Emoji {
    serde_json::from_value(json!({"name":"meta_emoji","value":"https://example.test/emoji.png"}))
        .unwrap()
}

#[tokio::test]
async fn held_user_success_cannot_replace_new_token_names_and_omitted_ids_complete() {
    let h = harness();
    let mut state = h.state;
    setup(state);
    let old = owner(state);
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut receive = Some(receive);
    let mut work = Box::pin(users::hydrate_with(state, old, "T1", |ids| {
        assert_eq!(ids, ["U_META", "U_MISSING"]);
        let receive = receive.take().unwrap();
        async move {
            receive.await.unwrap();
            Ok(vec![user("U_META", "Retired")])
        }
    }));
    pending(work.as_mut()).await;
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token
        .push_str("-replacement");
    users::hydrate_with(state, owner(state), "T1", |ids| async move {
        assert_eq!(ids, ["U_META", "U_MISSING"]);
        Ok(vec![user("U_META", "Replacement")])
    })
    .await;
    send.send(()).unwrap();
    work.await;
    assert_eq!(
        state.peek().core.workspaces["T1"].display_name("U_META"),
        "Replacement"
    );
    assert!(
        !state.peek().core.workspaces["T1"]
            .users
            .contains_key("U_MISSING")
    );
    users::hydrate_with(state, owner(state), "T1", |_| async {
        panic!("successful omitted ID must not loop")
    })
    .await;
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}

#[tokio::test]
async fn retired_emoji_error_cannot_release_replacement_claim_or_negative_completion() {
    let h = harness();
    let mut state = h.state;
    setup(state);
    let (old_send, old_receive) = tokio::sync::oneshot::channel();
    let mut old_receive = Some(old_receive);
    let mut old = Box::pin(emojis::hydrate_with(state, owner(state), "T1", |_| {
        let receive = old_receive.take().unwrap();
        async move {
            receive.await.unwrap();
            Err(Error::Offline("synthetic offline".into()))
        }
    }));
    pending(old.as_mut()).await;
    state.write().core.active_account = Some("replacement-same-team".into());
    let (new_send, new_receive) = tokio::sync::oneshot::channel();
    let mut new_receive = Some(new_receive);
    let mut new = Box::pin(emojis::hydrate_with(state, owner(state), "T1", |ids| {
        assert_eq!(ids, ["meta_emoji", "meta_unknown"]);
        let receive = new_receive.take().unwrap();
        async move {
            receive.await.unwrap();
            Ok(vec![emoji()])
        }
    }));
    pending(new.as_mut()).await;
    old_send.send(()).unwrap();
    old.await;
    emojis::hydrate_with(state, owner(state), "T1", |_| async {
        panic!("replacement claims must remain coalesced")
    })
    .await;
    assert!(
        !state.peek().core.workspaces["T1"]
            .custom_emoji
            .contains_key("meta_emoji")
    );
    new_send.send(()).unwrap();
    new.await;
    assert!(
        state.peek().core.workspaces["T1"]
            .custom_emoji
            .contains_key("meta_emoji")
    );
    emojis::hydrate_with(state, owner(state), "T1", |_| async {
        panic!("successful unknown emoji must not loop")
    })
    .await;
}

#[tokio::test]
async fn canceled_user_read_releases_claims_and_current_failure_can_retry_without_losing_rows() {
    let h = harness();
    let state = h.state;
    setup(state);
    let mut work = Box::pin(users::hydrate_with(state, owner(state), "T1", |_| async {
        std::future::pending::<Result<Vec<User>, Error>>().await
    }));
    pending(work.as_mut()).await;
    // Actual overlapping driver coalesces without creating completion facts.
    users::hydrate_with(state, owner(state), "T1", |_| async {
        panic!("duplicate pending read")
    })
    .await;
    drop(work);
    let calls = Rc::new(Cell::new(0));
    users::hydrate_with(state, owner(state), "T1", |_| {
        calls.set(calls.get() + 1);
        async { Err(Error::Offline("synthetic offline".into())) }
    })
    .await;
    assert_eq!(calls.get(), 1);
    assert_eq!(
        state.peek().core.workspaces["T1"].messages["C_META"]
            .messages
            .len(),
        1
    );
    users::hydrate_with(state, owner(state), "T1", |_| {
        calls.set(calls.get() + 1);
        async { Ok(vec![user("U_META", "Retry teammate")]) }
    })
    .await;
    assert_eq!(calls.get(), 2);
    assert_eq!(
        state.peek().core.workspaces["T1"].display_name("U_META"),
        "Retry teammate"
    );
    assert!(
        state
            .peek()
            .core
            .avatar_profile_hydrated
            .contains("U_MISSING")
    );
}

#[tokio::test]
async fn surface_parent_retirement_during_first_stage_prevents_every_followup() {
    let h = harness();
    let mut state = h.state;
    setup(state);
    let calls = Rc::new(Cell::new(0));
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut receive = Some(receive);
    let mut work = Box::pin(super::super::hydrate_surface_with(
        state,
        owner(state),
        |stage| {
            assert_eq!(stage, super::super::SurfaceStage::Activity);
            calls.set(calls.get() + 1);
            let receive = receive.take().unwrap();
            async move {
                receive.await.unwrap();
            }
        },
    ));
    pending(work.as_mut()).await;
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token
        .push_str("-new");
    send.send(()).unwrap();
    work.await;
    assert_eq!(calls.get(), 1);
    // A valid original owner still drives every stage in production order.
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    super::super::hydrate_surface_with(state, owner(state), |stage| {
        seen.borrow_mut().push(stage);
        async {}
    })
    .await;
    use super::super::SurfaceStage::*;
    assert_eq!(
        *seen.borrow(),
        [Activity, Emoji, Channels, Users, Avatars, Media]
    );
}

#[tokio::test]
async fn multi_chunk_users_retry_successful_omissions_after_failure_and_do_not_publish_before_cancellation()
 {
    let h = harness();
    let mut state = h.state;
    setup(state);
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C_META")
        .unwrap()
        .messages[0]
        .text = Some(
        (0..110)
            .map(|i| format!("<@U_BATCH_{i:03}>"))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let calls = Rc::new(std::cell::RefCell::new(Vec::new()));
    users::hydrate_with(state, owner(state), "T1", |ids| {
        calls.borrow_mut().push(ids.clone());
        let index = calls.borrow().len();
        async move {
            if index == 1 {
                Ok(vec![user(&ids[0], "Accepted partial")])
            } else {
                Err(Error::Offline("synthetic".into()))
            }
        }
    })
    .await;
    assert_eq!(
        calls.borrow().iter().map(Vec::len).collect::<Vec<_>>(),
        [100, 11]
    );
    let first = calls.borrow()[0][0].clone();
    let omitted = calls.borrow()[0][1].clone();
    assert_eq!(
        state.peek().core.workspaces["T1"].display_name(&first),
        "Accepted partial"
    );
    assert!(!state.peek().core.avatar_profile_hydrated.contains(&omitted));
    let (send, receive) = tokio::sync::oneshot::channel::<()>();
    let mut receive = Some(receive);
    let phase = Rc::new(Cell::new(0));
    let mut work = Box::pin(users::hydrate_with(state, owner(state), "T1", |ids| {
        phase.set(phase.get() + 1);
        let index = phase.get();
        let receive = if index == 2 {
            Some(receive.take().unwrap())
        } else {
            None
        };
        if index == 1 {
            assert!(ids.contains(&omitted));
        }
        async move {
            if let Some(receive) = receive {
                receive.await.unwrap();
            }
            Ok(vec![user(&ids[0], "Canceled partial")])
        }
    }));
    pending(work.as_mut()).await;
    assert_eq!(phase.get(), 2);
    assert!(
        !state.peek().core.workspaces["T1"]
            .users
            .contains_key(&omitted)
    );
    drop(work);
    drop(send);
    let retry = Rc::new(std::cell::RefCell::new(Vec::new()));
    users::hydrate_with(state, owner(state), "T1", |ids| {
        retry.borrow_mut().extend(ids);
        async { Ok(vec![]) }
    })
    .await;
    assert!(retry.borrow().contains(&omitted));
    assert_eq!(retry.borrow().len(), 110);
    users::hydrate_with(state, owner(state), "T1", |_| async {
        panic!("completed omissions")
    })
    .await;
}

#[tokio::test]
async fn multi_chunk_emoji_complete_only_successes_and_canceled_early_chunks_remain_retryable() {
    let h = harness();
    let mut state = h.state;
    setup(state);
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C_META")
        .unwrap()
        .messages[0]
        .text = Some(
        (0..110)
            .map(|i| format!(":x_meta_{i:03}:"))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let (send, receive) = tokio::sync::oneshot::channel::<()>();
    let mut receive = Some(receive);
    let calls = Rc::new(Cell::new(0));
    let mut canceled = Box::pin(emojis::hydrate_with(state, owner(state), "T1", |ids| {
        calls.set(calls.get() + 1);
        let index = calls.get();
        let receive = if index == 2 {
            Some(receive.take().unwrap())
        } else {
            None
        };
        async move {
            if let Some(receive) = receive {
                receive.await.unwrap();
            }
            Ok(vec![Emoji {
                name: ids[0].clone(),
                value: "https://example.test/emoji.png".into(),
                ..Default::default()
            }])
        }
    }));
    pending(canceled.as_mut()).await;
    assert_eq!(calls.get(), 2);
    assert!(state.peek().core.workspaces["T1"].custom_emoji.is_empty());
    assert!(
        !state
            .peek()
            .core
            .emoji_hydrated
            .contains(&("T1".into(), "x_meta_000".into()))
    );
    drop(canceled);
    drop(send);
    let calls = Rc::new(Cell::new(0));
    emojis::hydrate_with(state, owner(state), "T1", |ids| {
        calls.set(calls.get() + 1);
        let index = calls.get();
        async move {
            if index == 1 {
                assert_eq!(ids.len(), 100);
                Ok(vec![Emoji {
                    name: ids[0].clone(),
                    value: "https://example.test/emoji.png".into(),
                    ..Default::default()
                }])
            } else {
                assert_eq!(ids.len(), 10);
                Err(Error::Offline("synthetic".into()))
            }
        }
    })
    .await;
    assert_eq!(calls.get(), 2);
    assert!(
        state.peek().core.workspaces["T1"]
            .custom_emoji
            .contains_key("x_meta_000")
    );
    let retry = Rc::new(std::cell::RefCell::new(Vec::new()));
    emojis::hydrate_with(state, owner(state), "T1", |ids| {
        retry.borrow_mut().extend(ids);
        async { Ok(vec![]) }
    })
    .await;
    assert_eq!(retry.borrow().len(), 10);
    assert!(
        retry
            .borrow()
            .iter()
            .all(|name| name.as_str() >= "x_meta_100")
    );
    emojis::hydrate_with(state, owner(state), "T1", |_| async {
        panic!("only successful chunks completed")
    })
    .await;
}
