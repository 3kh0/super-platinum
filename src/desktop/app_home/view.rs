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
    let unavailable = h.view.as_ref().is_some_and(View::has_controls);
    let nodes = h
        .view
        .as_ref()
        .and_then(|v| {
            s.core
                .workspaces
                .get(&h.owner.ctx.session.team_id)
                .map(|w| {
                    v.raw()["blocks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|b| {
                            crate::blocks::block_nodes(crate::blocks::BlockCtx::new(w, &s.media), b)
                        })
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default();
    let confirm = h
        .confirmation
        .as_ref()
        .and_then(|b| b.raw().get("confirm"))
        .cloned();
    let epoch = s.media_epoch;
    let locked = loading || confirm.is_some() || cooling(&s, &h.owner.ctx);
    drop(s);
    rsx! {section{id:"app-home",class:"app-home","aria-label":"App Home","data-visit":key.visit,"data-revision":key.revision,
     div{class:"app-home-toolbar",h2{"Home"}span{class:"spacer"}button{disabled:locked,onclick:move |_|{dioxus::core::spawn_forever(refresh(state));},"Refresh"}}
     div{class:"app-home-content","aria-busy":loading,
      if loading{p{role:"status","Loading Home…"}}
      for node in &nodes{{crate::view::rich::rich_node_for_home(node,epoch,state,key)}}
      if unavailable{p{class:"app-home-notice",role:"status","This Home includes controls that are not supported here yet. Its actions are disabled to preserve the full form state."}}
     }
     if let Some(notice)=notice{p{class:"app-home-notice",role:"status","{notice}"}}
     if let Some(c)=confirm{
      section{class:"app-home-confirm",role:"alertdialog","aria-modal":"true","aria-label":c["title"]["text"].as_str().unwrap_or("Confirm app action"),tabindex:"-1","data-keyboard-dialog":"true",
       onmounted:move |_|{dioxus::document::eval("requestAnimationFrame(()=>document.querySelector('.app-home-confirm button')?.focus({preventScroll:true}));");},
       onkeydown:move |e|if e.key()==dioxus::prelude::Key::Escape{e.prevent_default();e.stop_propagation();actions::cancel(state,key);},
       h3{{c["title"]["text"].as_str().unwrap_or_default()}}p{{c["text"]["text"].as_str().unwrap_or_default()}}
       button{onclick:move |_|actions::cancel(state,key),{c["deny"]["text"].as_str().unwrap_or("Cancel")}}
       button{onclick:move |_|actions::confirm(state,key),{c["confirm"]["text"].as_str().unwrap_or("Continue")}}
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
    rsx! {button{class:style.class(),disabled:!enabled,"aria-label":"{aria}",title:if enabled{"App action"}else{"This action is unavailable or awaiting confirmation"},onclick:move |_|actions::click(state,key,m.clone()),"{label}"}}
}
