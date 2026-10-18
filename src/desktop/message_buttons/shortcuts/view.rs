use super::*;
pub(crate) fn dialog(state: Signal<ShellState>) -> Element {
    let s = state.read();
    let Some(d) = s.message_buttons.shortcuts.dialog.as_ref() else {
        return rsx! {};
    };
    let id = d.id;
    if d.calendar.is_some() {
        drop(s);
        return super::calendar::dialog(state, id);
    }
    rsx! { ShortcutDialog{key:"{id}",state,id} }
}
#[component]
fn ShortcutDialog(mut state: Signal<ShellState>, id: u64) -> Element {
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
    let current = is_current(&s, id);
    let choosing = current && d.phase == Phase::Choosing;
    let title = if d.kind == domain::Kind::Global {
        "App shortcuts"
    } else {
        "Message shortcuts"
    };
    let query_value = d.query.clone();
    let selected = d.selected;
    let selected_team = d.team.clone();
    let notice = d.notice.clone();
    let phase = d.phase;
    let page_index = d.page;
    let match_count = matching(d).count();
    let page_number = page_index + 1;
    let page_count = match_count.div_ceil(64);
    let rows: Vec<_> = matching(d)
        .skip(page_index * 64)
        .take(64)
        .map(|(index, a)| {
            (
                index,
                a.name.clone(),
                a.app_name.clone(),
                a.description.clone(),
            )
        })
        .collect();
    let teams = d
        .selected
        .and_then(|i| d.catalog.as_ref()?.actions.get(i))
        .filter(|a| {
            d.kind == domain::Kind::Global && domain::calendar::Provider::for_action(a).is_none()
        })
        .map(|a| a.applicable_teams(&d.owner.related))
        .unwrap_or_default();
    let ready = choosing
        && !d.fixture
        && selected.is_some()
        && (teams.is_empty() || selected_team.as_ref().is_some_and(|t| teams.contains(t)));
    let team_rows: Vec<_> = teams
        .iter()
        .map(|t| {
            (
                t.clone(),
                s.core
                    .workspaces
                    .get(t)
                    .map(|w| w.name.clone())
                    .unwrap_or_else(|| t.clone()),
            )
        })
        .collect();
    drop(s);
    rsx! {
        div{class:"scrim",onclick:move |_|cancel_owned(&mut state.write(),id)}
        section{class:"modal app-shortcut-dialog",role:"dialog","aria-modal":"true","aria-label":"{title}",
            id:"app-shortcut-dialog",tabindex:"-1","data-keyboard-dialog":"true","data-dialog-generation":id,"data-dialog-kind":"other",
            onmounted:move |_|{dioxus::document::eval(&include_str!("focus.js").replace("__OWNER__",&id.to_string()));},
            onkeydown:move |e|{if e.key()==Key::Escape{e.stop_propagation();cancel_owned(&mut state.write(),id)}},
            header{h2{"{title}"}button{id:"app-shortcut-close",title:"Close","aria-label":"Close",onclick:move |_|cancel_owned(&mut state.write(),id),{crate::icons::icon(crate::icons::Icon::Close,"icon")}}}
            div{class:"app-shortcut-body",
                if phase==Phase::Loading{p{role:"status","Loading app shortcuts…"}}
                if !current{p{role:"alert","The conversation or account changed. Close and reopen shortcuts."}}
                if phase==Phase::Choosing{
                    label{r#for:"app-shortcut-query","Search shortcuts"}
                    input{id:"app-shortcut-query",initial_value:"{query_value}",disabled:!choosing,autocomplete:"off",
                        oninput:move |e|query(&mut state.write(),id,e.value())}
                    div{class:"app-shortcut-results",
                        if rows.is_empty(){p{"No available shortcuts match this search."}}
                        for (index,name,app,description) in rows{
                            label{key:"{index}",class:"app-shortcut-row",
                                input{r#type:"radio",name:"app-shortcut-choice",checked:selected==Some(index),disabled:!choosing,
                                    onchange:move |_|choose(&mut state.write(),id,index)}
                                span{strong{"{name}"}span{"{app}"}small{"{description}"}}
                            }
                        }
                    }
                    if match_count > 64 {
                        div{class:"app-shortcut-pages",
                            button{disabled:!choosing||page_index==0,onclick:move |_|page(&mut state.write(),id,false),"Previous"}
                            span{"Page {page_number} of {page_count}"}
                            button{disabled:!choosing||(page_index+1)*64>=match_count,onclick:move |_|page(&mut state.write(),id,true),"Next"}
                        }
                    }
                    if !team_rows.is_empty(){
                        label{r#for:"app-shortcut-team","Run in workspace"}
                        select{id:"app-shortcut-team",value:selected_team.clone().unwrap_or_default(),disabled:!choosing,
                            onchange:move |e|choose_team(&mut state.write(),id,e.value()),
                            option{value:"",disabled:true,"Select workspace"}
                            for (team,label) in team_rows{option{key:"{team}",value:"{team}","{label}"}}
                        }
                    }
                }
                if let Some(text)=notice{p{role:"status","{text}"}}
            }
            footer{
                button{onclick:move |_|cancel_owned(&mut state.write(),id),if phase==Phase::Choosing||phase==Phase::Loading{"Cancel"}else{"Close"}}
                button{class:"primary app-shortcut-run",disabled:!ready,onclick:move |_|launch(state,id),
                    if phase==Phase::Sending{"Sending…"}else{"Run shortcut"}}
            }
        }
    }
}
