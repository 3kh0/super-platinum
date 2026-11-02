//! Owned picker; custom images have a separate current opaque display lease.
use super::*;
use crate::thread_feed::reaction_toggle::first_add as actions;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use super_platinum_core::slack::models::ThreadViewItem;
pub(super) fn opener(
    state: Signal<ShellState>,
    s: &ShellState,
    item: &ThreadViewItem,
    source: Option<&crate::thread_feed::Source>,
) -> Element {
    let action = source.and_then(|r| r.root_first_add(s, item));
    let notice = actions::status(s, item);
    rsx! {
        if let Some(action)=action{{let control_key=action.id();rsx!{OpenControl{key:"{control_key}",state,disabled:action.disabled(s),action}}}}
        else if actions::layout(s){button{r#type:"button",class:"chip",disabled:true,
            {crate::icons::icon(crate::icons::Icon::AddReaction,"icon sm")},"Add reaction"}}
        if let Some(notice)=notice{p{role:"status",class:"channel-discussion-context","{notice}"}}
    }
}
#[component]
fn OpenControl(state: Signal<ShellState>, action: actions::Open, disabled: bool) -> Element {
    let alive = use_hook(|| Arc::new(AtomicBool::new(true)));
    let retired = alive.clone();
    use_drop(move || retired.store(false, Ordering::Release));
    rsx! {button{r#type:"button",class:"chip",id:action.id(),disabled,
    onclick:move|e:MouseEvent|{let m=e.modifiers();if alive.load(Ordering::Acquire)&&!disabled&&!(m.ctrl()||m.meta()||m.alt()||m.shift())
        &&e.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary){action.start(state);}},
    {crate::icons::icon(crate::icons::Icon::AddReaction,"icon sm")},"Add reaction"}}
}
pub(crate) fn panel(state: Signal<ShellState>) -> Element {
    let Some(p) = actions::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {OwnedPicker{key:"{p.id}",state,id:p.id}}
}
#[component]
fn OwnedPicker(state: Signal<ShellState>, id: u64) -> Element {
    let alive = use_hook(|| Arc::new(AtomicBool::new(true)));
    let retired = alive.clone();
    use_drop(move || {
        retired.store(false, Ordering::Release);
        actions::close(state, id);
    });
    let Some(p) = actions::presentation(&state.read()).filter(|p| p.id == id) else {
        return rsx! {};
    };
    let (image_assets, media_epoch) = {
        let shell = state.read();
        let ids = actions::image_assets(&shell, &p.choices)
            .into_iter()
            .map(|id| {
                id.filter(|id| shell.media.is_ready(id) && shell.media.ready_bytes(id).is_some())
            })
            .collect::<Vec<_>>();
        (ids, shell.media_epoch)
    };
    let frame = p.frame;
    let ready = p.ready;
    let active = p.active;
    let active_id = p
        .choices
        .get(active)
        .and_then(|c| c.action.as_ref())
        .map(|a| a.id())
        .unwrap_or_default();
    let escape = alive.clone();
    let scrim = alive.clone();
    let close = alive.clone();
    let input = alive.clone();
    let keyboard = alive.clone();
    let more = alive.clone();
    let retry = alive.clone();
    let builtin = alive.clone();
    let workspace = alive.clone();
    let retry_custom = alive.clone();
    let custom_mode = p.custom_mode;
    let can_switch = p.can_switch;
    let can_retry_custom = p.can_retry_custom;
    rsx! {
        div{class:"scrim",onclick:move|_|if scrim.load(Ordering::Acquire){actions::close(state,id);}}
        section{id:"thread-root-first-add-dialog-{id}",class:"modal emoji-picker",role:"dialog","aria-modal":"true",
            "aria-labelledby":"thread-root-first-add-title-{id}",tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other",
            "data-thread-first-add":"{id}","data-emoji-choice-ready":if ready{"true"}else{"false"},
            onkeydown:move|e|if e.key()==Key::Escape&&escape.load(Ordering::Acquire){e.prevent_default();e.stop_propagation();actions::close(state,id);},
            header{h2{id:"thread-root-first-add-title-{id}","React with emoji"}
                button{r#type:"button",id:"thread-root-first-add-close-{id}","aria-label":"Close",
                    onclick:move|_|if close.load(Ordering::Acquire){actions::close(state,id);},
                    {crate::icons::icon(crate::icons::Icon::Close,"icon")}}
            }
            div{class:"thread-root-first-add-sources",role:"group","aria-label":"Emoji source",
                button{r#type:"button",id:"thread-root-first-add-mode-emoji-{id}","aria-pressed":!custom_mode,disabled:!can_switch,
                    onclick:move|e:MouseEvent|{let m=e.modifiers();if builtin.load(Ordering::Acquire)&&can_switch
                        &&!(m.ctrl()||m.meta()||m.alt()||m.shift())
                        &&e.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary){actions::set_custom_mode(state,id,frame,false);}},"Emoji"}
                button{r#type:"button",id:"thread-root-first-add-mode-workspace-{id}","aria-pressed":custom_mode,disabled:!can_switch,
                    onclick:move|e:MouseEvent|{let m=e.modifiers();if workspace.load(Ordering::Acquire)&&can_switch
                        &&!(m.ctrl()||m.meta()||m.alt()||m.shift())
                        &&e.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary){actions::set_custom_mode(state,id,frame,true);}},"Workspace"}
            }
            input{id:"thread-root-first-add-query-{id}",value:p.query,placeholder:"Search emoji…","aria-label":"Search emoji",
                "data-dialog-input":"true","aria-controls":"thread-root-first-add-options-{id}","aria-activedescendant":active_id,
                oninput:move|e|if input.load(Ordering::Acquire){actions::set_query(state,id,e.value());},
                onkeydown:move|e:KeyboardEvent|{
                    if !keyboard.load(Ordering::Acquire){return;}let m=e.modifiers();if m.ctrl()||m.meta()||m.alt()||m.shift(){return;}
                    let step=match e.key(){Key::ArrowLeft=>Some(-1),Key::ArrowRight=>Some(1),Key::ArrowUp=>Some(-8),Key::ArrowDown=>Some(8),_=>None};
                    if let Some(delta)=step{e.prevent_default();e.stop_propagation();actions::move_cursor(state,id,frame,delta);}
                    else if e.key()==Key::Enter{e.prevent_default();e.stop_propagation();if ready{actions::choose_active(state,id,frame);}}
                }
            }
            div{id:"thread-root-first-add-options-{id}",class:"emoji-options",role:"listbox","aria-label":"Emoji",
                for (index,row) in p.choices.iter().enumerate(){
                    {
                        let action=row.action.clone();let control_key=action.as_ref().map(|a|a.id()).unwrap_or_else(||format!("thread-root-first-add-fixture-{index}"));
                        let clicked=alive.clone();
                        rsx!{button{r#type:"button",key:"{control_key}",id:control_key,role:"option","aria-selected":index==active,
                            "aria-label":row.name.clone(),title:format!(":{}:",row.name),"data-first-add-name":row.name.clone(),disabled:!ready,
                            onclick:move|e:MouseEvent|{let m=e.modifiers();if clicked.load(Ordering::Acquire)&&ready&&!(m.ctrl()||m.meta()||m.alt()||m.shift())
                                &&e.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary){if let Some(action)=action.as_ref(){actions::choose(state,action.clone());}}},
                            if let Some(image)=row.image.as_ref(){ {custom_image(state,image,image_assets.get(index).cloned().flatten(),media_epoch)} }
                            else if custom_mode{span{class:"thread-root-first-add-image-frame",span{class:"thread-root-first-add-image-fallback","{row.glyph}"}}}
                            else{"{row.glyph}"}}}
                    }
                }
            }
            if p.choices.is_empty()&&!p.busy{p{class:"emoji-empty","No matching emoji available."}}
            footer{class:"emoji-footer",
                span{role:"status",if p.notice.is_empty(){"{p.choices.len()} of {p.total} emoji"}else{"{p.notice}"}}
                if p.can_retry{button{r#type:"button",id:"thread-root-first-add-retry-{id}",onclick:move|_|if retry.load(Ordering::Acquire){actions::retry(state,id);},"Retry"}}
                if can_retry_custom{button{r#type:"button",id:"thread-root-first-add-custom-retry-{id}",
                    onclick:move|e:MouseEvent|{let m=e.modifiers();if retry_custom.load(Ordering::Acquire)&&can_retry_custom
                        &&!(m.ctrl()||m.meta()||m.alt()||m.shift())
                        &&e.trigger_button().is_none_or(|b|b==dioxus::html::input_data::MouseButton::Primary){actions::retry_custom(state,id,frame);}},"Retry workspace"}}
                if p.choices.len()<p.total{button{r#type:"button",disabled:!ready,onclick:move|_|if more.load(Ordering::Acquire){actions::show_more(state,id,frame);},"Show more"}}
            }
        }
    }
}

fn custom_image(
    state: Signal<ShellState>,
    display: &actions::ImageDisplay,
    id: Option<super_platinum_core::MediaAssetId>,
    media_epoch: u64,
) -> Element {
    let label = display.label().to_owned();
    let ready = id.map(|id| (id, media_epoch));
    rsx! {span{class:"thread-root-first-add-image-frame",
        if let Some((id,media_epoch))=ready {
            crate::view::media_image::MediaImage{state,id,media_epoch,alt:label.clone(),compact:true,
                fallback:rsx!{span{class:"thread-root-first-add-image-fallback","{label}"}}}
        } else {span{class:"thread-root-first-add-image-fallback","{label}"}}
    }}
}
