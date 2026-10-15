//! Native composer completion, with account-scoped directory results.
use super::composer::{byte_to_utf16, utf16_to_byte};
use crate::{
    scheduled::Context,
    state::{ComposerTarget, ShellState},
};
use dioxus::{core::Task, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use super_platinum_core::{
    composer_completion::{self, Kind, LIMIT, Suggestion, Target, Trigger},
    slack::models::User,
    state::Workspace,
};

mod directory;
#[path = "completion/emoji.rs"]
mod emoji;
use super_platinum_core::composer_emoji::admission::{Gate, Snapshot, Ticket};

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
    #[serde(default = "selection_direction")]
    direction: String,
    #[serde(default)]
    serial: u64,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    catalog: String,
    #[serde(default)]
    choice: String,
    #[serde(default)]
    snapshot: Option<Snapshot>,
    #[serde(default)]
    after: Option<Snapshot>,
}
fn selection_direction() -> String {
    "none".into()
}
impl Event {
    fn ticket(&self) -> Option<Ticket> {
        Some(Ticket {
            serial: self.serial,
            seq: self.seq,
            owner: self.owner.clone(),
            catalog: self.catalog.clone(),
            choice: self.choice.clone(),
            snapshot: self.snapshot.clone()?,
        })
    }
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

pub(super) fn labels(
    workspace: &Workspace,
    text: &str,
) -> std::collections::HashMap<String, String> {
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

#[derive(Clone, PartialEq, Eq)]
struct EmojiOwner {
    scope: String,
    mount: Option<String>,
}
impl EmojiOwner {
    fn capture(shell: &ShellState, target: ComposerTarget) -> Self {
        Self {
            scope: super::composer::field_scope(shell, target),
            mount: super::composer::field_mount_token(shell, target),
        }
    }
    fn current(&self, shell: &ShellState, target: ComposerTarget) -> bool {
        *self == Self::capture(shell, target)
    }
}

#[component]
pub(super) fn ComposerCompletion(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
    field_id: &'static str,
) -> Element {
    let bridge_slot = use_hook(|| Rc::new(RefCell::new(None::<(String, document::Eval)>)));
    let observed = use_hook(|| Rc::new(RefCell::new(None::<String>)));
    let effect_bridge = bridge_slot.clone();
    use_effect(move || {
        let shell = state.read();
        let signature = serde_json::to_string(&(
            super::composer::field_scope(&shell, target),
            super::composer::field_mount_token(&shell, target),
            crate::emoji_catalog::current(&shell).map(|c| c.token.clone()),
            shell
                .core
                .active_team
                .as_ref()
                .and_then(|t| shell.core.workspaces.get(t))
                .map(|w| w.emoji_preferences.revision()),
        ))
        .unwrap();
        let changed = observed
            .borrow_mut()
            .replace(signature.clone())
            .is_some_and(|old| old != signature);
        if changed {
            if let Some((owner, bridge)) = effect_bridge.borrow().as_ref() {
                let _ = bridge.send(serde_json::json!({"kind":"emoji_invalidate","owner":owner}));
            }
        }
    });
    use_future(move || {
        let bridge_slot = bridge_slot.clone();
        async move {
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
            static NEXT_MOUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let scope = super::composer::field_scope(&state.peek(), target);
            let completion_owner = format!(
                "{scope}:mount:{}",
                NEXT_MOUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            let mut mount = None;
            for _ in 0..60 {
                mount = super::composer::field_mount_token(&state.peek(), target);
                if mount.is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            }
            let Some(mount) = mount else {
                return;
            };
            let emoji_owner = EmojiOwner {
                scope,
                mount: Some(mount),
            };
            let emoji_current = move |shell: &ShellState| emoji_owner.current(shell, target);
            let script = format!(
                "{}\n{}\nconst fieldId = {}; const labels = {}; const workspace = {}; const completionOwner = {}; const emojiNativeProbe = {};\n{}",
                include_str!("../composer_references.js"),
                include_str!("completion/emoji_selection_gate.js"),
                serde_json::to_string(field_id).unwrap(),
                serde_json::to_string(&initial_labels).unwrap(),
                serde_json::to_string(&workspace).unwrap(),
                serde_json::to_string(&completion_owner).unwrap(),
                crate::emoji_native::enabled(),
                crate::performance::completion::controller_script()
            );
            let mut bridge = document::eval(&script);
            *bridge_slot.borrow_mut() = Some((completion_owner.clone(), bridge));
            let gate = Rc::new(RefCell::new(Gate::new(completion_owner.clone())));
            let latest = Rc::new(Cell::new(0u64));
            let (queries, mut receiver) = tokio::sync::watch::channel(None::<emoji::Query>);
            let worker_gate = gate.clone();
            let worker_latest = latest.clone();
            let worker_current = emoji_current.clone();
            let emoji_task = spawn(async move {
                let mut worker = None::<emoji::Worker>;
                loop {
                    if let Some(worker) = &mut worker {
                        worker.waiting();
                    }
                    if receiver.changed().await.is_err() {
                        break;
                    }
                    let Some(query) = receiver.borrow_and_update().clone() else {
                        continue;
                    };
                    let calculated = tokio::task::spawn_blocking(move || {
                        let mut worker = worker.unwrap_or_default();
                        let result = worker.calculate(query);
                        (worker, result)
                    })
                    .await;
                    let _result_release = crate::performance::emoji_retirement::scope(7, 0, 0);
                    let Ok((returned, calculated)) = calculated else {
                        break;
                    };
                    worker = Some(returned);
                    let query = &calculated.query;
                    if crate::performance::emoji_lifecycle::enabled() {
                        crate::performance::emoji_lifecycle::record(
                            "worker-returned",
                            serde_json::json!({"seq":query.seq,"owner":query.owner,"snapshot":query.snapshot,"catalog":query.loaded.token,"preferences":query.preferences.revision(),"choices":calculated.results.choices}),
                        );
                    }
                    let shell = state.peek();
                    let revision = shell
                        .core
                        .active_team
                        .as_ref()
                        .and_then(|t| shell.core.workspaces.get(t))
                        .map(|w| w.emoji_preferences.revision());
                    if worker_latest.get() != query.seq
                        || !worker_current(&shell)
                        || !query.loaded.current(&shell)
                        || revision != Some(query.preferences.revision())
                    {
                        continue;
                    }
                    if crate::emoji_native::enabled() {
                        crate::emoji_native::observe(
                            "worker-present",
                            serde_json::json!({"seq":query.seq,"owner":query.owner,"snapshot":query.snapshot,"catalog":query.loaded.token,"preferences":query.preferences.revision(),"choices":calculated.results.choices}),
                        );
                    }
                    {
                        let _gate_release = crate::performance::emoji_retirement::scope(
                            6,
                            0,
                            crate::performance::emoji_retirement::scalar_id(&query.owner),
                        );
                        worker_gate.borrow_mut().present(
                            query.seq,
                            query.snapshot.clone(),
                            query.loaded.clone(),
                            query.loaded.token.clone(),
                            query.preferences.revision(),
                            calculated.choices,
                        );
                    }
                    if crate::performance::emoji_lifecycle::enabled() {
                        crate::performance::emoji_lifecycle::record(
                            "worker-present",
                            serde_json::json!({"seq":query.seq,"owner":query.owner,"catalog":query.loaded.token}),
                        );
                    }
                    let _ = bridge.send(calculated.results);
                }
            });
            let mut task: Option<Task> = None;
            let remote = Rc::new(RefCell::new((0u64, Vec::<User>::new())));
            while let Ok(event) = bridge.recv::<Event>().await {
                if crate::emoji_native::enabled() && event.kind == "emoji_choose" {
                    crate::emoji_native::boundary(
                        "completion-choose",
                        serde_json::json!({"ticket":event.ticket()}),
                    )
                    .await;
                }
                if !current(&state.peek()) {
                    break;
                }
                if matches!(event.kind.as_str(), "emoji_choose" | "emoji_inserted") {
                    let mut sent_answer = false;
                    let admitted = {
                        let shell = state.peek();
                        let loaded = crate::emoji_catalog::current(&shell);
                        let revision = shell
                            .core
                            .active_team
                            .as_ref()
                            .and_then(|t| shell.core.workspaces.get(t))
                            .map(|w| w.emoji_preferences.revision());
                        match (event.ticket(), loaded, revision) {
                            (Some(ticket), Some(loaded), Some(revision))
                                if emoji_current(&shell) =>
                            {
                                if event.kind == "emoji_choose" {
                                    let answer = gate.borrow_mut().admit(
                                        ticket,
                                        &completion_owner,
                                        &loaded,
                                        revision,
                                    );
                                    if crate::emoji_native::enabled() {
                                        crate::emoji_native::observe(
                                            "completion-admission",
                                            serde_json::json!({"answer":answer}),
                                        );
                                    }
                                    if let Some(answer) = answer {
                                        let _ = bridge.send(answer);
                                        sent_answer = true;
                                    }
                                    None
                                } else {
                                    event.after.as_ref().and_then(|after| {
                                        gate.borrow_mut().inserted(
                                            &ticket,
                                            after,
                                            &completion_owner,
                                            &loaded,
                                            revision,
                                        )
                                    })
                                }
                            }
                            _ => {
                                crate::performance::emoji_retirement::synchronous(
                                    6,
                                    0,
                                    crate::performance::emoji_retirement::scalar_id(
                                        &completion_owner,
                                    ),
                                    || gate.borrow_mut().dismiss(),
                                );
                                None
                            }
                        }
                    };
                    if crate::emoji_native::enabled() && event.kind == "emoji_inserted" {
                        crate::emoji_native::observe(
                            "completion-ack",
                            serde_json::json!({"ticket":event.ticket(),"after":event.after,"validated":admitted.is_some(),"model":match target {crate::state::ComposerTarget::Channel=>state.peek().core.composer.text.clone(),crate::state::ComposerTarget::Thread=>state.peek().core.thread_composer.text.clone()}}),
                        );
                    }
                    if let Some(usage) = admitted {
                        dioxus::core::spawn_forever(crate::emoji_preferences::record(state, usage));
                    }
                    if event.kind == "emoji_choose" && !sent_answer {
                        let _ =
                            bridge.send(serde_json::json!({"kind":"emoji_admit","allowed":false,
                        "serial":event.serial,"seq":event.seq,"owner":event.owner,
                        "catalog":event.catalog,"choice":event.choice}));
                    }
                    continue;
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
                latest.set(event.seq);
                crate::performance::emoji_retirement::synchronous(
                    6,
                    0,
                    crate::performance::emoji_retirement::scalar_id(&completion_owner),
                    || gate.borrow_mut().dismiss(),
                );
                let _ = crate::performance::emoji_retirement::synchronous(
                    5,
                    0,
                    crate::performance::emoji_retirement::scalar_id(&completion_owner),
                    || queries.send(None),
                );
                *remote.borrow_mut() = (event.seq, Vec::new());
                if event.kind != "query" {
                    continue;
                }
                let snapshot = Snapshot {
                    text: event.text.clone(),
                    anchor: event.anchor,
                    focus: event.cursor,
                    direction: event.direction.clone(),
                };
                let japanese = state
                    .peek()
                    .core
                    .active_team
                    .as_ref()
                    .and_then(|team| {
                        state
                            .peek()
                            .core
                            .workspaces
                            .get(team)
                            .and_then(|w| w.locale.as_deref())
                            .map(|locale| locale.starts_with("ja"))
                    })
                    .unwrap_or(false);
                if emoji::trigger(&snapshot, japanese).is_some() && emoji_current(&state.peek()) {
                    crate::emoji_preferences::ensure(state);
                    if let Some(loaded) = crate::emoji_catalog::ensure(state) {
                        let preferences = state.peek().core.workspaces[&workspace]
                            .emoji_preferences
                            .clone();
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as f64;
                        let _query_release = crate::performance::emoji_retirement::scope(
                            5,
                            0,
                            crate::performance::emoji_retirement::scalar_id(&completion_owner),
                        );
                        let _ = queries.send(Some(emoji::Query {
                            seq: event.seq,
                            owner: completion_owner.clone(),
                            snapshot,
                            loaded,
                            preferences,
                            japanese,
                            now_ms,
                        }));
                    } else {
                        let _ = bridge.send(Results {
                            seq: event.seq,
                            start: 0,
                            end: 0,
                            spacer: "",
                            choices: vec![],
                            status: "Loading emoji…",
                        });
                    }
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
                    let ctx = Rc::new(context.unwrap());
                    let remote = remote.clone();
                    let current = current.clone();
                    task = Some(spawn(async move {
                        let valid = |shell: &ShellState| {
                            current(shell)
                                && directory::owner_current(shell, &ctx)
                                && shell.channel_generation == channel_generation
                                && shell.thread_generation == thread_generation
                        };
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        if !valid(&state.peek()) {
                            return;
                        }
                        let fetched =
                            directory::fetch(state, ctx.clone(), trigger.query.clone(), &valid)
                                .await;
                        let shell = state.peek();
                        if !valid(&shell) {
                            return;
                        }
                        results.status = "";
                        match fetched {
                            Ok(users) => {
                                if let Some(workspace) =
                                    shell.core.workspaces.get(&ctx.session.team_id)
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
                            Err(status) => results.status = status,
                        }
                        let _ = bridge.send(&results);
                    }));
                }
            }
            if let Some(task) = task {
                task.cancel();
            }
            emoji_task.cancel();
            crate::performance::emoji_retirement::synchronous(
                6,
                0,
                crate::performance::emoji_retirement::scalar_id(&completion_owner),
                || gate.borrow_mut().dismiss(),
            );
            *bridge_slot.borrow_mut() = None;
        }
    });
    rsx! { div {id:"{field_id}-completion",class:"composer-completion",role:"listbox","aria-label":"Composer suggestions",hidden:true} }
}

#[cfg(test)]
mod tests;
