use super::*;
use actions::Field;
pub(super) fn dialog(state: Signal<ShellState>, id: u64) -> Element {
    rsx! {CalendarDialog{key:"calendar-{id}",state,id}}
}
#[component]
fn CalendarDialog(state: Signal<ShellState>, id: u64) -> Element {
    render(state, id)
}
fn render(mut state: Signal<ShellState>, id: u64) -> Element {
    let s = state.read();
    let Some(d) = s
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .filter(|d| d.id == id)
    else {
        return rsx! {};
    };
    let Some(calendar) = d.calendar.as_ref() else {
        return rsx! {};
    };
    let c = Presentation::capture(calendar);
    let owned = current(&s, id);
    let navigating = crate::app_home::navigation::pending_id(&s).is_some();
    let editing = editable(&s, id);
    let rate_limited = cooling(&s, &d.owner.ctx);
    let share_unavailable = editing
        && calendar.share
        && matches!(calendar.account, Some(cal::Account::Legacy { .. }))
        && sharing::resolve(&s, id).is_none();
    let ready = !rate_limited && actions::event(&s, id).is_some();
    let title = c.provider.label();
    let stage = c.stage;
    let modern = matches!(c.account, Some(cal::Account::Modern));
    let sharing = c.share_choices.clone();
    drop(s);
    rsx! {
        div{class:"scrim",onclick:move |_|close(&mut state.write(),id,false)}
        section{id:"app-shortcut-dialog",class:"modal app-modal calendar-shortcut-dialog",role:"dialog","aria-modal":"true","aria-label":"Create event","data-keyboard-dialog":"true","data-dialog-generation":id,"data-dialog-kind":"other","data-calendar-editing":editing,"data-calendar-ready":ready,tabindex:"-1",
            onmounted:move |_|{dioxus::document::eval(&format!("{}\ninstallCalendarAdmission({id});\n{}",include_str!("native.js"),include_str!("../focus.js").replace("__OWNER__",&id.to_string())));},
            onkeydown:move |e|{if e.key()==Key::Escape{e.stop_propagation();close(&mut state.write(),id,false);}},
            header{h2{"Create event"}button{id:"app-shortcut-close",title:"Close","aria-label":"Close",onclick:move |_|close(&mut state.write(),id,false),{crate::icons::icon(crate::icons::Icon::Close,"icon")}}}
            div{class:"app-modal-body",
                p{class:"app-modal-hint","{title}"}
                if rate_limited{p{role:"status","Slack asked us to wait before another calendar request. Your draft is retained."}}
                if !owned{p{role:"alert","The account or conversation changed. Close and reopen the calendar."}}
                if stage==Stage::Loading{p{role:"status","Loading calendar…"}}
                if stage==Stage::Connect{
                    p{"Connect your calendar to create an event. After connecting, close this dialog and open the shortcut again."}
                    if let Some(url)=c.auth_url.clone(){button{class:"primary",disabled:!owned,onclick:move |_|external(&state.peek(),id,&url),"Connect calendar"}}
                }
                if c.account.is_some() && stage!=Stage::Loading{
                    fieldset{id:"calendar-fields",style:"border:0;margin:0;padding:0;min-width:0",disabled:!editing,"data-calendar-enabled":editing,
                    div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-title","Title ",span{class:"app-modal-required","(optional)"}}
                        input{id:"calendar-title",initial_value:"{c.title}",disabled:!editing,autocomplete:"off",oninput:move |e|actions::edit(state,id,Field::Title,e.value())}}
                    div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-guest-query","Guests ",span{class:"app-modal-required","(optional)"}}
                        div{class:"calendar-selected-guests",
                            for person in c.guests.clone(){div{key:"{person.id}",class:"calendar-selected-guest",span{"{person.label}"}button{disabled:!editing,"aria-label":format!("Remove {}",person.label),onclick:move |_|actions::guest(state,id,person.id.clone(),true),"Remove"}}}
                        }
                        input{id:"calendar-guest-query",initial_value:"{c.guest_query}",disabled:!editing,autocomplete:"off",placeholder:"Search people",oninput:move |e|load::guests(state,id,e.value())}
                        if c.guest_loading{p{role:"status","Searching people…"}}
                        div{class:"calendar-guest-results",
                            for person in c.guest_choices.clone(){
                                if !c.guests.iter().any(|g|g.id==person.id){button{key:"{person.id}",disabled:!editing,onclick:move |_|actions::guest(state,id,person.id.clone(),false),"{person.label}"}}
                            }
                        }
                    }
                    div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-calendar","Calendar"}
                        select{id:"calendar-calendar",value:"{c.calendar}",disabled:!editing,onchange:move |e|actions::edit(state,id,Field::Calendar,e.value()),
                            option{value:"","Default calendar"}
                            for choice in c.calendars.clone(){option{key:"{choice.id}",value:"{choice.id}","{choice.name}"}}
                        }
                    }
                    div{class:"calendar-time-fields",
                        div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-date","Date"}
                            input{id:"calendar-date",r#type:"date",initial_value:"{c.date}",disabled:!editing,oninput:move |e|actions::edit(state,id,Field::Date,e.value())}}
                        div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-mode","Time selection"}
                            select{id:"calendar-mode",value:if c.custom{"custom"}else{"suggested"},disabled:!editing,onchange:move |e|actions::mode(state,id,e.value()=="custom"),option{value:"suggested","Suggested times"}option{value:"custom","Custom time"}}}
                    }
                    if !c.custom{
                        div{class:"calendar-time-fields",
                            div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-duration","Duration"}
                                select{id:"calendar-duration",value:"{c.duration}",disabled:!editing,onchange:move |e|actions::edit(state,id,Field::Duration,e.value()),
                                    for (minutes,label) in [(15,"15 minutes"),(30,"30 minutes"),(45,"45 minutes"),(60,"1 hour"),(90,"1.5 hours"),(120,"2 hours")]{option{key:"{minutes}",value:"{minutes}","{label}"}}
                                }}
                            div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-slot","Available times"}
                                select{id:"calendar-slot",value:c.start.map(|s|s.to_string()).unwrap_or_default(),disabled:!editing||c.slot_loading,onchange:move |e|actions::slot(state,id,e.value()),
                                    option{value:"",disabled:true,"Choose a time"}
                                    for slot in c.slots.clone(){option{key:"{slot.start}",value:"{slot.start}","{cal::time::label(slot.start)} – {cal::time::label(slot.end)}",if !slot.all_free{" · Some guests busy"}}}
                                }}
                        }
                        button{disabled:!editing||c.slot_loading,onclick:move |_|load::slots(state,id),"Refresh availability"}
                    }else{
                        div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-custom-slot","Suggested custom start"}
                            select{id:"calendar-custom-slot",value:c.start.map(|s|s.to_string()).unwrap_or_default(),disabled:!editing,onchange:move |e|actions::slot(state,id,e.value()),
                                option{value:"","Choose a time or enter one below"}
                                for slot in c.slots.clone(){option{key:"{slot.start}",value:"{slot.start}","{cal::time::label(slot.start)}"}}
                            }
                        }
                        for revision in [c.time_revision] {
                        div{key:"time-{revision}",class:"calendar-time-fields",
                            div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-start","Start time"}input{id:"calendar-start",r#type:"time",initial_value:"{c.start_text}",disabled:!editing,oninput:move |e|actions::edit(state,id,Field::Start,e.value())}}
                            div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-end","End time"}input{id:"calendar-end",r#type:"time",initial_value:"{c.end_text}",disabled:!editing,oninput:move |e|actions::edit(state,id,Field::End,e.value())}}
                        }
                    }}
                    if c.slot_loading{p{role:"status","Fetching availability…"}}
                    if let Some(notice)=c.slot_notice.clone(){p{role:"status","{notice}"}}
                    if c.start.zip(c.end).is_some_and(|(a,b)|a>=b){p{role:"alert","End time must be later than start time."}}
                    p{class:"app-modal-hint","Times use this computer’s local timezone."}
                    div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-conference","Conference"}
                        select{id:"calendar-conference",value:"{c.selected_room}",disabled:!editing||c.room_pending.is_some(),onchange:move |e|writes::room(state,id,e.value()),option{value:"","None"}
                            for provider in c.conferences.clone(){option{key:"{provider.id}",value:"{provider.id}","{provider.name}"}}
                        }
                        if c.room_pending.is_some(){p{role:"status","Creating conference…"}}
                        if let Some(url)=c.meeting_url.as_ref(){p{class:"app-modal-hint","{url}"}}
                    }
                    div{class:"app-modal-field",label{class:"app-modal-label",r#for:"calendar-description","Description ",span{class:"app-modal-required","(optional)"}}
                        textarea{id:"calendar-description",initial_value:"{c.description}",disabled:!editing,oninput:move |e|actions::edit(state,id,Field::Description,e.value())}}
                    if !modern{
                        div{class:"app-modal-field",
                            label{class:"app-modal-option",input{id:"calendar-share",r#type:"checkbox",checked:c.share,disabled:!editing||c.preference_pending.is_some(),onchange:move |e|actions::share(state,id,e.checked())}span{"Share this meeting"}}
                            label{class:"app-modal-label",r#for:"calendar-share-query","Find conversation"}
                            input{id:"calendar-share-query",initial_value:"{c.share_query}",disabled:!editing||!c.share,oninput:move |e|actions::share_query(state,id,e.value())}
                            if c.share_loading{p{role:"status","Searching conversations..."}}
                            if share_unavailable{p{role:"status","This sharing destination is unavailable. Choose another conversation or turn sharing off."}}
                            select{id:"calendar-share-channel","aria-label":"Share meeting with",value:"{c.share_channel}",disabled:!editing||!c.share,onchange:move |e|actions::edit(state,id,Field::ShareChannel,e.value()),
                                for choice in sharing{option{key:"{choice.id}",value:"{choice.id}","{choice.name}"}}
                            }
                        }
                    }
                }
                }
                if let Some(notice)=c.notice.clone(){p{role:"status","{notice}"}}
                if stage==Stage::Created{if let Some(url)=c.provider_link.clone(){button{disabled:!owned,onclick:move |_|external(&state.peek(),id,&url),"Open event in calendar"}}}
                if stage==Stage::Created && c.app_link.is_some(){button{disabled:!owned||navigating,onclick:move |_|navigation::app_home(state,id),"Open app Home"}}
                if stage==Stage::Created{{crate::app_home::navigation::status(state)}}
                if c.confirm_close{div{role:"alert",p{"Discard this event draft? Any conference already created will remain."}button{onclick:move |_|{if let Some(c)=active_mut(&mut state.write(),id){c.confirm_close=false;}},"Keep editing"}button{onclick:move |_|close(&mut state.write(),id,true),"Discard draft"}}}
            }
            footer{button{onclick:move |_|close(&mut state.write(),id,false),if stage==Stage::Editing{"Cancel"}else{"Close"}}span{class:"spacer"}
                if stage==Stage::Failed&&c.account.is_some(){button{disabled:!owned,onclick:move |_|actions::resume(state,id),"Edit retained draft"}}
                if matches!(stage,Stage::Created|Stage::Uncertain|Stage::Failed){button{disabled:!owned,onclick:move |_|actions::fresh(state,id),"Start a different event"}}
                if matches!(stage,Stage::Editing|Stage::Creating){button{id:"calendar-create",class:"primary",disabled:!ready,onclick:move |_|writes::create(state,id),if stage==Stage::Creating{"Creating…"}else{"Create event"}}}
            }
        }
    }
}

