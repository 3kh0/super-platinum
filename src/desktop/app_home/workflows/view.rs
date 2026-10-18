//! Typed speedbump UI. A visible summary never substitutes for an owned response.
use super::*;
use super_platinum_core::block_actions::home::workflow::{Auth, Input, InputType, Variant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Summary,
    Inputs,
    Connections,
    Scopes,
}

fn change_page(mut state: Signal<ShellState>, id: u64, page: Page) {
    let mut s = state.write();
    if !dialog_current(&s, id) || super::busy(&s, id) {
        return;
    }
    let d = s.app_home.workflow.as_mut().unwrap();
    if d.busy {
        return;
    }
    d.canvas = None;
    d.entity = None;
    d.entity_query = next();
    d.page = page;
}
fn done(mut state: Signal<ShellState>, id: u64, page: Page) {
    let mut s = state.write();
    if !dialog_current(&s, id) || super::busy(&s, id) {
        return;
    }
    let d = s.app_home.workflow.as_mut().unwrap();
    if d.busy || d.entity.is_some() {
        return;
    }
    let ready = match page {
        Page::Inputs => d.review.review_inputs().is_ok(),
        Page::Connections => d.review.review_connections().is_ok(),
        Page::Scopes => true,
        Page::Summary => false,
    };
    if ready {
        d.page = Page::Summary;
    }
}
pub(crate) fn panel(state: Signal<ShellState>) -> Element {
    let s = state.read();
    let Some(d) = s
        .app_home
        .workflow
        .as_ref()
        .filter(|d| !d.dismissed && d.origin.current(&s))
    else {
        return rsx! {};
    };
    let id = d.id;
    let review = d.review.clone();
    let connection_busy = d.busy;
    let busy = super::busy(&s, id);
    let page = d.page;
    let title = d
        .metadata
        .as_ref()
        .map(|m| m.title.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Run workflow".into());
    let notice = d.notice.clone();
    let multiple = review.variant() == Variant::Multiple;
    let unconfirmed_connection = salesforce_dispatch::unresolved(&s, &d.origin);
    let entities_ready = entity_picker::ready(&s, id, &review);
    let uncertain =
        s.app_home.workflow_attempts.iter().any(|a| {
            a.origin.workflow.same_action(&d.origin.workflow) && a.phase == Phase::Uncertain
        });
    let page = if multiple {
        page
    } else {
        match review.variant() {
            Variant::Inputs => Page::Inputs,
            Variant::Authentication => Page::Connections,
            Variant::Scopes => Page::Scopes,
            _ => Page::Summary,
        }
    };
    let ready = match page {
        Page::Summary => review.ready(),
        Page::Inputs => review.inputs_ready(),
        Page::Connections => review.auth_ready(),
        Page::Scopes => true,
    };
    let heading = match page {
        Page::Summary => {
            if multiple {
                "Complete these steps to get started"
            } else {
                "Run workflow"
            }
        }
        Page::Inputs => "Provide inputs",
        Page::Connections => "Connect accounts",
        Page::Scopes => "Review workflow access",
    };
    drop(s);
    rsx! {
      div {class:"scrim",onclick:move |_|cancel(state,id)}
      section {id:"home-workflow-dialog",class:"modal app-modal app-home-workflow",role:"dialog","aria-modal":"true","aria-labelledby":"home-workflow-title",tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":id,
        onkeydown:move |e|if e.key()==dioxus::prelude::Key::Escape {e.prevent_default();e.stop_propagation();cancel(state,id);},
        onmounted:move |_|{dioxus::document::eval("requestAnimationFrame(()=>document.querySelector('#home-workflow-dialog button')?.focus({preventScroll:true}));");},
        header {h2 {id:"home-workflow-title","{heading}"}button {r#type:"button",class:"icon-btn","aria-label":"Cancel workflow",onclick:move |_|cancel(state,id),{crate::icons::icon(crate::icons::Icon::Close,"icon")}}}
        div {class:"app-modal-body",
          p {class:"app-home-workflow-name","{title}"}
          match page {
            Page::Summary=>rsx!{
              if multiple && review.show_inputs() {section {class:"app-home-workflow-step",h3 {"Missing input values"}p {if review.inputs_confirmed(){"Inputs reviewed"}else{"Provide the values this workflow needs before it starts."}}button {disabled:busy||uncertain,onclick:move |_|change_page(state,id,Page::Inputs),if review.inputs_confirmed(){"View inputs"}else{"Provide inputs"}}}}
              if multiple && review.show_auth() {section {class:"app-home-workflow-step",h3 {"Third-party connection needed"}p {if review.connections_confirmed(){"Accounts reviewed"}else{"Connect the accounts used by this workflow."}}button {disabled:busy||uncertain,onclick:move |_|change_page(state,id,Page::Connections),if review.connections_confirmed(){"View accounts"}else{"Connect accounts"}}}}
              if multiple && review.show_scopes() {section {class:"app-home-workflow-step",h3 {"Review workflow access"}p {"This workflow may share information with other apps or people in your workspace."}button {disabled:busy||uncertain,onclick:move |_|change_page(state,id,Page::Scopes),"Learn more"}}}
              {ai_access(&review)}
            },
            Page::Inputs=>rsx!{
              for field in review.context() {{input(state,id,field.clone(),false,busy||uncertain)}}
              for field in review.custom() {{input(state,id,field.clone(),true,busy||uncertain)}}
              {canvas_picker::panel(state,id)}
              {entity_picker::panel(state,id)}
            },
            Page::Connections=>rsx!{
                if let Some(panel)=salesforce_panel::active_panel(state,id,connection_busy||uncertain) {{panel}}
                else {for auth in review.connections() {{connection(state,id,auth.clone(),connection_busy||uncertain)}}}
            },
            Page::Scopes=>rsx!{h3 {"What will the workflow be able to do?"}ul {for scope in review.bot_scopes().iter().filter_map(|v|v["description"].as_str()).filter(|v|!v.is_empty()) {li {"{scope}"}}}},
          }
          if let Some(notice)=notice {p {class:"app-modal-status",role:"status","{notice}"}}
          if busy {p {role:"status","Waiting for Slack…"}}
        }
        footer {
          if multiple && page!=Page::Summary {button {r#type:"button",disabled:busy,onclick:move |_|change_page(state,id,Page::Summary),"Back"}}
          button {r#type:"button",onclick:move |_|cancel(state,id),"Cancel"}
          if multiple && page!=Page::Summary {
            button {r#type:"button",class:"primary",disabled:busy||uncertain||!ready,onclick:move |_|done(state,id,page),"Done"}
          } else {
            button {r#type:"button",class:"primary",disabled:busy||uncertain||unconfirmed_connection||!entities_ready||!review.ready(),onclick:move |_|start(state,id),"Start workflow"}
          }
        }
      }
    }
}
fn ai_access(review: &Preflight) -> Element {
    if !review.contains_ai() {
        return rsx! {};
    }
    let channel = review
        .required_context()
        .iter()
        .any(|p| p["name"] == "channel_id");
    let canvas = review
        .required_context()
        .iter()
        .any(|p| p["name"] == "canvas_id");
    let scope = match (channel, canvas) {
        (true, true) => "channel and canvas",
        (true, false) => "channel",
        (false, true) => "canvas",
        _ => return rsx! {},
    };
    rsx! {section {class:"app-home-workflow-step",h3 {"Review AI workflow access"}p {"This workflow uses AI. Contents of the {scope} may be shared by the workflow and become visible to its builder. Review this access before continuing."}}}
}
fn input(state: Signal<ShellState>, id: u64, field: Input, custom: bool, busy: bool) -> Element {
    let title = if field.title.is_empty() {
        field.name.clone()
    } else {
        field.title.clone()
    };
    let name = field.name.clone();
    let description = field.description.clone();
    match field.kind {
        InputType::Canvas => canvas_picker::field(state, id, name, title),
        InputType::Text => {
            let value = state
                .read()
                .app_home
                .workflow
                .as_ref()
                .filter(|d| d.id == id)
                .and_then(|d| d.review.value(&name, custom))
                .unwrap_or_default()
                .to_owned();
            rsx! {div {class:"app-home-workflow-field",label {"{title}" input {r#type:"text",value,disabled:busy,required:true,oninput:move |e|set(state,id,&name,e.value(),custom)}}if !description.is_empty(){p {"{description}"}}}}
        }
        InputType::Conversation | InputType::User => {
            super::entity_picker::field(state, id, field, custom)
        }
    }
}
fn connection(state: Signal<ShellState>, id: u64, auth: Auth, busy: bool) -> Element {
    let title = if auth.label.is_empty() {
        "Connected account".into()
    } else {
        auth.label.clone()
    };
    let function = auth.function.clone();
    let provider = auth.provider.clone();
    let function_select = function.clone();
    let provider_select = provider.clone();
    let selected = auth
        .tokens
        .iter()
        .find(|t| t.default)
        .or_else(|| auth.tokens.first())
        .map(|t| t.id.clone())
        .unwrap_or_default();
    rsx! {section {class:"app-home-workflow-step",h3 {"{title}"}
        if !auth.tokens.is_empty() {
            label {"Account" select {value:selected,disabled:busy,
                onchange:move |e|connect(state,id,function_select.clone(),provider_select.clone(),Some(e.value())),
                option {value:"",disabled:true,"Choose an account"}
                for (index,token) in auth.tokens.iter().enumerate() {option {value:token.id.clone(),if token.user.is_empty(){"Connected account {index+1}"}else{"{token.user}"}}}
            }}
        }
        button {r#type:"button",disabled:busy,onclick:move |_|connect(state,id,function.clone(),provider.clone(),None),if auth.tokens.is_empty(){"Connect account"}else{"Connect another account"}}
    }}
}
