//! Message forwarding and copying links, with account-scoped async work.
use crate::state::{MessageVm, Overlay, ShellState};
use dioxus::prelude::*;
use std::sync::Arc;
pub(crate) mod check;
mod delivery;
pub(crate) mod inventory;
mod link_check;
mod people;
mod recovery;
mod recovery_check;
pub(crate) mod scheduled;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport, api},
};

#[derive(Debug, Clone)]
pub struct ForwardTarget {
    pub recovery_only: bool,
    pub epoch: u64,
    pub team: String,
    pub channel: String,
    pub message: MessageVm,
    pub id: String,
}
pub fn open(mut state: Signal<ShellState>, channel: String, message: MessageVm) {
    let mut shell = state.write();
    let Some(team) = shell.core.active_team.clone() else {
        return;
    };
    let id = api::ForwardMessage::new(channel.clone(), message.ts.clone(), String::new(), vec![])
        .client_msg_id;
    shell.forward_target = Some(ForwardTarget {
        recovery_only: false,
        epoch: shell.core.account_epoch,
        team,
        channel,
        message,
        id,
    });
    shell.overlay = Some(Overlay::Forward);
}
struct Context {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    epoch: u64,
}
impl Context {
    fn get(state: &Signal<ShellState>, target: &ForwardTarget) -> Option<Self> {
        let (transport, client, sessions) = crate::bootstrap::credentials(state)?;
        let shell = state.peek();
        if shell.core.account_epoch != target.epoch
            || shell.core.active_team.as_ref() != Some(&target.team)
        {
            return None;
        }
        Some(Self {
            transport,
            client,
            session: sessions.into_iter().find(|s| s.team_id == target.team)?,
            epoch: target.epoch,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.session.team_id)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
    }
}
#[derive(Clone, PartialEq)]
struct Destination {
    id: String,
    name: String,
    private: bool,
    dm: bool,
    person: Option<crate::new_dm::Person>,
}
fn destinations(shell: &ShellState, query: &str, selected: &[Destination]) -> Vec<Destination> {
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
    else {
        return vec![];
    };
    let only_channels = query.trim_start().starts_with('#');
    let query = query.trim().trim_start_matches('#').to_lowercase();
    let mut result = workspace
        .channels
        .values()
        .filter(|c| {
            !c.is_archived
                && (!only_channels || !(c.is_im || c.is_mpim))
                && !selected.iter().any(|d| d.id == c.id)
        })
        .filter_map(|c| {
            let name = super_platinum_core::state::channel_display_name(workspace, c);
            name.to_lowercase().contains(&query).then(|| Destination {
                id: c.id.clone(),
                name,
                private: c.is_private || c.is_group,
                dm: c.is_im || c.is_mpim,
                person: None,
            })
        })
        .collect::<Vec<_>>();
    result.sort_by_cached_key(|d| {
        let name = d.name.to_lowercase();
        let recency = workspace
            .recent_channels
            .iter()
            .position(|c| c == &d.id)
            .unwrap_or(usize::MAX);
        (
            !query.is_empty() && name != query,
            !query.is_empty() && !name.starts_with(&query),
            recency,
            name,
        )
    });
    result.truncate(30);
    result
}
fn same_dialog(shell: &ShellState, target: &ForwardTarget) -> bool {
    shell.overlay == Some(Overlay::Forward)
        && shell.forward_target.as_ref().is_some_and(|t| {
            t.id == target.id
                && t.epoch == shell.core.account_epoch
                && shell.core.active_team.as_ref() == Some(&t.team)
        })
}
fn close(mut state: Signal<ShellState>) {
    dioxus::document::eval("window.superPlatinumRichEditor?.restoreFocus('forward-note');");
    let mut shell = state.write();
    shell.overlay = None;
    shell.forward_target = None;
}

#[component]
pub fn ForwardDialog(mut state: Signal<ShellState>) -> Element {
    let target = use_hook(|| state.peek().forward_target.clone()).unwrap();
    let editor_visit = use_hook(scheduled::next_editor_visit);
    let mut query = use_signal(|| {
        if std::env::var("SUPER_PLATINUM_FIXTURE").is_ok_and(|s| s.starts_with("forward-people")) {
            "alex".into()
        } else {
            String::new()
        }
    });
    let mut selected = use_signal(move || {
        if scheduled::layout_fixture() {
            destinations(&state.peek(), "", &[])
                .into_iter()
                .filter(|d| d.id == "C2")
                .take(1)
                .collect()
        } else {
            vec![]
        }
    });
    let mut active = use_signal(|| 0usize);
    let mut sending = use_signal(|| false);
    let mut copying = use_signal(|| false);
    let mut error = use_signal(|| {
        if std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("forward-error") {
            "The destination is unavailable. Choose another conversation.".into()
        } else {
            String::new()
        }
    });
    use_effect(move || {
        if !error.read().is_empty() {
            document::eval(
                "requestAnimationFrame(()=>document.querySelector('#forward-dialog .forward-error[role=alert]')?.scrollIntoView({block:'nearest'}));",
            );
        }
    });
    let deliveries = use_signal(delivery::fixture);
    let mut saved = use_signal(recovery::Saved::fixture);
    let load_target = target.clone();
    use_future(move || recovery::load(state, load_target.clone(), saved, deliveries, error));
    let open_retry = use_signal(|| 0u64);
    let directory_query = use_memo(move || {
        let query = query.read();
        if sending() || query.trim_start().starts_with('#') {
            String::new()
        } else {
            query.trim().trim_start_matches('@').to_lowercase()
        }
    });
    let (directory, mut retry) = crate::new_dm::directory::use_search(state, directory_query);
    let matches =
        use_memo(move || {
            let ledger = deliveries.read();
            let mut excluded = selected();
            excluded.extend(ledger.0.iter().filter(|(id, _)| ledger.excluded(id)).map(
                |(id, d)| Destination {
                    id: id.clone(),
                    name: d.name.clone(),
                    private: false,
                    dm: false,
                    person: None,
                },
            ));
            people::matches(&state.read(), &query.read(), &excluded, &directory.read())
        });
    use_effect(move || {
        let index = active();
        if index < matches.read().len() {
            dioxus::document::eval(&format!(
                "requestAnimationFrame(()=>document.getElementById('forward-option-{index}')?.scrollIntoView({{block:'nearest'}}));"
            ));
        }
    });
    let snapshot = state.read();
    if !same_dialog(&snapshot, &target) {
        return rsx! {};
    }
    let source = snapshot
        .core
        .workspaces
        .get(&target.team)
        .and_then(|w| w.channels.get(&target.channel));
    let private = source.is_some_and(|c| c.is_private || c.is_group || c.is_im || c.is_mpim);
    let source_name = source
        .and_then(|c| {
            snapshot
                .core
                .workspaces
                .get(&target.team)
                .map(|w| super_platinum_core::state::channel_display_name(w, c))
        })
        .unwrap_or_default();
    let title = if target.recovery_only {
        "Saved forwarding delivery"
    } else if private {
        "Forward private message"
    } else {
        "Forward message"
    };
    let preview = target.message.clone();
    let copy_target = target.clone();
    let retry_target = target.clone();
    let dialog_id = target.id.clone();
    let initial_blocks = if let Some(receipt) =
        saved.read().receipts.values().find(|r| r.unresolved())
    {
        serde_json::to_string(&receipt.delivery.request.blocks).unwrap_or_else(|_| "[]".into())
    } else if std::env::var("SUPER_PLATINUM_FIXTURE").is_ok_and(|name| name.starts_with("forward"))
    {
        serde_json::json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Worth a look 🚀","style":{"bold":true}},{"type":"text","text":"\nThe original conversation stays attached."}]}]}]).to_string()
    } else {
        "[]".into()
    };
    rsx! {
        div {class:"scrim",onclick:move |_|if !sending(){close(state)}}
        section {id:"forward-dialog","data-editor-generation":"{editor_visit}","data-keyboard-dialog":"true","data-dialog-kind":"forward","data-dialog-generation":dialog_id.clone(),"data-escape-local":"true","data-scheduled-forward-busy":sending(),"data-scheduled-forward-owner":dialog_id.clone(),"data-forward-id":dialog_id,class:"modal forward-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"forward-title",
            onmounted:move |_|{dioxus::core::spawn_forever(check::run(state));dioxus::core::spawn_forever(link_check::run(state));dioxus::core::spawn_forever(recovery_check::run(state));},
            onkeydown:move |event|if event.key()==Key::Escape {event.stop_propagation();if !sending(){close(state)}},
            header {
                h2 {id:"forward-title","{title}"}
                button {"aria-label":"Close forward dialog",disabled:sending(),onclick:move |_|close(state),{crate::icons::icon(crate::icons::Icon::Close,"icon")}}
            }
            div {class:"forward-body",
            if !target.recovery_only {
            label {r#for:"forward-destination","To"}
            div {class:"forward-selected",
                for destination in selected.read().iter() {
                    {let id=destination.id.clone();rsx!{button {class:"forward-chip",disabled:sending(),"aria-label":"Remove {destination.name}",onclick:move |_|selected.write().retain(|d|d.id!=id),"{destination.name}",{crate::icons::icon(crate::icons::Icon::Close,"icon sm")}}}}
                }
            }
            input {id:"forward-destination",initial_value:query(),placeholder:"Search people or conversations",disabled:sending(),
                role:"combobox","aria-expanded":true,"aria-controls":"forward-options","aria-autocomplete":"list",
                "aria-activedescendant":(!matches.read().is_empty()).then(||format!("forward-option-{}",active().min(matches.read().len().saturating_sub(1)))),
                onmounted:move |_|crate::view::composer::focus_field_when_mounted("forward-destination"),
                oninput:move |event|{query.set(event.value());active.set(0);},
                onkeydown:move |event|match event.key(){
                    Key::ArrowDown=>{event.prevent_default();active.set((active()+1).min(matches.read().len().saturating_sub(1)));},
                    Key::ArrowUp=>{event.prevent_default();active.set(active().saturating_sub(1));},
                    Key::Enter=>{event.prevent_default();let choice=matches.read().get(active()).cloned();if let Some(choice)=choice {if people::choose(state,selected,directory,choice,error){query.set(String::new());active.set(0);crate::view::composer::set_field_text("forward-destination","");}}},
                    _=>{}
                },
            }
            div {class:"forward-options",role:"listbox",id:"forward-options","aria-label":"Conversations",
                for (index,destination) in matches.read().iter().enumerate() {
                    {let destination=destination.clone();rsx!{button {id:"forward-option-{index}","data-destination":destination.id.clone(),role:"option","aria-selected":index==active(),disabled:sending(),
                        class:if index==active(){"forward-option selected"}else{"forward-option"},
                        onclick:move |_|{if people::choose(state,selected,directory,destination.clone(),error){query.set(String::new());active.set(0);crate::view::composer::set_field_text("forward-destination","");crate::view::composer::focus_field_when_mounted("forward-destination");}},
                        {crate::icons::icon(if destination.dm {crate::icons::Icon::DirectMessages}else if destination.private {crate::icons::Icon::Lock}else{crate::icons::Icon::Tag},"icon sm")}
                        span {"{destination.name}"}
                        if let Some(person)=&destination.person {if !person.handle.is_empty() {small {"@{person.handle}"}}}
                    }}}
                }
                if matches.read().is_empty() {p {class:"forward-hint","No matching people or conversations"}}
            }
            if directory.read().loading {p {class:"forward-hint",role:"status","Searching workspace…"}}
            if !query.read().trim().is_empty() && !query.read().trim_start().starts_with('#') {
                if let Some(message)=directory.read().error {div {class:"forward-directory-error",role:"status",span {"{message}"}button {disabled:sending(),onclick:move |_|retry+=1,"Retry search"}}}
            }
            }
            if !target.recovery_only {
            if saved.read().loaded {crate::view::rich_editor::RichEditor {id:"forward-note",autofocus:false,label:"Add a message (optional)",blocks:initial_blocks,disabled:sending()||deliveries.read().uncertain(),expected_generation:Some(editor_visit)}}
            if deliveries.read().uncertain() {p {class:"forward-hint","Saved notes are shown with their delivery records below."}}
            if saved.read().mixed_notes() {p {class:"forward-hint","These saved forwards have different notes. Resolve their delivery before forwarding to another conversation."}}
            div {class:"forward-preview",
                strong {"{preview.author}"}
                span {class:"forward-source","From {source_name}"}
                for node in &preview.body {{crate::view::rich::rich_node(node,snapshot.media_epoch,state)}}
                {crate::view::rich::attachment_embeds(state,snapshot.media_epoch,&preview.attachments)}
            }
            if private {p {class:"forward-hint","Recipients will see a copy of this private message."}}
            scheduled::Controls {state,selected,busy:sending,error,dialog_id:target.id.clone(),private}
            }
            if !saved.read().loaded {
                if saved.read().load_failed {button {onclick:move |_|{saved.write().load_failed=false;error.set(String::new());spawn(recovery::load(state,retry_target.clone(),saved,deliveries,error));},"Retry saved forwards"}}
                else {p {class:"forward-hint",role:"status","Loading saved forwards…"}}
            }
            if target.recovery_only {p {class:"forward-hint","Review earlier forwards here, even when the original message is no longer loaded."}}
            delivery::Results {state,deliveries,saved,selected,busy:sending,error,open_retry}
            if !error.read().is_empty() {p {role:"alert",class:"forward-error","{error}"}}
            }
            if !target.recovery_only {footer {
                button {disabled:copying(),onclick:move |_|{
                    if copying(){return;}copying.set(true);error.set(String::new());
                    let target=copy_target.clone();
                    dioxus::core::spawn_forever(async move {
                        let result=copy_link(state,&target).await;
                        if let Ok(mut value)=copying.try_write(){*value=false;}
                        if let Err(message)=result {if let Ok(mut value)=error.try_write(){*value=message;}}
                    });
                },if copying(){"Copying…"}else{"Copy link"}}
                button {class:"primary",disabled:sending()||!saved.read().loaded||saved.read().mixed_notes()||selected.read().is_empty(),onclick:move |_|{
                    if sending(){return;}sending.set(true);error.set(String::new());
                    let target=target.clone();let destinations=selected();
                    let ui=delivery::Ui {state,deliveries,saved,selected,busy:sending,error,open_retry};
                    dioxus::core::spawn_forever(ui.submit(target,destinations,None));
                },if sending(){"Working…"}else{"Forward"}}
            }}
        }
    }
}
async fn read_note(dialog: &str) -> Option<Vec<serde_json::Value>> {
    let dialog = serde_json::to_string(dialog).unwrap();
    let mut eval = dioxus::document::eval(&format!(
        "await (async()=>{{let note=null;try{{const root=document.getElementById('forward-dialog');note=root?.dataset.forwardId==={dialog}?window.superPlatinumRichEditor.read('forward-note'):null;}}catch{{}}dioxus.send(note);await dioxus.recv();}})();"
    ));
    let note = eval
        .recv::<Option<Vec<serde_json::Value>>>()
        .await
        .ok()
        .flatten();
    let _ = eval.send("received");
    note
}
async fn copy_link(mut state: Signal<ShellState>, target: &ForwardTarget) -> Result<(), String> {
    let Some(ctx) = Context::get(&state, target) else {
        return Err("Connect to Slack to copy the message link.".into());
    };
    let link = api::fetch_permalink(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        &target.channel,
        &target.message.ts,
        target.message.thread_ts.as_deref(),
    )
    .await
    .map_err(|_| "The message link could not be loaded.".to_owned())?;
    if !ctx.current(&state.peek()) {
        return Ok(());
    }
    let value = serde_json::to_string(&link).unwrap();
    let mut eval = dioxus::document::eval(&format!(
        "await (async()=>{{let copied=false;try{{await navigator.clipboard.writeText({value});copied=true;}}catch{{}}dioxus.send(copied);await dioxus.recv();}})();"
    ));
    let copied = eval.recv::<bool>().await.unwrap_or(false);
    let _ = eval.send("received");
    if !ctx.current(&state.peek()) {
        return Ok(());
    }
    if copied {
        state.write().show_toast("Message link copied.");
        Ok(())
    } else {
        Err("The clipboard could not be updated. Please try again.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> ShellState {
        ShellState::fixture(crate::media::MediaRegistry::default())
    }
    #[test]
    fn conversation_search_excludes_archived_and_already_selected_targets() {
        let mut shell = fixture();
        let matches = destinations(&shell, "#ship", &[]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "C2");
        assert!(destinations(&shell, "SHIP", &matches).is_empty());
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .channels
            .get_mut("C2")
            .unwrap()
            .is_archived = true;
        assert!(destinations(&shell, "ship", &[]).is_empty());
        assert!(destinations(&shell, "maya", &[]).iter().any(|d| d.dm));
    }
    #[test]
    fn old_dialog_completion_cannot_close_a_new_dialog_or_other_account() {
        let mut shell = fixture();
        let target = ForwardTarget {
            recovery_only: false,
            epoch: shell.core.account_epoch,
            team: "T1".into(),
            channel: "C2".into(),
            message: shell.messages[0].clone(),
            id: "first".into(),
        };
        shell.forward_target = Some(target.clone());
        shell.overlay = Some(Overlay::Forward);
        assert!(same_dialog(&shell, &target));
        shell.forward_target.as_mut().unwrap().id = "new".into();
        assert!(!same_dialog(&shell, &target));
        shell.forward_target = Some(target.clone());
        shell.core.account_epoch += 1;
        assert!(!same_dialog(&shell, &target));
    }
}
