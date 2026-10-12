use crate::{
    icons::{Icon, icon},
    scheduled::Context,
    state::{ChannelOpen, ShellState},
};
use dioxus::prelude::*;
use super_platinum_core::slack::{api, models::Team};

#[component]
pub(super) fn WorkspaceHeading(state: Signal<ShellState>, workspace_name: String) -> Element {
    let mut opened =
        use_signal(|| std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("channel-create"));
    rsx! {
        div{class:"sidebar-title",
            button{title:"Accounts",onclick:move |_|{let mut state=state;state.write().overlay=Some(crate::state::Overlay::Accounts);},"{workspace_name}"}
            button {id:"new-channel-trigger",class:"new-channel-trigger",title:"Create channel","aria-label":"Create channel",onclick:move |_|opened.set(true),{icon(Icon::Add,"icon sm")}}
        }
        if opened() {CreateChannelDialog{state,onclose:move |_|{opened.set(false);dioxus::document::eval("document.getElementById('new-channel-trigger')?.focus();");}}}
    }
}
#[component]
fn CreateChannelDialog(mut state: Signal<ShellState>, onclose: EventHandler) -> Element {
    let mut name = use_signal(String::new);
    let mut private = use_signal(|| true);
    let mut teams = use_signal(Vec::<Team>::new);
    let mut team = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let scope = use_hook(|| {
        (
            state.peek().core.account_epoch,
            state.peek().core.active_team.clone(),
        )
    });
    if (
        state.read().core.account_epoch,
        state.read().core.active_team.clone(),
    ) != scope
    {
        return rsx! {};
    }
    let normalized = api::normalize_channel_name(&name.read());
    let valid = normalized.is_ok();
    rsx! {
        div{class:"scrim",onclick:move |_|if !busy(){onclose.call(())}}
        section{id:"channel-create-dialog",class:"modal channel-create",role:"dialog","aria-modal":"true","aria-labelledby":"channel-create-title",
            onkeydown:move |e|if e.key()==Key::Escape {e.stop_propagation();if !busy(){onclose.call(())}},
            onmounted:move |_|{
                super::channel_details::focus_dialog("channel-create-dialog");
                spawn(async move {
                    if crate::fixture::is_fixture(){teams.set(vec![Team{id:"T_FIXTURE".into(),name:Some("Desktop team".into()),..Default::default()}]);team.set("T_FIXTURE".into());loading.set(false);return;}
                    let Some(ctx)=Context::get(&state)else{loading.set(false);error.set("Connect to Slack to create a channel.".into());return;};
                    let result=api::fetch_channel_creation_teams(&ctx.transport,&ctx.client,&ctx.session).await;
                    if !ctx.current(&state.peek()){return;}
                    loading.set(false);
                    match result{Ok(items)=>{if items.len()==1{team.set(items[0].id.clone());}teams.set(items);},Err(e)=>error.set(super::channel_details::failure(&e))}
                });
            },
            header{h2{id:"channel-create-title","Create channel"}button{disabled:busy(),"aria-label":"Close create channel",onclick:move |_|onclose.call(()),{icon(Icon::Close,"icon")}}}
            label{r#for:"new-channel-name","Channel name"}
            input{id:"new-channel-name",value:"{name}",placeholder:"e.g. project-launch",disabled:busy(),oninput:move |e|name.set(e.value())}
            if !name.read().is_empty(){
                match &normalized {Ok(value)=>rsx!{p{class:"scheduled-hint","#{value}"}},Err(message)=>rsx!{p{class:"later-error","{message}"}}}
            }
            label{r#for:"new-channel-team","Workspace"}
            select{id:"new-channel-team",value:"{team}",disabled:busy()||loading(),onchange:move |e|team.set(e.value()),
                option{value:"",if loading(){"Loading workspaces…"}else{"Choose a workspace"}}
                for workspace in teams.read().iter(){option{value:"{workspace.id}","{workspace.name.as_deref().unwrap_or(&workspace.id)}"}}
            }
            fieldset{disabled:busy(),legend{"Visibility"}
                label{input{r#type:"radio",name:"channel-visibility",checked:private(),onchange:move |_|private.set(true)}span{strong{"Private"}small{"Only invited people can view or join."}}}
                label{input{r#type:"radio",name:"channel-visibility",checked:!private(),onchange:move |_|private.set(false)}span{strong{"Public"}small{"Anyone in the workspace can find and join."}}}
            }
            if !error.read().is_empty(){p{role:"alert",class:"later-error","{error}"}}
            footer{class:"scheduled-actions",
                button{disabled:busy(),onclick:move |_|onclose.call(()),"Cancel"}
                button{class:"primary",disabled:busy()||loading()||!valid||team.read().is_empty(),onclick:move |_|{
                    let Ok(name)=api::normalize_channel_name(&name.peek())else{return;};
                    let Some(ctx)=Context::get(&state)else{error.set("Connect to Slack to create a channel.".into());return;};
                    let target=team.peek().clone();
                    if !teams.peek().iter().any(|t|t.id==target){return;}
                    let private=private();let generation=state.peek().channel_generation;
                    busy.set(true);error.set(String::new());
                    dioxus::core::spawn_forever(async move{
                        if !ctx.current(&state.peek()){return;}
                        let result=api::create_conversation(&ctx.transport,&ctx.client,&ctx.session,&target,&name,private).await;
                        if !ctx.current(&state.peek()){return;}
                        let alive=if let Ok(mut b)=busy.try_write(){*b=false;true}else{false};
                        match result{
                            Ok(channel)=>{
                                let id=channel.id.clone();
                                {let mut shell=state.write();if let Some(w)=shell.core.workspaces.get_mut(&ctx.session.team_id){w.channels.entry(id.clone()).or_insert(channel);}shell.refresh_from_core();}
                                crate::bootstrap::persist_workspace(&state,&ctx.session.team_id);
                                if alive&&state.peek().channel_generation==generation{
                                    let index=state.peek().channels.iter().position(|c|c.id==id);
                                    if let Some(index)=index{state.write().select_channel(index,ChannelOpen::Global);}
                                    onclose.call(());
                                    crate::bootstrap::refresh_selected_channel(state).await;
                                }
                                state.write().show_toast("Channel created.");
                            }
                            Err(e)=>if let Ok(mut notice)=error.try_write(){*notice=super::channel_details::failure(&e);}
                        }
                    });
                },if busy(){"Creating…"}else{"Create channel"}}
            }
        }
    }
}
