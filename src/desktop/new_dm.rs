//! Recipient selection and guarded navigation for new direct conversations.
pub(crate) mod directory;
use crate::state::{ChannelOpen, MainView, Overlay, ShellState};
use dioxus::prelude::*;
use super_platinum_core::{
    direct_messages::{MAX_RECIPIENTS, Recipients},
    slack::{api, models::Channel},
    state::Workspace,
};

#[derive(Clone, PartialEq)]
pub(crate) struct Person {
    pub id: String,
    pub name: String,
    pub handle: String,
}

pub(crate) fn people(workspace: &Workspace, query: &str, selected: &[Person]) -> Vec<Person> {
    let query = query.trim().trim_start_matches('@').to_lowercase();
    // Keep only the best 30 candidates instead of sorting/cloning the entire directory.
    let mut best = Vec::with_capacity(31);
    for user in workspace.users.values() {
        if user.deleted
            || user.id == workspace.self_user_id
            || selected.iter().any(|p| p.id == user.id)
        {
            continue;
        }
        let name = workspace.display_name(&user.id);
        let handle = user.name.as_deref().unwrap_or_default();
        let folded = name.to_lowercase();
        let handle_folded = handle.to_lowercase();
        let real = user
            .real_name
            .as_deref()
            .or_else(|| user.profile.as_ref().and_then(|p| p.real_name.as_deref()))
            .unwrap_or_default();
        if !query.is_empty()
            && !folded.contains(&query)
            && !handle_folded.contains(&query)
            && !real.to_lowercase().contains(&query)
        {
            continue;
        }
        let rank = (
            folded != query && handle_folded != query,
            !folded.starts_with(&query) && !handle_folded.starts_with(&query),
            folded,
            user.id.clone(),
        );
        let index = best.partition_point(|(key, _)| key < &rank);
        if index < 30 {
            best.insert(
                index,
                (
                    rank,
                    Person {
                        id: user.id.clone(),
                        name,
                        handle: handle.into(),
                    },
                ),
            );
            best.truncate(30);
        }
    }
    best.into_iter().map(|(_, person)| person).collect()
}

#[derive(Default, Clone)]
struct Picker {
    query: String,
    selected: Vec<Person>,
    active: usize,
    pending: bool,
    error: Option<String>,
}
impl Picker {
    fn add(&mut self, person: Person) -> bool {
        if self.pending || self.selected.iter().any(|p| p.id == person.id) {
            return false;
        }
        if self.selected.len() == MAX_RECIPIENTS {
            self.error = Some("Choose up to eight people.".into());
            return false;
        }
        self.selected.push(person);
        self.query.clear();
        self.active = 0;
        self.error = None;
        true
    }
    fn remove(&mut self, id: &str) {
        if !self.pending {
            self.selected.retain(|p| p.id != id);
            self.error = None;
            self.active = 0;
        }
    }
}

pub fn open(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    shell.dismiss_overlay();
    shell.close_profile();
    shell.profile_hover = None;
    shell.overlay = Some(Overlay::NewDm);
}

#[derive(Clone)]
struct Visit {
    forward: Option<String>,
    generation: u64,
    channel: u64,
    thread: u64,
    view: MainView,
}
impl Visit {
    fn capture(shell: &ShellState) -> Self {
        Self {
            forward: (shell.overlay == Some(Overlay::Forward))
                .then(|| shell.forward_target.as_ref().map(|t| t.id.clone()))
                .flatten(),
            generation: shell.palette_generation,
            channel: shell.channel_generation,
            thread: shell.thread_generation,
            view: shell.main_view,
        }
    }
    fn current(&self, shell: &ShellState) -> bool {
        (if let Some(id) = &self.forward {
            shell.overlay == Some(Overlay::Forward)
                && shell.forward_target.as_ref().is_some_and(|t| &t.id == id)
        } else {
            shell.overlay == Some(Overlay::NewDm)
        }) && shell.palette_generation == self.generation
            && shell.channel_generation == self.channel
            && shell.thread_generation == self.thread
            && shell.main_view == self.view
    }
}

