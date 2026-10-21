use super::*;
use crate::{
    icons::{Icon, icon},
    media::MediaUri,
};

pub(super) fn projection_snapshot(p: &Projection) -> Value {
    json!({"title":p.title(),"description":p.description(),"triggerType":p.trigger_type(),
        "workflowId":p.workflow_id(),"app":p.app(),"owningTeam":p.owning_team(),
        "collaborators":p.collaborators(),"steps":p.raw_steps()})
}
pub(super) fn panel(state: Signal<ShellState>, d: Arc<Selection>) -> Element {
    let s = state.read();
    let cooldown = cooling(&s);
    let id = d.id.to_string();
    let current = d.selected(&s);
    let probe = fixture::enabled(&s).then(|| d.snapshot(&s).to_string());
    let m = d.model.lock().expect("workflow details model");
    let status = m.status;
    let initial = d.clone();
    let closing = d.clone();
    let retry = d.clone();
    let projection = m.projection.clone();
    let scope = d
        .owner
        .ctx
        .session
        .enterprise_id
        .as_deref()
        .unwrap_or(&d.owner.ctx.session.team_id);
    let error = m.error;
    rsx! {
        aside {key:"workflow-details-{id}",id:"channel-workflow-details",class:"channel-workflow-details",
            "aria-label":"Workflow details","data-selection":id,"data-status":status,
            "data-current":current.to_string(),"data-serial":m.serial,"data-fixture-readback":probe,
            onmounted:move |_|read::start(state,initial.clone(),true),
            header {class:"channel-workflow-details-header",
                h2 {"Workflow"}
                button {id:"channel-workflow-details-close",class:"icon-btn","aria-label":"Close workflow details",title:"Close",
                    onclick:move |_|close(state,closing.clone()),{icon(Icon::Close,"icon")}}
            }
            div {class:"channel-workflow-details-body",
                if status=="loading" {p {id:"channel-workflow-details-loading",role:"status","Loading workflow details\u{2026}"}}
                if let Some((title,message))=error {
                    div {role:"status",h3{id:"channel-workflow-details-error-title","{title}"}
                        p{id:"channel-workflow-details-error-message","{message}"}}
                }
                if cooldown.blocked {p{role:"status","Slack asked us to wait before loading workflow details."}}
                if matches!(status,"idle"|"failed") {
                    button {id:"channel-workflow-details-retry",disabled:cooldown.blocked,
                        onclick:move |_|read::start(state,retry.clone(),false),"Retry"}
                }
                if let Some(p)=projection {{contents(&s,&p,scope)}}
            }
        }
    }
}
fn contents(s: &ShellState, p: &Projection, scope: &str) -> Element {
    let title = p.title().unwrap_or("Workflow details");
    let description = p.description().unwrap_or_default();
    let media = p
        .app()
        .and_then(super::super::view::image_url)
        .map(|url| {
            s.media
                .register_icon(super_platinum_core::MediaAssetKind::Avatar, url)
        })
        .filter(|id| s.media.is_ready(id))
        .map(|id| id.webview_uri_at(s.media_epoch));
    let owning_workspace = p.owning_team().and_then(|id| s.core.workspaces.get(id));
    let managers = owning_workspace
        .map(|w| {
            p.collaborators()
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|id| {
                    w.users
                        .get(id)
                        .map(|u| super_platinum_core::state::display_name(Some(u), id))
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let created_by = owning_workspace
        .filter(|w| w.team_id != scope && !w.name.is_empty())
        .map(|w| w.name.as_str());
    let label = trigger_label(p);
    rsx! {
        div {class:"channel-workflow-details-summary",
            span {class:"channel-workflow-icon","aria-hidden":"true",
                if let Some(uri)=media {img{src:"{uri}",alt:""}}else{{icon(Icon::Play,"icon")}}}
            h3 {id:"channel-workflow-details-title","{title}"}
        }
        p {id:"channel-workflow-details-description","{description}"}
        if let Some(label)=label {p{class:"channel-workflow-details-trigger","{label}"}}
        if !managers.is_empty() || created_by.is_some(){section{h3{"Information"}
            if !managers.is_empty(){p{"Managed by {managers}"}}
            if let Some(name)=created_by{p{"Created by {name}"}}
        }}
        if let Some(steps)=p.raw_steps().filter(|steps|!steps.is_empty()) {
            section {class:"channel-workflow-details-steps",h3{"Steps"}
                if p.trigger_type()==Some("shortcut"){p{"When someone clicks the workflow link"}}
                {steps_view(steps,0)}
            }
        }
    }
}
/// Exact simple labels from b2Ud. Event summaries and subtype-specific external
/// labels need their separately acquired metadata; do not show raw type codes.
fn trigger_label(p: &Projection) -> Option<&'static str> {
    match p.trigger_type() {
        Some("scheduled") => Some("Starts on a schedule"),
        Some("webhook") => Some("Starts with a Webhook event"),
        Some("external") => Some("Starts with a Salesforce event"),
        Some("service") => Some("Starts with a third party event"),
        _ => None,
    }
}
fn steps_view(steps: &[Value], depth: usize) -> Element {
    if depth >= 12 {
        return rsx! {p{"More step details are unavailable."}};
    }
    rsx! {ol {for (index,step) in steps.iter().take(250).enumerate(){
        li {key:"step-{depth}-{index}",
            if step["type"]=="loop" {span{"Repeat"}}
            else {span{{step.pointer("/function/title").and_then(Value::as_str).unwrap_or("Step details unavailable")}}}
            if let Some(children)=step["steps"].as_array(){
                if step["type"]=="switch" {
                    for (branch_index,branch) in children.iter().take(26).enumerate(){
                        div {class:"channel-workflow-details-branch",
                            if branch.get("expression").is_none_or(Value::is_null){p{"Fallback"}}
                            else {p{"Branch {branch_letter(branch_index)}"}}
                            if let Some(nested)=branch["steps"].as_array(){{steps_view(nested,depth+1)}}
                        }
                    }
                    if children.len()>26 {p {"More branch details are unavailable."}}
                }else{{steps_view(children,depth+1)}}
            }
        }
    }}
    if steps.len()>250 {p {"More step details are unavailable."}}
    }
}
fn branch_letter(index: usize) -> char {
    char::from_u32(u32::from(b'A') + index as u32).unwrap_or('?')
}
