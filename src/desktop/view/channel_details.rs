use crate::{
    icons::{Icon, icon},
    scheduled::Context,
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::slack::{
    api::{self, ChannelTextField, ConversationMembers},
    models::Channel,
};

mod member_list;
use member_list::load_members;

#[derive(Default)]
struct Details {
    info: Option<Channel>,
    loaded: bool,
    loading: bool,
    error: String,
    members: ConversationMembers,
    members_loaded: bool,
    members_loading: bool,
    members_revision: Option<(u64, u64)>,
    members_snapshot_revision: Option<(u64, u64)>,
}

#[component]
pub(super) fn ChannelDetailsButton(
    state: Signal<ShellState>,
    channel: String,
    title: String,
) -> Element {
    let mut opened = use_signal(|| {
        matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-details"
                | "channel-mute"
                | "channel-mute-narrow"
                | "channel-notifications"
                | "channel-notifications-narrow"
                | "channel-leave"
                | "channel-leave-last"
                | "channel-join"
                | "member-add"
                | "member-live-refresh"
                | "member-add-error"
                | "member-remove"
                | "member-add-narrow"
                | "member-remove-narrow"
                | "channel-members")
        )
    });
    rsx! {
        h2{button{id:"channel-details-trigger",class:"header-name","aria-label":"Channel details for {title}",onclick:move |_|opened.set(true),"{title}"}}
        if opened(){DetailsDialog{state,channel,title:title.clone(),onclose:move |_|{opened.set(false);dioxus::document::eval("document.getElementById('channel-details-trigger')?.focus();");}}}
    }
}
#[component]
fn DetailsDialog(
    mut state: Signal<ShellState>,
    channel: String,
    title: String,
    onclose: EventHandler,
) -> Element {
    let scope = use_hook(|| {
        (
            state.peek().core.account_epoch,
            state.peek().core.active_team.clone(),
        )
    });
    let mut details = use_signal(|| Details {
        info: state.peek().core.active_team.as_ref().and_then(|t| {
            state
                .peek()
                .core
                .workspaces
                .get(t)
                .and_then(|w| w.channels.get(&channel).cloned())
        }),
        ..Default::default()
    });
    let mut observed_revision = use_signal(|| 0u64);
    let observed_channel = channel.clone();
    let observed_scope = scope.clone();
    use_effect(move || {
        let update = {
            let shell = state.read();
            if (shell.core.account_epoch, shell.core.active_team.clone()) != observed_scope {
                return;
            }
            let Some(workspace) = shell
                .core
                .active_team
                .as_ref()
                .and_then(|team| shell.core.workspaces.get(team))
            else {
                return;
            };
            let revision = workspace
                .channel_lifecycle
                .channel_revision(&observed_channel);
            if revision == *observed_revision.peek() {
                return;
            }
            (revision, workspace.channels.get(&observed_channel).cloned())
        };
        observed_revision.set(update.0);
        if let Some(info) = update.1 {
            details.write().info = Some(info);
        }
    });
    let mut members = use_signal(|| {
        std::env::var("SUPER_PLATINUM_FIXTURE")
            .is_ok_and(|name| name.starts_with("member-") || name == "channel-members")
    });
    let mut query = use_signal(String::new);
    let mut adding = use_signal(|| false);
    let mut shown = use_signal(|| 100usize);
    let mut editing = use_signal(|| None::<(ChannelTextField, String)>);
    let mut text = use_signal(String::new);
    let mut saving = use_signal(|| false);
    // Refresh only the open member pane, coalescing socket bursts in the loader.
    // Keeping AddMembers mounted preserves the query and selected people.
    let refresh_channel = channel.clone();
    let refresh_scope = scope.clone();
    use_effect(move || {
        let shell = state.read();
        if (shell.core.account_epoch, shell.core.active_team.clone()) != refresh_scope {
            return;
        }
        let revision = member_list::revision(&shell, &refresh_channel);
        let data = details.read();
        if members()
            && !saving()
            && !data.members_loading
            && data
                .members_revision
                .is_some_and(|previous| Some(previous) != revision)
        {
            spawn(load_members(state, details, refresh_channel.clone(), true));
        }
    });
    let focus_request = use_signal(|| (0u64, false));
    use_effect(move || {
        let (revision, editor) = focus_request();
        if revision > 0 {
            // Restore focus from the mounted component after the render that
            // replaces or re-enables controls, not from the detached task.
            let selector = if editor {
                "#channel-detail-edit"
            } else {
                "#channel-details-dialog [role=tab][aria-selected=true]"
            };
            dioxus::document::eval(&format!(
                "document.querySelector({})?.focus();",
                serde_json::to_string(selector).unwrap()
            ));
        }
    });
    let mut archive_confirmation = use_signal(|| None::<bool>);
    let mount_channel = channel.clone();
    let mount_members_channel = channel.clone();
    let members_channel = channel.clone();
    let more_channel = channel.clone();
    let retry_channel = channel.clone();
    let archive_channel = channel.clone();
    if (
        state.read().core.account_epoch,
        state.read().core.active_team.clone(),
    ) != scope
    {
        return rsx! {};
    }
    let data = details.read();
    let name = data
        .info
        .as_ref()
        .and_then(|c| c.name.as_deref())
        .unwrap_or(&channel);
    let editable = data.loaded
        && data.info.as_ref().is_some_and(|c| {
            !c.is_archived && c.extra.get("is_member").and_then(|v| v.as_bool()) != Some(false)
        });
    let heading = if data.info.as_ref().is_some_and(|c| c.is_mpim) {
        title
    } else {
        format!("#{name}")
    };
    let shell = state.read();
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t));
    let needle = query.read().to_lowercase();
    let mut names: Vec<_> = data
        .members
        .ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                workspace
                    .map(|w| w.display_name(id))
                    .unwrap_or_else(|| id.clone()),
            )
        })
        .filter(|(_, name)| name.to_lowercase().contains(&needle))
        .collect();
    names.sort_by_cached_key(|(_, name)| name.to_lowercase());
    rsx! {
        div{class:"scrim",onclick:move |_|if !saving(){onclose.call(())}}
        section{id:"channel-details-dialog",class:"modal channel-details",role:"dialog","aria-modal":"true","aria-labelledby":"channel-details-title",
            onkeydown:move |e|if e.key()==Key::Escape{e.stop_propagation();if !saving(){onclose.call(())}},
            onmounted:move |_|{
                focus_dialog("channel-details-dialog");
                spawn(load_info(state,details,mount_channel.clone()));
                if members(){spawn(load_members(state,details,mount_members_channel.clone(),false));}
            },
            header{h2{id:"channel-details-title","{heading}"}button{"aria-label":"Close channel details",disabled:saving(),onclick:move |_|onclose.call(()),{icon(Icon::Close,"icon")}}}
            div{class:"later-tabs",role:"tablist","aria-label":"Channel details tabs",
                button{role:"tab",class:if !members(){"active"}else{""},"aria-selected":!members(),disabled:saving(),onclick:move |_|{members.set(false);},"About"}
                button{role:"tab",class:if members(){"active"}else{""},"aria-selected":members(),disabled:saving(),onclick:move |_|{members.set(true);if !details.peek().members_loaded{spawn(load_members(state,details,members_channel.clone(),false));}},"Members"}
            }
            div {class:"channel-details-body",
            if data.error.is_empty() && editing.read().as_ref().is_some_and(|(field,expected)|data.info.as_ref().is_some_and(|info|field.value(info)!=*expected)) {
                p{role:"status",class:"scheduled-hint","This field changed in Slack. Your unsaved edit is preserved; cancel to review the new value."}
            }
            if !data.error.is_empty(){p{role:"alert",class:"later-error","{data.error}"}}
            if data.loading{p{role:"status",class:"scheduled-hint","Loading channel details…"}}
            if !members(){
                if !data.loaded&&!data.loading{button{onclick:move |_|{spawn(load_info(state,details,retry_channel.clone()));},"Retry"}}
                if let Some(info)=&data.info{
                    p{class:"scheduled-hint",if info.is_archived{"Archived · "}if info.is_mpim{"Group direct message"}else if info.is_private||info.is_group{"Private channel"}else{"Public channel"}}
                    MuteControl {state,channel:channel.clone(),disabled:!data.loaded||saving()||editing.read().is_some()||archive_confirmation.read().is_some()}
                    super::channel_notifications::ChannelNotifications {state,channel:channel.clone(),disabled:!data.loaded||saving()||editing.read().is_some()||archive_confirmation.read().is_some()}
                    for field in [ChannelTextField::Name,ChannelTextField::Topic,ChannelTextField::Description].into_iter().filter(|field|*field!=ChannelTextField::Name||(!info.is_im&&!info.is_mpim)){
                        {let original=field.value(info);let value=original.clone();let save_channel=channel.clone();let nodes=workspace.map(|w|crate::blocks::mrkdwn_blocks(crate::blocks::BlockCtx::new(w,&shell.media),&value)).unwrap_or_default();
                        rsx!{section{class:"channel-detail-field",
                            div{class:"channel-detail-label",h3{"{field.label()}"}span{class:"spacer"}
                                if editing.read().as_ref().is_none_or(|(f,_)|*f!=field){button{disabled:!editable||saving()||editing.read().is_some()||archive_confirmation.read().is_some(),onclick:move |_|{editing.set(Some((field,original.clone())));text.set(original.clone());details.write().error.clear();},"Edit {field.label()}"}}
                            }
                            if editing.read().as_ref().is_some_and(|(f,_)|*f==field){
                                label{r#for:"channel-detail-edit","{field.label()}"}
                                if field==ChannelTextField::Name {
                                    input{id:"channel-detail-edit",value:"{text}",disabled:saving(),onmounted:move |_|{dioxus::document::eval("document.getElementById('channel-detail-edit')?.focus();");},oninput:move |e|text.set(e.value())}
                                    if let Ok(name)=api::normalize_channel_name(&text.read()){p{class:"scheduled-hint","#{name}"}}
                                }else{textarea{id:"channel-detail-edit",onmounted:move |_|{dioxus::document::eval("document.getElementById('channel-detail-edit')?.focus();");},value:"{text}",disabled:saving(),oninput:move |e|text.set(e.value())}}
                                div{class:"scheduled-actions",
                                    button{disabled:saving(),onclick:move |_|{editing.set(None);details.write().error.clear();request_details_focus(focus_request, false);},"Cancel edit"}
                                    button{class:"primary",disabled:saving()||!editable,onclick:move |_|{
                                        let Some((field,expected))=editing.peek().clone()else{return;};
                                        let Some(ctx)=Context::get(&state)else{details.write().error="Connect to Slack to update this channel.".into();return;};
                                        let value=text.peek().clone();let target=save_channel.clone();saving.set(true);details.write().error.clear();
                                        dioxus::core::spawn_forever(async move{
                                            let result=save(state,details,&ctx,&target,field,&expected,&value).await;
                                            if let Ok(mut busy)=saving.try_write(){*busy=false;}
                                            match result{Ok(())=>{if let Ok(mut editor)=editing.try_write(){*editor=None;request_details_focus(focus_request, false);}},Err(error)=>{if let Ok(mut data)=details.try_write(){data.error=error;}request_details_focus(focus_request, true);}}
                                        });
                                    },if saving(){"Saving…"}else{"Save"}}
                                }
                            }else{div{class:"channel-detail-value",if value.is_empty(){"Not set"}else if field==ChannelTextField::Name{"{value}"}else{for node in &nodes{{super::rich::rich_node(node,shell.media_epoch,state)}}}}}
                        }} }
                    }
                    if let Some(created)=info.extra.get("created").and_then(|v|v.as_i64()){p{class:"scheduled-hint","Created {super_platinum_core::state::format_ts_date_label(&created.to_string())}"}}
                    p{class:"scheduled-hint","Channel ID: {channel}"}
                    super::channel_membership::MembershipControls {
                        state, channel:channel.clone(), ready:data.loaded,
                        disabled:saving()||editing.read().is_some()||archive_confirmation.read().is_some(),
                        join_only:false, onbusy:move |value|saving.set(value),
                        onchanged:move |_|{let mut data=details.write();data.members=Default::default();data.members_loaded=false;}
                    }
                    if data.loaded&&!info.is_im&&!info.is_mpim && info.extra.get("is_member").and_then(|v|v.as_bool()) != Some(false) && api::channel_membership_action(info) != Some(api::ChannelMembershipAction::Archive) {
                        if let Some(archive)=archive_confirmation(){
                            section{id:"channel-archive-confirm",class:"channel-archive-confirm",onmounted:move |_|{dioxus::document::eval("document.querySelector('#channel-archive-confirm button')?.focus();");},role:"group","aria-label":"Confirm channel archive change",
                                p{if archive{"Archive #{name}? Members will no longer be able to send messages. History remains available and the channel can be unarchived."}else{"Unarchive #{name}? Slack will restore the channel so members can use it again."}}
                                div{class:"scheduled-actions",
                                    button{disabled:saving(),onclick:move |_|{archive_confirmation.set(None);request_details_focus(focus_request, false);},"Cancel"}
                                    button{disabled:saving(),onclick:move |_|{
                                        let Some(ctx)=Context::get(&state)else{return;};
                                        let target=archive_channel.clone();saving.set(true);details.write().error.clear();
                                        dioxus::core::spawn_forever(async move{
                                            let result=change_archive(state,details,&ctx,&target,archive).await;
                                            if let Ok(mut busy)=saving.try_write(){*busy=false;}
                                            match result {Ok(())=>{if let Ok(mut pending)=archive_confirmation.try_write(){*pending=None;request_details_focus(focus_request, false);}},Err(e)=>{if let Ok(mut data)=details.try_write(){data.error=e;}}}
                                        });
                                    },if saving(){"Updating…"}else if archive{"Archive channel"}else{"Unarchive channel"}}
                                }
                            }
                        }else{
                            button{class:"channel-archive-action",disabled:saving()||editing.read().is_some(),onclick:move |_|archive_confirmation.set(Some(!details.peek().info.as_ref().is_some_and(|c|c.is_archived))),if info.is_archived{"Unarchive channel…"}else{"Archive channel…"}}
                        }
                    }

                }
            }else{
                super::member_admin::AddMembers {
                    state,channel:channel.clone(),private:data.info.as_ref().is_some_and(|info|info.is_private||info.is_group),
                    members:data.members.ids.clone(),disabled:saving()||data.members_loading||!data.loaded||!data.info.as_ref().is_some_and(api::channel_members_manageable),
                    onopen:move |value|adding.set(value),
                    onbusy:move |value|saving.set(value),onchanged:move |(id,present): (String,bool)|details.write().members.confirm_member(&id,present),
                }
                if !adding(){
                input{class:"list-find",r#type:"search",placeholder:"Search loaded members","aria-label":"Search loaded members",value:"{query}",disabled:saving(),oninput:move |e|{query.set(e.value());shown.set(100);}}
                p{class:"scheduled-hint","{data.members.ids.len()} members loaded"}
                div{class:"channel-members",for (id,name) in names.iter().take(shown()){
                    {let user=id.clone();
                    let initials=name.chars().next().map(|c|c.to_uppercase().to_string()).unwrap_or_default();
                    let avatar=workspace.and_then(|w|w.avatar_url(id)).and_then(|url|{let asset=shell.media.register_avatar(id,&url);shell.media.is_ready(&asset).then(||asset.uri_at(shell.media_epoch))});
                    rsx!{div{class:"channel-member-row",key:"{id}",button{class:"channel-member",disabled:saving(),onclick:move |_|{onclose.call(());dioxus::core::spawn_forever(crate::bootstrap::open_profile(state,user.clone()));},
                        span{class:"channel-member-avatar","aria-hidden":"true",if let Some(uri)=avatar{img{src:"{uri}",alt:""}}else{"{initials}"}}
                        span{"{name}"}
                    }
                    if workspace.is_some_and(|w|w.self_user_id!=*id){
                        super::member_admin::RemoveMember {
                            state,channel:channel.clone(),user:id.clone(),name:name.clone(),private:data.info.as_ref().is_some_and(|info|info.is_private||info.is_group),
                            disabled:saving()||data.members_loading||!data.loaded||!data.info.as_ref().is_some_and(|info|api::channel_members_manageable(info)&&info.extra.get("is_general").and_then(|v|v.as_bool())!=Some(true)),
                            onbusy:move |value|saving.set(value),onchanged:move |(id,present): (String,bool)|details.write().members.confirm_member(&id,present),
                        }
                    }
                    }}}
                }}
                if names.len()>shown(){button{disabled:saving(),onclick:move |_|shown.set(shown()+100),"Show more loaded members"}}
                if data.members_loading{p{role:"status","Loading members…"}}
                if !data.members_loading&&(!data.members_loaded||data.members.cursor.is_some()){button{disabled:saving(),onclick:move |_|{spawn(load_members(state,details,more_channel.clone(),false));},if data.members_loaded{"Load more members"}else{"Load members"}}}
                if data.members_loaded&&!data.members_loading {button{disabled:saving(),onclick:{let channel=channel.clone();move |_|{spawn(load_members(state,details,channel.clone(),true));}},"Reload members"}}
                if data.members_loaded&&names.is_empty(){p{class:"scheduled-hint","No matching loaded members."}}
                }
            }
            }
        }
    }
}
/// Personal notification state lives in the shared controller, so closing this
/// dialog cannot discard an in-flight or unconfirmed preference change.
#[component]
fn MuteControl(state: Signal<ShellState>, channel: String, disabled: bool) -> Element {
    let alive = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(true)));
    let drop_alive = alive.clone();
    use_drop(move || drop_alive.set(false));
    let change_alive = alive.clone();
    let temporary_alive = alive.clone();
    let show_alive = alive.clone();
    let mut choosing = use_signal(|| false);
    let mut duration = use_signal(|| String::from("hour"));
    let Some(view) = crate::channel_mute::presentation(&state.read(), &channel) else {
        return rsx! {};
    };
    let change_token = view.token.clone();
    let check_token = view.token.clone();
    let temporary_token = view.token.clone();
    let temporary_disabled = disabled || !view.can_temporary;
    let tomorrow_disabled = !view.can_mute_until_tomorrow;
    rsx! {
        section {class:"channel-mute-control","aria-label":"Channel notifications","aria-busy":view.busy,
            div {class:"channel-mute-actions",
                button {id:"channel-mute-action",r#type:"button",disabled:disabled||!view.can_change,
                    "aria-describedby":"channel-mute-status",
                    onclick:move |_|if change_alive.get()&&!disabled {crate::channel_mute::change(state,change_token.clone());},
                    {icon(Icon::Notifications,"icon sm")}
                    "{view.label}"
                }
                button {id:"channel-mute-temporary",r#type:"button",disabled:temporary_disabled,
                    "aria-expanded":choosing(),"aria-controls":"channel-mute-duration-controls",
                    title:view.temporary_reason.clone().unwrap_or_else(||"Choose how long to mute this channel".into()),
                    onclick:move |_|if show_alive.get()&&!temporary_disabled {let open=!choosing();duration.set("hour".into());choosing.set(open);},
                    "Mute temporarily"}
                if view.uncertain {
                    button {id:"channel-mute-check",r#type:"button",disabled:disabled||!view.can_check,
                        onclick:move |_|if alive.get()&&!disabled {crate::channel_mute::check(state,check_token.clone());},"Check status"}
                }
            }
            if choosing() {
                div {id:"channel-mute-duration-controls",class:"channel-mute-actions channel-mute-duration-controls",
                    label {r#for:"channel-mute-duration","Mute for"}
                    select {id:"channel-mute-duration",value:"{duration}",disabled:temporary_disabled,
                        onmounted:move |_|{dioxus::document::eval("document.getElementById('channel-mute-duration')?.focus();");},
                        onchange:move |event|duration.set(event.value()),
                        option {value:"hour","1 hour"}
                        option {value:"tomorrow",disabled:tomorrow_disabled,"Until tomorrow"}
                    }
                    button {id:"channel-mute-temporary-save",r#type:"button",
                        disabled:temporary_disabled||(duration()=="tomorrow"&&tomorrow_disabled),
                        onclick:move |_| {
                            if !temporary_alive.get()||temporary_disabled {return;}
                            let selected=match duration().as_str() {
                                "hour"=>api::TemporaryMuteDuration::OneHour,
                                "tomorrow" if !tomorrow_disabled=>api::TemporaryMuteDuration::UntilTomorrow,
                                _=>return,
                            };
                            choosing.set(false);
                            crate::channel_mute::temporarily_mute(state,temporary_token.clone(),selected);
                        },if duration()=="tomorrow" {"Mute until tomorrow"}else{"Mute for 1 hour"}}
                    button {id:"channel-mute-temporary-cancel",r#type:"button",onclick:move |_|{choosing.set(false);dioxus::document::eval("document.getElementById('channel-mute-temporary')?.focus();");},"Cancel"}
                }
                if let Some(reason)=view.temporary_reason.as_ref() {
                    p {id:"channel-mute-temporary-help",class:"scheduled-hint","{reason}"}
                }
            }
            p {id:"channel-mute-status",role:"status",class:"scheduled-hint",
                if let Some(message)=view.message {"{message}"}
                else if let Some(expiration)=view.temporary_until {
                    "Muted until {super_platinum_core::state::format_ts_date_label(&expiration.to_string())} at {super_platinum_core::state::format_ts_hm(&expiration.to_string())}."
                }
                else if view.muted {"Muted for you."}
                else {"Using your notification preferences."}
            }
        }
    }
}
pub(super) fn focus_dialog(id: &str) {
    let id = serde_json::to_string(id).unwrap();
    dioxus::document::eval(&format!(
        r#"const dialog=document.getElementById({id});if(dialog){{dialog.querySelector('button')?.focus();dialog.addEventListener('keydown',event=>{{if(event.key!=='Tab')return;const fields=[...dialog.querySelectorAll('button,input,textarea,select,[tabindex]')].filter(e=>!e.disabled&&e.tabIndex>=0&&e.getClientRects().length);if(!fields.length)return;const first=fields[0],last=fields.at(-1);if(event.shiftKey&&document.activeElement===first){{event.preventDefault();last.focus();}}else if(!event.shiftKey&&document.activeElement===last){{event.preventDefault();first.focus();}}}});}}"#
    ));
}
fn request_details_focus(mut request: Signal<(u64, bool)>, editor: bool) {
    if let Ok(mut value) = request.try_write() {
        *value = (value.0.wrapping_add(1), editor);
    }
}
fn metadata_revision(state: &Signal<ShellState>, ctx: &Context) -> u64 {
    state
        .peek()
        .core
        .workspaces
        .get(&ctx.session.team_id)
        .map(|w| w.channel_lifecycle.revision())
        .unwrap_or(0)
}
fn merge_info(mut state: Signal<ShellState>, ctx: &Context, info: &Channel, since: u64) -> Channel {
    if !ctx.current(&state.peek()) {
        return info.clone();
    }
    let mut shell = state.write();
    let effective = shell
        .core
        .workspaces
        .get_mut(&ctx.session.team_id)
        .map(|w| w.merge_channel_details(info.clone(), since))
        .unwrap_or_else(|| info.clone());
    shell.refresh_from_core();
    effective
}
async fn load_info(state: Signal<ShellState>, mut details: Signal<Details>, channel: String) {
    if crate::fixture::is_fixture() {
        details.write().loaded = true;
        return;
    }
    let Some(ctx) = Context::get(&state) else {
        details.write().error = "Connect to Slack to view channel details.".into();
        return;
    };
    if details.peek().loading {
        return;
    }
    details.write().loading = true;
    let read_revision = metadata_revision(&state, &ctx);
    let result =
        api::fetch_conversation_details(&ctx.transport, &ctx.client, &ctx.session, &channel).await;
    if !ctx.current(&state.peek()) {
        return;
    }
    details.write().loading = false;
    match result {
        Ok(info) => {
            let info = merge_info(state, &ctx, &info, read_revision);
            let mut data = details.write();
            data.info = Some(info);
            data.loaded = true;
            data.error.clear();
        }
        Err(error) => details.write().error = failure(&error),
    }
}
async fn save(
    mut state: Signal<ShellState>,
    mut details: Signal<Details>,
    ctx: &Context,
    channel: &str,
    field: ChannelTextField,
    expected: &str,
    value: &str,
) -> Result<(), String> {
    if !ctx.current(&state.peek()) {
        return Err("The active account changed.".into());
    }
    let normalized;
    let value = if field == ChannelTextField::Name {
        normalized = api::normalize_channel_name(value).map_err(str::to_owned)?;
        &normalized
    } else {
        value
    };
    let read_revision = metadata_revision(&state, ctx);
    let mut info =
        api::fetch_conversation_details(&ctx.transport, &ctx.client, &ctx.session, channel)
            .await
            .map_err(|e| failure(&e))?;
    if !ctx.current(&state.peek()) {
        return Err("The active account changed.".into());
    }
    info = merge_info(state, ctx, &info, read_revision);
    if field.value(&info) != expected {
        if let Ok(mut data) = details.try_write() {
            data.info = Some(info);
        }
        return Err("This field changed in Slack. Your edit is still here; cancel the edit to review the new value.".into());
    }
    let write_revision = metadata_revision(&state, ctx);
    api::set_conversation_text(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        channel,
        field,
        value,
    )
    .await
    .map_err(|e| failure(&e))?;
    if !ctx.current(&state.peek()) {
        return Ok(());
    }
    if field == ChannelTextField::Name {
        info.name = Some(value.into());
    } else {
        info.extra
            .insert(field.key().into(), serde_json::json!({"value":value}));
    }
    // Slack canonicalizes content such as Unicode emoji to :shortcodes:. Load
    // that canonical value before the next edit's conflict comparison.
    let mut result_revision = write_revision;
    let snapshot_revision = metadata_revision(&state, ctx);
    if let Ok(fresh) =
        api::fetch_conversation_details(&ctx.transport, &ctx.client, &ctx.session, channel).await
    {
        info = fresh;
        result_revision = snapshot_revision;
    }
    if !ctx.current(&state.peek()) {
        return Ok(());
    }
    info = merge_info(state, ctx, &info, result_revision);
    if let Ok(mut data) = details.try_write() {
        data.info = Some(info);
    }
    state
        .write()
        .show_toast(&format!("{} updated.", field.label()));
    crate::bootstrap::persist_workspace(&state, &ctx.session.team_id);
    Ok(())
}
async fn change_archive(
    mut state: Signal<ShellState>,
    mut details: Signal<Details>,
    ctx: &Context,
    channel: &str,
    archive: bool,
) -> Result<(), String> {
    if !ctx.current(&state.peek()) {
        return Err("The active account changed.".into());
    }
    let read_revision = metadata_revision(&state, ctx);
    let mut info =
        api::fetch_conversation_details(&ctx.transport, &ctx.client, &ctx.session, channel)
            .await
            .map_err(|e| failure(&e))?;
    if !ctx.current(&state.peek()) {
        return Err("The active account changed.".into());
    }
    info = merge_info(state, ctx, &info, read_revision);
    if info.is_archived == archive {
        if let Ok(mut data) = details.try_write() {
            data.info = Some(info);
        }
        return Err(
            "The channel's archive state changed in Slack. Cancel and review it before continuing."
                .into(),
        );
    }
    let write_revision = metadata_revision(&state, ctx);
    api::archive_conversation(&ctx.transport, &ctx.client, &ctx.session, channel, archive)
        .await
        .map_err(|e| failure(&e))?;
    if !ctx.current(&state.peek()) {
        return Ok(());
    }
    info.is_archived = archive;
    let mut result_revision = write_revision;
    let snapshot_revision = metadata_revision(&state, ctx);
    if let Ok(fresh) =
        api::fetch_conversation_details(&ctx.transport, &ctx.client, &ctx.session, channel).await
    {
        info = fresh;
        result_revision = snapshot_revision;
    }
    if !ctx.current(&state.peek()) {
        return Ok(());
    }
    info = merge_info(state, ctx, &info, result_revision);
    if let Ok(mut data) = details.try_write() {
        data.info = Some(info);
    }
    state.write().show_toast(if archive {
        "Channel archived."
    } else {
        "Channel unarchived."
    });
    crate::bootstrap::persist_workspace(&state, &ctx.session.team_id);
    Ok(())
}
pub(super) fn failure(error: &super_platinum_core::slack::Error) -> String {
    use super_platinum_core::slack::Error;
    match error{
        Error::Api(code) if matches!(code.as_str(),"no_permission"|"not_authorized"|"restricted_action"|"not_in_channel"|"user_is_restricted"|"missing_scope")=>"Slack did not permit this action.".into(),
        Error::Api(code) if code=="name_taken"=>"That channel name is already in use. Choose another name.".into(),
        Error::Api(code) if code.starts_with("invalid_name")=>"Slack rejected the channel name. Use up to 80 lowercase letters, numbers, hyphens or underscores.".into(),
        Error::Api(code) if code=="creation_workspace_unavailable"=>"Slack did not provide a workspace where you can create channels.".into(),
        Error::Api(code) if code=="too_long"=>"Slack rejected the length. Shorten the text and try again.".into(),
        Error::Api(code) if code=="is_archived"=>"This channel is archived.".into(),
        Error::Api(code) if code=="last_member"=>"You are the last member of this channel. Archive it instead, or have another member join before leaving.".into(),
        Error::Api(code) if code=="cant_leave_general"=>"Slack requires everyone to stay in this workspace's general channel.".into(),
        Error::Api(code) if code=="channel_not_found"=>"This channel is no longer available to your account.".into(),
        Error::Api(code) if code=="method_not_supported_for_channel_type"=>"Slack does not allow this action for this channel type.".into(),
        Error::Offline(_)=>"Reconnect to Slack and try again.".into(),
        Error::RateLimited{..}=>"Slack is limiting requests. Wait a moment before trying again.".into(),
        _=>"Slack could not confirm the result. Check the channel in Slack before trying again.".into()
    }
}
