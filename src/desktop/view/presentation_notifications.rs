//! Sound choices stay canonical; previews are local and explicitly requested.
use crate::{
    presentation_notifications::{self as prefs, Action},
    state::ShellState,
};
use dioxus::prelude::*;
#[component]
pub(crate) fn PresentationNotifications(state: Signal<ShellState>) -> Element {
    let visit = use_hook(prefs::next_id);
    use_effect(move || prefs::open(state, visit));
    use_drop(move || {
        prefs::close(state, visit);
        crate::notification::stop_sound_preview();
    });
    let ready =
        use_memo(move || prefs::presentation(&state.read(), visit).is_some_and(|p| !p.busy));
    use_effect(move || {
        if ready() {
            dioxus::document::eval(&format!(
                r#"requestAnimationFrame(()=>{{const p=document.getElementById('presentation-notifications');if(p?.dataset.visit!=='{visit}')return;const e=p.restorePreferenceFocus;if(!e)return;if(!e.isConnected){{p.restorePreferenceFocus=null;return;}}if(!e.disabled&&(document.activeElement===document.body||document.activeElement===e)){{e.focus({{preventScroll:true}});p.restorePreferenceFocus=null;}}else if(document.activeElement!==document.body&&document.activeElement!==e)p.restorePreferenceFocus=null;}})"#
            ));
        }
    });
    let Some(v) = prefs::presentation(&state.read(), visit) else {
        return rsx! {p{class:"scheduled-hint","Loading sound preferences…"}};
    };
    let disabled = !v.can_change;
    let settings = v.snapshot.as_ref().and_then(|p| p.settings().ok());
    rsx! {section{id:"presentation-notifications",class:"presentation-notifications","data-visit":visit,"aria-labelledby":"presentation-notifications-heading",
     div{class:"presentation-notifications-heading",h3{id:"presentation-notifications-heading","Sound & appearance"}button{id:"presentation-notifications-reload",r#type:"button",disabled:!v.can_read,onclick:move |_|prefs::reload(state,visit),if v.uncertain{"Check status"}else{"Reload"}}}
     div{id:"presentation-notifications-status",role:"status",class:"scheduled-hint",if v.busy{"Checking notification preferences…"}else if let Some(message)=v.message{"{message}"}else{"Changes apply immediately."}}
     div{id:"presentation-notifications-scroll",class:"presentation-notifications-scroll",
      if let Some(p)=settings{
       if !p.mute_sounds&&!p.desktop_disabled {SoundSelect{state,visit,id:"presentation-desktop-sound",label:"Message sound",value:p.desktop_sound.clone(),disabled,priority:false}}
       Toggle{id:"presentation-include-preview",label:"Include a preview of the message",checked:p.include_preview,disabled:disabled||p.desktop_disabled,onchange:move|value:bool|{retain_focus("presentation-include-preview",visit);prefs::change(state,visit,Action::IncludePreview(value));}}
       Toggle{id:"presentation-mute-sounds",label:"Mute messaging sounds",checked:p.mute_sounds,disabled,onchange:move|value:bool|{retain_focus("presentation-mute-sounds",visit);prefs::change(state,visit,Action::MuteSounds(value));}}
       if v.vip_available&&!p.mute_sounds&&!p.desktop_disabled {SoundSelect{state,visit,id:"presentation-priority-sound",label:"VIP message sound",value:p.priority_sound.clone(),disabled:disabled||!v.has_vips,priority:true}}
       if cfg!(target_os="macos") {DockBounce{state,visit,enabled:p.mac_bounce.as_deref().is_some_and(|v|!v.is_empty()),duration:v.bounce_duration.clone(),disabled:disabled||p.desktop_disabled}}
       span{id:"notification-sound-preview-status",class:"scheduled-hint",role:"status","aria-live":"polite"}
       p{class:"scheduled-hint","Preview plays locally. System notification permissions and device delivery are separate."}
      }
     }
    }}
}
#[component]
fn SoundSelect(
    state: Signal<ShellState>,
    visit: u64,
    id: &'static str,
    label: &'static str,
    value: Option<String>,
    disabled: bool,
    priority: bool,
) -> Element {
    let raw = value.clone().unwrap_or_default();
    let known = raw == "none"
        || super_platinum_core::slack::api::NOTIFICATION_SOUNDS
            .iter()
            .any(|(name, _)| *name == raw);
    let playable = super_platinum_core::slack::api::NOTIFICATION_SOUNDS
        .iter()
        .any(|(name, _)| *name == raw);
    let preview = raw.clone();
    rsx! {div{class:"presentation-sound",label{r#for:id,"{label}"}
     div{class:"presentation-sound-controls",
      select{id,value:"{raw}","data-canonical":"{raw}",disabled,onchange:move|e|{retain_focus(id,visit);let sound=e.value();if preview_allowed(state,visit,priority) && super_platinum_core::slack::api::NOTIFICATION_SOUNDS.iter().any(|(name,_)|*name==sound) {crate::notification::preview_sound(&sound);}prefs::change(state,visit,if priority{Action::PrioritySound(sound)}else{Action::DesktopSound(sound)});},
       if !known{option{value:"{raw}",selected:true,disabled:true,if raw.is_empty(){"Choose a sound"}else{"Current sound unavailable"}}}
       for (name,label) in super_platinum_core::slack::api::NOTIFICATION_SOUNDS {option{key:"{name}",value:*name,selected:raw==*name,"{label}"}}
       option{value:"none",selected:raw=="none",if priority{"Same as message sound"}else{"None"}}
      }
      button{id:"{id}-preview",r#type:"button",disabled:disabled||!playable,"aria-label":"Preview {label}",onclick:move |_|{if preview_allowed(state,visit,priority){crate::notification::preview_sound(&preview);}},"Preview"}
     }
    }}
}
#[component]
fn DockBounce(
    state: Signal<ShellState>,
    visit: u64,
    enabled: bool,
    duration: String,
    disabled: bool,
) -> Element {
    let enable_duration = duration.clone();
    rsx! {div{class:"presentation-sound",
        Toggle{id:"presentation-dock-bounce",label:"Bounce the Dock icon",checked:enabled,disabled,onchange:move|on:bool|{retain_focus("presentation-dock-bounce",visit);prefs::change(state,visit,Action::MacBounce(if on{enable_duration.clone()}else{String::new()}));}}
        if enabled {div{class:"presentation-sound-controls",
            label{r#for:"presentation-dock-duration","Duration"}
            select{id:"presentation-dock-duration",value:"{duration}","data-canonical":"{duration}",disabled,onchange:move|e|{retain_focus("presentation-dock-duration",visit);prefs::change(state,visit,Action::MacBounce(e.value()));},
                if duration!="short"&&duration!="long" {option{value:"{duration}",selected:true,disabled:true,"Current duration unavailable"}}
                option{value:"short",selected:duration=="short","Once"}
                option{value:"long",selected:duration=="long","Until I return"}
            }
        }}
    }}
}
#[component]
fn Toggle(
    id: &'static str,
    label: &'static str,
    checked: bool,
    disabled: bool,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {label{class:"presentation-toggle",r#for:id,span{"{label}"}input{id,r#type:"checkbox",checked,"data-canonical":checked.to_string(),disabled,onchange:move|e|onchange.call(e.checked())}}}
}
fn retain_focus(id: &str, visit: u64) {
    let id = serde_json::to_string(id).unwrap();
    dioxus::document::eval(&format!(
        r#"(()=>{{const p=document.getElementById('presentation-notifications'),e=document.getElementById({id});if(p?.dataset.visit!=='{visit}'||!e)return;if(document.activeElement===e)p.restorePreferenceFocus=e;if(e.type==='checkbox')e.checked=e.dataset.canonical==='true';else if(e.dataset.canonical!==undefined)e.value=e.dataset.canonical;}})()"#
    ));
}

fn preview_allowed(state: Signal<ShellState>, visit: u64, priority: bool) -> bool {
    prefs::presentation(&state.peek(), visit).is_some_and(|p| {
        p.can_change
            && (!priority || (p.vip_available && p.has_vips))
            && p.snapshot
                .as_ref()
                .and_then(|s| s.settings().ok())
                .is_some_and(|s| !s.mute_sounds && !s.desktop_disabled)
    })
}
