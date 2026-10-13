use crate::{
    icons::{Icon, icon},
    scheduled::{self, Context},
    state::{ComposerTarget, ShellState},
};
use dioxus::prelude::*;
use super_platinum_core::{scheduled::DraftFilter, slack::api};
const DRAFT_CONFLICT: &str = "This draft changed in Slack. Your edits are still here. You can save them as a separate draft.";

#[derive(Clone, PartialEq)]
pub(crate) struct ComposeSource {
    pub epoch: u64,
    pub team: String,
    pub channel: String,
    pub thread: Option<String>,
    pub text: String,
    pub reply_broadcast: bool,
    pub attachments: Vec<(u64, std::path::PathBuf, String)>,
}
impl ComposeSource {
    fn key(&self) -> super_platinum_core::drafts::DraftKey {
        super_platinum_core::drafts::DraftKey {
            team: self.team.clone(),
            channel: self.channel.clone(),
            thread: self.thread.clone(),
        }
    }
    pub(crate) fn capture(shell: &ShellState, target: ComposerTarget) -> Option<Self> {
        let thread = match target {
            ComposerTarget::Channel => None,
            ComposerTarget::Thread => Some(shell.thread_root.clone()?),
        };
        let (text, attachments) = if thread.is_some() {
            (
                &shell.core.thread_composer.text,
                &shell.core.thread_composer_attachments,
            )
        } else {
            (&shell.core.composer.text, &shell.core.composer_attachments)
        };
        if text.trim().is_empty() && attachments.is_empty() {
            return None;
        }
        Some(Self {
            epoch: shell.core.account_epoch,
            team: shell.core.active_team.clone()?,
            channel: shell.core.active_channel.clone()?,
            thread,
            text: text.clone(),
            reply_broadcast: target == ComposerTarget::Thread
                && shell.core.thread_composer.reply_broadcast,
            attachments: attachments
                .iter()
                .map(|a| (a.id, a.path.clone(), a.name.clone()))
                .collect(),
        })
    }
    pub(crate) fn request(&self, shell: &ShellState, post_at: i64) -> api::ScheduleMessage {
        let mut request = api::ScheduleMessage::new(
            self.channel.clone(),
            self.thread.clone(),
            compose_blocks(shell, &self.text),
            post_at,
        );
        if self.thread.is_some() && self.reply_broadcast {
            request.destinations[0]["broadcast"] = serde_json::Value::Bool(true);
        }
        request
    }
    fn clear_if_unchanged(&self, shell: &mut ShellState) -> Option<&'static str> {
        if shell.core.account_epoch != self.epoch {
            return None;
        }
        let key = super_platinum_core::drafts::DraftKey {
            team: self.team.clone(),
            channel: self.channel.clone(),
            thread: self.thread.clone(),
        };
        let target = if self.thread.is_some() {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        };
        if shell.composer_key(target).as_ref() != Some(&key) {
            if let Some(mut draft) = shell.core.drafts.items.get(&key).cloned() {
                draft
                    .attachments
                    .retain(|a| !self.attachments.iter().any(|(id, _, _)| *id == a.id));
                if draft.composer.text == self.text
                    && draft.composer.reply_broadcast == self.reply_broadcast
                {
                    draft.composer = Default::default();
                }
                shell
                    .core
                    .drafts
                    .capture(&key, &draft.composer, &draft.attachments);
            }
            return None;
        }
        if shell.core.active_team.as_ref() != Some(&self.team)
            || shell.core.active_channel.as_ref() != Some(&self.channel)
        {
            return None;
        }
        let (composer, attachments, id) = if let Some(root) = &self.thread {
            if shell.thread_root.as_ref() != Some(root) {
                return None;
            }
            (
                &mut shell.core.thread_composer,
                &mut shell.core.thread_composer_attachments,
                "thread-composer",
            )
        } else {
            (
                &mut shell.core.composer,
                &mut shell.core.composer_attachments,
                "channel-composer",
            )
        };
        // Preserve input typed while the request was in flight, and new attachments.
        attachments.retain(|a| !self.attachments.iter().any(|(id, _, _)| *id == a.id));
        if composer.text == self.text && composer.reply_broadcast == self.reply_broadcast {
            *composer = Default::default();
            Some(id)
        } else {
            None
        }
    }
}
#[component]
pub(crate) fn ScheduleButton(state: Signal<ShellState>, target: ComposerTarget) -> Element {
    let mut source = use_signal(|| None::<ComposeSource>);
    rsx! {
     button{class:"composer-icon-btn",title:"Schedule or save draft","aria-label":"Schedule or save draft",disabled:!can_schedule(&state.read(),target),onclick:move |_|source.set(ComposeSource::capture(&state.peek(),target)),{icon(Icon::Schedule,"icon sm")}}
     if let Some(current)=source(){ScheduleDialog{state,source:Some(current),onclose:move |_|source.set(None)}}
    }
}
#[component]
pub(crate) fn ScheduledPanel(mut state: Signal<ShellState>) -> Element {
    let mut editing = use_signal(|| {
        if matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("scheduled-edit" | "drafts-edit" | "drafts-conflict")
        ) {
            Some(("DrFixture".to_owned(), false))
        } else {
            None::<(String, bool)>
        }
    });
    let mut confirmation = use_signal(|| {
        if std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("scheduled-send-confirm") {
            state
                .peek()
                .core
                .scheduled
                .items
                .get("DrFixture")
                .cloned()
                .map(|draft| (draft, true))
        } else {
            None::<(super_platinum_core::scheduled::ScheduledMessage, bool)>
        }
    });
    let shell = state.read();
    let queue = scheduled::visible(&shell);
    let loading = queue.is_some_and(|q| q.loading);
    let filter = queue
        .map(|q| q.filter)
        .unwrap_or(shell.core.scheduled.filter);
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t));
    let local_count = shell
        .core
        .drafts
        .items
        .keys()
        .filter(|key| Some(&key.team) == shell.core.active_team.as_ref())
        .count()
        + crate::drafts::session::recovery_rows(&shell).len();
    let counts = [DraftFilter::Drafts, DraftFilter::Scheduled].map(|tab| {
        (
            tab,
            queue
                .map(|q| q.items.values().filter(|m| tab.accepts(m)).count())
                .unwrap_or(0)
                + if tab == DraftFilter::Drafts {
                    local_count
                } else {
                    0
                },
        )
    });
    let empty = (filter != DraftFilter::Drafts || local_count == 0)
        && queue.is_some_and(|q| q.loaded && q.sorted().is_empty())
        && !loading;
    rsx! {
      section{class:"scheduled-panel","aria-label":"Drafts and scheduled messages",
        header{class:"list-panel-header",h2{"Drafts & scheduled"}span{class:"spacer"}button{class:"later-refresh",disabled:loading,onclick:move |_|{spawn(scheduled::load(state,false));},"Refresh"}}
        div{class:"later-tabs",role:"tablist","aria-label":"Draft status",
          for (tab,count) in counts{
            button{role:"tab","aria-selected":tab==filter,class:if tab==filter{"active"}else{""},onclick:move |_|{state.write().core.scheduled.filter=tab;confirmation.set(None);},"{tab.label()}"
              span{"{count}"}
            }
          }
        }
        div{class:"scheduled-body",
          if let Some(error)=queue.and_then(|q|q.error.as_ref()){p{class:"later-error",role:"alert","{error}"}}
          if loading{p{class:"files-status",role:"status","Loading drafts…"}}
          if filter==DraftFilter::Drafts {crate::forward::inventory::SavedForwards{state}super::local_drafts::LocalDraftRows{state}}
          if let Some(queue)=queue{
            for item in queue.sorted(){
              {let id=item.id.clone();let edit=id.clone();let schedule=id.clone();let confirm=item.clone();let send=item.clone();let unschedule=id.clone();
               let name=destination_label(&shell,&item.destinations);let preview=workspace.map(|w|crate::message_vm::message_vm_with_pending(w,&item.preview(),&shell.media,false));let busy=queue.busy.contains(&id);
               let pending=confirmation.read().as_ref().filter(|(target,_)|target.id==id).cloned();
               rsx!{article{class:"scheduled-item",key:"{id}","aria-busy":busy,
                 div{class:"scheduled-meta",strong{if name.is_empty(){"No destination"}else{"{name}"}}span{class:"spacer"}
                   if item.scheduled(){time{"{super_platinum_core::state::format_ts_date_label(&item.post_at.to_string())} · {super_platinum_core::state::format_ts_hm(&item.post_at.to_string())}"}}else{span{"Not scheduled"}}
                 }
                 div{class:"scheduled-content",if let Some(preview)=preview{for node in &preview.body{{super::rich::rich_node(node,shell.media_epoch,state)}}}}
                 if !item.file_ids.is_empty(){p{class:"scheduled-hint","{attachment_label(item.file_ids.len())}"}}
                 div{class:"scheduled-actions",
                   if let Some((reviewed,sending))=pending{
                     span{if sending{"Send this message now?"}else{"Delete this message?"}}
                     button{class:if sending{"primary"}else{"danger"},disabled:busy,onclick:move |_|{confirmation.set(None);if sending{dioxus::core::spawn_forever(scheduled::send_reviewed(state,reviewed.clone()));}else{dioxus::core::spawn_forever(scheduled::remove(state,reviewed.clone()));}},if sending{"Send now"}else{"Delete"}}
                     button{onclick:move |_|confirmation.set(None),"Cancel"}
                   }else{
                     button{disabled:busy,onclick:move |_|editing.set(Some((edit.clone(),false))),if item.scheduled(){"Edit or reschedule"}else{"Edit draft"}}
                     button{disabled:busy||!item.can_send(),onclick:move |_|confirmation.set(Some((send.clone(),true))),"Send now"}
                     if item.scheduled(){button{disabled:busy,onclick:move |_|{dioxus::core::spawn_forever(scheduled::act(state,unschedule.clone(),scheduled::DraftAction::Unschedule));},"Save to drafts"}}
                     else{button{disabled:busy||!item.can_send(),onclick:move |_|editing.set(Some((schedule.clone(),true))),"Schedule"}}
                     button{disabled:busy,onclick:move |_|confirmation.set(Some((confirm.clone(),false))),"Delete"}
                   }
                 }
                 if !item.can_send(){p{class:"scheduled-hint","Open this draft in Slack to set its destination or send to multiple conversations."}}
               }}
              }
            }
            if empty{div{class:"scheduled-empty",h3{if filter==DraftFilter::Drafts{"No saved drafts"}else{"No scheduled messages"}}p{"Drafts are saved as you type. Use the clock beside Send to schedule a message."}}}
            if queue.has_more{button{class:"later-more",disabled:loading,onclick:move |_|{spawn(scheduled::load(state,true));},"Load more"}}
          }
        }
      }
      if let Some((id,schedule))=editing(){ScheduleDialog{key:"{id}-{schedule}",state,draft_id:Some(id),force_schedule:schedule,onclose:move |_|editing.set(None)}}
    }
}

