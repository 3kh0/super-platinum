use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use super_platinum_core::slack::api::{self, ChannelMembershipAction as Action};

#[component]
pub(super) fn MembershipControls(
    mut state: Signal<ShellState>,
    channel: String,
    ready: bool,
    disabled: bool,
    join_only: bool,
    onbusy: EventHandler<bool>,
    onchanged: EventHandler,
) -> Element {
    let mut busy = use_signal(|| false);
    let mut confirming = use_signal(|| {
        matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-leave" | "channel-leave-last")
        )
    });
    let mut error = use_signal(String::new);
    let mut focus = use_signal(|| 0u64);
    let id = format!(
        "channel-membership-{channel}-{}",
        if join_only { "composer" } else { "details" }
    );
    let focus_id = id.clone();
    use_effect(move || {
        if focus() > 0 {
            dioxus::document::eval(&format!(
                "const panel=document.getElementById({}); const target=panel?.querySelector('[data-membership-focus]') ?? document.querySelector('#channel-details-dialog [role=tab][aria-selected=true]') ?? document.getElementById('channel-composer');target?.focus();",
                serde_json::to_string(&focus_id).unwrap()
            ));
        }
    });
    let shell = state.read();
    let Some(info) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
        .and_then(|w| w.channels.get(&channel))
    else {
        return rsx! {};
    };
    let action = api::channel_membership_action(info);
    let private = info.is_private || info.is_group;
    let name = info.name.clone().unwrap_or_else(|| channel.clone());
    let nonmember = info.extra.get("is_member").and_then(|v| v.as_bool()) == Some(false);
    let leave = matches!(action, Some(Action::Leave | Action::Archive));
    let archive = action == Some(Action::Archive);
    if (join_only && leave) || info.is_im || info.is_mpim || info.is_archived {
        return rsx! {};
    }
    let run_channel = channel.clone();
    let run = move |_| {
        let Some(action) = action else {
            return;
        };
        if busy() || disabled || !ready {
            return;
        }
        let target = run_channel.clone();
        let ctx = Context::get(&state);
        if ctx.is_none() && !crate::fixture::is_fixture() {
            error.set("Connect to Slack to change channel membership.".into());
            return;
        }
        busy.set(true);
        onbusy.call(true);
        error.set(String::new());
        dioxus::core::spawn_forever(async move {
            let result = change(state, ctx, &target, action).await;
            let alive = if let Ok(mut value) = busy.try_write() {
                *value = false;
                true
            } else {
                false
            };
            if !alive {
                return;
            }
            onbusy.call(false);
            match result {
                Ok(()) => {
                    confirming.set(false);
                    onchanged.call(());
                }
                Err(message) => error.set(message),
            }
            focus += 1;
        });
    };
    rsx! {
        section{id,class:"channel-membership","aria-label":"Channel membership",
            if !error.read().is_empty(){p{role:"alert",class:"later-error","{error}"}}
            if ready && action.is_some() {
                if leave && confirming() {
                    p{if archive{"Archive #{name}?"}else{"Leave #{name}?"}}
                    p{class:"scheduled-hint",if archive{"You are the last member. The channel will be archived, and no one will be able to send messages. Its history stays available, and you can unarchive it later."}else if private{"You will need an invitation to rejoin this private channel."}else{"You can find and rejoin this public channel later."}}
                    p{class:"scheduled-hint","Your unsent drafts stay on this device."}
                    div{class:"scheduled-actions",
                        button{disabled:busy(),"data-membership-focus":"true",onclick:move |_|{confirming.set(false);error.set(String::new());focus+=1;},"Cancel"}
                        button{disabled:busy()||disabled,onclick:run,if busy(){if archive{"Archiving…"}else{"Leaving…"}}else if archive{"Archive channel"}else{"Leave channel"}}
                    }
                } else if leave {
                    button{class:"channel-archive-action",disabled:disabled||busy(),"data-membership-focus":"true",onclick:move |_|{confirming.set(true);error.set(String::new());focus+=1;},"Leave channel…"}
                } else {
                    button{class:"primary",disabled:disabled||busy(),"data-membership-focus":"true",onclick:run,if busy(){"Joining…"}else{"Join channel"}}
                }
            } else if ready && private && nonmember {
                p{class:"scheduled-hint","Ask a channel member for an invitation to rejoin."}
            }
        }
    }
}

async fn change(
    mut state: Signal<ShellState>,
    ctx: Option<Context>,
    channel: &str,
    action: Action,
) -> Result<(), String> {
    if crate::fixture::is_fixture() {
        let mut shell = state.write();
        let team = shell.core.active_team.clone().unwrap();
        let workspace = shell.core.workspaces.get_mut(&team).unwrap();
        let revision = workspace.channel_lifecycle.revision();
        if action == Action::Archive {
            workspace.confirm_channel_archive(channel, true, revision);
        } else {
            workspace.confirm_channel_membership(channel, action.member(), revision);
        }
        shell.refresh_from_core();
        return Ok(());
    }
    let ctx = ctx.ok_or_else(|| "Connect to Slack to change channel membership.".to_owned())?;
    if !ctx.current(&state.peek()) {
        return Err("The active account changed.".into());
    }
    let revision = state.peek().core.workspaces[&ctx.session.team_id]
        .channel_lifecycle
        .revision();
    let posting = crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session)
        .for_channels([&channel]);
    let fresh = api::fetch_conversation_details(&ctx.transport, &ctx.client, &ctx.session, channel)
        .await
        .map_err(|e| super::channel_details::failure(&e))?;
    if !ctx.current(&state.peek()) {
        return Err("The active account changed.".into());
    }
    posting.channels(&state, std::slice::from_ref(&fresh));
    let info = {
        let mut shell = state.write();
        let info = shell
            .core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .unwrap()
            .merge_channel_details(fresh, revision);
        shell.refresh_from_core();
        info
    };
    // The preflight can discover a remote join/leave while a confirmation was open.
    if api::channel_membership_action(&info) != Some(action) {
        return Err(
            "Channel membership or permissions changed. Review the channel before continuing."
                .into(),
        );
    }
    let revision = state.peek().core.workspaces[&ctx.session.team_id]
        .channel_lifecycle
        .revision();
    let posting = crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session)
        .for_channels([&channel]);
    let joined = api::change_conversation_membership(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        channel,
        action,
    )
    .await
    .map_err(|e| super::channel_details::failure(&e))?;
    if !ctx.current(&state.peek()) {
        return Ok(());
    }
    if let Some(info) = &joined {
        posting.channels(&state, std::slice::from_ref(info));
    }
    {
        let mut shell = state.write();
        let workspace = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
        if let Some(info) = joined {
            workspace.merge_channel_details(info, revision);
        }
        if action == Action::Archive {
            workspace.confirm_channel_archive(channel, true, revision);
        } else {
            workspace.confirm_channel_membership(channel, action.member(), revision);
        }
        shell.refresh_from_core();
        shell.show_toast(if action == Action::Archive {
            "Channel archived."
        } else if action.member() {
            "Channel joined."
        } else {
            "Channel left. Your unsent drafts are kept on this device."
        });
    }
    crate::bootstrap::persist_workspace(&state, &ctx.session.team_id);
    if action.member() && state.peek().core.active_channel.as_deref() == Some(channel) {
        crate::bootstrap::refresh_selected_channel(state).await;
    }
    Ok(())
}
