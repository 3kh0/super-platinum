use super::*;
pub(crate) use crate::app_home::inputs::external::fixture::capture::{
    capture_image, click, dom, readiness, wait,
};
use dioxus::desktop::DesktopContext;
pub(crate) async fn type_query(value: &str) -> Result<Value, String> {
    let selector = "#app-home [data-modal-users='home-choice'] input[type=search]";
    dom(&format!(r#"(()=>{{{}const e=document.querySelector({});if(!owned(e))return{{ok:false}};e.focus({{preventScroll:true}});Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,value:e.value}};}})()"#,crate::app_home::inputs::external::fixture::capture::ACTION_GUARD,json!(selector),json!(value))).await
}
pub(crate) fn unchanged(
    s: &ShellState,
    expected: &Value,
    registration: bool,
) -> Result<(), String> {
    let now = snapshot(s);
    for key in [
        "authority",
        "rawView",
        "formState",
        "retainedStates",
        "widget",
        "context",
        "mode",
        "channelPolicy",
        "channels",
        "members",
        "conversationFilter",
        "query",
        "inputRevision",
    ] {
        if registration && key == "inputRevision" {
            continue;
        }
        if now[key] != expected[key] {
            return Err(format!("Home channels changed during capture: {key}"));
        }
    }
    Ok(())
}
fn layout(s: &ShellState, expected: &Value, focus: Value) -> Result<String, String> {
    unchanged(s, expected, false)?;
    let available = Ticket::capture(s, &key()).is_some();
    let selected = super::super::display(s, &key()).unwrap_or_default();
    let field = super::super::super::form(s).and_then(|f| f.fields.get(&key()));
    let binding = json!({"authority":expected["authority"],"available":available,"selected":selected,"query":expected["query"],
        "optional":field.map(|f|f.optional),"multiple":is_multiple(),"presentation":expected["widget"]["presentation"],"fieldError":crate::app_home::inputs::presentation_error(s,&key())});
    let geometry = if is_multiple() {
        include_str!("layout-multi.js")
    } else {
        include_str!("layout.js")
    }
    .replace("__SP_FOCUS__", &focus.to_string());
    Ok(format!(
        r#"(()=>{{const x={binding},home=document.getElementById('app-home')||document.querySelector('.app-home');if(!home)return{{ok:false}};
        if(x.authority?.homeCurrent===true){{
            if(home.dataset.visit!==String(x.authority.homeVisit)||home.dataset.revision!==String(x.authority.homeRevision))return{{ok:false}};
            const hosts=[...home.querySelectorAll('[data-home-native-mount]')],book=window.__spHomeCalendarNative;
            if(hosts.length!==1||!book||hosts.some(h=>{{const es=[...book.entries.values()].filter(e=>e.host===h&&e.mount===Number(h.dataset.homeNativeMount));if(es.length!==1)return true;const e=es[0];return e.epoch!==x.authority.accountEpoch||!h.isConnected||e.node.parentNode!==h||e.closed||e.pending||e.seq<1||e.acked!==e.seq||e.node.value!==e.ackedValue;}}))return{{ok:false}};
        }}else if(home.hasAttribute('data-visit')||home.hasAttribute('data-revision')||home.querySelector('[data-modal-users]'))return{{ok:false}};
        if(x.authority?.homeCurrent===true){{
            const field=home.querySelector('[data-modal-users="home-choice"]');if(!field)return{{ok:false}};
            const shown=[...field.querySelectorAll('[data-selected-user]')].map(e=>[e.dataset.selectedUser,e.querySelector('span')?.textContent]);
            if(JSON.stringify(shown)!==JSON.stringify(x.selected))return{{ok:false}};
            if(document.querySelector('.app-modal')&&([...field.querySelectorAll('button,input')].some(e=>!e.disabled)||field.querySelector('input')?.getAttribute('aria-expanded')!=='false'))return{{ok:false}};
        }}
        if(!document.querySelector('.app-modal')&&x.authority?.homeCurrent===true){{
            const field=home.querySelector('[data-modal-users="home-choice"]');if(!field)return{{ok:false}};
            if([...field.querySelectorAll('button,input')].some(e=>e.disabled===x.available))return{{ok:false}};
            const selected=[...field.querySelectorAll('[data-selected-user]')].map(e=>[e.dataset.selectedUser,e.querySelector('span')?.textContent]);
            if(JSON.stringify(selected)!==JSON.stringify(x.selected))return{{ok:false}};
            const input=field.querySelector('input');if(input.value!==(x.query?.text||'')||input.getAttribute('aria-expanded')!==String(x.query!==null))return{{ok:false}};
            if(!!field.querySelector('[data-remove-user]')!==((x.multiple||x.optional===true)&&selected.length>0))return{{ok:false}};
            if(x.multiple){{
                if(input.getAttribute('aria-invalid')!==String(x.presentation.invalid))return{{ok:false}};
                const text=field.querySelector('[data-home-users-limit]')?.textContent||null;if(text!==x.presentation.text)return{{ok:false}};
                if([...field.querySelectorAll('[data-entity-id]')].some(e=>e.getAttribute('role')!=='checkbox'||e.getAttribute('aria-checked')!==String(x.selected.some(v=>v[0]===e.dataset.entityId))))return{{ok:false}};
            }}
            const rows=[...field.querySelectorAll('[data-entity-id]')].map(e=>({{id:e.dataset.entityId,label:e.textContent}}));
            if(JSON.stringify(rows)!==JSON.stringify((x.query?.rows||[]).map(c=>({{id:c.id,label:c.label}}))))return{{ok:false}};
            const error=field.closest('.app-modal-field')?.querySelector(':scope > .app-modal-error')?.textContent.trim()||null;if(error!==x.fieldError)return{{ok:false}};
        }}return {geometry};}})()"#
    ))
}
/// A refreshed native host may acknowledge after model-only ready(). Settle the
/// actual current DOM and registration before invoking its new event closures.
pub(crate) async fn settle_interaction(
    state: Signal<ShellState>,
    d: &DesktopContext,
) -> Result<(), String> {
    let incoming = snapshot(&state.peek());
    let script = layout(&state.peek(), &incoming, Value::Null)?;
    crate::slack_links_check::wait_dom(d, &script).await?;
    unchanged(&state.peek(), &incoming, true)?;
    let settled = snapshot(&state.peek());
    let script = layout(&state.peek(), &settled, Value::Null)?;
    crate::slack_links_check::wait_dom(d, &script).await?;
    unchanged(&state.peek(), &settled, false)
}
pub(crate) async fn shot(
    state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let incoming = snapshot(&state.peek());
    let script = layout(&state.peek(), &incoming, Value::Null)?;
    let mut geometry = crate::slack_links_check::wait_dom(d, &script).await?;
    unchanged(&state.peek(), &incoming, true)?;
    let model = snapshot(&state.peek());
    let inventory = geometry["inventory"].clone();
    let required = geometry["requiredViews"]
        .as_array()
        .ok_or("Missing users coverage")?
        .clone();
    capture_image(d, name, stage, &mut geometry, captures).await?;
    unchanged(&state.peek(), &model, false)?;
    let mut views = vec![];
    for key in required {
        let key = key.as_str().ok_or("Invalid users view key")?;
        let script = layout(&state.peek(), &model, json!(key))?;
        let mut view = crate::slack_links_check::wait_dom(d, &script).await?;
        unchanged(&state.peek(), &model, false)?;
        if view["inventory"] != inventory || view["focusKey"] != key || view["focusVisible"] != true
        {
            return Err("Users scroll changed inventory/visibility".into());
        }
        let capture_key = format!("{stage}-{key}");
        capture_image(d, name, &capture_key, &mut view, captures).await?;
        unchanged(&state.peek(), &model, false)?;
        views.push(json!({"key":key,"captureKey":capture_key,"geometry":view}));
    }
    stages.push(
        json!({"stage":stage,"model":model,"geometry":geometry,"scrollViews":views,"proof":proof}),
    );
    Ok(())
}
