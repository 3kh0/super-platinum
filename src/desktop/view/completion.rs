//! Native composer completion, with account-scoped directory results.
use super::composer::{byte_to_utf16, utf16_to_byte};
use crate::{
    scheduled::Context,
    state::{ComposerTarget, ShellState},
};
use dioxus::{core::Task, prelude::*};
use std::{cell::RefCell, rc::Rc};
use super_platinum_core::{
    composer_completion::{self, Kind, LIMIT, Suggestion, Target, Trigger},
    slack::{api, models::User},
    state::Workspace,
};

#[derive(serde::Deserialize)]
struct Event {
    kind: String,
    seq: u64,
    #[serde(default)]
    text: String,
    #[serde(default)]
    anchor: usize,
    #[serde(default)]
    cursor: usize,
    #[serde(default)]
    token: String,
}
#[derive(Clone, serde::Serialize)]
struct Choice {
    token: String,
    label: String,
    detail: String,
}
impl Choice {
    fn from(s: Suggestion) -> Option<Self> {
        Some(Self {
            token: s.target.token()?,
            label: s.label,
            detail: s.detail,
        })
    }
}
#[derive(Clone, serde::Serialize)]
struct Results {
    seq: u64,
    start: usize,
    end: usize,
    spacer: &'static str,
    choices: Vec<Choice>,
    status: &'static str,
}

fn labels(workspace: &Workspace, text: &str) -> std::collections::HashMap<String, String> {
    let mut labels = std::collections::HashMap::new();
    // Only resolve references present in this draft, never serialize the directory.
    for part in text.split('<').skip(1) {
        let Some((body, _)) = part.split_once('>') else {
            continue;
        };
        let id = body.split('|').next().unwrap_or(body);
        let label = if let Some(user) = id.strip_prefix('@') {
            workspace
                .users
                .get(user)
                .map(|_| format!("@{}", workspace.display_name(user)))
        } else if let Some(channel) = id.strip_prefix('#') {
            workspace
                .channels
                .get(channel)
                .and_then(|c| c.name.as_ref())
                .map(|name| format!("#{name}"))
        } else if let Some(group) = id.strip_prefix("!subteam^") {
            workspace
                .usergroups
                .get(group)
                .map(|g| format!("@{}", g.handle))
        } else {
            None
        };
        if let Some(label) = label {
            labels.insert(format!("<{id}>"), label);
        }
    }
    labels
}

fn remote_choice(workspace: &Workspace, remote: &User) -> Option<Choice> {
    let user = workspace.users.get(&remote.id).unwrap_or(remote);
    if user.deleted {
        return None;
    }
    let name = super_platinum_core::state::display_name(Some(user), &user.id);
    Choice::from(Suggestion {
        label: format!("@{name}"),
        detail: user
            .name
            .as_ref()
            .map(|s| format!("@{s}"))
            .unwrap_or_default(),
        target: Target::User(user.id.clone()),
    })
}

fn merge_remote(choices: &mut Vec<Choice>, workspace: &Workspace, users: &[User], query: &str) {
    let query = query.to_lowercase();
    let rank = |choice: &Choice| {
        let names =
            [&choice.label, &choice.detail].map(|s| s.trim_start_matches('@').to_lowercase());
        (
            !names.iter().any(|s| s == &query),
            !names.iter().any(|s| s.starts_with(&query)),
        )
    };
    choices.sort_by_key(&rank);
    for user in users {
        let Some(choice) = remote_choice(workspace, user) else {
            continue;
        };
        if choices.iter().any(|c| c.token == choice.token) {
            continue;
        }
        let index = choices.partition_point(|c| rank(c) <= rank(&choice));
        if index < LIMIT {
            choices.insert(index, choice);
            choices.truncate(LIMIT);
        }
    }
}