/// Capture only bounded displayed values, never clone remote canonical user/
/// channel records or the uncertainty ledger on a render.
struct Presentation {
    provider: cal::Provider,
    stage: Stage,
    account: Option<cal::Account>,
    auth_url: Option<String>,
    title: String,
    description: String,
    guests: Vec<Person>,
    guest_query: String,
    guest_choices: Vec<Person>,
    guest_loading: bool,
    calendars: Vec<cal::Choice>,
    calendar: String,
    date: String,
    custom: bool,
    duration: u16,
    start: Option<i64>,
    end: Option<i64>,
    time_revision: u64,
    start_text: String,
    end_text: String,
    slots: Vec<SlotOption>,
    slot_loading: bool,
    slot_notice: Option<String>,
    conferences: Vec<cal::Choice>,
    selected_room: String,
    room_pending: Option<u64>,
    meeting_url: Option<String>,
    share: bool,
    share_query: String,
    share_channel: String,
    share_choices: Vec<cal::Choice>,
    share_loading: bool,
    preference_pending: Option<u64>,
    notice: Option<String>,
    provider_link: Option<String>,
    app_link: Option<String>,
    confirm_close: bool,
}
#[derive(Clone, Copy)]
struct SlotOption {
    start: i64,
    end: i64,
    all_free: bool,
}
impl Presentation {
    fn capture(c: &Calendar) -> Self {
        let query = c.share_query.to_lowercase();
        let mut share_choices: Vec<_> = c
            .share_choices
            .iter()
            .filter(|v| v.id == c.share_channel)
            .take(1)
            .cloned()
            .collect();
        share_choices.extend(
            c.share_choices
                .iter()
                .filter(|v| {
                    v.id != c.share_channel
                        && (query.is_empty() || v.name.to_lowercase().contains(&query))
                })
                .take(99)
                .cloned(),
        );
        Self {
            provider: c.provider,
            stage: c.stage,
            account: c.account.clone(),
            auth_url: c.auth_url.clone(),
            title: c.title.clone(),
            description: c.description.clone(),
            guests: c.guests.clone(),
            guest_query: c.guest_query.clone(),
            guest_choices: c.guest_choices.clone(),
            guest_loading: c.guest_loading,
            calendars: c.calendars.clone(),
            calendar: c.calendar.clone(),
            date: c.date.clone(),
            custom: c.custom,
            duration: c.duration,
            start: c.start,
            end: c.end,
            time_revision: c.time_revision,
            start_text: c.start_text.clone(),
            end_text: c.end_text.clone(),
            slots: c
                .slots
                .iter()
                .map(|v| SlotOption {
                    start: v.start,
                    end: v.end,
                    all_free: v.all_free,
                })
                .collect(),
            slot_loading: c.slot_loading,
            slot_notice: c.slot_notice.clone(),
            conferences: c.conferences.clone(),
            selected_room: c.selected_room.clone(),
            room_pending: c.room_pending,
            meeting_url: c.rooms.get(&c.selected_room).cloned(),
            share: c.share,
            share_query: c.share_query.clone(),
            share_channel: c.share_channel.clone(),
            share_choices,
            share_loading: c.share_loading,
            preference_pending: c.preference_pending,
            notice: c.notice.clone(),
            provider_link: c.provider_link.clone(),
            app_link: c.app_link.clone(),
            confirm_close: c.confirm_close,
        }
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
