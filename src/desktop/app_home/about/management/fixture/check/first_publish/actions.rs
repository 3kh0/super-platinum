use super::*;

pub(super) async fn open(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let activation = if confirmation::share::drafts::requested() {
        confirmation::share::drafts::open_finish(desktop).await?
    } else {
        click(desktop, "workflow-management-finish-up").await?;
        Value::Null
    };
    until(state, visit, "original Finish Up controls", |p, _| {
        p["confirmation"]["current"] == true
            && p["confirmation"]["local"]["valid"] == true
            && p["confirmation"]["local"]["avatar"]["paneBound"] == true
    })
    .await?;
    native(desktop, "modal").await?;
    Ok(activation)
}
pub(super) async fn details(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<(), String> {
    input_ready(state, desktop, visit).await?;
    input(desktop, "workflow-finish-title", LOCAL_TITLE).await?;
    until(state, visit, "original local title", |p, _| {
        p["confirmation"]["local"]["title"] == LOCAL_TITLE
    })
    .await?;
    input_ready(state, desktop, visit).await?;
    input(desktop, "workflow-finish-description", LOCAL_DESCRIPTION).await?;
    until(state, visit, "original local description", |p, _| {
        p["confirmation"]["local"]["description"] == LOCAL_DESCRIPTION
    })
    .await?;
    input_ready(state, desktop, visit).await?;
    native(desktop, "observe").await?;
    Ok(())
}
/// Observe the committed original render/binding before the next independent
/// input. This never dispatches, retries, or rewrites an input event.
async fn input_ready(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<(), String> {
    let confirmation = product::snapshot(&state.peek(), visit)["confirmation"].clone();
    let local = &confirmation["local"];
    if confirmation["current"] != true || local["avatar"]["pane"] != "details" {
        return Err("Original Finish Up details input owner is unavailable".into());
    }
    let expected = json!({"owner":confirmation["id"].as_u64().ok_or("Original Finish Up owner absent")?.to_string(),
        "epoch":local["epoch"].as_u64().ok_or("Original Finish Up epoch absent")?.to_string(),
        "title":local["title"],"description":local["description"],"icon":local["iconUrl"]});
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "original Finish Up input DOM/model reconciliation",
        &include_str!("input_ready.js").replace("EXPECTED", &expected.to_string()),
    )
    .await?;
    let current = product::snapshot(&state.peek(), visit)["confirmation"].clone();
    if current["id"] != confirmation["id"]
        || current["current"] != true
        || current["local"]["avatar"]["paneBound"] != true
        || ["epoch", "title", "description", "iconUrl"]
            .iter()
            .any(|key| current["local"][*key] != local[*key])
    {
        return Err("Original Finish Up input owner changed while rendering".into());
    }
    Ok(())
}
pub(super) async fn cancel(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    cancel_button(state, desktop, visit, "workflow-finish-cancel").await
}
pub(super) async fn cancel_button(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    button: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    click(desktop, button).await?;
    until(state, visit, "Finish Up cancelled", |p, _| {
        p["confirmation"].is_null()
    })
    .await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('workflow-finish-up')&&document.getElementById('workflow-management-title')?.readOnly===false})"#).await?;
    let after = observe(state, desktop, visit).await?;
    if before["server"] != after["server"]
        || before["model"]["title"] != after["model"]["title"]
        || before["model"]["description"] != after["model"]["description"]
        || [
            "id",
            "revision",
            "title",
            "description",
            "editor",
            "trigger",
            "pending",
        ]
        .iter()
        .any(|key| {
            before["firstPublish"]["document"][*key] != after["firstPublish"]["document"][*key]
        })
        || after["native"]["originalNodes"] != true
    {
        return Err("Cancel changed original draft, owner, or server".into());
    }
    Ok(json!({"before":before,"after":after}))
}
async fn select(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    id: &str,
    value: &str,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById({});if(!e||e.disabled)return{{ok:false}};e.scrollIntoView({{block:'center',behavior:'instant'}});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true}};}})()"#,json!(id),json!(value))).await?;
    until(state, visit, "actual embedded selection", |p, _| {
        if id == "workflow-permissions-copy" {
            p["confirmation"]["copy"] == value
        } else {
            p["confirmation"]["runDraft"]["mode"] == value
        }
    })
    .await?;
    native(desktop, "observe").await?;
    Ok(())
}
async fn target(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    id: &str,
    add: bool,
) -> Result<Value, String> {
    let attr = if add {
        "managerResult"
    } else {
        "managerSelected"
    };
    let selector = if add {
        "[data-manager-result]"
    } else {
        "[data-manager-selected]"
    };
    let clicked=wait_dom(desktop,&format!(r#"(()=>{{const root=document.getElementById('workflow-finish-up'),rows=root?[...root.querySelectorAll({})].filter(e=>e.dataset[{}]==={}):[];if(rows.length!==1||rows[0].disabled)return{{ok:false}};const e=rows[0];e.scrollIntoView({{block:'center',behavior:'instant'}});const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};const proof={{ok:true,target:e.dataset[{}],text:e.textContent,connected:e.isConnected,hit:true}};e.click();return proof;}})()"#,json!(selector),json!(attr),json!(id),json!(attr))).await?;
    until(
        state,
        visit,
        "original embedded manager selection",
        |p, _| {
            p["confirmation"]["managerDraft"]["ids"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v == id) == add)
        },
    )
    .await?;
    Ok(clicked)
}
pub(super) async fn settings(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    select(
        state,
        desktop,
        visit,
        "workflow-permissions-run",
        "everyone",
    )
    .await?;
    click(desktop, "workflow-finish-show-more").await?;
    until(state, visit, "actual Show more", |p, _| {
        p["confirmation"]["local"]["showMore"] == true
    })
    .await?;
    select(
        state,
        desktop,
        visit,
        "workflow-permissions-copy",
        "collaborators",
    )
    .await?;
    let generation =
        product::snapshot(&state.peek(), visit)["confirmation"]["managerDraft"]["generation"]
            .as_u64()
            .ok_or("Manager query generation absent")?;
    input(desktop, "workflow-permissions-manager-query", "manager").await?;
    until(state, visit, "original manager query result", |p, _| {
        let m = &p["confirmation"]["managerDraft"];
        m["query"] == "manager"
            && m["pending"] == false
            && m["generation"].as_u64().is_some_and(|v| v > generation)
            && m["results"]
                .as_array()
                .is_some_and(|v| v.iter().any(|id| id == "UMANNEW01"))
    })
    .await?;
    let queried = observe(state, desktop, visit).await?;
    let added = target(state, desktop, visit, "UMANNEW01", true).await?;
    let removed = target(state, desktop, visit, "UMANOLD01", false).await?;
    let after = observe(state, desktop, visit).await?;
    if after["model"]["writes"] != 0 || before["native"]["fields"] != after["native"]["fields"] {
        return Err("Embedded choices changed parent or wrote early".into());
    }
    Ok(json!({"before":before,"queried":queried,"added":added,"removed":removed,"after":after}))
}