#[component]
pub(super) fn ComposerCompletion(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
    field_id: &'static str,
) -> Element {
    use_future(move || async move {
        let (epoch, destination, initial_labels, workspace) = {
            let shell = state.peek();
            let text = match target {
                ComposerTarget::Channel => &shell.core.composer.text,
                ComposerTarget::Thread => &shell.core.thread_composer.text,
            };
            let labels = shell
                .core
                .active_team
                .as_ref()
                .and_then(|team| shell.core.workspaces.get(team))
                .map(|w| labels(w, text))
                .unwrap_or_default();
            (
                shell.core.account_epoch,
                shell.composer_key(target),
                labels,
                shell.core.active_team.clone().unwrap_or_default(),
            )
        };
        let current = move |shell: &ShellState| {
            shell.core.account_epoch == epoch && shell.composer_key(target) == destination
        };
        let script = format!(
            "{}\nconst fieldId = {}; const labels = {}; const workspace = {};\n{}",
            include_str!("../composer_references.js"),
            serde_json::to_string(field_id).unwrap(),
            serde_json::to_string(&initial_labels).unwrap(),
            serde_json::to_string(&workspace).unwrap(),
            include_str!("../composer_completion.js")
        );
        let mut bridge = document::eval(&script);
        let mut task: Option<Task> = None;
        let remote = Rc::new(RefCell::new((0u64, Vec::<User>::new())));
        while let Ok(event) = bridge.recv::<Event>().await {
            if !current(&state.peek()) {
                break;
            }
            if event.kind == "choose" {
                let user = {
                    let remote = remote.borrow();
                    (remote.0 == event.seq)
                        .then(|| {
                            remote
                                .1
                                .iter()
                                .find(|u| {
                                    Target::User(u.id.clone()).token().as_deref()
                                        == Some(&event.token)
                                })
                                .cloned()
                        })
                        .flatten()
                };
                if let Some(user) = user {
                    let mut shell = state.write();
                    if let Some(team) = shell.core.active_team.clone() {
                        if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                            workspace.users.entry(user.id.clone()).or_insert(user);
                        }
                    }
                }
                continue;
            }
            if let Some(task) = task.take() {
                task.cancel();
            }
            *remote.borrow_mut() = (event.seq, Vec::new());
            if event.kind != "query" {
                continue;
            }
            let trigger = Trigger::at(
                &event.text,
                utf16_to_byte(&event.text, event.anchor),
                utf16_to_byte(&event.text, event.cursor),
            );
            let Some(trigger) = trigger else {
                let _ = bridge.send(Results {
                    seq: event.seq,
                    start: 0,
                    end: 0,
                    spacer: "",
                    choices: vec![],
                    status: "",
                });
                continue;
            };
            let (choices, context, channel_generation, thread_generation) = {
                let shell = state.peek();
                let choices = shell
                    .core
                    .active_team
                    .as_ref()
                    .and_then(|t| shell.core.workspaces.get(t))
                    .map(|ws| {
                        composer_completion::suggestions(ws, &trigger)
                            .into_iter()
                            .filter_map(Choice::from)
                            .collect()
                    })
                    .unwrap_or_default();
                (
                    choices,
                    Context::capture(&shell),
                    shell.channel_generation,
                    shell.thread_generation,
                )
            };
            let lookup = trigger.kind == Kind::Person
                && !trigger.query.is_empty()
                && context.is_some()
                && !crate::fixture::is_fixture();
            let mut results = Results {
                seq: event.seq,
                start: byte_to_utf16(&event.text, trigger.start),
                end: byte_to_utf16(&event.text, trigger.end),
                spacer: if event.text[trigger.end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_whitespace() || ",.;:!?)]}".contains(c))
                {
                    ""
                } else {
                    " "
                },
                choices,
                status: if lookup { "Searching people…" } else { "" },
            };
            if results.choices.is_empty() && !lookup {
                results.status = "No matches";
            }
            let _ = bridge.send(&results);
            if lookup {
                let ctx = context.unwrap();
                let remote = remote.clone();
                let current = current.clone();
                task = Some(spawn(async move {
                    let valid = |shell: &ShellState| {
                        current(shell)
                            && ctx.current(shell)
                            && shell.channel_generation == channel_generation
                            && shell.thread_generation == thread_generation
                    };
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    if !valid(&state.peek()) {
                        return;
                    }
                    let fetched = api::fetch_users_search(
                        &ctx.transport,
                        &ctx.client,
                        &ctx.session,
                        trigger.query.clone(),
                    )
                    .await;
                    let shell = state.peek();
                    if !valid(&shell) {
                        return;
                    }
                    results.status = "";
                    match fetched {
                        Ok(users) => {
                            if let Some(workspace) = shell.core.workspaces.get(&ctx.session.team_id)
                            {
                                merge_remote(
                                    &mut results.choices,
                                    workspace,
                                    &users,
                                    &trigger.query,
                                );
                            }
                            *remote.borrow_mut() = (event.seq, users);
                            if results.choices.is_empty() {
                                results.status = "No matches";
                            }
                        }
                        Err(_) => {
                            results.status = "Directory unavailable. Edit your search to retry."
                        }
                    }
                    let _ = bridge.send(&results);
                }));
            }
        }
        if let Some(task) = task {
            task.cancel();
        }
    });
    rsx! { div {id:"{field_id}-completion",class:"composer-completion",role:"listbox","aria-label":"Mention suggestions",hidden:true} }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{
        config::WorkspaceSession,
        slack::models::{Channel, UserGroup},
    };

    fn workspace() -> Workspace {
        Workspace::from_session(&WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://example.invalid".into(),
            token: String::new(),
            enterprise_id: None,
        })
    }
    #[test]
    fn draft_labels_resolve_only_referenced_identities_including_legacy_tokens() {
        let mut ws = workspace();
        for id in ["U1", "U2", "U3"] {
            ws.users.insert(
                id.into(),
                User {
                    id: id.into(),
                    name: Some("Alex".into()),
                    ..Default::default()
                },
            );
        }
        ws.channels.insert(
            "C1".into(),
            Channel {
                id: "C1".into(),
                name: Some("design".into()),
                ..Default::default()
            },
        );
        ws.usergroups.insert(
            "S1".into(),
            UserGroup {
                id: "S1".into(),
                handle: "helpers".into(),
                ..Default::default()
            },
        );
        let found = labels(
            &ws,
            "<@U1> <@U2|old-name> <#C1> <!subteam^S1> <@MISSING> @Alex <https://example.invalid>",
        );
        assert_eq!(found.len(), 4);
        assert_eq!(found["<@U1>"], "@Alex");
        assert_eq!(found["<@U2>"], "@Alex");
        assert_eq!(found["<#C1>"], "#design");
        assert_eq!(found["<!subteam^S1>"], "@helpers");
        assert!(!found.contains_key("<@U3>"));
    }
    #[test]
    fn remote_candidates_use_current_profiles_and_skip_deleted_or_invalid_identities() {
        let mut ws = workspace();
        let remote = User {
            id: "U9".into(),
            name: Some("old-handle".into()),
            real_name: Some("Old Name".into()),
            ..Default::default()
        };
        assert_eq!(remote_choice(&ws, &remote).unwrap().label, "@Old Name");
        ws.users.insert(
            "U9".into(),
            User {
                id: "U9".into(),
                real_name: Some("New Name".into()),
                ..Default::default()
            },
        );
        assert_eq!(remote_choice(&ws, &remote).unwrap().label, "@New Name");
        assert_eq!(remote_choice(&ws, &remote).unwrap().token, "<@U9>");
        ws.users.get_mut("U9").unwrap().deleted = true;
        assert!(remote_choice(&ws, &remote).is_none());
        assert!(
            remote_choice(
                &ws,
                &User {
                    id: "bad>id".into(),
                    ..Default::default()
                }
            )
            .is_none()
        );
        assert!(remote_choice(&ws, &User::default()).is_none());
    }
    #[test]
    fn exact_remote_match_is_not_hidden_by_eight_cached_prefix_matches() {
        let ws = workspace();
        let mut choices: Vec<_> = (0..8)
            .map(|i| {
                remote_choice(
                    &ws,
                    &User {
                        id: format!("U{i}"),
                        name: Some(format!("alex{i}")),
                        ..Default::default()
                    },
                )
                .unwrap()
            })
            .collect();
        let remote = User {
            id: "U99".into(),
            name: Some("alex".into()),
            ..Default::default()
        };
        merge_remote(&mut choices, &ws, &[remote.clone(), remote], "alex");
        assert_eq!(choices.len(), LIMIT);
        assert_eq!(choices[0].token, "<@U99>");
        assert_eq!(choices[1].token, "<@U0>");
        assert_eq!(choices.iter().filter(|c| c.token == "<@U99>").count(), 1);
    }
}
