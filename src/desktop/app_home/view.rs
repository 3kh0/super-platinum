pub(crate) use super::actions::Key;
use super::*;
use super_platinum_core::block_actions::BlockElementMetadata;
pub(crate) fn panel(state: Signal<ShellState>) -> Element {
    let s = state.read();
    let Some(h) = s.app_home.surface.as_ref().filter(|h| h.owner.current(&s)) else {
        return rsx! {section{class:"app-home",p{"Open Home to load this app."}button{onclick:move |_|{dioxus::core::spawn_forever(refresh(state));},"Load Home"}}};
    };
    let key = Key {
        visit: h.id,
        revision: h.revision,
    };
    let loading = h.loading;
    let notice = h.notice.clone();
    let form = inputs::form(&s).cloned();
    let unavailable = !inputs::supported(&s);
    let editable = inputs::editable(&s);
    let form_dirty = form.as_ref().is_some_and(|f| f.dirty());
    let mut blocks = Vec::new();
    let mut shown = std::collections::BTreeSet::new();
    if let Some(v) = h.view.as_ref() {
        for block in v.raw()["blocks"].as_array().unwrap() {
            let key = super_platinum_core::block_actions::form::InputKey {
                block_id: block["block_id"].as_str().unwrap_or_default().into(),
                action_id: block["element"]["action_id"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
            };
            if block["type"] == "input"
                && form.as_ref().is_some_and(|f| f.fields.contains_key(&key))
            {
                shown.insert(key.clone());
                blocks.push(HomeBlock::Input(key));
            } else if let Some(w) = s.core.workspaces.get(&h.owner.ctx.session.team_id) {
                blocks.push(HomeBlock::Display(crate::blocks::block_nodes(
                    crate::blocks::BlockCtx::new(w, &s.media),
                    block,
                )));
            }
        }
    }
    // A server removal/type conflict cannot hide retained unsent local input.
    if let Some(f) = form.as_ref() {
        for (key, field) in &f.fields {
            if field.dirty && !shown.contains(key) {
                blocks.push(HomeBlock::Input(key.clone()));
            }
        }
    }
    let calendar_confirm = h.calendar_confirmation.is_some();
    let confirm = h
        .confirmation
        .as_ref()
        .and_then(|b| b.raw().get("confirm"))
        .cloned()
        .or_else(|| calendar::confirmation(h));
    let epoch = s.media_epoch;
    let locked = loading || workflows::locked(&s) || confirm.is_some() || cooling(&s, &h.owner.ctx);
    drop(s);
    rsx! {section{id:"app-home",class:"app-home","aria-label":"App Home","data-visit":key.visit,"data-revision":key.revision,
     div{class:"app-home-toolbar",h2{"Home"}span{class:"spacer"}button{disabled:locked,onclick:move |_|{dioxus::core::spawn_forever(refresh(state));},"Refresh"}}
     div{class:"app-home-content","aria-busy":loading,
      if loading{p{role:"status","Loading Home…"}}
      form{onsubmit:move |event|event.prevent_default(),
       for block in &blocks {match block {
        HomeBlock::Display(nodes)=>rsx!{div{for node in nodes{{crate::view::rich::rich_node_for_home(node,epoch,state,key)}}}},
        HomeBlock::Input(input_key)=>{if let Some(field)=form.as_ref().and_then(|f|f.fields.get(input_key)) {
         let shape=crate::view::app_modal::control_shape(&field.kind);
         let dom_id=crate::view::app_modal::field_id(input_key);
         rsx!{crate::view::app_modal::ModalField {key:"{key.visit}-{key.revision}-{dom_id}-{shape}-{field.revision}",state,input_key:input_key.clone(),field:field.clone(),locked:locked||!editable,home:Some(key),home_raw:form.as_ref().and_then(|f|f.native_error_text(input_key)).map(str::to_owned)}}
        }else{rsx!{}}}
       }}
      }
      if unavailable{p{class:"app-home-notice",role:"status","This Home includes controls that are not supported here yet. Its actions are disabled to preserve the full form state."}}
     }
     button{disabled:locked||!form_dirty,onclick:move |_|inputs::reset(state,key),"Reset inputs"}
     {calendar::status(state,key)}
     {navigation::status(state)}
     {workflows::view::panel(state)}
     if let Some(notice)=notice{p{class:"app-home-notice",role:"status","{notice}"}}
     if let Some(c)=confirm{
      section{class:"app-home-confirm",role:"alertdialog","aria-modal":"true","aria-label":c["title"]["text"].as_str().unwrap_or("Confirm app action"),tabindex:"-1","data-keyboard-dialog":"true",
       onmounted:move |_|{dioxus::document::eval("requestAnimationFrame(()=>document.querySelector('.app-home-confirm button')?.focus({preventScroll:true}));");},
       onkeydown:move |e|if e.key()==dioxus::prelude::Key::Escape{e.prevent_default();e.stop_propagation();if calendar_confirm {calendar::cancel(state,key)} else {actions::cancel(state,key)};},
       h3{{c["title"]["text"].as_str().unwrap_or_default()}}p{{c["text"]["text"].as_str().unwrap_or_default()}}
       button{onclick:move |_|if calendar_confirm {calendar::cancel(state,key)} else {actions::cancel(state,key)},{c["deny"]["text"].as_str().unwrap_or("Cancel")}}
       button{onclick:move |_|if calendar_confirm {calendar::confirm(state,key)} else {actions::confirm(state,key)},{c["confirm"]["text"].as_str().unwrap_or("Continue")}}
      }
     }
    }}
}
pub(crate) fn button(
    state: Signal<ShellState>,
    key: Key,
    m: BlockElementMetadata,
    label: String,
    aria: String,
    style: crate::model::ButtonStyle,
) -> Element {
    let enabled = actions::enabled(&state.read(), key, &m);
    rsx! {button{r#type:"button",class:style.class(),disabled:!enabled,"aria-label":"{aria}",title:if enabled{"App action"}else{"This action is unavailable or awaiting confirmation"},onclick:move |_|actions::click(state,key,m.clone()),"{label}"}}
}

enum HomeBlock {
    Display(Vec<crate::model::RichNode>),
    Input(super_platinum_core::block_actions::form::InputKey),
}