#[component]
fn ScheduleDialog(
    mut state: Signal<ShellState>,
    #[props(default)] source: Option<ComposeSource>,
    #[props(default)] draft_id: Option<String>,
    #[props(default)] force_schedule: bool,
    onclose: EventHandler,
) -> Element {
    let initial = use_hook(|| {
        draft_id
            .as_ref()
            .and_then(|id| state.peek().core.scheduled.items.get(id).cloned())
    });
    let scope = use_hook(|| {
        source
            .as_ref()
            .map(|s| (s.epoch, Some(s.team.clone())))
            .unwrap_or_else(|| {
                (
                    state.peek().core.account_epoch,
                    state.peek().core.active_team.clone(),
                )
            })
    });
    let mut request = use_signal(|| {
        if let Some(draft) = &initial {
            api::ScheduleMessage::from_draft(draft)
        } else {
            let Some(source) = source.as_ref() else {
                return api::ScheduleMessage::new(String::new(), None, vec![], 0);
            };
            source.request(&state.peek(), super_platinum_core::state::now_secs() + 3600)
        }
    });
    let mut date = use_signal(String::new);
    let mut original_date = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let fixture_conflict =
        use_hook(|| std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("drafts-conflict"));
    let mut error = use_signal(|| {
        if fixture_conflict {
            DRAFT_CONFLICT.into()
        } else {
            String::new()
        }
    });
    let mut conflicted = use_signal(|| fixture_conflict);
    let mut copying = use_signal(|| false);
    // Keep one identity across explicit retries of this copy.
    let copy_id = use_hook(|| request.peek().unscheduled_copy().client_msg_id);
    let original_time = use_hook(|| {
        if request.peek().post_at > 0 {
            request.peek().post_at
        } else {
            super_platinum_core::state::now_secs() + 3600
        }
    });
    let mut scheduling = use_signal(|| {
        force_schedule || source.is_some() || initial.as_ref().is_some_and(|m| m.scheduled())
    });
    let blocks = serde_json::to_string(&request.peek().blocks).unwrap();
    let existing = draft_id.is_some();
    let title = if existing {
        "Edit message"
    } else {
        "Schedule message"
    };
    let destination = destination_label(&state.peek(), &request.peek().destinations);
    let close_source = source.clone();
    let file_count = source
        .as_ref()
        .map(|s| s.attachments.len())
        .unwrap_or(request.peek().file_ids.len());
    if (
        state.read().core.account_epoch,
        state.read().core.active_team.clone(),
    ) != scope
        || (initial.is_none() && source.is_none())
    {
        return rsx! {};
    }
    rsx! {
     div{class:"scrim",onclick:move |_|if !saving(){close_dialog(onclose)}}
     section{class:"modal schedule-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"schedule-title","aria-busy":saving(),
      onkeydown:move |e|if e.key()==Key::Escape{e.stop_propagation();if !saving(){close_dialog(onclose)}},
      onmounted:move |_|{spawn(async move{if let Some(value)=super::new_reminder::local_date_value(original_time).await{date.set(value.clone());original_date.set(value);}});},
      header{h2{id:"schedule-title","{title}"}button{"aria-label":"Close scheduling",disabled:saving(),onclick:move |_|close_dialog(onclose),{icon(Icon::Close,"icon")}}}
      p{class:"scheduled-hint","To {destination}"}
      super::rich_editor::RichEditor{id:"schedule-message",label:"Message",blocks,disabled:saving()}
      if file_count>0{p{class:"scheduled-hint","{attachment_label(file_count)} will be included."}}
      label{class:"schedule-toggle",input{r#type:"checkbox",checked:scheduling()&&!copying(),disabled:saving()||copying(),onchange:move |e|scheduling.set(e.checked())}span{"Schedule for later"}}
      if scheduling()&&!copying(){
      div{class:"schedule-presets",button{disabled:saving(),onclick:move |_|{spawn(async move{let mut eval=dioxus::document::eval("const d=new Date();d.setDate(d.getDate()+1);d.setHours(9,0,0,0);dioxus.send(Math.floor(d.getTime()/1000));");if let Ok(t)=eval.recv::<i64>().await{if let Some(v)=super::new_reminder::local_date_value(t).await{date.set(v);}}});},"Tomorrow at 9:00"}}
      label{r#for:"schedule-date","Date and time"}input{id:"schedule-date",r#type:"datetime-local",value:"{date}",disabled:saving(),oninput:move |e|date.set(e.value())}
      p{class:"scheduled-hint","Uses your local time zone. Messages can be scheduled up to 120 days ahead."}
      }
      if !error.read().is_empty(){p{class:"later-error",role:"alert","{error}"}}
      if conflicted(){label{class:"schedule-toggle",input{r#type:"checkbox",checked:copying(),disabled:saving(),onchange:move |e|copying.set(e.checked())}span{"Save my edits as a separate, unscheduled draft"}}}
      footer{button{disabled:saving(),onclick:move |_|{close_dialog(onclose);if let Some(source)=&close_source{super::composer::focus_field_when_mounted(if source.thread.is_some(){"thread-composer"}else{"channel-composer"});}},"Cancel"}
       button{class:"primary",disabled:saving()||(scheduling()&&!copying()&&date.read().is_empty()),onclick:move |_|{
        if saving(){return;}saving.set(true);error.set(String::new());let as_copy=copying();let copy_id=copy_id.clone();let schedule=scheduling()&&!as_copy;let date_value=date();let same_date=date_value==original_date();let source=source.clone();let id=if as_copy{None}else{draft_id.clone()};let scope=scope.clone();let expected=if as_copy{None}else{initial.clone()};
        dioxus::core::spawn_forever(async move{
         let value=serde_json::to_string(&date_value).unwrap();
         let mut eval=dioxus::document::eval(&format!("try{{const time=Date.parse({value});dioxus.send({{time:Number.isFinite(time)?Math.floor(time/1000):0,blocks:window.superPlatinumRichEditor.read('schedule-message'),text:document.getElementById('schedule-message').textContent}})}}catch(error){{dioxus.send({{error:error.name}})}} await dioxus.recv();"));
         // The pinned desktop bridge frees completed eval channels during JS GC.
         // Keep this channel alive until Rust has consumed its editor snapshot.
         let read=eval.recv::<serde_json::Value>().await.unwrap_or_default();
         let _=eval.send(());
         let time=if !schedule{0}else if same_date{original_time}else{read.get("time").and_then(|v|v.as_i64()).unwrap_or(0)};
         let result=async{
          if schedule{super_platinum_core::scheduled::validate_time(time,super_platinum_core::state::now_secs()).map_err(str::to_owned)?;}
          let blocks=read.get("blocks").and_then(|v|v.as_array()).cloned().ok_or("The message could not be read.")?;
          if read.get("text").and_then(|v|v.as_str()).is_none_or(|t|t.trim().is_empty())&&file_count==0{return Err("Add a message or a file.".into());}
          let ctx=Context::get(&state).ok_or("Connect to Slack to schedule this message.")?;
          if (ctx.epoch,Some(ctx.session.team_id.clone()))!=scope{return Err("The active account changed.".into());}
          if let Some(source)=&source{crate::drafts::integration::legacy_allowed(&state.peek(),&source.key()).map_err(str::to_owned)?;}
          if let Some(expected)=&expected{
           let limit=state.peek().core.scheduled.limit;
           let fetched=api::fetch_active_draft(&ctx.transport,&ctx.client,&ctx.session,&expected.id,limit).await;
           if !ctx.current(&state.peek()){return Err("The active account changed.".into());}
           let current=match fetched{Ok(draft)=>draft,Err(super_platinum_core::slack::Error::Api(code)) if code=="draft_not_active"=>{if let Ok(mut value)=conflicted.try_write(){*value=true;}return Err("This draft was sent or deleted. You can save your edits as a separate draft.".into());},Err(error)=>return Err(scheduled::failure(&error))};
           if !state.write().core.scheduled.verify_revision(expected,current){if let Ok(mut value)=conflicted.try_write(){*value=true;}return Err(DRAFT_CONFLICT.into());}
          }
          let mut args=request.peek().clone();args.blocks=blocks;args.post_at=time;
          if as_copy{args.client_msg_id=copy_id;}
          if let Some(source)=&source{if !source.attachments.is_empty()&&args.file_ids.is_empty(){
           let files=source.attachments.iter().map(|(_,path,_)|(path.clone(),std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)))).collect();
           args.file_ids=api::upload_unshared_files(&ctx.transport,&ctx.client,&ctx.session,files,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))).await.map_err(|e|scheduled::failure(&e))?;
          }}
          if !ctx.current(&state.peek()){return Err("The active account changed.".into());}
          if let Ok(mut value)=request.try_write(){*value=args.clone();}
          // Ownership may change during draft verification or attachment upload.
          // Recheck the exact captured destination immediately before dispatch.
          if let Some(source)=&source{crate::drafts::integration::legacy_allowed(&state.peek(),&source.key()).map_err(str::to_owned)?;}
          let saved=api::schedule_message(&ctx.transport,&ctx.client,&ctx.session,&args,id.as_deref()).await.map_err(|e|scheduled::failure(&e))?;
          if !ctx.current(&state.peek()){return Ok(());}
          scheduled::saved(state,&ctx,saved);
          if let Some(source)=&source{let clear={let mut shell=state.write();let clear=source.clear_if_unchanged(&mut shell);shell.checkpoint_composers();clear};if let Some(field)=clear{super::composer::set_field_text(field,"");}}
          if saving.try_read().is_ok(){close_dialog(onclose);}Ok::<(),String>(())
         }.await;
         if let Ok(mut value)=saving.try_write(){*value=false;}
         if let Err(message)=result{if let Ok(mut value)=error.try_write(){*value=message;}}
        });
       },if saving(){"Saving…"}else if copying(){"Save copy"}else if !scheduling(){"Save draft"}else if existing{"Save changes"}else{"Schedule message"}}
      }
     }
    }
}

fn attachment_label(count: usize) -> String {
    format!(
        "{count} attached {}",
        if count == 1 { "file" } else { "files" }
    )
}
fn close_dialog(onclose: EventHandler) {
    dioxus::document::eval("window.superPlatinumRichEditor?.restoreFocus('schedule-message');");
    onclose.call(());
}
pub(super) fn destination_label(shell: &ShellState, destinations: &[serde_json::Value]) -> String {
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t));
    destinations
        .iter()
        .map(|d| {
            let channel = d
                .get("channel_id")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown conversation");
            let name = workspace
                .and_then(|w| {
                    w.channels.get(channel).map(|c| {
                        let name = super_platinum_core::state::channel_display_name(w, c);
                        if c.is_im || c.is_mpim {
                            name
                        } else {
                            format!("#{name}")
                        }
                    })
                })
                .unwrap_or_else(|| channel.into());
            if d.get("thread_ts").and_then(|v| v.as_str()).is_some() {
                format!("{name} · Thread reply")
            } else {
                name
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
fn can_schedule(shell: &ShellState, target: ComposerTarget) -> bool {
    let Some(key) = shell.composer_key(target) else {
        return false;
    };
    if crate::drafts::integration::legacy_allowed(shell, &key).is_err() {
        return false;
    }
    match target {
        ComposerTarget::Channel => {
            !shell.core.composer.text.trim().is_empty()
                || !shell.core.composer_attachments.is_empty()
        }
        ComposerTarget::Thread => {
            shell.thread_root.is_some()
                && (!shell.core.thread_composer.text.trim().is_empty()
                    || !shell.core.thread_composer_attachments.is_empty())
        }
    }
}
fn compose_blocks(shell: &ShellState, text: &str) -> Vec<serde_json::Value> {
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
    else {
        return vec![];
    };
    let nodes =
        crate::blocks::mrkdwn_blocks(crate::blocks::BlockCtx::new(workspace, &shell.media), text);
    vec![
        serde_json::json!({"type":"rich_text","elements":nodes.iter().map(to_slack).collect::<Vec<_>>()}),
    ]
}
fn to_slack(node: &crate::state::RichNode) -> serde_json::Value {
    use crate::state::RichNode as N;
    use serde_json::json;
    match node {
        N::Text(text) => json!({"type":"text","text":text}),
        N::StyledText {
            text,
            bold,
            italic,
            strike,
            code,
        } => {
            json!({"type":"text","text":text,"style":{"bold":bold,"italic":italic,"strike":strike,"code":code}})
        }
        N::Link { label, url } => json!({"type":"link","text":label,"url":url}),
        N::UserMention { user_id, .. } => json!({"type":"user","user_id":user_id}),
        N::ChannelMention { channel_id, .. } => json!({"type":"channel","channel_id":channel_id}),
        N::GroupMention { group_id, .. } => json!({"type":"usergroup","usergroup_id":group_id}),
        N::Emoji { name, .. } | N::EmojiImage { name, .. } => {
            json!({"type":"emoji","name":name.trim_matches(':')})
        }
        N::Paragraph(children) => {
            json!({"type":"rich_text_section","elements":children.iter().map(to_slack).collect::<Vec<_>>()})
        }
        N::Code(text) => {
            json!({"type":"rich_text_preformatted","elements":[{"type":"text","text":text}]})
        }
        N::Quote(children) => {
            json!({"type":"rich_text_quote","elements":children.iter().flat_map(|n|match n{N::Paragraph(c)=>c.iter().map(to_slack).collect::<Vec<_>>(),_=>vec![to_slack(n)]}).collect::<Vec<_>>()})
        }
        _ => json!({"type":"text","text":node.plain_text()}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduling_clears_only_the_original_composer_and_original_attachments() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.composer.text = "Original 🚀".into();
        let source = ComposeSource::capture(&shell, ComposerTarget::Channel).unwrap();
        shell.core.composer.text = "A newer draft".into();
        assert_eq!(source.clear_if_unchanged(&mut shell), None);
        assert_eq!(shell.core.composer.text, "A newer draft");
        shell.core.composer.text = source.text.clone();
        shell.core.account_epoch += 1;
        assert_eq!(source.clear_if_unchanged(&mut shell), None);
        assert_eq!(shell.core.composer.text, source.text);
        shell.core.account_epoch -= 1;
        assert_eq!(
            source.clear_if_unchanged(&mut shell),
            Some("channel-composer")
        );
        assert!(shell.core.composer.text.is_empty());
    }
    #[test]
    fn successful_schedule_keeps_new_attachments_and_other_thread_drafts() {
        fn attachment(id: u64) -> super_platinum_core::domain::ComposerAttachment {
            super_platinum_core::domain::ComposerAttachment {
                id,
                path: format!("fixture-{id}.png").into(),
                name: format!("fixture-{id}.png"),
                bytes: 0,
                uploading: false,
                upload_started: None,
                upload_cancel: None,
                upload_progress: None,
                preview_path: None,
            }
        }
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.composer_attachments.push(attachment(1));
        let source = ComposeSource::capture(&shell, ComposerTarget::Channel).unwrap();
        shell.core.composer_attachments.push(attachment(2));
        source.clear_if_unchanged(&mut shell);
        assert_eq!(
            shell
                .core
                .composer_attachments
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        shell.thread_root = Some("root1".into());
        shell.core.thread_composer.text = "Reply one".into();
        let reply = ComposeSource::capture(&shell, ComposerTarget::Thread).unwrap();
        shell.thread_root = Some("root2".into());
        shell.core.thread_composer.text = "Reply two".into();
        assert!(reply.clear_if_unchanged(&mut shell).is_none());
        assert_eq!(shell.core.thread_composer.text, "Reply two");
    }
    #[test]
    fn native_formatting_roundtrips_into_rich_schedule_blocks() {
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let blocks = compose_blocks(
            &shell,
            "*Bold* and _italic_ 🚀\n> quoted\n```\nlet x = 1;\n```",
        );
        let elements = blocks[0]["elements"].as_array().unwrap();
        assert_eq!(elements[0]["elements"][0]["style"]["bold"], true);
        assert!(elements.iter().any(|e| e["type"] == "rich_text_quote"));
        assert!(
            elements
                .iter()
                .any(|e| e["type"] == "rich_text_preformatted")
        );
        assert!(blocks[0].to_string().contains('🚀'));
    }
    #[test]
    fn scheduling_after_navigation_clears_only_the_stored_source_draft() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.sync_composer_drafts();
        shell.core.composer = super_platinum_core::ComposerState::with_text("Schedule this");
        let source = ComposeSource::capture(&shell, ComposerTarget::Channel).unwrap();
        let key = shell.composer_key(ComposerTarget::Channel).unwrap();
        let other = if shell.active_channel == 0 { 1 } else { 0 };
        shell.select_channel(other, crate::state::ChannelOpen::Global);
        shell.core.composer = super_platinum_core::ComposerState::with_text("Keep new channel");
        assert!(source.clear_if_unchanged(&mut shell).is_none());
        assert!(!shell.core.drafts.items.contains_key(&key));
        assert_eq!(shell.core.composer.text, "Keep new channel");
    }
    #[test]
    fn scheduled_thread_captures_choice_and_does_not_clear_a_newer_choice() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.thread_root = Some("100.000001".into());
        shell.core.thread_composer.text = "Reply".into();
        shell.core.thread_composer.reply_broadcast = true;
        let source = ComposeSource::capture(&shell, ComposerTarget::Thread).unwrap();
        assert!(source.reply_broadcast);
        let request = source.request(&shell, 2000000000);
        assert_eq!(request.destinations[0]["broadcast"], true);
        assert_eq!(request.destinations[0]["thread_ts"], "100.000001");
        shell.core.thread_composer.reply_broadcast = false;
        assert_eq!(source.clear_if_unchanged(&mut shell), None);
        assert_eq!(shell.core.thread_composer.text, "Reply");
        shell.core.thread_composer.reply_broadcast = true;
        assert_eq!(
            source.clear_if_unchanged(&mut shell),
            Some("thread-composer")
        );
        assert!(!shell.core.thread_composer.reply_broadcast);
    }
}
