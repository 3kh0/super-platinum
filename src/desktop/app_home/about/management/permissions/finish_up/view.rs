use super::*;
use crate::view::media_image::MediaImage;
pub(in crate::app_home::about::management) fn panel(
    state: Signal<ShellState>,
    visit: u64,
) -> Element {
    let s = state.read();
    let editing = s
        .app_home
        .about
        .management
        .permissions
        .confirmation
        .as_ref()
        .filter(|c| c.original.visit == visit && c.original.embedded);
    let attempted = document(&s, visit)
        .and_then(|d| d.pending)
        .and_then(|id| attempt(&s, id))
        .filter(|a| !a.closed);
    let (o, l) = if let Some(c) = editing {
        (&c.original, c.finish_up.as_ref().unwrap())
    } else if let Some(a) = attempted {
        (a.dialog_original.as_ref().unwrap_or(&a.original), &a.local)
    } else {
        return rsx! {};
    };
    let id = o.id;
    let busy = attempted.is_some_and(|a| !a.finished);
    let submitted = attempted.is_some();
    let ready = editing.is_some_and(|c| {
        l.valid()
            && super::super::save_enabled(&s, visit)
            && c.settings
                .as_ref()
                .is_none_or(|v| v.desired_embedded().is_ok())
    });
    let title = l.title.clone();
    let description = l.description.clone();
    let icon = l.icon_url.clone();
    let epoch = l.epoch;
    let copy = editing.map(|c| c.selected.clone()).unwrap_or_default();
    let team = o.owner.ctx.session.name.clone();
    let notice = l.notice.clone();
    let more = l.more;
    let caption = if l.defaults { "Next" } else { "Publish" };
    let complete = attempted.is_some_and(|a| a.complete);
    let add_submitted = attempted
        .and_then(|a| a.confirmation_add.as_ref())
        .is_some_and(|a| a.submitted());
    let phase = attempted.map(|a| a.phase).unwrap_or("editing");
    let link = attempted
        .and_then(|a| a.followup.as_ref())
        .and_then(|f| f.current_trigger())
        .and_then(|t| t["shortcut_url"].as_str())
        .map(str::to_owned);
    let upload = l.avatar.upload.clone();
    let avatar_pending = l.avatar.pending;
    let pane = if l.share {
        "share"
    } else {
        l.avatar.pane.name()
    };
    let details = !l.share && l.avatar.pane == avatar::Pane::Details;
    let upload_pane = l.avatar.pane == avatar::Pane::Upload;
    let crop_pane = l.avatar.pane == avatar::Pane::Crop;
    let pane_bound = l.avatar.pane_bound;
    let heading = if l.share {
        "Share this workflow in a message"
    } else if upload_pane {
        if avatar_pending {
            "Uploading your photo\u{2026}"
        } else if l
            .avatar
            .notice
            .as_deref()
            .is_some_and(|v| v != "Sorry! Something went wrong. Please try again.")
        {
            "Dang"
        } else {
            "Eek"
        }
    } else if crop_pane {
        "Crop your photo"
    } else if complete {
        "Your workflow is published"
    } else {
        "Finish Up"
    };
    let crop_disabled = !l.avatar.crop_bound || !l.avatar.cropper.ready;
    let crop_commit_pending = l.avatar.cropper.interaction_pending;
    let crop_min = l
        .avatar
        .cropper
        .geometry
        .as_ref()
        .and_then(|g| {
            let width = g["display"]["width"].as_f64()?;
            let height = g["display"]["height"].as_f64()?;
            Some(128_f64.min(width.min(height)) * g["naturalWidth"].as_f64()? / width)
        })
        .unwrap_or(128.);
    let avatar_notice = l.avatar.notice.clone();
    let (x, y, size) = (l.avatar.x, l.avatar.y, l.avatar.size);
    let crop_valid = upload.as_ref().is_some_and(|(_, _, w, h)| {
        super_platinum_core::slack::api::avatar::Crop::new(x, y, size, *w, *h).is_ok()
    });
    let rows = attempted.map(receipt::outcomes).unwrap_or_default();
    drop(s);
    rsx! {
      div{class:"modal-overlay workflow-finish-overlay",
       section{id:"workflow-finish-up",class:"modal workflow-finish-up",role:"dialog","aria-modal":"true","aria-labelledby":"workflow-finish-heading",
        "data-finish-owner":id,"data-finish-epoch":epoch,"data-finish-title":title.clone(),"data-finish-description":description.clone(),"data-finish-icon":icon.clone(),"data-finish-busy":busy.to_string(),"data-finish-pane":pane,"data-finish-bound":pane_bound.to_string(),
        onkeydown:move|e|if e.key()==dioxus::prelude::Key::Escape {if pane=="share" {e.stop_propagation();confirmation_share::cancel(state,visit,id)}else if !submitted{e.stop_propagation();actions::escape(state,visit,id)}},
        h2{id:"workflow-finish-heading",tabindex:"-1","{heading}"}
        div{id:"workflow-finish-details",class:"workflow-finish-pane",style:if details{"display: flex;"}else{"display: none;"},
         p{"Make sure the workflow details and permission are correct before publishing."}
         div{class:"workflow-finish-fields",
          label{r#for:"workflow-finish-title","Name"}
          input{id:"workflow-finish-title",r#type:"text",value:title.clone(),readonly:submitted||!details||!pane_bound,required:true,
           onmounted:move |_|super::bind_visible(state,visit,id),oninput:move|e|actions::edit(state,visit,id,"title",e.value())}
          if protocol::validate_name(&title).is_err(){p{role:"status","Enter a valid workflow name, up to 80 characters."}}
          label{r#for:"workflow-finish-description","Description"}
          textarea{id:"workflow-finish-description",value:description.clone(),readonly:submitted||!details||!pane_bound,required:true,
           oninput:move|e|actions::edit(state,visit,id,"description",e.value())}
          if protocol::description_invalid(&description,80){p{role:"status","Enter a description, up to 80 characters."}}
          label{r#for:"workflow-finish-upload","Upload workflow icon"}
          input{id:"workflow-finish-upload",r#type:"file",accept:"image/jpeg,image/gif,image/png",disabled:submitted||!details||!pane_bound,
           onchange:move|e|if let Some(file)=e.files().into_iter().next(){avatar::choose(state,visit,id,file.path());}}
         }
         section{class:"workflow-finish-preview","aria-label":"Workflow preview",
          {preview(state,&icon,"Workflow icon")}
          strong{"{title}"}p{"{description}"}
         }
         if !submitted{
          fieldset{class:"workflow-finish-settings",disabled:!details,
           section{id:"workflow-permissions","data-confirmation":id,
            {settings::panel(state,visit,id)}
            button{id:"workflow-finish-show-more",r#type:"button",hidden:more,"aria-expanded":more.to_string(),onclick:move |_|actions::edit(state,visit,id,"more",String::new()),"Show more permissions"}
            div{id:"workflow-finish-more",style:if more{"display: block;"}else{"display: none;"},
             label{r#for:"workflow-permissions-copy","Who can copy this workflow?"}
             select{id:"workflow-permissions-copy",value:copy,onmounted:move |_|super::super::bind(state,visit,id),onchange:move|e|super::super::select(state,visit,id,e.value()),
              option{value:"everyone","Everyone in {team}"}option{value:"collaborators","Workflow managers only"}}
             {managers::panel(state,visit,id)}
            }
           }
          }
         }
         div{id:"workflow-permissions-native-notice",role:"status"}
         if let Some(notice)=notice{p{id:"workflow-finish-notice",role:"status","{notice}"}}
         if busy{p{role:"status","Finishing workflow: {phase}\u{2026}"}}
         for row in rows{p{"{row}"}}
         if !submitted{
          div{class:"workflow-finish-pane-actions",
           button{id:"workflow-finish-cancel",r#type:"button",disabled:!pane_bound,onclick:move |_|cancel(state,visit,id),"Cancel"}
           button{id:"workflow-finish-submit",r#type:"button",disabled:!ready,onclick:move |_|submit(state,visit,id),"{caption}"}
          }
         }else{
          if let Some(link)=link{p{class:"workflow-finish-link","{link}"}button{id:"workflow-finish-copy-link",r#type:"button",onclick:move |_|copy_link(state,visit,id),"Copy link"}}
          button{id:"workflow-finish-check",r#type:"button",disabled:busy||add_submitted,onclick:move |_|super::check(state,visit,id),"Check current publication"}
          if complete{
           button{id:"workflow-confirmation-share-open",r#type:"button",disabled:add_submitted,onmounted:move |_|confirmation_share::bind_entry(state,visit,id),onclick:move |_|confirmation_share::open(state,visit,id),"Share"}
           {confirmation_add::panel(state,visit,id)}
          }else{
           button{id:"workflow-finish-done",r#type:"button",disabled:busy,onclick:move |_|super::close(state,visit,id),"Done"}
          }
         }
        }
        {confirmation_share::panel(state,visit,id)}
        section{id:"workflow-finish-upload-pane",class:"workflow-finish-pane",style:if upload_pane{"display: flex;"}else{"display: none;"},"aria-label":"Upload workflow icon",
         button{id:"workflow-finish-upload-back",r#type:"button",disabled:!pane_bound,onclick:move |_|avatar::back(state,visit,id,"upload-back"),"Back"}
         div{class:"workflow-finish-avatar-status",role:"status",
          if avatar_pending {p{"Uploading your photo\u{2026}"}} else if let Some(error)=avatar_notice.clone(){p{"{error}"}}
         }
         div{class:"workflow-finish-pane-actions",
          button{id:"workflow-finish-upload-cancel",r#type:"button",disabled:!pane_bound,onclick:move |_|actions::upload_cancel(state,visit,id),"Cancel"}
          button{id:"workflow-finish-upload-retry",r#type:"button",hidden:avatar_pending||avatar_notice.is_none(),style:if avatar_pending||avatar_notice.is_none(){"display: none;"}else{"display: inline-flex;"},disabled:!pane_bound,onclick:move |_|avatar::back(state,visit,id,"upload-retry"),"Try a Different Image"}
         }
        }
        section{id:"workflow-finish-crop-pane",class:"workflow-finish-pane",style:if crop_pane{"display: flex;"}else{"display: none;"},"aria-label":"Crop workflow icon",
         button{id:"workflow-finish-crop-back",r#type:"button",disabled:!pane_bound,onclick:move |_|avatar::back(state,visit,id,"crop-back"),"Back"}
         if let Some(error)=avatar_notice.clone(){p{role:"status","{error}"}}
         if avatar_pending{p{role:"status","Saving your photo\u{2026}"}}
         if let Some((_,url,w,h))=upload{
          div{id:"workflow-finish-crop",onmounted:move |_|actions::bind(state,visit,id),
           {cropper::surface(state,visit,id,&url)}
           p{"Precise crop coordinates in the {w} by {h} image"}
           label{r#for:"workflow-finish-crop-x","Left"}
           input{id:"workflow-finish-crop-x",r#type:"number",min:"0",max:w.to_string(),value:x.to_string(),disabled:crop_disabled,oninput:move|e|avatar::edit(state,visit,id,"crop-x",e.value())}
           label{r#for:"workflow-finish-crop-y","Top"}
           input{id:"workflow-finish-crop-y",r#type:"number",min:"0",max:h.to_string(),value:y.to_string(),disabled:crop_disabled,oninput:move|e|avatar::edit(state,visit,id,"crop-y",e.value())}
           label{r#for:"workflow-finish-crop-size","Size"}
           input{id:"workflow-finish-crop-size",r#type:"number",min:crop_min.to_string(),max:w.min(h).to_string(),value:size.to_string(),disabled:crop_disabled,oninput:move|e|avatar::edit(state,visit,id,"crop-size",e.value())}
          }
         }
         div{class:"workflow-finish-pane-actions",
          button{id:"workflow-finish-crop-cancel",r#type:"button",disabled:!pane_bound||!crop_pane,onclick:move |_|avatar::back(state,visit,id,"crop-cancel"),"Cancel"}
          button{id:"workflow-finish-crop-apply",r#type:"button",disabled:avatar_pending||crop_commit_pending||crop_disabled||!crop_valid,onclick:move |_|avatar::apply(state,visit,id),"Save"}
         }
        }
       }
      }
    }
}

fn preview(state: Signal<ShellState>, url: &str, alt: &str) -> Element {
    let s = state.read();
    if url.is_empty() {
        return rsx! {span{class:"workflow-finish-icon-fallback",role:"img","aria-label":alt,{crate::icons::icon(crate::icons::Icon::Description, "icon")}}};
    }
    let id = s.media.register_image(
        super_platinum_core::MediaAssetKind::Attachment,
        url,
        "image/png",
    );
    let epoch = s.media_epoch;
    let fallback = rsx! {span{class:"workflow-finish-icon-fallback",role:"img","aria-label":alt,{crate::icons::icon(crate::icons::Icon::Description, "icon")}}};
    rsx! {MediaImage{state,id,media_epoch:epoch,class:"workflow-finish-icon",alt:alt.to_owned(),fallback}}
}
fn copy_link(state: Signal<ShellState>, visit: u64, id: u64) {
    let captured = {
        let s = state.peek();
        attempt(&s, id)
            .filter(|a| a.original.visit == visit && a.complete && a.original.owner.current(&s))
            .and_then(|a| a.followup.as_ref())
            .and_then(|f| f.current_trigger())
            .and_then(|t| t["shortcut_url"].as_str())
            .map(str::to_owned)
    };
    if let Some(link) = captured {
        dioxus::core::spawn_forever(async move {
            let _ = document::eval(&format!(
                "await navigator.clipboard.writeText({});",
                json!(link)
            ))
            .await;
        });
    }
}
