use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use super_platinum_core::slack::{
    Error,
    api::{self, ChannelMemberAction as Action, ChannelMemberResult},
    models::User,
};

#[derive(Clone, PartialEq)]
struct Person {
    id: String,
    name: String,
    detail: String,
}

fn person(user: &User) -> Person {
    Person {
        id: user.id.clone(),
        name: super_platinum_core::state::display_name(Some(user), &user.id),
        detail: user.name.clone().unwrap_or_else(|| user.id.clone()),
    }
}

fn eligible(user: &User, own: &str, members: &[String]) -> bool {
    !user.id.is_empty()
        && user.id != own
        && !user.deleted
        && !user.is_bot
        && !user.is_stranger
        && !members.contains(&user.id)
}

#[component]
pub(super) fn AddMembers(
    mut state: Signal<ShellState>,
    channel: String,
    private: bool,
    disabled: bool,
    members: Vec<String>,
    onbusy: EventHandler<bool>,
    onchanged: EventHandler<(String, bool)>,
    onopen: EventHandler<bool>,
) -> Element {
    let mut opened = use_signal(|| fixture("member-add") || fixture("member-add-error"));
    use_effect(move || onopen.call(opened()));
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<Person>::new);
    let mut selected = use_signal(|| {
        if fixture("member-add-error") {
            vec![Person {
                id: "U2".into(),
                name: "Jules".into(),
                detail: "jules".into(),
            }]
        } else {
            vec![]
        }
    });
    let mut errors = use_signal(|| {
        if fixture("member-add-error") {
            vec![(
                "Jules".into(),
                "Workspace policy does not allow adding this person.".into(),
            )]
        } else {
            Vec::<(String, String)>::new()
        }
    });
    let mut search_error = use_signal(String::new);
    let mut searching = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut status = use_signal(String::new);
    let mut focus = use_signal(|| 0u64);
    use_effect(move || {
        if focus() > 0 {
            dioxus::document::eval(
                "(document.getElementById('member-add-search') ?? document.getElementById('member-add-trigger'))?.focus();",
            );
        }
    });
    let excluded = members.clone();
    use_effect(move || {
        let value = query();
        results.set(vec![]);
        search_error.set(String::new());
        if value.trim().chars().count() < 2 {
            searching.set(false);
            return;
        }
        searching.set(true);
        let excluded = excluded.clone();
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if *query.peek() != value {
                return;
            }
            let found = search(state, &value, &excluded).await;
            if *query.peek() != value {
                return;
            }
            searching.set(false);
            match found {
                Ok(people) => results.set(people),
                Err(error) => search_error.set(error),
            }
        });
    });
    let target = channel.clone();
    let run = move |_| {
        if busy() || disabled || selected.peek().is_empty() {
            return;
        }
        let people = selected.peek().clone();
        let target = target.clone();
        let ctx = Context::get(&state);
        busy.set(true);
        onbusy.call(true);
        errors.set(vec![]);
        status.set(String::new());
        dioxus::core::spawn_forever(async move {
            let mut confirmed = Vec::new();
            let mut added = 0;
            let mut failures = Vec::new();
            for user in &people {
                match change(state, ctx.as_ref(), &target, &user.id, Action::Add).await {
                    Ok(result) => {
                        confirmed.push(user.id.clone());
                        if result == ChannelMemberResult::Changed {
                            added += 1;
                        }
                    }
                    Err(error) => {
                        let stop = stop_batch(&error);
                        failures.push((user.name.clone(), failure(&error)));
                        if stop {
                            break;
                        }
                    }
                }
            }
            refresh_info(state, ctx.as_ref(), &target).await;
            let Ok(mut active) = busy.try_write() else {
                return;
            };
            *active = false;
            drop(active);
            onbusy.call(false);
            for id in &confirmed {
                onchanged.call((id.clone(), true));
            }
            selected
                .write()
                .retain(|user| !confirmed.contains(&user.id));
            let mut summary = Vec::new();
            if added > 0 {
                summary.push(format!(
                    "Added {added} {}.",
                    if added == 1 { "person" } else { "people" }
                ));
            }
            let existing = confirmed.len() - added;
            if existing > 0 {
                summary.push(format!(
                    "{existing} {} already in the channel.",
                    if existing == 1 {
                        "person was"
                    } else {
                        "people were"
                    }
                ));
            }
            status.set(summary.join(" "));
            errors.set(failures);
            if selected.peek().is_empty() {
                opened.set(false);
                query.set(String::new());
            }
            focus += 1;
        });
    };
    rsx! {
        section{class:"member-admin","aria-label":"Add channel members",
            if !opened(){button{id:"member-add-trigger",class:"member-admin-button",disabled,onclick:move |_|{opened.set(true);errors.set(vec![]);focus+=1;},"Add people…"}}
            if !status.read().is_empty(){p{role:"status",class:"scheduled-hint","{status}"}}
            if opened(){
                h3{"Add people"}
                if private{p{class:"scheduled-hint","People you add can read this private channel’s history and shared files."}}
                label{r#for:"member-add-search","Find people in your workspace"}
                input{id:"member-add-search",class:"list-find",r#type:"search",placeholder:"Search by name",value:"{query}",disabled:busy()||disabled,onmounted:move |_|{dioxus::document::eval("document.getElementById('member-add-search')?.focus();");},oninput:move |event|query.set(event.value())}
                if !selected.read().is_empty(){div{class:"member-selection","aria-label":"Selected people",
                    for user in selected.read().iter(){
                        {let id=user.id.clone();rsx!{button{disabled:busy()||disabled,"aria-label":"Deselect {user.name}",onclick:move |_|selected.write().retain(|person|person.id!=id),"{user.name}",{crate::icons::icon(crate::icons::Icon::Close,"icon sm")}}}}
                    }
                }}
                if searching(){p{role:"status",class:"scheduled-hint","Searching…"}}
                if !search_error.read().is_empty(){p{role:"alert",class:"later-error","{search_error}"}}
                div{class:"member-search-results",
                    for user in results.read().iter().filter(|user| !members.contains(&user.id) && !selected.read().iter().any(|chosen|chosen.id==user.id)){
                        {let choice=user.clone();rsx!{button{class:"member-search-result",disabled:busy()||disabled,onclick:move |_|{if !selected.peek().iter().any(|person|person.id==choice.id){selected.write().push(choice.clone());}query.set(String::new());focus+=1;},span{"{user.name}"}small{"{user.detail}"}}}}
                    }
                }
                if !searching()&&query.read().trim().chars().count()>=2&&results.read().is_empty()&&search_error.read().is_empty(){p{class:"scheduled-hint","No people found. Try another name."}}
                for (name,message) in errors.read().iter(){p{role:"alert",class:"later-error","{name}: {message}"}}
                if !errors.read().is_empty(){p{class:"scheduled-hint","People who were not added remain selected. Review the results before trying again."}}
                div{class:"scheduled-actions",
                    button{disabled:busy(),onclick:move |_|{opened.set(false);query.set(String::new());selected.set(vec![]);errors.set(vec![]);focus+=1;},"Cancel"}
                    button{class:"primary",disabled:busy()||disabled||selected.read().is_empty(),onclick:run,if busy(){"Adding…"}else if selected.read().is_empty(){"Add people"}else if selected.read().len()==1{"Add 1 person"}else{"Add {selected.read().len()} people"}}
                }
            }
        }
    }
}

#[component]
pub(super) fn RemoveMember(
    state: Signal<ShellState>,
    channel: String,
    user: String,
    name: String,
    private: bool,
    disabled: bool,
    onbusy: EventHandler<bool>,
    onchanged: EventHandler<(String, bool)>,
) -> Element {
    let mut confirming = use_signal(|| fixture("member-remove") && user == "U1");
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut focus = use_signal(|| 0u64);
    let id = format!("member-remove-{user}");
    let focus_id = id.clone();
    use_effect(move || {
        if focus() > 0 {
            dioxus::document::eval(&format!(
                "document.getElementById({})?.querySelector('button')?.focus();",
                serde_json::to_string(&focus_id).unwrap()
            ));
        }
    });
    let run = move |_| {
        if busy() || disabled {
            return;
        }
        let target = channel.clone();
        let user = user.clone();
        let ctx = Context::get(&state);
        busy.set(true);
        onbusy.call(true);
        error.set(String::new());
        dioxus::core::spawn_forever(async move {
            let result = change(state, ctx.as_ref(), &target, &user, Action::Remove).await;
            refresh_info(state, ctx.as_ref(), &target).await;
            let Ok(mut active) = busy.try_write() else {
                return;
            };
            *active = false;
            drop(active);
            onbusy.call(false);
            match result {
                Ok(_) => {
                    onchanged.call((user, false));
                    dioxus::document::eval(
                        "document.querySelector('#channel-details-dialog [role=tab][aria-selected=true]')?.focus();",
                    );
                }
                Err(e) => {
                    error.set(failure(&e));
                    focus += 1;
                }
            }
        });
    };
    rsx! {div{id,class:"member-remove",
        if confirming(){
            div{class:"member-remove-confirm",role:"group","aria-label":"Confirm member removal",
                p{"Remove {name} from this channel?"}
                p{class:"scheduled-hint",if private{"They will need a new invitation to rejoin."}else{"They can find and rejoin this public channel."}}
                if !error.read().is_empty(){p{role:"alert",class:"later-error","{error}"}}
                div{class:"scheduled-actions",
                    button{disabled:busy(),onclick:move |_|{confirming.set(false);error.set(String::new());focus+=1;},"Cancel"}
                    button{disabled:busy()||disabled,onclick:run,if busy(){"Removing…"}else{"Remove from channel"}}
                }
            }
        }else{button{class:"member-remove-button",disabled,"aria-label":"Remove {name} from channel",onclick:move |_|{confirming.set(true);focus+=1;},"Remove…"}}
    }}
}

async fn search(
    mut state: Signal<ShellState>,
    query: &str,
    members: &[String],
) -> Result<Vec<Person>, String> {
    if crate::fixture::is_fixture() {
        let shell = state.peek();
        let workspace = &shell.core.workspaces["T1"];
        let query = query.to_lowercase();
        return Ok(workspace
            .users
            .values()
            .filter(|u| eligible(u, &workspace.self_user_id, members))
            .map(person)
            .filter(|u| {
                u.name.to_lowercase().contains(&query) || u.detail.to_lowercase().contains(&query)
            })
            .take(30)
            .collect());
    }
    let ctx = Context::get(&state).ok_or("Connect to Slack to find people.")?;
    let users = api::fetch_users_search(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        query.trim().into(),
    )
    .await
    .map_err(|e| failure(&e))?;
    if !ctx.current(&state.peek()) {
        return Err("The active account changed.".into());
    }
    let people = users
        .iter()
        .filter(|u| eligible(u, &ctx.session.user_id, members))
        .map(person)
        .collect();
    if let Some(workspace) = state.write().core.workspaces.get_mut(&ctx.session.team_id) {
        for user in users {
            workspace.users.insert(user.id.clone(), user);
        }
    }
    Ok(people)
}

async fn change(
    mut state: Signal<ShellState>,
    ctx: Option<&Context>,
    channel: &str,
    user: &str,
    action: Action,
) -> Result<ChannelMemberResult, Error> {
    if crate::fixture::is_fixture() {
        if fixture("member-add-error") {
            return Err(Error::Api("restricted_action".into()));
        }
        return Ok(ChannelMemberResult::Changed);
    }
    let ctx = ctx.ok_or_else(|| Error::Offline("no active session".into()))?;
    if !ctx.current(&state.peek()) {
        return Err(Error::Api("account_changed".into()));
    }
    let revision = state.peek().core.workspaces[&ctx.session.team_id]
        .channel_lifecycle
        .revision();
    let info =
        api::fetch_conversation_details(&ctx.transport, &ctx.client, &ctx.session, channel).await?;
    if !ctx.current(&state.peek()) {
        return Err(Error::Api("account_changed".into()));
    }
    let info = {
        let mut shell = state.write();
        let info = shell
            .core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .unwrap()
            .merge_channel_details(info, revision);
        shell.refresh_from_core();
        info
    };
    if !api::channel_members_manageable(&info) {
        return Err(Error::Api(
            if info.is_archived {
                "is_archived"
            } else {
                "not_in_channel"
            }
            .into(),
        ));
    }
    if action == Action::Remove
        && info.extra.get("is_general").and_then(|v| v.as_bool()) == Some(true)
    {
        return Err(Error::Api("cant_kick_from_general".into()));
    }
    api::change_conversation_member(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        channel,
        user,
        action,
    )
    .await
}

async fn refresh_info(mut state: Signal<ShellState>, ctx: Option<&Context>, channel: &str) {
    let Some(ctx) = ctx.filter(|ctx| ctx.current(&state.peek())) else {
        return;
    };
    let revision = state.peek().core.workspaces[&ctx.session.team_id]
        .channel_lifecycle
        .revision();
    if let Ok(info) =
        api::fetch_conversation_details(&ctx.transport, &ctx.client, &ctx.session, channel).await
    {
        if !ctx.current(&state.peek()) {
            return;
        }
        {
            let mut shell = state.write();
            shell
                .core
                .workspaces
                .get_mut(&ctx.session.team_id)
                .unwrap()
                .merge_channel_details(info, revision);
            shell.refresh_from_core();
        }
        crate::bootstrap::persist_workspace(&state, &ctx.session.team_id);
    }
}

fn stop_batch(error: &Error) -> bool {
    !matches!(error,Error::Api(code) if matches!(code.as_str(),"user_not_found"|"user_is_restricted"|"cant_invite"|"cant_invite_self"|"user_disabled"))
}
fn failure(error: &Error) -> String {
    match error {
        Error::Api(code) => match code.as_str() {
            "account_changed" => {
                "The active account changed. Remaining people were not added.".into()
            }
            "restricted_action" | "cant_invite" | "user_is_restricted" => {
                "Workspace policy does not allow this membership change.".into()
            }
            "cant_kick_from_general" => {
                "Slack does not allow removing members from the general channel.".into()
            }
            "cant_kick_self" => "Use Leave channel in About to leave this channel.".into(),
            "cant_invite_self" => "You are already a member of this channel.".into(),
            "user_not_found" | "user_disabled" => {
                "This person is no longer available in the workspace.".into()
            }
            "not_in_channel" => {
                "Channel membership changed. Reload members before trying again.".into()
            }
            _ => super::channel_details::failure(error),
        },
        _ => super::channel_details::failure(error),
    }
}
fn fixture(name: &str) -> bool {
    std::env::var("SUPER_PLATINUM_FIXTURE")
        .is_ok_and(|value| value == name || value == format!("{name}-narrow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_excludes_self_members_and_unavailable_identities() {
        let user = |value| serde_json::from_value::<User>(value).unwrap();
        for value in [
            serde_json::json!({"id":"U0"}),
            serde_json::json!({"id":"U1"}),
            serde_json::json!({"id":"U2","deleted":true}),
            serde_json::json!({"id":"U2","is_bot":true}),
            serde_json::json!({"id":"U2","is_stranger":true}),
            serde_json::json!({"id":""}),
        ] {
            assert!(!eligible(&user(value), "U0", &["U1".into()]));
        }
        assert!(eligible(
            &user(serde_json::json!({"id":"U2"})),
            "U0",
            &["U1".into()]
        ));
    }

    #[test]
    fn batch_stops_on_uncertain_global_or_rate_failures_but_can_skip_invalid_people() {
        for code in [
            "restricted_action",
            "not_in_channel",
            "is_archived",
            "account_changed",
            "internal_error",
        ] {
            assert!(stop_batch(&Error::Api(code.into())));
        }
        assert!(stop_batch(&Error::Offline("fixture".into())));
        assert!(stop_batch(&Error::Transport("fixture".into())));
        for code in ["user_not_found", "user_disabled", "cant_invite"] {
            assert!(!stop_batch(&Error::Api(code.into())));
        }
    }
}
