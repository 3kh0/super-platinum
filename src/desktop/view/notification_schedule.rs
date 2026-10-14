//! Renderer-owned selection and focus; all wall-clock semantics belong to core.
use crate::{
    notification_schedule::{self as schedule, Action, Bound, Day, Mode},
    state::ShellState,
};
use dioxus::prelude::*;
#[component]
pub(crate) fn NotificationSchedule(state: Signal<ShellState>) -> Element {
    let visit = use_hook(schedule::next_id);
    use_effect(move || schedule::open(state, visit));
    use_drop(move || schedule::close(state, visit));
    let view = schedule::presentation(&state.read(), visit);
    let ready =
        use_memo(move || schedule::presentation(&state.read(), visit).is_some_and(|v| !v.busy));
    use_effect(move || {
        if ready() {
            dioxus::document::eval(&format!(
                r#"requestAnimationFrame(()=>{{const p=document.getElementById('notification-schedule');if(p?.dataset.visit!=='{visit}')return;const e=p.restoreScheduleFocus;if(!e)return;if(!e.isConnected){{p.restoreScheduleFocus=null;return;}}if(!e.disabled&&(document.activeElement===document.body||document.activeElement===e)){{e.focus({{preventScroll:true}});p.restoreScheduleFocus=null;}}else if(document.activeElement!==document.body&&document.activeElement!==e)p.restoreScheduleFocus=null;}})"#
            ));
        }
    });
    let Some(v) = view else {
        return rsx! {p{class:"scheduled-hint","Loading working hours…"}};
    };
    let snapshot = v.snapshot;
    let parsed = snapshot.as_ref().and_then(|s| s.view().ok());
    let disabled = !v.can_change;
    rsx! {section{id:"notification-schedule",class:"notification-schedule","data-visit":visit,"aria-labelledby":"notification-schedule-heading",
      div{class:"notification-schedule-heading",div{class:"notification-schedule-title",h3{id:"notification-schedule-heading","Working hours"}button{id:"notification-schedule-reload",r#type:"button",disabled:!v.can_read,onclick:move |_|schedule::reload(state,visit),if v.uncertain{"Check status"}else{"Reload"}}}p{"Notifications pause outside these hours, except allowed priority alerts."}
       if let Some(zone)=snapshot.as_ref().and_then(|s|s.timezone()){p{id:"notification-schedule-timezone",class:"scheduled-hint","Time zone: {zone}"}}
      }
      div{id:"notification-schedule-status",role:"status",class:"scheduled-hint",
       if v.busy {"Checking working hours…"}else if let Some(message)=v.message {"{message}"}else{"Changes apply immediately."}
      }
      if v.uncertain {p{class:"scheduled-hint","The last change is unconfirmed. Checking never resends it."}}
      if let (Some(snapshot),Some(p))=(snapshot,parsed){
       div{id:"notification-schedule-scroll",class:"notification-schedule-scroll",
         label{class:"notification-schedule-mode",r#for:"notification-schedule-mode","Allow notifications",
          select{id:"notification-schedule-mode","data-canonical":p.mode.map(|m|m.as_str()).unwrap_or(""),value:p.mode.map(|m|m.as_str()).unwrap_or(""),disabled,
            onchange:move |e|{let a=match e.value().as_str(){"every_day"=>Mode::EveryDay,"weekdays"=>Mode::Weekdays,"custom"=>Mode::Custom,_=>return};retain_focus("notification-schedule-mode",visit);schedule::change(state,visit,Action::Mode(a));},
            if p.mode.is_none(){option{value:"",selected:true,disabled:true,"Choose days"}}
            option{value:"every_day",selected:p.mode==Some(Mode::EveryDay),"Every day"}option{value:"weekdays",selected:p.mode==Some(Mode::Weekdays),"Weekdays"}option{value:"custom",selected:p.mode==Some(Mode::Custom),"Custom"}
          }
         }
         if p.mode==Some(Mode::Custom){
          for day in p.days {
           div{key:"{day.day.as_str()}",class:"notification-schedule-day",role:"group","aria-labelledby":"notification-schedule-label-{day.day.as_str()}",
            h4{id:"notification-schedule-label-{day.day.as_str()}","{day_label(day.day)}"}
            div{class:"notification-schedule-times",
             TimeSelect{state,visit,day:Some(day.day),bound:Bound::Start,value:if day.none{Some("all_day".into())}else{day.start},options:snapshot.options(Some(day.day),Bound::Start).unwrap_or_default(),time24:p.time24,disabled}
             if !day.none {span{"to"}TimeSelect{state,visit,day:Some(day.day),bound:Bound::End,value:day.end,options:snapshot.options(Some(day.day),Bound::End).unwrap_or_default(),time24:p.time24,disabled}}
            }
           }
          }
         }else{
          div{class:"notification-schedule-times",
           TimeSelect{state,visit,day:None,bound:Bound::Start,value:p.start,options:snapshot.options(None,Bound::Start).unwrap_or_default(),time24:p.time24,disabled}
           span{"to"}
           TimeSelect{state,visit,day:None,bound:Bound::End,value:p.end,options:snapshot.options(None,Bound::End).unwrap_or_default(),time24:p.time24,disabled}
          }
          if p.mode==Some(Mode::Weekdays){p{class:"scheduled-hint","Notifications are paused on Saturday and Sunday."}}
         }
       }
      }
    }}
}
fn retain_focus(id: &str, visit: u64) {
    let id = serde_json::to_string(id).unwrap();
    dioxus::document::eval(&format!(
        r#"(()=>{{const p=document.getElementById('notification-schedule'),e=document.getElementById({id});if(p?.dataset.visit==='{visit}'&&e){{if(document.activeElement===e)p.restoreScheduleFocus=e;const old=e.dataset.canonical;if(old!==undefined)e.value=old;}}}})()"#
    ));
}
#[component]
fn TimeSelect(
    state: Signal<ShellState>,
    visit: u64,
    day: Option<Day>,
    bound: Bound,
    value: Option<String>,
    options: Vec<String>,
    time24: bool,
    disabled: bool,
) -> Element {
    let side = if bound == Bound::Start {
        "start"
    } else {
        "end"
    };
    let id = format!(
        "notification-schedule-{}-{side}",
        day.map(|d| d.as_str()).unwrap_or("daily")
    );
    let event_id = id.clone();
    let label = format!(
        "{} {}",
        day.map(day_label).unwrap_or("Every day"),
        if bound == Bound::Start {
            "start time"
        } else {
            "end time"
        }
    );
    let canonical = value.unwrap_or_default();
    rsx! {select{id:"{id}","aria-label":"{label}","data-canonical":"{canonical}",value:"{canonical}",disabled,
    onchange:move |e|{let value=e.value();retain_focus(&event_id,visit);let action=if value=="all_day"{let Some(day)=day else{return;};Action::None(day)}else{Action::Time{day,bound,value}};schedule::change(state,visit,action);},
    if canonical.is_empty(){option{value:"",selected:true,disabled:true,"--:--"}}
    for option in options {option{key:"{option}",value:"{option}",selected:option==canonical,"{time_label(&option,time24)}"}}
    }}
}
fn day_label(day: Day) -> &'static str {
    match day {
        Day::Monday => "Monday",
        Day::Tuesday => "Tuesday",
        Day::Wednesday => "Wednesday",
        Day::Thursday => "Thursday",
        Day::Friday => "Friday",
        Day::Saturday => "Saturday",
        Day::Sunday => "Sunday",
    }
}
fn time_label(value: &str, time24: bool) -> String {
    if value == "all_day" {
        return "None".into();
    }
    if value == "23:59" {
        return "Midnight".into();
    }
    if time24 {
        return value.into();
    }
    let Some((h, m)) = value.split_once(':') else {
        return value.into();
    };
    let Ok(h) = h.parse::<u32>() else {
        return value.into();
    };
    format!(
        "{}:{m} {}",
        if h % 12 == 0 { 12 } else { h % 12 },
        if h < 12 { "AM" } else { "PM" }
    )
}
