//! Reply notification controls; writes survive closing their originating pane.
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::{domain::ThreadKey, slack::api};

pub(crate) fn active(shell: &ShellState) -> Option<bool> {
    let key = crate::thread_unread::key(shell)?;
    active_for(shell, &key)
}
fn active_for(shell: &ShellState, key: &ThreadKey) -> Option<bool> {
    shell
        .core
        .thread_subscriptions
        .active(shell.core.account_epoch, key)
        .or_else(|| {
            shell
                .core
                .threads
                .get(key)?
                .messages
                .iter()
                .find(|m| m.ts.as_ref() == Some(&key.2))?
                .extra
                .get("subscribed")?
                .as_bool()
        })
}
pub(crate) fn busy(shell: &ShellState) -> bool {
    crate::thread_unread::key(shell).is_some_and(|key| {
        shell
            .core
            .thread_subscriptions
            .pending(shell.core.account_epoch, &key)
    })
}

pub(crate) async fn refresh(mut state: Signal<ShellState>, key: ThreadKey) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
        return;
    };
    let Some(session) = sessions.into_iter().find(|s| s.team_id == key.0) else {
        return;
    };
    let (epoch, revision) = {
        let shell = state.peek();
        (
            shell.core.account_epoch,
            shell
                .core
                .thread_subscriptions
                .revision(shell.core.account_epoch, &key),
        )
    };
    let result =
        api::fetch_thread_subscription(&transport, &client, &session, key.1.clone(), key.2.clone())
            .await;
    let mut shell = state.write();
    if shell.core.account_epoch != epoch
        || !shell
            .core
            .transport
            .as_ref()
            .is_some_and(|t| std::sync::Arc::ptr_eq(t, &transport))
    {
        return;
    }
    match result {
        Ok(value) => {
            if shell
                .core
                .thread_subscriptions
                .snapshot(epoch, &key, value, revision)
            {
                shell.core.reconcile_thread_subscription(&key);
            }
        }
        Err(error) => {
            if crate::thread_unread::key(&shell).as_ref() == Some(&key) {
                shell.report_failure(&error, "Reply notifications need a connection", || {
                    "Could not check reply notifications. Try the bell again.".into()
                });
            }
        }
    }
}

pub(crate) async fn toggle(mut state: Signal<ShellState>, key: ThreadKey, expected_epoch: u64) {
    if state.peek().core.account_epoch != expected_epoch {
        return;
    }
    let Some(was_active) = active_for(&state.peek(), &key) else {
        refresh(state, key).await;
        return;
    };
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
        return;
    };
    let Some(session) = sessions.into_iter().find(|s| s.team_id == key.0) else {
        return;
    };
    let (epoch, change, cursor) = {
        let mut shell = state.write();
        let epoch = shell.core.account_epoch;
        let Some(change) = shell
            .core
            .thread_subscriptions
            .begin(epoch, &key, !was_active)
        else {
            return;
        };
        (
            epoch,
            change,
            shell.core.thread_cursor(&key).map(str::to_owned),
        )
    };
    let result = api::set_thread_subscription(
        &transport,
        &client,
        &session,
        key.1.clone(),
        key.2.clone(),
        change.active,
        cursor,
    )
    .await;
    let mut shell = state.write();
    if shell.core.account_epoch != epoch
        || !shell
            .core
            .transport
            .as_ref()
            .is_some_and(|t| std::sync::Arc::ptr_eq(t, &transport))
    {
        return;
    }
    if !shell
        .core
        .thread_subscriptions
        .finish(epoch, &key, &change, result.is_ok())
    {
        return;
    }
    shell.core.reconcile_thread_subscription(&key);
    if let Err(error) = result {
        shell.report_failure(&error, "Reply notifications need a connection", || {
            "Could not change reply notifications. Please try again.".into()
        });
    }
}

pub(crate) fn control(state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let key = crate::thread_unread::key(snapshot);
    let epoch = snapshot.core.account_epoch;
    let active = active(snapshot);
    let busy = busy(snapshot);
    let label = if busy {
        "Updating reply notifications…"
    } else {
        match active {
            Some(true) => "Unfollow thread",
            Some(false) => "Follow thread",
            None => "Check reply notifications",
        }
    };
    rsx! {button {
        class:if active==Some(true) {"icon-btn thread-follow active"} else {"icon-btn thread-follow"},
        title:label, "aria-label":label, "aria-pressed":if active==Some(true) {"true"} else {"false"},
        disabled:busy || snapshot.thread_loading,
        onclick:move |_| {if let Some(key)=key.clone() {dioxus::core::spawn_forever(toggle(state,key,epoch));}},
        {crate::icons::icon(crate::icons::Icon::Notifications,"icon")}
    }}
}

pub(crate) fn fixture(shell: &mut ShellState, name: &str) {
    crate::thread_unread::fixture(shell);
    let key = crate::thread_unread::key(shell).unwrap();
    let epoch = shell.core.account_epoch;
    shell
        .core
        .thread_subscriptions
        .apply(epoch, &key, name != "thread-follow-off");
    if name == "thread-follow-pending" {
        shell.core.thread_subscriptions.begin(epoch, &key, false);
    }
}
