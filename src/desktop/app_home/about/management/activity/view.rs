use super::*;
#[cfg(test)]
mod tests;

pub(in crate::app_home::about::management) fn panel(
    mut state: Signal<ShellState>,
    visit: u64,
) -> Element {
    let s = state.read();
    let Some(d) = document(&s, visit).filter(|d| d.activity.active) else {
        return rsx! {};
    };
    let a = d.activity.clone();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let loading = h.read.is_some();
    let (export_shown, export_disabled, export_notice) = export::status(&s, visit);
    let exporting = export::busy(&s, visit);
    let read_id = h.read.unwrap_or(0);
    let blocked = !permitted(&s, visit);
    let filters = a.filters.clone();
    let (member_generation, member_rows, member_loading, member_notice) = members::view(&s, visit);
    let member_search = a.user_search.clone();
    let input_generation = a.member_generation;
    let team = h.owner.ctx.session.team_id.clone();
    let rows = a
        .page
        .as_ref()
        .map(|p| {
            routes::rows(p, &a.breadcrumbs)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let previous = !a.previous.is_empty();
    let more = a.page.as_ref().and_then(ActivityPage::next_query).is_some();
    let initial = a.page.is_none();
    let media_epoch = s.media_epoch;
    let detail_text = a.detail.as_ref().map(|detail| {
        let trigger = detail
            .trigger_message()
            .map(|v| message::project(&s, &team, visit, v));
        let steps = detail
            .steps()
            .iter()
            .map(|step| message::project(&s, &team, visit, &step["outcome_message"]))
            .collect::<Vec<_>>();
        (trigger, steps)
    });
    let actor_names = rows
        .iter()
        .map(|row| user(&s, &team, row["actor_id"].as_str().unwrap_or_default()))
        .collect::<Vec<_>>();
    drop(s);
    // Quoted value/selected bindings are nonvolatile in the pinned renderer.
    // Unrelated realtime renders must not erase native edits before IPC delivery;
    // actual model changes still update the same controls.
    rsx! {
        section{id:"workflow-activity",class:"app-home workflow-management-activity","data-activity-read":read_id,"data-page-revision":a.page_revision,
            onmounted:move |_|{if initial{load(state,visit,Page::First);}},
            div{class:"app-home-toolbar",
                h3{"Activity"}
                span{class:"spacer"}
                if export_shown {
                    button{id:"workflow-activity-export",r#type:"button",disabled:export_disabled,onclick:move |_|export::start(state,visit),"Export CSV"}
                }
                button{id:"workflow-activity-back",r#type:"button",onclick:move |_|back(&mut state.write(),visit),"Back to details"}
            }
            div{class:"app-home-content workflow-activity-content",
                if !a.breadcrumbs.is_empty(){
                    nav{class:"workflow-management-actions","aria-label":"Activity run navigation",
                        button{id:"workflow-activity-all",disabled:loading||blocked,onclick:{let expected=a.breadcrumbs.clone();move |_|depth(state,visit,expected.clone(),0)},"All runs"}
                        for index in 1..a.breadcrumbs.len(){
                            button{key:"{index}",disabled:loading||blocked,onclick:{let expected=a.breadcrumbs.clone();move |_|depth(state,visit,expected.clone(),index)},"Level {index}"}
                        }
                    }
                }
                form{id:"workflow-activity-filters","data-revision":filters.revision,class:"workflow-activity-filters workflow-management-filters",
                    onsubmit:move |e|{e.prevent_default();load(state,visit,Page::First);},
                    label{r#for:"workflow-activity-status","Status"
                        select{id:"workflow-activity-status","value":filters.status.clone(),disabled:blocked,
                            onchange:move |e|edit(&mut state.write(),visit,Field::Status,e.value()),
                            option{value:"","selected":filters.status.is_empty(),"All statuses"}option{value:"SUCCESS","selected":filters.status=="SUCCESS","Completed"}option{value:"PENDING","selected":filters.status=="PENDING","Processing"}
                            option{value:"ERROR","selected":filters.status=="ERROR","Failed"}option{value:"CANCELED","selected":filters.status=="CANCELED","Canceled"}option{value:"PENDING_USER_ACTION","selected":filters.status=="PENDING_USER_ACTION","Awaiting user action"}
                        }
                    }
                    label{r#for:"workflow-activity-from","From"
                        input{id:"workflow-activity-from",r#type:"date","value":filters.from,disabled:blocked,oninput:move |e|edit(&mut state.write(),visit,Field::From,e.value())}
                    }
                    label{r#for:"workflow-activity-to","Through"
                        input{id:"workflow-activity-to",r#type:"date","value":filters.to,disabled:blocked,oninput:move |e|edit(&mut state.write(),visit,Field::To,e.value())}
                    }
                    label{r#for:"workflow-activity-limit","Runs per page"
                        select{id:"workflow-activity-limit","value":filters.limit.clone(),disabled:blocked,onchange:move |e|edit(&mut state.write(),visit,Field::Limit,e.value()),
                            for n in [20,50,75,100]{option{key:"{n}",value:"{n}","selected":filters.limit==n.to_string(),"{n}"}}
                        }
                    }
                    label{r#for:"workflow-activity-user-search","Find a person"
                        input{id:"workflow-activity-user-search",r#type:"search","value":member_search,
                            "data-generation":input_generation,disabled:blocked||loading,
                            onfocus:move |_|members::start(state,visit),
                            oninput:move |e|{members::edit(&mut state.write(),visit,e.value());members::start(state,visit);},
                            oncompositionstart:move |_|members::composition(&mut state.write(),visit,true),
                            oncompositionend:move |_|{members::composition(&mut state.write(),visit,false);members::start(state,visit);}}
                    }
                    label{r#for:"workflow-activity-user","Started by"
                        select{id:"workflow-activity-user","value":filters.user.clone(),disabled:blocked,
                            onchange:move |e|members::choose(state,visit,member_generation,filters.revision,e.value()),
                            option{value:"","selected":filters.user.is_empty(),"Anyone"}
                            for (id,label) in member_rows{option{key:"{id}",value:id.clone(),"selected":filters.user==id,"{label}"}}
                        }
                    }
                    button{id:"workflow-activity-find-people",r#type:"button",disabled:blocked||loading||member_loading,onclick:move |_|members::start(state,visit),"Find people"}
                    button{id:"workflow-activity-apply",r#type:"submit",disabled:loading||blocked,"Apply filters"}
                }
                if member_loading{p{role:"status","Loading people…"}}
                p{class:"muted workflow-activity-hint","Use a date range of up to 90 days."}
                if let Some(notice)=a.notice{p{id:"workflow-activity-notice",role:"status","{notice}"}}
                if a.detail_row.is_some(){
                    section{id:"workflow-activity-detail",key:"{a.detail_revision}","data-detail-revision":a.detail_revision,"data-detail-loaded":a.detail.is_some().to_string(),tabindex:"-1",
                        div{class:"app-home-toolbar",h4{"Run details"}
                            button{id:"workflow-activity-close-detail",onclick:move |_|close_detail(&mut state.write(),visit,a.detail_revision),"Close run details"}
                        }
                        if let Some(ref detail)=a.detail{
                            p{id:"workflow-activity-detail-status",{presentation::detail_status(&detail)}}
                            if let Some((trigger,texts))=detail_text{
                                if let Some(trigger)=trigger{p{id:"workflow-activity-trigger-message",class:"workflow-activity-message",{message::render(&trigger,media_epoch,state,visit,message::navigation::Location::Trigger)}}}
                                if let Some(stamp)=detail.started(){p{class:"muted",{presentation::time(stamp,false)}}}
                                if detail.steps().is_empty(){p{"No step details were returned for this run."}}
                                ol{class:"workflow-activity-steps",
                                    for (index,step) in detail.steps().iter().enumerate(){
                                        {let outcome=presentation::step_state(step,index,detail.steps().len(),a.detail_row.as_ref());let text=texts[index].clone();rsx!{
                                            li{key:"{index}",p{id:"workflow-activity-step-message-{index}",class:"workflow-activity-message",{message::render(&text,media_epoch,state,visit,message::navigation::Location::Step(index))}}
                                                if outcome.error{span{"Failed"}}
                                                if outcome.success{span{"Completed"}}
                                                span{class:"muted",{presentation::time(&step["last_updated"],true)}}
                                            }
                                        }}
                                    }
                                }
                            }
                        }
                    }
                }else{
                    if a.page.is_some()&&rows.is_empty(){p{id:"workflow-activity-empty","No runs match these filters."}}
                    if a.page.is_none()&&!loading{p{"Apply filters to load activity."}}
                    ul{class:"workflow-activity-results",
                        for (index,row) in rows.iter().enumerate(){
                            {let execution=row["workflow_execution_id"].as_str().unwrap_or_default().to_owned();let children=row["child_execution_details"]["total_count"].as_u64().unwrap_or_default();
                             let parent=routes::is_parent(&a.breadcrumbs,row);let original=row.clone();let name=actor_names[index].clone();rsx!{
                                li{key:"{execution}-{index}",
                                    button{id:format!("workflow-activity-run-{execution}"),class:"workflow-management-row",disabled:loading||blocked,
                                        onclick:move |_|detail(state,visit,execution.clone()),
                                        span{{presentation::list_status(row)}}span{"Started by {name}"}
                                        span{class:"muted",{presentation::time(&row["last_updated"],false)}}
                                        span{{presentation::row_error(row)}}
                                    }
                                    if children>0{
                                        button{id:format!("workflow-activity-children-{}",row["workflow_execution_id"].as_str().unwrap_or_default()),disabled:loading||blocked||parent,
                                            onclick:move |_|child(state,visit,original.clone()),if children==1{"1 child run"}else{"{children} child runs"}}
                                    }
                                }
                            }}
                        }
                    }
                    div{class:"workflow-management-actions",
                        button{id:"workflow-activity-previous",disabled:!previous||loading||blocked,onclick:move |_|load(state,visit,Page::Previous),"Previous"}
                        button{id:"workflow-activity-next",disabled:!more||loading||blocked,onclick:move |_|load(state,visit,Page::Next),"Next"}
                    }
                }
            }
            p{id:"workflow-activity-export-notice",class:"workflow-activity-feedback",role:"status","{export_notice.unwrap_or_default()}"}
            p{id:"workflow-activity-export-native-notice",class:"workflow-activity-feedback",role:"status"}
            if loading&&!exporting{p{id:"workflow-activity-loading",class:"workflow-activity-feedback",role:"status","Loading activity…"}}
            if let Some(notice)=member_notice{
                p{id:"workflow-activity-member-notice",class:"workflow-activity-feedback",role:"status","{notice}"}
            }
            if let Some(notice)=a.name_notice{
                p{id:"workflow-activity-name-notice",class:"workflow-activity-feedback",role:"status","{notice}"}
            }
            if a.detail_row.is_some(){
                p{id:"workflow-activity-navigation-notice",key:"{a.detail_revision}",
                    class:"workflow-activity-feedback",role:"status","data-detail-revision":a.detail_revision}
            }
        }
    }
}
pub(super) fn user(s: &ShellState, team: &str, id: &str) -> String {
    s.core
        .workspaces
        .get(team)
        .and_then(|w| w.users.get(id))
        .map(|u| {
            u.profile
                .as_ref()
                .and_then(|p| p.display_name.as_deref())
                .filter(|v| !v.is_empty())
                .or(u.real_name.as_deref())
                .or(u.name.as_deref())
                .unwrap_or("Person unavailable")
                .to_owned()
        })
        .unwrap_or_else(|| "Person unavailable".into())
}
