pub(super) use super::super::fixture::capture::{
    capture_image, click, dom, readiness, type_query, wait,
};
use super::*;
use dioxus::desktop::DesktopContext;
// Model readiness can precede the next Dioxus paint. Bind the actual rendered
// owner and controls to this exact source snapshot before accepting geometry.
fn layout(state: &ShellState, expected: &Value, focus: Value) -> Result<String, String> {
    unchanged(state, expected)?;
    let available = Ticket::capture(state, &fixture::key()).is_some();
    let expanded = owner(state, &fixture::key()).is_some();
    let visible = state
        .app_home
        .external
        .picker
        .as_ref()
        .map_or(0, |p| p.query.visible);
    let rows = expected["query"]["rows"]
        .as_array()
        .map(|rows| rows.iter().take(visible).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let field_error = crate::app_home::inputs::presentation_error(state, &fixture::key());
    let binding = json!({"authority":expected["authority"],"widget":expected["widget"],
        "available":available,"expanded":expanded,"query":expected["query"]["text"],"rows":rows,"fieldError":field_error});
    let geometry = include_str!("layout.js").replace("__SP_FOCUS__", &focus.to_string());
    Ok(format!(
        r#"(()=>{{
        const expected={binding},home=document.getElementById('app-home')||document.querySelector('.app-home');
        if(!home)return{{ok:false,settling:'home'}};
        if(expected.authority?.homeCurrent===true){{
            if(home.dataset.visit!==String(expected.authority.homeVisit)||home.dataset.revision!==String(expected.authority.homeRevision))return{{ok:false,settling:'home-owner'}};
            // This fixture owns one actual date control. A previous mount's
            // ready model cannot certify the new rendered host's registration.
            const hosts=[...home.querySelectorAll('[data-home-native-mount]')],book=window.__spHomeCalendarNative;
            if(hosts.length!==1||!book||hosts.some(host=>{{
                const entries=[...book.entries.values()].filter(e=>e.host===host&&e.mount===Number(host.dataset.homeNativeMount));
                if(entries.length!==1)return true;
                const e=entries[0];
                return e.epoch!==expected.authority.accountEpoch||!host.isConnected||e.node.parentNode!==host||e.closed||e.type!=='date'||
                    e.pending||e.seq<1||e.acked!==e.seq||e.node.value!==e.ackedValue;
            }}))return{{ok:false,settling:'native-registration'}};
        }}else{{
            // A retired owner renders the empty Home, never its old Surface IDs.
            if(home.hasAttribute('data-visit')||home.hasAttribute('data-revision')||home.querySelector('[data-home-external],[data-external-search]'))return{{ok:false,settling:'retired-home'}};
        }}
        if(!document.querySelector('.app-modal')){{
            const field=home.querySelector('[data-home-external]');
            if(expected.widget!==null&&!field)return{{ok:false,settling:'field'}};
            if(field){{
                const controls=[...field.querySelectorAll('[data-external-open],[data-option-value],input[type=search],[data-external-done],[data-option-index]')];
                if(controls.some(e=>e.disabled===expected.available))return{{ok:false,settling:'availability'}};
                const search=field.querySelector('input[type=search]');
                if(!!search!==expected.expanded)return{{ok:false,settling:'expansion'}};
                if(search&&search.value!==expected.query)return{{ok:false,settling:'query'}};
                if(expected.widget!==null){{
                    const labels=[...field.querySelectorAll('.message-multi-selected-option > span')].map(e=>e.textContent);
                    if(JSON.stringify(labels)!==JSON.stringify(expected.widget.visible.map(o=>o.text.text)))return{{ok:false,settling:'selection'}};
                    const presentation=expected.widget.presentation,status=field.querySelector('[data-home-multi-status]');
                    if(field.getAttribute('aria-invalid')!==String(presentation.invalid)||
                        (presentation.text===null?!!status:!status||status.textContent!==presentation.text||status.dataset.warning!==String(presentation.warning)))return{{ok:false,settling:'presentation'}};
                }}
                const error=field.closest('.app-modal-field')?.querySelector(':scope > .app-modal-error')?.textContent.trim()||null;
                if(error!==expected.fieldError)return{{ok:false,settling:'field-error'}};
                if(search){{
                    const rows=[...field.querySelectorAll('[data-option-index]')];
                    if(rows.length!==expected.rows.length||rows.some((e,i)=>{{const option=expected.rows[i].option,label=e.closest('label');return !label||label.querySelector('.block-radio-label')?.textContent!==option.text.text||
                        e.checked!==expected.widget.visible.some(o=>o.value===option.value);}}))return{{ok:false,settling:'rows'}};
                }}
            }}
        }}
        return {geometry};
    }})()"#
    ))
}
pub(super) fn unchanged(state: &ShellState, expected: &Value) -> Result<(), String> {
    let now = snapshot(state);
    // Retired-stage homeCurrent=false is a captured fact, never fresh authority.
    for key in [
        "authority",
        "inputRevision",
        "formState",
        "widget",
        "contextSource",
        "query",
    ] {
        if now[key] != expected[key] {
            return Err(format!("Home source changed during capture: {key}"));
        }
    }
    Ok(())
}
pub(crate) async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let incoming = snapshot(&state.peek());
    let initial_layout = layout(&state.peek(), &incoming, Value::Null)?;
    let mut geometry = crate::slack_links_check::wait_dom(desktop, &initial_layout).await?;
    let before_model = snapshot(&state.peek());
    // Refresh paints a new native host. Its real mounted acknowledgement calls
    // native::receive -> bump before JS clears pending/acknowledges the sequence.
    // Permit that registration revision only before the first capture edge;
    // every semantic source remains fixed, and all later edges bind this exact
    // settled input revision through unchanged(). Never synthesize registration.
    for key in [
        "authority",
        "rawView",
        "formState",
        "retainedStates",
        "widget",
        "contextSource",
        "query",
    ] {
        if before_model[key] != incoming[key] {
            return Err(format!("Home source changed during DOM settlement: {key}"));
        }
    }
    if before_model["authority"]["homeCurrent"] == true
        && !crate::app_home::inputs::ready(&state.peek())
    {
        return Err("Current native Home registration is not ready".into());
    }
    let inventory = geometry["inventory"].clone();
    let required = geometry["requiredViews"]
        .as_array()
        .ok_or("Missing scroll coverage inventory")?
        .clone();
    capture_image(desktop, name, stage, &mut geometry, captures).await?;
    unchanged(&state.peek(), &before_model)?;
    let mut scroll_views = Vec::new();
    for key in required {
        let key = key.as_str().ok_or("Invalid scroll coverage key")?;
        let scoped_layout = layout(&state.peek(), &before_model, json!(key))?;
        let mut view = crate::slack_links_check::wait_dom(desktop, &scoped_layout).await?;
        unchanged(&state.peek(), &before_model)?;
        if view["inventory"] != inventory || view["focusKey"] != key || view["focusVisible"] != true
        {
            return Err(
                "Scroll capture changed result/control inventory or lost target visibility".into(),
            );
        }
        let capture_key = format!("{stage}-{key}");
        capture_image(desktop, name, &capture_key, &mut view, captures).await?;
        unchanged(&state.peek(), &before_model)?;
        let row_index = key
            .strip_prefix("row-")
            .and_then(|i| i.parse::<usize>().ok());
        let option = row_index
            .and_then(|i| before_model["query"]["rows"].get(i))
            .map(|row| row["option"].clone());
        if row_index.is_some() && option.is_none() {
            return Err("Scroll row has no current source option".into());
        }
        scroll_views.push(json!({"key":key,"captureKey":capture_key,"rowIndex":row_index,"option":option,"geometry":view}));
    }
    let after_model = snapshot(&state.peek());
    if before_model["query"]["rows"] != after_model["query"]["rows"]
        || before_model["formState"] != after_model["formState"]
        || before_model["widget"] != after_model["widget"]
    {
        return Err("Scrolling changed current result inventory or selected form state".into());
    }
    stages.push(json!({"stage":stage,"geometry":geometry,"scrollViews":scroll_views,"model":after_model,"proof":proof}));
    Ok(())
}