fn finish(shell: &mut ShellState, mut channel: Channel, recipients: &Recipients) -> bool {
    if channel.id.is_empty() {
        return false;
    }
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get_mut(t))
    else {
        return false;
    };
    let id = channel.id.clone();
    // Slack may return just an ID. Supply known identity, retaining any existing
    // channel metadata/read state instead of overwriting it with a sparse response.
    if recipients.ids().len() == 1 {
        channel.is_im = true;
        channel.user = Some(recipients.ids()[0].clone());
    } else {
        channel.is_mpim = true;
        channel.is_private = true;
        if channel.name.as_deref().is_none_or(str::is_empty) {
            channel.name = Some(
                recipients
                    .ids()
                    .iter()
                    .map(|u| workspace.display_name(u))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }
    workspace.channels.entry(id.clone()).or_insert(channel);
    let row = crate::channel_vm::channel_vm(workspace, &workspace.channels[&id], &shell.media);
    shell.refresh_from_core();
    if !shell.channels.iter().any(|c| c.id == id) {
        shell.channels.push(row);
    }
    let Some(index) = shell.channels.iter().position(|c| c.id == id) else {
        return false;
    };
    shell.select_channel(index, ChannelOpen::Global);
    shell.dismiss_overlay();
    true
}

fn submit(mut state: Signal<ShellState>, mut picker: Signal<Picker>) {
    if picker.peek().pending || state.peek().overlay != Some(Overlay::NewDm) {
        return;
    }
    let Some(ctx) = crate::scheduled::Context::get(&state) else {
        picker.write().error = Some("Sign in to open this conversation.".into());
        return;
    };
    let ids = picker
        .peek()
        .selected
        .iter()
        .map(|p| p.id.clone())
        .collect::<Vec<_>>();
    let recipients = match Recipients::new(&ids, &ctx.session.user_id) {
        Ok(recipients) => recipients,
        Err(error) => {
            picker.write().error = Some(error.into());
            return;
        }
    };
    let visit = Visit::capture(&state.peek());
    if !visit.current(&state.peek()) {
        return;
    }
    {
        let mut p = picker.write();
        p.pending = true;
        p.error = None;
    }
    // The task belongs to this component and is canceled when its dialog unmounts.
    spawn(async move {
        let posting =
            crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session);
        let result =
            api::open_direct_conversation(&ctx.transport, &ctx.client, &ctx.session, &recipients)
                .await;
        let Ok(mut p) = picker.try_write() else {
            return;
        };
        p.pending = false;
        if !ctx.current(&state.peek()) || !visit.current(&state.peek()) {
            return;
        }
        match result {
            Ok(channel) => {
                let observed = channel.clone();
                let finished = finish(&mut state.write(), channel, &recipients);
                if finished {
                    posting.channels(&state, std::slice::from_ref(&observed));
                    drop(p);
                    dioxus::core::spawn_forever(crate::bootstrap::refresh_selected_channel(state));
                } else {
                    p.error = Some("Slack returned an incomplete conversation. Try again.".into());
                }
            }
            Err(error) => {
                p.error = Some(
                    match error {
                        super_platinum_core::slack::Error::Api(code)
                            if code == "invalid_user_combination" =>
                        {
                            "These external people must share a channel before starting a group DM."
                        }
                        super_platinum_core::slack::Error::Api(code)
                            if matches!(
                                code.as_str(),
                                "missing_scope" | "no_permission" | "restricted_action"
                            ) =>
                        {
                            "Your workspace does not allow this conversation."
                        }
                        super_platinum_core::slack::Error::Offline(_) => {
                            "Connect to Slack, then try again."
                        }
                        _ => "Could not open this conversation. Try again.",
                    }
                    .into(),
                );
            }
        }
    });
}

