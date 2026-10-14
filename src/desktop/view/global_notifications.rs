//! Native controls use canonical selections; only keywords have a retained draft.
use crate::{
    global_notifications::{self as prefs, Action, Mode},
    state::ShellState,
};
use dioxus::prelude::*;
#[component]
pub(crate) fn GlobalNotifications(state: Signal<ShellState>) -> Element {
    let visit = use_hook(prefs::next_id);
    use_effect(move || prefs::open(state, visit));
    use_drop(move || prefs::close(state, visit));
    let seed = use_memo(move || prefs::presentation(&state.read(), visit).map(|p| p.keyword_seed));
    use_effect(move || {
        let Some(seed) = seed().filter(|seed| *seed != 0) else {
            return;
        };
        let Some(p) = prefs::presentation(&state.peek(), visit).filter(|p| p.keyword_seed == seed)
        else {
            return;
        };
        let text = serde_json::to_string(&p.keywords).unwrap();
        dioxus::document::eval(&format!(
            "{};requestAnimationFrame(()=>window.__globalKeywordSeed.sync('{visit}','{seed}',{text}));",
            include_str!("global_notifications/keywords.js")
        ));
    });
    let ready = use_memo(move || {
        prefs::presentation(&state.read(), visit).map(|p| (!p.busy, p.keyword_seed))
    });
    use_effect(move || {
        if matches!(ready(), Some((true, _))) {
            dioxus::document::eval(&format!(
                r#"requestAnimationFrame(()=>{{const p=document.getElementById('global-notifications');if(p?.dataset.visit!=='{visit}')return;const e=p.restorePreferenceFocus;if(!e)return;if(!e.isConnected){{p.restorePreferenceFocus=null;return;}}const target=e.disabled&&['global-keywords-save','global-keywords-discard'].includes(e.id)?document.getElementById('global-keywords-input'):e;if(target&&!target.disabled&&(document.activeElement===document.body||document.activeElement===e)){{target.focus({{preventScroll:true}});p.restorePreferenceFocus=null;}}else if(document.activeElement!==document.body&&document.activeElement!==e)p.restorePreferenceFocus=null;}})"#
            ));
        }
    });
    let Some(v) = prefs::presentation(&state.read(), visit) else {
        return rsx! {p{class:"scheduled-hint","Loading notification defaults…"}};
    };
    let disabled = !v.can_change;
    let settings = v.snapshot.as_ref().and_then(|s| s.global_settings().ok());
    let linked = v.linked;
    rsx! {section{id:"global-notifications",class:"global-notifications","data-visit":visit,"aria-labelledby":"global-notifications-heading",
      div{class:"global-notifications-heading",h3{id:"global-notifications-heading","Messaging defaults"}
        button{id:"global-notifications-reload",r#type:"button",disabled:!v.can_read,onclick:move |_|prefs::reload(state,visit),if v.uncertain{"Check status"}else{"Reload"}}
      }
      div{id:"global-notifications-status",class:"scheduled-hint",role:"status",if v.busy{"Checking notification preferences…"}else if let Some(message)=v.message{"{message}"}else{"Channel overrides still apply. Device delivery also depends on system settings."}}
      div{id:"global-notifications-scroll",class:"global-notifications-scroll",
       if let Some(p)=settings {
        ModeSelect{state,visit,id:"global-desktop-mode",label:"Notify me about",value:p.desktop,disabled,linked,mobile:false}
        Toggle{id:"global-desktop-push",label:"Desktop notifications",checked:p.desktop_push,disabled,onchange:move |on:bool|{retain_focus("global-desktop-push",visit);prefs::change(state,visit,Action::DesktopPush(on));}}
        Toggle{id:"global-mobile-push",label:"Mobile notifications",checked:p.mobile_enabled,disabled,onchange:move |on:bool|{retain_focus("global-mobile-push",visit);prefs::change(state,visit,Action::MobilePush(on));}}
        Toggle{id:"global-mobile-overrides",label:"Use different settings on mobile",checked:!linked,disabled:disabled||!p.mobile_enabled,onchange:move |separate:bool|{retain_focus("global-mobile-overrides",visit);if separate {prefs::separate_mobile(state,visit)}else{prefs::change(state,visit,Action::RelinkMobile);}}}
        if !linked&&p.mobile_enabled {ModeSelect{state,visit,id:"global-mobile-mode",label:"On mobile, notify me about",value:p.mobile,disabled,linked,mobile:true}}
        Toggle{id:"global-thread-replies",label:"Thread replies",checked:p.thread_replies,disabled,onchange:move |on:bool|{retain_focus("global-thread-replies",visit);prefs::change(state,visit,Action::ThreadReplies(on));}}
        div{class:"global-keywords",
         label{r#for:"global-keywords-input","Channel keywords"}
         p{class:"scheduled-hint","Separate keywords with commas. Changes are saved only when you choose Save keywords."}
         textarea{id:"global-keywords-input","data-seed":v.keyword_seed,onmounted:move |_|{dioxus::document::eval(&format!("{};window.__globalKeywordSeed.install('{visit}');",include_str!("global_notifications/keywords.js")));},initial_value:"{v.keywords}",disabled,rows:2,"aria-describedby":"global-keywords-error",oninput:move |e|prefs::edit_keywords(state,visit,e.value())}
         div{id:"global-keywords-error",role:"status",class:"scheduled-hint",if let Some(error)=v.keyword_error.clone(){"{error}"}else if v.keyword_dirty{"Unsaved changes"}}
         div{class:"global-keyword-actions",
           button{id:"global-keywords-discard",r#type:"button",disabled:disabled||!v.keyword_dirty,onclick:move |_|{retain_focus("global-keywords-discard",visit);prefs::discard_keywords(state,visit);},"Discard"}
           button{id:"global-keywords-save",r#type:"button",disabled:disabled||!v.keyword_dirty||v.keyword_error.is_some(),onclick:move |_|{retain_focus("global-keywords-save",visit);prefs::save_keywords(state,visit);},"Save keywords"}
         }
        }
       }
      }
    }}
}
#[component]
fn ModeSelect(
    state: Signal<ShellState>,
    visit: u64,
    id: &'static str,
    label: &'static str,
    value: Option<Mode>,
    disabled: bool,
    linked: bool,
    mobile: bool,
) -> Element {
    let raw = value.map(|m| m.as_str()).unwrap_or("");
    rsx! {label{class:"global-mode-row",r#for:id,"{label}",select{id,value:raw,"data-canonical":raw,disabled,onchange:move |e|{
      let mode=match e.value().as_str(){"everything"=>Mode::Everything,"mentions_dms"=>Mode::Mentions,_=>return};retain_focus(id,visit);
      prefs::change(state,visit,if mobile{Action::MobileMode(mode)}else{Action::DesktopMode{mode,linked_mobile:linked}});
    },
      if value.is_none(){option{value:"",selected:true,disabled:true,"Choose a default"}}
      option{value:"everything",selected:value==Some(Mode::Everything),"All new messages"}
      option{value:"mentions_dms",selected:value==Some(Mode::Mentions),"Mentions and direct messages"}
    }}}
}
#[component]
fn Toggle(
    id: &'static str,
    label: &'static str,
    checked: bool,
    disabled: bool,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {label{class:"global-toggle",r#for:id,span{"{label}"}input{id,r#type:"checkbox",checked,"data-canonical":checked.to_string(),disabled,onchange:move |e|onchange.call(e.checked())}}}
}
fn retain_focus(id: &str, visit: u64) {
    let id = serde_json::to_string(id).unwrap();
    dioxus::document::eval(&format!(
        r#"(()=>{{const p=document.getElementById('global-notifications'),e=document.getElementById({id});if(p?.dataset.visit!=='{visit}'||!e)return;if(document.activeElement===e)p.restorePreferenceFocus=e;if(e.dataset.canonical!==undefined){{if(e.type==='checkbox')e.checked=e.dataset.canonical==='true';else e.value=e.dataset.canonical;}}}})()"#
    ));
}
