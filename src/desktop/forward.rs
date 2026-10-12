//! Message forwarding and copying links, with account-scoped async work.
use crate::state::{MessageVm, Overlay, ShellState};
use dioxus::prelude::*;
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport, api},
};

#[derive(Debug, Clone)]
pub struct ForwardTarget {
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
    let query = query.trim().trim_start_matches('#').to_lowercase();
    let mut result = workspace
        .channels
        .values()
        .filter(|c| !c.is_archived && !selected.iter().any(|d| d.id == c.id))
        .filter_map(|c| {
            let name = super_platinum_core::state::channel_display_name(workspace, c);
            name.to_lowercase().contains(&query).then(|| Destination {
                id: c.id.clone(),
                name,
                private: c.is_private || c.is_group,
                dm: c.is_im || c.is_mpim,
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
    let mut query = use_signal(String::new);
    let mut selected = use_signal(Vec::<Destination>::new);
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
    let mut requests = use_signal(BTreeMap::<String, api::ForwardMessage>::new);
    let matches = use_memo(move || destinations(&state.read(), &query.read(), &selected.read()));
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
    let title = if private {
        "Forward private message"
    } else {
        "Forward message"
    };
    let preview = target.message.clone();
    let copy_target = target.clone();
    let dialog_id = target.id.clone();
    let initial_blocks = if std::env::var("SUPER_PLATINUM_FIXTURE")
        .is_ok_and(|name| name.starts_with("forward"))
    {
        serde_json::json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Worth a look 🚀","style":{"bold":true}},{"type":"text","text":"\nThe original conversation stays attached."}]}]}]).to_string()
    } else {
        "[]".into()
    };
    rsx! {
        div {class:"scrim",onclick:move |_|if !sending(){close(state)}}
        section {id:"forward-dialog","data-forward-id":dialog_id,class:"modal forward-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"forward-title",
            onkeydown:move |event|if event.key()==Key::Escape {event.stop_propagation();if !sending(){close(state)}},
            header {
                h2 {id:"forward-title","{title}"}
                button {"aria-label":"Close forward dialog",disabled:sending(),onclick:move |_|close(state),{crate::icons::icon(crate::icons::Icon::Close,"icon")}}
            }
            div {class:"forward-body",
            label {r#for:"forward-destination","To"}
            div {class:"forward-selected",
                for destination in selected.read().iter() {
                    {let id=destination.id.clone();rsx!{button {class:"forward-chip",disabled:sending(),"aria-label":"Remove {destination.name}",onclick:move |_|selected.write().retain(|d|d.id!=id),"{destination.name}",{crate::icons::icon(crate::icons::Icon::Close,"icon sm")}}}}
                }
            }
            input {id:"forward-destination",initial_value:"",placeholder:"Search conversations",disabled:sending(),
                role:"combobox","aria-expanded":true,"aria-controls":"forward-options","aria-autocomplete":"list",
                "aria-activedescendant":(!matches.read().is_empty()).then(||format!("forward-option-{}",active().min(matches.read().len().saturating_sub(1)))),
                onmounted:move |_|crate::view::composer::focus_field_when_mounted("forward-destination"),
                oninput:move |event|{query.set(event.value());active.set(0);},
                onkeydown:move |event|match event.key(){
                    Key::ArrowDown=>{event.prevent_default();active.set((active()+1).min(matches.read().len().saturating_sub(1)));},
                    Key::ArrowUp=>{event.prevent_default();active.set(active().saturating_sub(1));},
                    Key::Enter=>{event.prevent_default();let choice=matches.read().get(active()).cloned();if let Some(choice)=choice {selected.write().push(choice);query.set(String::new());active.set(0);crate::view::composer::set_field_text("forward-destination","");}},
                    _=>{}
                },
            }
            div {class:"forward-options",role:"listbox",id:"forward-options","aria-label":"Conversations",
                for (index,destination) in matches.read().iter().enumerate() {
                    {let destination=destination.clone();rsx!{button {id:"forward-option-{index}",role:"option","aria-selected":index==active(),disabled:sending(),
                        class:if index==active(){"forward-option selected"}else{"forward-option"},
                        onclick:move |_|{selected.write().push(destination.clone());query.set(String::new());active.set(0);crate::view::composer::set_field_text("forward-destination","");crate::view::composer::focus_field_when_mounted("forward-destination");},
                        {crate::icons::icon(if destination.dm {crate::icons::Icon::DirectMessages}else if destination.private {crate::icons::Icon::Lock}else{crate::icons::Icon::Tag},"icon sm")}
                        span {"{destination.name}"}
                    }}}
                }
                if matches.read().is_empty() {p {class:"forward-hint","No matching conversations"}}
            }
            crate::view::rich_editor::RichEditor {id:"forward-note",label:"Add a message (optional)",blocks:initial_blocks,disabled:sending()}
            div {class:"forward-preview",
                strong {"{preview.author}"}
                span {class:"forward-source","From {source_name}"}
                for node in &preview.body {{crate::view::rich::rich_node(node,snapshot.media_epoch,state)}}
                {crate::view::rich::attachment_embeds(state,snapshot.media_epoch,&preview.attachments)}
            }
            if private {p {class:"forward-hint","Recipients will see a copy of this private message."}}
            if !error.read().is_empty() {p {role:"alert",class:"forward-error","{error}"}}
            }
            footer {
                button {disabled:copying(),onclick:move |_|{
                    if copying(){return;}copying.set(true);error.set(String::new());
                    let target=copy_target.clone();
                    dioxus::core::spawn_forever(async move {
                        let result=copy_link(state,&target).await;
                        if let Ok(mut value)=copying.try_write(){*value=false;}
                        if let Err(message)=result {if let Ok(mut value)=error.try_write(){*value=message;}}
                    });
                },if copying(){"Copying…"}else{"Copy link"}}
                button {class:"primary",disabled:sending()||selected.read().is_empty(),onclick:move |_|{
                    if sending(){return;}sending.set(true);error.set(String::new());
                    let target=target.clone();let destinations=selected();
                    dioxus::core::spawn_forever(async move {
                        let blocks=read_note(&target.id).await;
                        if !same_dialog(&state.peek(),&target){return;}
                        let Some(ctx)=Context::get(&state,&target) else {if let Ok(mut busy)=sending.try_write(){*busy=false;}return;};
                        let Some(blocks)=blocks else {if let Ok(mut busy)=sending.try_write(){*busy=false;}if let Ok(mut e)=error.try_write(){*e="The note could not be read. Please try again.".into();}return;};
                        // Snapshot every request before the first network await;
                        // closing the dialog must not cancel later destinations.
                        let batch=destinations.into_iter().map(|destination| {
                                let Ok(mut cached)=requests.try_write() else {return None;};
                                let entry=cached.entry(destination.id.clone()).or_insert_with(||api::ForwardMessage::new(target.channel.clone(),target.message.ts.clone(),destination.id.clone(),blocks.clone()));
                                if entry.blocks!=blocks {*entry=api::ForwardMessage::new(target.channel.clone(),target.message.ts.clone(),destination.id.clone(),blocks.clone());}
                                Some((destination,entry.clone()))
                        }).collect::<Option<Vec<_>>>();
                        let Some(batch)=batch else {return;};
                        let mut successful=Vec::new();let mut errors=Vec::new();
                        for (destination,request) in batch {
                            if !ctx.current(&state.peek()){return;}
                            match api::forward_message(&ctx.transport,&ctx.client,&ctx.session,&request).await {
                                Ok(_)=>successful.push(destination.id),
                                Err(failure)=>errors.push(format!("{}: {}",destination.name,failure_message(&failure))),
                            }
                        }
                        if !ctx.current(&state.peek()){return;}
                        if same_dialog(&state.peek(),&target) {
                            if let Ok(mut value)=selected.try_write(){value.retain(|d|!successful.contains(&d.id));}
                            if let Ok(mut value)=sending.try_write(){*value=false;}
                            if errors.is_empty(){close(state);state.write().show_toast("Message forwarded.");}
                            else if let Ok(mut value)=error.try_write(){*value=format!("{} Your note is still here.",errors.join(" "));}
                        } else if !errors.is_empty() {state.write().show_toast("Some forwards could not be confirmed. Check the destination conversations.");}
                        if state.peek().core.active_channel.as_ref().is_some_and(|c|successful.contains(c)) {
                            crate::bootstrap::refresh_selected_channel(state).await;
                        }
                    });
                },if sending(){"Forwarding…"}else{"Forward"}}
            }
        }
    }
}
fn failure_message(error: &super_platinum_core::slack::Error) -> String {
    use super_platinum_core::slack::Error;
    match error {
        Error::RateLimited {
            retry_after_secs: Some(seconds),
        } => format!("Slack asked you to wait {seconds} seconds before trying again."),
        Error::RateLimited { .. } => "Slack is limiting requests. Try again shortly.".into(),
        Error::Api(code)
            if matches!(
                code.as_str(),
                "channel_not_found" | "not_in_channel" | "is_archived"
            ) =>
        {
            "This conversation is unavailable for forwarding.".into()
        }
        Error::Api(code)
            if matches!(
                code.as_str(),
                "restricted_action" | "not_allowed" | "permission_denied" | "no_permission"
            ) =>
        {
            "You do not have permission to forward this message here.".into()
        }
        Error::Api(_) => "Slack could not forward this message.".into(),
        Error::Offline(_) | Error::TransportNotConfigured => {
            "Reconnect to Slack to forward this message.".into()
        }
        _ => "Delivery could not be confirmed. Check the conversation before retrying.".into(),
    }
}
async fn read_note(dialog: &str) -> Option<Vec<serde_json::Value>> {
    let dialog = serde_json::to_string(dialog).unwrap();
    dioxus::document::eval(&format!(
        "try{{const root=document.getElementById('forward-dialog');dioxus.send(root?.dataset.forwardId==={dialog}?window.superPlatinumRichEditor.read('forward-note'):null);}}catch{{dioxus.send(null);}}"
    ))
    .recv::<Option<Vec<serde_json::Value>>>()
    .await
    .ok().flatten()
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
    let copied=dioxus::document::eval(&format!("try{{await navigator.clipboard.writeText({value});dioxus.send(true);}}catch{{dioxus.send(false);}}" )).recv::<bool>().await.unwrap_or(false);
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