#[component]
pub fn NewDmDialog(mut state: Signal<ShellState>) -> Element {
    let mut picker = use_signal(|| {
        let mut picker = Picker::default();
        if let Ok(fixture) = std::env::var("SUPER_PLATINUM_FIXTURE") {
            if fixture.starts_with("new-dm-directory") {
                picker.query = if fixture.ends_with("empty") {
                    "zzznomatch"
                } else {
                    "alex"
                }
                .into();
            } else if fixture.starts_with("new-dm-") {
                let shell = state.peek();
                if let Some(workspace) = shell
                    .core
                    .active_team
                    .as_ref()
                    .and_then(|t| shell.core.workspaces.get(t))
                {
                    picker.selected = people(workspace, "", &[]).into_iter().take(3).collect();
                }
                picker.pending = fixture == "new-dm-pending";
                if fixture == "new-dm-error" {
                    picker.error = Some("Could not open this conversation. Try again.".into());
                }
            }
        }
        picker
    });
    let (directory, mut retry) = directory::use_directory(state, picker);
    let matches = use_memo(move || {
        let shell = state.read();
        let p = picker.read();
        let local = shell
            .core
            .active_team
            .as_ref()
            .and_then(|t| shell.core.workspaces.get(t))
            .map(|w| people(w, &p.query, &p.selected))
            .unwrap_or_default();
        directory
            .read()
            .append(&shell, &p.query, &p.selected, local)
    });
    use_effect(move || {
        let active = picker.read().active;
        document::eval(&format!(
            "document.getElementById('new-dm-option-{active}')?.scrollIntoView({{block:'nearest'}});"
        ));
    });
    let p = picker.read().clone();
    let options = matches.read().clone();
    let count = p.selected.len();
    let searching = directory.read().loading;
    let directory_error = directory.read().error;
    rsx! {
        div {class:"scrim", onclick:move |_| state.write().dismiss_overlay()}
        section {class:"modal new-dm-dialog", id:"shell-overlay-dialog", role:"dialog", "aria-modal":"true", "aria-labelledby":"new-dm-title", tabindex:"-1",
            "data-keyboard-dialog":"true", "data-dialog-kind":"other",
            header {
                h2 {id:"new-dm-title", "New message"}
                button {"aria-label":"Close new message", onclick:move |_|state.write().dismiss_overlay(), {crate::icons::icon(crate::icons::Icon::Close,"icon")}}
            }
            div {class:"new-dm-body",
                label {r#for:"new-dm-input", "To"}
                if !p.selected.is_empty() {
                    div {class:"new-dm-selected", "aria-label":"Selected recipients",
                        for person in &p.selected {
                            {let id = person.id.clone(); rsx! {
                                button {key:"{person.id}", class:"new-dm-chip", disabled:p.pending, "aria-label":"Remove {person.name}", onclick:move |_|picker.write().remove(&id),
                                    span {"{person.name}"} {crate::icons::icon(crate::icons::Icon::Close,"icon sm")}
                                }
                            }}
                        }
                    }
                }
                input {id:"new-dm-input", "data-recipient-input":"true", initial_value:"{p.query}", "data-dialog-input":"true", disabled:p.pending,
                    autocomplete:"off", "autocorrect":"off", autocapitalize:"none", spellcheck:"false",
                    placeholder:"Find people by name or handle", role:"combobox", "aria-label":"Find recipients", "aria-autocomplete":"list", "aria-controls":"new-dm-options", "aria-expanded":true,
                    "aria-activedescendant":(!options.is_empty()).then(||format!("new-dm-option-{}",p.active.min(options.len()-1))),
                    oninput:move |event|{let mut p=picker.write();p.query=event.value();p.active=0;p.error=None;},
                    onkeydown:move |event|{ if event.is_composing() { return; } match event.key() {
                        Key::ArrowDown => {event.prevent_default();let len=matches.read().len();let mut p=picker.write();p.active=(p.active+1).min(len.saturating_sub(1));}
                        Key::ArrowUp => {event.prevent_default();let mut p=picker.write();p.active=p.active.saturating_sub(1);}
                        Key::Enter => {event.prevent_default();let person=matches.read().get(picker.peek().active).cloned();if let Some(person)=person {directory::choose(state,picker,directory,person);}}
                        Key::Backspace if picker.peek().query.is_empty() => {let mut p=picker.write();if !p.pending {p.selected.pop();p.error=None;}}
                        _=>{}
                    }}
                }
                div {class:"new-dm-options", id:"new-dm-options", role:"listbox", "aria-label":"People",
                    if options.is_empty() {p {class:"results-empty",
                        if searching {"Searching your workspace…"}
                        else if directory_error.is_none() {"No matching people. Try another name or handle."}
                    }}
                    for (index, person) in options.iter().enumerate() {
                        {let person = person.clone(); rsx! {
                            button {key:"{person.id}", id:"new-dm-option-{index}", "data-recipient-option":"true", class:if index == p.active {"new-dm-option active"} else {"new-dm-option"}, role:"option", "aria-selected":index==p.active, disabled:p.pending || count==MAX_RECIPIENTS, tabindex:"-1",
                                onclick:move |_|directory::choose(state,picker,directory,person.clone()),
                                span {class:"new-dm-person", strong {"{person.name}"}
                                    if !person.handle.is_empty() {span {"@{person.handle}"}}
                                }
                                {crate::icons::icon(crate::icons::Icon::Add,"icon sm")}
                            }
                        }}
                    }
                }
                div {class:"new-dm-directory-status", role:"status", "aria-live":"polite",
                    if let Some(error) = directory_error {
                        span {"{error}"}
                        button {class:"new-dm-retry", disabled:p.pending, onclick:move |_|retry+=1, "Retry search"}
                    } else if searching && !options.is_empty() {span {"Searching your workspace…"}}
                }
                p {class:"new-dm-status", role:"status", "aria-live":"polite",
                    if let Some(error)=&p.error {"{error}"}
                    else if p.pending {"Opening conversation…"}
                    else {"{count} of 8 people selected. You’re included automatically."}
                }
            }
            footer {class:"new-dm-footer",
                button {class:"secondary-btn", onclick:move |_|state.write().dismiss_overlay(), "Cancel"}
                button {class:"primary-btn", disabled:p.pending || count==0, "aria-busy":p.pending, onclick:move |_|submit(state,picker), "Open conversation"}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell() -> ShellState {
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "new-dm")
    }
    fn person(i: usize) -> Person {
        Person {
            id: format!("U{i}"),
            name: format!("Person {i}"),
            handle: format!("person{i}"),
        }
    }
    #[test]
    fn selection_is_unique_bounded_and_frozen_while_opening() {
        let mut p = Picker::default();
        for i in 0..10 {
            p.add(person(i));
        }
        assert_eq!(p.selected.len(), 8);
        assert!(p.error.is_some());
        p.remove("U2");
        p.add(person(1));
        assert_eq!(p.selected.len(), 7);
        p.pending = true;
        p.remove("U1");
        p.add(person(9));
        assert_eq!(p.selected.len(), 7);
        assert!(p.selected.iter().any(|p| p.id == "U1"));
        p.pending = false;
        p.add(person(9));
        assert_eq!(p.selected.len(), 8);
    }
    #[test]
    fn directory_excludes_self_deleted_and_selected_and_ranks_handles() {
        let mut s = shell();
        let ws = s
            .core
            .workspaces
            .get_mut(s.core.active_team.as_ref().unwrap())
            .unwrap();
        ws.users.clear();
        ws.self_user_id = "SELF".into();
        for i in 0..50 {
            ws.users.insert(
                format!("U{i}"),
                super_platinum_core::slack::models::User {
                    id: format!("U{i}"),
                    name: Some(format!("person{i}")),
                    real_name: Some(format!("Person {i}")),
                    deleted: i == 3,
                    ..Default::default()
                },
            );
        }
        ws.users.insert(
            "SELF".into(),
            super_platinum_core::slack::models::User {
                id: "SELF".into(),
                name: Some("person".into()),
                ..Default::default()
            },
        );
        let results = people(ws, "person", &[person(2)]);
        assert_eq!(results.len(), 30);
        assert!(
            !results
                .iter()
                .any(|p| matches!(p.id.as_str(), "SELF" | "U2" | "U3"))
        );
        assert_eq!(people(ws, "@person4", &[])[0].id, "U4");
        assert!(people(ws, "nonexistent", &[]).is_empty());
    }
    #[test]
    fn navigation_away_or_reopened_dialog_invalidates_completion() {
        let mutations: [fn(&mut ShellState); 5] = [
            |s| {
                s.dismiss_overlay();
                s.overlay = Some(Overlay::NewDm);
            },
            |s| s.channel_generation += 1,
            |s| s.thread_generation += 1,
            |s| s.main_view = MainView::Home,
            |s| s.overlay = Some(Overlay::Palette),
        ];
        for mutate in mutations {
            let mut s = shell();
            s.main_view = MainView::Dms;
            let visit = Visit::capture(&s);
            assert!(visit.current(&s));
            mutate(&mut s);
            assert!(!visit.current(&s));
        }
    }
    #[test]
    fn opens_group_in_home_preserves_draft_and_existing_metadata() {
        let mut s = shell();
        let old = s.core.active_channel.clone().unwrap();
        s.core.composer = super_platinum_core::ComposerState::with_text("Unsent group draft");
        let recipients = Recipients::new(&["U1".into(), "U2".into()], "SELF").unwrap();
        assert!(finish(
            &mut s,
            Channel {
                id: "G_NEW".into(),
                ..Default::default()
            },
            &recipients
        ));
        assert_eq!(s.core.active_channel.as_deref(), Some("G_NEW"));
        assert_eq!(s.main_view, MainView::Home);
        assert!(s.overlay.is_none());
        let ws = s
            .core
            .workspaces
            .get_mut(s.core.active_team.as_ref().unwrap())
            .unwrap();
        let channel = ws.channels.get_mut("G_NEW").unwrap();
        assert!(channel.is_mpim);
        assert!(!channel.is_im);
        assert!(channel.name.is_some());
        channel.last_read = Some("123.456".into());
        channel.is_starred = true;
        assert!(finish(
            &mut s,
            Channel {
                id: "G_NEW".into(),
                ..Default::default()
            },
            &recipients
        ));
        let ws = &s.core.workspaces[s.core.active_team.as_ref().unwrap()];
        assert_eq!(ws.channels["G_NEW"].last_read.as_deref(), Some("123.456"));
        assert!(ws.channels["G_NEW"].is_starred);
        let index = s.channels.iter().position(|c| c.id == old).unwrap();
        s.select_channel(index, ChannelOpen::Global);
        assert_eq!(s.core.composer.text, "Unsent group draft");
    }
    #[test]
    fn empty_response_never_changes_selection() {
        let mut s = shell();
        let old = s.core.active_channel.clone();
        assert!(!finish(
            &mut s,
            Channel::default(),
            &Recipients::new(&["U1".into()], "SELF").unwrap()
        ));
        assert_eq!(s.core.active_channel, old);
    }
}
