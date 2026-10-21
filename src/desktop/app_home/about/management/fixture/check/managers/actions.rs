use super::*;
pub(super) async fn open(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<(), String> {
    click(desktop, "workflow-management-permissions").await?;
    until(state, visit, "original manager controls", |d, _| {
        d["confirmation"]["current"] == true
    })
    .await?;
    native(desktop, false).await?;
    Ok(())
}
pub(super) async fn select(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    id: &str,
    value: &str,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById({});if(!e||e.disabled)return{{ok:false}};e.scrollIntoView({{block:'center',behavior:'instant'}});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true}};}})()"#,json!(id),json!(value))).await?;
    until(state, visit, "original selected copy/run access", |d, _| {
        if id == "workflow-permissions-copy" {
            d["confirmation"]["selected"] == value
        } else {
            d["confirmation"]["runDraft"]["mode"] == value
        }
    })
    .await?;
    native(desktop, false).await?;
    Ok(())
}
pub(super) async fn query(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    value: &str,
    required: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let generation = before["permissions"]["confirmation"]["managerDraft"]["generation"]
        .as_u64()
        .ok_or("Original manager query generation absent")?;
    let confirmation = before["permissions"]["confirmation"]["id"].clone();
    input(desktop, "workflow-permissions-manager-query", value).await?;
    until(state, visit, "actual manager query result", |d, _| {
        let m = &d["confirmation"]["managerDraft"];
        d["confirmation"]["id"] == confirmation
            && m["generation"]
                .as_u64()
                .is_some_and(|current| current > generation)
            && m["query"] == value
            && m["pending"] == false
            && m["results"]
                .as_array()
                .is_some_and(|r| r.iter().any(|id| id == required))
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    if !draft(&after) {
        return Err("Manager query replaced original draft".into());
    }
    Ok(json!({"before":before,"after":after}))
}
pub(super) async fn target(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    id: &str,
    add: bool,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
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
    let clicked=wait_dom(desktop,&format!(r#"(()=>{{const box=document.getElementById('workflow-permissions-manager-settings'),rows=box?[...box.querySelectorAll({})].filter(e=>e.dataset[{}]==={}):[];if(rows.length!==1||rows[0].disabled)return{{ok:false}};const e=rows[0];e.scrollIntoView({{block:'center',behavior:'instant'}});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};const proof={{ok:true,target:e.dataset[{}],label:e.textContent,connected:e.isConnected,insideOriginalBox:box.contains(e),hit:true}};e.click();return proof;}})()"#,json!(selector),json!(attr),json!(id),json!(attr))).await?;
    until(state, visit, "actual manager selection", |d, _| {
        d["confirmation"]["managerDraft"]["ids"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == id) == add)
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    if !draft(&after) || before["server"] != after["server"] {
        return Err("Manager selection dispatched or replaced original draft".into());
    }
    Ok(json!({"before":before,"clicked":clicked,"after":after}))
}
pub(super) async fn chosen(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let searched = query(state, desktop, visit, "manager", "UMANNEW01").await?;
    let added = target(state, desktop, visit, "UMANNEW01", true).await?;
    let removed = target(
        state,
        desktop,
        visit,
        if endpoint::self_remove() {
            "UWORKFLOW"
        } else {
            "UMANOLD01"
        },
        false,
    )
    .await?;
    if endpoint::copy_changed() {
        select(
            state,
            desktop,
            visit,
            "workflow-permissions-copy",
            "everyone",
        )
        .await?;
    }
    if endpoint::case() == "managers-self-rejected-held" {
        select(
            state,
            desktop,
            visit,
            "workflow-permissions-run",
            "everyone",
        )
        .await?;
    }
    Ok(json!({"search":searched,"add":added,"remove":removed}))
}
pub(super) async fn close_local(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    id: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    click(desktop, id).await?;
    until(state, visit, "closed local manager settings", |d, _| {
        d["confirmation"].is_null()
    })
    .await?;
    wait_dom(desktop,r#"({ok:document.getElementById('workflow-management-title')?.readOnly===false&&!document.getElementById('workflow-permissions')})"#).await?;
    let after = observe(state, desktop, visit).await?;
    if !draft(&after)
        || before["server"] != after["server"]
        || before["permissions"]["document"]["id"] != after["permissions"]["document"]["id"]
        || after["permissions"]["document"]["revision"].as_u64()
            <= before["permissions"]["document"]["revision"].as_u64()
    {
        return Err("No-op/Cancel changed source, native draft, or document custody".into());
    }
    Ok(json!({"action":id,"before":before,"after":after}))
}
pub(super) async fn back_reopen(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: &mut u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let before = observe(state, desktop, *visit).await?;
    click(desktop, "workflow-management-back").await?;
    wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
    let after = observe(state, desktop, *visit).await?;
    if before["server"] != after["server"] {
        return Err("Back dispatched a request".into());
    }
    stage(
        state,
        desktop,
        name,
        "back",
        *visit,
        json!({"before":before,"after":after}),
        captures,
        stages,
    )
    .await?;
    let (reopened, management_entry) = enter(state, desktop).await?;
    *visit = reopened["visit"].as_u64().ok_or("Reopened visit missing")?;
    let remount=wait_dom(desktop,r#"(()=>{const old=window.__workflowManagersOriginal,root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');return{ok:!!old&&!!root?.__workflowManagement&&!!title&&!!description&&!old.root.isConnected&&old.root!==root&&old.title!==title&&old.description!==description,originalDetached:!!old&&!old.root.isConnected,newRoot:!!old&&old.root!==root,newTitle:!!old&&old.title!==title,newDescription:!!old&&old.description!==description}})()"#).await?;
    native(desktop, true).await?;
    let after = observe(state, desktop, *visit).await?;
    if !draft(&after)
        || after["permissions"]["document"]["id"] != before["permissions"]["document"]["id"]
        || !after["permissions"]["document"]["pending"].is_null()
        || after["permissions"]["attempts"] != before["permissions"]["attempts"]
    {
        return Err(format!(
            "Reopen lost original managers or draft: before={before} after={after}"
        ));
    }
    stage(
        state,
        desktop,
        name,
        "reopened",
        *visit,
        if management_entry.is_null(){json!({"before":before,"after":after,"remount":remount})}else{json!({"before":before,"after":after,"remount":remount,"managementEntry":management_entry})},
        captures,
        stages,
    )
    .await
}

/// A workflow without a trigger is opened from its real Managed list row.
/// AppWorkflow resolution requires an actual trigger and cannot represent it.
pub(super) async fn enter(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
) -> Result<(Value, Value), String> {
    click(desktop, "workflow-configuration").await?;
    let entry = if endpoint::case() == "managers-no-trigger" {
        let listed = wait(state, "original triggerless Managed list", |v| {
            v["busy"] == false && v["route"] == "managed"
        })
        .await?;
        let visit = listed["visit"]
            .as_u64()
            .ok_or("Managed list visit absent")?;
        let before = observe(state, desktop, visit).await?;
        let clicked=wait_dom(desktop,r#"(()=>{const e=document.getElementById('workflow-management-open-WF_REVIEW'),root=document.getElementById('workflow-management');if(!e||e.disabled||!root?.contains(e))return{ok:false};e.scrollIntoView({block:'center',behavior:'instant'});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(h!==e&&!e.contains(h))return{ok:false};const proof={ok:true,id:e.id,text:e.textContent,connected:e.isConnected,hit:true};e.click();return proof;})()"#).await?;
        json!({"managed":before,"clicked":clicked})
    } else {
        Value::Null
    };
    let model = wait(state, "original manager workflow editor", |v| {
        v["busy"] == false && v["route"] == "editor"
    })
    .await?;
    Ok((model, entry))
}
