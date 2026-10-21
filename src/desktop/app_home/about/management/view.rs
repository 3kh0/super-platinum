use super::*;
pub(crate) fn panel(mut state: Signal<ShellState>) -> Element {
    let s = state.read();
    let Some(h) = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .filter(|h| visible(&s, h.id))
    else {
        return rsx! {};
    };
    let visit = h.id;
    let direct = h.entry.is_some();
    let busy = h.read.is_some();
    let query = h.query.clone();
    let rows: Vec<_> = h
        .list
        .visible_rows()
        .into_iter()
        .map(|r| {
            (
                r.id().to_owned(),
                r.raw()["title"]
                    .as_str()
                    .unwrap_or("Untitled workflow")
                    .to_owned(),
                r.raw()["is_published"] == true,
            )
        })
        .collect();
    let more = h.list.next_query().is_some();
    let surface_notice = h.notice.clone();
    let d = document(&s, visit).cloned();
    let activity_active = d.as_ref().is_some_and(|d| d.activity.active);
    let confirmation = delete::confirmation(&s, visit);
    let confirming = delete::confirming(&s, visit)
        || unpublish::confirming(&s, visit)
        || permissions::confirming(&s, visit);
    let unpublish_confirmation = unpublish::confirmation(&s, visit);
    let unpublish_owned = unpublish::owns(&s, visit);
    let permission_confirmation = permissions::confirmation(&s, visit);
    let permission_selected = permissions::selected(&s, visit).unwrap_or_default();
    let permission_owned = permissions::owns(&s, visit);
    let can_permissions = permissions::available(&s, visit);
    let can_save_permissions = permissions::save_enabled(&s, visit);
    let team_name = h.owner.ctx.session.name.clone();
    let can_unpublish = unpublish::available(&s, visit);
    let delete_owned = delete::owns(&s, visit);
    let deleted = delete::terminal(&s, visit);
    let can_activity = activity::permitted(&s, visit) && !busy && !confirming && !delete_owned;
    let can_edit = editable(&s, visit);
    let can_save = can_edit
        && d.as_ref().is_some_and(|d| {
            d.dirty()
                && d.editor
                    .metadata_command(&d.title, &d.description, None)
                    .is_ok()
        });
    let outcomes = write::outcomes(&s, visit);
    let document_id = d.as_ref().map_or(0, |d| d.id);
    let revision = d.as_ref().map_or(0, |d| d.revision);
    let policy_notice=d.as_ref().filter(|_|!can_edit&&!busy&&!confirming&&!delete_owned).and_then(|d|d.pending.is_none().then_some(
        "Editing is unavailable until current permissions, collaborator access, step availability, and trigger metadata are verified."));
    drop(s);
    rsx! {
        section{id:"workflow-management",class:"app-home workflow-management","data-management-visit":visit,
            "data-management-document":document_id,"data-management-revision":revision,
            "data-management-editable":if can_edit{"true"}else{"false"},
            onmounted:move |_|native::mount(visit),
            div{class:"app-home-toolbar",
                h2{if d.is_some(){"Workflow details"}else if direct{"Workflow activity"}else{"Managed by you"}}
                span{class:"spacer"}
                button{id:if direct{"workflow-entry-back"}else{"workflow-management-back"},r#type:"button",onclick:move |_|{
                    if direct {entry::leave(state,visit);}else if visible(&state.peek(),visit){state.write().app_home.about.management.surface=None;}
                },if direct{"Back to conversation"}else{"Back to About"}}
            }
            if direct {p{id:"workflow-entry-navigation-notice",class:"app-home-notice",role:"status"}}
            if activity_active {
                {activity::panel(state,visit)}
            }else if let Some(d)=d{
                div{class:"app-home-content workflow-management-fields",onmounted:move |_|entry::ready(state,visit),
                    label{r#for:"workflow-management-title","Name"}
                    input{id:"workflow-management-title",r#type:"text",value:d.title.clone(),readonly:!can_edit,
                        oninput:move |e|edit(state,visit,true,e.value())}
                    if let Err(error)=protocol::validate_name(&d.title){p{role:"alert",class:"app-home-notice",{name_error(error)}}}
                    label{r#for:"workflow-management-description","Description"}
                    textarea{id:"workflow-management-description",rows:4,value:d.description.clone(),readonly:!can_edit,
                        oninput:move |e|edit(state,visit,false,e.value())}
                }
                div{class:"workflow-management-feedback",
                    p{id:"workflow-management-navigation-notice",role:"status"}
                    if let Some(notice)=surface_notice{p{role:"status","{notice}"}}
                    if let Some(notice)=d.notice{p{id:if delete_owned{"workflow-delete-status"}else if unpublish_owned{"workflow-unpublish-status"}else if permission_owned{"workflow-permissions-status"}else{"workflow-management-document-notice"},role:"status","{notice}"}}
                    if let Some(notice)=policy_notice{p{role:"status","{notice}"}}
                    if busy{p{role:"status","Checking workflow details and permissions…"}}
                    for outcome in outcomes{p{class:"workflow-management-outcome","{outcome}"}}
                }
                if let Some(confirmation)=confirmation {
                    section{id:"workflow-delete-confirmation",class:"workflow-management-feedback",role:"region","aria-labelledby":"workflow-delete-heading",
                        "data-confirmation":confirmation,
                        h3{id:"workflow-delete-heading","Permanently delete this workflow?"}
                        p{"When you delete this workflow, it will stop working for anyone who uses it. This cannot be undone. Your local draft will be retained."}
                        p{id:"workflow-delete-native-notice",role:"status"}
                        div{class:"workflow-management-actions",
                            button{id:"workflow-delete-cancel",r#type:"button",onclick:move |_|delete::cancel(state,visit,confirmation),"Cancel"}
                            button{id:"workflow-delete-confirm",class:"danger",r#type:"button",onclick:move |_|delete::confirm(state,visit,confirmation),"Delete"}
                        }
                    }
                }
                if let Some(confirmation)=unpublish_confirmation {
                    section{id:"workflow-unpublish-confirmation",class:"workflow-management-feedback",role:"region","aria-labelledby":"workflow-unpublish-heading","data-confirmation":confirmation,
                        h3{id:"workflow-unpublish-heading","Unpublish this workflow?"}
                        p{"When you unpublish this workflow, it will stop working for anyone who uses it. Shared links will not start the workflow unless it is published again. Your local draft will be retained."}
                        p{id:"workflow-unpublish-native-notice",role:"status"}
                        div{class:"workflow-management-actions",
                            button{id:"workflow-unpublish-cancel",r#type:"button",onclick:move |_|unpublish::cancel(state,visit,confirmation),"Cancel"}
                            button{id:"workflow-unpublish-confirm",class:"danger",r#type:"button",onclick:move |_|unpublish::confirm(state,visit,confirmation),"Unpublish"}
                        }
                    }
                }
                if let Some(id)=permission_confirmation {
                    section{id:"workflow-permissions",class:"workflow-management-feedback",role:"region","aria-labelledby":"workflow-permissions-heading","data-confirmation":id,
                        h3{id:"workflow-permissions-heading","Permissions"}
                        label{r#for:"workflow-permissions-copy","Who can copy this workflow?"}
                        select{id:"workflow-permissions-copy",value:permission_selected,
                            onmounted:move |_|permissions::bind(state,visit,id),
                            onchange:move |e|permissions::select(state,visit,id,e.value()),
                            option{value:"everyone","Everyone in {team_name}"}
                            option{value:"collaborators","Workflow managers only"}
                        }
                        {permissions::settings::panel(state,visit,id)}
                        {permissions::managers::panel(state,visit,id)}
                        p{id:"workflow-permissions-native-notice",role:"status"}
                        div{class:"workflow-management-actions",
                            button{id:"workflow-permissions-cancel",r#type:"button",onclick:move |_|permissions::cancel(state,visit,id),"Cancel"}
                            button{id:"workflow-permissions-save",r#type:"button",disabled:!can_save_permissions,onclick:move |_|permissions::confirm(state,visit,id),"Save"}
                        }
                    }
                }
                div{class:"workflow-management-actions",
                    button{id:"workflow-management-activity",r#type:"button",disabled:!can_activity,onclick:move |_|activity::show(state,visit),"Activity"}
                    if !direct {button{id:"workflow-management-managed",r#type:"button",disabled:busy||confirming,onclick:move |_|read::list(state,visit,false),"Managed by you"}}
                    button{id:if permission_owned{"workflow-permissions-check"}else{"workflow-management-check"},r#type:"button",disabled:busy||confirming||delete_owned,onclick:move |_|read::editor(state,visit,d.editor.workflow_id().into()),"Check current workflow"}
                    if d.observed.is_some()&&!delete_owned&&!unpublish_owned&&!permission_owned{button{id:"workflow-management-use-current",r#type:"button",disabled:busy||confirming,onclick:move |_|write::use_current(state,visit),"Use current workflow"}}
                    if !deleted {button{id:"workflow-management-delete",class:"danger",r#type:"button",disabled:!can_edit,onclick:move |_|delete::open(state,visit),"Delete workflow"}}
                    if d.editor.raw_workflow()["is_published"]==true {button{id:"workflow-management-unpublish",class:"danger",r#type:"button",disabled:!can_unpublish,onclick:move |_|unpublish::open(state,visit),"Unpublish workflow"}}
                    button{id:"workflow-management-permissions",r#type:"button",disabled:!can_permissions,onclick:move |_|permissions::open(state,visit),"Permissions"}
                    button{id:"workflow-management-save",r#type:"button",disabled:!can_save,onclick:move |_|write::save(state,visit),"Save details"}
                }
            }else if direct {
                div{class:"app-home-content",
                    if busy {p{role:"status","Loading workflow details and permissions..."}}
                    if let Some(notice)=surface_notice {p{id:"workflow-entry-notice",role:"status","{notice}"}}
                    button{id:"workflow-entry-retry",r#type:"button",disabled:busy,onclick:move |_|entry::retry(state,visit),"Retry"}
                }
            }else{
                form{class:"workflow-management-filters",onsubmit:move |e|{e.prevent_default();read::list(state,visit,false);},
                    input{id:"workflow-management-query",r#type:"search","aria-label":"Search managed workflows",value:query.query,disabled:busy,
                        oninput:move |e|{if visible(&state.peek(),visit){state.write().app_home.about.management.surface.as_mut().unwrap().query.query=e.value();}}}
                    select{id:"workflow-management-status","aria-label":"Workflow status",disabled:busy,
                        value:match query.published{None=>"all",Some(true)=>"published",Some(false)=>"draft"},
                        onchange:move |e|{if visible(&state.peek(),visit){state.write().app_home.about.management.surface.as_mut().unwrap().query.published=match e.value().as_str(){"published"=>Some(true),"draft"=>Some(false),_=>None};}},
                        option{value:"all","All statuses"}option{value:"published","Published"}option{value:"draft","Unpublished"}}
                    select{id:"workflow-management-sort","aria-label":"Sort workflows",disabled:busy,value:if query.sort==Sort::Ascending{"asc"}else{"desc"},
                        onchange:move |e|{if visible(&state.peek(),visit){state.write().app_home.about.management.surface.as_mut().unwrap().query.sort=if e.value()=="asc"{Sort::Ascending}else{Sort::Descending};}},
                        option{value:"desc","Recently updated"}option{value:"asc","Oldest updated"}}
                    button{id:"workflow-management-search",r#type:"submit",disabled:busy,"Search"}
                }
                div{class:"app-home-content",
                    if busy{p{role:"status","Loading managed workflows…"}}
                    if let Some(notice)=surface_notice{p{role:"status","{notice}"}}
                    if rows.is_empty()&&!busy{p{"No matching workflows."}}
                    for (id,title,published) in rows{
                        button{id:format!("workflow-management-open-{id}"),class:"workflow-management-row",r#type:"button",disabled:busy,
                            onclick:move |_|read::editor(state,visit,id.clone()),span{"{title}"}span{if published{"Published"}else{"Unpublished"}}}
                    }
                    if more{button{id:"workflow-management-more",r#type:"button",disabled:busy,onclick:move |_|read::list(state,visit,true),"Load more"}}
                }
            }
        }
    }
}
fn name_error(error: protocol::NameError) -> &'static str {
    match error {
        protocol::NameError::Required => "Enter a workflow name.",
        protocol::NameError::TooLong => "Use a name of 80 characters or fewer.",
        protocol::NameError::Reserved => "This name is reserved. Choose another name.",
        protocol::NameError::Emoji => "Workflow names cannot contain emoji.",
        protocol::NameError::RestrictedCharacters | protocol::NameError::SpecialCharacters => {
            "Remove unsupported characters from the workflow name."
        }
    }
}
